//! Claude Desktop Code's Windows registry, hook broker and plugin folder.

mod history;
pub(super) use history::Tracker as HistoryTracker;

use super::{Feed, MutationError, SessionLink, storage, windows};
use crate::pipe::Scope;
use rooom_clients::claude::{self, ClientConfig, PluginReceipt, PluginState, Prompt, Registry};
use rooom_core::inbox::{
    Client, Decision, DeliveryState, Receipt, Request, RequestKey, Session, SessionKey,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, File},
    io::{self, Read},
    os::windows::{ffi::OsStringExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::UNIX_EPOCH,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
};

const HOOK_LIFETIME_MS: u64 = 600_000;
const NO_CHANNEL: &str = "The Claude plugin has no fresh check-in for this session.";
const STALE_REGISTRY: &str = "The Claude process registry entry is stale or unverified.";
const UNSUPPORTED_ENGINE: &str = "The Claude Code registry reports an unsupported engine version.";
const BAD_FOLDER: &str = "The Claude channel folder is unavailable.";
const UNVERIFIED_CALLER: &str = "The Claude hook caller is unverified.";
const NO_SESSION_LINK: &str = "This request has no verified Claude Desktop session link.";
const DESKTOP_RECORD_LIMIT: u64 = 1024 * 1024;

struct PendingHook {
    root: PathBuf,
    request: Request,
    caller: u32,
    deadline: u64,
    decision: Option<Decision>,
    taken: bool,
}

#[derive(Default)]
struct Hooks {
    pending: Vec<PendingHook>,
    resolved: BTreeSet<(PathBuf, RequestKey)>,
}

fn hooks() -> &'static Mutex<Hooks> {
    static HOOKS: OnceLock<Mutex<Hooks>> = OnceLock::new();
    HOOKS.get_or_init(Mutex::default)
}

pub(super) fn read(
    root: &Path,
    now_ms: u64,
    history: Option<&mut HistoryTracker>,
) -> Result<Feed, &'static str> {
    let config = config(root)?;
    let scope = Scope::current().map_err(|_| STALE_REGISTRY)?;
    let sessions = &config.sessions;
    let mut feed = Feed::default();
    let mut active = BTreeSet::new();
    let mut unsupported = false;
    for file in entries(sessions).map_err(|_| BAD_FOLDER)? {
        let Some(pid) = record_id(&file).and_then(|pid| pid.parse::<u32>().ok()) else {
            continue;
        };
        let registry = match registry(&scope, &config, pid) {
            Ok(registry) => registry,
            Err(UNSUPPORTED_ENGINE) => {
                unsupported = true;
                continue;
            }
            Err(_) => continue,
        };
        let key = key(registry.session_id);
        active.insert((pid, key.clone()));
        let title = registry
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Claude Desktop Code".into());
        let send_blocked = check_in(root, &key, now_ms).err().map(str::to_owned);
        feed.sessions.push(Session {
            key,
            title,
            send_blocked,
        });
    }
    if feed.sessions.is_empty() && unsupported {
        return Err(UNSUPPORTED_ENGINE);
    }
    if let Ok(mut state) = hooks().lock() {
        let expired: Vec<_> = state
            .pending
            .extract_if(.., |hook| hook.deadline <= now_ms)
            .collect();
        state.resolved.extend(
            expired
                .into_iter()
                .map(|hook| (hook.root, hook.request.key)),
        );
        feed.requests = state
            .pending
            .iter()
            .filter(|hook| {
                hook.root == root
                    && active.contains(&(hook.caller, hook.request.key.session.clone()))
            })
            .map(|hook| {
                let mut request = hook.request.clone();
                if hook.decision.is_some() {
                    request.decision_blocked =
                        Some("Waiting for the Claude hook to take the decision.".into());
                }
                request
            })
            .collect();
    }
    collect_delivery(root, now_ms, &mut feed)?;
    if let Some(history) = history {
        history.append(sessions, &mut feed);
    }
    Ok(feed)
}

