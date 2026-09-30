//! Command-line driving adapter of Bezel. `main.rs` is the composition root;
//! everything here is testable against any [`DeviceBus`] and [`ScreenConnector`].
#![forbid(unsafe_code)]

mod devices;
mod screen;

use bezel_core::domain::geometry::Orientation;
use bezel_core::ports::{DeviceBus, ScreenConnector};
use clap::{Parser, Subcommand, ValueEnum};

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

    /// Log what is sent to and received from the screen (stderr).
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,

    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// Which screen a command talks to.
#[derive(Debug, Clone, clap::Args)]
pub struct Target {
    /// Port or USB address of the screen (default: the first awake screen).
    #[arg(long, short = 's')]
    pub screen: Option<String>,
}

/// Orientation names on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrientationArg {
    /// Taller than wide.
    Portrait,
    /// Portrait turned 180°.
    ReversePortrait,
    /// Wider than tall.
    Landscape,
    /// Landscape turned 180°.
    ReverseLandscape,
}

impl From<OrientationArg> for Orientation {
    fn from(o: OrientationArg) -> Self {
        match o {
            OrientationArg::Portrait => Orientation::Portrait,
            OrientationArg::ReversePortrait => Orientation::ReversePortrait,
            OrientationArg::Landscape => Orientation::Landscape,
            OrientationArg::ReverseLandscape => Orientation::ReverseLandscape,
        }
    }
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
    /// Show an animated test pattern: color bars, a marker in each corner
    /// (red top-left, green top-right, white bottom-right, blue bottom-left)
    /// and a moving strip that exercises partial updates.
    TestPattern {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// How long to animate.
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        /// Orientation to draw in.
        #[arg(long, value_enum, default_value_t = OrientationArg::Portrait)]
        orientation: OrientationArg,
        /// Hand the screen back to its standalone mode when done.
        #[arg(long)]
        release: bool,
    },
    /// Set the backlight level.
    Brightness {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// Level in percent (0-100).
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        percent: u8,
    },
    /// Hand the screen back to its standalone mode (clock or stored media).
    Release {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
    },
}

/// Runs a parsed command and returns what should be printed on stdout.
pub fn run<B, C>(cli: &Cli, bus: &B, connector: &C) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    match &cli.command {
        Command::Devices { json } => devices::list(bus, *json),
        Command::TestPattern {
            target,
            seconds,
            orientation,
            release,
        } => screen::test_pattern(
            bus,
            connector,
            target,
            *seconds,
            (*orientation).into(),
            *release,
        ),
        Command::Brightness { target, percent } => {
            screen::brightness(bus, connector, target, *percent)
        }
        Command::Release { target } => screen::release(bus, connector, target),
    }
}
