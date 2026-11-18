fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "get_snapshot",
        "open_panel",
        "close_panel",
        "focus_panel",
        "toggle_pin",
        "send_message",
        "answer_request",
        "open_request_session",
        "smoke_rendered",
        "smoke_next_action",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("build Rooom application resources");
}