/// The secured pipe gives us the actual peer PID. Require the same Rooom
/// executable and a parent in the scoped Claude process registry.
pub fn verify_hook_peer(root: &Path, helper_pid: u32) -> Result<u32, &'static str> {
    let config = config(root)?;
    let scope = Scope::current().map_err(|_| UNVERIFIED_CALLER)?;
    scope
        .verify_process(helper_pid)
        .map_err(|_| UNVERIFIED_CALLER)?;
    let helper = process_info(helper_pid).map_err(|_| UNVERIFIED_CALLER)?;
    let own_image = std::env::current_exe().map_err(|_| UNVERIFIED_CALLER)?;
    if !same_image(&helper.image, &own_image) {
        return Err(UNVERIFIED_CALLER);
    }
    let mut child = helper_pid;
    let mut created = helper.creation_ticks;
    for depth in 0..=2 {
        let parent = parent_pid(child).map_err(|_| UNVERIFIED_CALLER)?;
        scope
            .verify_process(parent)
            .map_err(|_| UNVERIFIED_CALLER)?;
        let process = process_info(parent).map_err(|_| UNVERIFIED_CALLER)?;
        if process.creation_ticks >= created {
            break;
        }
        if same_image(&process.image, &config.process_image) {
            registry(&scope, &config, parent)?;
            return Ok(parent);
        }
        if depth == 2 || !config.native || !git_bash(&process.image) {
            break;
        }
        child = parent;
        created = process.creation_ticks;
    }
    Err(UNVERIFIED_CALLER)
}

fn git_bash(image: &Path) -> bool {
    std::env::var_os("ProgramFiles").is_some_and(|programs| {
        let git = PathBuf::from(programs).join("Git");
        ["bin/bash.exe", "usr/bin/bash.exe"]
            .iter()
            .any(|path| same_image(image, &git.join(path)))
    })
}

pub fn register_hook(
    root: &Path,
    now_ms: u64,
    caller_pid: u32,
    payload: &[u8],
) -> Result<Request, &'static str> {
    let native = claude::parse_hook(payload)?;
    let config = config(root)?;
    let scope = Scope::current().map_err(|_| UNVERIFIED_CALLER)?;
    let caller = registry(&scope, &config, caller_pid)?;
    if native.session_id != caller.session_id {
        return Err(UNVERIFIED_CALLER);
    }
    // Those two tools require an action-specific replacement for updatedInput.
    if matches!(
        native.tool_name.as_str(),
        "AskUserQuestion" | "ExitPlanMode"
    ) {
        return Err("This Claude permission request is handled in its own window.");
    }
    let request = Request {
        key: RequestKey {
            session: key(native.session_id),
            id: native.prompt_id,
        },
        summary: format!("Allow {}?", native.tool_name),
        workspace: windows::workspace_label(caller.cwd.as_deref()),
        detail: serde_json::to_string_pretty(&native.tool_input)
            .map_err(|_| "The Claude permission request could not be rendered.")?,
        decision_blocked: None,
        allow_for_session: false,
        can_open_session: registry_link(&config, &caller).is_some(),
    };
    let mut state = hooks()
        .lock()
        .map_err(|_| "The Claude hook state is unavailable.")?;
    if state
        .pending
        .iter()
        .any(|hook| hook.root == root && hook.request.key == request.key)
        || state
            .resolved
            .contains(&(root.to_path_buf(), request.key.clone()))
    {
        return Err("This Claude permission request was already registered.");
    }
    state.pending.push(PendingHook {
        root: root.to_path_buf(),
        request: request.clone(),
        caller: caller_pid,
        deadline: now_ms.saturating_add(HOOK_LIFETIME_MS),
        decision: None,
        taken: false,
    });
    Ok(request)
}

pub(super) fn session_link(
    root: &Path,
    request: &RequestKey,
    now_ms: u64,
) -> Result<SessionLink, &'static str> {
    let caller = hooks()
        .lock()
        .map_err(|_| NO_SESSION_LINK)?
        .pending
        .iter()
        .find(|hook| matching(hook, root, request) && hook.deadline > now_ms)
        .map(|hook| hook.caller)
        .ok_or(NO_SESSION_LINK)?;
    let config = config(root)?;
    let scope = Scope::current().map_err(|_| NO_SESSION_LINK)?;
    let owner = registry(&scope, &config, caller)?;
    if owner.session_id != request.session.id {
        return Err(NO_SESSION_LINK);
    }
    registry_link(&config, &owner).ok_or(NO_SESSION_LINK)
}

