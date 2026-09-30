//! Build script: embeds the Tauri config and generates an `allow-*`
//! permission per app command, so the capability grants each one by name.

use std::process::ExitCode;

/// Every `#[tauri::command]` the UI may invoke.
const COMMANDS: &[&str] = &[
    "list_screens",
    "sensor_catalog",
    "sample_sensors",
    "editor_session",
    "render_preview",
    "push_theme",
    "set_live",
    "set_brightness",
    "release_screen",
    "save_theme",
    "list_themes",
    "open_theme",
    "new_theme",
    "import_theme",
    "add_image",
    "list_assets",
    "list_fonts",
    "get_autostart",
    "set_autostart",
];

fn main() -> ExitCode {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    let attributes = tauri_build::Attributes::new().app_manifest(manifest);
    match tauri_build::try_build(attributes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}
