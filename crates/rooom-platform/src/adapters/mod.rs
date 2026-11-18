pub mod antigravity;
pub mod claude;
#[cfg(feature = "adapter-fixtures")]
pub mod codex;
mod codex_live;
pub mod harness;
pub mod storage;
mod windows;

use rooom_core::inbox::{
    Client, Decision, Receipt, Request, RequestKey, Session, SessionKey, TimelineEntry,
};
use std::path::Path;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

#[derive(Default)]
pub struct Feed {
    pub warning: Option<&'static str>,
    pub sessions: Vec<Session>,
    pub requests: Vec<Request>,
    pub receipts: Vec<Receipt>,
    pub timeline: Vec<TimelineEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationError {
    Blocked,
    Uncertain,
}

#[derive(Default)]
pub struct DeliveryTrackers {
    antigravity: antigravity::DeliveryTracker,
    claude: claude::HistoryTracker,
    codex: codex_live::DeliveryTracker,
    harness: harness::HistoryTracker,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SessionLink {
    Uri {
        uri: String,
        error: &'static str,
    },
    Desktop {
        window: windows::DesktopWindow,
        unavailable: &'static str,
        denied: &'static str,
    },
}

impl SessionLink {
    /// Open the checked destination on the resident's Windows UI thread.
    pub fn open(self) -> Result<(), &'static str> {
        match self {
            Self::Uri { uri, error } => windows::execute_uri(&uri, SW_SHOWNORMAL)
                .then_some(())
                .ok_or(error),
            Self::Desktop {
                window,
                unavailable,
                denied,
            } => window.foreground(unavailable, denied),
        }
    }
}

pub fn request_session_link(
    root: &Path,
    request: &RequestKey,
    now_ms: u64,
    fixtures: bool,
) -> Result<SessionLink, &'static str> {
    storage::verify_private(root).map_err(|_| "The client channel folder is not private.")?;
    match (fixtures, request.session.client) {
        (false, Client::Codex) => codex_live::session_link(root, &request.session),
        (false, Client::Claude) => claude::session_link(root, request, now_ms),
        (false, Client::Antigravity) => antigravity::session_link(root, request, now_ms),
        (false, Client::Harness) => harness::session_link(root, request, now_ms),
        _ => Err("This request has no verified session link."),
    }
}

pub fn read(
    client: Client,
    root: &Path,
    now_ms: u64,
    trackers: Option<&mut DeliveryTrackers>,
    #[cfg_attr(not(feature = "adapter-fixtures"), allow(unused_variables))] fixtures: bool,
) -> Result<Feed, &'static str> {
    storage::verify_private(root).map_err(|_| "The client channel folder is not private.")?;
    match client {
        Client::Claude => claude::read(root, now_ms, trackers.map(|t| &mut t.claude)),
        #[cfg(feature = "adapter-fixtures")]
        Client::Codex if fixtures => codex::read(root, now_ms),
        Client::Codex => codex_live::read(root, now_ms, trackers.map(|t| &mut t.codex)),
        Client::Antigravity => {
            antigravity::read(root, now_ms, trackers.map(|t| &mut t.antigravity))
        }
        #[cfg(feature = "adapter-fixtures")]
        Client::Harness if fixtures => harness::read(root, now_ms),
        Client::Harness => harness::read_live(root, now_ms, trackers.map(|t| &mut t.harness)),
    }
}

pub fn send(
    root: &Path,
    now_ms: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
    trackers: &mut DeliveryTrackers,
    #[cfg_attr(not(feature = "adapter-fixtures"), allow(unused_variables))] fixtures: bool,
) -> Result<Receipt, MutationError> {
    storage::verify_private(root).map_err(|_| MutationError::Blocked)?;
    match target.client {
        Client::Claude => claude::send(root, now_ms, target, id, text),
        #[cfg(feature = "adapter-fixtures")]
        Client::Codex if fixtures => codex::send(root, now_ms, target, id, text),
        Client::Codex => codex_live::send(root, now_ms, target, id, text, &mut trackers.codex),
        Client::Antigravity => {
            antigravity::send(root, now_ms, target, id, text, &mut trackers.antigravity)
        }
        #[cfg(feature = "adapter-fixtures")]
        Client::Harness if fixtures => harness::send(root, now_ms, target, id, text),
        Client::Harness => harness::send_live(root, now_ms, target, id, text),
    }
}

pub fn answer(
    root: &Path,
    now_ms: u64,
    expected: &Request,
    decision: Decision,
    trackers: &mut DeliveryTrackers,
    fixtures: bool,
) -> Result<(), MutationError> {
    storage::verify_private(root).map_err(|_| MutationError::Blocked)?;
    let target = &expected.key;
    if target.session.client == Client::Codex && !fixtures {
        return codex_live::answer(root, expected, decision, &mut trackers.codex);
    }
    if decision == Decision::AllowForSession {
        return Err(MutationError::Blocked);
    }
    let fresh = read(target.session.client, root, now_ms, None, fixtures)
        .map_err(|_| MutationError::Blocked)?;
    if !fresh.requests.contains(expected) {
        return Err(MutationError::Blocked);
    }
    match target.session.client {
        Client::Claude => claude::answer(root, now_ms, target, decision),
        #[cfg(feature = "adapter-fixtures")]
        Client::Codex if fixtures => codex::answer(root, now_ms, target, decision),
        Client::Codex => Err(MutationError::Blocked),
        Client::Antigravity => antigravity::answer_expected(root, now_ms, expected, decision),
        #[cfg(feature = "adapter-fixtures")]
        Client::Harness if fixtures => harness::answer(root, now_ms, target, decision),
        Client::Harness => harness::answer_live(root, now_ms, expected, decision),
    }
}