fn registry_link(config: &Config, owner: &Registry) -> Option<SessionLink> {
    let index = config.desktop_sessions.as_deref()?;
    let cwd = Path::new(owner.cwd.as_deref()?);
    desktop_session_link(index, &owner.session_id, cwd)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DesktopSession {
    session_id: String,
    cli_session_id: String,
    cwd: PathBuf,
    #[serde(default)]
    is_archived: bool,
}

fn desktop_session_link(index: &Path, cli_id: &str, cwd: &Path) -> Option<SessionLink> {
    if !windows::session_uuid(cli_id) || !cwd.is_absolute() {
        return None;
    }
    let mut matched = None;
    for org in fs::read_dir(index).ok()? {
        let org = org.ok()?.path();
        if !safe_dir(&org) {
            continue;
        }
        for account in fs::read_dir(org).ok()? {
            let account = account.ok()?.path();
            if !safe_dir(&account) {
                continue;
            }
            for entry in fs::read_dir(account).ok()? {
                let path = entry.ok()?.path();
                let Some(local_id) = record_id(&path).filter(|stem| {
                    stem.strip_prefix("local_")
                        .is_some_and(windows::session_uuid)
                }) else {
                    continue;
                };
                let Ok((file, info)) = windows::open_plain_file(&path) else {
                    continue;
                };
                if info.len() > DESKTOP_RECORD_LIMIT {
                    continue;
                }
                let mut bytes = Vec::new();
                if file
                    .take(DESKTOP_RECORD_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .is_err()
                    || bytes.len() as u64 != info.len()
                {
                    continue;
                }
                let Ok(record) = serde_json::from_slice::<DesktopSession>(&bytes) else {
                    continue;
                };
                if record.is_archived
                    || record.cli_session_id != cli_id
                    || record.session_id != local_id
                {
                    continue;
                }
                if !same_image(&record.cwd, cwd) || matched.is_some() {
                    return None;
                }
                matched = Some(SessionLink::Uri {
                    uri: format!("claude://claude.ai/epitaxy/{local_id}"),
                    error: "Windows could not open the Claude Desktop session.",
                });
            }
        }
    }
    matched
}

/// Stage one explicit decision. The card closes after the helper acknowledges it.
pub fn answer(
    root: &Path,
    now_ms: u64,
    target: &RequestKey,
    decision: Decision,
) -> Result<(), MutationError> {
    let mut state = hooks().lock().map_err(|_| MutationError::Blocked)?;
    let hook = state
        .pending
        .iter_mut()
        .find(|hook| matching(hook, root, target))
        .ok_or(MutationError::Blocked)?;
    if hook.deadline <= now_ms || hook.decision.is_some() || !live_hook(root, hook) {
        return Err(MutationError::Blocked);
    }
    hook.decision = Some(decision);
    Ok(())
}

pub fn take_hook_answer(root: &Path, now_ms: u64, request: &RequestKey) -> Option<Decision> {
    let mut state = hooks().lock().ok()?;
    let hook = state
        .pending
        .iter_mut()
        .find(|hook| matching(hook, root, request))?;
    if hook.deadline <= now_ms || hook.taken {
        return None;
    }
    let decision = hook.decision?;
    if !live_hook(root, hook) {
        return None;
    }
    hook.taken = true;
    Some(decision)
}

pub fn confirm_hook_answer(root: &Path, request: &RequestKey) -> bool {
    let Ok(mut state) = hooks().lock() else {
        return false;
    };
    let Some(index) = state
        .pending
        .iter()
        .position(|hook| matching(hook, root, request) && hook.taken)
    else {
        return false;
    };
    let hook = state.pending.remove(index);
    state.resolved.insert((hook.root, hook.request.key));
    true
}

pub fn cancel_hook(root: &Path, request: &RequestKey) {
    if let Ok(mut state) = hooks().lock()
        && let Some(index) = state
            .pending
            .iter()
            .position(|hook| matching(hook, root, request))
    {
        let hook = state.pending.remove(index);
        state.resolved.insert((hook.root, hook.request.key));
    }
}

fn matching(hook: &PendingHook, root: &Path, request: &RequestKey) -> bool {
    hook.root == root && hook.request.key == *request
}

fn live_hook(root: &Path, hook: &PendingHook) -> bool {
    let Ok(config) = config(root) else {
        return false;
    };
    let Ok(scope) = Scope::current() else {
        return false;
    };
    registry(&scope, &config, hook.caller)
        .is_ok_and(|entry| entry.session_id == hook.request.key.session.id)
}

pub fn send(
    root: &Path,
    now_ms: u64,
    target: &SessionKey,
    id: &str,
    text: &str,
) -> Result<Receipt, MutationError> {
    if target.client != Client::Claude
        || !claude::valid_id(&target.id)
        || !claude::valid_id(id)
        || !claude::valid_message(text)
    {
        return Err(MutationError::Blocked);
    }
    let feed = read(root, now_ms, None).map_err(|_| MutationError::Blocked)?;
    if !feed
        .sessions
        .iter()
        .any(|session| session.key == *target && session.send_blocked.is_none())
        || feed
            .receipts
            .iter()
            .any(|receipt| receipt.session == *target && receipt.id == id)
    {
        return Err(MutationError::Blocked);
    }
    let Some(expires_at) = now_ms.checked_add(claude::DELIVERY_LIFETIME_MS) else {
        return Err(MutationError::Blocked);
    };
    let folder = subdir(root, &["plugin", "pending", &target.id], true)
        .map_err(|_| MutationError::Blocked)?;
    subdir(root, &["plugin", "receipts", &target.id], true).map_err(|_| MutationError::Blocked)?;
    let message = Prompt {
        session_id: target.id.clone(),
        id: id.into(),
        text: text.into(),
        queued_at: now_ms,
        expires_at,
    };
    publish(&folder.join(format!("{id}.json")), &message)?;
    Ok(receipt(target.clone(), id.into(), DeliveryState::Queued))
}

fn collect_delivery(root: &Path, now_ms: u64, feed: &mut Feed) -> Result<(), &'static str> {
    if let Some(confirmed) = optional_subdir(root, &["plugin", "receipts"])? {
        for folder in entries(&confirmed).map_err(|_| BAD_FOLDER)? {
            let Some(session) = component(&folder) else {
                continue;
            };
            if !safe_dir(&folder) {
                continue;
            }
            for file in entries(&folder).map_err(|_| BAD_FOLDER)? {
                let Some(id) = record_id(&file) else { continue };
                let Ok(proof) = read_record(&file, claude::parse_receipt) else {
                    continue;
                };
                if proof.session_id != session || proof.id != id {
                    continue;
                }
                let state = match proof.state {
                    PluginState::Taken => DeliveryState::Received,
                    PluginState::Started => DeliveryState::Started,
                    PluginState::Refused => DeliveryState::Rejected,
                    PluginState::Expired => DeliveryState::Expired,
                };
                feed.receipts
                    .push(receipt(key(proof.session_id), proof.id, state));
            }
        }
    }
    collect_pending(root, now_ms, &mut feed.receipts)
}

fn collect_pending(
    root: &Path,
    now_ms: u64,
    receipts: &mut Vec<Receipt>,
) -> Result<(), &'static str> {
    let Some(pending) = optional_subdir(root, &["plugin", "pending"])? else {
        return Ok(());
    };
    for folder in entries(&pending).map_err(|_| BAD_FOLDER)? {
        let Some(session) = component(&folder) else {
            continue;
        };
        if !safe_dir(&folder) {
            continue;
        }
        for file in entries(&folder).map_err(|_| BAD_FOLDER)? {
            let Some(id) = record_id(&file) else {
                expire_stage(&file, now_ms).map_err(|_| BAD_FOLDER)?;
                continue;
            };
            let Ok(prompt) = read_record(&file, claude::parse_prompt) else {
                continue;
            };
            if prompt.session_id != session || prompt.id != id {
                continue;
            }
            if receipts
                .iter()
                .any(|proof| proof.session.id == session && proof.id == id)
            {
                remove_if_exists(&file).map_err(|_| BAD_FOLDER)?;
            } else if prompt.expires_at <= now_ms {
                match fs::remove_file(&file) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(BAD_FOLDER),
                }
                let folder =
                    subdir(root, &["plugin", "receipts", session], true).map_err(|_| BAD_FOLDER)?;
                let proof = PluginReceipt {
                    session_id: session.into(),
                    id: id.into(),
                    state: PluginState::Expired,
                };
                let file = folder.join(format!("{id}.json"));
                match publish(&file, &proof) {
                    Ok(()) => receipts.push(receipt(
                        key(prompt.session_id),
                        prompt.id,
                        DeliveryState::Expired,
                    )),
                    Err(_) if file.is_file() => {}
                    Err(_) => return Err(BAD_FOLDER),
                }
            } else if prompt.queued_at <= now_ms {
                // A restart can recover a file published before the resident
                // committed its enqueue acknowledgement.
                receipts.push(receipt(
                    key(prompt.session_id),
                    prompt.id,
                    DeliveryState::Queued,
                ));
            }
        }
    }
    Ok(())
}

