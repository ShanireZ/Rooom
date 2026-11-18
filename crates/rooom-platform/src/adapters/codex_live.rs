//! Discovery through the validated Desktop owner and delivery through its bundled queue.

use super::windows::session_uuid as thread_id;
use super::{Feed, MutationError, SessionLink, claude, windows};
use crate::pipe::Scope;
use rooom_clients::codex::{self, QueueReceipt};
use rooom_clients::codex_approval::{self as native, Approval, Choice, Kind};
use rooom_core::inbox::{
    Client, Decision, DeliveryState, Receipt, Request, RequestKey, Session, SessionKey,
    TimelineEntry,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    ops::ControlFlow,
    os::windows::{fs::MetadataExt, io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
};
use windows_sys::Win32::{
    Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT, System::Pipes::GetNamedPipeServerProcessId,
    UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
};

const MAX_RECENT: usize = 16;
const MAX_HEADER_CHECKS: usize = 128;
const OWNER_TIMEOUT: Duration = Duration::from_millis(700);
const APPROVAL_IO: Duration = Duration::from_millis(2500);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_EVENTS: usize = 4 * 1024 * 1024;
const DELIVERY_LIFETIME_MS: u64 = 3_600_000;
const UNAVAILABLE: &str = "The verified Codex Desktop owner is unavailable.";
const HISTORY_UNAVAILABLE: &str =
    "Codex history is unavailable or incomplete. The live channel remains available.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    desktop_executable: PathBuf,
    engine_executable: PathBuf,
    data_directory: PathBuf,
    thread: Option<String>,
    #[serde(default)]
    approvals: bool,
}

