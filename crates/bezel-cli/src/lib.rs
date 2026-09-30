//! Command-line driving adapter of Bezel. `main.rs` is the composition root;
//! everything here is testable against any [`DeviceBus`], [`ScreenConnector`]
//! and [`SensorSource`](bezel_core::ports::SensorSource).
#![forbid(unsafe_code)]

mod devices;
mod screen;
mod sensors;

use std::time::Duration;

use bezel_core::domain::geometry::Orientation;
use bezel_core::ports::{DeviceBus, ScreenConnector};
use clap::{Parser, Subcommand, ValueEnum};

pub use sensors::{WatchStyle, run as run_sensors};

/// Product version: the one CI or `scripts/install-local.sh` stamped, else the crate's.
pub const VERSION: &str = match option_env!("BEZEL_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Control USB smart screens (Turing, TURZX and compatible) from the terminal.
#[derive(Debug, Parser)]
#[command(name = "bezel", version = VERSION)]
pub struct Cli {
    /// Use simulated hardware instead of the real machine: a Turing 8.8"
    /// screen and demo sensor values (demos and tests).
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

/// Options of `bezel sensors`.
#[derive(Debug, Clone, clap::Args)]
pub struct SensorsArgs {
    /// Print JSON instead of a table (one document per line with --watch).
    #[arg(long)]
    pub json: bool,
    /// Refresh every SECS seconds (fractions allowed, at least 0.25) until
    /// interrupted.
    #[arg(long, value_name = "SECS", value_parser = sensors::parse_interval)]
    pub watch: Option<Duration>,
    /// Stop after N refreshes of --watch.
    #[arg(long, value_name = "N", requires = "watch")]
    pub count: Option<u64>,
    /// Report how long one sample of every sensor took (JSON: sampleMillis).
    #[arg(long)]
    pub timing: bool,
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
    /// Show the machine's sensors: CPU, GPU, memory, disks, network, board.
    /// Rates and usages are measured between two samples 250 ms apart, so
    /// the first output takes a quarter of a second.
    Sensors(SensorsArgs),
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
        // Streams its output and needs a sensor source: see `run_sensors`.
        Command::Sensors(_) => anyhow::bail!("`sensors` runs through run_sensors"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_devices::{FakeBus, FakeConnector};

    fn run_args(args: &[&str]) -> anyhow::Result<String> {
        let cli = Cli::try_parse_from(args)?;
        run(&cli, &FakeBus::turing_88(), &FakeConnector::default())
    }

    #[test]
    fn run_dispatches_screen_commands_and_leaves_sensors_to_run_sensors() {
        let listed = run_args(&["bezel", "--fake", "devices"]).unwrap();
        assert!(listed.starts_with("1. Turing"), "{listed}");
        let err = run_args(&["bezel", "--fake", "sensors"]).unwrap_err();
        assert!(err.to_string().contains("run_sensors"), "{err}");
        assert!(
            run_args(&["bezel", "sensors", "--count", "2"]).is_err(),
            "--count needs --watch"
        );
        let cli =
            Cli::try_parse_from(["bezel", "sensors", "--watch", "0.5", "--count", "3"]).unwrap();
        let Command::Sensors(args) = cli.command else {
            unreachable!("parsed as sensors")
        };
        assert_eq!(args.watch, Some(Duration::from_millis(500)));
        assert_eq!(args.count, Some(3));
    }
}
