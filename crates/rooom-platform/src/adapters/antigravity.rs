//! Antigravity's verified Windows loopback service channel.

mod history;
mod navigation;

use super::{Feed, MutationError, SessionLink};
use crate::pipe::Scope;
use rooom_clients::antigravity::{self, Conversation, Hub, NativeTrajectory, TrajectoryCursor};
use rooom_core::inbox::{
    Client, Decision, DeliveryState, Receipt, Request, RequestKey, Session, SessionKey,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP_STATE_ESTAB, MIB_TCP_STATE_LISTEN, MIB_TCPROW_OWNER_PID,
        TCP_TABLE_OWNER_PID_ALL,
    },
    Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT,
};

const RPC_BASE: &str = "/exa.language_server_pb.LanguageServerService/";
const RPC_TIMEOUT: Duration = Duration::from_millis(800);
const MAX_FRAME: usize = 256 * 1024;
const MAX_TRAJECTORY_FRAME: usize = 4 * 1024 * 1024;
const DELIVERY_OBSERVATION_MS: u64 = 3_600_000;

#[derive(Default)]
pub struct DeliveryTracker {
    pending: Vec<PendingDelivery>,
    history: history::Tracker,
}

struct PendingDelivery {
    target: SessionKey,
    id: String,
    text: String,
    before: TrajectoryCursor,
    service: (u32, u16, String),
    expires_at: u64,
    ambiguous: bool,
}

impl DeliveryTracker {
    fn expire(&mut self, now_ms: u64) {
        self.pending.retain(|item| now_ms < item.expires_at);
    }

    fn track(&mut self, mut next: PendingDelivery) {
        for pending in &mut self.pending {
            if pending.service == next.service
                && pending.target == next.target
                && pending.text == next.text
            {
                // Identical outstanding inputs cannot be attributed to distinct
                // sends by this native protocol.
                pending.ambiguous = true;
                next.ambiguous = true;
            }
        }
        self.pending.push(next);
    }

    fn observe(&mut self, endpoint: &Endpoint, feed: &mut Feed) {
        let service = endpoint.delivery_source();
        let mut conversations = BTreeMap::new();
        self.pending.retain(|item| {
            if item.ambiguous
                || item.service != service
                || !feed
                    .sessions
                    .iter()
                    .any(|session| session.key == item.target)
            {
                return true;
            }
            let trajectory = conversations
                .entry(item.target.id.clone())
                .or_insert_with(|| endpoint.native_trajectory(&item.target.id).ok());
            let Some((external_id, started)) = trajectory
                .as_ref()
                .and_then(|trajectory| trajectory.delivery_evidence(&item.before, &item.text))
            else {
                return true;
            };
            if !rooom_core::inbox::identifier(&external_id) {
                return true;
            }
            feed.receipts.push(Receipt {
                id: item.id.clone(),
                session: item.target.clone(),
                state: if started {
                    DeliveryState::Started
                } else {
                    DeliveryState::Received
                },
                external_id: Some(external_id),
            });
            !started
        });
    }
}

pub fn read(
    root: &Path,
    now_ms: u64,
    tracker: Option<&mut DeliveryTracker>,
) -> Result<Feed, &'static str> {
    let mut tracker = tracker;
    if let Some(tracker) = tracker.as_mut() {
        tracker.expire(now_ms);
    }
    let endpoint = Endpoint::verify(root, now_ms)?;
    let mut feed = Feed::default();
    let conversations = endpoint.conversations()?;
    let navigation_ready = conversations.iter().any(|conversation| {
        navigation::cascade_id(&conversation.id) && !conversation.permissions.is_empty()
    }) && endpoint.navigation().is_some_and(|nav| nav.available());
    for conversation in conversations {
        let workspace =
            super::windows::file_uri_workspace_label(conversation.workspace_uri.as_deref());
        let can_open_session = navigation_ready && navigation::cascade_id(&conversation.id);
        let key = SessionKey {
            client: Client::Antigravity,
            id: conversation.id,
        };
        feed.sessions.push(Session {
            key: key.clone(),
            title: conversation.title,
            send_blocked: None,
        });
        for step in conversation.permissions {
            feed.requests.push(Request {
                key: RequestKey {
                    session: key.clone(),
                    id: step.id,
                },
                summary: step.summary,
                workspace: workspace.clone(),
                detail: step.detail,
                decision_blocked: None,
                allow_for_session: false,
                can_open_session,
            });
        }
    }
    if endpoint.hub.uses_trajectory_receipts() {
        let mut temporary_history = history::Tracker::default();
        let history = if let Some(tracker) = tracker {
            tracker.observe(&endpoint, &mut feed);
            &mut tracker.history
        } else {
            &mut temporary_history
        };
        let directory = match &endpoint.live {
            Some(live) => live.config.hub_directory.parent(),
            None => Some(root),
        };
        if let Some(directory) = directory {
            history.append(directory, &mut feed);
        }
    }
    Ok(feed)
}

