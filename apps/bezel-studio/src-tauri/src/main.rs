//! Binary entry point of Bezel Studio: hooks panics, starts the app and
//! turns a startup failure into a non-zero exit code.

#![forbid(unsafe_code)]
// Release builds on Windows must not open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use bezel_studio::diag;

fn main() -> ExitCode {
    // First: from here on, a panic (the app's, Tauri's, a library's) says a
    // fixed line and where it happened, never its message
    // (D-2026-10-01-gif-sticker-search-14).
    diag::hook_panics();
    match bezel_studio::run() {
        Ok(()) => ExitCode::SUCCESS,
        // Which part failed, by the variant of Tauri's error, without its
        // text: the studio prints fixed text only
        // (D-2026-10-01-gif-sticker-search-12). A failure in Tauri's setup
        // does not come back here: Tauri panics.
        Err(error) => {
            diag::report(bezel_studio::start_failure(&error));
            ExitCode::FAILURE
        }
    }
}
