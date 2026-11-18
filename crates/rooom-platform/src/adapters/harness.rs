use super::{Feed, MutationError, SessionLink, storage, windows::DesktopWindow};
use crate::pipe::Scope;
use rooom_clients::harness::{self, CheckIn, Event, Stage};
use rooom_core::inbox::{
    Client, Decision, DeliveryState, Receipt, Request, RequestKey, Session, SessionKey,
    TimelineEntry,
};
use serde::{Deserialize, Serialize};
mod history;
pub use history::HistoryTracker;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

const FRESH_MS: u64 = 15_000;
const REQUEST_MS: u64 = 600_000;
const QUEUE_MS: u64 = 3_600_000;
const CLOCK_SLOP_MS: u64 = 1000;
const EPOCH_FILETIME_MS: u64 = 11_644_473_600_000;
const RENDERER_MS: u64 = 5_000;
const OPEN_MS: u64 = 3_000;
const NO_WINDOW: &str = "The verified Harness Desktop window is unavailable.";
const NO_REQUEST: &str = "The Harness request is no longer available.";

fn fresh(now: u64, timestamp: u64, lifetime: u64) -> bool {
    timestamp <= now.saturating_add(CLOCK_SLOP_MS) && now.saturating_sub(timestamp) < lifetime
}

#[cfg(feature = "adapter-fixtures")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
}

#[cfg(feature = "adapter-fixtures")]
fn check_in(root: &Path, now: u64) -> Result<CheckIn, &'static str> {
    let config =
        fs::read(root.join("settings.json")).map_err(|_| "Harness plugin is not set up.")?;
    let config: Settings =
        serde_json::from_slice(&config).map_err(|_| "Malformed Harness settings.")?;
    if !config.enabled {
        return Err("Harness adapter is disabled.");
    }
    checked_record(root, now)
}

fn record(path: &Path) -> io::Result<Vec<u8>> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    let info = fs::symlink_metadata(path)?;
    if !info.is_file() || info.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(bytes)
}

fn checked_record(root: &Path, now: u64) -> Result<CheckIn, &'static str> {
    let bytes = record(&root.join("check-in.json")).map_err(|_| "No Harness plugin check-in.")?;
    let check = harness::parse_check_in(&bytes)?;
    if !fresh(now, check.at, FRESH_MS) {
        return Err("The Harness plugin check-in is stale.");
    }
    Ok(check)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveConfig {
    host_executable: PathBuf,
    session: Option<String>,
    history_directory: Option<PathBuf>,
}

impl LiveConfig {
    fn read(root: &Path) -> Result<Self, &'static str> {
        let bytes = record(&root.join("live.json")).map_err(|_| "Harness plugin is not set up.")?;
        let config: Self =
            serde_json::from_slice(&bytes).map_err(|_| "Malformed Harness live configuration.")?;
        if config
            .host_executable
            .file_name()
            .is_none_or(|name| name != "DeepSeek Harness.exe")
            || super::windows::product_version(&config.host_executable).ok() != Some([0, 2, 2, 0])
            || config.session.as_deref().is_some_and(|id| !safe_id(id))
            || config
                .history_directory
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
        {
            return Err("Unsupported Harness Desktop installation or channel.");
        }
        Ok(config)
    }

    fn check(&self, root: &Path, id: &str, now: u64) -> Result<CheckIn, &'static str> {
        const BAD_HOST: &str = "The Harness plugin host is not verified.";
        storage::verify_private(root).map_err(|_| BAD_HOST)?;
        let check = checked_record(root, now)?;
        let pid = check.pid.ok_or(BAD_HOST)?;
        let process = super::claude::process_info(pid).map_err(|_| BAD_HOST)?;
        let created = (process.creation_ticks / 10_000)
            .checked_sub(EPOCH_FILETIME_MS)
            .ok_or(BAD_HOST)?;
        let scope = Scope::current().map_err(|_| BAD_HOST)?;
        if check.agent.id != id
            || check.at < created
            || !scope.owns_process(pid).unwrap_or(false)
            || !super::claude::same_image(&process.image, &self.host_executable)
        {
            return Err(BAD_HOST);
        }
        Ok(check)
    }

    fn window(&self, check: &CheckIn, now: u64) -> Option<DesktopWindow> {
        let renderer = check.renderer.as_ref()?;
        let host_pid = check.pid?;
        if !fresh(now, renderer.at, RENDERER_MS)
            || super::claude::parent_pid(host_pid).ok()? != renderer.pid
        {
            return None;
        }
        let host = super::claude::process_info(host_pid).ok()?;
        let scope = Scope::current().ok()?;
        let window = DesktopWindow::find(&scope, &self.host_executable, |pid| pid == renderer.pid)?;
        (window.creation < host.creation_ticks).then_some(window)
    }
}