fn key(id: String) -> SessionKey {
    SessionKey {
        client: Client::Claude,
        id,
    }
}

fn receipt(session: SessionKey, id: String, state: DeliveryState) -> Receipt {
    Receipt {
        external_id: Some(id.clone()),
        id,
        session,
        state,
    }
}

struct Config {
    process_image: PathBuf,
    sessions: PathBuf,
    workspace: Option<PathBuf>,
    native: bool,
    desktop_sessions: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveConfig {
    desktop_executable: PathBuf,
    engine_executable: PathBuf,
    registry_directory: PathBuf,
    workspace: Option<PathBuf>,
}

fn config(root: &Path) -> Result<Config, &'static str> {
    storage::verify_private(root).map_err(|_| BAD_FOLDER)?;
    let path = root.join("live.json");
    if !matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == io::ErrorKind::NotFound)
    {
        let live: LiveConfig = read_record(&path, |bytes| {
            serde_json::from_slice(bytes).map_err(|_| "The Claude live configuration is invalid.")
        })?;
        if super::windows::product_version(&live.desktop_executable).ok() != Some([5, 11492, 5, 0])
            || !live.engine_executable.is_absolute()
            || !live
                .engine_executable
                .components()
                .any(|part| part.as_os_str() == claude::ENGINE_VERSION)
            || !live.registry_directory.is_absolute()
            || !safe_dir(&live.registry_directory)
            || live
                .workspace
                .as_ref()
                .is_some_and(|path| !path.is_absolute() || !safe_dir(path))
        {
            return Err("The installed Claude Desktop Code channel could not be verified.");
        }
        return Ok(Config {
            process_image: live.engine_executable,
            sessions: live.registry_directory,
            workspace: live.workspace,
            native: true,
            desktop_sessions: desktop_index(&live.desktop_executable),
        });
    }
    if !cfg!(feature = "adapter-fixtures") {
        return Err("Claude Desktop Code has not been enabled.");
    }
    let fixture: ClientConfig = read_record(&root.join("client.json"), claude::parse_config)?;
    let process_image = PathBuf::from(fixture.process_image);
    if !process_image.is_absolute() {
        return Err("The Claude Code executable is unverified.");
    }
    Ok(Config {
        process_image,
        sessions: subdir(root, &["sessions"], false).map_err(|_| BAD_FOLDER)?,
        workspace: None,
        native: false,
        desktop_sessions: None,
    })
}

