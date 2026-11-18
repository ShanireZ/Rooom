use crate::{PanelPolicy, Resident, get_snapshot};
use rooom_core::{Edge, Settings, ShortcutIssue, Surface};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{
    App, AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub(crate) const SHELL: &str = "shell";
pub const TRAY: &str = "rooom";
const OPEN: &str = "open";
const QUIT: &str = "quit";

pub fn install(app: &App, session: u32) -> Result<(), Box<dyn std::error::Error>> {
    let builder = WebviewWindowBuilder::new(app, SHELL, WebviewUrl::App("index.html".into()))
        .title("Rooom")
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false);
    #[cfg(feature = "first-batch-smoke")]
    let builder = crate::smoke::observe_webview(builder, app, session);
    #[cfg(not(feature = "first-batch-smoke"))]
    let builder =
        builder.data_directory(webview_data_dir(&app.path().app_local_data_dir()?, session));
    let window = builder.build()?;
    #[cfg(feature = "first-batch-smoke")]
    crate::smoke::trace_focus(&window, "built");
    let settings = get_snapshot(app.state::<Resident>())?.settings;
    window.set_content_protected(settings.capture_exclusion)?;
    place(&window, &settings, Surface::Strip)?;
    let handle = app.handle().clone();
    window.on_window_event(move |event| {
        #[cfg(feature = "first-batch-smoke")]
        if let WindowEvent::Focused(focused) = event {
            if let Some(window) = handle.get_webview_window(SHELL) {
                crate::smoke::trace_focus(&window, if *focused { "focus-in" } else { "focus-out" });
            }
        }
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Err(error) = set_surface(&handle, Surface::Strip) {
                eprintln!("Rooom could not close its panel: {error}");
            }
        }
    });

    let open = MenuItem::with_id(app, OPEN, "Open Rooom", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = app
        .default_window_icon()
        .ok_or("Missing Rooom icon")?
        .clone();
    TrayIconBuilder::with_id(TRAY)
        .icon(icon)
        .tooltip("Rooom")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if let Err(error) = tray_action(app, event.id().as_ref()) {
                eprintln!("Rooom could not handle the tray command: {error}");
            }
        })
        .build(app)?;
    #[cfg(feature = "first-batch-smoke")]
    crate::smoke::trace_focus(&window, "before-show");
    window.show()?;
    #[cfg(feature = "first-batch-smoke")]
    crate::smoke::trace_focus(&window, "after-show");
    Ok(())
}

pub fn webview_data_dir(root: &std::path::Path, session: u32) -> std::path::PathBuf {
    root.join("webview").join(session.to_string())
}

#[cfg(test)]
#[test]
fn same_accounts_webviews_have_distinct_directories_across_sessions() {
    let root = std::path::Path::new("Rooom");
    assert_eq!(webview_data_dir(root, 1), root.join("webview").join("1"));
    assert_ne!(webview_data_dir(root, 1), webview_data_dir(root, 2));
}

pub fn tray_action(app: &AppHandle, id: &str) -> Result<(), String> {
    match id {
        OPEN => set_surface(app, Surface::Panel),
        QUIT => {
            app.exit(0);
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn set_surface(app: &AppHandle, surface: Surface) -> Result<(), String> {
    let resident = app.state::<Resident>();
    let mut snapshot = resident
        .0
        .lock()
        .map_err(|_| "The resident state is unavailable.")?;
    if snapshot.surface == surface {
        drop(snapshot);
        if surface == Surface::Panel {
            app.state::<PanelPolicy>()
                .0
                .lock()
                .map_err(|error| error.to_string())?
                .surface_changed(surface);
        }
        return Ok(());
    }
    let window = app
        .get_webview_window(SHELL)
        .ok_or("Missing Rooom window")?;
    #[cfg(feature = "first-batch-smoke")]
    crate::smoke::trace_focus(&window, "before-reveal");
    place(&window, &snapshot.settings, surface)?;
    if surface == Surface::Strip {
        window
            .set_focusable(false)
            .map_err(|error| error.to_string())?;
        snapshot.pinned = false;
    }
    #[cfg(feature = "first-batch-smoke")]
    crate::smoke::trace_focus(&window, "after-reveal");
    snapshot.set_surface(surface);
    let value = snapshot.clone();
    drop(snapshot);
    app.state::<PanelPolicy>()
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .surface_changed(surface);
    app.emit_to(SHELL, "rooom:snapshot", value)
        .map_err(|error| error.to_string())
}

pub fn focus_panel(app: &AppHandle) -> Result<(), String> {
    if get_snapshot(app.state::<Resident>())?.surface == Surface::Panel {
        let window = app
            .get_webview_window(SHELL)
            .ok_or("Missing Rooom window")?;
        window
            .set_focusable(true)
            .map_err(|error| error.to_string())?;
        #[cfg(feature = "first-batch-smoke")]
        crate::smoke::trace_focus(&window, "explicit-focus");
        let hwnd = window.hwnd().map_err(|error| error.to_string())?.0;
        // Tao's forced-focus fallback synthesizes Alt. Let Windows decide
        // whether this trusted interaction may activate the panel.
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, SetForegroundWindow,
        };
        if unsafe { GetForegroundWindow() != hwnd && SetForegroundWindow(hwnd) == 0 } {
            return Err("Windows did not grant the panel foreground focus.".into());
        }
    }
    Ok(())
}

pub fn toggle_pin(app: &AppHandle) -> Result<(), String> {
    let resident = app.state::<Resident>();
    let mut snapshot = resident.0.lock().map_err(|error| error.to_string())?;
    if snapshot.surface != Surface::Panel {
        return Ok(());
    }
    snapshot.toggle_pin();
    let value = snapshot.clone();
    drop(snapshot);
    app.state::<PanelPolicy>()
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .surface_changed(Surface::Panel);
    app.emit_to(SHELL, "rooom:snapshot", value)
        .map_err(|error| error.to_string())
}

pub fn register_shortcut(app: &AppHandle, configured: Option<&str>) -> Option<ShortcutIssue> {
    let configured = configured?;
    let Ok(shortcut) = configured.parse::<Shortcut>() else {
        return Some(ShortcutIssue::Invalid);
    };
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Err(error) = shortcut_pressed(&handle) {
                        eprintln!("Rooom could not open from its shortcut: {error}");
                    }
                });
            }
        })
        .err()
        .map(|_| ShortcutIssue::Unavailable)
}