impl Config {
    fn read(root: &Path) -> Result<Self, &'static str> {
        let file = real_file(&root.join("live.json")).map_err(|_| UNAVAILABLE)?;
        let config: Self = serde_json::from_reader(file.take(4096))
            .map_err(|_| "Invalid Codex Desktop channel configuration.")?;
        let package = format!("OpenAI.Codex_{}_x64__2p2nqsd0c76g0", codex::DESKTOP_VERSION);
        let desktop = &config.desktop_executable;
        let valid_install = desktop
            .file_name()
            .is_some_and(|name| name == "ChatGPT.exe")
            && desktop.parent().is_some_and(|app| {
                app.file_name().is_some_and(|name| name == "app")
                    && app
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| name == package.as_str())
                    && claude::same_image(
                        &app.join("resources/codex.exe"),
                        &config.engine_executable,
                    )
            });
        if !desktop.is_absolute()
            || !config.engine_executable.is_absolute()
            || !valid_install
            || !config.data_directory.is_absolute()
            || !directory(&config.data_directory)
            || config.thread.as_deref().is_some_and(|id| !thread_id(id))
        {
            return Err("This Codex Desktop installation or channel is unsupported.");
        }
        Ok(config)
    }

    fn candidates(&self, selected: Option<&str>) -> Vec<Route> {
        let selected = selected.or(self.thread.as_deref());
        let suffix = selected.map(|id| format!("-{id}.jsonl"));
        let top = self.data_directory.join("sessions");
        let mut routes = Vec::new();
        let mut checked = 0;
        let _ = visit_rollouts(&top, 3, &mut |path, filename| {
            if suffix
                .as_ref()
                .is_some_and(|suffix| !filename.ends_with(suffix))
            {
                return ControlFlow::Continue(());
            }
            checked += 1;
            if let Some(route) = read_route(path, filename, selected) {
                routes.push(route);
            }
            if routes.len() == MAX_RECENT || checked == MAX_HEADER_CHECKS {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        routes
    }
}

struct Route {
    id: String,
    path: PathBuf,
    workspace: Option<String>,
}

struct OwnedRoute {
    route: Route,
    owner: String,
}

fn codex_link(thread: &str) -> Option<SessionLink> {
    thread_id(thread).then(|| SessionLink::Uri {
        uri: format!("codex://threads/{thread}"),
        error: "Windows could not open the Codex Desktop session.",
    })
}

pub(super) fn session_link(root: &Path, target: &SessionKey) -> Result<SessionLink, &'static str> {
    let link = codex_link(&target.id).ok_or(UNAVAILABLE)?;
    let config = Config::read(root)?;
    if !config.approvals || config.thread.as_ref().is_some_and(|id| id != &target.id) {
        return Err(UNAVAILABLE);
    }
    let routes = config.candidates(Some(&target.id));
    if routes.len() != 1 || owned(&config, routes)?.len() != 1 {
        return Err(UNAVAILABLE);
    }
    Ok(link)
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    struct Rollouts {
        root: PathBuf,
        day: PathBuf,
    }

    impl Rollouts {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let temp = std::env::temp_dir().canonicalize().unwrap();
            let root = temp.join(format!(
                "rooom-m3-codex-discovery-20261118-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&root).unwrap();
            let day = root.join("sessions/2026/11/18");
            fs::create_dir_all(&day).unwrap();
            Self { root, day }
        }

        fn config(&self) -> Config {
            Config {
                desktop_executable: PathBuf::new(),
                engine_executable: PathBuf::new(),
                data_directory: self.root.clone(),
                thread: None,
                approvals: false,
            }
        }

        fn add(&self, index: usize, id: &str, source: Value, engine: &str) {
            let header = json!({"type":"session_meta","payload":{
                "id":id,"originator":"Codex Desktop","source":source,"cli_version":engine
            }});
            let path = self.day.join(format!("rollout-{index:04}-{id}.jsonl"));
            fs::write(path, format!("{header}\n")).unwrap();
        }
    }

    impl Drop for Rollouts {
        fn drop(&mut self) {
            assert_eq!(self.root.canonicalize().unwrap(), self.root);
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    fn id(index: usize) -> String {
        format!("00000000-0000-4000-8000-{index:012x}")
    }

    #[test]
    fn session_link_opens_only_a_fixed_codex_thread_destination() {
        let thread = id(1);
        let link = codex_link(&thread).unwrap();
        assert!(
            matches!(link, SessionLink::Uri { uri, .. } if uri == format!("codex://threads/{thread}"))
        );
        for value in [
            "https://example.com",
            "wake/00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000001?tab=other",
            "00000000-0000-4000-8000-00000000000\0",
        ] {
            assert!(codex_link(value).is_none());
        }
    }

    #[test]
    fn automatic_discovery_finds_desktop_behind_newer_ineligible_rollouts() {
        let rollouts = Rollouts::new();
        let desktop = id(0);
        rollouts.add(0, &desktop, json!("vscode"), codex::ENGINE_VERSION);
        for index in 1..=MAX_RECENT + 4 {
            rollouts.add(
                index,
                &id(index),
                json!({"subagent":{"thread_spawn":{"parent_thread_id":desktop}}}),
                codex::ENGINE_VERSION,
            );
        }
        rollouts.add(MAX_RECENT + 5, &id(50), json!("cli"), codex::ENGINE_VERSION);
        rollouts.add(MAX_RECENT + 6, &id(51), json!("vscode"), "999.0.0");

        let config = rollouts.config();
        let routes = config.candidates(None);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].id, desktop);
        assert_eq!(config.candidates(Some(&desktop)).len(), 1);
    }

    #[test]
    fn automatic_discovery_keeps_the_newest_sixteen_eligible_routes() {
        let rollouts = Rollouts::new();
        for index in 0..=MAX_RECENT {
            rollouts.add(index, &id(index), json!("vscode"), codex::ENGINE_VERSION);
        }
        rollouts.add(MAX_RECENT + 1, &id(99), json!("cli"), codex::ENGINE_VERSION);

        let routes = rollouts.config().candidates(None);
        let ids: Vec<_> = routes.into_iter().map(|route| route.id).collect();
        assert_eq!(ids, (1..=MAX_RECENT).rev().map(id).collect::<Vec<_>>());
    }

    #[test]
    fn discovery_bounds_header_reads_and_preserves_exact_selection() {
        let rollouts = Rollouts::new();
        let desktop = id(0);
        rollouts.add(0, &desktop, json!("vscode"), codex::ENGINE_VERSION);
        for index in 1..=MAX_HEADER_CHECKS {
            rollouts.add(index, &id(index), json!("cli"), codex::ENGINE_VERSION);
        }

        let mut config = rollouts.config();
        assert!(config.candidates(None).is_empty());
        assert_eq!(config.candidates(Some(&desktop)).len(), 1);
        config.thread = Some(desktop.clone());
        let routes = config.candidates(None);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].id, desktop);

        rollouts.add(
            MAX_HEADER_CHECKS + 1,
            &desktop,
            json!("vscode"),
            codex::ENGINE_VERSION,
        );
        assert_eq!(config.candidates(Some(&desktop)).len(), 2);
    }
}

