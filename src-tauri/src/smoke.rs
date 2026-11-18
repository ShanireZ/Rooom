use crate::{Resident, get_snapshot, shell};
use rooom_core::Surface;
use std::{
    fs, io,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use tauri::{
    App, AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
    WebviewWindowBuilder, Wry, webview::PageLoadEvent,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowDisplayAffinity};

pub struct Profile(pub PathBuf);

#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusProof {
    owned_foreground_observations: u32,
    focusable_observations: u32,
    samples: Vec<FocusSample>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FocusSample {
    phase: &'static str,
    foreground: usize,
    no_activate: bool,
}

pub fn trace_focus(window: &WebviewWindow, phase: &'static str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, WS_EX_NOACTIVATE,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let state = window.state::<std::sync::Mutex<FocusProof>>();
    let mut proof = state.lock().unwrap();
    let foreground = unsafe { GetForegroundWindow() } as usize;
    let no_activate =
        unsafe { GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE) } as u32 & WS_EX_NOACTIVATE != 0;
    proof.owned_foreground_observations += u32::from(foreground == hwnd.0 as usize);
    proof.focusable_observations += u32::from(!no_activate);
    if proof.samples.len() < 32 {
        proof.samples.push(FocusSample {
            phase,
            foreground,
            no_activate,
        });
    }
}

pub fn full() -> bool {
    std::env::var_os("ROOOM_SMOKE_FULL").is_some()
}

#[cfg(feature = "first-batch-adapters")]
pub fn live() -> bool {
    std::env::var_os("ROOOM_SMOKE_LIVE").is_some()
}

pub fn cursor(
    app: &AppHandle,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> PhysicalPosition<f64> {
    if app.state::<Profile>().0.join("pointer-near").is_file() {
        PhysicalPosition::new(
            f64::from(position.x) + f64::from(size.width) / 2.0,
            f64::from(position.y) + f64::from(size.height) / 2.0,
        )
    } else {
        PhysicalPosition::new(f64::NEG_INFINITY, f64::NEG_INFINITY)
    }
}

pub fn profile() -> io::Result<PathBuf> {
    let profile = std::env::var_os("ROOOM_SMOKE_ROOT")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("The smoke build needs a disposable ROOOM_SMOKE_ROOT"))?;
    if !profile.is_absolute() || !profile.is_dir() {
        return Err(io::Error::other(
            "The smoke profile must be an existing absolute directory",
        ));
    }
    Ok(profile)
}

pub fn observe_webview<'a>(
    builder: WebviewWindowBuilder<'a, Wry, App>,
    app: &App,
    session: u32,
) -> WebviewWindowBuilder<'a, Wry, App> {
    app.manage(std::sync::Mutex::new(FocusProof::default()));
    builder
        .data_directory(shell::webview_data_dir(&app.state::<Profile>().0, session))
        .on_page_load(|window, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = window.eval(include_str!("smoke_observer.js"));
            }
        })
}

pub fn start(app: AppHandle, profile: PathBuf) {
    thread::spawn(move || {
        let actions: &[_] = if cfg!(feature = "first-batch-adapters") {
            &["quit"]
        } else if full() {
            if profile.join("conflict").is_file() {
                &["open", "quit"]
            } else {
                &["shortcut", "pin", "unpin", "focus", "close", "quit"]
            }
        } else {
            &["open", "close", "quit"]
        };
        for &action in actions {
            #[cfg(feature = "first-batch-adapters")]
            let seconds = if live() { 900 } else { 60 };
            #[cfg(not(feature = "first-batch-adapters"))]
            let seconds = 60;
            let limit = Instant::now() + Duration::from_secs(seconds);
            while !profile.join(format!("{action}.request")).is_file() {
                if Instant::now() >= limit {
                    eprintln!("Rooom smoke timed out waiting for {action}.");
                    app.exit(1);
                    return;
                }
                thread::sleep(Duration::from_millis(40));
            }
            let handle = app.clone();
            if app
                .run_on_main_thread(move || {
                    let result = match action {
                        "close" => shell::set_surface(&handle, Surface::Strip),
                        "shortcut" => shell::shortcut_pressed(&handle),
                        "pin" | "unpin" => shell::toggle_pin(&handle),
                        "focus" => shell::focus_panel(&handle),
                        _ => shell::tray_action(&handle, action),
                    };
                    if let Err(error) = result {
                        eprintln!("Rooom smoke could not run {action}: {error}");
                        handle.exit(1);
                    }
                })
                .is_err()
            {
                return;
            }
        }
    });
}

