//! Binary entry point of Bezel Studio: starts the app and turns a startup
//! failure into a non-zero exit code.

#![forbid(unsafe_code)]
// Release builds on Windows must not open a console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    match bezel_studio::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bezel-studio: {error}");
            ExitCode::FAILURE
        }
    }
}
