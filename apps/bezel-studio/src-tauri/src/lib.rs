//! Bezel Studio — the desktop driving adapter of Bezel.
//!
//! The webview UI turns clicks into calls on the [`backend`]; every rule
//! about screens, sensors and themes lives in the core and its adapters.
//! [`run`] is the composition root: it picks real or simulated adapters,
//! starts the refresh loop that samples sensors and feeds the live screen,
//! and keeps the app in the tray while a screen is live. What it composes
//! once the runtime is up is `setup`, which a test runs on Tauri's mock
//! runtime.

#![forbid(unsafe_code)]

pub mod backend;
pub mod clock;
pub mod commands;
pub mod diag;
pub mod dto;
pub mod gifs;
pub mod library;
pub mod manager;
pub mod media;
pub mod messages;
pub mod settings;
pub mod storage;
pub mod studio;
pub mod texts;
pub mod thumbnails;
mod tray;
pub mod udev_help;
pub mod video;

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::theme::Theme;
use bezel_core::ports::{DesktopModeHid, DeviceBus, GifSource, ScreenConnector};
use bezel_devices::fake::FakeStorage;
use bezel_devices::{FakeBus, FakeConnector, FakeHid, SystemBus, SystemConnector, SystemHid};
use bezel_klipy::KlipyClient;
use bezel_media::FfmpegTranscoder;
use bezel_media::archive::{DiskArchive, MemoryArchive, storage_dir};
use bezel_media::collection::{DiskCollection, collection_dir};
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::{FakeSensors, SystemSensors};
use bezel_themes::FsThemeStore;
use tauri::{App, AppHandle, Emitter as _, Manager, Runtime, WindowEvent};

use crate::backend::{
    Backend, DEFAULT_MODEL, SensorFactory, Session, default_orientation, sleep_until,
};
use crate::commands::{Shared, Unsaved};
use crate::diag::DiagCode;
use crate::gifs::{
    Gifs, KEY_FILE, KeyFile, KlipyKey, Provider, SharedGifs, SourceFactory, UserAsked,
    collection_in,
};
use crate::library::ThemeLibrary;
use crate::manager::Copies;
use crate::settings::SettingsFile;
use crate::storage::{MediaSetup, StorageState};
use crate::studio::Studio;
use crate::texts::Texts;
use crate::thumbnails::Thumbnails;
use crate::udev_help::UdevHelp;

/// Label of the main window in `tauri.conf.json`.
pub(crate) const MAIN_WINDOW: &str = "main";

/// Set to `1` to serve a simulated Turing 8.8" and scripted sensors instead
/// of the real machine: demos and checks that must never touch a screen.
pub const SIMULATION_SWITCH: &str = "BEZEL_FAKE";

/// Event asking the UI to settle unsaved edits before the window closes.
pub const CLOSE_EVENT: &str = "close-requested";

/// What the window's close button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnClose {
    /// A screen is live: the window hides and Bezel keeps driving the
    /// screen from the tray (the edits stay in the session).
    Hide,
    /// Edits are unsaved: the UI asks (save, discard or cancel) and then
    /// closes the window itself.
    Ask,
    /// The window closes and the app ends.
    Close,
}

/// What closing the window does with a screen `live` and edits `unsaved`.
pub fn on_close(live: bool, unsaved: bool) -> OnClose {
    if live {
        OnClose::Hide
    } else if unsaved {
        OnClose::Ask
    } else {
        OnClose::Close
    }
}

/// Event asking the UI to settle unsaved edits before the app quits (the
/// tray's Quit); the UI then calls `quit_app`.
pub const QUIT_EVENT: &str = "quit-requested";

/// What the tray's Quit does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnQuit {
    /// Edits are unsaved: the window shows and the UI asks (save, discard
    /// or cancel), then quits the app itself.
    Ask,
    /// The app ends.
    Exit,
}

/// What quitting does with edits `unsaved` (whether a screen is live or not:
/// quitting ends the live mode too).
pub fn on_quit(unsaved: bool) -> OnQuit {
    if unsaved { OnQuit::Ask } else { OnQuit::Exit }
}

/// Argument of the start at login: open in the tray, without the window.
pub const HIDDEN_ARG: &str = "--hidden";

/// WebKitGTK's switch that turns its DMA-BUF renderer off.
#[cfg(target_os = "linux")]
const DMABUF_SWITCH: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";

/// What the terminal says when the app did not start: which part failed,
/// told by the variant of Tauri's `error`, never by its text
/// (D-2026-10-01-gif-sticker-search-12, review W1 of round 2, iter 4).
pub fn start_failure(error: &tauri::Error) -> DiagCode {
    match error {
        tauri::Error::Runtime(_) => DiagCode::NoWindow,
        tauri::Error::Setup(_) => DiagCode::SetupFailed,
        tauri::Error::PluginInitialization(..) => DiagCode::PluginNotStarted,
        _ => DiagCode::NotStarted,
    }
}

/// What the terminal says when the refresh loop's thread did not start,
/// told by the kind of the `error`: the system out of threads or memory, or
/// another cause.
fn refresh_loop_failure(error: &io::Error) -> DiagCode {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::OutOfMemory => DiagCode::RefreshLoopNoResources,
        _ => DiagCode::RefreshLoopNotStarted,
    }
}

/// What the terminal says when the process could not restart itself with
/// the DMA-BUF renderer off, told by the kind of `exec`'s `error`: its
/// program file gone, running it not allowed, or another cause.
#[cfg(target_os = "linux")]
fn restart_failure(error: &io::Error) -> DiagCode {
    match error.kind() {
        io::ErrorKind::NotFound => DiagCode::DmabufRestartNoFile,
        io::ErrorKind::PermissionDenied => DiagCode::DmabufRestartDenied,
        _ => DiagCode::DmabufRendererOn,
    }
}

/// Starts the app and blocks until it exits.
///
/// # Errors
///
/// Tauri's error when the app cannot start; [`start_failure`] says which
/// part failed.
pub fn run() -> Result<(), tauri::Error> {
    #[cfg(target_os = "linux")]
    restart_without_dmabuf_renderer();

    let start: Start<tauri::Wry> = Start {
        simulate: switch_on(std::env::var_os(SIMULATION_SWITCH).as_deref()),
        hidden: std::env::args().any(|a| a == HIDDEN_ARG),
        folders: Box::new(|app: &AppHandle| Folders::of(app)),
        gif_source: klipy_source(),
        tray: Box::new(add_tray),
    };
    tauri::Builder::default()
        // First plugin: a second launch shows this window and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        // Opens the guide's fixed pages from Rust only (`open_guide`): no
        // permission lets the webview call it, nor are its links opened.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([HIDDEN_ARG])
                .build(),
        )
        .setup(move |app| setup(app, start))
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let live = window
                .try_state::<Shared>()
                .is_some_and(|b| b.studio().live_key().is_some());
            let unsaved = window.try_state::<Unsaved>().is_some_and(|u| u.get());
            match on_close(live, unsaved) {
                OnClose::Hide => {
                    api.prevent_close();
                    // Best effort: a window that cannot hide stays open, and
                    // the screen keeps updating either way.
                    let _ = window.hide();
                }
                // A UI that cannot be asked does not keep the window open.
                OnClose::Ask => match window.emit(CLOSE_EVENT, ()) {
                    Ok(()) => api.prevent_close(),
                    Err(_) => diag::report(DiagCode::UnsavedEditsNotAsked),
                },
                OnClose::Close => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::leave_desktop_mode,
            commands::quit_app,
            commands::sensor_catalog,
            commands::sample_sensors,
            commands::editor_session,
            commands::render_preview,
            commands::video_auto,
            commands::open_guide,
            commands::push_theme,
            commands::set_live,
            commands::set_brightness,
            commands::release_screen,
            commands::save_theme,
            commands::list_themes,
            commands::open_theme,
            commands::new_theme,
            commands::import_theme,
            commands::add_image,
            commands::add_media,
            commands::list_assets,
            commands::list_fonts,
            commands::get_autostart,
            commands::set_autostart,
            commands::storage_overview,
            commands::media_tools,
            commands::locate_ffmpeg,
            commands::pick_media,
            commands::prepare_upload,
            commands::prepare_theme_video,
            commands::run_upload,
            commands::cancel_job,
            commands::delete_stored,
            commands::play_stored,
            commands::stop_playback,
            commands::set_boot_media,
            commands::set_unsaved,
            commands::close_window,
            commands::preferences,
            commands::set_language,
            commands::set_sensor_options,
            commands::pick_folder,
            commands::show_sensors,
            commands::restart_screen,
            commands::theme_thumbnail,
            commands::set_theme_filter,
            commands::manager_overview,
            commands::manager_thumbnail,
            commands::plan_move,
            commands::plan_copy,
            commands::plan_rename,
            commands::plan_restore,
            commands::run_plan,
            commands::delete_files,
            commands::pick_originals,
            commands::associate_candidates,
            commands::associate_original,
            commands::cache_info,
            commands::clear_cache,
            commands::set_cache_limit,
            commands::klipy_key,
            commands::save_klipy_key,
            commands::remove_klipy_key,
            commands::search_gifs,
            commands::gif_preview,
            commands::collect_gif,
            commands::gif_collection,
            commands::rename_collected,
            commands::collected_users,
            commands::delete_collected,
            commands::use_collected,
            commands::open_link,
        ])
        .run(tauri::generate_context!())
}

/// Keeps the tray's live item in step with whether a screen is live.
type LiveSync = Box<dyn Fn(bool) + Send>;

/// Finds where the app keeps its files, once Tauri knows its paths.
type FindFolders<R> = Box<dyn FnOnce(&AppHandle<R>) -> tauri::Result<Folders> + Send>;

/// Adds the tray icon with its menu, given whether a screen is live and
/// the labels; answers what keeps its live item in step.
type AddTray<R> = Box<dyn FnOnce(&AppHandle<R>, bool, &Texts) -> tauri::Result<LiveSync> + Send>;

/// What [`setup`] gets from [`run`]: the start's switches, where the files
/// are, the GIF provider and the tray. A test of the setup gives temporary
/// folders, a fake GIF source and no tray icon.
struct Start<R: Runtime> {
    /// Simulated screen and sensors ([`SIMULATION_SWITCH`]).
    simulate: bool,
    /// Started at login ([`HIDDEN_ARG`]): the window stays hidden.
    hidden: bool,
    folders: FindFolders<R>,
    /// Makes the GIF source for a saved key: KLIPY's client.
    gif_source: SourceFactory,
    tray: AddTray<R>,
}

/// The app's start, once the runtime is up (Tauri's `setup`): the backend
/// and the GIF state composed and kept as the app's state, the tray, the
/// refresh loop and the window. Nothing is asked of KLIPY here, nor by
/// anything started here (D-2026-10-01-gif-sticker-search-3). The GIF state
/// makes and searches a source only with a [`UserAsked`], which only a
/// command Tauri is running has, so a search through it from here does not
/// compile. What the type cannot stop is making Tauri dispatch an
/// invocation the window never sent, another command making a proof of
/// its own request or calling a GIF command's function with it, or making
/// a second KLIPY client: the source guard
/// `tests::nothing_in_the_app_forges_an_invocation`
/// (D-2026-10-01-gif-sticker-search-10, -11, -12) refuses, by identifier in
/// the studio's production code (raw names and the tokens of macro calls
/// included), the Tauri APIs that do the first or load a page in the
/// window (`eval`, `with_webview`, `on_message`, `invoke_key`, `navigate`,
/// ...) and literals that are `javascript:` URLs; what an invocation is made
/// of (`Invoke`, `InvokeMessage`, `InvokeBody`, `payload`), so that no
/// invoke handler reads one; a print, a log or a formatted panic anywhere
/// but in [`diag`], which says fixed text only; [`UserAsked::of`] but in
/// the bodies of `search_gifs`, `gif_preview` and `collect_gif` (and
/// [`UserAsked`] renamed, in a qualified path, in another macro call or in
/// an `impl` outside its module); the window's invocation (`Request`)
/// taken by any other function; a command function named but at its
/// definition and in the list of `generate_handler!` in [`run`]; and any
/// `KlipyClient::new` but the source factory's. Code written to get past
/// it otherwise is left to code review.
fn setup<R: Runtime>(app: &App<R>, start: Start<R>) -> Result<(), Box<dyn std::error::Error>> {
    // Each part that fails says so, before the app says it did not start.
    let folders =
        (start.folders)(app.handle()).inspect_err(|_| diag::report(DiagCode::FoldersNotFound))?;
    let backend: Shared = Arc::new(compose(&folders, start.simulate));
    // Before the window asks for it: the last theme, or a blank one for the
    // connected screen.
    backend.restore_theme();
    app.manage(Arc::clone(&backend));
    // Its own state, which asks nothing of KLIPY until a search.
    let gifs: SharedGifs = Arc::new(gifs(&folders, start.gif_source));
    app.manage(gifs);
    app.manage(Unsaved::default());
    let live = backend.studio().live_key().is_some();
    let live_item = (start.tray)(app.handle(), live, &backend.texts())
        .inspect_err(|_| diag::report(DiagCode::TrayNotAdded))?;
    start_refresh_loop(backend, live_item);
    if !start.hidden {
        show_main_window(app.handle());
    }
    Ok(())
}

/// Adds the tray icon and keeps its menu as the app's state, for the
/// commands that relabel it or follow live mode.
fn add_tray(app: &AppHandle, live: bool, text: &Texts) -> tauri::Result<LiveSync> {
    let tray = tray::create(app, live, text)?;
    app.manage(tray.live().clone());
    app.manage(tray.clone());
    let item = tray.live().clone();
    Ok(Box::new(move |live| item.sync(live)))
}

/// Shows, restores and focuses the main window.
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        // Best effort for each step: the window manager may refuse focus or
        // unminimizing, and there is nothing better to do than try the rest.
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Sends a storage job's progress (an upload's, a storage manager plan's)
/// to the window as [`commands::PROGRESS_EVENT`]; one that cannot be sent is
/// said ([`diag`]), and the job goes on. It is given no key and no
/// invocation.
pub(crate) fn emit_progress<R: Runtime>(app: &AppHandle<R>, progress: dto::ProgressDto) {
    if app.emit(commands::PROGRESS_EVENT, progress).is_err() {
        diag::report(DiagCode::StorageProgressNotSent);
    }
}

/// The adapters: real ones, or simulated ones when `simulate`.
struct Adapters {
    bus: Arc<dyn DeviceBus + Send + Sync>,
    connector: Arc<dyn ScreenConnector + Send + Sync>,
    hid: Arc<dyn DesktopModeHid + Send + Sync>,
    sensors: SensorFactory,
}

/// Usable bytes of the simulated screen's memory card (a 32 GB card).
const SIMULATED_CARD_BYTES: u64 = 31_914_983_424;

fn adapters(simulate: bool) -> Adapters {
    if simulate {
        diag::report(DiagCode::Simulated);
        let storage = FakeStorage::default().with_card(SIMULATED_CARD_BYTES);
        Adapters {
            bus: Arc::new(FakeBus::turing_88()),
            connector: Arc::new(FakeConnector::with_storage(storage)),
            hid: Arc::new(FakeHid::answering(0x88)),
            sensors: Arc::new(|_| Box::new(FakeSensors::demo())),
        }
    } else {
        Adapters {
            bus: Arc::new(SystemBus),
            connector: Arc::new(SystemConnector),
            hid: Arc::new(SystemHid),
            sensors: Arc::new(|options| Box::new(SystemSensors::with_options(options))),
        }
    }
}

/// A blank theme for the most common screen (horizontal, like every
/// bar-shaped one), until [`Backend::restore_theme`] picks the real one.
fn starting_theme() -> Theme {
    let name = texts::texts(clock::language()).untitled;
    match model_by_id(DEFAULT_MODEL) {
        Some(m) => Theme::blank(name, m.panel, default_orientation(m)),
        None => Theme::blank(name, Size::new(480, 1920), Orientation::Landscape),
    }
}

/// Where the app keeps its files.
struct Folders {
    /// The app's config folder: `settings.json`, `klipy.json`.
    config: PathBuf,
    /// The user's data folder: `<data>/bezel` is shared with the CLI (the
    /// local copies, the collection) and holds the themes
    /// `install-local.sh` installs.
    data: PathBuf,
    /// The app's data folder: the user's themes.
    app_data: PathBuf,
    /// The app's cache folder.
    cache: PathBuf,
    /// The installed app's resources (the bundled themes), when known.
    resources: Option<PathBuf>,
}

impl Folders {
    /// The folders Tauri finds for the app on this system.
    fn of<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Self> {
        let path = app.path();
        Ok(Self {
            config: path.app_config_dir()?,
            data: path.data_dir()?,
            app_data: path.app_data_dir()?,
            cache: path.app_cache_dir()?,
            resources: path.resource_dir().ok(),
        })
    }
}

/// Folders of the themes that ship with Bezel: next to the installed app
/// (packages) and in the user's data folder (`install-local.sh`).
fn bundled_theme_dirs(folders: &Folders) -> Vec<PathBuf> {
    [
        folders.resources.as_ref().map(|d| d.join("themes")),
        Some(folders.data.join("bezel").join("themes")),
    ]
    .into_iter()
    .flatten()
    .filter(|d| d.is_dir())
    .collect()
}