pub(super) fn session_link(
    root: &Path,
    request: &RequestKey,
    now_ms: u64,
) -> Result<SessionLink, &'static str> {
    const NO_SESSION: &str = "The Antigravity request is no longer available.";
    if request.session.client != Client::Antigravity || !navigation::cascade_id(&request.session.id)
    {
        return Err(NO_SESSION);
    }
    let endpoint = Endpoint::verify(root, now_ms)?;
    if !endpoint.conversations()?.iter().any(|conversation| {
        conversation.id == request.session.id
            && conversation
                .permissions
                .iter()
                .any(|step| step.id == request.id)
    }) {
        return Err(NO_SESSION);
    }
    endpoint
        .require_current(root, now_ms)
        .map_err(|_| NO_SESSION)?;
    let navigation = endpoint
        .navigation()
        .ok_or("The Antigravity Desktop window is unavailable.")?;
    Ok(SessionLink::Desktop {
        window: navigation.open(&request.session.id)?,
        unavailable: "The local Antigravity Desktop page is unavailable.",
        denied: "Windows could not bring Antigravity Desktop to the foreground.",
    })
}

pub fn send(
    root: &Path,
    now_ms: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
    tracker: &mut DeliveryTracker,
) -> Result<Receipt, MutationError> {
    if target.client != Client::Antigravity
        || text.len() > 16 * 1024
        || text.trim().is_empty()
        || !rooom_core::inbox::identifier(id)
    {
        return Err(MutationError::Blocked);
    }
    tracker.expire(now_ms);
    if tracker.pending.iter().any(|item| item.id == id) {
        return Err(MutationError::Blocked);
    }
    let endpoint = Endpoint::verify(root, now_ms).map_err(|_| MutationError::Blocked)?;
    let conversations = endpoint
        .conversations()
        .map_err(|_| MutationError::Blocked)?;
    if !conversations
        .iter()
        .any(|conversation| conversation.id == target.id)
    {
        return Err(MutationError::Blocked);
    }
    let external_id = if endpoint.hub.uses_trajectory_receipts() {
        let trajectory = endpoint
            .native_trajectory(&target.id)
            .map_err(|_| MutationError::Blocked)?;
        let body = trajectory
            .send_body(
                &target.id,
                text,
                &endpoint.status,
                &endpoint.hub.desktop_version,
            )
            .map_err(|_| MutationError::Blocked)?;
        endpoint.require_current(root, now_ms)?;
        let reply = endpoint.post("SendUserCascadeMessage", &body);
        if reply != Err(MutationError::Blocked) {
            tracker.track(PendingDelivery {
                target: target.clone(),
                id: id.into(),
                text: text.into(),
                before: trajectory.cursor(),
                service: endpoint.delivery_source(),
                expires_at: now_ms.saturating_add(DELIVERY_OBSERVATION_MS),
                ambiguous: false,
            });
        }
        antigravity::check_native_send_ack(&reply?).map_err(|_| MutationError::Uncertain)?;
        None
    } else {
        let trajectory = endpoint
            .trajectory(&target.id)
            .map_err(|_| MutationError::Blocked)?;
        let body = antigravity::send_body(&trajectory, &target.id, id, text)
            .map_err(|_| MutationError::Blocked)?;
        endpoint.require_current(root, now_ms)?;
        let reply = endpoint.post("SendUserCascadeMessage", &body)?;
        Some(
            antigravity::parse_send_ack(&reply, &target.id, id)
                .map_err(|_| MutationError::Uncertain)?,
        )
    };
    Ok(Receipt {
        id: id.into(),
        session: target.clone(),
        state: DeliveryState::Queued,
        external_id,
    })
}