fn directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|info| {
        info.is_dir() && info.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    })
}

fn real_file(path: &Path) -> io::Result<File> {
    let info = fs::symlink_metadata(path)?;
    if !info.is_file() || info.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    File::open(path)
}

fn visit_rollouts(
    folder: &Path,
    depth: u8,
    visit: &mut impl FnMut(&Path, &str) -> ControlFlow<()>,
) -> ControlFlow<()> {
    if !directory(folder) {
        return ControlFlow::Continue(());
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return ControlFlow::Continue(());
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort_unstable_by(|a, b| b.cmp(a));
    for path in paths {
        if depth != 0 {
            visit_rollouts(&path, depth - 1, visit)?;
        } else if let Some(filename) = path.file_name().and_then(|name| name.to_str())
            && filename.starts_with("rollout-")
            && filename.ends_with(".jsonl")
        {
            visit(&path, filename)?;
        }
    }
    ControlFlow::Continue(())
}

fn read_route(path: &Path, filename: &str, selected: Option<&str>) -> Option<Route> {
    let mut line = String::new();
    let (file, _) = windows::open_plain_file(path).ok()?;
    BufReader::new(file)
        .take(codex::MAX_DOCUMENT_BYTES as u64 + 1)
        .read_line(&mut line)
        .ok()?;
    let header = codex::parse_thread_metadata(&line).ok()?;
    if !thread_id(&header.id)
        || !filename.ends_with(&format!("-{}.jsonl", header.id))
        || !header.desktop_origin()
        || header.cli_version != codex::ENGINE_VERSION
        || selected.is_some_and(|id| id != header.id)
    {
        return None;
    }
    Some(Route {
        id: header.id,
        path: path.to_owned(),
        workspace: windows::workspace_label(header.cwd.as_deref()),
    })
}

async fn ipc_write(pipe: &mut NamedPipeClient, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    pipe.write_u32_le(bytes.len() as u32).await?;
    pipe.write_all(&bytes).await
}

async fn query(
    pipe: &mut NamedPipeClient,
    method: &str,
    version: u8,
    client: &str,
    params: Value,
    target: Option<&str>,
) -> io::Result<Value> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = format!(
        "rooom-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut packet = json!({
        "type":"request", "requestId":id, "method":method, "version":version,
        "sourceClientId":client, "params":params, "timeoutMs":if target.is_some() { 1800 } else { 450 }
    });
    if let Some(target) = target {
        packet["targetClientId"] = json!(target);
    }
    ipc_write(pipe, &packet).await?;
    loop {
        let value = ipc_read(pipe).await?;
        if value["type"] == "response" && value["requestId"] == id {
            return Ok(value);
        }
        if value["type"] == "client-discovery-request"
            && let Some(id) = value["requestId"].as_str()
        {
            ipc_write(
                pipe,
                &json!({
                    "type":"client-discovery-response","requestId":id,"response":{"canHandle":false}
                }),
            )
            .await?;
        }
    }
}

async fn ipc_read(pipe: &mut NamedPipeClient) -> io::Result<Value> {
    let size = pipe.read_u32_le().await? as usize;
    if size == 0 || size > 8 * 1024 * 1024 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; size];
    pipe.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

async fn desktop(config: &Config) -> Result<(NamedPipeClient, String), &'static str> {
    let mut pipe = ClientOptions::new()
        .open(r"\\.\pipe\codex-ipc")
        .map_err(|_| UNAVAILABLE)?;
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle().cast(), &mut pid) } == 0
        || !Scope::current()
            .and_then(|scope| scope.owns_process(pid))
            .unwrap_or(false)
        || !claude::process_info(pid)
            .is_ok_and(|process| claude::same_image(&process.image, &config.desktop_executable))
    {
        return Err(UNAVAILABLE);
    }
    let initialized = query(
        &mut pipe,
        "initialize",
        0,
        "initializing-client",
        json!({"clientType":"rooom"}),
        None,
    )
    .await
    .map_err(|_| UNAVAILABLE)?;
    let client = initialized["result"]["clientId"]
        .as_str()
        .filter(|id| codex::identifier(id))
        .ok_or(UNAVAILABLE)?
        .to_owned();
    Ok((pipe, client))
}

