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

/// Starts the app and blocks until it exits.
///
/// # Errors
///
/// Tauri's error when the app cannot start.
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
                    Err(e) => tracing::warn!("unsaved edits not asked about: {e}"),
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
/// compile. Forging an invocation (Tauri's IPC entry or a script run in the
/// window) or making a second KLIPY client is what the type cannot stop:
/// the source guard `tests::nothing_in_the_app_forges_an_invocation`
/// refuses both in production code.
fn setup<R: Runtime>(app: &App<R>, start: Start<R>) -> Result<(), Box<dyn std::error::Error>> {
    let folders = (start.folders)(app.handle())?;
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
    let live_item = (start.tray)(app.handle(), live, &backend.texts())?;
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
        eprintln!("bezel-studio: {SIMULATION_SWITCH}=1, simulated Turing 8.8\" and sensors");
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
        Err(e) => {
            tracing::error!("local copies are kept for this run only: {e}");
            Copies::in_memory(MemoryArchive::new())
        }
    }
}

/// KLIPY's client for a saved key (D-2026-10-01-gif-sticker-search-2),
/// made only for a user action ([`UserAsked`]): making one asks nothing.
/// The one reader of the key's text outside the key file
/// (D-2026-10-01-gif-sticker-search-10).
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
            if let Err(e) = backend.studio().refresh_catalog() {
                tracing::warn!("sensor catalog: {e}");
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
    if let Err(e) = spawned {
        eprintln!("bezel-studio: refresh loop not started: {e}");
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
    let error = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(DMABUF_SWITCH, "1")
        .exec();
    eprintln!("bezel-studio: could not restart with the DMA-BUF renderer off: {error}");
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
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

    /// What the studio's production code must not hold, matched with its
    /// comments set aside and its whitespace removed: each lets the app
    /// forge a user action (D-2026-10-01-gif-sticker-search-3).
    const FORGERIES: [&str; 9] = [
        // An invocation handed to a webview as if the window sent it, the
        // invoke key it must carry, and the invocation itself.
        "on_message",
        "invoke_key",
        "InvokeRequest",
        // A script run in the window (it can invoke a command, or press
        // Search): as a method, as a path or a function taken, with a
        // callback, naming the window's IPC, or injected at load.
        ".eval(",
        "::eval",
        "eval_with_callback",
        "__TAURI",
        "initialization_script",
        "js_init_script",
    ];

    /// What makes production code impossible to classify: a module read
    /// from another path, or code included from another file.
    const UNCLASSIFIABLE: [&str; 2] = ["#[path", "include!("];

    /// A comment or a literal of Rust source.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Span {
        Comment,
        Literal,
    }

    /// A Rust file of the studio, as the source guard reads it.
    struct Source {
        /// Its path under `src/`, `/`-separated.
        name: String,
        /// The whole file.
        text: String,
        /// `text` with its comments and literals blanked: only code.
        skeleton: String,
        /// `text` with its comments blanked: code and literals.
        code: String,
        /// Its inline test modules, as offsets in all three.
        tests: Vec<Range<usize>>,
    }

    impl Source {
        /// Reads `text` as the file `name`; why it cannot be classified
        /// when one of its comments, literals or test modules never closes.
        fn new(name: &str, text: &str) -> Result<Self, String> {
            let spans =
                spans(text).ok_or_else(|| format!("{name}: a comment or literal never closes"))?;
            let skeleton = blank(text, &spans, &[Span::Comment, Span::Literal]);
            let tests = test_modules(&skeleton)
                .ok_or_else(|| format!("{name}: a test module never closes"))?;
            Ok(Self {
                name: name.into(),
                text: text.into(),
                code: blank(text, &spans, &[Span::Comment]),
                skeleton,
                tests,
            })
        }

        /// Its code without the inline test modules (each leaves a line
        /// break, so no name is made across one).
        fn production(&self) -> String {
            let mut kept = String::new();
            let mut from = 0;
            for module in &self.tests {
                kept.push_str(&self.code[from..module.start]);
                kept.push('\n');
                from = module.end;
            }
            kept.push_str(&self.code[from..]);
            kept
        }

        /// The code of its production function `name`, whitespace removed:
        /// empty when it has none.
        fn function(&self, name: &str) -> String {
            let header = format!("fn {name}(");
            let start = self
                .skeleton
                .match_indices(&header)
                .map(|(at, _)| at)
                .find(|at| !self.tests.iter().any(|module| module.contains(at)));
            let body = start.and_then(|at| {
                let open = at + self.skeleton[at..].find('{')?;
                Some(open..block_end(&self.skeleton, open)?)
            });
            body.map(|body| squeezed(&self.code[body]))
                .unwrap_or_default()
        }
    }

    /// `text` without whitespace: a name split by spaces or lines is whole.
    fn squeezed(text: &str) -> String {
        text.split_whitespace().collect()
    }

    /// The length of the block comment `rest` starts with, nested ones
    /// included: `None` when it never closes.
    fn block_comment_len(rest: &str) -> Option<usize> {
        let mut depth = 0_usize;
        let mut at = 0;
        while at < rest.len() {
            if rest[at..].starts_with("/*") {
                depth += 1;
                at += 2;
            } else if rest[at..].starts_with("*/") {
                depth = depth.checked_sub(1)?;
                at += 2;
                if depth == 0 {
                    return Some(at);
                }
            } else {
                at += rest[at..].chars().next()?.len_utf8();
            }
        }
        None
    }

    /// The length of the string `rest` starts with (at its quote), escapes
    /// included: `None` when it never closes.
    fn string_len(rest: &str) -> Option<usize> {
        let mut escaped = false;
        for (at, c) in rest.char_indices().skip(1) {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => return Some(at + 1),
                _ => {}
            }
        }
        None
    }

    /// The length of the char literal `rest` starts with (at its quote):
    /// `None` when the quote starts a lifetime or a label instead.
    fn char_len(rest: &str) -> Option<usize> {
        let mut chars = rest.char_indices().skip(1);
        let (_, first) = chars.next()?;
        let (at, next) = chars.next()?;
        if first == '\\' {
            // An escape: through the quote after the escaped character.
            let after = at + next.len_utf8();
            return rest[after..].find('\'').map(|end| after + end + 1);
        }
        (next == '\'').then_some(at + 1)
    }

    /// The length of the raw string `rest` starts with, its prefix being
    /// the `word` (`r`, `br` or `cr`) then `#`s and a quote: `None` when
    /// none starts there (a word, or a raw identifier), `Some(None)` when
    /// it never closes.
    fn raw_string_len(rest: &str, word: usize) -> Option<Option<usize>> {
        if !matches!(&rest[..word], "r" | "br" | "cr") {
            return None;
        }
        let hashes = rest[word..].len() - rest[word..].trim_start_matches('#').len();
        let quote = word + hashes;
        if !rest[quote..].starts_with('"') {
            return None;
        }
        let close = format!("\"{}", "#".repeat(hashes));
        Some(
            rest[quote + 1..]
                .find(&close)
                .map(|end| quote + 1 + end + close.len()),
        )
    }

    /// The comments and literals of Rust `code`, in order: `None` when one
    /// of them never closes.
    fn spans(code: &str) -> Option<Vec<(Range<usize>, Span)>> {
        let mut found = Vec::new();
        let mut at = 0;
        while let Some(c) = code[at..].chars().next() {
            let rest = &code[at..];
            let word = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let span = if rest.starts_with("//") {
                Some((rest.find('\n').unwrap_or(rest.len()), Span::Comment))
            } else if rest.starts_with("/*") {
                Some((block_comment_len(rest)?, Span::Comment))
            } else if c == '"' {
                Some((string_len(rest)?, Span::Literal))
            } else if c == '\'' {
                char_len(rest).map(|len| (len, Span::Literal))
            } else if word > 0 {
                let Some(len) = raw_string_len(rest, word) else {
                    // A whole word: never a raw string's `r` inside one.
                    at += word;
                    continue;
                };
                Some((len?, Span::Literal))
            } else {
                None
            };
            let len = span.map_or(c.len_utf8(), |(len, kind)| {
                found.push((at..at + len, kind));
                len
            });
            at += len;
        }
        Some(found)
    }

    /// `code` with the bytes of its `spans` of the `blanked` kinds turned
    /// into spaces (line breaks kept): offsets and lines stay where they
    /// were.
    fn blank(code: &str, spans: &[(Range<usize>, Span)], blanked: &[Span]) -> String {
        let mut bytes = code.as_bytes().to_vec();
        for (range, _) in spans.iter().filter(|(_, kind)| blanked.contains(kind)) {
            for byte in &mut bytes[range.clone()] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    /// The end (past its brace) of the block opened at `open` in
    /// `skeleton`: `None` when it never closes.
    fn block_end(skeleton: &str, open: usize) -> Option<usize> {
        let mut depth = 0_usize;
        for (at, byte) in skeleton.bytes().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(at + 1);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The lines of `text` that hold something, each with its offset.
    fn filled_lines(text: &str) -> Vec<(usize, &str)> {
        let mut at = 0;
        text.split_inclusive('\n')
            .map(|line| {
                let start = at;
                at += line.len();
                (start, line)
            })
            .filter(|(_, line)| !line.trim().is_empty())
            .collect()
    }

    /// The module `line` declares (`[pub[(…)] ]mod NAME`) and what follows
    /// its name (`;`, or `{` and the rest of the line), if it declares one.
    fn module_item(line: &str) -> Option<(&str, &str)> {
        let (visibility, item) = line.trim().split_once("mod ")?;
        let visible = visibility.is_empty()
            || visibility == "pub "
            || (visibility.starts_with("pub(")
                && visibility.ends_with(") ")
                && !visibility.contains(['{', ';']));
        let name = item
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(item.len());
        (visible && name > 0).then(|| (&item[..name], item[name..].trim()))
    }

    /// The inline test modules of `skeleton`: from a `#[cfg(test)]` line
    /// whose next line opens a `mod` block, through the brace that closes
    /// it. `None` when that brace is missing.
    fn test_modules(skeleton: &str) -> Option<Vec<Range<usize>>> {
        let lines = filled_lines(skeleton);
        let mut found: Vec<Range<usize>> = Vec::new();
        for pair in lines.windows(2) {
            let [(start, attribute), (next, header)] = pair else {
                continue;
            };
            let inside = found.last().is_some_and(|module| module.end > *start);
            let opens = module_item(header).is_some_and(|(_, rest)| rest.starts_with('{'));
            if inside || attribute.trim() != "#[cfg(test)]" || !opens {
                continue;
            }
            let open = next + header.find('{')?;
            found.push(*start..block_end(skeleton, open)?);
        }
        Some(found)
    }

    /// For each `mod {module};` line of `parent`, whether the line before
    /// it (blank lines and comments aside) is `#[cfg(test)]`.
    fn declarations(parent: &Source, module: &str) -> Vec<bool> {
        let lines = filled_lines(&parent.skeleton);
        lines
            .iter()
            .enumerate()
            .filter(|(_, (_, line))| module_item(line) == Some((module, ";")))
            .map(|(at, _)| at > 0 && lines[at - 1].1.trim() == "#[cfg(test)]")
            .collect()
    }

    /// Whether `source` is built for tests only: every declaration of its
    /// module (in its parent file among `sources`) comes right under a
    /// `#[cfg(test)]` line, or is made by a test-only parent. Why it cannot
    /// be classified when nothing declares it.
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

    /// What the production `code` of the file `name` holds that it must
    /// not, or that makes it impossible to classify.
    fn forgeries(name: &str, code: &str) -> Vec<String> {
        let code = squeezed(code);
        let mut found: Vec<String> = FORGERIES
            .iter()
            .chain(&UNCLASSIFIABLE)
            .filter(|token| code.contains(*token))
            .map(|token| format!("{name}: `{token}` in production code"))
            .collect();
        let moved = code.match_indices("cfg_attr(").any(|(at, _)| {
            code[at..]
                .split(']')
                .next()
                .is_some_and(|attribute| attribute.contains("path"))
        });
        if moved {
            found.push(format!("{name}: a module path set by `cfg_attr`"));
        }
        for (at, _) in code.match_indices("KlipyClient") {
            let made = code[at..].starts_with("KlipyClient::new(");
            let imported =
                code[..at].ends_with("usebezel_klipy::") && code[at..].starts_with("KlipyClient;");
            if !made && !imported {
                found.push(format!("{name}: KLIPY's client named outside its import"));
            }
        }
        found
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

    /// D-2026-10-01-gif-sticker-search-3 (DoD critic, row 3), the source
    /// guard. [`UserAsked`] guards the GIF state, its source factory and
    /// the command functions: setup code has no command `Request` to make
    /// one from. It cannot stop the app from forging an invocation Tauri
    /// then dispatches, nor from making a second KLIPY client; this does.
    /// It reads every `.rs` file under `src/` as it runs (a new file is read
    /// too) and checks that:
    /// - production code holds none of `FORGERIES`: no invocation handed to
    ///   a webview with the app's invoke key, no script run in the window;
    /// - `KlipyClient::new` is called once, in `klipy_source`, and the type
    ///   is named nowhere else but its import;
    /// - KLIPY's API and file hosts are named nowhere in the studio, tests
    ///   included: they are `bezel-klipy`'s.
    ///
    /// Test code is left out by one rule, and only it:
    /// - a file whose module is declared only right under a `#[cfg(test)]`
    ///   line (`mod tests;`), or by such a file;
    /// - in the other files, an inline module right under a `#[cfg(test)]`
    ///   line (`mod tests {`), through the brace that closes it, found
    ///   with comments and literals set aside.
    ///
    /// Anything else marked `#[cfg(test)]` is read as production. Comments
    /// are not code; literals are. The rule fails closed: a file nothing
    /// declares, a comment, literal or test module that never closes, and
    /// production code that reads a module from another path or includes
    /// another file's code cannot be classified, and fail the test.
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
        let hosts = ["api", "static"].map(|host| format!("{host}.klipy.com"));
        let mut made = 0;
        for source in &read {
            for host in hosts
                .iter()
                .filter(|host| source.text.contains(host.as_str()))
            {
                problems.push(format!("{}: names {host}", source.name));
            }
            match test_only(source, &read) {
                Err(why) => problems.push(why),
                Ok(true) => {}
                Ok(false) => {
                    let code = source.production();
                    made += squeezed(&code).matches("KlipyClient::new(").count();
                    problems.extend(forgeries(&source.name, &code));
                }
            }
        }
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(made, 1, "KLIPY's client is made once in production");
        let lib = read.iter().find(|source| source.name == "lib.rs").unwrap();
        let factory = lib.function("klipy_source");
        assert!(factory.contains("KlipyClient::new("), "{factory}");
        // The rule read this file's tests as tests, and the GIF files as
        // they are built.
        assert!(!lib.production().contains("fn nothing_in_the_app_forges"));
        let role = |name: &str| {
            let source = read.iter().find(|source| source.name == name).unwrap();
            test_only(source, &read).unwrap()
        };
        assert!(role("gifs/tests.rs") && role("manager/tests.rs") && role("storage/tests.rs"));
        assert!(!role("gifs/asked.rs") && !role("gifs.rs") && !role("main.rs"));
    }

    /// The source guard's rule on made-up files: an inline test module ends
    /// at its own closing brace whatever its comments and literals hold, so
    /// code after it is production; one that never closes, or a literal
    /// that never closes, cannot be classified.
    #[test]
    fn the_source_guard_cuts_only_test_modules() {
        let file = r##"fn before() {}
#[cfg(test)]
mod tests {
    const C: [char; 3] = ['{', '\'', '"'];
    const S: &str = r#"}"#; // }
    /* } /* } */ */
    fn inside<'a>(_: &'a str) -> &'a str { "}\"" }
}
fn after() { window.on_message(request) }
"##;
        let source = Source::new("x.rs", file).unwrap();
        let code = source.production();
        assert!(
            code.contains("fn before()") && code.contains("fn after()"),
            "{code}"
        );
        assert!(!code.contains("fn inside"), "{code}");
        assert_eq!(forgeries("x.rs", &code).len(), 1, "{code}");
        assert!(Source::new("y.rs", "#[cfg(test)]\nmod tests {\n").is_err());
        assert!(Source::new("z.rs", "const S: &str = \"open;\n").is_err());
    }
}
