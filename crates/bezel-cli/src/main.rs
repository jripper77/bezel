//! `bezel`: command-line control of USB smart screens. Composition root.
#![forbid(unsafe_code)]

use bezel_cli::{Cli, run};
use bezel_devices::{FakeBus, FakeConnector, SystemBus, SystemConnector};
use clap::Parser;
use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("bezel=debug,bezel_devices=debug,bezel_core=debug")
            .with_writer(std::io::stderr)
            .init();
    }
    let result = if cli.fake {
        run(&cli, &FakeBus::turing_88(), &FakeConnector::default())
    } else {
        run(&cli, &SystemBus, &SystemConnector)
    };
    match result {
        Ok(out) => {
            let mut stdout = std::io::stdout().lock();
            // A closed pipe (`bezel devices | head`) is not an error worth a panic.
            let _ = stdout.write_all(out.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bezel: {e:#}");
            ExitCode::FAILURE
        }
    }
}