async fn discover(
    pipe: &mut NamedPipeClient,
    client: &str,
    thread: &str,
) -> io::Result<Option<String>> {
    let reply = query(
        pipe,
        "thread-owner-discovery",
        1,
        client,
        json!({"hostId":"local","conversationId":thread}),
        None,
    )
    .await?;
    Ok((reply["resultType"] == "success")
        .then(|| reply["handledByClientId"].as_str())
        .flatten()
        .filter(|id| codex::identifier(id))
        .map(str::to_owned))
}

async fn snapshot(
    pipe: &mut NamedPipeClient,
    client: &str,
    thread: &str,
    owner: &str,
) -> io::Result<Value> {
    let follow = |following| {
        json!({"type":"broadcast","method":"thread-stream-following-changed",
        "sourceClientId":client,"targetClientIds":[owner],"version":1,
        "params":{"hostId":"local","conversationId":thread,"following":following}})
    };
    ipc_write(pipe, &follow(true)).await?;
    let state = loop {
        let mut msg = ipc_read(pipe).await?;
        if msg["type"] == "client-discovery-request"
            && let Some(id) = msg["requestId"].as_str()
        {
            ipc_write(pipe, &json!({"type":"client-discovery-response","requestId":id,"response":{"canHandle":false}})).await?;
            continue;
        }
        if msg["type"] != "broadcast"
            || msg["method"] != "thread-stream-state-changed"
            || msg["sourceClientId"] != owner
            || msg["params"]["conversationId"] != thread
            || msg["params"]["hostId"] != "local"
        {
            continue;
        }
        if msg["version"] != native::STREAM_VERSION {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let change = &mut msg["params"]["change"];
        if change["type"] == "snapshot" {
            break change["conversationState"].take();
        }
    };
    ipc_write(pipe, &follow(false)).await?;
    if !state.is_object() || state["id"] != thread {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(state)
}

fn owned(config: &Config, routes: Vec<Route>) -> Result<Vec<OwnedRoute>, &'static str> {
    if routes.is_empty() {
        return Ok(Vec::new());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| UNAVAILABLE)?;
    runtime.block_on(async {
        tokio::time::timeout(OWNER_TIMEOUT, async {
            let (mut pipe, client) = desktop(config).await?;
            let mut owned = Vec::new();
            for route in routes {
                if let Some(owner) = discover(&mut pipe, &client, &route.id)
                    .await
                    .map_err(|_| UNAVAILABLE)?
                {
                    owned.push(OwnedRoute { route, owner });
                }
            }
            Ok(owned)
        })
        .await
        .map_err(|_| UNAVAILABLE)?
    })
}

#[derive(Default)]
pub struct DeliveryTracker {
    pending: Vec<Pending>,
    approvals: Approvals,
    history: History,
}

struct Lease {
    id: String,
    owner: String,
    native: Approval,
    workspace: Option<String>,
    first_seen: Instant,
    submitted: bool,
}

impl Lease {
    fn card(&self) -> Request {
        Request {
            key: RequestKey {
                session: SessionKey {
                    client: Client::Codex,
                    id: self.native.thread.clone(),
                },
                id: self.id.clone(),
            },
            summary: match self.native.kind {
                Kind::Command => "Command approval",
                Kind::File => "File change approval",
            }
            .into(),
            workspace: self.workspace.clone(),
            detail: self.native.detail.clone(),
            decision_blocked: self
                .submitted
                .then(|| "The client did not confirm the decision.".into()),
            allow_for_session: self.native.session_grant,
            can_open_session: true,
        }
    }

    fn active(&self, now: Instant, wait: Duration) -> bool {
        now.saturating_duration_since(self.first_seen) < wait
    }
}

struct Approvals {
    entries: BTreeMap<String, Lease>,
    sequence: u64,
    wait: Duration,
}

impl Default for Approvals {
    fn default() -> Self {
        let wait = if cfg!(feature = "adapter-fixtures")
            && std::env::var_os("ROOOM_SMOKE_LIVE").is_some()
        {
            std::env::var("ROOOM_TEST_CODEX_WAIT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| (1000..=native::APPROVAL_WAIT_MS).contains(value))
        } else {
            None
        };
        Self {
            entries: BTreeMap::new(),
            sequence: 0,
            wait: Duration::from_millis(wait.unwrap_or(native::APPROVAL_WAIT_MS)),
        }
    }
}

impl Approvals {
    fn project(
        &mut self,
        route: &Route,
        owner: &str,
        snapshot: &Value,
        now: Instant,
    ) -> Vec<Request> {
        let thread = route.id.as_str();
        let Some(pending) = native::requests(snapshot, thread) else {
            return Vec::new();
        };
        self.entries.retain(|_, entry| {
            entry.native.thread != thread
                || native::still_waiting(snapshot, thread, &entry.native) != Some(false)
        });
        let mut result = Vec::new();
        for approval in pending {
            let key = approval.identity();
            let changed = self
                .entries
                .get(&key)
                .is_none_or(|entry| entry.owner != owner || entry.native != approval);
            if changed {
                let (first_seen, submitted) = self
                    .entries
                    .get(&key)
                    .map(|entry| (entry.first_seen, entry.submitted))
                    .unwrap_or((now, false));
                self.sequence += 1;
                self.entries.insert(
                    key.clone(),
                    Lease {
                        id: format!("codex-approval-{}", self.sequence),
                        owner: owner.into(),
                        native: approval,
                        workspace: route.workspace.clone(),
                        first_seen,
                        submitted,
                    },
                );
            }
            let entry = &self.entries[&key];
            if entry.active(now, self.wait) {
                result.push(entry.card());
            }
        }
        result
    }

    fn read(&mut self, config: &Config, routes: &[OwnedRoute]) -> Vec<Request> {
        if routes.is_empty() || !config.approvals {
            return Vec::new();
        }
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return Vec::new();
        };
        let mut cards = Vec::new();
        let _ = runtime.block_on(async {
            tokio::time::timeout(APPROVAL_IO, async {
                let (mut pipe, client) = desktop(config).await.map_err(io::Error::other)?;
                for OwnedRoute { route, owner } in routes {
                    let state = snapshot(&mut pipe, &client, &route.id, owner).await?;
                    cards.extend(self.project(route, owner, &state, Instant::now()));
                }
                Ok::<(), io::Error>(())
            })
            .await
        });
        cards
    }
}

#[cfg(test)]
#[test]
fn reobserving_the_same_native_request_never_renews_the_ten_minute_deadline() {
    let state = json!({
        "requests":[{"method":"item/commandExecution/requestApproval","id":42,
            "params":{"threadId":"thread","turnId":"turn","itemId":"item","command":"Write-Output synthetic"}}],
        "turnHistory":{"history":{"entitiesByKey":{"local-turn":{
            "turnId":"turn","status":"inProgress","params":{"approvalsReviewer":"user"}
        }}}}
    });
    let mut tracker = Approvals {
        entries: BTreeMap::new(),
        sequence: 0,
        wait: Duration::from_millis(native::APPROVAL_WAIT_MS),
    };
    let first = Instant::now();
    let route = Route {
        id: "thread".into(),
        path: PathBuf::new(),
        workspace: Some("Synthetic project".into()),
    };
    let original = tracker.project(&route, "owner-1", &state, first);
    assert_eq!(original.len(), 1);
    assert_eq!(original[0].workspace.as_deref(), Some("Synthetic project"));
    let before = first + Duration::from_millis(599_999);
    assert_eq!(tracker.project(&route, "owner-1", &state, before), original);
    let expiry = first + Duration::from_secs(600);
    assert!(
        tracker
            .project(&route, "owner-2", &state, expiry)
            .is_empty()
    );
    assert!(
        tracker
            .project(&route, "owner-1", &state, expiry)
            .is_empty()
    );
    assert_eq!(tracker.entries.len(), 1);
}

struct Pending {
    receipt: Receipt,
    text: String,
    rollout: PathBuf,
    created: u64,
    offset: u64,
    expires: u64,
    ambiguous: bool,
}

impl Pending {
    fn observe(&self) -> Option<Receipt> {
        let mut file = real_file(&self.rollout).ok()?;
        let metadata = file.metadata().ok()?;
        if metadata.creation_time() != self.created
            || metadata.len().checked_sub(self.offset)? > MAX_EVENTS as u64
        {
            return None;
        }
        file.seek(SeekFrom::Start(self.offset)).ok()?;
        let mut bytes = Vec::new();
        file.take(MAX_EVENTS as u64).read_to_end(&mut bytes).ok()?;
        let end = bytes.iter().rposition(|value| *value == b'\n')?;
        let events = std::str::from_utf8(&bytes[..end]).ok()?;
        let turn = codex::native_delivery_evidence(events, &self.receipt.session.id, &self.text)
            .ok()??;
        let mut proof = self.receipt.clone();
        proof.state = DeliveryState::Started;
        if proof.external_id.is_none() {
            proof.external_id = Some(turn);
        }
        Some(proof)
    }
}

pub fn read(
    root: &Path,
    now: u64,
    tracker: Option<&mut DeliveryTracker>,
) -> Result<Feed, &'static str> {
    let config = Config::read(root)?;
    let routes = owned(&config, config.candidates(None))?;
    let mut feed = Feed::default();
    let mut temporary_history = History::default();
    let history = if let Some(tracker) = tracker {
        feed.requests = tracker.approvals.read(&config, &routes);
        tracker.pending.retain(|pending| {
            if now >= pending.expires {
                return false;
            }
            if !pending.ambiguous
                && let Some(proof) = pending.observe()
            {
                feed.receipts.push(proof);
                return false;
            }
            true
        });
        &mut tracker.history
    } else {
        &mut temporary_history
    };
    history
        .rollouts
        .retain(|id, _| routes.iter().any(|owned| owned.route.id == *id));
    for OwnedRoute { route, .. } in routes {
        let session = SessionKey {
            client: Client::Codex,
            id: route.id,
        };
        history.append(&route.path, &session, &mut feed);
        feed.sessions.push(Session {
            key: session,
            title: "Codex Desktop".into(),
            send_blocked: None,
        });
    }
    Ok(feed)
}

