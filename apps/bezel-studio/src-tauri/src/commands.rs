//! The `#[tauri::command]`s the UI invokes: each runs its [`Backend`] method
//! on a blocking thread (screens and files block) and opens the native file
//! dialogs the method needs.

use std::path::PathBuf;
use std::sync::Arc;

use bezel_core::ports::ThemeLocation;
use bezel_themes::dto::ThemeDto;
use bezel_themes::native::EXTENSION;
use tauri::ipc::Response;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt as _;

use crate::backend::{Backend, UNTITLED, UiResult};
use crate::clock::now;
use crate::dto::{
    AddedDto, AssetDto, ImportedDto, SampleDto, SavedDto, ScreenDto, SensorDto, SessionDto,
    ThemeEntryDto, parse_orientation,
};
use crate::media::IMAGE_EXTENSIONS;

/// State managed by Tauri.
pub type Shared = Arc<Backend>;

/// Runs `work` on the blocking pool with the backend.
async fn blocking<T: Send + 'static>(
    state: &State<'_, Shared>,
    work: impl FnOnce(&Backend) -> UiResult<T> + Send + 'static,
) -> UiResult<T> {
    let backend = Arc::clone(state);
    tauri::async_runtime::spawn_blocking(move || work(&backend))
        .await
        .map_err(|e| e.to_string())?
}

/// A file chosen in the native open dialog, or `None` when cancelled.
fn pick_file<R: Runtime>(
    app: &AppHandle<R>,
    filter: &str,
    extensions: &[&str],
) -> UiResult<Option<PathBuf>> {
    app.dialog()
        .file()
        .add_filter(filter, extensions)
        .blocking_pick_file()
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

/// Lists the connected screens (read-only).
#[tauri::command]
pub async fn list_screens(state: State<'_, Shared>) -> UiResult<Vec<ScreenDto>> {
    blocking(&state, Backend::screens).await
}

/// The machine's sensors.
#[tauri::command]
pub async fn sensor_catalog(state: State<'_, Shared>) -> UiResult<Vec<SensorDto>> {
    blocking(&state, Backend::catalog).await
}

/// The latest readings and the live screen's state.
#[tauri::command]
pub async fn sample_sensors(state: State<'_, Shared>) -> UiResult<SampleDto> {
    blocking(&state, |b| Ok(b.sample())).await
}

/// The theme being edited.
#[tauri::command]
pub async fn editor_session(state: State<'_, Shared>) -> UiResult<SessionDto> {
    blocking(&state, |b| Ok(b.session())).await
}

/// Renders the UI's theme; the body is an 8-byte size header and RGBA.
#[tauri::command]
pub async fn render_preview(state: State<'_, Shared>, theme: ThemeDto) -> UiResult<Response> {
    blocking(&state, move |b| b.render(&theme, now()).map(Response::new)).await
}

/// Shows the UI's theme on the live screen now.
#[tauri::command]
pub async fn push_theme(state: State<'_, Shared>, theme: ThemeDto) -> UiResult<()> {
    blocking(&state, move |b| b.push(&theme, now())).await
}

/// Turns live mode on (on `screen`) or off.
#[tauri::command]
pub async fn set_live(state: State<'_, Shared>, on: bool, screen: Option<String>) -> UiResult<()> {
    blocking(&state, move |b| b.set_live(on, screen.as_deref(), now())).await
}

/// Sets a screen's brightness, 0 to 100.
#[tauri::command]
pub async fn set_brightness(state: State<'_, Shared>, screen: String, percent: u8) -> UiResult<()> {
    blocking(&state, move |b| b.set_brightness(&screen, percent)).await
}

/// Hands a screen back to its own mode.
#[tauri::command]
pub async fn release_screen(state: State<'_, Shared>, screen: String) -> UiResult<()> {
    blocking(&state, move |b| b.release(&screen)).await
}

/// Saves the UI's theme; `save_as` asks where first. `None` when cancelled.
#[tauri::command]
pub async fn save_theme<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Shared>,
    theme: ThemeDto,
    save_as: bool,
) -> UiResult<Option<SavedDto>> {
    let target = if save_as {
        let chosen = app
            .dialog()
            .file()
            .add_filter("Bezel", &[EXTENSION])
            .set_directory(state.library.user_dir())
            .set_file_name(format!("{}.{EXTENSION}", theme.name))
            .blocking_save_file();
        match chosen {
            None => return Ok(None),
            Some(path) => {
                let path = path.into_path().map_err(|e| e.to_string())?;
                Some(ThemeLocation(path.display().to_string()))
            }
        }
    } else {
        None
    };
    blocking(&state, move |b| b.save(&theme, target).map(Some)).await
}

/// The theme library.
#[tauri::command]
pub async fn list_themes(state: State<'_, Shared>) -> UiResult<Vec<ThemeEntryDto>> {
    blocking(&state, |b| Ok(b.themes())).await
}

/// Opens a theme of the library.
#[tauri::command]
pub async fn open_theme(state: State<'_, Shared>, location: String) -> UiResult<ThemeDto> {
    blocking(&state, move |b| b.open(&location)).await
}

/// Starts a blank theme sized for `screen`, in `orientation` (a theme.json
/// name) or the one [`Backend::new_theme`] picks.
#[tauri::command]
pub async fn new_theme(
    state: State<'_, Shared>,
    screen: Option<String>,
    name: Option<String>,
    orientation: Option<String>,
) -> UiResult<ThemeDto> {
    let orientation = orientation
        .as_deref()
        .map(|o| parse_orientation(o).ok_or_else(|| format!("unknown orientation {o:?}")))
        .transpose()?;
    blocking(&state, move |b| {
        b.new_theme(
            screen.as_deref(),
            name.as_deref().unwrap_or(UNTITLED),
            orientation,
        )
    })
    .await
}

/// Theme files the import dialog offers: Bezel's own, the TURZX app's and
/// turing-smart-screen-python's `theme.yaml` (its folder comes along).
const IMPORT_EXTENSIONS: [&str; 4] = [EXTENSION, "turtheme", "yaml", "yml"];

/// Asks for a theme file and imports it. `None` when cancelled.
#[tauri::command]
pub async fn import_theme<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Shared>,
) -> UiResult<Option<ImportedDto>> {
    let Some(path) = pick_file(&app, "Themes", &IMPORT_EXTENSIONS)? else {
        return Ok(None);
    };
    blocking(&state, move |b| b.import(&path).map(Some)).await
}

/// Asks for an image and adds it to the theme. `None` when cancelled.
#[tauri::command]
pub async fn add_image<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Shared>,
) -> UiResult<Option<AddedDto>> {
    let Some(path) = pick_file(&app, "Images", IMAGE_EXTENSIONS)? else {
        return Ok(None);
    };
    blocking(&state, move |b| b.add_image(&path).map(Some)).await
}

/// The theme's assets with previews.
#[tauri::command]
pub async fn list_assets(state: State<'_, Shared>) -> UiResult<Vec<AssetDto>> {
    blocking(&state, |b| Ok(b.assets())).await
}

/// Font families themes can use.
#[tauri::command]
pub fn list_fonts(state: State<'_, Shared>) -> Vec<String> {
    state.fonts.clone()
}

/// Whether Bezel starts at login.
#[tauri::command]
pub fn get_autostart<R: Runtime>(app: AppHandle<R>) -> UiResult<bool> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Starts Bezel at login (in the tray, showing the last theme live) or not.
#[tauri::command]
pub fn set_autostart<R: Runtime>(app: AppHandle<R>, on: bool) -> UiResult<()> {
    let manager = app.autolaunch();
    if on {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())
}
