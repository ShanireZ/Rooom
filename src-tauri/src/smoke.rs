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

pub fn full() -> bool {
    std::env::var_os("ROOOM_SMOKE_FULL").is_some()
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
        let actions: &[_] = if full() {
            if profile.join("conflict").is_file() {
                &["open", "quit"]
            } else {
                &["shortcut", "pin", "unpin", "focus", "close", "quit"]
            }
        } else {
            &["open", "close", "quit"]
        };
        for &action in actions {
            let limit = Instant::now() + Duration::from_secs(60);
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
) -> Result<(), String> {
    let snapshot = get_snapshot(app.state::<Resident>())?;
    let expected_surface = match snapshot.surface {
        Surface::Strip => "strip",
        Surface::Panel => "panel",
    };
    if revision != snapshot.revision || surface != expected_surface || !content_visible {
        return Err("The WebView has not rendered the current snapshot.".into());
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
    let mut affinity = 0;
    if unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) } == 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    let receipt = serde_json::json!({
        "snapshot": snapshot,
        "hwnd": hwnd as usize,
        "affinity": affinity,
        "tray": app.tray_by_id(shell::TRAY).is_some(),
        "scaleFactor": window.scale_factor().map_err(|error| error.to_string())?,
        "warningVisible": warning_visible,
        "foreground": unsafe { GetForegroundWindow() } as usize,
        "shortcutRegistered": snapshot.settings.shortcut.as_deref()
            .is_some_and(|shortcut| app.global_shortcut().is_registered(shortcut)),
    });
    let bytes = serde_json::to_vec(&receipt).map_err(|error| error.to_string())?;
    let temporary = profile.0.join(format!("{revision}.tmp"));
    fs::write(&temporary, bytes)
        .and_then(|_| fs::rename(&temporary, profile.0.join(format!("{revision}.json"))))
        .map_err(|error| error.to_string())
}
