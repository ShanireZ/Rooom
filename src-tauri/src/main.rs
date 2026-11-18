#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adapter_batch;
mod settings_file;
mod shell;
#[cfg(feature = "first-batch-smoke")]
mod smoke;

use rooom_core::inbox::{Decision, Request, RequestKey, SessionKey};
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

#[tauri::command]
async fn send_message(app: AppHandle, target: SessionKey, text: String) -> Result<String, String> {
    if text.trim().is_empty() || text.len() > 16_384 || text.contains('\0') {
        return Err("Enter a message of up to 16 KiB.".into());
    }
    adapter_batch::send(app, target, text).await
}

#[tauri::command]
async fn answer_request(
    app: AppHandle,
    request: Request,
    decision: Decision,
) -> Result<(), String> {
    adapter_batch::answer(app, request, decision).await
}

#[tauri::command]
async fn open_request_session(app: AppHandle, request: RequestKey) -> Result<(), String> {
    adapter_batch::open_request_session(app, request).await
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Rooom could not start: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arg = std::env::args_os().nth(1);
    #[cfg(feature = "first-batch-adapters")]
    if arg.as_deref().is_some_and(|arg| arg == "--fixture-codex") {
        rooom_platform::adapters::codex::fixture_cli(std::env::args_os().skip(2))?;
        return Ok(());
    }
    if arg.as_deref().is_some_and(|arg| arg == "--hook") {
        run_hook();
        return Ok(());
    }
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
            #[cfg(not(feature = "first-batch-smoke"))]
            match adapter_batch::Batch::new(&profile, &scope, false) {
                Ok(batch) => {
                    app.manage(batch);
                }
                Err(_) => eprintln!("Rooom could not open its private adapter folder."),
            }
            #[cfg(feature = "first-batch-adapters")]
            app.manage(adapter_batch::Batch::new(&profile, &scope, !smoke::live())?);
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
            if app.try_state::<adapter_batch::Batch>().is_some() {
                adapter_batch::start(app.handle().clone());
            }
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
        send_message,
        answer_request,
        open_request_session,
        smoke::smoke_rendered,
        smoke::smoke_next_action,
    ]);
    #[cfg(not(feature = "first-batch-smoke"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_snapshot,
        open_panel,
        close_panel,
        focus_panel,
        toggle_pin,
        send_message,
        answer_request,
        open_request_session,
    ]);

    builder
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(batch) = app.try_state::<adapter_batch::Batch>() {
                    batch.shutdown();
                }
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

fn run_hook() {
    use rooom_platform::pipe::hook;
    use std::io::{Read, Write};
    let Ok(scope) = Scope::current() else { return };
    #[cfg(feature = "first-batch-smoke")]
    let scope = {
        let id = std::env::args()
            .nth(2)
            .or_else(|| std::env::var("ROOOM_SMOKE_ID").ok());
        let Some(scope) = id.and_then(|id| scope.fixture(&id).ok()) else {
            return;
        };
        scope
    };
    // Missing/malformed/expired IPC stays silent, leaving the client prompt intact.
    let mut bytes = Vec::new();
    if std::io::stdin()
        .take((hook::MAX_PAYLOAD + 1) as u64)
        .read_to_end(&mut bytes)
        .is_ok()
    {
        let _ = hook::decide(&scope, &bytes, |decision| {
            let mut stdout = std::io::stdout().lock();
            writeln!(stdout, "{}", hook::output(decision))?;
            stdout.flush()
        });
    }
}