#[tauri::command]
pub fn smoke_rendered(
    window: WebviewWindow,
    app: AppHandle,
    profile: State<'_, Profile>,
    revision: u32,
    surface: String,
    content_visible: bool,
    warning_visible: bool,
    approval_confirmed: bool,
    mut inbox_view: serde_json::Value,
) -> Result<(), String> {
    let snapshot = get_snapshot(app.state::<Resident>())?;
    let expected_surface = match snapshot.surface {
        Surface::Strip => "strip",
        Surface::Panel => "panel",
    };
    if revision != snapshot.revision || surface != expected_surface || !content_visible {
        return Err("The WebView has not rendered the current snapshot.".into());
    }
    let expected_view = if snapshot.surface == Surface::Panel {
        use rooom_core::inbox::SessionKey;
        let key = |value: &SessionKey| format!("{}/{}", value.client.slug(), value.id);
        let inbox = &snapshot.inbox;
        serde_json::json!({
            "sessions": inbox.sessions.iter().map(|s| key(&s.key)).collect::<Vec<_>>(),
            "requests": inbox.requests.iter().map(|r| (key(&r.key.session), &r.key.id)).collect::<Vec<_>>(),
            "requestWorkspaces": inbox.requests.iter().filter_map(|r| {
                r.workspace.as_ref().map(|workspace| (key(&r.key.session), &r.key.id, workspace))
            }).collect::<Vec<_>>(),
            "sessionGrants": inbox.requests.iter().filter(|r| r.allow_for_session).map(|r| (key(&r.key.session), &r.key.id)).collect::<Vec<_>>(),
            "deliveries": inbox.deliveries.iter().map(|r| (key(&r.session), &r.id, r.state)).collect::<Vec<_>>(),
            "timeline": inbox.timeline.iter().map(|e| (key(&e.session), &e.id)).collect::<Vec<_>>(),
            "blocked": inbox.adapters.iter().filter(|a| a.reason.is_some()).map(|a| a.client).collect::<Vec<_>>(),
        })
    } else {
        serde_json::Value::Null
    };
    if inbox_view != expected_view {
        return Err("The rendered inbox does not match the Rust snapshot.".into());
    }
    if let Some(view) = inbox_view.as_object_mut() {
        view.remove("requestWorkspaces");
        let cards = snapshot
            .inbox
            .requests
            .iter()
            .filter(|r| r.workspace.is_some())
            .map(|r| format!("{}/{}", r.key.session.client.slug(), r.key.session.id))
            .collect::<Vec<_>>();
        view.insert("requestWorkspaceCards".into(), serde_json::json!(cards));
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    trace_focus(&window, "render");
    let focus_proof = app
        .state::<std::sync::Mutex<FocusProof>>()
        .lock()
        .unwrap()
        .clone();
    let mut affinity = 0;
    if unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) } == 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    let mut projection = serde_json::to_value(snapshot.clone()).map_err(|_| "Invalid snapshot.")?;
    // The observation artifact carries identifiers and states only.
    projection.as_object_mut().unwrap().remove("inbox");
    let receipt = serde_json::json!({
        "snapshot": projection,
        "inboxView": inbox_view,
        "adapterIssues": snapshot.inbox.adapters.iter().filter_map(|a| {
            a.reason.as_ref().map(|reason| (a.client, reason))
        }).collect::<Vec<_>>(),
        "hwnd": hwnd as usize,
        "affinity": affinity,
        "tray": app.tray_by_id(shell::TRAY).is_some(),
        "scaleFactor": window.scale_factor().map_err(|error| error.to_string())?,
        "warningVisible": warning_visible,
        "approvalConfirmed": approval_confirmed,
        "foreground": unsafe { GetForegroundWindow() } as usize,
        "focusProof": focus_proof,
        "shortcutRegistered": snapshot.settings.shortcut.as_deref()
            .is_some_and(|shortcut| app.global_shortcut().is_registered(shortcut)),
    });
    let bytes = serde_json::to_vec(&receipt).map_err(|error| error.to_string())?;
    let temporary = profile.0.join(format!("{revision}.tmp"));
    fs::write(&temporary, bytes)
        .and_then(|_| fs::rename(&temporary, profile.0.join(format!("{revision}.json"))))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn smoke_next_action(app: AppHandle) -> Option<serde_json::Value> {
    if !cfg!(feature = "first-batch-adapters") {
        return None;
    }
    let profile = app.state::<Profile>().inner().0.clone();
    let file = profile.join("view-action.json");
    let bytes = fs::read(&file).ok()?;
    fs::remove_file(file).ok()?;
    let action: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if action.get("probe") == Some(&serde_json::Value::Bool(true)) {
        let targets =
            serde_json::from_value::<Vec<rooom_core::inbox::SessionKey>>(action["targets"].clone())
                .ok()?;
        let text = action["text"].as_str()?;
        let mut blocked = Vec::new();
        for target in targets {
            let client = target.client.slug();
            if crate::send_message(app.clone(), target, text.into())
                .await
                .is_err()
            {
                blocked.push(client);
            }
        }
        let _ = fs::write(
            profile.join("blocked-probe.tmp"),
            serde_json::to_vec(&blocked).ok()?,
        )
        .and_then(|_| {
            fs::rename(
                profile.join("blocked-probe.tmp"),
                profile.join("blocked-probe.json"),
            )
        });
        None
    } else {
        Some(action)
    }
}