fn desktop_index(executable: &Path) -> Option<PathBuf> {
    let package = executable.parent()?.parent()?.file_name()?.to_str()?;
    if !package.eq_ignore_ascii_case("Claude_5.11492.5.0_x64__pzs8sxrjxfjjc") {
        return None;
    }
    let app_data = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    if !app_data.is_absolute() {
        return None;
    }
    subdir(
        &app_data,
        &[
            "Claude-Data",
            "claude_pzs8sxrjxfjjc",
            "claude-code-sessions",
        ],
        false,
    )
    .ok()
}

fn registry(scope: &Scope, config: &Config, pid: u32) -> Result<Registry, &'static str> {
    let entry = read_record(
        &config.sessions.join(format!("{pid}.json")),
        claude::parse_registry,
    )?;
    if entry.pid != pid
        || entry.entrypoint != "claude-desktop"
        || entry.kind != "interactive"
        || !local_pid_domain(
            entry.pid_domain.as_deref(),
            std::env::var("COMPUTERNAME").ok().as_deref(),
        )
        || config.workspace.as_ref().is_some_and(|workspace| {
            !entry
                .cwd
                .as_ref()
                .is_some_and(|cwd| same_image(Path::new(cwd), workspace))
        })
    {
        return Err(STALE_REGISTRY);
    }
    if entry.version != claude::ENGINE_VERSION {
        return Err(UNSUPPORTED_ENGINE);
    }
    scope.verify_process(pid).map_err(|_| STALE_REGISTRY)?;
    let observed = process_info(pid).map_err(|_| STALE_REGISTRY)?;
    if entry.proc_start != observed.creation_ticks.to_string()
        || !same_image(&observed.image, &config.process_image)
    {
        return Err(STALE_REGISTRY);
    }
    Ok(entry)
}

