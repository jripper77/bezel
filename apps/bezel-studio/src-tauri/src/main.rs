//! Binary entry point of Bezel Studio: starts the app and turns a startup
//! failure into a non-zero exit code.

#![forbid(unsafe_code)]
// Release builds on Windows must not open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use bezel_studio::diag::{self, DiagCode};

fn main() -> ExitCode {
    match bezel_studio::run() {
        Ok(()) => ExitCode::SUCCESS,
        // Said without Tauri's error text: the studio prints fixed text only
        // (D-2026-10-01-gif-sticker-search-12).
        Err(_) => {
            diag::report(DiagCode::NotStarted);
            ExitCode::FAILURE
        }
    }
}