fn resident_check_in(root: &Path, host_pid: u32, now: u64) -> Result<(), &'static str> {
    let path = root.join("resident.json");
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ResidentCheckIn {
        pid: u32,
        host_pid: u32,
        at: u64,
    }
    let pid = std::process::id();
    if record(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ResidentCheckIn>(&bytes).ok())
        .is_some_and(|check| {
            check.pid == pid && check.host_pid == host_pid && fresh(now, check.at, 2000)
        })
    {
        return Ok(());
    }
    let bytes = serde_json::json!({"pid":pid,"hostPid":host_pid,"at":now}).to_string();
    storage::publish(&path, bytes.as_bytes(), true)
        .map_err(|_| "The Harness resident check-in could not be saved.")
}

pub fn read_live(
    root: &Path,
    now: u64,
    mut history: Option<&mut HistoryTracker>,
) -> Result<Feed, &'static str> {
    let config = LiveConfig::read(root)?;
    let sessions = root.join("sessions");
    storage::verify_private(&sessions)
        .map_err(|_| "The Harness sessions folder is unavailable.")?;
    let mut feed = Feed::default();
    let mut last_error = None;
    for item in fs::read_dir(sessions)
        .map_err(|_| "The Harness sessions folder is unavailable.")?
        .flatten()
    {
        let name = item.file_name();
        let Some(id) = name.to_str().filter(|id| safe_id(id)) else {
            continue;
        };
        if config
            .session
            .as_deref()
            .is_some_and(|selected| selected != id)
        {
            continue;
        }
        let folder = item.path();
        if storage::verify_private(&folder).is_err() {
            continue;
        }
        match config.check(&folder, id, now) {
            Ok(check) => {
                if let Some(pid) = check.pid {
                    resident_check_in(&folder, pid, now)?;
                }
                expire(&folder, now)?;
                let can_open = config.window(&check, now).is_some();
                let mut other = read_session(&folder, now, check)?;
                if can_open {
                    for request in &mut other.requests {
                        request.can_open_session = request.decision_blocked.is_none();
                    }
                }
                if let Some(directory) = &config.history_directory {
                    let fallback = &mut HistoryTracker::default();
                    let tracker = history.as_deref_mut().unwrap_or(fallback);
                    let (mut timeline, warning) = tracker.project(directory, id);
                    feed.warning = feed.warning.or(warning);
                    // Durable messages carry the same native IDs as plugin events.
                    other
                        .timeline
                        .retain(|entry| !timeline.iter().any(|message| message.id == entry.id));
                    timeline.append(&mut other.timeline);
                    other.timeline = timeline;
                }
                feed.sessions.extend(other.sessions);
                feed.requests.extend(other.requests);
                feed.receipts.extend(other.receipts);
                feed.timeline.extend(other.timeline);
            }
            Err(error) => {
                let _ = expire(&folder, now);
                last_error = Some(error);
            }
        }
    }
    if feed.sessions.is_empty() {
        return Err(last_error.unwrap_or("The Harness plugin has no fresh session check-in."));
    }
    if let Some(tracker) = history {
        tracker.retain(&feed.sessions);
    }
    Ok(feed)
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Navigation {
    session_id: String,
    call_id: String,
    host_pid: u32,
    nonce: String,
    expires_at: u64,
    opened: bool,
}