#[derive(Default)]
struct History {
    rollouts: BTreeMap<String, HistoryProjection>,
}

struct HistoryProjection {
    path: PathBuf,
    stamp: (u64, u64, u64),
    messages: Vec<TimelineEntry>,
}

impl History {
    fn append(&mut self, path: &Path, session: &SessionKey, feed: &mut Feed) {
        match self.read(path, session) {
            Ok(messages) => feed.timeline.extend_from_slice(messages),
            Err(_) => {
                self.rollouts.remove(&session.id);
                feed.warning = Some(HISTORY_UNAVAILABLE);
            }
        }
    }

    fn read(&mut self, path: &Path, session: &SessionKey) -> io::Result<&[TimelineEntry]> {
        let (mut file, metadata) = windows::open_plain_file(path)?;
        let stamp = windows::file_stamp(&metadata);
        if !self
            .rollouts
            .get(&session.id)
            .is_some_and(|cached| cached.path == path && cached.stamp == stamp)
        {
            let messages = read_history(&mut file, session, stamp)?;
            self.rollouts.insert(
                session.id.clone(),
                HistoryProjection {
                    path: path.to_owned(),
                    stamp,
                    messages,
                },
            );
        }
        Ok(&self.rollouts[&session.id].messages)
    }
}

fn read_history(
    file: &mut File,
    session: &SessionKey,
    stamp: (u64, u64, u64),
) -> io::Result<Vec<TimelineEntry>> {
    let mut header = String::new();
    let header_len = BufReader::new(&mut *file)
        .take(codex::MAX_DOCUMENT_BYTES as u64 + 1)
        .read_line(&mut header)? as u64;
    let thread = codex::parse_thread_metadata(&header).map_err(|_| io::ErrorKind::InvalidData)?;
    if !header.ends_with('\n')
        || thread.id != session.id
        || !thread.desktop_origin()
        || thread.cli_version != codex::ENGINE_VERSION
    {
        return Err(io::ErrorKind::InvalidData.into());
    }

    let (start, bytes) = windows::jsonl_tail(file, stamp, header_len, codex::HISTORY_WINDOW)?;
    Ok(codex::parse_history(&bytes, start, &thread)
        .map_err(|_| io::ErrorKind::InvalidData)?
        .into_iter()
        .map(|message| TimelineEntry {
            session: session.clone(),
            id: message.id,
            text: message.text,
        })
        .collect())
}

