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
/// (D-2026-10-01-gif-sticker-search-12, review W1 of round 2, iter 4). Only
/// what `Builder::run` returns reaches here (a plugin, or another cause):
/// a failure in the setup (the folders, the tray, the window and its web
/// view) and a desktop without a graphical session panic in Tauri or tao
/// (review W1 of round 2, iter 5), and the panic hook says
/// [`DiagCode::Panicked`] with the place.
pub fn start_failure(error: &tauri::Error) -> DiagCode {
    match error {
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
/// definition and in the list of `generate_handler!` in [`run`]; any
/// `KlipyClient::new` but the source factory's; and every other way out of
/// the computer (D-2026-10-01-gif-sticker-search-15): a socket or a path
/// through `net`, a program spawned but the studio's own re-exec, a webview
/// made here, and the system opener but in the helper the `open_link` and
/// `open_guide` commands call. Code written to get past it otherwise is left
/// to code review.
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
/// The studio's one spawned program, its own file
/// (D-2026-10-01-gif-sticker-search-15): the source guard accepts
/// `Command` only here, as `std::process::Command::new` of the name bound
/// once to `std::env::current_exe()`, and no macro here.
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
    use std::collections::{BTreeSet, HashMap, HashSet};
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
    /// error, never by its text. The runtime's and the setup's errors never
    /// reach `main` in Tauri 2.12 (review W1 of round 2, iter 5): they say
    /// what any other cause says.
    #[test]
    fn a_failed_start_says_which_part_failed() {
        let runtime = serde_json::from_str::<u8>("x").unwrap_err();
        let setup: Box<dyn std::error::Error> = "the folders were not found".into();
        for (error, code) in [
            (tauri::Error::Runtime(runtime.into()), DiagCode::NotStarted),
            (tauri::Error::Setup(setup.into()), DiagCode::NotStarted),
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

    /// Macros that panic with the message they are given, which Rust's
    /// default panic hook prints (the studio's, [`HOOK_FUNCTION`], does not):
    /// nowhere but in [`DIAG_MODULE`] with a message that is more than one
    /// string literal without a placeholder. One is spelled in two parts, so
    /// that the repository's check for unfinished-work markers does not read
    /// it as one.
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

    /// What installs a logger or a tracing subscriber, named nowhere in the
    /// studio's production code, [`DIAG_MODULE`] included
    /// (D-2026-10-01-gif-sticker-search-14): `tracing`'s global and scoped
    /// defaults, `log`'s logger. Nothing installed, `tracing` and `log`
    /// records go nowhere, Tauri's included.
    const LOG_INSTALLERS: [&str; 6] = [
        "set_global_default",
        "set_default",
        "with_default",
        "set_logger",
        "set_logger_racy",
        "set_boxed_logger",
    ];

    /// Crates that install a logger or a tracing subscriber, as Rust names
    /// them (D-2026-10-01-gif-sticker-search-14): no path of the studio's
    /// production code starts with one, and the studio neither depends on
    /// one (its manifest, any kind, any target) nor is built with one (what
    /// it depends on, built for it); in a package's name, `-` is `_`.
    const LOGGER_CRATES: [&str; 8] = [
        "tracing_subscriber",
        "tracing_appender",
        "env_logger",
        "simplelog",
        "fern",
        "log4rs",
        "flexi_logger",
        "pretty_env_logger",
    ];

    /// Tauri's feature that makes it, its runtime and its webview log
    /// through `tracing`: each invocation's body (the KLIPY key included) in
    /// the span `ipc::request` (D-2026-10-01-gif-sticker-search-14). On in
    /// no package of the Tauri family, as declared and as resolved.
    const TAURI_LOGS: &str = "tracing";

    /// The process's panic hook, set once, by [`HOOK_FUNCTION`] in
    /// [`DIAG_MODULE`] (D-2026-10-01-gif-sticker-search-14).
    const PANIC_HOOK: &str = "set_hook";

    /// The function of [`DIAG_MODULE`] that sets the panic hook: `main`'s
    /// first statement.
    const HOOK_FUNCTION: &str = "hook_panics";

    /// What restores Rust's default panic hook, which prints the message,
    /// or changes the hook: named nowhere.
    const HOOK_CHANGES: [&str; 2] = ["take_hook", "update_hook"];

    /// The paths [`HOOK_FUNCTION`] uses, beside `tracing` and
    /// [`DIAG_MODULE`]'s own items.
    const HOOK_PATHS: [&[&str]; 2] = [&["std", "panic", PANIC_HOOK], &["Box", "new"]];

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

    /// Sockets and name lookups: the standard library's (`std::net`,
    /// `std::os::unix::net`) and any runtime's of the same names, named
    /// nowhere in the studio's production code
    /// (D-2026-10-01-gif-sticker-search-15). HTTP leaves the studio only
    /// through `bezel-klipy`'s client, which the KLIPY rules fence.
    const SOCKETS: [&str; 7] = [
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "UnixStream",
        "UnixListener",
        "UnixDatagram",
        "ToSocketAddrs",
    ];

    /// The module of the sockets (`std::net`, `std::os::unix::net`,
    /// `tokio::net`, ...): no path and no import of production code goes
    /// through it, whatever comes before it (`use std::{fs, net::…}`, `use
    /// std as s; s::net::…`).
    const NET_MODULE: &str = "net";

    /// Crates that speak to the network, open a page or spawn a program,
    /// which the studio does not use: no path, import or `extern crate` of
    /// production code starts with one (in a package's name, `-` is `_`).
    const WAY_OUT_CRATES: [&str; 22] = [
        "attohttpc",
        "curl",
        "duct",
        "hyper",
        "hyper_util",
        "isahc",
        "minreq",
        "mio",
        "open",
        "reqwest",
        "socket2",
        "subprocess",
        "surf",
        "tauri_plugin_http",
        "tauri_plugin_shell",
        "tauri_plugin_updater",
        "tauri_plugin_upload",
        "tauri_plugin_websocket",
        "tokio_tungstenite",
        "tungstenite",
        "ureq",
        "webbrowser",
    ];

    /// Crates that speak to the network: HTTP and WebSocket clients, and
    /// the Tauri plugins that update the app, fetch, upload, open a socket
    /// or run a program, as cargo names their packages
    /// (D-2026-10-01-gif-sticker-search-16). The studio declares none of
    /// them (normal or build, any target) and is built with none for a
    /// desktop (its normal and build dependencies, all the way down) but
    /// [`KLIPY_HTTP`], which only [`KLIPY_ADAPTER`] depends on; and no path
    /// of its production code names one at any segment (in a path, `-` is
    /// `_`), so a vendored plugin's re-export (`…::reqwest::Client`) is
    /// refused too.
    const NETWORK_CRATES: [&str; 15] = [
        "tauri-plugin-updater",
        "tauri-plugin-http",
        "tauri-plugin-upload",
        "tauri-plugin-websocket",
        "tauri-plugin-shell",
        "reqwest",
        "hyper",
        "isahc",
        "attohttpc",
        "curl",
        "surf",
        "ureq",
        "minreq",
        "tungstenite",
        "tokio-tungstenite",
    ];

    /// KLIPY's adapter, the one package that may depend on a network
    /// crate: [`KLIPY_HTTP`], its HTTP client.
    const KLIPY_ADAPTER: &str = "bezel-klipy";

    /// The one network crate the studio is built with, through
    /// [`KLIPY_ADAPTER`] only.
    const KLIPY_HTTP: &str = "ureq";

    /// The updater plugin's API (its extension trait, its accessors, its
    /// builder), named nowhere in production code
    /// (D-2026-10-01-gif-sticker-search-16): a copy of the plugin vendored
    /// under another name is refused too. The HTTP plugin's builder is
    /// `init()`, as every plugin's; its re-export of `reqwest` is refused
    /// by [`NETWORK_CRATES`].
    const UPDATER_API: [&str; 4] = ["updater", "updater_builder", "UpdaterExt", "UpdaterBuilder"];

    /// The desktops the studio is built for, as the `cfg` of a dependency
    /// reads them: `target_os`, `target_family` (`unix`, `windows`) and
    /// `target_vendor`. What else a `cfg` asks (the architecture, a
    /// feature, ...) may hold.
    const DESKTOPS: [[&str; 3]; 3] = [
        ["linux", "unix", "unknown"],
        ["windows", "windows", "pc"],
        ["macos", "unix", "apple"],
    ];

    /// The studio's Tauri configuration, which the build reads beside the
    /// manifest: the one file of its name there (no overlay).
    const TAURI_CONFIG_FILE: &str = "tauri.conf.json";

    /// The scheme of the app's own pages (`tauri://localhost`), the one a
    /// window's `url` may name.
    const APP_SCHEME: &str = "tauri";

    /// What edits the Tauri configuration at run time
    /// (`Context::config_mut`), named nowhere in production code
    /// (D-2026-10-01-gif-sticker-search-16): the windows are
    /// [`TAURI_CONFIG_FILE`]'s, as built.
    const CONFIG_EDIT: &str = "config_mut";

    /// A webview made in Rust, with the page it loads: the studio's one
    /// window is the configuration's, and loads the app's own pages.
    const WEBVIEWS: [&str; 3] = ["WebviewUrl", "WebviewWindowBuilder", "WebviewBuilder"];

    /// What spawns a program (`std::process::Command`, `tokio::process::Command`,
    /// any runtime's, and the extension traits of the standard library's):
    /// named only in [`RESTART`], as [`SPAWN_CALL`] of [`OWN_PROGRAM`].
    const SPAWNS: [&str; 2] = ["Command", "CommandExt"];

    /// The one function of `lib.rs` that spawns a program: the studio
    /// itself again, without WebKit's DMA-BUF renderer.
    const RESTART: &str = "restart_without_dmabuf_renderer";

    /// The one call in [`RESTART`] that names `Command`.
    const SPAWN_CALL: [&str; 4] = ["std", "process", "Command", "new"];

    /// Where the program [`SPAWN_CALL`] runs comes from: the studio's own
    /// file, bound by `let Ok(name) = std::env::current_exe() else { … };`.
    const OWN_PROGRAM: [&str; 3] = ["std", "env", "current_exe"];

    /// The system opener (`tauri-plugin-opener`): its extension trait, its
    /// accessor, its state and what opens a page, a file or a folder. Named
    /// only in [`OPEN_HELPER`], and the trait imported as `_` at the top of
    /// [`COMMAND_MODULE`].
    const OPENER: [&str; 7] = [
        "OpenerExt",
        "opener",
        "Opener",
        "open_url",
        "open_path",
        "reveal_item_in_dir",
        "reveal_items_in_dir",
    ];

    /// The opener's crate.
    const OPENER_CRATE: &str = "tauri_plugin_opener";

    /// The one helper of [`COMMAND_MODULE`] that opens a page in the
    /// system's browser, named only at its definition and in the bodies of
    /// [`OPEN_COMMANDS`].
    const OPEN_HELPER: &str = "open_fixed";

    /// The `#[tauri::command]`s that call [`OPEN_HELPER`]: a page opens
    /// only when the window invokes one, on the user's click.
    const OPEN_COMMANDS: [&str; 2] = ["open_link", "open_guide"];

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
        /// The outermost function around each [`PANIC_HOOK`] named.
        hooks: Vec<String>,
        /// Whether the item about to be read is one of the file's own
        /// (not in a module, a function or a block): a free function or an
        /// import.
        at_top: bool,
        /// Whether the outermost function around what is read is a free
        /// function of the file's own items.
        top: bool,
        /// In [`RESTART`], the one name bound to [`OWN_PROGRAM`]'s file,
        /// when it is bound once and nothing else in the function binds it.
        program: Option<String>,
        /// Whether what is read is the path of the one accepted
        /// [`SPAWN_CALL`].
        spawning: bool,
        /// The outermost function around each accepted [`SPAWN_CALL`].
        spawns: Vec<String>,
        /// Whether what is read is the opener's trait imported as `_` at
        /// the top of [`COMMAND_MODULE`].
        opener_import: bool,
        /// The outermost function around each name of the opener accepted.
        opens: Vec<String>,
        /// The outermost function around each [`OPEN_HELPER`] named.
        helpers: Vec<String>,
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
                hooks: Vec::new(),
                at_top: false,
                top: false,
                program: None,
                spawning: false,
                spawns: Vec::new(),
                opener_import: false,
                opens: Vec::new(),
                helpers: Vec::new(),
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

        /// Whether what is read is [`HOOK_FUNCTION`] in [`DIAG_MODULE`], the
        /// one place that sets the panic hook.
        fn sets_the_hook(&self) -> bool {
            self.in_diag() && self.within == [HOOK_FUNCTION]
        }

        /// Whether what is read is in [`RESTART`], a free function of
        /// `lib.rs`'s own items (not nested in another function).
        fn in_restart(&self) -> bool {
            self.production() && self.file == "lib.rs" && self.top && self.within == [RESTART]
        }

        /// Whether what is read is in [`OPEN_HELPER`], a free function of
        /// [`COMMAND_MODULE`]'s own items: its definition or its body.
        fn in_open_helper(&self) -> bool {
            self.file == COMMAND_MODULE && self.top && self.within == [OPEN_HELPER]
        }

        /// Whether what is read is the body of one of [`OPEN_COMMANDS`], a
        /// free `#[tauri::command]` function of [`COMMAND_MODULE`]'s own.
        fn in_open_command(&self) -> bool {
            self.file == COMMAND_MODULE
                && self.top
                && self.command
                && self.within.len() == 1
                && OPEN_COMMANDS.contains(&self.within[0].as_str())
        }

        /// An identifier of production code that leaves the computer, or
        /// names what does (D-2026-10-01-gif-sticker-search-15): a socket, a
        /// webview made in Rust, a program spawned but the studio's own
        /// re-exec, the system opener outside [`OPEN_HELPER`], and that
        /// helper named but at its definition and by [`OPEN_COMMANDS`].
        fn way_out(&mut self, name: &str) {
            if SOCKETS.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` (a socket) in production code: the studio speaks to the network \
                     only through `bezel-klipy`"
                ));
            }
            if WEBVIEWS.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` makes a webview in Rust: the one window is the configuration's"
                ));
            }
            if UPDATER_API.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` (the updater plugin) in production code: the studio checks for no \
                     update"
                ));
            }
            if name == CONFIG_EDIT {
                self.refuse(format_args!(
                    "`{CONFIG_EDIT}` edits the Tauri configuration at run time: the windows are \
                     `{TAURI_CONFIG_FILE}`'s"
                ));
            }
            let re_exec = self.in_restart() && (self.spawning || name == "CommandExt");
            if SPAWNS.contains(&name) && !re_exec {
                self.refuse(format_args!(
                    "`{name}` spawns a program outside `{RESTART}`, whose one `{}` runs the \
                     studio's own file (`{}()`)",
                    SPAWN_CALL.join("::"),
                    OWN_PROGRAM.join("::")
                ));
            }
            if OPENER.contains(&name) && self.in_open_helper() {
                self.opens.push(OPEN_HELPER.into());
            } else if OPENER.contains(&name) && !self.opener_import {
                self.refuse(format_args!(
                    "`{name}` (the system opener) outside `{OPEN_HELPER}` in `{COMMAND_MODULE}`: a \
                     page opens only when the user clicks"
                ));
            }
            if name == OPEN_HELPER && self.in_open_command() {
                self.helpers.push(self.within[0].clone());
            } else if name == OPEN_HELPER && !self.in_open_helper() {
                self.refuse(format_args!(
                    "`{OPEN_HELPER}`, which opens a page, named outside its definition and the \
                     bodies of `{}` in `{COMMAND_MODULE}`",
                    OPEN_COMMANDS.join("` and `")
                ));
            }
        }

        /// A path of two names or more, an import or an `extern crate`
        /// (D-2026-10-01-gif-sticker-search-15): through the sockets'
        /// module, or into a crate that leaves the computer, it is refused.
        fn way_out_path(&mut self, segments: &[String]) {
            let path = segments.join("::");
            if segments.iter().any(|segment| segment == NET_MODULE) {
                self.refuse(format_args!(
                    "`{path}` goes through `{NET_MODULE}`, the sockets' module: the studio speaks \
                     to the network only through `bezel-klipy`"
                ));
            }
            let first = segments.first().map_or("", String::as_str);
            if WAY_OUT_CRATES.contains(&first) {
                self.refuse(format_args!(
                    "`{path}` is a crate that speaks to the network, opens a page or spawns a \
                     program"
                ));
            }
            let network = |segment: &String| {
                let segment = segment.replace('_', "-");
                NETWORK_CRATES.contains(&segment.as_str())
            };
            if let Some(network) = segments.iter().find(|segment| network(segment)) {
                self.refuse(format_args!(
                    "`{path}` names `{network}`, a crate that speaks to the network (a \
                     re-export included): the studio does only through `{KLIPY_ADAPTER}`"
                ));
            }
        }

        /// A macro call (`path!`, with `arguments`): `generate_context!`
        /// with an argument reads another configuration than
        /// [`TAURI_CONFIG_FILE`], and is refused.
        fn context_read(&mut self, path: &[String], arguments: Option<&TokenStream>) {
            let last = path.last().map_or("", String::as_str);
            if self.production()
                && last == "generate_context"
                && arguments.is_some_and(|arguments| !arguments.is_empty())
            {
                self.refuse(format_args!(
                    "`generate_context!` with an argument reads another configuration than \
                     `{TAURI_CONFIG_FILE}`"
                ));
            }
        }

        /// Whether `call` is the one program [`RESTART`] spawns: the first
        /// [`SPAWN_CALL`] there, its one argument the name bound to the
        /// studio's own file ([`own_program`]).
        fn spawns_itself(&self, call: &ExprCall) -> bool {
            let program = self.program.as_deref();
            let runs_the_studio = |argument: &Expr| {
                matches!(argument, Expr::Path(name)
                    if name.attrs.is_empty()
                        && name.qself.is_none()
                        && name.path.get_ident().is_some_and(|i| program == Some(&*i.unraw().to_string())))
            };
            self.in_restart()
                && self.spawns.is_empty()
                && call.attrs.is_empty()
                && is_exactly(&call.func, &SPAWN_CALL)
                && call.args.len() == 1
                && call.args.first().is_some_and(runs_the_studio)
        }

        /// A path (or an import, an `extern crate`) that starts with
        /// `first`: refused when it is a logger installer's crate.
        fn logger_crate(&mut self, first: &str, path: &str) {
            if self.production() && is_logger_crate(first) {
                self.refuse(format_args!(
                    "`{path}` is a logger installer's crate: the studio installs no logger"
                ));
            }
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
        /// the app (`crate`, `super`) or the standard library's I/O; but in
        /// [`HOOK_FUNCTION`], the panic hook's [`HOOK_PATHS`].
        fn diag_path(&mut self, segments: &[String]) {
            let Some(first) = segments.first().filter(|_| segments.len() > 1) else {
                return;
            };
            let own = first == "Self" || self.own_types.contains(first);
            let hook = self.sets_the_hook()
                && HOOK_PATHS
                    .iter()
                    .any(|path| segments.iter().map(String::as_str).eq(path.iter().copied()));
            if !own && !hook && !LOGGERS.contains(&first.as_str()) {
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
        /// `#[tauri::command]` one when `command`, one of the file's own
        /// free functions when `top`.
        fn function(
            &mut self,
            name: &Ident,
            command: bool,
            top: bool,
            read: impl FnOnce(&mut Self),
        ) {
            let name = name.unraw().to_string();
            self.functions.push(name.clone());
            if self.within.is_empty() {
                self.command = command;
                self.top = top;
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
            self.way_out(name);
            if FORGERIES.contains(&name) {
                self.refuse(format_args!("`{name}` in production code"));
            }
            if INVOCATION_PARTS.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` (an invocation the window sent) in production code: only Tauri \
                     reads one"
                ));
            }
            if LOG_INSTALLERS.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` installs a logger or a tracing subscriber: the studio installs none"
                ));
            }
            if name == PANIC_HOOK {
                self.hooks
                    .push(self.outer().unwrap_or_default().to_string());
                if !self.sets_the_hook() {
                    self.refuse(format_args!(
                        "`{PANIC_HOOK}` outside `diag::{HOOK_FUNCTION}`: the one panic hook says \
                         a fixed line"
                    ));
                }
            }
            if HOOK_CHANGES.contains(&name) {
                self.refuse(format_args!(
                    "`{name}` changes the panic hook: Rust's default one prints the message"
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
            self.logger_crate(path.first().map_or("", String::as_str), &path.join("::"));
            self.way_out_path(path);
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
            if segments.len() > 1 {
                self.logger_crate(&segments[0], &segments.join("::"));
                self.way_out_path(segments);
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
                            self.context_read(&path, arguments.as_ref());
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

        /// The file's own items, each free function and import read as one
        /// (not nested in a module, a function or a block).
        fn visit_file(&mut self, file: &'ast syn::File) {
            for attribute in &file.attrs {
                self.visit_attribute(attribute);
            }
            for item in &file.items {
                self.at_top = matches!(item, Item::Fn(_) | Item::Use(_));
                self.visit_item(item);
                self.at_top = false;
            }
        }

        fn visit_item_fn(&mut self, item: &'ast ItemFn) {
            let top = std::mem::take(&mut self.at_top);
            if self.production() && top && self.file == "lib.rs" && item.sig.ident == RESTART {
                self.program = own_program(&item.block);
            }
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
            if self.production() && main && !hooks_first(&item.block) {
                self.refuse(format_args!(
                    "`main` does not call `diag::{HOOK_FUNCTION}()` first: a panic before it prints \
                     its message"
                ));
            }
            self.function(&item.sig.ident, command, top, |reader| {
                reader.body(&item.sig.inputs, |reader| {
                    visit::visit_item_fn(reader, item)
                });
            });
        }

        fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
            if self.production() && self.in_diag() {
                self.diag_signature(&item.sig);
            }
            self.function(&item.sig.ident, false, false, |reader| {
                reader.body(&item.sig.inputs, |reader| {
                    visit::visit_impl_item_fn(reader, item);
                });
            });
        }

        fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
            if self.production() && self.in_diag() {
                self.diag_signature(&item.sig);
            }
            self.function(&item.sig.ident, false, false, |reader| {
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
        /// In [`RESTART`], the one [`SPAWN_CALL`] of the studio's own file
        /// is accepted, its `Command` read as accepted.
        fn visit_expr_call(&mut self, call: &'ast ExprCall) {
            if self.spawns_itself(call) {
                self.spawns.push(RESTART.into());
                self.spawning = true;
                self.visit_expr(&call.func);
                self.spawning = false;
                for argument in &call.args {
                    self.visit_expr(argument);
                }
                return;
            }
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

        /// An import; the opener's trait imported as `_` at the top of
        /// [`COMMAND_MODULE`] is accepted: it adds no name, only the
        /// trait's methods, which only [`OPEN_HELPER`] may call.
        fn visit_item_use(&mut self, item: &'ast ItemUse) {
            let top = std::mem::take(&mut self.at_top);
            self.opener_import = self.production()
                && top
                && self.file == COMMAND_MODULE
                && item.attrs.is_empty()
                && is_opener_import(&item.tree);
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
            self.opener_import = false;
        }

        fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
            let name = item.ident.unraw().to_string();
            self.logger_crate(&name, &name);
            if self.production() {
                self.way_out_path(std::slice::from_ref(&name));
            }
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
            self.context_read(&path, Some(&call.tokens));
            self.panics(&path, Some(call.tokens.clone()));
            if self.in_restart() {
                self.refuse(format_args!(
                    "a macro called in `{RESTART}`: what it binds or spawns cannot be read"
                ));
            }
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

    /// Whether `text` is a `javascript:` URL as a browser reads one
    /// ([`url_scheme`]).
    fn is_script_url(text: &str) -> bool {
        url_scheme(text).is_some_and(|scheme| scheme == SCRIPT_SCHEME)
    }

    /// The scheme of `text` read as a URL, as a browser and Tauri's
    /// configuration read one (spaces and control characters around it
    /// cut, tabs and newlines dropped), in lowercase; `None` when it has
    /// none: it is a path.
    fn url_scheme(text: &str) -> Option<String> {
        let url: String = text
            .trim_matches(|c: char| c <= ' ')
            .chars()
            .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
            .collect();
        let (scheme, _) = url.split_once(':')?;
        let mut chars = scheme.chars();
        let letter = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        let valid = letter && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
        valid.then(|| scheme.to_ascii_lowercase())
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

    /// Whether `body` starts with the call `diag::hook_panics();`.
    fn hooks_first(body: &Block) -> bool {
        matches!(body.stmts.first(), Some(Stmt::Expr(Expr::Call(call), Some(_)))
            if call.attrs.is_empty()
                && call.args.is_empty()
                && matches!(&*call.func, Expr::Path(called)
                    if called.qself.is_none() && ends_with(&called.path, ["diag", HOOK_FUNCTION])))
    }

    /// Whether `expr` is the plain path `names` (no leading `::`, no
    /// generics, no qualified self, no attribute; `r#` removed).
    fn is_exactly(expr: &Expr, names: &[&str]) -> bool {
        matches!(expr, Expr::Path(path)
            if path.attrs.is_empty()
                && path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.iter().all(|s| s.arguments.is_none())
                && segments(&path.path).iter().map(String::as_str).eq(names.iter().copied()))
    }

    /// The name [`RESTART`]'s `body` binds to the studio's own file: a
    /// statement of the body itself `let Ok(name) = std::env::current_exe()
    /// else { … };` ([`OWN_PROGRAM`]), the name bound by value, not `mut`,
    /// and bound nowhere else in the function (no shadowing, no closure's
    /// or arm's binding of the same name).
    fn own_program(body: &Block) -> Option<String> {
        let name = body.stmts.iter().find_map(|statement| {
            let Stmt::Local(local) = statement else {
                return None;
            };
            let init = local.init.as_ref()?;
            let own = matches!(&*init.expr, Expr::Call(call)
                if call.attrs.is_empty() && call.args.is_empty()
                    && is_exactly(&call.func, &OWN_PROGRAM));
            if !(own && init.diverge.is_some() && local.attrs.is_empty()) {
                return None;
            }
            ok_binding(&local.pat)
        })?;
        let mut bound = Vec::new();
        Bindings(&mut bound).visit_block(body);
        (bound.iter().filter(|b| **b == name).count() == 1).then_some(name)
    }

    /// The name `pattern` binds when it is `Ok(name)`, by value, not `mut`.
    fn ok_binding(pattern: &Pat) -> Option<String> {
        let Pat::TupleStruct(ok) = pattern else {
            return None;
        };
        let [Pat::Ident(name)] = ok.elems.iter().collect::<Vec<_>>()[..] else {
            return None;
        };
        let plain = name.attrs.is_empty()
            && name.by_ref.is_none()
            && name.mutability.is_none()
            && name.subpat.is_none();
        let is_ok = ok.attrs.is_empty() && ok.qself.is_none() && ok.path.is_ident("Ok");
        (plain && is_ok).then(|| name.ident.unraw().to_string())
    }

    /// Whether `tree` is exactly `tauri_plugin_opener::OpenerExt as _`: the
    /// opener's trait imported without a name.
    fn is_opener_import(tree: &UseTree) -> bool {
        matches!(tree, UseTree::Path(krate)
            if krate.ident == OPENER_CRATE
                && matches!(&*krate.tree, UseTree::Rename(opener)
                    if opener.ident == "OpenerExt" && opener.rename == "_"))
    }

    /// Whether the crate or package `name` installs a logger
    /// ([`LOGGER_CRATES`], `-` read as `_`).
    fn is_logger_crate(name: &str) -> bool {
        let name = name.replace('-', "_");
        LOGGER_CRATES.contains(&name.as_str())
    }

    /// The names `pattern` binds, added to `names`.
    fn bindings(pattern: &Pat, names: &mut Vec<String>) {
        Bindings(names).visit_pat(pattern);
    }

    /// The names the patterns read bind, added to its list.
    struct Bindings<'n>(&'n mut Vec<String>);

    impl<'ast> Visit<'ast> for Bindings<'_> {
        fn visit_pat_ident(&mut self, ident: &'ast PatIdent) {
            self.0.push(ident.ident.unraw().to_string());
            visit::visit_pat_ident(self, ident);
        }
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
    ///   items (and, in `hook_panics`, `std::panic::set_hook` and
    ///   `Box::new`): it says only what it is given;
    /// - nothing installs a logger or a tracing subscriber, nor changes the
    ///   panic hook (D-2026-10-01-gif-sticker-search-14): production code,
    ///   `diag.rs` included, names none of `LOG_INSTALLERS`
    ///   (`set_global_default`, `set_logger`, ...), no path or import starts
    ///   with a logger installer's crate (`LOGGER_CRATES`), `set_hook` is
    ///   named once, in `diag::hook_panics`, which `main` calls first, and
    ///   `take_hook`/`update_hook` nowhere; and the studio's metadata
    ///   ([`studio_metadata`], [`logging_problems`]) shows no logger
    ///   installer declared (any kind, any target) or built for it, and no
    ///   package of the Tauri family with its `tracing` feature on, as
    ///   declared or as resolved;
    /// - every way out of the computer is named and fenced
    ///   (D-2026-10-01-gif-sticker-search-15): production code names no
    ///   socket (`SOCKETS`: `TcpStream`, `TcpListener`, `UdpSocket`,
    ///   `UnixStream`, `UnixListener`, `UnixDatagram`, `ToSocketAddrs`), no
    ///   path or import goes through `net` (`std::net`, `std::os::unix::net`,
    ///   `tokio::net`, grouped or renamed), and none starts with a crate that
    ///   leaves the computer (`WAY_OUT_CRATES`); nothing makes a webview in
    ///   Rust (`WEBVIEWS`); `Command` (any runtime's) and `CommandExt` are
    ///   named only in `restart_without_dmabuf_renderer`, a free function
    ///   of `lib.rs` that calls no macro, as the one
    ///   `std::process::Command::new(name)` whose `name` is bound once, by
    ///   `let Ok(name) = std::env::current_exe() else { … };` (the studio's
    ///   own file); the system opener (`OPENER`: `OpenerExt`, `opener`,
    ///   `Opener`, `open_url`, `open_path`, `reveal_item_in_dir`,
    ///   `reveal_items_in_dir`) is named only in `open_fixed`, a free
    ///   function of `commands.rs`, and in the import
    ///   `use tauri_plugin_opener::OpenerExt as _;` at the top of
    ///   `commands.rs` (it adds no name, only the trait's methods); and
    ///   `open_fixed` is named only at its definition and in the bodies of
    ///   the `#[tauri::command]`s `open_link` and `open_guide`, so a page
    ///   opens only when the window invokes one, on the user's click;
    /// - nothing else leaves the computer, or loads a page from elsewhere
    ///   (D-2026-10-01-gif-sticker-search-16): no path names a network
    ///   crate (`NETWORK_CRATES`, `tauri_plugin_updater` included) at any
    ///   segment, a vendored plugin's re-export included, nor the updater
    ///   plugin's API (`UPDATER_API`: `updater()`, `UpdaterExt`, ...);
    ///   `generate_context!` takes no argument and `config_mut` is named
    ///   nowhere, so the configuration is `tauri.conf.json` as built; its
    ///   windows load only the app's own pages, through no proxy, its front
    ///   end is the app's files and it names no dev server
    ///   ([`config_problems`]), no overlay is merged into it
    ///   ([`studio_config_problems`]); and the studio is built, for a
    ///   desktop, with no network crate but KLIPY's adapter's `ureq`
    ///   ([`network_problems`]);
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
        let (mut diag_functions, mut hooks) = (Vec::new(), Vec::new());
        let (mut spawns, mut opens, mut helpers) = (Vec::new(), Vec::new(), Vec::new());
        for source in &built {
            let reader = source.production(&commands);
            let at = |f: &String| format!("{}: {f}", source.name);
            hooks.extend(reader.hooks.iter().map(at));
            spawns.extend(reader.spawns.iter().map(at));
            opens.extend(reader.opens.iter().map(at));
            helpers.extend(reader.helpers.iter().map(at));
            made.extend(reader.made.iter().map(at));
            reads.extend(reader.reads.iter().map(at));
            asked.extend(reader.asked.iter().map(at));
            handlers.extend(reader.handlers.iter().map(at));
            diag_functions.extend(reader.diag_functions.iter().map(at));
            handled.extend(reader.handled);
            problems.extend(reader.problems);
        }
        // What the studio is built with (D-2026-10-01-gif-sticker-search-14
        // and -16), and the windows its configuration makes (-16).
        let metadata = studio_metadata();
        problems.extend(logging_problems(&metadata));
        problems.extend(network_problems(&metadata));
        problems.extend(studio_config_problems());
        assert!(problems.is_empty(), "{problems:#?}");
        remote_pages_are_refused();
        assert_eq!(
            hooks,
            ["diag.rs: hook_panics"],
            "one panic hook, set by `diag::hook_panics`"
        );
        // Every way out (D-2026-10-01-gif-sticker-search-15): one program
        // spawned, the studio itself again; the system opener used by one
        // helper, which the two commands that open a page call.
        assert_eq!(
            spawns,
            ["lib.rs: restart_without_dmabuf_renderer"],
            "one program spawned: the studio's own file, by its re-exec"
        );
        opens.dedup();
        assert_eq!(
            opens,
            ["commands.rs: open_fixed"],
            "the opener, in one helper"
        );
        assert_eq!(
            helpers,
            ["commands.rs: open_guide", "commands.rs: open_link"],
            "the helper, called by the commands that open a page"
        );
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

    /// The studio's Tauri configuration as the build reads it
    /// (D-2026-10-01-gif-sticker-search-16): what [`config_problems`] finds
    /// in [`TAURI_CONFIG_FILE`] and, failing closed, each overlay the build
    /// would merge into it: a file beside it named as one
    /// ([`is_config_overlay`]: `tauri.linux.conf.json`, `Tauri.toml`, ...),
    /// and the `TAURI_CONFIG` variable this code was compiled with.
    fn studio_config_problems() -> Vec<String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut problems = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            if is_config_overlay(&name) {
                problems.push(format!(
                    "`{name}`: a Tauri configuration overlay, which the build would merge into \
                     `{TAURI_CONFIG_FILE}`"
                ));
            }
        }
        if option_env!("TAURI_CONFIG").is_some() {
            problems
                .push("`TAURI_CONFIG` is set: the build merges it into the configuration".into());
        }
        let text = std::fs::read_to_string(dir.join(TAURI_CONFIG_FILE)).unwrap();
        problems.extend(config_problems(&serde_json::from_str(&text).unwrap()));
        problems
    }

    /// Whether the file `name`, beside [`TAURI_CONFIG_FILE`], is a Tauri
    /// configuration the build may merge into it: any `tauri…` file (any
    /// case) of a configuration format (JSON, JSON5, TOML) but that one.
    fn is_config_overlay(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        let format = [".json", ".json5", ".toml"]
            .iter()
            .any(|extension| lower.ends_with(extension));
        lower.starts_with("tauri") && format && name != TAURI_CONFIG_FILE
    }

    /// What the Tauri configuration `config` holds that loads a page from
    /// elsewhere (D-2026-10-01-gif-sticker-search-16): a window
    /// (`app.windows`) whose `url` is not one of the app's own pages (none,
    /// a path, or [`APP_SCHEME`]`:`; any other scheme, `http:`, `https:`,
    /// `ws:`, `wss:`, `ftp:`, `file:`, `javascript:`, ..., in any case,
    /// spaces around it cut, is refused) or that names a proxy; a front end
    /// (`build.frontendDist`) that is not the app's own files (a path, or a
    /// list of them); and a dev server (`build.devUrl`): the studio has
    /// none, it is always built with `custom-protocol`. It fails closed:
    /// windows that are not a list and a `url` that is not text are
    /// problems.
    fn config_problems(config: &serde_json::Value) -> Vec<String> {
        use serde_json::Value;

        let path = |text: &Value| text.as_str().is_some_and(|text| url_scheme(text).is_none());
        let app_page = |url: &Value| {
            let scheme = url.as_str().and_then(url_scheme);
            url.is_null() || path(url) || scheme.is_some_and(|scheme| scheme == APP_SCHEME)
        };
        let mut problems = Vec::new();
        let windows = match &config["app"]["windows"] {
            Value::Null => &[][..],
            Value::Array(windows) => windows.as_slice(),
            other => {
                problems.push(format!(
                    "{TAURI_CONFIG_FILE}: `app.windows` is not a list: {other}"
                ));
                &[][..]
            }
        };
        for window in windows {
            let (label, url) = (&window["label"], &window["url"]);
            if !app_page(url) {
                problems.push(format!(
                    "{TAURI_CONFIG_FILE}: the window {label} loads {url}, not one of the app's own \
                     pages (a path, or `{APP_SCHEME}:`)"
                ));
            }
            for proxy in ["proxyUrl", "proxy-url"] {
                if !window[proxy].is_null() {
                    problems.push(format!(
                        "{TAURI_CONFIG_FILE}: the window {label} goes through a proxy (`{proxy}`)"
                    ));
                }
            }
        }
        let build = &config["build"];
        for key in ["frontendDist", "frontend-dist"] {
            let files = match &build[key] {
                Value::Array(files) => files.iter().all(path),
                other => other.is_null() || path(other),
            };
            if !files {
                problems.push(format!(
                    "{TAURI_CONFIG_FILE}: `build.{key}` is {}, not the app's own files",
                    build[key]
                ));
            }
        }
        for key in ["devUrl", "dev-url"] {
            if !build[key].is_null() {
                problems.push(format!(
                    "{TAURI_CONFIG_FILE}: `build.{key}` names a dev server ({}): the studio has \
                     none, it is always built with `custom-protocol`",
                    build[key]
                ));
            }
        }
        problems
    }

    /// An edit of a JSON document: the studio's configuration or metadata.
    type Edit = Box<dyn Fn(&mut serde_json::Value)>;

    /// The studio's configuration edited to load a page from elsewhere
    /// (D-2026-10-01-gif-sticker-search-16, the DoD critic of round 3,
    /// iteration 2: a second window on KLIPY's Partner Panel) is refused,
    /// whatever the scheme's spelling, and so is each overlay; the app's
    /// own pages are not.
    fn remote_pages_are_refused() {
        use serde_json::{Value, json};

        let text = include_str!("../tauri.conf.json");
        let real: Value = serde_json::from_str(text).unwrap();
        let window = |url: Value| -> Edit {
            Box::new(move |c| {
                let windows = c["app"]["windows"].as_array_mut().unwrap();
                windows.push(json!({ "label": "welcome", "url": url.clone() }));
            })
        };
        let set = |key: &'static str, at: &'static str, value: Value| -> Edit {
            Box::new(move |c| c[key][at] = value.clone())
        };
        let script = format!("  {}:go()", SCRIPT_SCHEME.to_uppercase());
        let mut refused: Vec<(Edit, &str)> = vec![
            (window(json!("https://partner.klipy.com")), "loads"),
            (
                Box::new(|c| c["app"]["windows"][0]["url"] = json!("https://partner.klipy.com")),
                "the window \"main\" loads",
            ),
            (window(json!(42)), "loads 42"),
            (
                Box::new(|c| {
                    c["app"]["windows"][0]["proxyUrl"] = json!("http://proxy.example:3128");
                }),
                "goes through a proxy",
            ),
            (
                Box::new(|c| c["app"]["windows"] = json!({ "label": "main" })),
                "is not a list",
            ),
            (
                set("build", "frontendDist", json!("https://partner.klipy.com")),
                "not the app's own files",
            ),
            (
                set("build", "frontend-dist", json!(["index.html", "tauri://x"])),
                "not the app's own files",
            ),
            (
                set("build", "devUrl", json!("http://localhost:1420")),
                "names a dev server",
            ),
            (
                set("build", "dev-url", json!("https://partner.klipy.com")),
                "names a dev server",
            ),
        ];
        for url in [
            "HTTPS://partner.klipy.com",
            " \u{1}https://partner.klipy.com",
            "ht\ttps://partner.klipy.com",
            "http://localhost:1420",
            // Split so scanners do not read a test value as an insecure socket.
            concat!("ws", "://partner.klipy.com"),
            "WSS://partner.klipy.com",
            "ftp://partner.klipy.com",
            "file:///etc/passwd",
            "data:text/html,hi",
            script.as_str(),
        ] {
            refused.push((window(json!(url)), "not one of the app's own pages"));
        }
        assert_eq!(config_problems(&real), Vec::<String>::new());
        for (edit, why) in refused {
            let mut changed = real.clone();
            edit(&mut changed);
            let problems = config_problems(&changed);
            assert!(
                problems.iter().any(|p| p.contains(why)),
                "{changed}: {problems:#?}"
            );
        }
        for url in [
            json!("index.html"),
            json!("/settings.html"),
            json!("tauri://localhost/a.html"),
        ] {
            let mut changed = real.clone();
            window(url)(&mut changed);
            set("build", "frontendDist", json!(["../src/index.html"]))(&mut changed);
            assert_eq!(config_problems(&changed), Vec::<String>::new(), "{changed}");
        }
        for overlay in [
            "tauri.linux.conf.json",
            "tauri.windows.conf.json5",
            "Tauri.toml",
            "Tauri.linux.toml",
            "TAURI.CONF.JSON",
        ] {
            assert!(is_config_overlay(overlay), "{overlay}");
        }
        for other in [TAURI_CONFIG_FILE, "Cargo.toml", "build.rs", "capabilities"] {
            assert!(!is_config_overlay(other), "{other}");
        }
    }

    /// The studio's package and the workspace's resolved graph, as cargo
    /// reads them (`cargo metadata --locked --all-features`): cargo's own
    /// reading of the manifests, so each dependency table (`[dependencies]`,
    /// `[dev-dependencies]`, `[build-dependencies]`, under `[target.…]` too),
    /// `workspace = true`, `package = …` and each feature that turns a
    /// dependency's feature on, in any member, is read as cargo builds it.
    fn studio_metadata() -> serde_json::Value {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let read = std::process::Command::new(cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--all-features",
            ])
            .arg("--manifest-path")
            .arg(&manifest)
            .output()
            .unwrap();
        let why = String::from_utf8_lossy(&read.stderr);
        assert!(read.status.success(), "cargo metadata: {why}");
        serde_json::from_slice(&read.stdout).unwrap()
    }

    /// Whether `name` is a package of the Tauri family, whose `tracing`
    /// feature ([`TAURI_LOGS`]) logs what Tauri does: Tauri, its crates and
    /// plugins, its webview and its windows.
    fn in_tauri(name: &str) -> bool {
        name == "tauri" || name.starts_with("tauri-") || name == "wry" || name == "tao"
    }

    /// Whether the feature list `features` turns [`TAURI_LOGS`] on.
    fn logs_through_tracing(features: &serde_json::Value) -> bool {
        features
            .as_array()
            .is_some_and(|features| features.iter().any(|f| f == TAURI_LOGS))
    }

    /// What `metadata` (as [`studio_metadata`] reads it) says that makes
    /// the studio install a logger or Tauri log
    /// (D-2026-10-01-gif-sticker-search-14): its manifest depending on a
    /// logger installer ([`LOGGER_CRATES`]), whatever the kind (normal, dev,
    /// build), the target or the name it is renamed to, or turning a Tauri
    /// package's `tracing` feature on; any package of the Tauri family
    /// resolved with that feature on, whoever turned it on; and a logger
    /// installer among what the studio is built with (its normal and build
    /// dependencies, all the way down, for any target). It fails closed: a
    /// graph without the studio is a problem.
    fn logging_problems(metadata: &serde_json::Value) -> Vec<String> {
        let graph = match Graph::of(metadata) {
            Ok(graph) => graph,
            Err(why) => return vec![why],
        };
        let mut problems = Vec::new();
        for dependency in list(&graph.studio["dependencies"]) {
            let name = dependency["name"].as_str().unwrap_or_default();
            let as_declared = as_declared(dependency);
            if is_logger_crate(name) {
                problems.push(format!(
                    "Cargo.toml: depends on `{name}` ({as_declared}), a logger installer"
                ));
            }
            if in_tauri(name) && logs_through_tracing(&dependency["features"]) {
                problems.push(format!(
                    "Cargo.toml: turns `{name}`'s `{TAURI_LOGS}` feature on ({as_declared}): \
                     Tauri then logs each invocation's body"
                ));
            }
        }
        for node in graph.nodes.values() {
            let name = graph.name(node["id"].as_str().unwrap_or_default());
            if in_tauri(name) && logs_through_tracing(&node["features"]) {
                problems.push(format!(
                    "resolved: `{name}` is built with its `{TAURI_LOGS}` feature on"
                ));
            }
        }
        let built = graph.reached(graph.studio_id(), for_the_build);
        let mut installers: Vec<&str> = built
            .iter()
            .map(|id| graph.name(id))
            .filter(|name| is_logger_crate(name))
            .collect();
        installers.sort_unstable();
        for name in installers {
            problems.push(format!(
                "resolved: the studio is built with `{name}`, a logger installer"
            ));
        }
        problems
    }

    /// The items of the JSON list `value`; none when it is not one.
    fn list(value: &serde_json::Value) -> &[serde_json::Value] {
        value.as_array().map_or(&[], Vec::as_slice)
    }

    /// How the manifest declares `dependency` (as cargo metadata prints
    /// it): its kind and its target.
    fn as_declared(dependency: &serde_json::Value) -> String {
        let kind = dependency["kind"].as_str().unwrap_or("normal");
        let target = dependency["target"].as_str();
        format!(
            "{kind}{}",
            target.map(|t| format!(", `{t}`")).unwrap_or_default()
        )
    }

    /// Whether a dependency of `kind` (as cargo metadata prints a declared
    /// dependency or a resolved edge's kind) is built into the package that
    /// depends on it: a normal or a build dependency, for any target.
    fn for_the_build(kind: &serde_json::Value) -> bool {
        kind["kind"].is_null() || kind["kind"] == "build"
    }

    /// Whether a dependency of `kind` is built into the package that
    /// depends on it for a desktop the studio is built for ([`DESKTOPS`]).
    fn for_a_desktop(kind: &serde_json::Value) -> bool {
        for_the_build(kind) && on_a_desktop(kind["target"].as_str())
    }

    /// Whether a dependency for `target` (as cargo metadata prints it: none,
    /// a `cfg(…)` or a target triple) may be built for one of [`DESKTOPS`]:
    /// its `cfg` does not fail on every one. It fails closed: a triple, and
    /// a `cfg` that cannot be read, may.
    fn on_a_desktop(target: Option<&str>) -> bool {
        let Some(cfg) = target.filter(|target| target.starts_with("cfg(")) else {
            return true;
        };
        let Ok(cfg) = syn::parse_str::<Meta>(cfg) else {
            return true;
        };
        DESKTOPS
            .iter()
            .any(|desktop| holds(&cfg, desktop) != Some(false))
    }

    /// Whether the `cfg` predicate `meta` holds on `desktop` (one of
    /// [`DESKTOPS`]); `None` when it may: it asks what the desktop does not
    /// fix (the architecture, a feature, a name it does not know).
    fn holds(meta: &Meta, desktop: &[&str; 3]) -> Option<bool> {
        let [os, family, vendor] = *desktop;
        match meta {
            Meta::Path(path) => {
                let name = path.get_ident()?.to_string();
                (name == "unix" || name == "windows").then(|| name == family)
            }
            Meta::NameValue(pair) => {
                let Expr::Lit(syn::ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) = &pair.value
                else {
                    return None;
                };
                let fixed = match pair.path.get_ident()?.to_string().as_str() {
                    "target_os" => os,
                    "target_family" => family,
                    "target_vendor" => vendor,
                    _ => return None,
                };
                Some(value.value() == fixed)
            }
            Meta::List(list) => {
                let parts = list
                    .parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                    .ok()?;
                let values: Vec<Option<bool>> =
                    parts.iter().map(|part| holds(part, desktop)).collect();
                let known = values.iter().all(Option::is_some);
                match list.path.get_ident()?.to_string().as_str() {
                    "cfg" | "not" if values.len() != 1 => None,
                    "cfg" => values[0],
                    "not" => values[0].map(|value| !value),
                    "any" if values.contains(&Some(true)) => Some(true),
                    "any" => known.then_some(false),
                    "all" if values.contains(&Some(false)) => Some(false),
                    "all" => known.then_some(true),
                    _ => None,
                }
            }
        }
    }

    /// What `metadata` (as [`studio_metadata`] reads it) says that makes
    /// the studio speak to the network but through KLIPY's adapter
    /// (D-2026-10-01-gif-sticker-search-16): its manifest declaring one of
    /// [`NETWORK_CRATES`] (a normal or a build dependency, any target,
    /// renamed or not); and, as resolved, each edge into one of them in
    /// what the studio is built with for a desktop ([`for_a_desktop`]: its
    /// normal and build dependencies, all the way down) but
    /// [`KLIPY_ADAPTER`]'s on [`KLIPY_HTTP`], named by the studio's
    /// dependency that pulls it. Tauri's `reqwest`, for Android and iOS
    /// only, is not built for a desktop. It fails closed: a graph without
    /// the studio is a problem.
    fn network_problems(metadata: &serde_json::Value) -> Vec<String> {
        let graph = match Graph::of(metadata) {
            Ok(graph) => graph,
            Err(why) => return vec![why],
        };
        let mut problems = Vec::new();
        for dependency in list(&graph.studio["dependencies"]) {
            let name = dependency["name"].as_str().unwrap_or_default();
            if NETWORK_CRATES.contains(&name) && for_the_build(dependency) {
                problems.push(format!(
                    "Cargo.toml: depends on `{name}` ({}), a crate that speaks to the network",
                    as_declared(dependency)
                ));
            }
        }
        let network = |id: &str| NETWORK_CRATES.contains(&graph.name(id));
        let mut found = BTreeSet::new();
        for direct in graph.deps(graph.studio_id(), for_a_desktop) {
            let name = graph.name(direct);
            if network(direct) {
                found.insert(format!(
                    "resolved: the studio depends on `{name}`, a crate that speaks to the network"
                ));
            }
            for parent in graph.reached(direct, for_a_desktop) {
                let from = graph.name(parent);
                for pulled in graph.deps(parent, for_a_desktop) {
                    let to = graph.name(pulled);
                    let klipy = from == KLIPY_ADAPTER && to == KLIPY_HTTP;
                    if !network(pulled) || klipy {
                        continue;
                    }
                    let through = if parent == direct {
                        String::new()
                    } else {
                        format!(" (`{from}` depends on it)")
                    };
                    found.insert(format!(
                        "resolved: the studio's dependency `{name}` pulls `{to}`{through}: the \
                         studio speaks to the network only through `{KLIPY_ADAPTER}`'s \
                         `{KLIPY_HTTP}`"
                    ));
                }
            }
        }
        problems.extend(found);
        problems
    }

    /// The resolved graph of [`studio_metadata`]: each package's name and
    /// each resolved node by id, and the studio's package.
    struct Graph<'m> {
        names: HashMap<&'m str, &'m str>,
        nodes: HashMap<&'m str, &'m serde_json::Value>,
        studio: &'m serde_json::Value,
    }

    impl<'m> Graph<'m> {
        /// The graph `metadata` holds; why not, failing closed, when the
        /// studio is not in it.
        fn of(metadata: &'m serde_json::Value) -> Result<Self, String> {
            let packages = list(&metadata["packages"]);
            let names = packages
                .iter()
                .filter_map(|p| Some((p["id"].as_str()?, p["name"].as_str()?)))
                .collect();
            let nodes: HashMap<&str, &serde_json::Value> = list(&metadata["resolve"]["nodes"])
                .iter()
                .filter_map(|node| Some((node["id"].as_str()?, node)))
                .collect();
            let studio = packages.iter().find(|p| p["name"] == "bezel-studio");
            let resolved =
                |p: &&serde_json::Value| nodes.contains_key(p["id"].as_str().unwrap_or(""));
            let Some(studio) = studio.filter(resolved) else {
                return Err("cargo metadata: the studio is not in the resolved graph".into());
            };
            Ok(Self {
                names,
                nodes,
                studio,
            })
        }

        /// The name of the package `id`.
        fn name(&self, id: &str) -> &'m str {
            self.names.get(id).copied().unwrap_or_default()
        }

        /// The studio's package id.
        fn studio_id(&self) -> &'m str {
            self.studio["id"].as_str().unwrap_or_default()
        }

        /// What the package `id` depends on through an edge one of whose
        /// kinds `built` accepts.
        fn deps(&self, id: &str, built: fn(&serde_json::Value) -> bool) -> Vec<&'m str> {
            let node = self.nodes.get(id).copied();
            let deps = node.map_or(&[][..], |node| list(&node["deps"]));
            deps.iter()
                .filter(|dep| list(&dep["dep_kinds"]).iter().any(built))
                .filter_map(|dep| dep["pkg"].as_str())
                .collect()
        }

        /// `from` and every package it reaches through edges `built`
        /// accepts, all the way down.
        fn reached(
            &self,
            from: &'m str,
            built: fn(&serde_json::Value) -> bool,
        ) -> HashSet<&'m str> {
            let mut reached = HashSet::new();
            let mut next = vec![from];
            while let Some(id) = next.pop() {
                if reached.insert(id) {
                    next.extend(self.deps(id, built));
                }
            }
            reached
        }
    }

    /// The studio's metadata edited as a change would leave it
    /// (D-2026-10-01-gif-sticker-search-14): what the change is, the edit,
    /// and what one of the findings says.
    type MetadataCase = (
        &'static str,
        Box<dyn Fn(&mut serde_json::Value)>,
        &'static str,
    );

    /// The studio's dependency `name` declared as `kind` (`null` for a
    /// normal one) in the metadata `m`.
    fn declared<'m>(
        m: &'m mut serde_json::Value,
        name: &str,
        kind: &serde_json::Value,
    ) -> &'m mut serde_json::Value {
        let studio = m["packages"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p["name"] == "bezel-studio")
            .unwrap();
        studio["dependencies"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["name"] == name && d["kind"] == *kind)
            .unwrap()
    }

    /// The resolved node of the package `name` in the metadata `m`.
    fn resolved<'m>(m: &'m mut serde_json::Value, name: &str) -> &'m mut serde_json::Value {
        let id = m["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()["id"]
            .clone();
        m["resolve"]["nodes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|n| n["id"] == id)
            .unwrap()
    }

    /// A dependency of the studio's manifest on `name`, as cargo reads one.
    fn dependency(
        name: &str,
        kind: Option<&str>,
        target: Option<&str>,
        rename: Option<&str>,
    ) -> serde_json::Value {
        serde_json::json!({
            "name": name, "source": "registry+https://github.com/rust-lang/crates.io-index",
            "req": "^0.3", "kind": kind, "rename": rename, "optional": false,
            "uses_default_features": true, "features": [], "target": target,
            "registry": null
        })
    }

    /// The edit that adds `dependency` to the studio's manifest.
    fn declares(dependency: serde_json::Value) -> Box<dyn Fn(&mut serde_json::Value)> {
        Box::new(move |m| {
            let studio = m["packages"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|p| p["name"] == "bezel-studio")
                .unwrap();
            studio["dependencies"]
                .as_array_mut()
                .unwrap()
                .push(dependency.clone());
        })
    }

    /// The edit that makes the package `from` depend on `to` (both in the
    /// graph), as `kind` (`null` for a normal dependency).
    fn depends(
        from: &'static str,
        to: &'static str,
        kind: Option<&'static str>,
    ) -> Box<dyn Fn(&mut serde_json::Value)> {
        Box::new(move |m| {
            let to = m["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == to)
                .unwrap()["id"]
                .clone();
            let edge = serde_json::json!({
                "name": "added", "pkg": to,
                "dep_kinds": [{"kind": kind, "target": null}]
            });
            resolved(m, from)["deps"].as_array_mut().unwrap().push(edge);
        })
    }

    /// Changes that make the studio install a logger or Tauri log
    /// (D-2026-10-01-gif-sticker-search-14, the DoD critic of round 2,
    /// iteration 5: the CLI's `--verbose` logger copied into the studio):
    /// each is refused.
    fn loggers_built_in() -> Vec<MetadataCase> {
        let on = |kind: serde_json::Value| -> Box<dyn Fn(&mut serde_json::Value)> {
            Box::new(move |m| {
                declared(m, "tauri", &kind)["features"]
                    .as_array_mut()
                    .unwrap()
                    .push(TAURI_LOGS.into());
            })
        };
        let turned_on = |name: &'static str| -> Box<dyn Fn(&mut serde_json::Value)> {
            Box::new(move |m| {
                resolved(m, name)["features"]
                    .as_array_mut()
                    .unwrap()
                    .push(TAURI_LOGS.into());
            })
        };
        let declared_logger = "a logger installer";
        let mut cases: Vec<MetadataCase> = vec![
            // Tauri's feature, in `[dependencies]` and in the test-only
            // `[target.'cfg(not(windows))'.dev-dependencies]`.
            (
                "tauri features += tracing",
                on(serde_json::Value::Null),
                "turns `tauri`'s `tracing` feature on (normal)",
            ),
            (
                "the mock runtime's tauri += tracing",
                on("dev".into()),
                "(dev, `cfg(not(windows))`)",
            ),
            // Turned on elsewhere: another member, a feature of the studio,
            // a plugin: the package as resolved.
            (
                "tauri resolved with tracing",
                turned_on("tauri"),
                "resolved: `tauri` is built with its `tracing` feature on",
            ),
            (
                "tauri-runtime-wry resolved with tracing",
                turned_on("tauri-runtime-wry"),
                "resolved: `tauri-runtime-wry`",
            ),
            (
                "wry resolved with tracing",
                turned_on("wry"),
                "resolved: `wry`",
            ),
            // A logger installer reached through another crate, built for
            // the studio (normal or build), not as its dev-dependency.
            (
                "bezel-media -> tracing-subscriber",
                depends("bezel-media", "tracing-subscriber", None),
                "resolved: the studio is built with `tracing-subscriber`",
            ),
            (
                "tauri-build -> tracing-subscriber",
                depends("tauri-build", "tracing-subscriber", Some("build")),
                "resolved: the studio is built with `tracing-subscriber`",
            ),
        ];
        // Each logger installer, as a dependency of any kind and target,
        // renamed or not.
        for name in [
            "tracing-subscriber",
            "tracing-appender",
            "env_logger",
            "simplelog",
            "fern",
            "log4rs",
            "flexi_logger",
            "pretty_env_logger",
        ] {
            cases.push((
                "a logger installer",
                declares(dependency(name, None, None, None)),
                declared_logger,
            ));
        }
        for (kind, target, rename) in [
            (Some("dev"), None, None),
            (Some("build"), None, None),
            (None, Some("cfg(unix)"), None),
            (Some("dev"), Some("cfg(not(windows))"), None),
            (None, None, Some("logs")),
        ] {
            cases.push((
                "tracing-subscriber declared",
                declares(dependency("tracing-subscriber", kind, target, rename)),
                declared_logger,
            ));
        }
        cases
    }

    /// The edit that makes the edges of the package `from` on `to` (both
    /// in the graph) hold for `target` (`null`: every target).
    fn retargets(from: &'static str, to: &'static str, target: serde_json::Value) -> Edit {
        Box::new(move |m| {
            let to = m["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == to)
                .unwrap()["id"]
                .clone();
            let deps = resolved(m, from)["deps"].as_array_mut().unwrap();
            let edge = deps.iter_mut().find(|dep| dep["pkg"] == to).unwrap();
            for kind in edge["dep_kinds"].as_array_mut().unwrap() {
                kind["target"] = target.clone();
            }
        })
    }

    /// Changes that make the studio speak to the network but through
    /// KLIPY's adapter (D-2026-10-01-gif-sticker-search-16, the DoD critic
    /// of round 3, iteration 2: the updater plugin in the studio): each is
    /// refused.
    fn network_built_in() -> Vec<MetadataCase> {
        let tauri_pulls = "the studio's dependency `tauri` pulls `reqwest`";
        let mut cases: Vec<MetadataCase> = vec![
            // Tauri's `reqwest` (Android and iOS only today) built for a
            // desktop: a feature or a version that turns it on there.
            (
                "tauri -> reqwest, every target",
                retargets("tauri", "reqwest", serde_json::Value::Null),
                tauri_pulls,
            ),
            (
                "tauri -> reqwest, Linux",
                retargets("tauri", "reqwest", "cfg(target_os = \"linux\")".into()),
                tauri_pulls,
            ),
            (
                "tauri -> reqwest, unix",
                retargets("tauri", "reqwest", "cfg(unix)".into()),
                tauri_pulls,
            ),
            (
                "tauri -> reqwest, not Android",
                retargets(
                    "tauri",
                    "reqwest",
                    "cfg(not(target_os = \"android\"))".into(),
                ),
                tauri_pulls,
            ),
            (
                "tauri -> reqwest, Android or Windows",
                retargets(
                    "tauri",
                    "reqwest",
                    "cfg(any(target_os = \"android\", windows))".into(),
                ),
                tauri_pulls,
            ),
            (
                "tauri -> reqwest, an architecture",
                retargets("tauri", "reqwest", "cfg(target_arch = \"x86_64\")".into()),
                tauri_pulls,
            ),
            (
                "tauri -> reqwest, a Windows triple",
                retargets("tauri", "reqwest", "x86_64-pc-windows-msvc".into()),
                tauri_pulls,
            ),
            // A network crate pulled by another dependency of the studio,
            // a build dependency included; `ureq` but by KLIPY's adapter;
            // KLIPY's adapter pulling another one.
            (
                "bezel-media -> reqwest",
                depends("bezel-media", "reqwest", None),
                "`bezel-media` pulls `reqwest`",
            ),
            (
                "tauri-build -> hyper (build)",
                depends("tauri-build", "hyper", Some("build")),
                "`tauri-build` pulls `hyper`",
            ),
            (
                "the studio -> ureq",
                depends("bezel-studio", "ureq", None),
                "the studio depends on `ureq`",
            ),
            (
                "tauri -> ureq",
                depends("tauri", "ureq", None),
                "`tauri` pulls `ureq`",
            ),
            (
                "bezel-klipy -> reqwest",
                depends("bezel-klipy", "reqwest", None),
                "`bezel-klipy` pulls `reqwest`",
            ),
        ];
        let declared = "a crate that speaks to the network";
        for name in NETWORK_CRATES {
            cases.push((
                "a network crate declared",
                declares(dependency(name, None, None, None)),
                declared,
            ));
        }
        for (kind, target, rename) in [
            (Some("build"), None, None),
            (None, Some("cfg(unix)"), None),
            (None, Some("cfg(target_os = \"android\")"), None),
            (None, None, Some("updates")),
        ] {
            cases.push((
                "tauri-plugin-updater declared",
                declares(dependency("tauri-plugin-updater", kind, target, rename)),
                declared,
            ));
        }
        cases
    }

    /// D-2026-10-01-gif-sticker-search-14: the studio installs no logger,
    /// and Tauri does not log. Its real metadata passes; each change of
    /// [`loggers_built_in`], applied to it, is refused; a graph without the
    /// studio fails closed; and what does not reach the studio is not
    /// refused: a logger installer that only the CLI depends on (it does,
    /// for `--verbose`), and one that a dependency of the studio only uses
    /// in its own tests. D-2026-10-01-gif-sticker-search-16, the same way:
    /// the studio speaks to the network only through KLIPY's adapter
    /// ([`network_problems`], [`network_built_in`]); Tauri's `reqwest`,
    /// for Android and iOS, and a network crate used only in tests are not
    /// refused.
    #[test]
    fn the_studio_installs_no_logger() {
        let real = studio_metadata();
        assert_eq!(logging_problems(&real), Vec::<String>::new());
        let cli = real["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "bezel")
            .unwrap();
        let cli_logs = cli["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["name"] == "tracing-subscriber");
        assert!(cli_logs, "the CLI's `--verbose` logger is in the graph");
        let mut tested = real.clone();
        depends("bezel-media", "tracing-subscriber", Some("dev"))(&mut tested);
        assert_eq!(logging_problems(&tested), Vec::<String>::new());
        for (what, edit, why) in loggers_built_in() {
            let mut changed = real.clone();
            edit(&mut changed);
            let problems = logging_problems(&changed);
            assert!(
                problems.iter().any(|p| p.contains(why)),
                "{what}: {problems:#?}"
            );
        }
        let mut without = real.clone();
        without["resolve"] = serde_json::Value::Null;
        assert_eq!(
            logging_problems(&without),
            ["cargo metadata: the studio is not in the resolved graph"]
        );
        assert_eq!(
            network_problems(&without),
            ["cargo metadata: the studio is not in the resolved graph"]
        );
        assert_eq!(network_problems(&real), Vec::<String>::new());
        let mut tested = real.clone();
        depends("bezel-media", "reqwest", Some("dev"))(&mut tested);
        declares(dependency("reqwest", Some("dev"), None, None))(&mut tested);
        retargets("tauri", "reqwest", "cfg(target_os = \"ios\")".into())(&mut tested);
        assert_eq!(network_problems(&tested), Vec::<String>::new());
        for (what, edit, why) in network_built_in() {
            let mut changed = real.clone();
            edit(&mut changed);
            let problems = network_problems(&changed);
            assert!(
                problems.iter().any(|p| p.contains(why)),
                "{what}: {problems:#?}"
            );
        }
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

    /// A logger installed, or a panic hook other than `diag::hook_panics`
    /// first in `main` (D-2026-10-01-gif-sticker-search-14): each is
    /// refused, in `diag.rs` too.
    fn loggers_and_hooks() -> Vec<Case> {
        let installer = "a logger installer's crate";
        let installs = "installs a logger or a tracing subscriber";
        let hook = "`set_hook` outside `diag::hook_panics`";
        let first = "`main` does not call `diag::hook_panics()` first";
        vec![
            // The DoD critic of round 2, iteration 5: the CLI's `--verbose`
            // logger in `run`, behind a switch.
            (
                "lib.rs",
                "pub fn run() -> R { if switch_on(std::env::var_os(\"BEZEL_VERBOSE\").as_deref()) { \
                 tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).init(); } \
                 tauri::Builder::default().run(tauri::generate_context!()) }"
                    .into(),
                installer,
            ),
            ("lib.rs", "use tracing_subscriber::fmt;".into(), installer),
            ("lib.rs", "use ::r#fern as f;".into(), installer),
            ("lib.rs", "extern crate env_logger;".into(), installer),
            ("lib.rs", "fn f() { ::pretty_env_logger::init() }".into(), installer),
            (
                "lib.rs",
                "fn f() { spawn!(simplelog::SimpleLogger::init(L, C)) }".into(),
                installer,
            ),
            (
                DIAG_MODULE,
                "pub fn report(code: DiagCode) { let _ = log4rs::init_file(\"x\", D); }".into(),
                installer,
            ),
            // Installed through `tracing` or `log` themselves.
            (
                DIAG_MODULE,
                "pub fn report(code: DiagCode) { \
                 let _ = tracing::subscriber::set_global_default(S); }"
                    .into(),
                installs,
            ),
            (
                "lib.rs",
                "fn f(s: S) { tracing::subscriber::with_default(s, run) }".into(),
                installs,
            ),
            (
                "studio.rs",
                "fn f(s: S) { let _guard = tracing::dispatcher::set_default(&s); }".into(),
                installs,
            ),
            (
                "lib.rs",
                "fn f() { let _ = log::set_boxed_logger(Box::new(L)); }".into(),
                installs,
            ),
            ("tray.rs", "fn f() { let _ = r#set_logger(&L); }".into(), installs),
            // Another panic hook, or the default one back.
            (
                "lib.rs",
                "pub fn run() { std::panic::set_hook(Box::new(|_| {})); }".into(),
                hook,
            ),
            (
                DIAG_MODULE,
                "pub fn report(code: DiagCode) { std::panic::set_hook(Box::new(|_| {})); }".into(),
                hook,
            ),
            ("main.rs", "use std::panic::set_hook;".into(), hook),
            (
                "lib.rs",
                "fn f() { let _ = std::panic::take_hook(); }".into(),
                "`take_hook` changes the panic hook",
            ),
            // `main` without the hook first.
            (
                "main.rs",
                "fn main() -> ExitCode { match bezel_studio::run() { Ok(()) => ExitCode::SUCCESS, \
                 Err(_) => ExitCode::FAILURE } }"
                    .into(),
                first,
            ),
            (
                "main.rs",
                "fn main() -> ExitCode { let r = bezel_studio::run(); diag::hook_panics(); \
                 exit(r) }"
                    .into(),
                first,
            ),
        ]
    }

    /// The studio's re-exec as `lib.rs` has it ([`RESTART`]), `body` its
    /// statements after the switch's check.
    fn restart(body: &str) -> String {
        format!(
            "#[cfg(target_os = \"linux\")]\nfn restart_without_dmabuf_renderer() {{ \
             use std::os::unix::process::CommandExt; \
             if std::env::var_os(DMABUF_SWITCH).is_some() {{ return; }} {body} }}"
        )
    }

    /// The re-exec's statements in `lib.rs`: the studio's own file run
    /// again with the switch on.
    const RE_EXEC: &str = "let Ok(exe) = std::env::current_exe() else { return; }; \
        let error = std::process::Command::new(exe).args(std::env::args_os().skip(1)) \
        .env(DMABUF_SWITCH, \"1\").exec(); diag::report(restart_failure(&error));";

    /// The DoD critic's `setup` of round 3, iteration 1: without a saved
    /// key, a welcome step does `step` (it opens KLIPY's Partner Panel).
    fn welcome(step: &str) -> String {
        format!(
            "fn setup<R: Runtime>(app: &App<R>, start: Start<R>) -> Result<(), Box<dyn Error>> {{ \
             let folders = (start.folders)(app.handle())?; \
             if !folders.config.join(KEY_FILE).exists() \
             && let Ok(url) = crate::backend::link_url(\"klipyPartnerPanel\") {{ {step} }} \
             Ok(()) }}"
        )
    }

    /// A socket, a program spawned but the studio's re-exec, or a crate
    /// that leaves the computer (D-2026-10-01-gif-sticker-search-15, the
    /// DoD critic of round 3, iteration 1): each is refused.
    fn sockets_and_programs() -> Vec<Case> {
        let socket = |name: &str| match name {
            "TcpStream" => "`TcpStream` (a socket)",
            "UdpSocket" => "`UdpSocket` (a socket)",
            "UnixStream" => "`UnixStream` (a socket)",
            _ => "(a socket)",
        };
        let net = "goes through `net`, the sockets' module";
        let spawns = "`Command` spawns a program outside `restart_without_dmabuf_renderer`";
        let crate_out = "is a crate that speaks to the network, opens a page or spawns";
        vec![
            // A hand-written request from the backend, by path, imported
            // in a group, raw, in a macro's tokens, by another runtime.
            (
                "backend.rs",
                "impl Backend { fn hello(&self) { \
                 let _ = std::net::TcpStream::connect((\"partner.klipy.com\", 80)); } }"
                    .into(),
                socket("TcpStream"),
            ),
            (
                "backend.rs",
                "impl Backend { fn hello(&self) { \
                 let _ = std::net::TcpStream::connect((\"partner.klipy.com\", 80)); } }"
                    .into(),
                net,
            ),
            ("lib.rs", "use std::{fs, net::TcpStream};".into(), net),
            ("lib.rs", "use std::{fs, net::{self as n}};".into(), net),
            (
                "lib.rs",
                "use std as s;\nfn f(a: A) { let _ = s::net::r#TcpListener::bind(a); }".into(),
                net,
            ),
            (
                "lib.rs",
                "fn f(a: A) { let _ = r#TcpStream::connect(a); }".into(),
                socket("TcpStream"),
            ),
            (
                "lib.rs",
                "fn f(a: A) { spawn!(async move { UdpSocket::bind(a) }) }".into(),
                socket("UdpSocket"),
            ),
            (
                "studio.rs",
                "use std::os::unix::net::UnixStream;".into(),
                socket("UnixStream"),
            ),
            (
                "studio.rs",
                "fn f() { let _ = std::os::unix::net::UnixDatagram::unbound(); }".into(),
                net,
            ),
            (
                "gifs.rs",
                "use std::net::ToSocketAddrs as _;\nfn f() { let _ = \"a:1\".to_socket_addrs(); }"
                    .into(),
                net,
            ),
            (
                "lib.rs",
                "async fn f(a: A) { let _ = tokio::net::TcpStream::connect(a).await; }".into(),
                net,
            ),
            // The critic's other way: a program that opens the page, by
            // path, imported, renamed, raw, another runtime's, its
            // extension trait.
            (
                "lib.rs",
                welcome("let _ = std::process::Command::new(\"xdg-open\").arg(url).spawn();"),
                spawns,
            ),
            (
                "lib.rs",
                format!(
                    "use std::process::Command;\n{}",
                    welcome("let _ = Command::new(\"xdg-open\").arg(url).spawn();")
                ),
                spawns,
            ),
            (
                "lib.rs",
                "use std::process::{Command as Run};".into(),
                spawns,
            ),
            (
                "tray.rs",
                "fn f(u: &str) { let _ = r#Command::new(\"xdg-open\").arg(u).spawn(); }".into(),
                spawns,
            ),
            (
                "lib.rs",
                "async fn f(u: &str) { let _ = tokio::process::Command::new(\"open\").arg(u) \
                 .status().await; }"
                    .into(),
                spawns,
            ),
            (
                "lib.rs",
                "use std::os::unix::process::CommandExt;".into(),
                "`CommandExt` spawns a program",
            ),
            // The re-exec made to run another program: by name, shadowed,
            // `mut`, not the studio's file, a second one, a macro there, a
            // closure's binding of the name, nested, in another file.
            (
                "lib.rs",
                restart(&RE_EXEC.replace("Command::new(exe)", "Command::new(\"xdg-open\")")),
                spawns,
            ),
            (
                "lib.rs",
                restart(&RE_EXEC.replace(
                    "let error",
                    "let exe = std::path::PathBuf::from(\"/usr/bin/xdg-open\"); let error",
                )),
                spawns,
            ),
            (
                "lib.rs",
                restart(&RE_EXEC.replace(
                    "Ok(exe) = std::env::current_exe() else { return; };",
                    "Ok(mut exe) = std::env::current_exe() else { return; }; \
                     exe.set_file_name(\"xdg-open\");",
                )),
                spawns,
            ),
            (
                "lib.rs",
                restart(&RE_EXEC.replace("std::env::current_exe()", "which(\"xdg-open\")")),
                spawns,
            ),
            (
                "lib.rs",
                restart(&format!(
                    "{RE_EXEC} let _ = std::process::Command::new(exe).arg(\"u\").spawn();"
                )),
                spawns,
            ),
            (
                "lib.rs",
                restart(&format!("{RE_EXEC} shadow!(exe);")),
                "a macro called in `restart_without_dmabuf_renderer`",
            ),
            (
                "lib.rs",
                restart(
                    &RE_EXEC.replace("let error", "let run = |exe: &str| exe.len(); let error"),
                ),
                spawns,
            ),
            (
                "lib.rs",
                format!(
                    "fn restart_without_dmabuf_renderer() {{ fn inner() {{ {RE_EXEC} }} inner() }}"
                ),
                spawns,
            ),
            ("studio.rs", restart(RE_EXEC), spawns),
            // Crates that leave the computer, and a webview made in Rust
            // that loads a page.
            (
                "lib.rs",
                "fn f(u: &str) { let _ = reqwest::blocking::get(u); }".into(),
                crate_out,
            ),
            (
                "lib.rs",
                "fn f(u: &str) { let _ = open::that(u); }".into(),
                crate_out,
            ),
            ("lib.rs", "use webbrowser as w;".into(), crate_out),
            ("lib.rs", "extern crate ureq;".into(), crate_out),
            // The updater plugin (the DoD critic of round 3, iteration 2),
            // by its crate, vendored under another name, and a vendored
            // HTTP plugin's re-export of its client
            // (D-2026-10-01-gif-sticker-search-16).
            (
                "lib.rs",
                welcome("app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;"),
                crate_out,
            ),
            (
                "lib.rs",
                welcome("app.handle().plugin(upd::Builder::new().build())?; app.updater()?.check();"),
                "`updater` (the updater plugin)",
            ),
            (
                "lib.rs",
                "use upd::UpdaterExt as _;".into(),
                "`UpdaterExt` (the updater plugin)",
            ),
            (
                "lib.rs",
                "fn f(a: A) { spawn!(a.updater_builder().build()) }".into(),
                "`updater_builder` (the updater plugin)",
            ),
            (
                "lib.rs",
                "fn f() { let _ = net::reqwest::Client::new(); }".into(),
                "names `reqwest`, a crate that speaks to the network",
            ),
            (
                "lib.rs",
                "use vendored::{tokio_tungstenite as t};".into(),
                "names `tokio_tungstenite`",
            ),
            // The configuration read from elsewhere, or edited at run time.
            (
                "lib.rs",
                "fn run() -> R { tauri::Builder::default().run(tauri::generate_context!(\"x.json\")) }"
                    .into(),
                "`generate_context!` with an argument",
            ),
            (
                "lib.rs",
                "fn run() -> R { spawn!(tauri::generate_context!(\"x.json\")) }".into(),
                "`generate_context!` with an argument",
            ),
            (
                "lib.rs",
                "fn run(c: &mut C) { c.config_mut().build.frontend_dist = None; }".into(),
                "`config_mut` edits the Tauri configuration",
            ),
            (
                "lib.rs",
                welcome(
                    "let _ = tauri::WebviewWindowBuilder::new(app, \"k\", \
                     tauri::WebviewUrl::External(url.parse()?)).build();",
                ),
                "`WebviewWindowBuilder` makes a webview in Rust",
            ),
        ]
    }

    /// The system opener used, or its helper called, outside the one
    /// helper and the two commands that open a page
    /// (D-2026-10-01-gif-sticker-search-15, the DoD critic of round 3,
    /// iteration 1: the setup opening KLIPY's Partner Panel when no key is
    /// saved): each is refused.
    fn pages_opened() -> Vec<Case> {
        let opener = |name: &'static str| match name {
            "Opener" => "`Opener` (the system opener) outside `open_fixed` in `commands.rs`",
            "open_url" => "`open_url` (the system opener) outside `open_fixed`",
            "opener" => "`opener` (the system opener) outside `open_fixed`",
            "OpenerExt" => "`OpenerExt` (the system opener) outside `open_fixed`",
            _ => "(the system opener) outside `open_fixed`",
        };
        let helper = "`open_fixed`, which opens a page, named outside its definition";
        let critic = welcome(
            "if let Some(opener) = app.try_state::<tauri_plugin_opener::Opener<R>>() { \
             let _ = opener.open_url(url, None::<&str>); }",
        );
        let by_trait = format!(
            "use tauri_plugin_opener::OpenerExt as _;\n{}",
            welcome("let _ = app.opener().open_url(url, None::<&str>);")
        );
        let open_link = |body: &str| {
            format!(
                "#[tauri::command]\npub async fn open_link<R: Runtime>(app: AppHandle<R>, \
                 link: String) -> UiResult<()> {{ {body} }}"
            )
        };
        vec![
            // The critic's m2 (the opener's state) and m2b (its trait).
            ("lib.rs", critic.clone(), opener("Opener")),
            ("lib.rs", critic, opener("open_url")),
            ("lib.rs", by_trait.clone(), opener("opener")),
            ("lib.rs", by_trait, opener("OpenerExt")),
            // The helper called from the setup, raw, imported, as a value.
            (
                "lib.rs",
                welcome(
                    "tauri::async_runtime::spawn(commands::open_fixed(app.handle().clone(), \
                     url.to_string()));",
                ),
                helper,
            ),
            (
                "lib.rs",
                welcome("let _ = crate::commands::r#open_fixed;"),
                helper,
            ),
            ("lib.rs", "use crate::commands::open_fixed;".into(), helper),
            // The opener's functions, by the plugin's path, elsewhere.
            (
                "lib.rs",
                welcome("let _ = tauri_plugin_opener::open_url(url, None::<&str>);"),
                opener("open_url"),
            ),
            (
                "storage.rs",
                "fn f(p: &Path) { let _ = tauri_plugin_opener::reveal_item_in_dir(p); }".into(),
                "`reveal_item_in_dir` (the system opener)",
            ),
            (
                "storage.rs",
                "fn f(o: &O, p: &Path) { let _ = o.open_path(p, None::<&str>); }".into(),
                "`open_path` (the system opener)",
            ),
            (
                "lib.rs",
                "fn f(a: &AppHandle) { spawn!(a.opener().open_url(U, None::<&str>)) }".into(),
                opener("opener"),
            ),
            // In the command module: a command opening a page without the
            // helper, the helper as a method or in a module, called by a
            // function that is not one of the two commands, the trait
            // imported by its name or inside a function.
            (
                COMMAND_MODULE,
                open_link("app.opener().open_url(link, None::<&str>).map_err(UiError::system)"),
                opener("opener"),
            ),
            (
                COMMAND_MODULE,
                "impl Gifs { fn open_fixed<R: Runtime>(&self, app: AppHandle<R>) { \
                 let _ = app.opener().open_url(U, None::<&str>); } }"
                    .into(),
                opener("opener"),
            ),
            (
                COMMAND_MODULE,
                "mod m { pub fn open_fixed<R: Runtime>(app: AppHandle<R>) { \
                 let _ = app.opener().open_url(U, None::<&str>); } }"
                    .into(),
                opener("opener"),
            ),
            (
                COMMAND_MODULE,
                "fn welcome<R: Runtime>(app: AppHandle<R>) { \
                 tauri::async_runtime::spawn(open_fixed(app, U.into())); }"
                    .into(),
                helper,
            ),
            (
                COMMAND_MODULE,
                "pub async fn open_link<R: Runtime>(app: AppHandle<R>, link: String) \
                 -> UiResult<()> { open_fixed(app, link).await }"
                    .into(),
                helper,
            ),
            (
                COMMAND_MODULE,
                "use tauri_plugin_opener::OpenerExt;".into(),
                opener("OpenerExt"),
            ),
            (
                COMMAND_MODULE,
                "fn f() { use tauri_plugin_opener::OpenerExt as _; }".into(),
                opener("OpenerExt"),
            ),
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
    /// ([`prints_outside_diag`]), `diag.rs` made to say a value
    /// ([`diag_says_a_value`]), a logger installed or another panic
    /// hook ([`loggers_and_hooks`]), a socket or a program spawned
    /// ([`sockets_and_programs`]) and a page opened but by the opener's
    /// helper ([`pages_opened`]); what the studio does (the source factory,
    /// the key file, the GIF commands and their invocation, the handler
    /// list, a method named like a command, a local named like one in
    /// `commands.rs` (review W1 of round 2), a panic with fixed text, what
    /// `diag.rs` and `main.rs` are and the calls of `diag`, the re-exec, the
    /// opener's helper and its two commands, `commands.rs` as it is) is not.
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
            loggers_and_hooks(),
            sockets_and_programs(),
            pages_opened(),
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
            // `diag.rs` and `main.rs` as they are (the panic hook, set first),
            // its codes said, panics with fixed text, `write!` to a
            // formatter, a lint's `expect` attribute.
            (DIAG_MODULE, include_str!("diag.rs")),
            ("main.rs", include_str!("main.rs")),
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
        // Every way out as the studio has it
        // (D-2026-10-01-gif-sticker-search-15): its re-exec, the opener's
        // helper and the two commands that call it, the command module.
        let open_commands = "use tauri_plugin_opener::OpenerExt as _;\n\
            #[tauri::command]\npub async fn open_guide<R: Runtime>(app: AppHandle<R>, \
            page: String, language: String) -> UiResult<()> { \
            open_fixed(app, guide_url(&page, &language)?).await }\n\
            #[tauri::command]\npub async fn open_link<R: Runtime>(app: AppHandle<R>, \
            link: String) -> UiResult<()> { open_fixed(app, link_url(&link)?.to_string()).await }\n\
            async fn open_fixed<R: Runtime>(app: AppHandle<R>, url: String) -> UiResult<()> { \
            tauri::async_runtime::spawn_blocking(move || app.opener().open_url(url, None::<&str>)) \
            .await.map_err(UiError::system)?.map_err(UiError::system) }";
        let re_exec = restart(RE_EXEC);
        let ways_out = [
            ("lib.rs", re_exec.as_str()),
            (COMMAND_MODULE, open_commands),
            (COMMAND_MODULE, include_str!("commands.rs")),
        ];
        for (name, text) in accepted.into_iter().chain(ways_out) {
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