/// The resident passes the card that the owner saw, allowing a same-key
/// replacement with changed contents to be rejected before making a write.
pub fn answer_expected(
    root: &Path,
    now_ms: u64,
    expected: &Request,
    decision: Decision,
) -> Result<(), MutationError> {
    let target = &expected.key;
    if target.session.client != Client::Antigravity
        || expected.decision_blocked.is_some()
        || decision == Decision::AllowForSession
    {
        return Err(MutationError::Blocked);
    }
    let endpoint = Endpoint::verify(root, now_ms).map_err(|_| MutationError::Blocked)?;
    let conversations = endpoint
        .conversations()
        .map_err(|_| MutationError::Blocked)?;
    let waiting = conversations
        .into_iter()
        .find(|conversation| {
            conversation.id == target.session.id
                && super::windows::file_uri_workspace_label(conversation.workspace_uri.as_deref())
                    == expected.workspace
        })
        .and_then(|conversation| {
            conversation.permissions.into_iter().find(|step| {
                step.id == target.id
                    && step.summary == expected.summary
                    && step.detail == expected.detail
            })
        })
        .ok_or(MutationError::Blocked)?;
    endpoint.require_current(root, now_ms)?;
    let allow = matches!(decision, Decision::Allow);
    let reply = endpoint.post("HandleCascadeUserInteraction", &waiting.reply(allow))?;
    if endpoint.live.is_none() {
        return antigravity::check_decision_ack(&reply, &waiting, allow)
            .map_err(|_| MutationError::Uncertain);
    }
    antigravity::check_native_send_ack(&reply).map_err(|_| MutationError::Uncertain)?;
    endpoint
        .require_current(root, now_ms)
        .map_err(|_| MutationError::Uncertain)?;
    let after = endpoint
        .conversations()
        .map_err(|_| MutationError::Uncertain)?;
    if after.iter().any(|conversation| {
        conversation.id == target.session.id
            && !conversation.permissions.iter().any(|step| {
                step.trajectory_id == waiting.trajectory_id && step.step_index == waiting.step_index
            })
    }) {
        Ok(())
    } else {
        Err(MutationError::Uncertain)
    }
}

struct Endpoint {
    hub: Hub,
    live: Option<LiveEndpoint>,
    scope: Scope,
    status: Vec<u8>,
}