#[cfg(test)]
#[test]
fn recent_replies_survive_a_large_and_still_growing_rollout() {
    struct RolloutFile(PathBuf);

    impl Drop for RolloutFile {
        fn drop(&mut self) {
            fs::remove_file(&self.0).unwrap();
        }
    }

    let path = std::env::temp_dir().join(format!(
        "rooom-m3-codex-history-20261118-{}.jsonl",
        std::process::id()
    ));
    let fixture = include_str!("../../../../tests/fixtures/codex/thread.jsonl");
    let (header, reply) = fixture.split_once('\n').unwrap();
    let filler = "x".repeat(5 * 1024 * 1024);
    let input = format!(
        "{header}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"reasoning\",\"summary\":\"{filler}\"}}}}\n{reply}{{"
    );
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let source = RolloutFile(path);
    std::io::Write::write_all(&mut file, input.as_bytes()).unwrap();
    drop(file);
    let session = SessionKey {
        client: Client::Codex,
        id: "session-1".into(),
    };
    let mut history = History::default();
    let mut feed = Feed::default();
    history.append(&source.0, &session, &mut feed);
    assert_eq!(feed.timeline.len(), 2);
    assert_eq!(feed.timeline[0].id, "input:item-prompt");
    assert_eq!(feed.timeline[0].text, "Review the synthetic Codex results.");
    assert_eq!(feed.timeline[1].id, "item-intro");
    assert_eq!(feed.timeline[1].text, "Synthetic Codex timeline entry.");
    assert_eq!(feed.warning, None);
    assert_eq!(fs::read(&source.0).unwrap(), input.as_bytes());

    // A foreign header under the same path must clear the cached replies.
    let foreign = input.replacen("session-1", "foreign-session", 1);
    fs::write(&source.0, &foreign).unwrap();
    let mut rejected = Feed::default();
    history.append(&source.0, &session, &mut rejected);
    assert!(rejected.timeline.is_empty());
    assert_eq!(rejected.warning, Some(HISTORY_UNAVAILABLE));
    assert!(history.rollouts.is_empty());

    let appended = format!(
        "{}{}",
        input.strip_suffix('{').unwrap(),
        reply.replace("item-intro", "item-next")
    );
    fs::write(&source.0, &appended).unwrap();
    let mut recovered = Feed::default();
    history.append(&source.0, &session, &mut recovered);
    let ids: Vec<_> = recovered
        .timeline
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(ids, ["input:item-prompt", "item-intro", "item-next"]);
    assert_eq!(recovered.warning, None);
    assert_eq!(fs::read(&source.0).unwrap(), appended.as_bytes());
}