pub(super) fn session_link(
    root: &Path,
    request: &RequestKey,
    now: u64,
) -> Result<SessionLink, &'static str> {
    if !safe_id(&request.id) {
        return Err(NO_REQUEST);
    }
    let (folder, check) = live_session(root, &request.session, now).map_err(|_| NO_REQUEST)?;
    let config = LiveConfig::read(root)?;
    let window = config.window(&check, now).ok_or(NO_WINDOW)?;
    let host_pid = check.pid.ok_or(NO_WINDOW)?;
    if !read_session(&folder, now, check)?
        .requests
        .iter()
        .any(|item| item.key == *request && item.decision_blocked.is_none())
    {
        return Err(NO_REQUEST);
    }
    let navigation = folder.join("navigation");
    storage::verify_private(&navigation).map_err(|_| NO_WINDOW)?;
    let file = navigation.join(format!("{}.json", request.id));
    let get_request = || {
        record(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Navigation>(&bytes).ok())
    };
    if get_request().is_some_and(|old| old.expires_at <= now) {
        fs::remove_file(&file).map_err(|_| NO_WINDOW)?;
    }
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nonce = format!(
        "{}-{now}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut proof = Navigation {
        session_id: request.session.id.clone(),
        call_id: request.id.clone(),
        host_pid,
        nonce,
        expires_at: now.saturating_add(OPEN_MS),
        opened: false,
    };
    let payload = serde_json::to_vec(&proof).map_err(|_| NO_WINDOW)?;
    storage::publish(&file, &payload, false).map_err(|_| NO_WINDOW)?;
    proof.opened = true;
    let deadline = Instant::now() + Duration::from_millis(OPEN_MS);
    let confirmed = loop {
        if get_request().is_some_and(|pending| pending == proof) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(40));
    };
    let _ = fs::remove_file(file);
    if !confirmed {
        return Err("Harness Desktop did not confirm the session selection.");
    }
    Ok(SessionLink::Desktop {
        window,
        unavailable: NO_WINDOW,
        denied: "Windows could not bring Harness Desktop to the foreground.",
    })
}

fn live_session(
    root: &Path,
    target: &SessionKey,
    now: u64,
) -> Result<(PathBuf, CheckIn), MutationError> {
    if target.client != Client::Harness || !safe_id(&target.id) {
        return Err(MutationError::Blocked);
    }
    let config = LiveConfig::read(root).map_err(|_| MutationError::Blocked)?;
    if config
        .session
        .as_ref()
        .is_some_and(|selected| selected != &target.id)
    {
        return Err(MutationError::Blocked);
    }
    let parent = root.join("sessions");
    storage::verify_private(&parent).map_err(|_| MutationError::Blocked)?;
    let session = parent.join(&target.id);
    let check = config
        .check(&session, &target.id, now)
        .map_err(|_| MutationError::Blocked)?;
    Ok((session, check))
}

fn paths(folder: PathBuf) -> Result<Vec<PathBuf>, &'static str> {
    storage::verify_private(&folder).map_err(|_| "Harness plugin folder is not private.")?;
    fs::read_dir(folder)
        .map_err(|_| "Harness plugin folder is unavailable.")?
        .map(|item| {
            let item = item.map_err(|_| "Harness plugin entry is unreadable.")?;
            if !item
                .file_type()
                .map_err(|_| "Harness plugin entry is unreadable.")?
                .is_file()
            {
                return Err("Unexpected Harness plugin entry.");
            }
            Ok(item.path())
        })
        .filter(|file| {
            !file
                .as_ref()
                .is_ok_and(|p| p.extension().is_some_and(|ext| ext != "json"))
        })
        .collect()
}

#[cfg(feature = "adapter-fixtures")]
pub fn read(root: &Path, now_ms: u64) -> Result<Feed, &'static str> {
    // Expiration is independent of the plugin being present or enabled.
    expire(root, now_ms)?;
    let check = check_in(root, now_ms)?;
    read_session(root, now_ms, check)
}