fn local_pid_domain(domain: Option<&str>, machine: Option<&str>) -> bool {
    match domain {
        None | Some("win32") => true,
        Some(domain) => domain.strip_prefix("win32:").is_some_and(|host| {
            !host.is_empty() && machine.is_some_and(|machine| host.eq_ignore_ascii_case(machine))
        }),
    }
}

#[cfg(test)]
#[test]
fn host_qualified_claude_pid_domains_stay_local() {
    assert!(local_pid_domain(Some("win32:host"), Some("HOST")));
    assert!(!local_pid_domain(Some("win32:other"), Some("HOST")));
    assert!(!local_pid_domain(Some("wsl:host"), Some("HOST")));
}

fn check_in(root: &Path, target: &SessionKey, now_ms: u64) -> Result<(), &'static str> {
    let folder = subdir(root, &["plugin", "check-ins"], false).map_err(|_| NO_CHANNEL)?;
    let check_in = read_record(
        &folder.join(format!("{}.json", target.id)),
        claude::parse_check_in,
    )
    .map_err(|_| NO_CHANNEL)?;
    // A heartbeat can be published after this scan sampled the local clock.
    if check_in.session_id != target.id
        || check_in.at > now_ms.saturating_add(1000)
        || now_ms.saturating_sub(check_in.at) >= claude::PLUGIN_FRESH_MS
    {
        return Err(NO_CHANNEL);
    }
    Ok(())
}

pub(super) fn same_image(actual: &Path, configured: &Path) -> bool {
    fs::canonicalize(actual)
        .ok()
        .zip(fs::canonicalize(configured).ok())
        .is_some_and(|(actual, configured)| actual == configured)
}

pub(super) struct NativeProcess {
    pub image: PathBuf,
    pub creation_ticks: u64,
}

pub(super) fn process_info(pid: u32) -> io::Result<NativeProcess> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let handle = Handle(handle);
    let mut path = vec![0u16; 32_768];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle.0, 0, path.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if exited.dwLowDateTime != 0 || exited.dwHighDateTime != 0 {
        return Err(io::ErrorKind::NotFound.into());
    }
    Ok(NativeProcess {
        image: OsString::from_wide(&path[..length as usize]).into(),
        creation_ticks: (u64::from(created.dwHighDateTime) << 32)
            | u64::from(created.dwLowDateTime),
    })
}