fn output(exe: &Path, home: &Path, args: &[&str]) -> Result<String, MutationError> {
    let mut child = Command::new(exe)
        .args(args)
        .env("CODEX_HOME", home)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|_| MutationError::Blocked)?;
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut value = String::new();
        let result = stdout.take(8193).read_to_string(&mut value).map(|_| value);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .ok()
                    .and_then(Result::ok)
                    .filter(|value| value.len() <= 8192)
                    .ok_or(MutationError::Uncertain);
            }
            Ok(Some(_)) => return Err(MutationError::Uncertain),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(MutationError::Uncertain);
            }
        }
    }
}

pub fn send(
    root: &Path,
    now: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
    tracker: &mut DeliveryTracker,
) -> Result<Receipt, MutationError> {
    if target.client != Client::Codex
        || !thread_id(&target.id)
        || !codex::identifier(id)
        || text.len() > 8192
        || text.trim().is_empty()
        || text.contains('\0')
    {
        return Err(MutationError::Blocked);
    }
    let config = Config::read(root).map_err(|_| MutationError::Blocked)?;
    if config
        .thread
        .as_ref()
        .is_some_and(|selected| selected != &target.id)
    {
        return Err(MutationError::Blocked);
    }
    let version = output(
        &config.engine_executable,
        &config.data_directory,
        &["--version"],
    )
    .map_err(|_| MutationError::Blocked)?;
    if version.trim().strip_prefix("codex-cli ") != Some(codex::ENGINE_VERSION) {
        return Err(MutationError::Blocked);
    }
    let routes =
        owned(&config, config.candidates(Some(&target.id))).map_err(|_| MutationError::Blocked)?;
    let [OwnedRoute { route, .. }] = routes.as_slice() else {
        return Err(MutationError::Blocked);
    };
    let mut file = real_file(&route.path).map_err(|_| MutationError::Blocked)?;
    let metadata = file.metadata().map_err(|_| MutationError::Blocked)?;
    let offset = metadata.len();
    if offset == 0 || file.seek(SeekFrom::Start(offset - 1)).is_err() {
        return Err(MutationError::Blocked);
    }
    let mut last = [0];
    file.read_exact(&mut last)
        .map_err(|_| MutationError::Blocked)?;
    if last != [b'\n'] {
        return Err(MutationError::Blocked);
    }
    let result = output(
        &config.engine_executable,
        &config.data_directory,
        &["queue", "--thread", &target.id, "--message", text],
    );
    if result == Err(MutationError::Blocked) {
        return Err(MutationError::Blocked);
    }
    let ack = result
        .ok()
        .and_then(|value| QueueReceipt::parse_stdout(&value).ok())
        .filter(|receipt| receipt.thread_id == target.id);
    let receipt = Receipt {
        id: id.into(),
        session: target.clone(),
        state: DeliveryState::Queued,
        external_id: ack.as_ref().map(|ack| ack.message_id.clone()),
    };
    let mut ambiguous = false;
    for pending in &mut tracker.pending {
        if pending.receipt.session == *target && pending.text == text {
            pending.ambiguous = true;
            ambiguous = true;
        }
    }
    tracker.pending.push(Pending {
        receipt: receipt.clone(),
        text: text.into(),
        rollout: route.path.clone(),
        created: metadata.creation_time(),
        offset,
        expires: now.saturating_add(DELIVERY_LIFETIME_MS),
        ambiguous,
    });
    if ack.is_none() {
        return Err(MutationError::Uncertain);
    }
    windows::execute_uri(
        &format!("codex://threads/wake/{}", target.id),
        SW_SHOWNOACTIVATE,
    );
    Ok(receipt)
}

