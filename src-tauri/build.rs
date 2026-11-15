fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "get_snapshot",
        "open_panel",
        "close_panel",
        "focus_panel",
        "toggle_pin",
        "smoke_rendered",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("build Rooom application resources");
}