impl Endpoint {
    fn verify(root: &Path, now_ms: u64) -> Result<Self, &'static str> {
        let scope =
            Scope::current().map_err(|_| "The Antigravity owner session is unavailable.")?;
        let (hub, live) = read_hub(root, now_ms, &scope)?;
        let mut endpoint = Self {
            hub,
            live,
            scope,
            status: Vec::new(),
        };
        let status = endpoint.read_rpc(
            "GetUserStatus",
            &json!({"metadata": antigravity::metadata_for(&endpoint.hub.desktop_version)}),
        )?;
        antigravity::check_status(&status, &endpoint.hub)?;
        endpoint.status = status;
        Ok(endpoint)
    }

    fn delivery_source(&self) -> (u32, u16, String) {
        (
            self.hub.pid,
            self.hub.http_port,
            self.hub.instance_id.clone(),
        )
    }

    fn navigation(&self) -> Option<navigation::Route<'_>> {
        let live = self.live.as_ref()?;
        if self.hub.desktop_version != "2.32.2" {
            return None;
        }
        let https_port = live.https_port?;
        if https_port == 0
            || !owns_listener(self.hub.pid, https_port)
            || native_instance(self.hub.pid, &live.config).as_deref() != Some(&self.hub.instance_id)
        {
            return None;
        }
        navigation::Route::new(&self.scope, &live.config.desktop_executable, https_port)
    }

    fn trajectory(&self, cascade_id: &str) -> Result<Vec<u8>, &'static str> {
        self.read_rpc("GetCascadeTrajectory", &json!({"cascadeId": cascade_id}))
    }

    fn native_trajectory(&self, cascade_id: &str) -> Result<NativeTrajectory, &'static str> {
        antigravity::parse_native_trajectory(&self.trajectory(cascade_id)?, cascade_id)
    }

    fn require_current(&self, root: &Path, now_ms: u64) -> Result<(), MutationError> {
        if read_hub(root, now_ms, &self.scope)
            .is_ok_and(|(hub, live)| hub == self.hub && live == self.live)
        {
            Ok(())
        } else {
            Err(MutationError::Blocked)
        }
    }

    fn conversations(&self) -> Result<Vec<Conversation>, &'static str> {
        let response = self.read_rpc("GetAllCascadeTrajectories", &json!({}))?;
        match &self.live {
            Some(live) => {
                antigravity::parse_native_summaries(&response, live.config.conversation.as_deref())
            }
            None => antigravity::parse_summaries(&response),
        }
    }

    fn read_rpc(&self, method: &'static str, body: &Value) -> Result<Vec<u8>, &'static str> {
        self.post(method, body)
            .map_err(|_| "The verified Antigravity service is unavailable.")
    }

    fn post(&self, method: &'static str, body: &Value) -> Result<Vec<u8>, MutationError> {
        if !self.scope.owns_process(self.hub.pid).unwrap_or(false)
            || self.live.as_ref().is_some_and(|live| {
                native_instance(self.hub.pid, &live.config).as_deref()
                    != Some(&self.hub.instance_id)
            })
        {
            return Err(MutationError::Blocked);
        }
        let port = self.hub.http_port;
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let mut socket = TcpStream::connect_timeout(&address, RPC_TIMEOUT)
            .map_err(|_| MutationError::Blocked)?;
        let client_port = socket
            .local_addr()
            .map_err(|_| MutationError::Blocked)?
            .port();
        if !owns_connection(self.hub.pid, port, client_port) {
            return Err(MutationError::Blocked);
        }
        let bytes = serde_json::to_vec(body).map_err(|_| MutationError::Blocked)?;
        let mut frame = format!(
            "POST {RPC_BASE}{method} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
             Connect-Protocol-Version: 1\r\nX-Codeium-Csrf-Token: {}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.hub.csrf_token,
            bytes.len(),
        )
        .into_bytes();
        frame.extend(bytes);
        socket
            .set_write_timeout(Some(RPC_TIMEOUT))
            .map_err(|_| MutationError::Blocked)?;
        socket
            .write_all(&frame)
            .map_err(|_| MutationError::Uncertain)?;
        let max_frame = if method == "GetCascadeTrajectory" && self.hub.uses_trajectory_receipts() {
            MAX_TRAJECTORY_FRAME
        } else {
            MAX_FRAME
        };
        read_response(&mut socket, max_frame).ok_or(MutationError::Uncertain)
    }
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveConfig {
    desktop_executable: PathBuf,
    service_executable: PathBuf,
    hub_directory: PathBuf,
    conversation: Option<String>,
}

#[derive(PartialEq, Eq)]
struct LiveEndpoint {
    config: LiveConfig,
    https_port: Option<u16>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeHub {
    pid: u32,
    http_port: u16,
    https_port: Option<u16>,
    ls_version: String,
    csrf_token: String,
}

const BAD_ANNOUNCEMENT: &str = "The Antigravity service announcement is unavailable.";

fn read_hub(
    root: &Path,
    now_ms: u64,
    scope: &Scope,
) -> Result<(Hub, Option<LiveEndpoint>), &'static str> {
    let path = root.join("live.json");
    if !matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    {
        let live: LiveConfig = serde_json::from_slice(&read_announcement(&path)?)
            .map_err(|_| "The Antigravity live configuration is invalid.")?;
        let (hub, https_port) = find_native_hub(&live, scope)?;
        return Ok((
            hub,
            Some(LiveEndpoint {
                config: live,
                https_port,
            }),
        ));
    }
    if !cfg!(feature = "adapter-fixtures") {
        return Err("Antigravity has not been enabled.");
    }
    antigravity::parse_hub(&read_announcement(&root.join("hub.json"))?, now_ms)
        .map(|hub| (hub, None))
}

fn read_announcement(path: &Path) -> Result<Vec<u8>, &'static str> {
    if !fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }) {
        return Err(BAD_ANNOUNCEMENT);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(4097).read_to_end(&mut bytes))
        .map_err(|_| BAD_ANNOUNCEMENT)?;
    if bytes.len() > 4096 {
        return Err("The Antigravity service announcement is too large.");
    }
    Ok(bytes)
}