pub fn shortcut_pressed(app: &AppHandle) -> Result<(), String> {
    set_surface(app, Surface::Panel)
}

#[derive(Clone, Default)]
pub struct PointerWatch(Arc<AtomicBool>);

impl PointerWatch {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub fn watch_pointer(app: AppHandle) -> PointerWatch {
    let watch = PointerWatch::default();
    let stopped = watch.0.clone();
    thread::spawn(move || {
        let pending = Arc::new(AtomicBool::new(false));
        while !stopped.load(Ordering::Relaxed) {
            if !pending.swap(true, Ordering::AcqRel) {
                let (handle, pending) = (app.clone(), pending.clone());
                if app
                    .run_on_main_thread(move || {
                        let _ = observe_pointer(&handle);
                        pending.store(false, Ordering::Release);
                    })
                    .is_err()
                {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(80));
        }
    });
    watch
}

fn observe_pointer(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(SHELL)
        .ok_or("Missing Rooom window")?;
    let snapshot = get_snapshot(app.state::<Resident>())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let margin = if snapshot.surface == Surface::Strip {
        8.0 * window.scale_factor().map_err(|error| error.to_string())?
    } else {
        0.0
    };
    #[cfg(feature = "first-batch-smoke")]
    let cursor = crate::smoke::cursor(app, position, size);
    #[cfg(not(feature = "first-batch-smoke"))]
    let cursor = window
        .cursor_position()
        .map_err(|error| error.to_string())?;
    let x = cursor.x - f64::from(position.x);
    let y = cursor.y - f64::from(position.y);
    let near = x >= -margin
        && y >= -margin
        && x < f64::from(size.width) + margin
        && y < f64::from(size.height) + margin;
    // A focused WebView child and the outer HWND belong to the same foreground window.
    let foreground = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    let focused = foreground == window.hwnd().map_err(|error| error.to_string())?.0;
    let next = app
        .state::<PanelPolicy>()
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .observe(
            snapshot.surface,
            near,
            snapshot.pinned || focused,
            Instant::now(),
        );
    if let Some(surface) = next {
        set_surface(app, surface)?;
    }
    Ok(())
}

fn choose_monitor(window: &WebviewWindow, display: Option<&str>) -> Result<Monitor, String> {
    if let Some(display) = display {
        let monitors = window
            .available_monitors()
            .map_err(|error| error.to_string())?;
        if let Some(monitor) = monitors
            .into_iter()
            .find(|monitor| monitor.name().is_some_and(|name| name == display))
        {
            return Ok(monitor);
        }
    }
    window
        .primary_monitor()
        .map_err(|error| error.to_string())?
        .ok_or("No display is available.".into())
}

fn place(window: &WebviewWindow, settings: &Settings, surface: Surface) -> Result<(), String> {
    let monitor = choose_monitor(window, settings.display.as_deref())?;
    let work = monitor.work_area();
    let (width, height) = match (surface, settings.edge) {
        (Surface::Panel, _) => (380, 210),
        (Surface::Strip, Edge::Left | Edge::Right) => (14, 72),
        (Surface::Strip, Edge::Top | Edge::Bottom) => (72, 14),
    };
    let scale = monitor.scale_factor();
    let width = ((f64::from(width) * scale).round() as u32).min(work.size.width);
    let height = ((f64::from(height) * scale).round() as u32).min(work.size.height);
    let spare_x = (work.size.width - width) as i32;
    let spare_y = (work.size.height - height) as i32;
    let (dx, dy) = match settings.edge {
        Edge::Left => (0, spare_y / 2),
        Edge::Right => (spare_x, spare_y / 2),
        Edge::Top => (spare_x / 2, 0),
        Edge::Bottom => (spare_x / 2, spare_y),
    };
    window
        .set_size(PhysicalSize::new(width, height))
        .and_then(|_| {
            window.set_position(PhysicalPosition::new(
                work.position.x + dx,
                work.position.y + dy,
            ))
        })
        .map_err(|error| error.to_string())
}