fn read_session(root: &Path, now_ms: u64, check: CheckIn) -> Result<Feed, &'static str> {
    let key = SessionKey {
        client: Client::Harness,
        id: check.agent.id,
    };
    let mut feed = Feed {
        sessions: vec![Session {
            key: key.clone(),
            title: check.title,
            send_blocked: None,
        }],
        ..Feed::default()
    };
    for file in paths(root.join("events"))? {
        let raw = match record(&file) {
            Ok(raw) => raw,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Harness event is unavailable."),
        };
        match harness::parse_event(&raw)? {
            Event::Approval {
                agent,
                call_id,
                tool_name,
                reason,
                at,
                cwd,
            } if agent.id == key.id && fresh(now_ms, at, REQUEST_MS) => {
                if root
                    .join("decided")
                    .join(format!("{call_id}.json"))
                    .is_file()
                {
                    continue;
                }
                let staged = root
                    .join("answers")
                    .join(format!("{call_id}.json"))
                    .is_file();
                feed.requests.push(Request {
                    key: RequestKey {
                        session: key.clone(),
                        id: call_id,
                    },
                    summary: tool_name,
                    workspace: super::windows::workspace_label(cwd.as_str()),
                    detail: reason,
                    decision_blocked: staged.then(|| "Waiting for the Harness plugin.".into()),
                    allow_for_session: false,
                    can_open_session: false,
                });
            }
            Event::Message {
                agent,
                message_id,
                text,
            } if agent.id == key.id => feed.timeline.push(TimelineEntry {
                session: key.clone(),
                id: message_id,
                text,
            }),
            _ => {}
        }
    }
    for file in paths(root.join("receipts"))? {
        let raw = record(&file).map_err(|_| "Harness receipt is unavailable.")?;
        let proof = harness::parse_delivery(&raw)?;
        if proof.session_id != key.id
            || file
                .file_stem()
                .is_none_or(|name| name != proof.request_id.as_str())
        {
            return Err("Harness receipt does not match its target.");
        }
        feed.receipts.push(Receipt {
            external_id: Some(proof.request_id.clone()),
            id: proof.request_id,
            session: key.clone(),
            state: match proof.stage {
                Stage::Accepted => DeliveryState::Received,
                Stage::Started => DeliveryState::Started,
                Stage::Refused => DeliveryState::Rejected,
                Stage::Expired => DeliveryState::Expired,
            },
        });
    }
    Ok(feed)
}

#[cfg(feature = "adapter-fixtures")]
pub fn send(
    root: &Path,
    now_ms: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
) -> Result<Receipt, MutationError> {
    let feed = read(root, now_ms).map_err(|_| MutationError::Blocked)?;
    if !feed.sessions.iter().any(|s| &s.key == target) || !safe_id(id) || text.trim().is_empty() {
        return Err(MutationError::Blocked);
    }
    publish_prompt(root, now_ms, target, id, text)
}

pub fn send_live(
    root: &Path,
    now: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
) -> Result<Receipt, MutationError> {
    if !safe_id(id) || text.trim().is_empty() || text.len() > 16 * 1024 || text.contains('\0') {
        return Err(MutationError::Blocked);
    }
    let (folder, _) = live_session(root, target, now)?;
    publish_prompt(&folder, now, target, id, text)
}

fn publish_prompt(
    root: &Path,
    now_ms: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
) -> Result<Receipt, MutationError> {
    let folder = root.join("prompts");
    storage::verify_private(&folder).map_err(|_| MutationError::Blocked)?;
    let path = folder.join(format!("{id}.json"));
    let bytes = serde_json::to_vec(&serde_json::json!({
        "requestId": id, "sessionId": target.id, "mode": "queue",
        "content": [{ "type": "text", "text": text }],
        "expiresAt": now_ms.saturating_add(QUEUE_MS),
    }))
    .map_err(|_| MutationError::Blocked)?;
    storage::publish(&path, &bytes, false).map_err(|_| MutationError::Blocked)?;
    Ok(Receipt {
        id: id.into(),
        session: target.clone(),
        state: DeliveryState::Queued,
        external_id: Some(id.into()),
    })
}

