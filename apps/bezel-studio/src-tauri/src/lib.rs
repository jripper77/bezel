//! Bezel Studio — the desktop driving adapter of Bezel.
//!
//! The webview UI turns clicks into calls on the [`backend`]; every rule
//! about screens, sensors and themes lives in the core and its adapters.
//! [`run`] is the composition root: it picks real or simulated adapters,
//! starts the refresh loop that samples sensors and feeds the live screen,
//! and keeps the app in the tray while a screen is live.

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
use bezel_core::ports::{DesktopModeHid, DeviceBus, GifCollection, GifSource, ScreenConnector};
use bezel_devices::fake::FakeStorage;
use bezel_devices::{FakeBus, FakeConnector, FakeHid, SystemBus, SystemConnector, SystemHid};
use bezel_klipy::KlipyClient;
use bezel_media::FfmpegTranscoder;
use bezel_media::archive::{DiskArchive, MemoryArchive, storage_dir};
use bezel_media::collection::{DiskCollection, MemoryCollection, collection_dir};
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::{FakeSensors, SystemSensors};
use bezel_themes::FsThemeStore;
use tauri::{AppHandle, Emitter as _, Manager, WindowEvent};

use crate::backend::{
    Backend, DEFAULT_MODEL, SensorFactory, Session, default_orientation, sleep_until,
};
use crate::commands::{Shared, Unsaved};
use crate::gifs::{Gifs, KEY_FILE, KeyFile, Provider, SharedGifs};
use crate::library::ThemeLibrary;
use crate::manager::Copies;
use crate::settings::SettingsFile;
use crate::storage::{MediaSetup, StorageState};
use crate::studio::Studio;
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

    let simulate = switch_on(std::env::var_os(SIMULATION_SWITCH).as_deref());
    let hidden = std::env::args().any(|a| a == HIDDEN_ARG);
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
        .setup(move |app| {
            let backend: Shared = Arc::new(compose(app.handle(), simulate)?);
            // Before the window asks for it: the last theme, or a blank one
            // for the connected screen.
            backend.restore_theme();
            app.manage(Arc::clone(&backend));
            // Its own state, which asks nothing of KLIPY until a search.
            let gifs: SharedGifs = Arc::new(gifs(app.handle())?);
            app.manage(gifs);
            app.manage(Unsaved::default());
            let live = backend.studio().live_key().is_some();
            let tray = tray::create(app.handle(), live, &backend.texts())?;
            app.manage(tray.live().clone());
            app.manage(tray.clone());
            start_refresh_loop(backend, tray.live().clone());
            if !hidden {
                show_main_window(app.handle());
            }
            Ok(())
        })
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

/// Shows, restores and focuses the main window.
pub(crate) fn show_main_window(app: &AppHandle) {
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

/// Folders of the themes that ship with Bezel: next to the installed app
/// (packages) and in the user's data folder (`install-local.sh`).
fn bundled_theme_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let path = app.path();
    [
        path.resource_dir().ok().map(|d| d.join("themes")),
        path.data_dir().ok().map(|d| d.join("bezel").join("themes")),
    ]
    .into_iter()
    .flatten()
    .filter(|d| d.is_dir())
    .collect()
}

fn compose(app: &AppHandle, simulate: bool) -> tauri::Result<Backend> {
    let Adapters {
        bus,
        connector,
        hid,
        sensors,
    } = adapters(simulate);
    // The bundled themes' fonts first, so previews match every machine.
    let bundled_dirs = bundled_theme_dirs(app);
    let bundled_fonts = bundled_dirs
        .iter()
        .flat_map(|dir| font_files(&dir.join("fonts")))
        .collect();
    let renderer = SkiaRenderer::with_fonts(bundled_fonts, SystemFonts::Load);
    let fonts = renderer.font_families();
    let path = app.path();
    let settings = SettingsFile::new(path.app_config_dir()?.join("settings.json"));
    let system_language = clock::language();
    let language = settings.load().language().unwrap_or(system_language);
    let ffmpeg = settings.load().ffmpeg_path.map(PathBuf::from);
    let cache = path.app_cache_dir()?;
    let copies = if simulate {
        // The simulated 8.8" is keyed like a real one: never in the
        // user's catalog.
        Copies::in_memory(MemoryArchive::new())
    } else {
        copies(&path.data_dir()?)
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
    Ok(Backend {
        bus,
        connector,
        hid,
        store: Arc::new(FsThemeStore),
        library: ThemeLibrary::new(path.app_data_dir()?.join("themes"), bundled_theme_dirs(app)),
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
    })
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

/// The GIF search and the collection (D-2026-10-01-gif-sticker-search-3,
/// -5): the KLIPY key in `<config>/klipy.json`, the collection in
/// `<data>/bezel/collection` shared with the CLI's data folder (in memory for
/// this run when that folder cannot be made), a background's copy in
/// `<cache>/collection`. Nothing is read from KLIPY here.
fn gifs(app: &AppHandle) -> tauri::Result<Gifs> {
    let path = app.path();
    let collection: Box<dyn GifCollection> =
        match DiskCollection::open(collection_dir(&path.data_dir()?)) {
            Ok(disk) => Box::new(disk),
            Err(e) => {
                tracing::error!("the GIF collection is kept for this run only: {e}");
                Box::new(MemoryCollection::new())
            }
        };
    let provider = Provider {
        source: Arc::new(|key: &str, customer: &str| -> Arc<dyn GifSource> {
            Arc::new(KlipyClient::new(key, customer))
        }),
        customer_id: bezel_klipy::new_customer_id,
    };
    Ok(Gifs::new(
        KeyFile::new(path.app_config_dir()?.join(KEY_FILE)),
        provider,
        collection,
        path.app_cache_dir()?.join("collection"),
    ))
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
fn start_refresh_loop(backend: Shared, live_item: tray::LiveItem) {
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
                live_item.sync(live);
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
    use super::*;
    use bezel_core::app::discover_screens;

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
}
