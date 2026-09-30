//! `bezel`: command-line control of USB smart screens. Composition root.
#![forbid(unsafe_code)]

use bezel_cli::{Cli, Command, SensorsArgs, WatchStyle, run, run_sensors};
use bezel_core::ports::SensorSource;
use bezel_devices::{FakeBus, FakeConnector, SystemBus, SystemConnector};
use bezel_sensors::{FakeSensors, SystemSensors};
use clap::Parser;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

/// `bezel sensors`, streamed straight to stdout.
fn sensors(args: &SensorsArgs, fake: bool) -> anyhow::Result<String> {
    let mut source: Box<dyn SensorSource> = if fake {
        Box::new(FakeSensors::demo())
    } else {
        Box::new(SystemSensors::new())
    };
    let mut stdout = std::io::stdout().lock();
    let style = if stdout.is_terminal() {
        WatchStyle::Redraw
    } else {
        WatchStyle::Append
    };
    run_sensors(args, source.as_mut(), &mut stdout, style)?;
    Ok(String::new())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("bezel=debug,bezel_devices=debug,bezel_core=debug")
            .with_writer(std::io::stderr)
            .init();
    }
    let result = match &cli.command {
        Command::Sensors(args) => sensors(args, cli.fake),
        _ if cli.fake => run(&cli, &FakeBus::turing_88(), &FakeConnector::default()),
        _ => run(&cli, &SystemBus, &SystemConnector),
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