#[cfg(feature = "adapter-fixtures")]
pub fn answer(
    root: &Path,
    now_ms: u64,
    target: &RequestKey,
    decision: Decision,
) -> Result<(), MutationError> {
    let feed = read(root, now_ms).map_err(|_| MutationError::Blocked)?;
    if !feed
        .requests
        .iter()
        .any(|r| &r.key == target && r.decision_blocked.is_none())
    {
        return Err(MutationError::Blocked);
    }
    publish_answer(root, target, decision)
}

pub fn answer_live(
    root: &Path,
    now: u64,
    expected: &Request,
    decision: Decision,
) -> Result<(), MutationError> {
    let (folder, check) = live_session(root, &expected.key.session, now)?;
    let feed = read_session(&folder, now, check).map_err(|_| MutationError::Blocked)?;
    if !matches_pending_approval(&feed.requests, expected) {
        return Err(MutationError::Blocked);
    }
    publish_answer(&folder, &expected.key, decision)
}

fn matches_pending_approval(requests: &[Request], expected: &Request) -> bool {
    let mut approval = expected.clone();
    // read_session reads the approval. Window availability is projected separately.
    approval.can_open_session = false;
    approval.decision_blocked.is_none() && requests.contains(&approval)
}

#[cfg(test)]
mod approval_tests {
    use super::*;

    #[test]
    fn renderer_availability_does_not_change_the_approval_identity() {
        let native = Request {
            key: RequestKey {
                session: SessionKey {
                    client: Client::Harness,
                    id: "session-1".into(),
                },
                id: "request-1".into(),
            },
            summary: "shell".into(),
            workspace: Some("fixture".into()),
            detail: "synthetic command".into(),
            decision_blocked: None,
            allow_for_session: false,
            can_open_session: false,
        };
        let mut card = native.clone();
        card.can_open_session = true;
        let native = [native];
        assert!(matches_pending_approval(&native, &card));

        card.detail = "different command".into();
        assert!(!matches_pending_approval(&native, &card));
        card.detail = native[0].detail.clone();
        card.decision_blocked = Some("pending answer".into());
        assert!(!matches_pending_approval(&native, &card));
    }
}

fn publish_answer(
    root: &Path,
    target: &RequestKey,
    decision: Decision,
) -> Result<(), MutationError> {
    let folder = root.join("answers");
    storage::verify_private(&folder).map_err(|_| MutationError::Blocked)?;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "callId": target.id, "sessionId": target.session.id,
        "outcome": match decision {
            Decision::Allow => "allowed-once",
            Decision::Deny => "denied",
            Decision::AllowForSession => return Err(MutationError::Blocked),
        }
    }))
    .map_err(|_| MutationError::Blocked)?;
    storage::publish(&folder.join(format!("{}.json", target.id)), &bytes, false)
        .map_err(|_| MutationError::Blocked)
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expiration {
    request_id: String,
    session_id: String,
    expires_at: u64,
}

fn expire(root: &Path, now_ms: u64) -> Result<(), &'static str> {
    let prompts = root.join("prompts");
    let receipts = root.join("receipts");
    for file in paths(prompts.clone())? {
        let raw = match record(&file) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Harness prompt is unavailable."),
        };
        let prompt: Expiration =
            serde_json::from_slice(&raw).map_err(|_| "Malformed Harness prompt.")?;
        if !safe_id(&prompt.request_id)
            || !safe_id(&prompt.session_id)
            || file
                .file_stem()
                .is_none_or(|name| name != prompt.request_id.as_str())
        {
            return Err("Harness prompt does not match its target.");
        }
        if now_ms < prompt.expires_at {
            continue;
        }
        // Removing the published packet wins against a plugin rename/claim.
        if fs::remove_file(&file).is_ok() {
            storage::verify_private(&receipts)
                .map_err(|_| "Harness receipt folder is not private.")?;
            let proof = serde_json::json!({
                "requestId": prompt.request_id, "sessionId": prompt.session_id, "stage": "expired"
            });
            storage::publish(
                &receipts.join(file.file_name().unwrap()),
                proof.to_string().as_bytes(),
                false,
            )
            .map_err(|_| "Harness expiry receipt could not be saved.")?;
        }
    }
    Ok(())
}