fn compose(folders: &Folders, simulate: bool) -> Backend {
    let Adapters {
        bus,
        connector,
        hid,
        sensors,
    } = adapters(simulate);
    // The bundled themes' fonts first, so previews match every machine.
    let bundled_dirs = bundled_theme_dirs(folders);
    let bundled_fonts = bundled_dirs
        .iter()
        .flat_map(|dir| font_files(&dir.join("fonts")))
        .collect();
    let renderer = SkiaRenderer::with_fonts(bundled_fonts, SystemFonts::Load);
    let fonts = renderer.font_families();
    let settings = SettingsFile::new(folders.config.join("settings.json"));
    let system_language = clock::language();
    let language = settings.load().language().unwrap_or(system_language);
    let ffmpeg = settings.load().ffmpeg_path.map(PathBuf::from);
    let cache = &folders.cache;
    let copies = if simulate {
        // The simulated 8.8" is keyed like a real one: never in the
        // user's catalog.
        Copies::in_memory(MemoryArchive::new())
    } else {
        copies(&folders.data)
    };
    let storage = StorageState::new(
        Box::new(FfmpegTranscoder::new(ffmpeg)),
        copies,
        cache.join("sending"),
    );
    // Screens that cannot play videos get the theme's video decoded here by
    // the storage tab's converter.
    let measured = sensors(settings.load().sensor_options());
    let studio = Studio::new(measured, Box::new(renderer), language, starting_theme())
        .with_host_decoding(storage.shared_media(), cache.join("playing"));
    Backend {
        bus,
        connector,
        hid,
        store: Arc::new(FsThemeStore),
        library: ThemeLibrary::new(folders.app_data.join("themes"), bundled_theme_dirs(folders)),
        settings,
        system_language,
        make_sensors: sensors,
        // Only Linux grants USB access through udev rules.
        udev: cfg!(target_os = "linux")
            .then(|| UdevHelp::new(cache.join(bezel_devices::udev::FILE_NAME))),
        fonts,
        studio: Session::new(studio),
        storage,
        thumbnails: thumbnails(&bundled_dirs, cache.join("thumbnails")),
    }
}

/// The local copies of what the studio sends, in `<data>/bezel/storage`
/// shared with the CLI (D-2026-09-30-storage-manager-5); in memory for this
/// run when that folder cannot be made.
fn copies(data: &Path) -> Copies {
    match DiskArchive::open(storage_dir(data)) {
        Ok(archive) => Copies::on_disk(archive),
        Err(_) => {
            diag::report(DiagCode::CopiesInMemory);
            Copies::in_memory(MemoryArchive::new())
        }
    }
}

/// KLIPY's client for a saved key (D-2026-10-01-gif-sticker-search-2),
/// made only for a user action ([`UserAsked`]): making one asks nothing.
/// The one reader of the key's text outside the key file
/// (D-2026-10-01-gif-sticker-search-10): `key.expose_secret()` only as a
/// direct argument of `KlipyClient::new`, and no macro here, which the
/// source guard checks (a print or a log of the key does not pass it).
fn klipy_source() -> SourceFactory {
    Arc::new(
        |_: &UserAsked, key: &KlipyKey, customer: &str| -> Arc<dyn GifSource> {
            Arc::new(KlipyClient::new(key.expose_secret(), customer))
        },
    )
}

/// The GIF search and the collection (D-2026-10-01-gif-sticker-search-3,
/// -5): sources from `source` for the KLIPY key in `<config>/klipy.json`,
/// the collection in `<data>/bezel/collection` shared with the CLI's data
/// folder (a folder that cannot be used is said, never replaced by one in
/// memory: review W2), a background's copy in `<cache>/collection`.
/// Nothing is read from KLIPY here.
fn gifs(folders: &Folders, source: SourceFactory) -> Gifs {
    let provider = Provider {
        source,
        customer_id: bezel_klipy::new_customer_id,
    };
    Gifs::new(
        KeyFile::new(folders.config.join(KEY_FILE)),
        provider,
        collection_in(collection_dir(&folders.data), |dir| {
            DiskCollection::open(dir)
        }),
        folders.cache.join("collection"),
    )
}

/// The library's thumbnails, kept in `dir`: drawn with the bundled themes'
/// fonts and the installed ones (loaded on the first thumbnail drawn) and the
/// demo sensor values.
fn thumbnails(bundled_dirs: &[PathBuf], dir: PathBuf) -> Thumbnails {
    let font_dirs: Vec<PathBuf> = bundled_dirs.iter().map(|d| d.join("fonts")).collect();
    Thumbnails::new(
        dir,
        Box::new(move || {
            let fonts = font_dirs.iter().flat_map(|d| font_files(d)).collect();
            Box::new(SkiaRenderer::with_fonts(fonts, SystemFonts::Load))
        }),
        Box::new(|| Box::new(FakeSensors::demo())),
    )
}

/// The storage tab's Locate button moves the ffmpeg the adapter looks for.
impl MediaSetup for FfmpegTranscoder {
    fn set_tool_path(&mut self, path: Option<PathBuf>) {
        self.set_ffmpeg_path(path);
    }

    fn tool_in_use(&mut self) -> Option<PathBuf> {
        self.ffmpeg_in_use()
    }

    fn spare(&self) -> Box<dyn MediaSetup> {
        Box::new(FfmpegTranscoder::new(
            self.ffmpeg_path().map(Path::to_path_buf),
        ))
    }
}

/// Shows the last live screen again, then samples and refreshes the live
/// screen when the session says (the theme's refresh, a visible GIF's
/// frames, an attempt to connect a failed screen again), on its own thread
/// for the life of the app. The tray's live item follows.
fn start_refresh_loop(backend: Shared, live_item: LiveSync) {
    let spawned = std::thread::Builder::new()
        .name("bezel-refresh".into())
        .spawn(move || {
            if backend.studio().refresh_catalog().is_err() {
                diag::report(DiagCode::SensorCatalogNotRead);
            }
            backend.restore_live(clock::now());
            loop {
                let due = backend.tick(clock::now(), Instant::now());
                // Not under the session's lock: the menu waits for the main
                // thread.
                let live = backend.studio().live_key().is_some();
                live_item(live);
                std::thread::sleep(sleep_until(due, Instant::now()));
            }
        });
    if let Err(error) = spawned {
        diag::report(refresh_loop_failure(&error));
    }
}

