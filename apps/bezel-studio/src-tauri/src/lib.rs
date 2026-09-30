//! Bezel Studio — the desktop driving adapter of Bezel.
//!
//! The webview UI turns clicks into calls on the core's use cases; every rule
//! about screens lives in the core. [`run`] is the composition root.

#![forbid(unsafe_code)]

pub mod commands;
pub mod dto;

use std::ffi::OsStr;
use std::sync::Arc;

use bezel_devices::{FakeBus, SystemBus};
use tauri::Manager;

use crate::commands::{AppState, SharedBus};

/// Label of the main window in `tauri.conf.json`.
const MAIN_WINDOW: &str = "main";

/// Set to `1` to serve a simulated Turing 8.8" instead of the real USB bus:
/// demos and end-to-end checks that must never touch a real screen.
pub const SIMULATION_SWITCH: &str = "BEZEL_FAKE";

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

    let state = AppState {
        bus: compose_bus(std::env::var_os(SIMULATION_SWITCH).as_deref()),
    };
    tauri::Builder::default()
        // First plugin: a second launch focuses this window and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(state)
        .invoke_handler(tauri::generate_handler![commands::list_screens])
        .run(tauri::generate_context!())
}

/// The bus the app talks to: the simulated one when the switch is `1`.
pub fn compose_bus(switch: Option<&OsStr>) -> SharedBus {
    if switch_on(switch) {
        eprintln!("bezel-studio: {SIMULATION_SWITCH}=1, serving a simulated Turing 8.8\"");
        Arc::new(FakeBus::turing_88())
    } else {
        Arc::new(SystemBus)
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
    fn simulated_bus_has_the_turing_88() {
        let bus = compose_bus(Some(OsStr::new("1")));
        assert_eq!(discover_screens(bus.as_ref()).unwrap().len(), 1);
    }
}
