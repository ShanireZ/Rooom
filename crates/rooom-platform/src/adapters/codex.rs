//! The first-batch Codex Desktop fixture transport. The resident supplies an
//! isolated owner directory. No installed Codex executable or owner pipe is
//! selected by this module.
use super::{Feed, MutationError, storage};
use rooom_clients::codex::{
    self, Approval, CommandRequest, DeliveryEventKind, Owner, QueueReceipt, Rollout,
};
use rooom_core::inbox::{
    Client, Decision, DeliveryState, Receipt, Request, RequestKey, Session, SessionKey,
    TimelineEntry,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DELIVERY_ID_ENV: &str = "ROOOM_CODEX_FIXTURE_DELIVERY";
const QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

struct Route {
    owner: Owner,
    rollout: Rollout,
}

impl Route {
    fn session(&self) -> SessionKey {
        SessionKey {
            client: Client::Codex,
            id: self.rollout.thread.id.clone(),
        }
    }

    fn matches(&self, approval: &Approval, command: &CommandRequest) -> bool {
        self.owner.handled_by_client_id.as_deref() == Some(&approval.handled_by_client_id)
            && self.owner.conversation_id == command.params.thread_id
            && self.owner.live_request.as_ref() == Some(&approval.request)
    }
}

fn get_route(root: &Path) -> Result<Route, &'static str> {
    let owner = codex::parse_owner(
        &read_text(&root.join("owner.json")).map_err(|_| "Codex Desktop owner is unavailable")?,
    )?;
    if !owner.enabled {
        return Err("Codex Desktop fixture is disabled");
    }
    if owner.desktop_version != codex::DESKTOP_VERSION
        || owner.engine_version != codex::ENGINE_VERSION
    {
        return Err("Unsupported Codex Desktop or engine version");
    }
    if owner.handled_by_client_id.is_none() {
        return Err("Codex Desktop has no active thread owner");
    }
    let rollout = codex::parse_rollout(
        &read_text(&root.join("thread.jsonl")).map_err(|_| "Codex thread is unavailable")?,
    )?;
    if !rollout.thread.desktop_origin() || owner.conversation_id != rollout.thread.id {
        return Err("This thread does not belong to the active Desktop owner");
    }
    if rollout.thread.cli_version != codex::ENGINE_VERSION {
        return Err("Unsupported Codex thread engine version");
    }
    Ok(Route { owner, rollout })
}

fn pending(root: &Path) -> Result<Option<(Approval, CommandRequest)>, &'static str> {
    let Some(input) = optional_text(&root.join("approval.json"))? else {
        return Ok(None);
    };
    let approval = codex::parse_approval(&input)?;
    let command = approval.command()?;
    Ok(Some((approval, command)))
}

pub fn read(root: &Path, _now_ms: u64) -> Result<Feed, &'static str> {
    let route = get_route(root)?;
    let session = route.session();
    let mut feed = Feed::default();
    feed.sessions.push(Session {
        key: session.clone(),
        title: "Codex Desktop".into(),
        send_blocked: None,
    });
    if let Some((approval, command)) = pending(root)?
        && route.matches(&approval, &command)
        && !has_confirmation(root, &approval, &command)?
    {
        feed.requests.push(Request {
            key: RequestKey {
                session: session.clone(),
                id: command.id,
            },
            summary: "Command approval".into(),
            workspace: super::windows::workspace_label(route.rollout.thread.cwd.as_deref()),
            detail: command.params.command,
            decision_blocked: None,
            allow_for_session: false,
            can_open_session: false,
        });
    }

    feed.receipts = read_receipts(root, &session)?;
    feed.timeline = route
        .rollout
        .messages
        .into_iter()
        .map(|message| TimelineEntry {
            session: session.clone(),
            id: message.id,
            text: message.text,
        })
        .collect();
    Ok(feed)
}