fn native_instance(pid: u32, live: &LiveConfig) -> Option<String> {
    let process = super::claude::process_info(pid).ok()?;
    super::claude::same_image(&process.image, &live.service_executable)
        .then(|| process.creation_ticks.to_string())
}

fn find_native_hub(live: &LiveConfig, scope: &Scope) -> Result<(Hub, Option<u16>), &'static str> {
    const UNSUPPORTED: &str = "The installed Antigravity channel could not be verified.";
    let [major, minor, patch, revision] =
        super::windows::product_version(&live.desktop_executable).map_err(|_| UNSUPPORTED)?;
    let version = format!("{major}.{minor}.{patch}");
    if revision != 0
        || !antigravity::supported_native_version(&version)
        || !live.hub_directory.is_absolute()
        || !live.service_executable.is_absolute()
        || live
            .conversation
            .as_ref()
            .is_some_and(|id| !rooom_core::inbox::identifier(id))
        || !fs::symlink_metadata(&live.hub_directory).is_ok_and(|metadata| {
            metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
        })
    {
        return Err(UNSUPPORTED);
    }
    let mut verified = None;
    for file in fs::read_dir(&live.hub_directory).map_err(|_| BAD_ANNOUNCEMENT)? {
        let path = file.map_err(|_| BAD_ANNOUNCEMENT)?.path();
        let Some(_) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| {
                name.strip_prefix("hub_")?
                    .strip_suffix(".json")?
                    .parse::<u32>()
                    .ok()
            })
        else {
            continue;
        };
        let Some(native) = read_announcement(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<NativeHub>(&bytes).ok())
            .filter(|hub| hub.pid != 0 && hub.ls_version == version)
        else {
            continue;
        };
        // The announcement's filename can carry the launcher PID.
        let pid = native.pid;
        if !scope.owns_process(pid).unwrap_or(false) {
            continue;
        }
        let Some(instance_id) = native_instance(pid, live) else {
            continue;
        };
        let hub = Hub {
            pid,
            http_port: native.http_port,
            desktop_version: version.clone(),
            ls_version: native.ls_version,
            csrf_token: native.csrf_token,
            instance_id,
            issued_at_ms: 0,
            enabled: true,
        };
        if hub.http_port == 0 || !rooom_core::inbox::identifier(&hub.csrf_token) {
            continue;
        }
        let candidate = (hub, native.https_port);
        if verified
            .as_ref()
            .is_some_and(|current| current != &candidate)
        {
            return Err("More than one Antigravity service is running.");
        }
        verified = Some(candidate);
    }
    verified.ok_or(BAD_ANNOUNCEMENT)
}

fn read_response(socket: &mut TcpStream, max_frame: usize) -> Option<Vec<u8>> {
    let deadline = Instant::now() + RPC_TIMEOUT;
    let mut reply = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        socket.set_read_timeout(Some(remaining)).ok()?;
        let count = socket.read(&mut chunk).ok()?;
        if count == 0 {
            break;
        }
        if reply.len() + count > max_frame {
            return None;
        }
        reply.extend_from_slice(&chunk[..count]);
    }
    parse_response(&reply)
}

