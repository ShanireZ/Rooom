use crate::{Resident, shell};
use rooom_core::inbox::{Client, Decision, Inbox, Receipt, Request, RequestKey, SessionKey};
use rooom_platform::adapters::{self, DeliveryTrackers, MutationError, storage};
use rooom_platform::pipe::{Listener, Scope, hook};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

pub struct Batch {
    root: PathBuf,
    receipts: PathBuf,
    fixtures: bool,
    inbox: Mutex<Inbox>,
    deliveries: Mutex<DeliveryTrackers>,
    listener: Mutex<Option<Listener>>,
    stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

impl Batch {
    pub fn new(profile: &Path, scope: &Scope, fixtures: bool) -> io::Result<Self> {
        let root = profile.join("adapters");
        if fixtures {
            storage::verify_private(&root)?;
        } else {
            storage::create_private(&root)?;
            for client in Client::ALL {
                storage::create_private(&root.join(client.slug()))?;
            }
            storage::create_private(&root.join("harness/sessions"))?;
        }
        let receipts = if cfg!(feature = "first-batch-smoke") {
            root.join("delivery-receipts.json")
        } else {
            root.join(format!("delivery-receipts-{}.json", scope.session()))
        };
        let inbox = match fs::read(&receipts) {
            Ok(bytes) => Inbox::restore(
                serde_json::from_slice::<Vec<Receipt>>(&bytes)
                    .map_err(|_| io::Error::other("Invalid delivery receipt record."))?,
            )
            .ok_or_else(|| io::Error::other("Invalid delivery receipt identity."))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Inbox::default(),
            Err(error) => return Err(error),
        };
        let claude = root.join(Client::Claude.slug());
        if !fixtures {
            for folder in ["plugin", "plugin/check-ins", "plugin/receipts"] {
                storage::create_private(&claude.join(folder))?;
            }
        }
        let listener = if fixtures || claude.join("live.json").is_file() {
            Some(hook::listen(scope, claude)?)
        } else {
            None
        };
        Ok(Self {
            root,
            receipts,
            fixtures,
            inbox: Mutex::new(inbox),
            deliveries: Mutex::default(),
            listener: Mutex::new(listener),
            stop: Mutex::new(None),
        })
    }

    fn client(&self, client: Client) -> PathBuf {
        self.root.join(client.slug())
    }

    fn enabled(&self, client: Client) -> bool {
        self.fixtures || self.client(client).join("live.json").is_file()
    }

    fn persist(&self, inbox: &Inbox) -> Result<(), String> {
        storage::verify_private(&self.root)
            .and_then(|_| {
                let bytes =
                    serde_json::to_vec(&inbox.snapshot().deliveries).map_err(io::Error::other)?;
                storage::publish(&self.receipts, &bytes, true)
            })
            .map_err(|_| "The delivery receipt could not be saved.".into())
    }

    pub fn shutdown(&self) {
        self.stop.lock().unwrap().take();
        self.listener.lock().unwrap().take();
    }
}

pub fn start(app: AppHandle) {
    let (stop, stopped) = std::sync::mpsc::channel();
    *app.state::<Batch>().stop.lock().unwrap() = Some(stop);
    thread::spawn(move || {
        loop {
            if refresh(&app).is_err() {
                eprintln!("Rooom could not save adapter receipt state.");
                app.exit(1);
                return;
            }
            let interval = if app.state::<Batch>().fixtures {
                200
            } else {
                750
            };
            if stopped.recv_timeout(Duration::from_millis(interval))
                != Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            {
                return;
            }
        }
    });
}

fn refresh(app: &AppHandle) -> Result<(), String> {
    let batch = app.state::<Batch>();
    let mut inbox = batch.inbox.lock().unwrap();
    let mut arrival = false;
    let mut changed_receipts = false;
    for client in Client::ALL {
        let result = if !batch.enabled(client) {
            Err("This client has not been enabled.")
        } else {
            let mut deliveries = batch.deliveries.lock().unwrap();
            adapters::read(
                client,
                &batch.client(client),
                now_ms(),
                Some(&mut deliveries),
                batch.fixtures,
            )
        };
        match result {
            Ok(feed) => {
                for proof in feed.receipts {
                    changed_receipts |= inbox.record_evidence(proof);
                }
                arrival |= inbox.replace_source(
                    client,
                    feed.warning.map(str::to_owned),
                    feed.sessions,
                    feed.requests,
                    feed.timeline,
                );
            }
            Err(reason) => {
                inbox.replace_source(
                    client,
                    Some(reason.into()),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
            }
        }
    }
    if changed_receipts {
        batch.persist(&inbox)?;
    }
    let view = inbox.snapshot().clone();
    drop(inbox);
    if !arrival && app.state::<Resident>().inner().0.lock().unwrap().inbox == view {
        return Ok(());
    }
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if arrival {
            // Passive reveal before publishing the new cards.
            let _ = shell::set_surface(&handle, rooom_core::Surface::Panel);
        }
        let mut snapshot = handle.state::<Resident>().inner().0.lock().unwrap();
        if snapshot.set_inbox(view) {
            let _ = handle.emit_to(shell::SHELL, "rooom:snapshot", snapshot.clone());
        }
    })
    .map_err(|_| "The Rooom window is unavailable.".into())
}

