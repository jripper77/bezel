//! The `#[tauri::command]`s the UI invokes: each runs its [`Backend`] method
//! on a blocking thread (screens and files block) and opens the native file
//! dialogs the method needs.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bezel_core::domain::job::Progress;
use bezel_core::domain::screen::Confirm;
use bezel_core::ports::ThemeLocation;
use bezel_themes::dto::ThemeDto;
use bezel_themes::native::EXTENSION;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter as _, Manager as _, Runtime, State, WebviewWindow};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt as _;

use crate::backend::{Backend, UNTITLED, UiResult};
use crate::clock::now;
use crate::dto::{
    AddedDto, AssetDto, ImportedDto, JobDto, MediaToolsDto, PrepareDto, ProgressDto, SampleDto,
    SavedDto, ScreenDto, SensorDto, SessionDto, StorageDto, ThemeEntryDto, parse_orientation,
};
use crate::media::{IMAGE_EXTENSIONS, MEDIA_EXTENSIONS};
use crate::storage::{ProgressThrottle, StorageResult};
use crate::tray::LiveItem;

/// State managed by Tauri.
pub type Shared = Arc<Backend>;

/// Runs `work` on the blocking pool with the backend.
async fn blocking<T: Send + 'static, E: From<String> + Send + 'static>(
    state: &State<'_, Shared>,
    work: impl FnOnce(&Backend) -> Result<T, E> + Send + 'static,
) -> Result<T, E> {
    let backend = Arc::clone(state);
    tauri::async_runtime::spawn_blocking(move || work(&backend))
        .await
        .map_err(|e| E::from(e.to_string()))?
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

/// Turns live mode on (on `screen`) or off; the tray's live item follows.
#[tauri::command]
pub async fn set_live(
    app: AppHandle,
    state: State<'_, Shared>,
    on: bool,
    screen: Option<String>,
) -> UiResult<()> {
    let result = blocking(&state, move |b| b.set_live(on, screen.as_deref(), now())).await;
    let live = state.studio().live_key().is_some();
    if let Some(item) = app.try_state::<LiveItem>() {
        item.sync(live);
    }
    result
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
                let location = ThemeLocation(path.display().to_string());
                // The user picked it: the window may save there from now on.
                state.library.grant(&location);
                Some(location)
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

/// Opens a theme of the library (or one picked in a dialog this session).
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

/// Whether the UI holds edits that are not saved: closing the window asks
/// first then ([`crate::on_close`]).
#[derive(Debug, Default)]
pub struct Unsaved(AtomicBool);

impl Unsaved {
    /// Whether edits are unsaved.
    pub fn get(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// The UI tells whether it holds unsaved edits.
#[tauri::command]
pub fn set_unsaved(state: State<'_, Unsaved>, unsaved: bool) {
    state.0.store(unsaved, Ordering::SeqCst);
}

/// Closes the window once the UI settled its unsaved edits: it hides while
/// a screen is live (Bezel stays in the tray), else the app ends.
#[tauri::command]
pub fn close_window<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, Shared>,
) -> UiResult<()> {
    let live = state.studio().live_key().is_some();
    match crate::on_close(live, false) {
        crate::OnClose::Hide => window.hide(),
        crate::OnClose::Ask | crate::OnClose::Close => window.destroy(),
    }
    .map_err(|e| e.to_string())
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

// ------------------------------------------------------------- storage --

/// Event carrying a running upload's progress ([`ProgressDto`]).
pub const PROGRESS_EVENT: &str = "storage-progress";

/// The answer of the UI's confirmation dialog (which names the file) as the
/// core takes it. Only these commands, the human-facing edge, make a
/// [`Confirm`] (D-2026-09-30-storage-video-1).
fn confirm_of(confirmed: bool) -> Confirm {
    if confirmed { Confirm::Yes } else { Confirm::No }
}

/// Capacity and files of a screen.
#[tauri::command]
pub async fn storage_overview(
    state: State<'_, Shared>,
    screen: String,
) -> StorageResult<StorageDto> {
    blocking(&state, move |b| b.storage_overview(&screen, now())).await
}

/// Whether ffmpeg can convert videos, with install hints when it cannot.
#[tauri::command]
pub async fn media_tools(state: State<'_, Shared>) -> UiResult<MediaToolsDto> {
    blocking(&state, |b| Ok(b.media_tools())).await
}

/// Asks where ffmpeg is and uses it when it works. `None` when cancelled.
#[tauri::command]
pub async fn locate_ffmpeg<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Shared>,
) -> UiResult<Option<MediaToolsDto>> {
    let Some(path) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    blocking(&state, move |b| Ok(Some(b.locate_ffmpeg(&path)))).await
}

/// Asks for an image or a video to send to a screen. `None` when cancelled.
#[tauri::command]
pub async fn pick_media<R: Runtime>(app: AppHandle<R>) -> UiResult<Option<String>> {
    Ok(pick_file(&app, "Media", MEDIA_EXTENSIONS)?.map(|p| p.display().to_string()))
}

/// The preflight of sending the local file `source` to `medium` of `screen`.
#[tauri::command]
pub async fn prepare_upload(
    state: State<'_, Shared>,
    screen: String,
    source: String,
    medium: String,
) -> StorageResult<PrepareDto> {
    blocking(&state, move |b| {
        b.prepare_upload(&screen, &PathBuf::from(source), &medium, now())
    })
    .await
}

/// The preflight of sending the theme's video to the live screen.
#[tauri::command]
pub async fn prepare_theme_video(
    state: State<'_, Shared>,
    screen: String,
) -> StorageResult<PrepareDto> {
    blocking(&state, move |b| b.prepare_theme_video(&screen, now())).await
}

/// Runs a prepared upload; progress goes out as [`PROGRESS_EVENT`].
#[tauri::command]
pub async fn run_upload<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Shared>,
    ticket: u64,
    overwrite: bool,
) -> StorageResult<JobDto> {
    blocking(&state, move |b| {
        let mut throttle = ProgressThrottle::default();
        let mut report = |progress: Progress| {
            if throttle.pass(progress)
                && let Err(e) = app.emit(PROGRESS_EVENT, ProgressDto::from(progress))
            {
                tracing::warn!("upload progress not sent: {e}");
            }
        };
        b.run_upload(ticket, confirm_of(overwrite), now(), &mut report)
    })
    .await
}

/// Asks the running upload to stop.
#[tauri::command]
pub fn cancel_job(state: State<'_, Shared>) -> bool {
    state.cancel_job()
}

/// Deletes a stored file; `confirmed` comes from the dialog naming it.
#[tauri::command]
pub async fn delete_stored(
    state: State<'_, Shared>,
    screen: String,
    path: String,
    confirmed: bool,
) -> StorageResult<()> {
    blocking(&state, move |b| {
        b.delete_stored(&screen, &path, confirm_of(confirmed), now())
    })
    .await
}

/// Plays a stored file.
#[tauri::command]
pub async fn play_stored(
    state: State<'_, Shared>,
    screen: String,
    path: String,
) -> StorageResult<()> {
    blocking(&state, move |b| b.play_stored(&screen, &path, now())).await
}

/// Stops what the screen plays.
#[tauri::command]
pub async fn stop_playback(state: State<'_, Shared>, screen: String) -> StorageResult<()> {
    blocking(&state, move |b| b.stop_playback(&screen, now())).await
}

/// Sets the boot media (`None`: the built-in screen) and the brightness
/// the screen starts with (`None`: the screen's own); `confirmed` comes from
/// the dialog naming both.
#[tauri::command]
pub async fn set_boot_media(
    state: State<'_, Shared>,
    screen: String,
    path: Option<String>,
    confirmed: bool,
    brightness: Option<u8>,
) -> StorageResult<()> {
    blocking(&state, move |b| {
        let confirm = confirm_of(confirmed);
        b.set_boot_media(&screen, path.as_deref(), brightness, confirm, now())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_confirmed_dialog_is_confirm_yes() {
        assert_eq!(confirm_of(true), Confirm::Yes);
        assert_eq!(confirm_of(false), Confirm::No);
    }
}