fn parse_response(reply: &[u8]) -> Option<Vec<u8>> {
    let end_of_headers = reply.windows(4).position(|bytes| bytes == b"\r\n\r\n")?;
    let headers = std::str::from_utf8(&reply[..end_of_headers]).ok()?;
    let mut lines = headers.split("\r\n");
    let mut status = lines.next()?.split_whitespace();
    if status.next()? != "HTTP/1.1" || status.next()? != "200" {
        return None;
    }
    let mut size = None;
    let mut chunked = false;
    for line in lines {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("Transfer-Encoding") {
            if chunked || !value.trim().eq_ignore_ascii_case("chunked") {
                return None;
            }
            chunked = true;
        }
        if name.eq_ignore_ascii_case("Content-Length") {
            if size.is_some() {
                return None;
            }
            size = Some(value.trim().parse::<usize>().ok()?);
        }
    }
    let bytes = &reply[end_of_headers + 4..];
    match (size, chunked) {
        (None, true) => decode_chunks(bytes),
        (Some(size), false) if size == bytes.len() => Some(bytes.to_vec()),
        _ => None,
    }
}

fn decode_chunks(mut bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let (line, rest) = take_line(bytes)?;
        let hex = line.split(|byte| *byte == b';').next()?.trim_ascii_end();
        if !hex.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        let size = usize::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
        if size == 0 {
            bytes = rest;
            loop {
                let (line, rest) = take_line(bytes)?;
                if line.is_empty() {
                    return rest.is_empty().then_some(decoded);
                }
                if !line
                    .iter()
                    .position(|byte| *byte == b':')
                    .is_some_and(|i| i > 0)
                {
                    return None;
                }
                bytes = rest;
            }
        }
        let (chunk, rest) = rest.split_at_checked(size)?;
        bytes = rest.strip_prefix(b"\r\n")?;
        decoded.extend_from_slice(chunk);
    }
}

fn take_line(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = bytes.windows(2).position(|pair| pair == b"\r\n")?;
    Some((&bytes[..end], &bytes[end + 2..]))
}

#[cfg(test)]
mod tests {
    use super::parse_response;

    #[test]
    fn rejects_conflicting_or_incomplete_http_frame() {
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n0\r\n\r\n").is_none());
        assert!(
            parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n")
                .is_none()
        );
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1000000000000000000000000\r\n").is_none());
    }
}

/// Verify the server side of this exact connection, so process reuse on the
/// listening port cannot redirect credentials or a mutating call.
fn owns_connection(pid: u32, port: u16, client_port: u16) -> bool {
    owns_socket(pid, port, Some(client_port))
}

fn owns_listener(pid: u32, port: u16) -> bool {
    owns_socket(pid, port, None)
}

fn owns_socket(pid: u32, port: u16, client_port: Option<u16>) -> bool {
    const AF_INET: u32 = 2;
    const MAX_TABLE: u32 = 16 * 1024 * 1024;
    let mut required_bytes = 0;
    // Windows returns the necessary buffer size for a null buffer.
    let result = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut required_bytes,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if result != ERROR_INSUFFICIENT_BUFFER {
        return false;
    }
    for _ in 0..3 {
        if !(4..=MAX_TABLE).contains(&required_bytes) {
            return false;
        }
        let allocated = required_bytes as usize;
        let mut data = vec![0u32; allocated.div_ceil(4)];
        // A u32 buffer has the alignment required by the count and IPv4 row.
        let result = unsafe {
            GetExtendedTcpTable(
                data.as_mut_ptr().cast(),
                &mut required_bytes,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        let rows = data[0] as usize;
        if result != NO_ERROR || rows > (allocated - 4) / size_of::<MIB_TCPROW_OWNER_PID>() {
            return false;
        }
        // The verified count fits after the leading DWORD in the aligned buffer.
        let records = unsafe {
            std::slice::from_raw_parts(data.as_ptr().add(1).cast::<MIB_TCPROW_OWNER_PID>(), rows)
        };
        let localhost = u32::from_ne_bytes(Ipv4Addr::LOCALHOST.octets());
        return records.iter().any(|row| {
            row.dwOwningPid == pid
                && row.dwLocalAddr == localhost
                && u16::from_be(row.dwLocalPort as u16) == port
                && match client_port {
                    Some(peer) => {
                        row.dwState == MIB_TCP_STATE_ESTAB as u32
                            && row.dwRemoteAddr == localhost
                            && u16::from_be(row.dwRemotePort as u16) == peer
                    }
                    None => row.dwState == MIB_TCP_STATE_LISTEN as u32,
                }
        });
    }
    false
}