pub fn answer(
    root: &Path,
    expected: &Request,
    decision: Decision,
    tracker: &mut DeliveryTracker,
) -> Result<(), MutationError> {
    let config = Config::read(root).map_err(|_| MutationError::Blocked)?;
    let thread = expected.key.session.id.as_str();
    if expected.key.session.client != Client::Codex
        || !config.approvals
        || config
            .thread
            .as_deref()
            .is_some_and(|selected| selected != thread)
        || config.candidates(Some(thread)).len() != 1
    {
        return Err(MutationError::Blocked);
    }
    let wait = tracker.approvals.wait;
    let lease = tracker
        .approvals
        .entries
        .values_mut()
        .find(|entry| entry.id == expected.key.id && entry.native.thread == thread)
        .filter(|entry| {
            !entry.submitted && entry.active(Instant::now(), wait) && entry.card() == *expected
        })
        .ok_or(MutationError::Blocked)?;
    let choice = match decision {
        Decision::Allow => Choice::Allow,
        Decision::Deny => Choice::Deny,
        Decision::AllowForSession => Choice::AllowForSession,
    };
    let (method, params) = lease.native.reply(choice).ok_or(MutationError::Blocked)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| MutationError::Blocked)?;
    let result = runtime.block_on(async {
        tokio::time::timeout(APPROVAL_IO, async {
            let (mut pipe, client) = desktop(&config).await.map_err(io::Error::other)?;
            if discover(&mut pipe, &client, thread).await?.as_deref() != Some(&lease.owner) {
                return Ok(false);
            }
            let before = snapshot(&mut pipe, &client, thread, &lease.owner).await?;
            if !native::requests(&before, thread)
                .is_some_and(|pending| pending.contains(&lease.native))
                || !lease.active(Instant::now(), wait)
            {
                return Ok(false);
            }
            lease.submitted = true;
            let reply = query(&mut pipe, method, 1, &client, params, Some(&lease.owner)).await?;
            if !native::acknowledged(&reply, &lease.owner) {
                return Ok(false);
            }
            loop {
                let next = snapshot(&mut pipe, &client, thread, &lease.owner).await?;
                match native::still_waiting(&next, thread, &lease.native) {
                    Some(false) => return Ok::<_, io::Error>(true),
                    Some(true) => tokio::time::sleep(Duration::from_millis(40)).await,
                    None => return Ok(false),
                }
            }
        })
        .await
    });
    if matches!(result, Ok(Ok(true))) {
        Ok(())
    } else if lease.submitted {
        Err(MutationError::Uncertain)
    } else {
        Err(MutationError::Blocked)
    }
}