pub(super) fn parent_pid(pid: u32) -> io::Result<u32> {
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let handle = Handle(handle);
    let mut item = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut present = unsafe { Process32FirstW(handle.0, &mut item) };
    while present != 0 {
        if item.th32ProcessID == pid {
            return Ok(item.th32ParentProcessID);
        }
        present = unsafe { Process32NextW(handle.0, &mut item) };
    }
    Err(io::ErrorKind::NotFound.into())
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn read_record<T>(
    file: &Path,
    parse: impl FnOnce(&[u8]) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let metadata = fs::symlink_metadata(file).map_err(|_| "The Claude record is unavailable.")?;
    if !metadata.is_file() || reparse(&metadata) {
        return Err("The Claude record is not a plain file.");
    }
    let mut bytes = Vec::new();
    File::open(file)
        .and_then(|file| {
            file.take(claude::RECORD_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| "The Claude record is unavailable.")?;
    parse(&bytes)
}

fn reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn safe_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir() && !reparse(&metadata))
}

fn subdir(root: &Path, names: &[&str], create: bool) -> io::Result<PathBuf> {
    if !safe_dir(root) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut path = root.to_path_buf();
    for name in names {
        path.push(name);
        if create {
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        let info = fs::symlink_metadata(&path)?;
        if !info.is_dir() || reparse(&info) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    Ok(path)
}

fn optional_subdir(root: &Path, names: &[&str]) -> Result<Option<PathBuf>, &'static str> {
    match subdir(root, names, false) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(BAD_FOLDER),
    }
}

fn entries(folder: &Path) -> io::Result<Vec<PathBuf>> {
    fs::read_dir(folder)?
        .map(|result| result.map(|entry| entry.path()))
        .collect()
}

fn component(path: &Path) -> Option<&str> {
    path.file_name()?
        .to_str()
        .filter(|name| claude::valid_id(name))
}

fn record_id(path: &Path) -> Option<&str> {
    (path.extension()?.to_str()? == "json").then_some(())?;
    path.file_stem()?.to_str().filter(|id| claude::valid_id(id))
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn publish(path: &Path, value: &impl Serialize) -> Result<(), MutationError> {
    let bytes = serde_json::to_vec(value).map_err(|_| MutationError::Blocked)?;
    storage::publish(path, &bytes, false).map_err(|_| MutationError::Blocked)
}

fn expire_stage(path: &Path, now_ms: u64) -> io::Result<()> {
    let Some((pid, seq)) = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| {
            name.strip_prefix(".rooom-")?
                .strip_suffix(".tmp")?
                .split_once('-')
        })
    else {
        return Ok(());
    };
    if pid.parse::<u32>().is_err() || seq.parse::<u64>().is_err() {
        return Ok(());
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let expired = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .is_some_and(|age| {
            u128::from(now_ms) >= age.as_millis() + u128::from(claude::DELIVERY_LIFETIME_MS)
        });
    if expired && metadata.is_file() && !reparse(&metadata) {
        remove_if_exists(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub(super) struct Fixture(pub(super) PathBuf);

    impl Fixture {
        pub(super) fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "rooom-m3-claude-20261115-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            Self(directory)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn desktop_link_requires_one_active_record_for_the_registry_session_and_workspace() {
        let fixture = Fixture::new();
        let index = fixture.0.join("index");
        let account = index.join("org/account");
        let cwd = fixture.0.join("target");
        let other_cwd = fixture.0.join("other");
        fs::create_dir_all(&account).unwrap();
        fs::create_dir(&cwd).unwrap();
        fs::create_dir(&other_cwd).unwrap();

        let cli = "01234567-89ab-4cde-8f01-23456789abcd";
        let local = "local_12345678-9abc-4def-8012-3456789abcde";
        let second = "local_23456789-abcd-4ef0-8123-456789abcdef";
        let write = |stem: &str, id: &str, workspace: &Path, archived: bool| {
            fs::write(
                account.join(format!("{stem}.json")),
                serde_json::to_vec(&json!({
                    "sessionId": id, "cliSessionId": cli, "cwd": workspace,
                    "isArchived": archived,
                }))
                .unwrap(),
            )
            .unwrap();
        };
        write(local, second, &cwd, false);
        assert!(desktop_session_link(&index, cli, &cwd).is_none());
        write(local, local, &other_cwd, false);
        assert!(desktop_session_link(&index, cli, &cwd).is_none());
        write(local, local, &cwd, true);
        assert!(desktop_session_link(&index, cli, &cwd).is_none());
        write(local, local, &cwd, false);
        let link = desktop_session_link(&index, cli, &cwd).unwrap();
        assert!(
            matches!(link, SessionLink::Uri { uri, .. } if uri == format!("claude://claude.ai/epitaxy/{local}"))
        );
        assert!(desktop_session_link(&index, &format!("{cli}?other"), &cwd).is_none());
        write(second, second, &cwd, false);
        assert!(desktop_session_link(&index, cli, &cwd).is_none());
    }

    #[test]
    fn a_reused_registry_pid_does_not_discover_an_active_session() {
        let fixture = Fixture::new();
        let root = &fixture.0;
        let scope = Scope::current().unwrap();
        let pid = std::process::id();
        let process = process_info(pid).unwrap();
        let config = Config {
            process_image: process.image,
            sessions: subdir(root, &["sessions"], true).unwrap(),
            workspace: None,
            native: true,
            desktop_sessions: None,
        };
        let path = config.sessions.join(format!("{pid}.json"));
        let mut native = json!({
            "sessionId": "session-1", "kind": "interactive", "pid": pid,
            "entrypoint": "claude-desktop", "status": "waiting",
            "procStart": (process.creation_ticks - 1).to_string(), "version": claude::ENGINE_VERSION,
        });
        fs::write(&path, serde_json::to_vec(&native).unwrap()).unwrap();
        assert_eq!(registry(&scope, &config, pid).unwrap_err(), STALE_REGISTRY);
        native["procStart"] = json!(process.creation_ticks.to_string());
        fs::write(&path, serde_json::to_vec(&native).unwrap()).unwrap();
        assert_eq!(
            registry(&scope, &config, pid).unwrap().session_id,
            "session-1"
        );
    }

    #[test]
    fn pending_text_is_deleted_after_plugin_proof_or_expiry() {
        let fixture = Fixture::new();
        let root = &fixture.0;
        let session = "session-1";
        let pending = subdir(root, &["plugin", "pending", session], true).unwrap();
        let receipt_dir = subdir(root, &["plugin", "receipts", session], true).unwrap();
        let deadline = 100 + claude::DELIVERY_LIFETIME_MS;
        for id in ["will-expire", "will-be-taken"] {
            publish(
                &pending.join(format!("{id}.json")),
                &Prompt {
                    session_id: session.into(),
                    id: id.into(),
                    text: "synthetic prompt".into(),
                    queued_at: 100,
                    expires_at: deadline,
                },
            )
            .unwrap();
        }
        publish(
            &receipt_dir.join("will-be-taken.json"),
            &PluginReceipt {
                session_id: session.into(),
                id: "will-be-taken".into(),
                state: PluginState::Taken,
            },
        )
        .unwrap();
        let mut feed = Feed::default();
        collect_delivery(root, deadline - 1, &mut feed).unwrap();
        assert_eq!(feed.receipts.len(), 2);
        assert!(
            feed.receipts
                .iter()
                .any(|proof| proof.id == "will-be-taken" && proof.state == DeliveryState::Received)
        );
        assert!(
            feed.receipts
                .iter()
                .any(|proof| proof.id == "will-expire" && proof.state == DeliveryState::Queued)
        );
        assert!(!pending.join("will-be-taken.json").exists());
        assert!(pending.join("will-expire.json").is_file());

        collect_delivery(root, deadline, &mut Feed::default()).unwrap();
        assert!(!pending.join("will-expire.json").exists());
        let mut feed = Feed::default();
        collect_delivery(root, deadline, &mut feed).unwrap();
        assert!(
            feed.receipts
                .iter()
                .any(|proof| proof.id == "will-expire" && proof.state == DeliveryState::Expired)
        );
        assert!(
            !fs::read_to_string(receipt_dir.join("will-expire.json"))
                .unwrap()
                .contains("synthetic prompt")
        );
    }
}
