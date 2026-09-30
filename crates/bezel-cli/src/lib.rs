//! Command-line driving adapter of Bezel. `main.rs` is the composition root;
//! everything here is testable against any [`DeviceBus`].
#![forbid(unsafe_code)]

mod devices;

use bezel_core::ports::DeviceBus;
use clap::{Parser, Subcommand};

/// Product version: the one CI or `scripts/install-local.sh` stamped, else the crate's.
pub const VERSION: &str = match option_env!("BEZEL_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Control USB smart screens (Turing, TURZX and compatible) from the terminal.
#[derive(Debug, Parser)]
#[command(name = "bezel", version = VERSION)]
pub struct Cli {
    /// Use a simulated Turing 8.8" instead of the real USB bus (demos and tests).
    #[arg(long, global = true, hide = true)]
    pub fake: bool,

    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List the connected smart screens (read-only: nothing is written to them).
    Devices {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

/// Runs a parsed command and returns what should be printed on stdout.
pub fn run<B: DeviceBus + ?Sized>(cli: &Cli, bus: &B) -> anyhow::Result<String> {
    match &cli.command {
        Command::Devices { json } => devices::list(bus, *json),
    }
}
