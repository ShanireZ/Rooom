#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings_file;
mod shell;
#[cfg(feature = "first-batch-smoke")]
mod smoke;

use rooom_core::{PanelTimer, Snapshot};
use rooom_platform::pipe::{self, Action, Scope, Startup};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

struct Resident(Mutex<Snapshot>);
#[derive(Default)]
struct PanelPolicy(Mutex<PanelTimer>);
struct PipeSlot(Mutex<Option<pipe::Listener>>);

#[tauri::command]
fn get_snapshot(resident: State<'_, Resident>) -> Result<Snapshot, String> {
    resident
        .0
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "The resident state is unavailable.".into())
}

#[tauri::command]
fn open_panel(app: AppHandle) -> Result<(), String> {
    shell::set_surface(&app, rooom_core::Surface::Panel)
}

#[tauri::command]
fn close_panel(app: AppHandle) -> Result<(), String> {
    shell::set_surface(&app, rooom_core::Surface::Strip)
}

#[tauri::command]
fn focus_panel(app: AppHandle) -> Result<(), String> {
    shell::focus_panel(&app)
}

#[tauri::command]
fn toggle_pin(app: AppHandle) -> Result<(), String> {
    shell::toggle_pin(&app)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Rooom could not start: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let scope = Scope::current()?;
    #[cfg(feature = "first-batch-smoke")]
    let scope = scope.fixture(&std::env::var("ROOOM_SMOKE_ID")?)?;
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--health")
    {
        let receipt = scope.request(Action::Health)?;
        println!(
            "{}",
            serde_json::json!({ "pid": receipt.pid, "session": receipt.session })
        );
        return Ok(());
    }
    let Startup::Primary(pipe) = pipe::start_or_activate(&scope)? else {
        return Ok(());
    };
    let session = scope.session();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            #[cfg(feature = "first-batch-smoke")]
            let profile = {
                let profile = smoke::profile()?;
                app.manage(smoke::Profile(profile.clone()));
                profile
            };
            #[cfg(not(feature = "first-batch-smoke"))]
            let profile = app.path().app_config_dir()?;

            let (settings, issue) =
                settings_file::SettingsFile::new(profile.join("settings.json")).load();
            let mut snapshot = Snapshot::new(settings, issue);
            snapshot.shortcut_issue =
                shell::register_shortcut(app.handle(), snapshot.settings.shortcut.as_deref());
            app.manage(Resident(Mutex::new(snapshot)));
            app.manage(PanelPolicy::default());
            shell::install(app, session)?;
            let handle = app.handle().clone();
            let failure_handle = handle.clone();
            let listener = pipe.serve(
                move || {
                    let (done, reply) = pipe::activation_reply();
                    let target = handle.clone();
                    handle
                        .run_on_main_thread(move || {
                            if !done.is_closed()
                                && shell::set_surface(&target, rooom_core::Surface::Panel).is_ok()
                            {
                                let _ = done.send(());
                            }
                        })
                        .ok()?;
                    Some(reply)
                },
                move |error| {
                    eprintln!("Rooom lost its resident pipe: {error}");
                    failure_handle.exit(1);
                },
            )?;
            app.manage(PipeSlot(Mutex::new(Some(listener))));
            #[cfg(feature = "first-batch-smoke")]
            smoke::start(app.handle().clone(), profile);
            #[cfg(feature = "first-batch-smoke")]
            let watch = smoke::full();
            #[cfg(not(feature = "first-batch-smoke"))]
            let watch = true;
            if watch {
                app.manage(shell::watch_pointer(app.handle().clone()));
            }
            Ok(())
        });

    #[cfg(feature = "first-batch-smoke")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_snapshot,
        open_panel,
        close_panel,
        focus_panel,
        toggle_pin,
        smoke::smoke_rendered,
    ]);
    #[cfg(not(feature = "first-batch-smoke"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_snapshot,
        open_panel,
        close_panel,
        focus_panel,
        toggle_pin
    ]);

    builder
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(watch) = app.try_state::<shell::PointerWatch>() {
                    watch.stop();
                }
                if let Some(slot) = app.try_state::<PipeSlot>() {
                    slot.0.lock().unwrap().take();
                }
            }
        });
    Ok(())
}