pub async fn send(app: AppHandle, target: SessionKey, text: String) -> Result<String, String> {
    dispatch(move || {
        let batch = app
            .try_state::<Batch>()
            .ok_or("This client has not been verified.")?;
        if !batch.enabled(target.client) {
            return Err("The selected session is no longer available.".into());
        }
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let id = format!("send-{}-{}-{seq}", now_ms(), std::process::id());
        let mut inbox = batch.inbox.lock().unwrap();
        if !inbox.reserve_send(&id, &target) {
            return Err("The selected session cannot receive messages.".into());
        }
        if let Err(error) = batch.persist(&inbox) {
            inbox.finish_send(&id, None, true);
            return Err(error);
        }
        let result = adapters::send(
            &batch.client(target.client),
            now_ms(),
            &target,
            &id,
            &text,
            &mut batch.deliveries.lock().unwrap(),
            batch.fixtures,
        );
        let blocked = result == Err(MutationError::Blocked);
        inbox.finish_send(&id, result.ok(), blocked);
        batch.persist(&inbox)?;
        if blocked {
            return Err("The selected session is no longer available.".into());
        }
        Ok(id)
    })
    .await
}

pub async fn answer(app: AppHandle, expected: Request, decision: Decision) -> Result<(), String> {
    dispatch(move || {
        let batch = app
            .try_state::<Batch>()
            .ok_or("This client has not been verified.")?;
        let mut inbox = batch.inbox.lock().unwrap();
        if expected.decision_blocked.is_some() || !inbox.snapshot().requests.contains(&expected) {
            return Err("The request is no longer pending.".into());
        }
        let client = expected.key.session.client;
        if !batch.enabled(client) {
            return Err("The request is no longer pending.".into());
        }
        match adapters::answer(
            &batch.client(client),
            now_ms(),
            &expected,
            decision,
            &mut batch.deliveries.lock().unwrap(),
            batch.fixtures,
        ) {
            Ok(()) => {
                if matches!(client, Client::Antigravity | Client::Codex) {
                    inbox.confirmed_answer(&expected.key);
                }
                Ok(())
            }
            Err(MutationError::Blocked) => {
                Err("The native request changed or is unavailable.".into())
            }
            Err(MutationError::Uncertain) => {
                inbox.mark_answer_uncertain(&expected.key);
                Err("The client did not confirm the decision. Check the client window.".into())
            }
        }
    })
    .await
}

pub async fn open_request_session(app: AppHandle, request: RequestKey) -> Result<(), String> {
    dispatch(move || {
        let batch = app
            .try_state::<Batch>()
            .ok_or("The request is no longer available.")?;
        if !batch
            .inbox
            .lock()
            .unwrap()
            .can_open_request_session(&request)
        {
            return Err("The request is no longer available.".into());
        }
        let link = adapters::request_session_link(
            &batch.client(request.session.client),
            &request,
            now_ms(),
            batch.fixtures,
        )?;
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = send.send(link.open());
        })
        .map_err(|_| "The Rooom window is unavailable.")?;
        receive
            .recv()
            .map_err(|_| "The Rooom window is unavailable.")?
            .map_err(str::to_owned)
    })
    .await
}

async fn dispatch<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "The adapter worker is unavailable.".to_string())?
}