fn read_receipts(root: &Path, session: &SessionKey) -> Result<Vec<Receipt>, &'static str> {
    let events = optional_text(&root.join("delivery-events.jsonl"))?
        .map(|input| codex::parse_delivery_events(&input))
        .transpose()?
        .unwrap_or_default();
    let mut receipts = Vec::new();
    for item in fs::read_dir(root).map_err(|_| "Cannot read Codex fixture folder")? {
        let item = item.map_err(|_| "Cannot read Codex fixture entry")?;
        let name = item.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_prefix("queue-receipt-"))
            .and_then(|name| name.strip_suffix(".json"))
            .filter(|id| queue_message_id(id).is_some())
        else {
            continue;
        };
        let input = read_text(&item.path()).map_err(|_| "Cannot read Codex queue receipt")?;
        let receipt = QueueReceipt::parse_json(&input)?;
        if receipt.thread_id != session.id
            || Some(&receipt.message_id) != queue_message_id(id).as_ref()
        {
            return Err("Codex receipt does not match its queued thread and message");
        }
        let mut state = DeliveryState::Queued;
        for event in &events {
            if event.thread_id == session.id && event.queued_submission_id == receipt.message_id {
                match event.kind {
                    DeliveryEventKind::TaskStarted => state = DeliveryState::Started,
                    DeliveryEventKind::QueueTaken if state == DeliveryState::Queued => {
                        state = DeliveryState::Received;
                    }
                    DeliveryEventKind::QueueTaken => (),
                }
            }
        }
        receipts.push(Receipt {
            id: id.into(),
            session: session.clone(),
            state,
            external_id: Some(receipt.message_id),
        });
    }
    Ok(receipts)
}

fn queue_message_id(id: &str) -> Option<String> {
    let message_id = format!("fixture-queue-{id}");
    (codex::identifier(id) && codex::identifier(&message_id)).then_some(message_id)
}

pub fn send(
    root: &Path,
    _now_ms: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
) -> Result<Receipt, MutationError> {
    let route = get_route(root).map_err(|_| MutationError::Blocked)?;
    let message_id = queue_message_id(id).ok_or(MutationError::Blocked)?;
    if route.session() != *target || !valid_prompt(text) || queue_path(root, id).exists() {
        return Err(MutationError::Blocked);
    }
    let exe = std::env::current_exe().map_err(|_| MutationError::Blocked)?;
    let mut child = Command::new(exe)
        .args([OsStr::new("--fixture-codex"), root.as_os_str()])
        .args(["queue", "--thread", &target.id, "--message", text])
        .env(DELIVERY_ID_ENV, id)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| MutationError::Blocked)?;
    let deadline = Instant::now() + QUEUE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(MutationError::Uncertain);
            }
        }
    };
    if !status.success() {
        return Err(MutationError::Uncertain);
    }
    let mut output = String::new();
    child
        .stdout
        .take()
        .ok_or(MutationError::Uncertain)?
        .take(512)
        .read_to_string(&mut output)
        .map_err(|_| MutationError::Uncertain)?;
    let receipt = QueueReceipt::parse_stdout(&output).map_err(|_| MutationError::Uncertain)?;
    if receipt.thread_id != target.id || receipt.message_id != message_id {
        return Err(MutationError::Uncertain);
    }
    Ok(Receipt {
        id: id.into(),
        session: target.clone(),
        state: DeliveryState::Queued,
        external_id: Some(receipt.message_id),
    })
}

fn queue_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("queue-receipt-{id}.json"))
}

fn valid_prompt(text: &str) -> bool {
    text.len() <= 8_192 && !text.trim().is_empty() && !text.contains('\0')
}

