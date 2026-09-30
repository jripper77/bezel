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
pub mod library;
pub mod media;
pub mod settings;
pub mod studio;
mod tray;

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::theme::Theme;
use bezel_core::ports::{DeviceBus, ScreenConnector, SensorSource};
use bezel_devices::{FakeBus, FakeConnector, SystemBus, SystemConnector};
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::{FakeSensors, SystemSensors};
use bezel_themes::FsThemeStore;
use tauri::{AppHandle, Manager, WindowEvent};

use crate::backend::{Backend, DEFAULT_MODEL, UNTITLED, default_orientation};
use crate::commands::Shared;
use crate::library::ThemeLibrary;
use crate::settings::SettingsFile;
use crate::studio::Studio;

/// Label of the main window in `tauri.conf.json`.
const MAIN_WINDOW: &str = "main";

/// Set to `1` to serve a simulated Turing 8.8" and scripted sensors instead
/// of the real machine: demos and checks that must never touch a screen.
pub const SIMULATION_SWITCH: &str = "BEZEL_FAKE";

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
            start_refresh_loop(backend);
            tray::create(app.handle())?;
            if !hidden {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // While a screen is live, closing the window keeps Bezel in the
            // tray so the screen keeps updating.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let live = window
                    .try_state::<Shared>()
                    .is_some_and(|b| b.studio().live_key().is_some());
                if live {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_screens,
            commands::sensor_catalog,
            commands::sample_sensors,
            commands::editor_session,
            commands::render_preview,
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
            commands::list_assets,
            commands::list_fonts,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .run(tauri::generate_context!())
}

/// Shows, restores and focuses the main window.
pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The adapters: real ones, or simulated ones when `simulate`.
struct Adapters {
    bus: Arc<dyn DeviceBus + Send + Sync>,
    connector: Arc<dyn ScreenConnector + Send + Sync>,
    sensors: Box<dyn SensorSource>,
}

fn adapters(simulate: bool) -> Adapters {
    if simulate {
        eprintln!("bezel-studio: {SIMULATION_SWITCH}=1, simulated Turing 8.8\" and sensors");
        Adapters {
            bus: Arc::new(FakeBus::turing_88()),
            connector: Arc::new(FakeConnector::default()),
            sensors: Box::new(FakeSensors::demo()),
        }
    } else {
        Adapters {
            bus: Arc::new(SystemBus),
            connector: Arc::new(SystemConnector),
            sensors: Box::new(SystemSensors::new()),
        }
    }
}

/// A blank theme for the most common screen (horizontal, like every
/// bar-shaped one), until [`Backend::restore_theme`] picks the real one.
fn starting_theme() -> Theme {
    match model_by_id(DEFAULT_MODEL) {
        Some(m) => Theme::blank(UNTITLED, m.panel, default_orientation(m)),
        None => Theme::blank(UNTITLED, Size::new(480, 1920), Orientation::Landscape),
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
        sensors,
    } = adapters(simulate);
    // The bundled themes' fonts first, so previews match every machine.
    let bundled_fonts = bundled_theme_dirs(app)
        .iter()
        .flat_map(|dir| font_files(&dir.join("fonts")))
        .collect();
    let renderer = SkiaRenderer::with_fonts(bundled_fonts, SystemFonts::Load);
    let fonts = renderer.font_families();
    let studio = Studio::new(
        sensors,
        Box::new(renderer),
        clock::language(),
        starting_theme(),
    );
    let path = app.path();
    Ok(Backend {
        bus,
        connector,
        store: Arc::new(FsThemeStore),
        library: ThemeLibrary::new(path.app_data_dir()?.join("themes"), bundled_theme_dirs(app)),
        settings: SettingsFile::new(path.app_config_dir()?.join("settings.json")),
        fonts,
        studio: Mutex::new(studio),
    })
}

/// Shows the last live screen again, then samples and refreshes the live
/// screen at the theme's pace, on its own thread for the life of the app.
fn start_refresh_loop(backend: Shared) {
    let spawned = std::thread::Builder::new()
        .name("bezel-refresh".into())
        .spawn(move || {
            if let Err(e) = backend.studio().refresh_catalog() {
                tracing::warn!("sensor catalog: {e}");
            }
            backend.restore_live(clock::now());
            loop {
                let wait = backend.tick(clock::now());
                std::thread::sleep(Duration::from_secs_f32(wait));
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
        let mut a = adapters(true);
        assert_eq!(discover_screens(a.bus.as_ref()).unwrap().len(), 1);
        assert!(!a.sensors.catalog().unwrap().is_empty());
    }

    #[test]
    fn the_starting_theme_fits_the_88_horizontally() {
        let theme = starting_theme();
        assert_eq!(theme.canvas, Size::new(1920, 480));
        assert_eq!(theme.orientation, Orientation::Landscape);
        assert!(theme.elements.is_empty());
    }
}