/// Whether a switch set to `value` is on: only `1` is.
fn switch_on(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

/// WebKitGTK's DMA-BUF renderer kills the app with a Wayland protocol error
/// on NVIDIA's driver (seen on the dev machine with ddc-control). WebKit reads
/// the switch once at start and setting it in-process needs `unsafe`, so the
/// process replaces itself with the switch on, unless the user chose a value.
#[cfg(target_os = "linux")]
fn restart_without_dmabuf_renderer() {
    use std::os::unix::process::CommandExt;

    if std::env::var_os(DMABUF_SWITCH).is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // `exec` returns only when the process could not replace itself.
    let error = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(DMABUF_SWITCH, "1")
        .exec();
    diag::report(restart_failure(&error));
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    #[cfg(not(windows))]
    use std::time::Duration;

    use super::*;
    use bezel_core::app::discover_screens;
    #[cfg(not(windows))]
    use bezel_core::domain::gifs::{GifItem, GifKind, GifPage, Rendition, RenditionFormat, Tier};
    use bezel_media::collection::FakeGifSource;
    #[cfg(not(windows))]
    use bezel_media::collection::GifCall;
    #[cfg(not(windows))]
    use serde_json::{Value, json};
    #[cfg(not(windows))]
    use tauri::RunEvent;
    #[cfg(not(windows))]
    use tauri::ipc::{CallbackFn, InvokeBody};
    #[cfg(not(windows))]
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    #[cfg(not(windows))]
    use tauri::utils::config::WindowConfig;
    #[cfg(not(windows))]
    use tauri::webview::{InvokeRequest, WebviewWindow};

    use proc_macro2::{Delimiter, Ident, Spacing, TokenStream, TokenTree};
    use syn::ext::IdentExt as _;
    use syn::punctuated::Punctuated;
    use syn::token::Comma;
    use syn::visit::{self, Visit};
    use syn::{
        Arm, AttrStyle, Attribute, BinOp, Block, Expr, ExprCall, ExprClosure, ExprForLoop, ExprIf,
        ExprMethodCall, ExprStruct, ExprWhile, Fields, FnArg, ImplItem, ImplItemFn, ImplItemType,
        Item, ItemExternCrate, ItemFn, ItemImpl, ItemType, ItemUse, Lit, Macro, Meta, Pat,
        PatIdent, QSelf, ReturnType, Signature, Stmt, TraitItemFn, Type, UseName, UseRename,
        UseTree,
    };

    #[cfg(not(windows))]
    use crate::gifs::SavedKey;

    /// An obvious fake KLIPY key.
    #[cfg(not(windows))]
    const KEY: &str = "fake-KLIPY_key-0123456789abcdef";

    /// How long the started app idles, after its refresh loop's first
    /// round, for anything it started to ask KLIPY.
    #[cfg(not(windows))]
    const IDLE: Duration = Duration::from_millis(1500);

    /// An empty temporary folder for the test `name`.
    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bezel-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    impl Folders {
        /// The app's folders, all inside `root`.
        fn under(root: &Path) -> Self {
            Self {
                config: root.join("config"),
                data: root.join("data"),
                app_data: root.join("app-data"),
                cache: root.join("cache"),
                resources: None,
            }
        }
    }

    /// A GIF source factory over `source` that says on `made` each source
    /// it makes.
    fn counting(source: &FakeGifSource, made: mpsc::Sender<()>) -> SourceFactory {
        let source = source.clone();
        Arc::new(
            move |_: &UserAsked, _: &KlipyKey, _: &str| -> Arc<dyn GifSource> {
                let _ = made.send(());
                Arc::new(source.clone())
            },
        )
    }

    /// D-2026-10-01-gif-sticker-search-3 (DoD critic, row 3): the app's own
    /// setup, run by Tauri's runtime (the mock one) with a KLIPY key saved,
    /// asks nothing of KLIPY. The GIF source is never made, so never asked,
    /// while the app idles: its refresh loop goes round once, then a while
    /// more. Only the folders (temporary), the GIF source (counted), the
    /// screen (simulated) and the tray (none: it needs the desktop's) are
    /// the test's.
    ///
    /// Not built on Windows (D-2026-10-01-gif-sticker-search-8): the mock
    /// runtime's `test` feature makes the Windows test binary fail to load
    /// without the Common Controls v6 manifest; what it proves does not
    /// depend on the platform.
    #[cfg(not(windows))]
    #[test]
    fn the_app_setup_sends_nothing_at_start() {
        let root = temp_root("setup");
        let folders = Folders::under(&root);
        let saved = SavedKey::new(KlipyKey::parse(KEY).unwrap(), "customer-0001");
        KeyFile::new(folders.config.join(KEY_FILE))
            .save(&saved)
            .unwrap();
        let source = FakeGifSource::new();
        let (made, made_rx) = mpsc::channel();
        let (round, rounds) = mpsc::channel();
        let start = Start {
            simulate: true,
            hidden: false,
            folders: Box::new(move |_: &AppHandle<MockRuntime>| Ok(folders)),
            gif_source: counting(&source, made),
            tray: Box::new(move |_: &AppHandle<MockRuntime>, _, _: &Texts| {
                // The refresh loop syncs the live item after each round.
                let synced: LiveSync = Box::new(move |_| {
                    let _ = round.send(());
                });
                Ok(synced)
            }),
        };
        let app = mock_builder()
            .setup(move |app| setup(app, start))
            .build(context_with_the_window())
            .unwrap();

        let (seen, seen_rx) = mpsc::channel();
        app.run_return(move |app, event| {
            if !matches!(event, RunEvent::Ready) {
                return;
            }
            let went_round = rounds.recv_timeout(Duration::from_secs(60)).is_ok();
            let asked = made_rx.recv_timeout(IDLE).is_ok();
            let composed =
                app.try_state::<Shared>().is_some() && app.try_state::<Unsaved>().is_some();
            let key_saved = app
                .try_state::<SharedGifs>()
                .is_some_and(|gifs| gifs.key_status().is_ok_and(|key| key.configured));
            seen.send((went_round, asked, composed, key_saved)).unwrap();
            // The window closes: the app ends.
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                window.destroy().unwrap();
            }
        });
        let (went_round, asked, composed, key_saved) = seen_rx.recv().unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            composed,
            "the setup kept the backend and the session's state"
        );
        assert!(key_saved, "the app's GIF state has the saved key");
        assert!(went_round, "the setup started the refresh loop");
        assert!(!asked, "a GIF source was made at start");
        assert!(source.calls().is_empty(), "KLIPY was asked at start");
    }

    /// A mock context with the window as `tauri.conf.json` has it, made
    /// before the setup.
    #[cfg(not(windows))]
    fn context_with_the_window() -> tauri::Context<MockRuntime> {
        let mut context = mock_context(noop_assets());
        context.config_mut().app.windows.push(WindowConfig {
            label: MAIN_WINDOW.into(),
            visible: false,
            ..WindowConfig::default()
        });
        context
    }

    /// The window invokes `command` with `args` through Tauri's IPC, as
    /// `bridge.js` does: its answer, or the error it was rejected with.
    #[cfg(not(windows))]
    fn invoke(
        window: &WebviewWindow<MockRuntime>,
        command: &str,
        args: Value,
    ) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::from(args),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.into(),
        };
        get_ipc_response(window, request).map(|body| body.deserialize().unwrap())
    }

    /// D-2026-10-01-gif-sticker-search-3: what lets the GIF state ask KLIPY
    /// is a command the window invoked. Through Tauri's IPC (the mock
    /// runtime's), with the arguments the UI sends (`bridge.js`, unchanged),
    /// `search_gifs`, `gif_preview` and `collect_gif` reach the source,
    /// made on the first of them and not by the setup.
    #[cfg(not(windows))]
    #[test]
    fn the_windows_gif_commands_reach_the_source() {
        let root = temp_root("ipc");
        let folders = Folders::under(&root);
        KeyFile::new(folders.config.join(KEY_FILE))
            .save(&SavedKey::new(
                KlipyKey::parse(KEY).unwrap(),
                "customer-0001",
            ))
            .unwrap();
        let cat = GifItem {
            id: "a1".into(),
            title: "Cat".into(),
            kind: GifKind::Gif,
            page_url: None,
            renditions: vec![Rendition {
                tier: Tier::Small,
                format: RenditionFormat::Gif,
                location: "f/a1.gif".into(),
                width: 2,
                height: 2,
                bytes: None,
            }],
        };
        let page = GifPage {
            items: vec![cat],
            has_next: false,
        };
        let source = FakeGifSource::new()
            .with_page(GifKind::Gif, "cat", 1, page)
            .with_file("f/a1.gif", crate::media::tests::gif(2));
        let (made, made_rx) = mpsc::channel();
        let start = Start {
            simulate: true,
            hidden: true,
            folders: Box::new(move |_: &AppHandle<MockRuntime>| Ok(folders)),
            gif_source: counting(&source, made),
            tray: Box::new(|_: &AppHandle<MockRuntime>, _, _: &Texts| {
                let synced: LiveSync = Box::new(|_| {});
                Ok(synced)
            }),
        };
        let app = mock_builder()
            .setup(move |app| setup(app, start))
            .invoke_handler(tauri::generate_handler![
                commands::search_gifs,
                commands::gif_preview,
                commands::collect_gif,
            ])
            .build(context_with_the_window())
            .unwrap();

        let (seen, seen_rx) = mpsc::channel();
        app.run_return(move |app, event| {
            if !matches!(event, RunEvent::Ready) {
                return;
            }
            let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
                return;
            };
            let at_start = made_rx.try_iter().count();
            let search = json!({"kind": "gif", "text": "cat", "page": 1, "explicit": false});
            let answers = [
                invoke(&window, "search_gifs", search),
                invoke(&window, "gif_preview", json!({"id": "a1", "still": false})),
                invoke(&window, "collect_gif", json!({"id": "a1"})),
            ];
            let made = made_rx.try_iter().count();
            seen.send((at_start, answers, made)).unwrap();
            // The window closes: the app ends.
            window.destroy().unwrap();
        });
        let (at_start, [found, preview, collected], made) = seen_rx.recv().unwrap();
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(at_start, 0, "a GIF source was made at start");
        let found = found.unwrap();
        assert_eq!(found["items"][0]["id"], "a1", "{found}");
        let preview = preview.unwrap();
        let preview = preview.as_str().unwrap_or_default();
        assert!(preview.starts_with("data:image/gif;base64,"), "{preview}");
        let collected = collected.unwrap();
        assert_eq!(collected["source"]["id"], "a1", "{collected}");
        assert_eq!(made, 1, "one source, made for the first command");
        // The search, the preview's file, the collected file.
        let calls = source.calls();
        let asked = match &calls[..] {
            [
                GifCall::Page(query),
                GifCall::Download {
                    location: shown, ..
                },
                GifCall::Download { location: kept, .. },
            ] => Some((query.text.as_str(), shown.as_str(), kept.as_str())),
            _ => None,
        };
        assert_eq!(asked, Some(("cat", "f/a1.gif", "f/a1.gif")), "{calls:?}");
    }

    /// D-2026-10-01-gif-sticker-search-10: through Tauri's IPC (the mock
    /// runtime's), with the argument the UI sends (`bridge.js`, unchanged:
    /// `{key}`, a string), `save_klipy_key` reads the key straight into a
    /// [`KlipyKey`]: one that cannot be a key rejects with `invalidInput`,
    /// as before and never quoting it, so the window still says which
    /// characters a key has; a key saves, and the window sees its last 4.
    #[cfg(not(windows))]
    #[test]
    fn the_window_sends_the_key_as_before() {
        let root = temp_root("ipc-key");
        let folders = Folders::under(&root);
        let key_file = KeyFile::new(folders.config.join(KEY_FILE));
        let (made, made_rx) = mpsc::channel();
        let start = Start {
            simulate: true,
            hidden: true,
            folders: Box::new(move |_: &AppHandle<MockRuntime>| Ok(folders)),
            gif_source: counting(&FakeGifSource::new(), made),
            tray: Box::new(|_: &AppHandle<MockRuntime>, _, _: &Texts| {
                let synced: LiveSync = Box::new(|_| {});
                Ok(synced)
            }),
        };
        let app = mock_builder()
            .setup(move |app| setup(app, start))
            .invoke_handler(tauri::generate_handler![
                commands::klipy_key,
                commands::save_klipy_key,
            ])
            .build(context_with_the_window())
            .unwrap();

        let (seen, seen_rx) = mpsc::channel();
        app.run_return(move |app, event| {
            if !matches!(event, RunEvent::Ready) {
                return;
            }
            let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
                return;
            };
            let answers = [
                invoke(&window, "save_klipy_key", json!({"key": "not a key!"})),
                invoke(&window, "save_klipy_key", json!({"key": KEY})),
                invoke(&window, "klipy_key", json!({})),
            ];
            seen.send(answers).unwrap();
            // The window closes: the app ends.
            window.destroy().unwrap();
        });
        let [refused, saved, status] = seen_rx.recv().unwrap();
        let on_disk = key_file.load();
        let _ = std::fs::remove_dir_all(&root);

        let refused = refused.unwrap_err();
        assert_eq!(refused["code"], "invalidInput", "{refused}");
        assert!(!refused.to_string().contains("not a key"), "{refused}");
        let shown = json!({"configured": true, "last4": "cdef"});
        assert_eq!(saved.unwrap(), shown);
        assert_eq!(status.unwrap(), shown);
        assert!(on_disk.unwrap().is_some(), "the key was saved");
        assert!(made_rx.try_recv().is_err(), "saving a key asks nothing");
    }

    /// Review W1 of round 2, iter 4: a start that failed says which part
    /// failed, by the variant of Tauri's error or the kind of the I/O
    /// error, never by its text.
    #[test]
    fn a_failed_start_says_which_part_failed() {
        let runtime = serde_json::from_str::<u8>("x").unwrap_err();
        let setup: Box<dyn std::error::Error> = "the folders were not found".into();
        for (error, code) in [
            (tauri::Error::Runtime(runtime.into()), DiagCode::NoWindow),
            (tauri::Error::Setup(setup.into()), DiagCode::SetupFailed),
            (
                tauri::Error::PluginInitialization("dialog".into(), "no portal".into()),
                DiagCode::PluginNotStarted,
            ),
            (tauri::Error::WindowNotFound, DiagCode::NotStarted),
            (tauri::Error::UnknownPath, DiagCode::NotStarted),
        ] {
            assert_eq!(start_failure(&error), code, "{error:?}");
        }
        let io = io::Error::from;
        for (kind, code) in [
            (io::ErrorKind::WouldBlock, DiagCode::RefreshLoopNoResources),
            (io::ErrorKind::OutOfMemory, DiagCode::RefreshLoopNoResources),
            (io::ErrorKind::Other, DiagCode::RefreshLoopNotStarted),
        ] {
            assert_eq!(refresh_loop_failure(&io(kind)), code, "{kind:?}");
        }
        #[cfg(target_os = "linux")]
        for (kind, code) in [
            (io::ErrorKind::NotFound, DiagCode::DmabufRestartNoFile),
            (
                io::ErrorKind::PermissionDenied,
                DiagCode::DmabufRestartDenied,
            ),
            (
                io::ErrorKind::ArgumentListTooLong,
                DiagCode::DmabufRendererOn,
            ),
        ] {
            assert_eq!(restart_failure(&io(kind)), code, "{kind:?}");
        }
    }

    /// Review W2: the app's collection lives in its folder: one that cannot
    /// be used is said (`collectionUnavailable`), never replaced by one kept
    /// in memory, and the collection opens once it can be.
    #[test]
    fn the_app_never_keeps_the_collection_in_memory() {
        let root = temp_root("collection");
        let folders = Folders::under(&root);
        let folder = collection_dir(&folders.data);
        std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
        std::fs::write(&folder, b"not a folder").unwrap();
        let (made, made_rx) = mpsc::channel();
        let gifs = gifs(&folders, counting(&FakeGifSource::new(), made));
        let unusable = gifs.list(false).unwrap_err();
        assert_eq!(unusable.code(), "collectionUnavailable", "{unusable}");

        std::fs::remove_file(&folder).unwrap();
        assert!(gifs.list(false).unwrap().is_empty());
        assert!(folder.join("files").is_dir(), "opened in its folder");
        assert!(made_rx.try_recv().is_err(), "the collection is local");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_one_turns_the_simulation_on() {
        assert!(switch_on(Some(OsStr::new("1"))));
        assert!(!switch_on(Some(OsStr::new("0"))));
        assert!(!switch_on(Some(OsStr::new(""))));
        assert!(!switch_on(None));
    }

    #[test]
    fn simulated_adapters_have_the_turing_88_and_sensors() {
        let a = adapters(true);
        assert_eq!(discover_screens(a.bus.as_ref()).unwrap().len(), 1);
        let mut sensors = (a.sensors)(Default::default());
        assert!(!sensors.catalog().unwrap().is_empty());
    }

    #[test]
    fn closing_hides_while_live_and_asks_over_unsaved_edits() {
        assert_eq!(on_close(true, false), OnClose::Hide);
        assert_eq!(on_close(true, true), OnClose::Hide, "the edits stay");
        assert_eq!(on_close(false, true), OnClose::Ask);
        assert_eq!(on_close(false, false), OnClose::Close);
    }

    #[test]
    fn quitting_asks_over_unsaved_edits_only() {
        assert_eq!(on_quit(true), OnQuit::Ask);
        assert_eq!(on_quit(false), OnQuit::Exit);
    }

    /// The `allow-<command>` permissions of the window's capability, and
    /// the commands `build.rs` generates them for.
    fn permissions() -> (Vec<String>, Vec<String>) {
        let build = include_str!("../build.rs");
        let list = &build[build.find("const COMMANDS").unwrap()..];
        let list = &list[..list.find("];").unwrap()];
        let commands = list
            .split('"')
            .skip(1)
            .step_by(2)
            .map(|c| format!("allow-{}", c.replace('_', "-")))
            .collect();
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let allowed = capability["permissions"].as_array().unwrap().iter();
        let allowed = allowed
            .filter_map(|p| p.as_str())
            .filter(|p| p.starts_with("allow-"))
            .map(str::to_string)
            .collect();
        (commands, allowed)
    }

    /// The commands `generate_handler!` registers in [`run`].
    fn handled() -> Vec<String> {
        let source = include_str!("lib.rs");
        let list = &source[source.find("generate_handler![").unwrap()..];
        let list = &list[..list.find(']').unwrap()];
        list.split(',')
            .filter_map(|entry| entry.trim().rsplit_once("::"))
            .map(|(_, command)| format!("allow-{}", command.replace('_', "-")))
            .collect()
    }

    /// The commands of the GIF search and the collection, and the one that
    /// opens a fixed link (D-2026-10-01-gif-sticker-search-3..-5).
    const GIF_COMMANDS: [&str; 12] = [
        "klipy_key",
        "save_klipy_key",
        "remove_klipy_key",
        "search_gifs",
        "gif_preview",
        "collect_gif",
        "gif_collection",
        "rename_collected",
        "collected_users",
        "delete_collected",
        "use_collected",
        "open_link",
    ];

    #[test]
    fn every_command_is_allowed_by_name() {
        let (commands, allowed) = permissions();
        assert!(commands.contains(&"allow-run-plan".to_string()));
        for command in GIF_COMMANDS {
            let permission = format!("allow-{}", command.replace('_', "-"));
            assert!(commands.contains(&permission), "{command} in build.rs");
        }
        assert_eq!(commands, allowed, "build.rs and capabilities/default.json");
        let mut handled = handled();
        let mut listed = commands.clone();
        handled.sort_unstable();
        listed.sort_unstable();
        assert_eq!(handled, listed, "generate_handler! and build.rs");
    }

    #[test]
    fn the_starting_theme_fits_the_88_horizontally() {
        let theme = starting_theme();
        assert_eq!(theme.canvas, Size::new(1920, 480));
        assert_eq!(theme.orientation, Orientation::Landscape);
        assert!(theme.elements.is_empty());
    }

    // ------------------------------------------------- the source guard --

    /// Identifiers the studio's production code must not hold
    /// (D-2026-10-01-gif-sticker-search-10): each lets the app forge a user
    /// action (D-2026-10-01-gif-sticker-search-3).
    const FORGERIES: [&str; 9] = [
        // An invocation handed to a webview as if the window sent it, the
        // invoke key it must carry, and the invocation itself.
        "on_message",
        "invoke_key",
        "InvokeRequest",
        // A script run in the window (it can invoke a command, or press
        // Search): now, with a callback, through the platform's webview, or
        // injected at load.
        "eval",
        "eval_with_callback",
        "with_webview",
        "initialization_script",
        "js_init_script",
        // A page loaded in the window, which a `javascript:` URL makes a
        // script run there. The studio never navigates its window.
        "navigate",
    ];

    /// What an IPC invocation is made of, as Tauri hands it to an invoke
    /// handler (D-2026-10-01-gif-sticker-search-12): the invocation, its
    /// message, its body and the accessor of the body (what the window
    /// sent, the KLIPY key included). No production code names them: only
    /// Tauri reads an invocation, and a command its arguments.
    const INVOCATION_PARTS: [&str; 4] = ["Invoke", "InvokeMessage", "InvokeBody", "payload"];

    /// The one module that prints or logs (D-2026-10-01-gif-sticker-search-12).
    const DIAG_MODULE: &str = "diag.rs";

    /// The closed list of what [`DIAG_MODULE`] says, the one type (with
    /// `&'static str`) its functions take.
    const DIAG_CODE: &str = "DiagCode";

    /// Macros that print what they are given, called nowhere but in
    /// [`DIAG_MODULE`].
    const PRINT_MACROS: [&str; 5] = ["println", "eprintln", "print", "eprint", "dbg"];

    /// Macros that panic with the message they are given, which the panic
    /// hook prints: nowhere but in [`DIAG_MODULE`] with a message that is
    /// more than one string literal without a placeholder. One is spelled
    /// in two parts, so that the repository's check for unfinished-work
    /// markers does not read it as one.
    const PANICS: [&str; 4] = ["panic", "unreachable", concat!("to", "do"), "unimplemented"];

    /// Assertions whose message follows the condition.
    const ASSERTS: [&str; 2] = ["assert", "debug_assert"];

    /// Assertions that print their operands when they fail: nowhere but in
    /// [`DIAG_MODULE`], with or without a message.
    const COMPARISONS: [&str; 4] = [
        "assert_eq",
        "assert_ne",
        "debug_assert_eq",
        "debug_assert_ne",
    ];

    /// Methods (and paths) that panic printing the value they hold (a
    /// `Result`'s error): nowhere but in [`DIAG_MODULE`]. `panic_any`
    /// panics with any value, which the panic hook prints when it is text.
    const UNWRAPS: [&str; 5] = ["unwrap", "expect", "unwrap_err", "expect_err", "panic_any"];

    /// Crates that log, whose paths no module but [`DIAG_MODULE`] uses.
    const LOGGERS: [&str; 2] = ["log", "tracing"];

    /// The loggers' macros, which no module but [`DIAG_MODULE`] calls by
    /// their bare names either (imported, or by `#[macro_use]`).
    const LOG_MACROS: [&str; 7] = ["trace", "debug", "info", "warn", "error", "event", "log"];

    /// The process's output streams, which no module but [`DIAG_MODULE`]
    /// names (`writeln!(std::io::stderr(), …)` prints), whatever the case
    /// and the suffix (`Stderr`, `StdoutLock`).
    const STDIO: [&str; 2] = ["stdout", "stderr"];

    /// Files that are the output streams, which no literal of production
    /// code but [`DIAG_MODULE`]'s names (compared without case).
    const STREAM_FILES: [&str; 6] = [
        "/dev/stdout",
        "/dev/stderr",
        "/dev/fd/",
        "/proc/self/fd/",
        "conout$",
        "conerr$",
    ];

    /// The module of the `#[tauri::command]` functions: the window's
    /// invocations enter there.
    const COMMAND_MODULE: &str = "commands.rs";

    /// The window's invocation, as a command takes it
    /// (`tauri::ipc::Request`): its body is what the window sent.
    const INVOCATION: &str = "Request";

    /// The macro that lists the commands the window may invoke: in `run`,
    /// the one place that names a command function but its definition.
    const HANDLER: &str = "generate_handler";

    /// The prefix of the macro `#[tauri::command]` makes for each command
    /// (`__cmd__search_gifs!`), which `generate_handler!` calls.
    const GENERATED: &str = "__cmd__";

    /// The one accessor that reads a [`KlipyKey`]'s text, defined in
    /// [`KEY_MODULE`].
    const KEY_READER: &str = "expose_secret";

    /// The key file's serializer in [`KEY_MODULE`], the accessor's one
    /// reader there: serde calls it, through the attribute of the file
    /// JSON's `key`, and no code names it but its definition.
    const KEY_WRITER: &str = "write_key";

    /// The one call of [`KEY_WRITER`] that takes the key's text.
    const KEY_WRITE: &str = "serialize_str";

    /// The key's module: its type, its accessor, its file.
    const KEY_MODULE: &str = "gifs/key.rs";

    /// The proof that the user asked, made by `UserAsked::of` only.
    const PROOF: &str = "UserAsked";

    /// The proof's module, the one place that defines it.
    const PROOF_MODULE: &str = "gifs/asked.rs";

    /// The commands of `commands.rs` that take the window's `Request` and
    /// make a [`UserAsked`] of it, the only ones that may.
    const PROOF_COMMANDS: [&str; 3] = ["search_gifs", "gif_preview", "collect_gif"];

    /// The URL scheme that runs a script in the page that loads it (its
    /// `:` is added where it is used, so that no literal here is one).
    const SCRIPT_SCHEME: &str = "javascript";

    /// What no literal of the studio holds, tests included: the window's
    /// IPC object, then KLIPY's API and file hosts (they are
    /// `bezel-klipy`'s). Made here, so that this file's literals do not
    /// hold them.
    fn markers() -> [String; 3] {
        [
            format!("__{}", "TAURI"),
            format!("{}.klipy.com", "api"),
            format!("{}.klipy.com", "static"),
        ]
    }

    /// A Rust file of the studio, as the source guard reads it.
    struct Source {
        /// Its path under `src/`, `/`-separated.
        name: String,
        /// The whole file.
        text: String,
        /// Its syntax tree.
        tree: syn::File,
    }

    impl Source {
        /// Parses `text` as the file `name`; why it cannot be classified
        /// when it is not Rust that `syn` reads.
        fn new(name: &str, text: &str) -> Result<Self, String> {
            let tree =
                syn::parse_file(text).map_err(|e| format!("{name}: cannot be parsed: {e}"))?;
            Ok(Self {
                name: name.into(),
                text: text.into(),
                tree,
            })
        }

        /// The `#[tauri::command]` functions its production code defines.
        fn commands(&self) -> Vec<String> {
            self.production(&[]).defined
        }

        /// Its production code read, the studio's command functions being
        /// `commands`: items under `#[cfg(test)]` left out.
        fn production<'s>(&'s self, commands: &'s [String]) -> Reader<'s> {
            let mut reader = Reader::new(&self.name, Reading::Production, commands);
            if reader.in_diag() {
                reader.own_types = self.tree.items.iter().filter_map(enum_name).collect();
            }
            reader.visit_file(&self.tree);
            if reader.klipy_named > reader.made.len() + reader.klipy_imported {
                reader.refuse("KLIPY's client named outside its import");
            }
            let in_readers: Vec<String> = reader
                .macros
                .iter()
                .filter(|(function, _)| reader.reads.contains(function))
                .map(|(function, called)| {
                    format!("`{called}!` in `{function}`, which reads the KLIPY key")
                })
                .collect();
            for problem in in_readers {
                reader.refuse(problem);
            }
            reader
        }

        /// Its literals (escapes read) that hold a marker, tests included,
        /// and KLIPY's hosts anywhere in its text, comments too.
        fn literals(&self) -> Vec<String> {
            let mut reader = Reader::new(&self.name, Reading::Literals, &[]);
            reader.visit_file(&self.tree);
            for host in markers().iter().skip(1) {
                if self.text.contains(host.as_str()) {
                    reader.refuse(format_args!("names {host}"));
                }
            }
            reader.problems
        }
    }

    /// What a reading of a file covers.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Reading {
        /// Its literals, tests included.
        Literals,
        /// Its production code, against every other rule.
        Production,
    }

    /// The source guard's reading of one file's syntax tree, the tokens of
    /// macro calls and attributes (which `syn` leaves unparsed) included.
    struct Reader<'f> {
        file: &'f str,
        reading: Reading,
        /// The studio's command functions, which only the list of
        /// `generate_handler!` in `run` may name.
        commands: &'f [String],
        /// The functions around what is read, the outermost first.
        within: Vec<String>,
        /// Whether the outermost of them is a free `#[tauri::command]`
        /// function.
        command: bool,
        /// How deep in macro calls' tokens what is read is.
        in_macro: usize,
        /// Whether what is read is the list of `generate_handler!` in
        /// `run` in `lib.rs`.
        in_handler: bool,
        /// Whether what is read is the invocation's type imported by its
        /// own name, at the top of the command module or the proof's.
        importing: bool,
        /// Every function read, in order.
        functions: Vec<String>,
        /// The free `#[tauri::command]` functions it defines.
        defined: Vec<String>,
        /// The commands the list of `generate_handler!` in `run` names.
        handled: Vec<String>,
        /// The outermost function around each `generate_handler!` call.
        handlers: Vec<String>,
        /// The outermost function around each `KlipyClient::new`.
        made: Vec<String>,
        /// The outermost function around each read of the key's text, its
        /// accessor's definition included.
        reads: Vec<String>,
        /// The outermost function around each `UserAsked::of` it accepts.
        asked: Vec<String>,
        /// Each macro called in a function: the outermost function and the
        /// macro's path.
        macros: Vec<(String, String)>,
        /// `KlipyClient` imported by its name (`use …::KlipyClient;`).
        klipy_imported: usize,
        /// `KlipyClient` named at all.
        klipy_named: usize,
        /// The names bound where what is read is (parameters, `let`, closure
        /// and `match` patterns, ...), by scope, the outermost first: in the
        /// command module, a name alone bound there is a local, not the
        /// command of the same name (review W1 of round 2).
        scopes: Vec<Vec<String>>,
        /// In [`DIAG_MODULE`], the enums it defines: the only types (with
        /// `Self` and `tracing`) its paths start with.
        own_types: Vec<String>,
        /// Whether what is read is in `impl DiagCode` in [`DIAG_MODULE`]:
        /// its methods take `self`.
        in_code_impl: bool,
        /// The functions [`DIAG_MODULE`] defines.
        diag_functions: Vec<String>,
        /// What it holds that it must not, or that cannot be classified.
        problems: Vec<String>,
    }

    impl<'f> Reader<'f> {
        fn new(file: &'f str, reading: Reading, commands: &'f [String]) -> Self {
            Self {
                file,
                reading,
                commands,
                within: Vec::new(),
                command: false,
                in_macro: 0,
                in_handler: false,
                importing: false,
                functions: Vec::new(),
                defined: Vec::new(),
                handled: Vec::new(),
                handlers: Vec::new(),
                made: Vec::new(),
                reads: Vec::new(),
                asked: Vec::new(),
                macros: Vec::new(),
                klipy_imported: 0,
                klipy_named: 0,
                scopes: Vec::new(),
                own_types: Vec::new(),
                in_code_impl: false,
                diag_functions: Vec::new(),
                problems: Vec::new(),
            }
        }

        fn refuse(&mut self, what: impl std::fmt::Display) {
            self.problems.push(format!("{}: {what}", self.file));
        }

        fn production(&self) -> bool {
            self.reading == Reading::Production
        }

        /// Whether the file is one of the GIF and key modules or the
        /// command module, which do not panic either.
        fn quiet(&self) -> bool {
            self.file == "gifs.rs" || self.file.starts_with("gifs/") || self.file == COMMAND_MODULE
        }

        /// Whether the file is [`DIAG_MODULE`], the one that prints and logs.
        fn in_diag(&self) -> bool {
            self.file == DIAG_MODULE
        }

        /// Whether production code outside [`DIAG_MODULE`] is read: the
        /// code that neither prints nor logs.
        fn silent(&self) -> bool {
            self.production() && !self.in_diag()
        }

        /// Reads, with `read`, code where the names `pattern` binds are
        /// bound, in a new scope.
        fn scoped(&mut self, patterns: &[&Pat], read: impl FnOnce(&mut Self)) {
            let mut names = Vec::new();
            for pattern in patterns {
                bindings(pattern, &mut names);
            }
            self.scopes.push(names);
            read(self);
            self.scopes.pop();
        }

        /// Binds the names `pattern` binds in the innermost scope (a `let`,
        /// a condition's `let`): bound from what follows on.
        fn bind(&mut self, pattern: &Pat) {
            if let Some(scope) = self.scopes.last_mut() {
                bindings(pattern, scope);
            }
        }

        /// Whether `name` is bound where what is read is.
        fn bound(&self, name: &str) -> bool {
            self.scopes.iter().flatten().any(|bound| bound == name)
        }

        /// Reads, with `read`, a function's body, its parameters bound, in
        /// scopes of its own: an item does not see the locals around it.
        fn body(&mut self, inputs: &Punctuated<FnArg, Comma>, read: impl FnOnce(&mut Self)) {
            let outer = std::mem::take(&mut self.scopes);
            let parameters: Vec<&Pat> = inputs
                .iter()
                .filter_map(|input| match input {
                    FnArg::Typed(typed) => Some(&*typed.pat),
                    FnArg::Receiver(_) => None,
                })
                .collect();
            self.scoped(&parameters, read);
            self.scopes = outer;
        }

        /// A condition (`if`, `while`): what each `let` in it binds is bound
        /// in the conditions after it and in the scope around (the branch
        /// it guards).
        fn condition(&mut self, condition: &Expr) {
            match condition {
                Expr::Let(binding) => {
                    for attribute in &binding.attrs {
                        self.visit_attribute(attribute);
                    }
                    self.visit_expr(&binding.expr);
                    self.visit_pat(&binding.pat);
                    self.bind(&binding.pat);
                }
                Expr::Binary(both) if matches!(both.op, BinOp::And(_)) => {
                    self.condition(&both.left);
                    self.condition(&both.right);
                }
                other => self.visit_expr(other),
            }
        }

        /// A function's signature in [`DIAG_MODULE`]: no generics, and only
        /// a `DiagCode` (`self` in `impl DiagCode`) or a `&'static str`
        /// taken, so no value of the app's reaches what it says.
        fn diag_signature(&mut self, signature: &Signature) {
            let name = signature.ident.unraw().to_string();
            self.diag_functions.push(name.clone());
            if !signature.generics.params.is_empty() || signature.generics.where_clause.is_some() {
                self.refuse(format_args!(
                    "`fn {name}` in `{DIAG_MODULE}` is generic: it takes only `{DIAG_CODE}` and `&'static str`"
                ));
            }
            for input in &signature.inputs {
                let allowed = match input {
                    FnArg::Receiver(receiver) => {
                        self.in_code_impl && receiver.colon_token.is_none()
                    }
                    FnArg::Typed(typed) => self.diag_type(&typed.ty),
                };
                if !allowed {
                    let taken = match input {
                        FnArg::Typed(typed) => match &*typed.pat {
                            Pat::Ident(parameter) => parameter.ident.unraw().to_string(),
                            _ => "a pattern".into(),
                        },
                        FnArg::Receiver(_) => "self".into(),
                    };
                    self.refuse(format_args!(
                        "`fn {name}` in `{DIAG_MODULE}` takes `{taken}` of another type: it takes \
                         only `{DIAG_CODE}` and `&'static str`"
                    ));
                }
            }
        }

        /// Whether a function of [`DIAG_MODULE`] may take `ty`: a
        /// `DiagCode` (`Self` in `impl DiagCode`), or a `&'static str`.
        fn diag_type(&self, ty: &Type) -> bool {
            match ty {
                Type::Path(path) if path.qself.is_none() => {
                    let names = segments(&path.path);
                    let plain = path.path.segments.iter().all(|s| s.arguments.is_none());
                    plain && (names == [DIAG_CODE] || (self.in_code_impl && names == ["Self"]))
                }
                Type::Reference(reference) => {
                    reference.mutability.is_none()
                        && reference
                            .lifetime
                            .as_ref()
                            .is_some_and(|lifetime| lifetime.ident == "static")
                        && matches!(&*reference.elem, Type::Path(text)
                            if text.qself.is_none() && text.path.is_ident("str"))
                }
                Type::Paren(inner) => self.diag_type(&inner.elem),
                _ => false,
            }
        }

        /// An item of [`DIAG_MODULE`]'s production code: it holds only
        /// closed enums (no data), functions, `impl DiagCode`, constants and
        /// imports of `tracing`; no `static`, no type that holds data, no
        /// trait, no macro of its own, no module.
        fn diag_item(&mut self, item: &Item) {
            let refused = match item {
                Item::Enum(codes) => {
                    if !codes.generics.params.is_empty() {
                        self.refuse(format_args!(
                            "`{}` in `{DIAG_MODULE}` is generic",
                            codes.ident
                        ));
                    }
                    for variant in &codes.variants {
                        if !matches!(variant.fields, Fields::Unit) {
                            self.refuse(format_args!(
                                "`{}::{}` in `{DIAG_MODULE}` carries data: what it says is closed",
                                codes.ident, variant.ident
                            ));
                        }
                    }
                    None
                }
                Item::Impl(block) => {
                    let of_code = matches!(&*block.self_ty, Type::Path(path)
                        if path.qself.is_none() && path.path.is_ident(DIAG_CODE));
                    let inherent = block.trait_.is_none() && block.generics.params.is_empty();
                    (!(of_code && inherent)).then_some("an `impl` other than `impl DiagCode`")
                }
                Item::Use(import) => {
                    let roots = use_roots(&import.tree);
                    let logger = roots.iter().all(|root| LOGGERS.iter().any(|l| *root == l));
                    (!logger || roots.is_empty()).then_some("an import of another than a logger")
                }
                Item::Fn(_) | Item::Const(_) => None,
                Item::Static(_) => Some("a `static`"),
                Item::Struct(_) | Item::Union(_) => Some("a type that holds data"),
                Item::Trait(_) | Item::TraitAlias(_) => Some("a trait"),
                Item::Macro(_) => Some("a macro's item (`macro_rules!`, `thread_local!`, ...)"),
                Item::Mod(_) => Some("a module"),
                _ => Some("an item it does not need"),
            };
            if let Some(refused) = refused {
                self.refuse(format_args!(
                    "{refused} in `{DIAG_MODULE}`: it says only what it is given"
                ));
            }
        }

        /// A path in [`DIAG_MODULE`]: one of two or more names starts with
        /// `tracing`, `Self` or an enum of its own, never another module of
        /// the app (`crate`, `super`) or the standard library's I/O.
        fn diag_path(&mut self, segments: &[String]) {
            let Some(first) = segments.first().filter(|_| segments.len() > 1) else {
                return;
            };
            let own = first == "Self" || self.own_types.contains(first);
            if !own && !LOGGERS.contains(&first.as_str()) {
                self.refuse(format_args!(
                    "`{}` in `{DIAG_MODULE}`: it uses only a logger and its own items",
                    segments.join("::")
                ));
            }
        }

        /// A macro called, `arguments` its tokens when known: outside
        /// [`DIAG_MODULE`], a panic or an assertion that formats its message
        /// (more than one string literal without a placeholder) or prints
        /// its operands; in the GIF, key and command modules, any panic or
        /// assertion.
        fn panics(&mut self, segments: &[String], arguments: Option<TokenStream>) {
            if !self.silent() {
                return;
            }
            let last = segments.last().map_or("", String::as_str);
            let path = segments.join("::");
            let panics = PANICS.contains(&last);
            let asserts = ASSERTS.contains(&last);
            let compares = COMPARISONS.contains(&last);
            if self.quiet() && (panics || asserts || compares) {
                self.refuse(format_args!(
                    "`{path}!` panics in a GIF, key or command module"
                ));
            }
            if compares {
                self.refuse(format_args!(
                    "`{path}!` prints its operands when it fails, outside `{DIAG_MODULE}`"
                ));
            }
            let skipped = usize::from(asserts);
            let message = arguments.map(|tokens| arguments_of(tokens).into_iter().skip(skipped));
            let formats = message.is_some_and(|mut parts| match (parts.next(), parts.next()) {
                (None, _) => false,
                (Some(only), None) => !is_plain_text(&only),
                _ => true,
            });
            if (panics || asserts) && formats {
                self.refuse(format_args!(
                    "`{path}!` formats its message outside `{DIAG_MODULE}`: the panic hook prints it"
                ));
            }
        }

        /// A method called, or a function named by a path: one that panics
        /// printing the value it holds ([`UNWRAPS`]) outside [`DIAG_MODULE`].
        fn panics_printing(&mut self, name: &str) {
            if self.silent() && UNWRAPS.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` panics printing what it holds, outside `{DIAG_MODULE}`"
                ));
            }
        }

        /// Whether an item marked with `attributes` is left out: a test
        /// item, in a reading of production code.
        fn skips(&self, attributes: &[Attribute]) -> bool {
            self.production() && attributes.iter().any(is_test)
        }

        /// A type alias of `ty` (`type … = ty`): refused for the proof.
        fn alias(&mut self, ty: &Type) {
            if self.production() && is_proof(ty) {
                self.refuse(format_args!("`{PROOF}` renamed: it is named as itself"));
            }
        }

        /// Reads, with `read`, the function `name`, a free
        /// `#[tauri::command]` one when `command`.
        fn function(&mut self, name: &Ident, command: bool, read: impl FnOnce(&mut Self)) {
            let name = name.unraw().to_string();
            self.functions.push(name.clone());
            if self.within.is_empty() {
                self.command = command;
            }
            self.within.push(name);
            read(self);
            self.within.pop();
        }

        /// The outermost function around what is read.
        fn outer(&self) -> Option<&str> {
            self.within.first().map(String::as_str)
        }

        /// Whether what is read is in the body of one of [`PROOF_COMMANDS`].
        fn in_gif_command(&self) -> bool {
            self.file == COMMAND_MODULE
                && self.command
                && self.outer().is_some_and(|f| PROOF_COMMANDS.contains(&f))
        }

        /// Whether the invocation's type ([`INVOCATION`]) may be named where
        /// what is read is: in one of [`PROOF_COMMANDS`], in `UserAsked::of`
        /// that takes it, or imported by its own name at the top of their
        /// modules.
        fn takes_the_invocation(&self) -> bool {
            self.in_gif_command()
                || (self.file == PROOF_MODULE && self.within == ["of"])
                || self.importing
        }

        /// A read of the key's text, accepted or not.
        fn read_key(&mut self) {
            if let Some(outer) = self.outer().map(str::to_string) {
                self.reads.push(outer);
            }
        }

        /// An identifier, its `r#` removed.
        fn ident(&mut self, name: &str) {
            if !self.production() {
                return;
            }
            if FORGERIES.contains(&name) {
                self.refuse(format_args!("`{name}` in production code"));
            }
            if INVOCATION_PARTS.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` (an invocation the window sent) in production code: only Tauri \
                     reads one"
                ));
            }
            if name == "KlipyClient" {
                self.klipy_named += 1;
            }
            if name == KEY_READER {
                self.key_reader();
            }
            if name == KEY_WRITER && !(self.file == KEY_MODULE && self.within == [KEY_WRITER]) {
                self.refuse(format_args!(
                    "`{KEY_WRITER}`, the key file's serializer, named outside its definition: \
                     serde calls it, for the key file's `key` only"
                ));
            }
            if name == PROOF && self.in_macro > 0 && !self.in_gif_command() {
                self.refuse(format_args!(
                    "`{PROOF}` inside a macro call outside the GIF commands"
                ));
            }
            if name == INVOCATION && !self.takes_the_invocation() {
                self.refuse(format_args!(
                    "`{INVOCATION}` (the window's invocation) named outside the GIF commands \
                     that take it (`{}` in `{COMMAND_MODULE}`) and `{PROOF}::of`",
                    PROOF_COMMANDS.join("`, `")
                ));
            }
            let lower = name.to_ascii_lowercase();
            if self.silent() && STDIO.iter().any(|stream| lower.starts_with(stream)) {
                self.refuse(format_args!(
                    "`{name}` names an output stream outside `{DIAG_MODULE}`"
                ));
            }
        }

        /// A path that names a command function: accepted in the list of
        /// `generate_handler!` in `run`, refused anywhere else, so that a
        /// command is entered only through IPC. It names one when it ends
        /// with a command's name after the command module (`commands::…`,
        /// `crate::commands::…`), or in the command module after nothing,
        /// `self` or `super`, or when it is the macro Tauri makes for a
        /// command (`__cmd__…`, exported at the crate's root). Elsewhere a
        /// name alone is not the command's: the core's use cases and the
        /// backend's methods share their names, and an import of the
        /// command is refused ([`Reader::imported`]).
        fn command_named(&mut self, segments: &[String]) {
            let Some((last, parent)) = segments.split_last() else {
                return;
            };
            let generated = last.strip_prefix(GENERATED);
            let name = generated.unwrap_or(last);
            if !self.commands.iter().any(|command| command == name) {
                return;
            }
            let in_module = parent.last().is_some_and(|module| module == "commands");
            let here = self.file == COMMAND_MODULE
                && parent
                    .iter()
                    .all(|module| module == "self" || module == "super");
            // A name alone that a parameter or a pattern binds where it is
            // read is that local, not the command function.
            let local = parent.is_empty() && generated.is_none() && self.bound(name);
            if !(generated.is_some() || in_module || here) || local {
                return;
            }
            if self.in_handler {
                self.handled.push(name.to_string());
            } else {
                self.refuse(format_args!(
                    "`{name}` names a command function outside its definition and the list of \
                     `{HANDLER}!` in `run`: a command is entered only through IPC"
                ));
            }
        }

        /// What a `use` imports, `path` (its last name the item's, `*` for
        /// a glob), renamed when `renamed`: a command function is not
        /// imported, nor the command module renamed or imported by a glob.
        fn imported(&mut self, path: &[String], renamed: bool) {
            if !self.production() {
                return;
            }
            let last = path.last().map_or("", String::as_str);
            let module = path.len() > 1 && path[path.len() - 2] == "commands";
            if renamed && (last == "commands" || (last == "self" && module)) {
                self.refuse("the command module renamed: it is named as itself");
            }
            if last == "*" && module {
                self.refuse("the command module imported by a glob");
            }
            if path.len() > 1 {
                self.panics_printing(last);
            }
            self.command_named(path);
        }

        /// The key's accessor, named where no rule accepts it (the two
        /// accepted reads are not read as identifiers): refused, but in its
        /// own definition.
        fn key_reader(&mut self) {
            self.read_key();
            if self.file == KEY_MODULE && self.within == [KEY_READER] {
                return;
            }
            if self.in_macro > 0 {
                self.refuse(format_args!(
                    "`{KEY_READER}` reads the KLIPY key inside a macro call"
                ));
            } else {
                self.refuse(format_args!(
                    "`{KEY_READER}` reads the KLIPY key outside its two uses: an argument of \
                     `KlipyClient::new` in `klipy_source`, and of `{KEY_WRITE}` in \
                     `{KEY_WRITER}`, the key file's serializer"
                ));
            }
        }

        /// A path (`a::b::c`, `r#` removed), a macro's when `called`.
        fn path(&mut self, segments: &[String], called: bool) {
            if !self.production() {
                return;
            }
            if segments
                .windows(2)
                .any(|pair| pair == ["KlipyClient", "new"])
            {
                let around = self.within.first().cloned().unwrap_or_default();
                self.made.push(around);
            }
            if segments.windows(2).any(|pair| pair == [PROOF, "of"]) {
                let command = self.outer().filter(|_| self.in_gif_command());
                match command.map(str::to_string) {
                    Some(command) => self.asked.push(command),
                    None => self.refuse(format_args!(
                        "`{PROOF}::of` outside the GIF commands that take the window's \
                         `Request` (`{}` in `commands.rs`)",
                        PROOF_COMMANDS.join("`, `")
                    )),
                }
            }
            let last = segments.last().map_or("", String::as_str);
            if called && let Some(outer) = self.outer().map(str::to_string) {
                self.macros.push((outer, segments.join("::")));
            }
            if called && last == "include" {
                self.refuse("`include!` in production code: it cannot be classified");
            }
            let logs = segments.len() > 1 && LOGGERS.contains(&segments[0].as_str());
            let prints = called && (PRINT_MACROS.contains(&last) || LOG_MACROS.contains(&last));
            if self.silent() && (logs || prints) {
                self.refuse(format_args!(
                    "`{}` prints or logs outside `{DIAG_MODULE}`",
                    segments.join("::")
                ));
            }
            if !called && segments.len() > 1 {
                self.panics_printing(last);
            }
            if self.in_diag() {
                self.diag_path(segments);
            }
            self.command_named(segments);
        }

        /// A literal: its value, escapes read, holds no marker.
        fn literal(&mut self, literal: &Lit) {
            let value = match literal {
                Lit::Str(text) => text.value(),
                Lit::ByteStr(bytes) => String::from_utf8_lossy(&bytes.value()).into_owned(),
                Lit::CStr(text) => text.value().to_string_lossy().into_owned(),
                Lit::Verbatim(other) => other.to_string(),
                _ => return,
            };
            for marker in markers() {
                if value.contains(&marker) {
                    self.refuse(format_args!("a literal holds {marker}"));
                }
            }
            if is_script_url(&value) {
                self.refuse(format_args!("a literal is a `{SCRIPT_SCHEME}:` URL"));
            }
            let lower = value.to_ascii_lowercase();
            if self.silent() && STREAM_FILES.iter().any(|file| lower.contains(file)) {
                self.refuse(format_args!(
                    "a literal names an output stream's file outside `{DIAG_MODULE}`"
                ));
            }
        }

        /// Tokens `syn` leaves unparsed (a macro call's, an attribute's):
        /// each path, called as a macro when a `!` follows it, each of its
        /// identifiers, each literal, and the same inside each group. A
        /// name after a `.` (a field, a method) is an identifier, not a
        /// path.
        fn tokens(&mut self, tokens: TokenStream) {
            let tokens: Vec<TokenTree> = tokens.into_iter().collect();
            let mut at = 0;
            while let Some(token) = tokens.get(at) {
                match token {
                    TokenTree::Group(group) => self.tokens(group.stream()),
                    TokenTree::Literal(literal) => self.literal(&Lit::new(literal.clone())),
                    TokenTree::Punct(_) => {}
                    TokenTree::Ident(_) => {
                        let (path, next) = path_at(&tokens, at);
                        for segment in &path {
                            self.ident(segment);
                        }
                        let member = is_member(&tokens, at);
                        let called = is_punct(tokens.get(next), '!');
                        if member && is_call(tokens.get(next)) {
                            self.panics_printing(&path[0]);
                        }
                        if !member {
                            self.path(&path, called);
                        }
                        if !member && called {
                            let arguments = match tokens.get(next + 1) {
                                Some(TokenTree::Group(group)) => Some(group.stream()),
                                _ => None,
                            };
                            self.panics(&path, arguments);
                        }
                        at = next;
                        continue;
                    }
                }
                at += 1;
            }
        }
    }

    impl<'ast> Visit<'ast> for Reader<'_> {
        fn visit_item(&mut self, item: &'ast Item) {
            if self.skips(item_attributes(item)) {
                return;
            }
            if self.production() && self.in_diag() {
                self.diag_item(item);
            }
            visit::visit_item(self, item);
        }

        fn visit_impl_item(&mut self, item: &'ast ImplItem) {
            if !self.skips(impl_item_attributes(item)) {
                visit::visit_impl_item(self, item);
            }
        }

        fn visit_item_fn(&mut self, item: &'ast ItemFn) {
            let command = item.attrs.iter().any(is_command);
            if command {
                self.defined.push(item.sig.ident.unraw().to_string());
            }
            if self.production() && self.in_diag() {
                self.diag_signature(&item.sig);
            }
            let main = self.file == "main.rs" && item.sig.ident == "main";
            if self.production() && main && returns_a_result(&item.sig.output) {
                self.refuse("`main` returns a `Result`: its error is printed when it fails");
            }
            self.function(&item.sig.ident, command, |reader| {
                reader.body(&item.sig.inputs, |reader| {
                    visit::visit_item_fn(reader, item)
                });
            });
        }

        fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
            if self.production() && self.in_diag() {
                self.diag_signature(&item.sig);
            }
            self.function(&item.sig.ident, false, |reader| {
                reader.body(&item.sig.inputs, |reader| {
                    visit::visit_impl_item_fn(reader, item);
                });
            });
        }

        fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
            if self.production() && self.in_diag() {
                self.diag_signature(&item.sig);
            }
            self.function(&item.sig.ident, false, |reader| {
                reader.body(&item.sig.inputs, |reader| {
                    visit::visit_trait_item_fn(reader, item);
                });
            });
        }

        /// A block: each `let` binds its names from the next statement on
        /// (its value and its `else` read before).
        fn visit_block(&mut self, block: &'ast Block) {
            self.scoped(&[], |reader| {
                for statement in &block.stmts {
                    let Stmt::Local(local) = statement else {
                        reader.visit_stmt(statement);
                        continue;
                    };
                    for attribute in &local.attrs {
                        reader.visit_attribute(attribute);
                    }
                    if let Some(init) = &local.init {
                        reader.visit_expr(&init.expr);
                        if let Some((_, otherwise)) = &init.diverge {
                            reader.visit_expr(otherwise);
                        }
                    }
                    reader.visit_pat(&local.pat);
                    reader.bind(&local.pat);
                }
            });
        }

        fn visit_expr_closure(&mut self, closure: &'ast ExprClosure) {
            let inputs: Vec<&Pat> = closure.inputs.iter().collect();
            self.scoped(&inputs, |reader| visit::visit_expr_closure(reader, closure));
        }

        fn visit_arm(&mut self, arm: &'ast Arm) {
            self.scoped(&[&arm.pat], |reader| visit::visit_arm(reader, arm));
        }

        fn visit_expr_for_loop(&mut self, looped: &'ast ExprForLoop) {
            for attribute in &looped.attrs {
                self.visit_attribute(attribute);
            }
            self.visit_expr(&looped.expr);
            self.scoped(&[&looped.pat], |reader| {
                reader.visit_pat(&looped.pat);
                reader.visit_block(&looped.body);
            });
        }

        fn visit_expr_if(&mut self, branch: &'ast ExprIf) {
            for attribute in &branch.attrs {
                self.visit_attribute(attribute);
            }
            self.scoped(&[], |reader| {
                reader.condition(&branch.cond);
                reader.visit_block(&branch.then_branch);
            });
            if let Some((_, otherwise)) = &branch.else_branch {
                self.visit_expr(otherwise);
            }
        }

        fn visit_expr_while(&mut self, looped: &'ast ExprWhile) {
            for attribute in &looped.attrs {
                self.visit_attribute(attribute);
            }
            self.scoped(&[], |reader| {
                reader.condition(&looped.cond);
                reader.visit_block(&looped.body);
            });
        }

        /// A method call: one that panics printing what it holds is
        /// refused outside [`DIAG_MODULE`]. In the key file's serializer
        /// ([`KEY_WRITER`]), the key's text as the one argument of
        /// [`KEY_WRITE`] is accepted, its key read as an expression.
        fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
            let method = call.method.unraw().to_string();
            self.panics_printing(&method);
            let writes_the_key = self.production()
                && self.file == KEY_MODULE
                && self.within == [KEY_WRITER]
                && method == KEY_WRITE
                && call.turbofish.is_none()
                && call.args.len() == 1;
            let key = call.args.first().and_then(key_text);
            let Some(key) = key.filter(|_| writes_the_key) else {
                visit::visit_expr_method_call(self, call);
                return;
            };
            for attribute in &call.attrs {
                self.visit_attribute(attribute);
            }
            self.visit_expr(&call.receiver);
            self.visit_ident(&call.method);
            self.read_key();
            self.visit_expr(key);
        }

        fn visit_attribute(&mut self, attribute: &'ast Attribute) {
            let path = attribute.path();
            let set_by_cfg_attr = match &attribute.meta {
                Meta::List(list) => path.is_ident("cfg_attr") && names(list.tokens.clone(), "path"),
                _ => false,
            };
            if self.production() && (path.is_ident("path") || set_by_cfg_attr) {
                self.refuse("a module read from another path: it cannot be classified");
            }
            let derives_more = match &attribute.meta {
                Meta::List(list) if path.is_ident("derive") => list
                    .tokens
                    .clone()
                    .into_iter()
                    .any(|token| matches!(token, TokenTree::Ident(name) if name != "Debug")),
                _ => false,
            };
            if self.production() && self.file == PROOF_MODULE && derives_more {
                self.refuse(format_args!(
                    "`{PROOF_MODULE}` derives more than `Debug`: a `{PROOF}` made another way"
                ));
            }
            visit::visit_attribute(self, attribute);
        }

        /// A call; the key's text as an argument of `KlipyClient::new` in
        /// `klipy_source` is accepted, its key read as an expression.
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            let makes_the_client = self.production()
                && self.file == "lib.rs"
                && self.outer() == Some("klipy_source")
                && matches!(&*call.func, Expr::Path(func)
                    if func.qself.is_none() && ends_with(&func.path, ["KlipyClient", "new"]));
            if !makes_the_client {
                visit::visit_expr_call(self, call);
                return;
            }
            for attribute in &call.attrs {
                self.visit_attribute(attribute);
            }
            self.visit_expr(&call.func);
            for argument in &call.args {
                match key_text(argument) {
                    Some(key) => {
                        self.read_key();
                        self.visit_expr(key);
                    }
                    None => self.visit_expr(argument),
                }
            }
        }

        /// A struct literal: one in the proof's module makes a proof, which
        /// only `UserAsked::of` may.
        fn visit_expr_struct(&mut self, literal: &'ast ExprStruct) {
            if self.production() && self.file == PROOF_MODULE && self.outer() != Some("of") {
                self.refuse(format_args!(
                    "a struct literal in `{PROOF_MODULE}` outside `{PROOF}::of`: a `{PROOF}` \
                     made another way"
                ));
            }
            visit::visit_expr_struct(self, literal);
        }

        fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
            if self.production() && self.file != PROOF_MODULE && is_proof(&item.self_ty) {
                self.refuse(format_args!(
                    "an `impl` for `{PROOF}` outside `{PROOF_MODULE}`"
                ));
            }
            self.in_code_impl = self.in_diag()
                && item.trait_.is_none()
                && matches!(&*item.self_ty, Type::Path(path) if path.path.is_ident(DIAG_CODE));
            visit::visit_item_impl(self, item);
            self.in_code_impl = false;
        }

        fn visit_item_type(&mut self, item: &'ast ItemType) {
            self.alias(&item.ty);
            visit::visit_item_type(self, item);
        }

        fn visit_impl_item_type(&mut self, item: &'ast ImplItemType) {
            self.alias(&item.ty);
            visit::visit_impl_item_type(self, item);
        }

        fn visit_use_rename(&mut self, rename: &'ast UseRename) {
            if self.production() && rename.ident.unraw() == PROOF {
                self.refuse(format_args!("`{PROOF}` renamed: it is named as itself"));
            }
            visit::visit_use_rename(self, rename);
        }

        fn visit_qself(&mut self, qself: &'ast QSelf) {
            if self.production() && is_proof(&qself.ty) {
                self.refuse(format_args!(
                    "`{PROOF}` in a qualified path (`<{PROOF}>::…`)"
                ));
            }
            visit::visit_qself(self, qself);
        }

        fn visit_item_use(&mut self, item: &'ast ItemUse) {
            let logs = use_roots(&item.tree)
                .iter()
                .any(|root| LOGGERS.iter().any(|logger| *root == logger));
            if self.silent() && logs {
                self.refuse(format_args!("a logger imported outside `{DIAG_MODULE}`"));
            }
            for (path, renamed) in use_leaves(&item.tree, &[]) {
                self.imported(&path, renamed);
            }
            visit::visit_item_use(self, item);
        }

        fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
            let logs = LOGGERS.iter().any(|logger| item.ident.unraw() == logger);
            if self.silent() && logs {
                self.refuse(format_args!("a logger imported outside `{DIAG_MODULE}`"));
            }
            visit::visit_item_extern_crate(self, item);
        }

        /// An imported name: the invocation's type imported by its own
        /// name at the top of its two modules is accepted.
        fn visit_use_name(&mut self, name: &'ast UseName) {
            let ident = name.ident.unraw();
            if ident == "KlipyClient" {
                self.klipy_imported += 1;
            }
            self.importing = ident == INVOCATION
                && self.within.is_empty()
                && (self.file == COMMAND_MODULE || self.file == PROOF_MODULE);
            visit::visit_use_name(self, name);
            self.importing = false;
        }

        fn visit_ident(&mut self, ident: &'ast Ident) {
            self.ident(&ident.unraw().to_string());
        }

        fn visit_path(&mut self, path: &'ast syn::Path) {
            self.path(&segments(path), false);
            visit::visit_path(self, path);
        }

        /// A macro call; the list of `generate_handler!` in `run` names the
        /// command functions.
        fn visit_macro(&mut self, call: &'ast Macro) {
            let path = segments(&call.path);
            self.path(&path, true);
            self.panics(&path, Some(call.tokens.clone()));
            for segment in &call.path.segments {
                self.visit_path_segment(segment);
            }
            let lists = self.production() && path.last().is_some_and(|last| last == HANDLER);
            if lists {
                let around = self.within.first().cloned().unwrap_or_default();
                self.handlers.push(around);
            }
            self.in_handler = lists && self.file == "lib.rs" && self.within == ["run"];
            self.in_macro += 1;
            self.tokens(call.tokens.clone());
            self.in_macro -= 1;
            self.in_handler = false;
        }

        fn visit_lit(&mut self, literal: &'ast Lit) {
            self.literal(literal);
            visit::visit_lit(self, literal);
        }

        fn visit_token_stream(&mut self, tokens: &'ast TokenStream) {
            self.tokens(tokens.clone());
        }
    }

    /// Whether `attribute` is exactly `#[cfg(test)]`.
    fn is_test(attribute: &Attribute) -> bool {
        matches!(attribute.style, AttrStyle::Outer)
            && matches!(&attribute.meta, Meta::List(list)
                if list.path.is_ident("cfg") && list.tokens.to_string() == "test")
    }

    /// Whether `attribute` makes a command (`#[tauri::command]`).
    fn is_command(attribute: &Attribute) -> bool {
        let last = attribute.path().segments.last();
        matches!(attribute.style, AttrStyle::Outer)
            && last.is_some_and(|segment| segment.ident.unraw() == "command")
    }

    /// Whether `path` ends with `tail` (`r#` removed).
    fn ends_with(path: &syn::Path, tail: [&str; 2]) -> bool {
        segments(path).ends_with(&tail.map(String::from))
    }

    /// The receiver of `expr` when it is the call `receiver.method()`, no
    /// argument and no turbofish.
    fn receiver<'e>(expr: &'e Expr, method: &str) -> Option<&'e Expr> {
        match expr {
            Expr::MethodCall(call)
                if call.attrs.is_empty()
                    && call.turbofish.is_none()
                    && call.args.is_empty()
                    && call.method.unraw() == method =>
            {
                Some(&call.receiver)
            }
            _ => None,
        }
    }

    /// The key `expr` reads when it is `key.expose_secret()`.
    fn key_text(expr: &Expr) -> Option<&Expr> {
        receiver(expr, KEY_READER)
    }

    /// Whether `ty` is the proof (`UserAsked`, a reference to it, ...).
    fn is_proof(ty: &Type) -> bool {
        match ty {
            Type::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident.unraw() == PROOF),
            Type::Reference(reference) => is_proof(&reference.elem),
            Type::Paren(inner) => is_proof(&inner.elem),
            Type::Group(inner) => is_proof(&inner.elem),
            _ => false,
        }
    }

    /// Whether `text` is a `javascript:` URL as a browser reads one:
    /// leading spaces and control characters cut, tabs and newlines
    /// dropped, the scheme in any case.
    fn is_script_url(text: &str) -> bool {
        let url: String = text
            .trim_start_matches(|c: char| c <= ' ')
            .chars()
            .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
            .collect();
        url.split_once(':')
            .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case(SCRIPT_SCHEME))
    }

    /// The attributes of `item`; none for one `syn` does not parse.
    fn item_attributes(item: &Item) -> &[Attribute] {
        match item {
            Item::Const(item) => &item.attrs,
            Item::Enum(item) => &item.attrs,
            Item::ExternCrate(item) => &item.attrs,
            Item::Fn(item) => &item.attrs,
            Item::ForeignMod(item) => &item.attrs,
            Item::Impl(item) => &item.attrs,
            Item::Macro(item) => &item.attrs,
            Item::Mod(item) => &item.attrs,
            Item::Static(item) => &item.attrs,
            Item::Struct(item) => &item.attrs,
            Item::Trait(item) => &item.attrs,
            Item::TraitAlias(item) => &item.attrs,
            Item::Type(item) => &item.attrs,
            Item::Union(item) => &item.attrs,
            Item::Use(item) => &item.attrs,
            _ => &[],
        }
    }

    /// The attributes of the impl item `item`; none for one `syn` does not
    /// parse.
    fn impl_item_attributes(item: &ImplItem) -> &[Attribute] {
        match item {
            ImplItem::Const(item) => &item.attrs,
            ImplItem::Fn(item) => &item.attrs,
            ImplItem::Type(item) => &item.attrs,
            ImplItem::Macro(item) => &item.attrs,
            _ => &[],
        }
    }

    /// The segments of `path`, `r#` removed.
    fn segments(path: &syn::Path) -> Vec<String> {
        path.segments
            .iter()
            .map(|segment| segment.ident.unraw().to_string())
            .collect()
    }

    /// The first name of each path a `use` tree imports.
    fn use_roots(tree: &UseTree) -> Vec<Ident> {
        match tree {
            UseTree::Path(path) => vec![path.ident.unraw()],
            UseTree::Name(name) => vec![name.ident.unraw()],
            UseTree::Rename(rename) => vec![rename.ident.unraw()],
            UseTree::Glob(_) => Vec::new(),
            UseTree::Group(group) => group.items.iter().flat_map(use_roots).collect(),
        }
    }

    /// Each path a `use` tree under `prefix` imports (`r#` removed; `*`
    /// for a glob), and whether it is renamed.
    fn use_leaves(tree: &UseTree, prefix: &[String]) -> Vec<(Vec<String>, bool)> {
        let with = |name: &Ident| {
            let mut path = prefix.to_vec();
            path.push(name.unraw().to_string());
            path
        };
        match tree {
            UseTree::Path(path) => use_leaves(&path.tree, &with(&path.ident)),
            UseTree::Name(name) => vec![(with(&name.ident), false)],
            UseTree::Rename(rename) => vec![(with(&rename.ident), true)],
            UseTree::Glob(_) => {
                let mut path = prefix.to_vec();
                path.push("*".into());
                vec![(path, false)]
            }
            UseTree::Group(group) => group
                .items
                .iter()
                .flat_map(|tree| use_leaves(tree, prefix))
                .collect(),
        }
    }

    /// The path that starts at the identifier `tokens[at]` (`a::b::c`, `r#`
    /// removed) and where the tokens after it start.
    fn path_at(tokens: &[TokenTree], mut at: usize) -> (Vec<String>, usize) {
        let mut path = Vec::new();
        while let Some(TokenTree::Ident(ident)) = tokens.get(at) {
            path.push(ident.unraw().to_string());
            at += 1;
            if !separates(tokens, at) {
                break;
            }
            at += 2;
        }
        (path, at)
    }

    /// Whether `tokens[at..]` starts with the path separator `::`.
    fn separates(tokens: &[TokenTree], at: usize) -> bool {
        match tokens.get(at) {
            Some(TokenTree::Punct(first)) => {
                first.as_char() == ':'
                    && first.spacing() == Spacing::Joint
                    && is_punct(tokens.get(at + 1), ':')
            }
            _ => false,
        }
    }

    /// Whether `token` is the punctuation `c`.
    fn is_punct(token: Option<&TokenTree>, c: char) -> bool {
        matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == c)
    }

    /// Whether the identifier `tokens[at]` follows a `.` (it is a field or
    /// a method), not a `..` (a range's end, which may be a path).
    fn is_member(tokens: &[TokenTree], at: usize) -> bool {
        let Some(before) = at.checked_sub(1) else {
            return false;
        };
        let dot = |i: usize| is_punct(tokens.get(i), '.');
        dot(before) && !before.checked_sub(1).is_some_and(dot)
    }

    /// Whether `token` is a call's parenthesized arguments.
    fn is_call(token: Option<&TokenTree>) -> bool {
        matches!(token, Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis)
    }

    /// The arguments of a macro call (its tokens split at the commas
    /// outside any group), each as tokens; none for no tokens.
    fn arguments_of(tokens: TokenStream) -> Vec<Vec<TokenTree>> {
        let mut arguments = vec![Vec::new()];
        for token in tokens {
            match (&token, arguments.last_mut()) {
                (TokenTree::Punct(comma), _) if comma.as_char() == ',' => {
                    arguments.push(Vec::new());
                }
                (_, Some(argument)) => argument.push(token),
                (_, None) => {}
            }
        }
        arguments.retain(|argument| !argument.is_empty());
        arguments
    }

    /// Whether `argument` is one string literal without a placeholder: a
    /// message the panic hook prints as it is written.
    fn is_plain_text(argument: &[TokenTree]) -> bool {
        let [TokenTree::Literal(literal)] = argument else {
            return false;
        };
        let Lit::Str(text) = Lit::new(literal.clone()) else {
            return false;
        };
        let text = text.value();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' && chars.next_if_eq(&'{').is_none() {
                return false;
            }
        }
        true
    }

    /// Whether a function returns `output`, a `Result` (by its last name).
    fn returns_a_result(output: &ReturnType) -> bool {
        match output {
            ReturnType::Type(_, ty) => matches!(&**ty, Type::Path(path)
                if path.path.segments.last().is_some_and(|last| last.ident == "Result")),
            ReturnType::Default => false,
        }
    }

    /// The names `pattern` binds, added to `names`.
    fn bindings(pattern: &Pat, names: &mut Vec<String>) {
        struct Bindings<'n>(&'n mut Vec<String>);
        impl<'ast> Visit<'ast> for Bindings<'_> {
            fn visit_pat_ident(&mut self, ident: &'ast PatIdent) {
                self.0.push(ident.ident.unraw().to_string());
                visit::visit_pat_ident(self, ident);
            }
        }
        Bindings(names).visit_pat(pattern);
    }

    /// The name of `item` when it is an enum.
    fn enum_name(item: &Item) -> Option<String> {
        match item {
            Item::Enum(codes) => Some(codes.ident.unraw().to_string()),
            _ => None,
        }
    }

    /// Whether `tokens` hold the identifier `name`, in a group or not.
    fn names(tokens: TokenStream, name: &str) -> bool {
        tokens.into_iter().any(|token| match token {
            TokenTree::Ident(ident) => ident.unraw() == name,
            TokenTree::Group(group) => names(group.stream(), name),
            _ => false,
        })
    }

    /// For each `mod {module};` item of `parent`, whether it is under
    /// `#[cfg(test)]`.
    fn declarations(parent: &Source, module: &str) -> Vec<bool> {
        parent
            .tree
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Mod(declared) if declared.content.is_none() && declared.ident == module => {
                    Some(declared.attrs.iter().any(is_test))
                }
                _ => None,
            })
            .collect()
    }

    /// Whether `source` is built for tests only: every declaration of its
    /// module (in its parent file among `sources`) is under `#[cfg(test)]`,
    /// or is made by a test-only parent. Why it cannot be classified when
    /// nothing declares it.
    fn test_only(source: &Source, sources: &[Source]) -> Result<bool, String> {
        let (dir, file) = source.name.rsplit_once('/').unwrap_or(("", &source.name));
        if dir.is_empty() && matches!(file, "lib.rs" | "main.rs") {
            return Ok(false);
        }
        let stem = file.trim_end_matches(".rs");
        // `a/mod.rs` is the module `a`, declared where `a.rs` would be.
        let (dir, module) = if stem == "mod" {
            dir.rsplit_once('/').unwrap_or(("", dir))
        } else {
            (dir, stem)
        };
        let parents = if dir.is_empty() {
            ["lib.rs".to_string(), "main.rs".to_string()]
        } else {
            [format!("{dir}.rs"), format!("{dir}/mod.rs")]
        };
        let mut declared = Vec::new();
        for parent in sources.iter().filter(|s| parents.contains(&s.name)) {
            let found = declarations(parent, module);
            if !found.is_empty() && test_only(parent, sources)? {
                declared.extend(found.iter().map(|_| true));
            } else {
                declared.extend(found);
            }
        }
        if declared.is_empty() {
            return Err(format!("{}: no `mod {module};` declares it", source.name));
        }
        Ok(declared.iter().all(|&under_test| under_test))
    }

    /// Every `.rs` file under `src/`, found when the test runs (a file
    /// added later is read too), or why one cannot be read.
    fn studio_sources() -> Vec<Result<Source, String>> {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut dirs = vec![src.clone()];
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension() == Some(OsStr::new("rs")) {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
            .iter()
            .map(|path| {
                let parts = path.strip_prefix(&src).unwrap().components();
                let parts: Vec<_> = parts
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect();
                let name = parts.join("/");
                let text = std::fs::read_to_string(path)
                    .map_err(|e| format!("{name}: cannot be read: {e}"))?;
                Source::new(&name, &text)
            })
            .collect()
    }

    /// D-2026-10-01-gif-sticker-search-3, -10, -11 and -12 (DoD row 3),
    /// the source guard. [`UserAsked`] guards the GIF state, its source factory
    /// and the command functions: setup code has no command `Request` to
    /// make one from. [`KlipyKey`] cannot be printed. Neither stops the app
    /// from forging an invocation Tauri then dispatches, from making a
    /// second KLIPY client or a proof in another command, from calling a
    /// GIF command's function from another command with that command's
    /// invocation, nor from reading the key's text or the window's
    /// invocation where it should not; this does, for the studio's
    /// production code. It parses every `.rs` file under `src/` as it runs
    /// (a new file is read too) with `syn` and reads each identifier, `r#`
    /// removed, the tokens of macro calls and attributes included. It
    /// checks that:
    /// - production code holds none of `FORGERIES`: no invocation handed to
    ///   a webview with the app's invoke key, no script run in the window,
    ///   no page loaded in it (`navigate`); and none of `INVOCATION_PARTS`
    ///   (`Invoke`, `InvokeMessage`, `InvokeBody`, `payload`): no code reads
    ///   an invocation the window sent, an invoke handler wrapped around
    ///   `generate_handler!` included;
    /// - no path names a command function (each free `#[tauri::command]`
    ///   function, found in the syntax trees; or the macro Tauri makes for
    ///   it, `__cmd__…`), nor does a `use` import one, but the list of
    ///   `generate_handler!` in `run`, the one `generate_handler!`: a
    ///   command is entered only through IPC. Its definition is a name, not
    ///   a path, a method or a field of the same name is not a path, and in
    ///   `commands.rs` a name alone that a parameter or a pattern binds
    ///   where it is read (`let video_auto = …; Ok(video_auto)`) is that
    ///   local, from the binding on and in its scope only;
    /// - the window's invocation (`Request`) is named only in the GIF
    ///   commands that take it (`search_gifs`, `gif_preview` and
    ///   `collect_gif`), in `UserAsked::of` and in the imports of their two
    ///   modules by its own name: no other function takes it, under any
    ///   spelling (`use … as`, `type … =`, a qualified path, a macro's
    ///   tokens);
    /// - `KlipyClient::new` is called once, in `klipy_source`, and the type
    ///   is named nowhere else but its import;
    /// - the key's accessor (`KEY_READER`) is named in two places only,
    ///   besides its definition in `gifs/key.rs`: as `key.expose_secret()`,
    ///   a direct argument of `KlipyClient::new` in `klipy_source`, and the
    ///   one argument of `serialize_str` in `write_key` (`KEY_WRITER`), the
    ///   key file's serializer in `gifs/key.rs`. Anywhere else (bound to a
    ///   variable, inside any macro call's tokens, a path, another argument,
    ///   method, field, function or file, the `KeyJson` literal of
    ///   `KeyFile::save` included) it is refused, and `write_key` is named
    ///   nowhere but at its definition: serde calls it, through the
    ///   attribute of the key file JSON's `key`, which is a `KlipyKey`;
    /// - a function that reads the key's text (those three) calls no macro
    ///   at all;
    /// - nothing prints or logs but `diag.rs`
    ///   (D-2026-10-01-gif-sticker-search-12): elsewhere, production code
    ///   calls no print macro (`PRINT_MACROS`) nor a logger's macro by its
    ///   bare name, uses no logger's path, imports no logger, names no
    ///   output stream (`stderr`, `Stdout`, ...) nor holds a literal naming
    ///   its file (`/dev/stderr`, ...), calls no panic or assertion that
    ///   formats its message (more than one string literal without a
    ///   placeholder) or prints its operands (`assert_eq!`, ...), and no
    ///   method or function that panics printing what it holds (`unwrap`,
    ///   `expect`, `panic_any`, ...); `main` returns no `Result`. The GIF,
    ///   key and command modules call no panic or assertion at all. In
    ///   `diag.rs`, every function takes only a `DiagCode` (`self` in `impl
    ///   DiagCode`) or a `&'static str` and is not generic, every enum is
    ///   closed (no variant holds data), and it holds no `static`, type that
    ///   holds data, trait, other `impl`, macro of its own, module, nor an
    ///   import or a path of two names or more but of a logger or its own
    ///   items: it says only what it is given;
    /// - `UserAsked::of` is named (called, or taken as a value, a macro's
    ///   tokens included) only in the bodies of the `#[tauri::command]`
    ///   functions `search_gifs`, `gif_preview` and `collect_gif` of
    ///   `commands.rs`. So that no other spelling reaches it, `UserAsked` is
    ///   not renamed (`use … as`, `type … =`), not in a qualified path
    ///   (`<UserAsked>::of`), not in another macro call's tokens, and has no
    ///   `impl` outside `gifs/asked.rs`, where it derives only `Debug` and
    ///   only `of` makes one;
    /// - no literal of the studio, tests included, holds the window's IPC
    ///   object or is a `javascript:` URL (any case, leading spaces and
    ///   controls cut, tabs and newlines dropped), and no file names KLIPY's
    ///   API and file hosts: they are `bezel-klipy`'s.
    ///
    /// Test code is left out by one rule, and only it: an item (a module,
    /// an `impl` and its items, a function, a `use`, ...) marked exactly
    /// `#[cfg(test)]`, and a file whose module is declared only so (`mod
    /// tests;`), or by such a file. Anything else is read as production.
    /// The rule fails closed: a file `syn` cannot parse, a file nothing
    /// declares, and production code that reads a module from another path
    /// or includes another file's code cannot be classified, and fail the
    /// test.
    #[test]
    fn nothing_in_the_app_forges_an_invocation() {
        let mut read = Vec::new();
        let mut problems = Vec::new();
        for source in studio_sources() {
            match source {
                Ok(source) => read.push(source),
                Err(why) => problems.push(why),
            }
        }
        let mut built = Vec::new();
        for source in &read {
            problems.extend(source.literals());
            match test_only(source, &read) {
                Err(why) => problems.push(why),
                Ok(true) => {}
                Ok(false) => built.push(source),
            }
        }
        let commands: Vec<String> = built.iter().flat_map(|s| s.commands()).collect();
        let (mut made, mut reads, mut asked) = (Vec::new(), Vec::new(), Vec::new());
        let (mut handled, mut handlers) = (Vec::new(), Vec::new());
        let mut diag_functions = Vec::new();
        for source in &built {
            let reader = source.production(&commands);
            let at = |f: &String| format!("{}: {f}", source.name);
            made.extend(reader.made.iter().map(at));
            reads.extend(reader.reads.iter().map(at));
            asked.extend(reader.asked.iter().map(at));
            handlers.extend(reader.handlers.iter().map(at));
            diag_functions.extend(reader.diag_functions.iter().map(at));
            handled.extend(reader.handled);
            problems.extend(reader.problems);
        }
        assert!(problems.is_empty(), "{problems:#?}");
        // The one module that prints is built, and its functions were read.
        assert!(
            diag_functions.iter().any(|f| f == "diag.rs: report"),
            "{diag_functions:?}"
        );
        assert_eq!(
            made,
            ["lib.rs: klipy_source"],
            "KLIPY's client is made once in production, by the source factory"
        );
        assert_eq!(
            reads,
            [
                "gifs/key.rs: expose_secret",
                "gifs/key.rs: write_key",
                "lib.rs: klipy_source"
            ],
            "the key's text is read by its accessor, for the key file and KLIPY's client"
        );
        assert_eq!(
            asked,
            [
                "commands.rs: search_gifs",
                "commands.rs: gif_preview",
                "commands.rs: collect_gif"
            ],
            "a proof is made by the GIF commands that take the window's request"
        );
        // The commands were found, and the one handler list names each of
        // them once.
        assert_eq!(handlers, ["lib.rs: run"], "one `generate_handler!`");
        assert!(commands.len() >= GIF_COMMANDS.len(), "{commands:?}");
        for command in GIF_COMMANDS {
            assert!(commands.iter().any(|c| c == command), "{command}");
        }
        let mut commands = commands;
        commands.sort_unstable();
        handled.sort_unstable();
        assert_eq!(handled, commands, "generate_handler! and the commands");
        // The rule read this file's tests as tests, and the GIF files as
        // they are built.
        let lib = read.iter().find(|source| source.name == "lib.rs").unwrap();
        let functions = lib.production(&[]).functions;
        assert!(functions.iter().any(|f| f == "klipy_source"));
        assert!(
            !functions
                .iter()
                .any(|f| f == "nothing_in_the_app_forges_an_invocation")
        );
        let role = |name: &str| {
            let source = read.iter().find(|source| source.name == name).unwrap();
            test_only(source, &read).unwrap()
        };
        assert!(role("gifs/tests.rs") && role("manager/tests.rs") && role("storage/tests.rs"));
        assert!(!role("gifs/asked.rs") && !role("gifs/key.rs") && !role("gifs.rs"));
        assert!(!role("main.rs") && !role(DIAG_MODULE));
    }

    /// The command functions of the studio's `commands.rs`.
    fn studio_commands() -> Vec<String> {
        Source::new(COMMAND_MODULE, include_str!("commands.rs"))
            .unwrap()
            .commands()
    }

    /// The source guard's findings in the made-up file `name` read as
    /// production, the studio's commands and its own being the command
    /// functions, and the functions around each `KlipyClient::new`.
    fn guard(name: &str, text: &str) -> (Vec<String>, Vec<String>) {
        let source = Source::new(name, text).unwrap();
        let mut commands = studio_commands();
        commands.extend(source.commands());
        let reader = source.production(&commands);
        let mut problems = source.literals();
        problems.extend(reader.problems);
        (problems, reader.made)
    }

    /// The source guard's rule on a made-up file: an item marked
    /// `#[cfg(test)]` (a module, an `impl`, a method) is cut whatever its
    /// comments and literals hold, and only it, so code after it, and code
    /// under another `cfg`, is production; a file that is not Rust cannot
    /// be classified.
    #[test]
    fn the_source_guard_cuts_only_test_modules() {
        let file = r##"fn before() {}
#[cfg(test)]
mod tests {
    const C: [char; 3] = ['{', '\'', '"'];
    const S: &str = r#"}"#; // }
    /* } /* } */ */
    fn inside<'a>(w: &'a W) { w.eval("}\"") }
}
#[cfg(test)]
impl W {
    fn helper(&self) { self.with_webview(|_| {}) }
}
impl W {
    #[cfg(test)]
    fn in_a_test() { KlipyClient::new("k", "c"); }
    fn kept(&self, r: R) { self.on_message(r) }
}
#[cfg(any(test, feature = "x"))]
mod read { fn as_production(w: W) { w.invoke_key() } }
fn after(w: W) { w.on_message(request) }
"##;
        let source = Source::new("x.rs", file).unwrap();
        let reader = source.production(&[]);
        assert_eq!(
            reader.functions,
            ["before", "kept", "as_production", "after"]
        );
        assert_eq!(
            reader.problems,
            [
                "x.rs: `on_message` in production code",
                "x.rs: `invoke_key` in production code",
                "x.rs: `on_message` in production code",
            ]
        );
        assert!(reader.made.is_empty(), "{:?}", reader.made);
        assert!(Source::new("y.rs", "#[cfg(test)]\nmod tests {\n").is_err());
        assert!(Source::new("z.rs", "const S: &str = \"open;\n").is_err());
    }

    /// A made-up case of the source guard: a file's name, its text, and
    /// what one of its findings says.
    type Case = (&'static str, String, &'static str);

    /// The key's text reaching a macro, a variable or another place
    /// (D-2026-10-01-gif-sticker-search-10, review W1 of round 2): each is
    /// refused.
    fn key_leaks() -> Vec<Case> {
        let factory = |body: &str| {
            format!(
                "fn klipy_source() -> F {{ Arc::new(|_: &UserAsked, key: &KlipyKey, c: &str| \
                 {{ {body} }}) }}"
            )
        };
        let made = "Arc::new(KlipyClient::new(key.expose_secret(), c))";
        let write = |more: &str, text: &str| {
            format!(
                "fn write_key<S: Serializer>(key: &KlipyKey, s: S) -> Result<S::Ok, S::Error> \
                 {{ {more} s.serialize_str({text}) }}"
            )
        };
        vec![
            // Printed or logged by the factory (the reviewer's m3e, m3d).
            (
                "lib.rs",
                factory(&format!(
                    "eprintln!(\"bezel-studio: KLIPY client for key {{}}\", \
                     key.expose_secret()); {made}"
                )),
                "inside a macro call",
            ),
            (
                "lib.rs",
                factory(&format!(
                    "tracing::debug!(key = key.expose_secret()); {made}"
                )),
                "inside a macro call",
            ),
            // Any macro where the key is read, even without the key.
            (
                "lib.rs",
                factory(&format!(
                    "eprintln!(\"bezel-studio: KLIPY for {{c}}\"); {made}"
                )),
                "`eprintln!` in `klipy_source`, which reads the KLIPY key",
            ),
            // Bound to a variable (then passed to anything), passed through
            // another expression, read through a path, or by another
            // function.
            (
                "lib.rs",
                factory("let k = key.expose_secret(); Arc::new(KlipyClient::new(k, c))"),
                "outside its two uses",
            ),
            (
                "lib.rs",
                factory("Arc::new(KlipyClient::new(&key.expose_secret().to_owned(), c))"),
                "outside its two uses",
            ),
            (
                "lib.rs",
                factory("Arc::new(KlipyClient::new(KlipyKey::expose_secret(key), c))"),
                "outside its two uses",
            ),
            (
                "lib.rs",
                "fn warm_up(key: &KlipyKey) { KlipyClient::new(key.expose_secret(), c); }".into(),
                "outside its two uses",
            ),
            // The key file's serializer: formatted, bound, a macro there.
            (
                KEY_MODULE,
                write("", "&format!(\"{}\", key.expose_secret())"),
                "inside a macro call",
            ),
            (
                KEY_MODULE,
                write("let k = key.expose_secret();", "k"),
                "outside its two uses",
            ),
            (
                KEY_MODULE,
                write(
                    "let _ = writeln!(std::io::sink(), \"saving\");",
                    "key.expose_secret()",
                ),
                "`writeln!` in `write_key`, which reads the KLIPY key",
            ),
            // The critic of round 2, iter 4: the key file's JSON held the
            // key as a `String`, which an error could quote. The literal
            // that made it is refused now; the JSON holds a `KlipyKey`.
            (
                KEY_MODULE,
                "fn save(s: &SavedKey) -> J { KeyJson { key: s.key.expose_secret().to_string(), \
                 customer_id: s.customer_id.clone() } }"
                    .into(),
                "outside its two uses",
            ),
            // Another method of the serializer, the serializer elsewhere,
            // or called by code (with any serializer: to a `String`, a
            // file) rather than by serde for the key file.
            (
                KEY_MODULE,
                write("", "&key.expose_secret().to_owned()"),
                "outside its two uses",
            ),
            (
                KEY_MODULE,
                "fn write_key<S: Serializer>(key: &KlipyKey, s: S) -> R { \
                 s.collect_str(key.expose_secret()) }"
                    .into(),
                "outside its two uses",
            ),
            (
                "lib.rs",
                "fn write_key<S: Serializer>(key: &KlipyKey, s: S) -> R { \
                 s.serialize_str(key.expose_secret()) }"
                    .into(),
                "outside its two uses",
            ),
            (
                KEY_MODULE,
                "fn found(k: &KlipyKey) -> String { let mut out = Vec::new(); \
                 let _ = write_key(k, &mut serde_json::Serializer::new(&mut out)); \
                 String::from_utf8_lossy(&out).into_owned() }"
                    .into(),
                "the key file's serializer, named outside its definition",
            ),
            (
                "gifs.rs",
                "fn found(k: &KlipyKey) -> String { serde_json::to_string(&Wrap(k, key::write_key)) }"
                    .into(),
                "the key file's serializer, named outside its definition",
            ),
            // A command logging it.
            (
                "commands.rs",
                "fn save_klipy_key(key: KlipyKey) { tracing::info!(\"{}\", key.expose_secret()) }"
                    .into(),
                "inside a macro call",
            ),
        ]
    }

    /// A proof made outside the GIF commands' bodies, or reached by another
    /// spelling (D-2026-10-01-gif-sticker-search-10, the critic of round 2):
    /// each is refused.
    fn proofs_made_elsewhere() -> Vec<Case> {
        // The critic's mutant: a command the window calls at start makes a
        // proof of its request and searches.
        let preferences = |made: &str| {
            format!(
                "#[tauri::command]\npub fn preferences(request: Request<'_>, \
                 gifs: State<'_, SharedGifs>, state: State<'_, Shared>) -> PreferencesDto {{ \
                 let asked = {made}; \
                 let _ = query(\"gif\", \"\", 1, false, Language::En).map(|q| gifs.search(&asked, &q)); \
                 state.preferences() }}"
            )
        };
        let outside = "`UserAsked::of` outside the GIF commands";
        vec![
            (
                "commands.rs",
                preferences("UserAsked::of(&request)"),
                outside,
            ),
            (
                "commands.rs",
                preferences("crate::gifs::r#UserAsked::r#of(&request)"),
                outside,
            ),
            (
                "commands.rs",
                preferences("<UserAsked>::of(&request)"),
                "qualified path",
            ),
            // A helper, the setup, a method or a file that is not the
            // command's.
            (
                "commands.rs",
                "fn asked(request: &Request<'_>) -> UserAsked { UserAsked::of(request) }".into(),
                outside,
            ),
            (
                "lib.rs",
                "fn setup(app: &App) { let make = UserAsked::of; }".into(),
                outside,
            ),
            (
                "commands.rs",
                "impl Gifs { fn search_gifs(r: Request<'_>) { UserAsked::of(&r); } }".into(),
                outside,
            ),
            (
                "gifs.rs",
                "#[tauri::command]\nfn search_gifs(r: Request<'_>) { UserAsked::of(&r); }".into(),
                outside,
            ),
            // Other spellings.
            (
                "commands.rs",
                "use crate::gifs::UserAsked as Proof;".into(),
                "renamed",
            ),
            ("commands.rs", "type Proof = UserAsked;".into(), "renamed"),
            (
                "commands.rs",
                "macro_rules! of { ($t:ident, $r:expr) => { $t::of($r) } }\n\
                 fn f(r: &Request<'_>) { of!(UserAsked, r); }"
                    .into(),
                "`UserAsked` inside a macro call",
            ),
            (
                "gifs.rs",
                "impl From<&Request<'_>> for UserAsked { \
                 fn from(r: &Request<'_>) -> Self { Self::of(r) } }"
                    .into(),
                "an `impl` for `UserAsked`",
            ),
            // Made otherwise in its own module.
            (
                PROOF_MODULE,
                "#[derive(Debug, Default)]\npub struct UserAsked { _invoked: () }".into(),
                "derives more than `Debug`",
            ),
            (
                PROOF_MODULE,
                "impl UserAsked { pub fn at_start() -> Self { Self { _invoked: () } } }".into(),
                "a struct literal",
            ),
        ]
    }

    /// A page loaded in the window, or a URL that runs a script
    /// (D-2026-10-01-gif-sticker-search-10, review of round 2): each is
    /// refused.
    fn pages_loaded() -> Vec<Case> {
        let script = "`javascript:` URL";
        vec![
            (
                "lib.rs",
                "fn f(w: WebviewWindow, u: Url) { let _ = w.navigate(u); }".into(),
                "`navigate`",
            ),
            (
                "lib.rs",
                "fn f(w: &Webview, u: Url) { let _ = tauri::Webview::r#navigate(w, u); }".into(),
                "`navigate`",
            ),
            (
                "lib.rs",
                "const GO: &str = \" JavaScript:document.getElementById('x').click()\";".into(),
                script,
            ),
            (
                "lib.rs",
                r#"fn f() -> String { format!("java\tscript:{}", 1) }"#.into(),
                script,
            ),
            (
                "lib.rs",
                "#[cfg(test)]\nmod tests { const GO: &[u8] = b\"JAVASCRIPT:go()\"; }".into(),
                script,
            ),
        ]
    }

    /// A command function entered from Rust, not through IPC
    /// (D-2026-10-01-gif-sticker-search-11, review W1 and the critic of
    /// round 2): each is refused.
    fn commands_called_from_rust() -> Vec<Case> {
        let named = "names a command function";
        vec![
            // The critic's mutant (the reviewer's `cb2`): the command the
            // window calls at start searches with its own invocation.
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn preferences(request: Request<'_>, \
                 gifs: State<'_, SharedGifs>, state: State<'_, Shared>) \
                 -> UiResult<PreferencesDto> { let _ = search_gifs(request, gifs, \
                 state.clone(), \"gif\".into(), String::new(), 1, None).await; \
                 Ok(state.preferences()) }"
                    .into(),
                "`search_gifs` names a command function",
            ),
            // A command calling another one that is not a GIF command.
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn release_screen(state: State<'_, Shared>, \
                 screen: String) -> UiResult<()> { \
                 set_brightness(state.clone(), screen.clone(), 0).await?; \
                 blocking(&state, move |b| b.release_screen(&screen)).await }"
                    .into(),
                "`set_brightness` names a command function",
            ),
            // From another file: raw and qualified, as a value, imported,
            // renamed, in a macro's tokens, Tauri's macro for it, a second
            // handler list.
            (
                "lib.rs",
                "fn setup(app: &App) { let _ = crate::commands::r#gif_preview; }".into(),
                named,
            ),
            ("lib.rs", "use crate::commands::collect_gif;".into(), named),
            (
                "lib.rs",
                "use crate::commands::{search_gifs as warm_up};".into(),
                named,
            ),
            (
                "lib.rs",
                "fn setup(h: H) { spawn!(async move { commands::search_gifs(h).await }); }".into(),
                named,
            ),
            (
                "lib.rs",
                "fn setup(i: I) { commands::__cmd__search_gifs!(search_gifs, i); }".into(),
                named,
            ),
            (
                "lib.rs",
                "fn setup(i: I) { crate::r#__cmd__collect_gif!(collect_gif, i); }".into(),
                named,
            ),
            (
                COMMAND_MODULE,
                "fn warm_up(h: H) { let _ = self::search_gifs(h); }".into(),
                named,
            ),
            // The command module renamed, or its names all imported.
            (
                "lib.rs",
                "use crate::commands as c;\nfn setup(h: H) { c::search_gifs(h); }".into(),
                "the command module renamed",
            ),
            (
                "lib.rs",
                "use crate::commands::{self as c};".into(),
                "the command module renamed",
            ),
            ("lib.rs", "use crate::commands::*;".into(), "by a glob"),
            (
                "lib.rs",
                "fn setup(b: B) -> B { b.invoke_handler(tauri::generate_handler![\
                 commands::search_gifs]) }"
                    .into(),
                named,
            ),
        ]
    }

    /// The window's invocation taken by a function but the GIF commands
    /// (D-2026-10-01-gif-sticker-search-11, the critic of round 2): each is
    /// refused.
    fn invocations_taken_elsewhere() -> Vec<Case> {
        let taken = "`Request` (the window's invocation) named outside";
        vec![
            // The critic's mutant: the key's command prints the body of its
            // invocation, the key.
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn save_klipy_key(request: Request<'_>, \
                 gifs: State<'_, SharedGifs>, state: State<'_, Shared>, key: KlipyKey) \
                 -> UiResult<KeyDto> { eprintln!(\"{:?}\", request.body()); \
                 with_gifs(&gifs, &state, move |g, _| g.save_key(key)).await }"
                    .into(),
                taken,
            ),
            // Without a print, by its full path or raw; renamed, aliased, by
            // a helper, in another module.
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn save_klipy_key(\
                 request: tauri::ipc::Request<'_>, key: KlipyKey) -> R { \
                 let body = request.body(); keep(body, key) }"
                    .into(),
                taken,
            ),
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn klipy_key(\
                 request: tauri::ipc::r#Request<'_>) -> R { keep(request) }"
                    .into(),
                taken,
            ),
            (
                COMMAND_MODULE,
                "use tauri::ipc::Request as R;\n#[tauri::command]\n\
                 pub async fn remove_klipy_key(request: R<'_>) -> U { keep(request) }"
                    .into(),
                taken,
            ),
            (
                COMMAND_MODULE,
                "type Invocation<'a> = tauri::ipc::Request<'a>;".into(),
                taken,
            ),
            (
                COMMAND_MODULE,
                "fn body_of(request: &Request<'_>) -> Vec<u8> { request.body().to_vec() }".into(),
                taken,
            ),
            (
                "gifs.rs",
                "use tauri::ipc::Request;\nfn remember(request: &Request<'_>) {}".into(),
                taken,
            ),
        ]
    }

    /// A print, a log or a panic in the command module
    /// (D-2026-10-01-gif-sticker-search-11, -12): each is refused.
    fn prints_in_commands() -> Vec<Case> {
        let prints = "prints or logs outside `diag.rs`";
        let command = |body: &str| {
            format!("#[tauri::command]\npub fn save_klipy_key(key: KlipyKey) {{ {body} }}")
        };
        vec![
            (
                COMMAND_MODULE,
                command("eprintln!(\"bezel-studio: key saved\")"),
                prints,
            ),
            (
                COMMAND_MODULE,
                command("tracing::warn!(\"key saved\")"),
                prints,
            ),
            (
                COMMAND_MODULE,
                command("::log::info!(\"key saved\")"),
                prints,
            ),
            (COMMAND_MODULE, command("dbg!(&key)"), prints),
            // A panic, even without a value: not in the GIF, key and
            // command modules.
            (
                COMMAND_MODULE,
                command("unreachable!(\"key saved\")"),
                "panics in a GIF, key or command module",
            ),
            // A logger's macro by its bare name, its import, its crate.
            (COMMAND_MODULE, command("warn!(\"key saved\")"), prints),
            (
                COMMAND_MODULE,
                "use tracing::{self as t};".into(),
                "a logger imported",
            ),
            (
                COMMAND_MODULE,
                "extern crate tracing;".into(),
                "a logger imported",
            ),
            // An output stream.
            (
                COMMAND_MODULE,
                command("let _ = writeln!(std::io::stderr(), \"key saved\");"),
                "`stderr` names an output stream",
            ),
        ]
    }

    /// An invoke handler wrapped around `generate_handler!` that logs each
    /// invocation (the DoD critic of round 2, iteration 3): it prints with
    /// `print`, the invocation's body with its arguments (`save_klipy_key`'s
    /// is the KLIPY key).
    fn logged(print: &str) -> String {
        format!(
            "fn logged<R: Runtime>(\
             handler: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,\
             ) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {{ \
             move |invoke| {{ {print}; handler(invoke) }} }}\n\
             fn run() -> R {{ tauri::Builder::default().invoke_handler(logged(\
             tauri::generate_handler![commands::save_klipy_key])).run(tauri::generate_context!()) }}"
        )
    }

    /// A print, a log or a formatted panic outside `diag.rs`, or what an
    /// invocation is made of named (D-2026-10-01-gif-sticker-search-12, the
    /// DoD critic of round 2, iteration 3): each is refused.
    fn prints_outside_diag() -> Vec<Case> {
        let prints = "prints or logs outside `diag.rs`";
        let formats = "formats its message outside `diag.rs`";
        let unwraps = "panics printing what it holds";
        let read = "(an invocation the window sent) in production code";
        let f = |body: &str| format!("fn f(x: X, r: R, ok: bool) {{ {body} }}");
        vec![
            // The critic's mutant: the IPC wrapper prints each invocation's
            // body; through `diag` too (which does not compile: `report`
            // takes a `DiagCode`), or without naming the invocation's type.
            (
                "lib.rs",
                logged(
                    "eprintln!(\"ipc {} {:?}\", invoke.message.command(), \
                     invoke.message.payload())",
                ),
                "`payload` (an invocation the window sent)",
            ),
            (
                "lib.rs",
                logged(
                    "eprintln!(\"ipc {} {:?}\", invoke.message.command(), \
                     invoke.message.payload())",
                ),
                "`Invoke` (an invocation the window sent)",
            ),
            (
                "lib.rs",
                logged("eprintln!(\"ipc {:?}\", invoke.message)"),
                prints,
            ),
            (
                "lib.rs",
                logged("diag::report(invoke.message.payload())"),
                "`payload` (an invocation the window sent)",
            ),
            (
                "lib.rs",
                "fn run(b: B) -> B { b.invoke_handler(move |invoke| { \
                 tracing::debug!(body = ?invoke.message.payload()); true }) }"
                    .into(),
                prints,
            ),
            // The other mutants: a value printed in the backend, an error
            // logged, a panic that formats a value, the invocation's message
            // named.
            (
                "backend.rs",
                "impl Backend { fn f(&self) { eprintln!(\"{:?}\", self.settings.load()); } }"
                    .into(),
                prints,
            ),
            (
                "storage.rs",
                f("if let Err(e) = r { tracing::warn!(\"{e}\") }"),
                prints,
            ),
            ("studio.rs", f("panic!(\"{x:?}\")"), formats),
            (
                "lib.rs",
                "fn f<R: Runtime>(m: &tauri::ipc::InvokeMessage<R>) {}".into(),
                read,
            ),
            ("lib.rs", "use tauri::ipc::{InvokeBody as B};".into(), read),
            // Other prints and logs: by name, imported, as a crate, an
            // output stream or its file, from `main`.
            ("lib.rs", f("println!(\"{x:?}\")"), prints),
            ("lib.rs", f("log::info!(\"{x:?}\")"), prints),
            (
                "library.rs",
                "use tracing::warn;".into(),
                "a logger imported",
            ),
            ("lib.rs", "extern crate log;".into(), "a logger imported"),
            (
                "lib.rs",
                f("let _ = writeln!(std::io::stderr(), \"{x:?}\");"),
                "`stderr` names an output stream",
            ),
            (
                "tray.rs",
                f("let out: std::io::Stdout = make(); keep(out, x)"),
                "`Stdout` names an output stream",
            ),
            (
                "settings.rs",
                f("let _ = std::fs::write(\"/dev/stderr\", format!(\"{x:?}\"));"),
                "an output stream's file",
            ),
            (
                "main.rs",
                "fn main() -> Result<(), tauri::Error> { bezel_studio::run() }".into(),
                "`main` returns a `Result`",
            ),
            // Panics and assertions that print a value: a formatted
            // message, the operands, the value an `unwrap` holds.
            ("studio.rs", f("unreachable!(\"{}\", x)"), formats),
            ("studio.rs", f("assert!(ok, \"{x:?}\")"), formats),
            ("studio.rs", f("debug_assert!(ok, \"at {}\", x)"), formats),
            (
                "studio.rs",
                f("assert_eq!(x, r)"),
                "prints its operands when it fails",
            ),
            (
                "studio.rs",
                f("r.unwrap_or_else(|e| panic!(\"{e}\"))"),
                formats,
            ),
            ("studio.rs", f("r.expect(&format!(\"{x:?}\"))"), unwraps),
            ("studio.rs", f("r.expect(\"saved\")"), unwraps),
            ("studio.rs", f("r.unwrap()"), unwraps),
            ("studio.rs", f("let _ = rs.map(Result::unwrap);"), unwraps),
            ("studio.rs", f("std::panic::panic_any(x)"), unwraps),
            ("studio.rs", "use std::panic::panic_any;".into(), unwraps),
            // Inside another macro call's tokens.
            (
                "lib.rs",
                f("spawn!(async move { eprintln!(\"{x:?}\") })"),
                prints,
            ),
            (
                "lib.rs",
                f("spawn!(async move { panic!(\"{x:?}\") })"),
                formats,
            ),
            ("lib.rs", f("spawn!(async move { r.unwrap() })"), unwraps),
        ]
    }

    /// `diag.rs` made to say a value of the app's
    /// (D-2026-10-01-gif-sticker-search-12): each is refused.
    fn diag_says_a_value() -> Vec<Case> {
        let takes = "of another type: it takes only `DiagCode` and `&'static str`";
        let said = "in `diag.rs`: it says only what it is given";
        let report = |body: &str| format!("pub fn report(code: DiagCode) {{ {body} }}");
        vec![
            // Functions that take a value.
            ("diag.rs", "pub fn note(text: &str) {}".into(), takes),
            ("diag.rs", "pub fn note(text: String) {}".into(), takes),
            (
                "diag.rs",
                "pub fn note(text: &'static mut str) {}".into(),
                takes,
            ),
            (
                "diag.rs",
                "pub fn note(code: DiagCode, n: u64) {}".into(),
                takes,
            ),
            (
                "diag.rs",
                "pub fn show(value: impl std::fmt::Display) {}".into(),
                takes,
            ),
            (
                "diag.rs",
                "pub fn show<T: std::fmt::Debug>(value: T) {}".into(),
                "is generic",
            ),
            (
                "diag.rs",
                "fn ipc<R: Runtime>(body: &tauri::ipc::InvokeBody) {}".into(),
                takes,
            ),
            (
                "diag.rs",
                "impl DiagCode { pub fn with(self, text: String) {} }".into(),
                takes,
            ),
            (
                "diag.rs",
                "pub fn report(code: Option<DiagCode>) {}".into(),
                takes,
            ),
            // Codes that carry data, other types, traits, `impl`s.
            (
                "diag.rs",
                "pub enum DiagCode { Said(String) }".into(),
                "carries data",
            ),
            ("diag.rs", "pub struct Said(pub String);".into(), said),
            ("diag.rs", "pub trait Say { fn say(&self); }".into(), said),
            (
                "diag.rs",
                "impl From<String> for DiagCode { fn from(s: String) -> Self { Self::A } }".into(),
                said,
            ),
            // State the app writes, read and said: a `static`, another
            // module's, the environment's; an import; a macro of its own.
            (
                "diag.rs",
                "pub static LAST: std::sync::Mutex<String> = \
                 std::sync::Mutex::new(String::new());"
                    .into(),
                said,
            ),
            (
                "diag.rs",
                "thread_local! { static SAID: String = String::new(); }".into(),
                said,
            ),
            (
                "diag.rs",
                report("eprintln!(\"{:?}\", crate::gifs::LAST.lock())"),
                "`crate::gifs::LAST` in `diag.rs`",
            ),
            (
                "diag.rs",
                report("eprintln!(\"{:?}\", std::env::var(\"X\"))"),
                "`std::env::var` in `diag.rs`",
            ),
            ("diag.rs", "use crate::commands::Shared;".into(), said),
            (
                "diag.rs",
                "macro_rules! say { ($x:expr) => { eprintln!(\"{:?}\", $x) } }".into(),
                said,
            ),
            ("diag.rs", "mod inner { fn f() {} }".into(), said),
        ]
    }

    /// A command function named where a local of the same name is not
    /// bound (review W1 of round 2): before the binding, in its own value,
    /// after its scope, with a path: each is refused.
    fn commands_beside_locals() -> Vec<Case> {
        let named = "names a command function";
        vec![
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn preferences(state: State<'_, Shared>) -> R { \
                 let _ = cache_info(state.clone()).await; let cache_info = 1; \
                 Ok(state.preferences()) }"
                    .into(),
                "`cache_info` names a command function",
            ),
            (
                COMMAND_MODULE,
                "fn f(s: S, t: T) -> V { let video_auto = video_auto(s, t); video_auto }".into(),
                "`video_auto` names a command function",
            ),
            (
                COMMAND_MODULE,
                "fn f(s: S) { { let preferences = 1; } let _ = preferences(s); }".into(),
                named,
            ),
            (
                COMMAND_MODULE,
                "fn f(s: S) { let g = |cache_info: u8| cache_info; let _ = cache_info(s); }".into(),
                named,
            ),
            (
                COMMAND_MODULE,
                "fn f(o: Option<u8>, s: S) { if let Some(preferences) = o { keep(preferences) } \
                 else { let _ = preferences(s); } }"
                    .into(),
                named,
            ),
            (
                COMMAND_MODULE,
                "fn f(o: Option<u8>, s: S) -> u8 { let Some(video_auto) = o else { \
                 let _ = video_auto(s); return 0 }; video_auto }"
                    .into(),
                named,
            ),
            (
                COMMAND_MODULE,
                "fn f(s: S, t: T) { let video_auto = 1; let _ = self::video_auto(s, t); }".into(),
                named,
            ),
            (
                COMMAND_MODULE,
                "fn f(v: V) { let video_auto = 1; fn inner(s: S) { video_auto(s); } }".into(),
                named,
            ),
        ]
    }

    /// The source guard reads identifiers, not text
    /// (D-2026-10-01-gif-sticker-search-10, -11, -12): a raw name, a name
    /// passed to a macro, an alias's import, an escaped literal and each
    /// rule's other forms are refused in made-up production files, and so
    /// are the key's text in a macro or a variable ([`key_leaks`]), a proof
    /// made outside the GIF commands ([`proofs_made_elsewhere`]), a page
    /// loaded in the window ([`pages_loaded`]), a command function entered
    /// from Rust ([`commands_called_from_rust`]) or named beside a local of
    /// the same name ([`commands_beside_locals`]), the window's invocation
    /// taken elsewhere ([`invocations_taken_elsewhere`]), a print, a log or
    /// a panic in the command module ([`prints_in_commands`]), a print, a
    /// log, a formatted panic or an invocation's parts outside `diag.rs`
    /// ([`prints_outside_diag`]) and `diag.rs` made to say a value
    /// ([`diag_says_a_value`]); what the studio does (the source factory,
    /// the key file, the GIF commands and their invocation, the handler
    /// list, a method named like a command, a local named like one in
    /// `commands.rs` (review W1 of round 2), a panic with fixed text, what
    /// `diag.rs` is and its calls) is not.
    #[test]
    fn the_source_guard_reads_identifiers_not_text() {
        let call = "macro_rules! call { ($w:ident, $m:ident, $s:expr) => { $w.$m($s) } }";
        let by_macro = format!("{call}\nfn f(w: W) {{ call!(w, eval, \"go()\") }}");
        let refused = [
            ("lib.rs", r##"fn f(w: W) { w.r#eval("go()") }"##, "`eval`"),
            ("lib.rs", by_macro.as_str(), "`eval`"),
            (
                "lib.rs",
                "fn f(w: W) { w.with_webview(|_| {}) }",
                "`with_webview`",
            ),
            (
                "lib.rs",
                "fn f(w: W, r: R) { w.on_message(r, f) }",
                "`on_message`",
            ),
            (
                "lib.rs",
                "use tauri::webview::{InvokeRequest as I};",
                "`InvokeRequest`",
            ),
            (
                "lib.rs",
                r##"fn f() -> S { format!("window.\x5f_TAURI__") }"##,
                "literal",
            ),
            (
                "gifs.rs",
                r##"fn f(c: &str) { eprintln!("saved {c}") }"##,
                "prints",
            ),
            (
                "gifs/key.rs",
                r##"fn f() { ::tracing::info!("saved") }"##,
                "prints",
            ),
            ("gifs.rs", "use log::warn;", "logger"),
            (
                "gifs.rs",
                "fn f(k: &KlipyKey) -> &str { k.expose_secret() }",
                "reads",
            ),
            (
                "lib.rs",
                "fn g(k: &KlipyKey) -> &str { k.r#expose_secret() }",
                "reads",
            ),
            (
                "lib.rs",
                "type K = KlipyClient;",
                "named outside its import",
            ),
            (
                "lib.rs",
                "#[path = \"elsewhere.rs\"]\nmod moved;",
                "classified",
            ),
            ("lib.rs", "include!(\"elsewhere.rs\");", "classified"),
        ];
        let more = [
            key_leaks(),
            proofs_made_elsewhere(),
            pages_loaded(),
            commands_called_from_rust(),
            commands_beside_locals(),
            invocations_taken_elsewhere(),
            prints_in_commands(),
            prints_outside_diag(),
            diag_says_a_value(),
        ];
        let more = more.iter().flatten();
        let refused = refused
            .into_iter()
            .chain(more.map(|(n, t, w)| (*n, t.as_str(), *w)));
        for (name, text, why) in refused {
            let (problems, _) = guard(name, text);
            assert!(
                problems.iter().any(|p| p.contains(why)),
                "{name}: {text}: {problems:#?}"
            );
        }
        let factory = "use bezel_klipy::KlipyClient;\n\
            fn klipy_source() -> F { Arc::new(|_: &UserAsked, key: &KlipyKey, c: &str| \
            Arc::new(KlipyClient::new(key.expose_secret(), c))) }";
        assert_eq!(
            guard("lib.rs", factory),
            (Vec::new(), vec!["klipy_source".to_string()])
        );
        let accepted = [
            (
                KEY_MODULE,
                "impl KlipyKey { pub(crate) fn expose_secret(&self) -> &str { &self.0 } }\n\
                 #[derive(Serialize, Deserialize)]\nstruct KeyJson { \
                 #[serde(serialize_with = \"write_key\", deserialize_with = \"read_key\")] \
                 key: KlipyKey, customer_id: String }\n\
                 fn write_key<S: Serializer>(key: &KlipyKey, serializer: S) -> \
                 Result<S::Ok, S::Error> { serializer.serialize_str(key.expose_secret()) }\n\
                 impl KeyFile { fn save(&self, s: &SavedKey) -> J { KeyJson { \
                 key: s.key.clone(), customer_id: s.customer_id.clone() } } }",
            ),
            (
                COMMAND_MODULE,
                "use tauri::ipc::{Request, Response};\n#[tauri::command]\n\
                 pub async fn search_gifs(request: Request<'_>) -> R { \
                 let asked = UserAsked::of(&request); go(&asked) }",
            ),
            (
                PROOF_MODULE,
                "use tauri::ipc::Request;\n#[derive(Debug)]\n\
                 pub struct UserAsked { _invoked: () }\n\
                 impl UserAsked { pub fn of(_request: &Request<'_>) -> Self { \
                 Self { _invoked: () } } }",
            ),
            // The handler list in `run`; a command's definition, and a
            // method or a field named like one, in a macro's tokens too.
            (
                "lib.rs",
                "fn run() -> R { tauri::Builder::default().invoke_handler(\
                 tauri::generate_handler![commands::search_gifs, commands::r#preferences])\
                 .run(tauri::generate_context!()) }",
            ),
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub fn preferences(state: State<'_, Shared>) -> P { \
                 let p = state.preferences(); Prefs { set_language: format!(\"{}\", \
                 state.set_language), ..p } }",
            ),
            // A local named like a command in `commands.rs` (review W1 of
            // round 2): the reviewer's `fp2` and `fp1`, a parameter, a
            // closure's, a `match` arm's, a condition's, a `for`'s, a
            // `let … else`'s, a field's shorthand, a macro's tokens.
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub async fn video_auto(state: State<'_, Shared>, \
                 theme: ThemeDto) -> UiResult<VideoAutoDto> { \
                 let video_auto = blocking(&state, move |b| b.video_auto(&theme)).await?; \
                 Ok(video_auto) }",
            ),
            (
                COMMAND_MODULE,
                "#[tauri::command]\npub fn preferences(state: State<'_, Shared>) -> P { \
                 let preferences = state.preferences(); preferences }",
            ),
            (
                COMMAND_MODULE,
                "fn keep(video_auto: VideoAutoDto) -> VideoAutoDto { video_auto }",
            ),
            (
                COMMAND_MODULE,
                "fn f(v: V) -> W { v.into_iter().map(|cache_info| cache_info).collect() }",
            ),
            (
                COMMAND_MODULE,
                "fn f(r: R) -> X { match r { Ok(preferences) => preferences, \
                 Err(e) => e.into() } }",
            ),
            (
                COMMAND_MODULE,
                "fn f(o: Option<X>) -> X { if let Some(cache_info) = o && cache_info.ok \
                 { cache_info } else { X::default() } }",
            ),
            (
                COMMAND_MODULE,
                "fn f(o: Option<X>) { while let Some(preferences) = next(o) { keep(preferences) } \
                 for cache_info in o { keep(cache_info) } }",
            ),
            (
                COMMAND_MODULE,
                "fn f(o: Option<X>) -> X { let Some(video_auto) = o else { return X::default() }; \
                 video_auto }",
            ),
            (
                COMMAND_MODULE,
                "fn f(s: S) -> Dto { let video_auto = s.video_auto(); \
                 let preferences = format!(\"{}\", video_auto); Dto { video_auto, preferences } }",
            ),
            // `diag.rs` as it is, its codes said, panics with fixed text,
            // `write!` to a formatter, a lint's `expect` attribute.
            (DIAG_MODULE, include_str!("diag.rs")),
            (
                DIAG_MODULE,
                "pub fn note(code: DiagCode, text: &'static str) { \
                 tracing::warn!(code = ?code, \"{text}\") }\n\
                 impl DiagCode { pub const fn text(self) -> &'static str { \"x\" } }",
            ),
            (
                "lib.rs",
                "fn f(r: R) { if r.is_err() { diag::report(DiagCode::NotStarted); } }",
            ),
            (
                "studio.rs",
                "fn f(ok: bool) { assert!(ok); debug_assert!(ok, \"a {{braced}} text\"); \
                 if !ok { panic!(\"fixed text\") } unreachable!() }",
            ),
            (
                "messages.rs",
                "impl fmt::Display for E { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> \
                 fmt::Result { write!(f, \"{{{}}}\", self.0) } }",
            ),
            (
                "studio.rs",
                "#[expect(dead_code)]\nfn f(m: &Mutex<u8>) -> u8 { \
                 *m.lock().unwrap_or_else(PoisonError::into_inner) }",
            ),
        ];
        for (name, text) in accepted {
            assert_eq!(
                guard(name, text),
                (Vec::new(), Vec::new()),
                "{name}: {text}"
            );
        }
        let second = "fn warm_up() { bezel_klipy::KlipyClient::new(\"k\", \"c\"); }";
        let (problems, made) = guard("lib.rs", &format!("{factory}\n{second}"));
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(made, ["klipy_source", "warm_up"]);
    }
}