/// Stub for the X-5 executable invocation. Only the fixture binary registers
/// this entry point. The child rechecks the owner's thread before writing.
pub fn fixture_cli(mut args: impl Iterator<Item = OsString>) -> io::Result<()> {
    let root = PathBuf::from(args.next().ok_or_else(invalid)?);
    let profile = PathBuf::from(std::env::var_os("ROOOM_SMOKE_ROOT").ok_or_else(invalid)?);
    if !profile.is_absolute() || root != profile.join("adapters").join("codex") {
        return Err(invalid());
    }
    storage::verify_private(&root)?;
    let args = args.collect::<Vec<_>>();
    let [queue, thread_flag, thread_id, message_flag, message] = args.as_slice() else {
        return Err(invalid());
    };
    if queue != "queue" || thread_flag != "--thread" || message_flag != "--message" {
        return Err(invalid());
    }
    let message = message
        .to_str()
        .filter(|value| valid_prompt(value))
        .ok_or_else(invalid)?;
    let receipt = queue_fixture(
        &root,
        thread_id.to_str().ok_or_else(invalid)?,
        &std::env::var(DELIVERY_ID_ENV).map_err(|_| invalid())?,
        message,
    )?;
    writeln!(
        io::stdout(),
        "Queued message {} for thread {}.",
        receipt.message_id,
        receipt.thread_id
    )
}

fn queue_fixture(
    root: &Path,
    thread_id: &str,
    id: &str,
    message: &str,
) -> io::Result<QueueReceipt> {
    let route = get_route(root).map_err(|_| invalid())?;
    let message_id = queue_message_id(id).ok_or_else(invalid)?;
    if route.rollout.thread.id != thread_id || !valid_prompt(message) {
        return Err(invalid());
    }
    let expected = optional_text(&root.join("expected-prompt.txt")).map_err(|_| invalid())?;
    if expected.as_deref().is_some_and(|text| text != message) {
        return Err(invalid());
    }
    let receipt = QueueReceipt {
        message_id,
        thread_id: thread_id.into(),
    };
    write_once(&queue_path(root, id), &receipt)?;
    Ok(receipt)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApprovalReceipt {
    handled_by_client_id: String,
    conversation_id: String,
    request_id: String,
    turn_id: String,
    item_id: String,
    result: ApprovalResult,
}

#[derive(Deserialize, Serialize)]
struct ApprovalResult {
    method: String,
    result: ApprovalStatus,
}

#[derive(Deserialize, Serialize)]
struct ApprovalStatus {
    ok: bool,
    decision: String,
}

impl ApprovalReceipt {
    fn confirms(&self, approval: &Approval, command: &CommandRequest) -> bool {
        self.handled_by_client_id == approval.handled_by_client_id
            && self.conversation_id == command.params.thread_id
            && self.request_id == command.id
            && self.turn_id == command.params.turn_id
            && self.item_id == command.params.item_id
            && self.result.method == codex::COMMAND_DECISION
            && self.result.result.ok
            && matches!(self.result.result.decision.as_str(), "accept" | "decline")
    }
}

fn has_confirmation(
    root: &Path,
    approval: &Approval,
    command: &CommandRequest,
) -> Result<bool, &'static str> {
    let path = root.join("approval-receipt.json");
    optional_text(&path)?
        .map(|input| parse_json::<ApprovalReceipt>(&input).map(|r| r.confirms(approval, command)))
        .transpose()
        .map(|ok| ok.unwrap_or(false))
}

pub fn answer(
    root: &Path,
    _now_ms: u64,
    target: &RequestKey,
    decision: Decision,
) -> Result<(), MutationError> {
    let route = get_route(root).map_err(|_| MutationError::Blocked)?;
    let (approval, command) = pending(root)
        .map_err(|_| MutationError::Blocked)?
        .ok_or(MutationError::Blocked)?;
    if route.session() != target.session
        || command.id != target.id
        || !route.matches(&approval, &command)
    {
        return Err(MutationError::Blocked);
    }
    let decision = match decision {
        Decision::Allow
            if command
                .params
                .available_decisions
                .iter()
                .any(|value| value == "accept") =>
        {
            "accept"
        }
        Decision::Allow | Decision::AllowForSession => return Err(MutationError::Blocked),
        Decision::Deny => "decline",
    };
    let receipt = ApprovalReceipt {
        handled_by_client_id: approval.handled_by_client_id,
        conversation_id: command.params.thread_id,
        request_id: command.id,
        turn_id: command.params.turn_id,
        item_id: command.params.item_id,
        result: ApprovalResult {
            method: codex::COMMAND_DECISION.into(),
            result: ApprovalStatus {
                ok: true,
                decision: decision.into(),
            },
        },
    };
    write_once(&root.join("approval-receipt.json"), &receipt).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            MutationError::Blocked
        } else {
            MutationError::Uncertain
        }
    })
}

fn write_once<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| invalid())?;
    if path.exists() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    storage::publish(path, &bytes, false)
}

fn read_text(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    File::open(path)?
        .take(codex::MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    if text.len() > codex::MAX_DOCUMENT_BYTES {
        return Err(invalid());
    }
    Ok(text)
}

fn optional_text(path: &Path) -> Result<Option<String>, &'static str> {
    match read_text(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Cannot read Codex fixture record"),
    }
}

fn parse_json<T: DeserializeOwned>(input: &str) -> Result<T, &'static str> {
    serde_json::from_str(input).map_err(|_| "Malformed Codex fixture record")
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid Codex fixture invocation or record",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicU32, Ordering};

    const THREAD: &str = include_str!("../../../../tests/fixtures/codex/thread.jsonl");
    const APPROVAL: &str = include_str!("../../../../tests/fixtures/codex/approval.json");
    const OWNER: &str = include_str!("../../../../tests/fixtures/codex/owner.json");
    const SETTINGS: &str = include_str!("../../../../tests/fixtures/codex/settings.json");
    const MESSAGE: &str = "Synthetic Codex message utf8 ✓";

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!(
                "rooom-m3-codex-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&root).unwrap();
            let fixture = Self(root);
            fixture.put("thread.jsonl", THREAD);
            fixture.put("owner.json", OWNER);
            fixture.put("approval.json", APPROVAL);
            fixture.put("settings.json", SETTINGS);
            fixture.put("expected-prompt.txt", MESSAGE);
            fixture
        }

        fn put(&self, name: &str, contents: &str) {
            fs::write(self.0.join(name), contents).unwrap();
        }

        fn has_write(&self) -> bool {
            self.0.join("approval-receipt.json").exists()
                || queue_path(&self.0, "delivery-1").exists()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn target() -> RequestKey {
        RequestKey {
            session: SessionKey {
                client: Client::Codex,
                id: "session-1".into(),
            },
            id: "request-1".into(),
        }
    }

    #[test]
    fn answers_the_observed_request_once_and_requires_correlated_delivery_evidence() {
        let fixture = Fixture::new();
        let target = target();
        let feed = read(&fixture.0, 0).unwrap();
        assert_eq!(feed.sessions[0].key, target.session);
        assert_eq!(feed.requests[0].key, target);
        assert_eq!(feed.timeline[0].session, target.session);
        assert!(!fixture.has_write());

        answer(&fixture.0, 0, &target, Decision::Deny).unwrap();
        assert!(matches!(
            answer(&fixture.0, 0, &target, Decision::Allow),
            Err(MutationError::Blocked)
        ));
        let approval_log = read_text(&fixture.0.join("approval-receipt.json")).unwrap();
        let receipt: ApprovalReceipt = parse_json(&approval_log).unwrap();
        assert_eq!(receipt.result.result.decision, "decline");
        assert!(read(&fixture.0, 0).unwrap().requests.is_empty());

        assert!(queue_fixture(&fixture.0, "other-thread", "delivery-1", MESSAGE).is_err());
        assert!(
            queue_fixture(&fixture.0, &target.session.id, "delivery-1", "wrong words").is_err()
        );
        assert!(!queue_path(&fixture.0, "delivery-1").exists());
        let receipt = queue_fixture(&fixture.0, &target.session.id, "delivery-1", MESSAGE).unwrap();
        assert!(queue_fixture(&fixture.0, &target.session.id, "delivery-1", MESSAGE).is_err());
        let queue_log = read_text(&queue_path(&fixture.0, "delivery-1")).unwrap();
        assert!(!queue_log.contains(MESSAGE));
        assert_eq!(
            read(&fixture.0, 0).unwrap().receipts[0].state,
            DeliveryState::Queued
        );

        fixture.put(
            "delivery-events.jsonl",
            &json!({
                "type": "task_started", "thread_id": "other-thread",
                "queued_submission_id": receipt.message_id, "turn_id": "turn-2"
            })
            .to_string(),
        );
        assert_eq!(
            read(&fixture.0, 0).unwrap().receipts[0].state,
            DeliveryState::Queued
        );
        fixture.put(
            "delivery-events.jsonl",
            &json!({
                "type": "queue_taken", "thread_id": "session-1",
                "queued_submission_id": receipt.message_id
            })
            .to_string(),
        );
        assert_eq!(
            read(&fixture.0, 0).unwrap().receipts[0].state,
            DeliveryState::Received
        );
        fixture.put(
            "delivery-events.jsonl",
            &json!({
                "type": "task_started", "thread_id": "session-1",
                "queued_submission_id": receipt.message_id, "turn_id": "turn-2"
            })
            .to_string(),
        );
        assert_eq!(
            read(&fixture.0, 0).unwrap().receipts[0].state,
            DeliveryState::Started
        );
        assert_eq!(
            read_text(&fixture.0.join("settings.json")).unwrap(),
            SETTINGS
        );
    }

    #[test]
    fn disabled_foreign_unknown_and_missing_owners_write_nothing() {
        let fixture = Fixture::new();
        let target = target();
        for (field, value) in [
            ("enabled", json!(false)),
            ("desktopVersion", json!("0.0.0")),
            ("engineVersion", json!("0.0.0")),
            ("handledByClientId", Value::Null),
            ("conversationId", json!("foreign-thread")),
        ] {
            let mut owner: Value = serde_json::from_str(OWNER).unwrap();
            owner[field] = value;
            fixture.put("owner.json", &owner.to_string());
            assert!(
                read(&fixture.0, 0).is_err(),
                "unchecked owner field {field}"
            );
            assert!(matches!(
                answer(&fixture.0, 0, &target, Decision::Allow),
                Err(MutationError::Blocked)
            ));
            assert!(matches!(
                send(&fixture.0, 0, &target.session, "delivery-1", MESSAGE),
                Err(MutationError::Blocked)
            ));
            assert!(queue_fixture(&fixture.0, &target.session.id, "delivery-1", MESSAGE).is_err());
            assert!(!fixture.has_write());
        }
        fixture.put("owner.json", OWNER);
        fixture.put(
            "thread.jsonl",
            &THREAD.replace("Codex Desktop", "codex-tui"),
        );
        assert!(read(&fixture.0, 0).is_err());
        assert!(matches!(
            send(&fixture.0, 0, &target.session, "delivery-1", MESSAGE),
            Err(MutationError::Blocked)
        ));
        assert!(queue_fixture(&fixture.0, &target.session.id, "delivery-1", MESSAGE).is_err());
        assert!(!fixture.has_write());
    }

    #[test]
    fn same_request_id_with_changed_turn_owner_or_wire_is_not_answered() {
        let fixture = Fixture::new();
        let target = target();
        assert_eq!(read(&fixture.0, 0).unwrap().requests[0].key, target);
        for owner in [
            OWNER.replace("desktop-owner-1", "replacement-owner"),
            OWNER.replace("turn-1", "turn-2"),
            OWNER.replace("synthetic approval", "changed command"),
        ] {
            fixture.put("owner.json", &owner);
            assert!(read(&fixture.0, 0).unwrap().requests.is_empty());
            assert!(matches!(
                answer(&fixture.0, 0, &target, Decision::Allow),
                Err(MutationError::Blocked)
            ));
            assert!(!fixture.has_write());
        }
    }
}
