//! Command-line driving adapter of Bezel. `main.rs` is the composition root;
//! everything here is testable against any [`DeviceBus`], [`ScreenConnector`],
//! [`DesktopModeHid`], [`SensorSource`], [`FrameRenderer`], [`ThemeStore`] and
//! [`MediaTranscoder`].
#![forbid(unsafe_code)]

pub mod clock;
mod devices;
pub mod live;
mod messages;
mod screen;
mod sensors;
pub mod storage;
pub mod theme;
pub mod udev_rules;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::theme::Fit;
use bezel_core::ports::{
    DesktopModeHid, DeviceBus, FrameRenderer, MediaTranscoder, ScreenConnector, SensorSource,
    ThemeStore,
};
use clap::{Parser, Subcommand, ValueEnum};

pub use live::{Pace, RunRequest, SleepPace};
pub use sensors::{WatchStyle, run as run_sensors};
pub use storage::{ProgressStyle, StorageArgs, StorageKit, run as run_storage_command};

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
    /// screen, a Turing USB panel in desktop mode and demo sensor values
    /// (demos and tests).
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

/// Orientation names on the command line: how the screen stands on the
/// desk (`vertical`/`horizontal` work too; `reverse-*` is upside down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrientationArg {
    /// Taller than wide (vertical).
    #[value(alias = "vertical")]
    Portrait,
    /// Portrait turned 180°.
    #[value(alias = "vertical-flipped")]
    ReversePortrait,
    /// Wider than tall (horizontal).
    #[value(alias = "horizontal")]
    Landscape,
    /// Landscape turned 180°.
    #[value(alias = "horizontal-flipped")]
    ReverseLandscape,
}

/// How `bezel show` fits an image to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FitArg {
    /// Fill the screen, cropping what overflows.
    Cover,
    /// Show the whole image, with bars where it does not fill.
    Contain,
    /// Stretch to the screen.
    Fill,
    /// Original size, from the top-left corner.
    None,
}

impl From<FitArg> for Fit {
    fn from(f: FitArg) -> Self {
        match f {
            FitArg::Cover => Fit::Cover,
            FitArg::Contain => Fit::Contain,
            FitArg::Fill => Fit::Fill,
            FitArg::None => Fit::None,
        }
    }
}

impl From<Orientation> for OrientationArg {
    fn from(o: Orientation) -> Self {
        match o {
            Orientation::Portrait => OrientationArg::Portrait,
            Orientation::ReversePortrait => OrientationArg::ReversePortrait,
            Orientation::Landscape => OrientationArg::Landscape,
            Orientation::ReverseLandscape => OrientationArg::ReverseLandscape,
        }
    }
}

impl OrientationArg {
    /// The name the help and the hints use (`vertical`, `horizontal-flipped`, ...).
    pub const fn cli_name(self) -> &'static str {
        match self {
            OrientationArg::Portrait => "vertical",
            OrientationArg::ReversePortrait => "vertical-flipped",
            OrientationArg::Landscape => "horizontal",
            OrientationArg::ReverseLandscape => "horizontal-flipped",
        }
    }
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
    /// List the connected smart screens, and Turing USB panels in the
    /// vendor's desktop mode (read-only: nothing is written to them).
    Devices {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Switch a Turing USB panel in the vendor's desktop mode back to USB
    /// monitor mode (not validated on hardware): asks the panel its model,
    /// then sends the two HID reports that switch it; it restarts as a
    /// USB screen. Nothing is sent without --yes.
    MonitorMode {
        /// HID address of the panel, as `bezel devices` lists it (needed
        /// when several panels are in desktop mode).
        #[command(flatten)]
        target: Target,
        /// Confirm the switch.
        #[arg(long)]
        yes: bool,
    },
    /// Print the Linux udev rule that lets your user open every supported
    /// screen without root (stdout), and the one-line sudo command that
    /// installs it (stderr). Bezel never runs that command itself.
    UdevRules,
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
    /// Show a picture (PNG, JPEG or GIF) until something else is drawn.
    Show {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// The picture.
        image: PathBuf,
        /// Orientation (default: horizontal for a wide picture, vertical
        /// otherwise).
        #[arg(long, value_enum)]
        orientation: Option<OrientationArg>,
        /// How the picture fills the screen.
        #[arg(long, value_enum, default_value_t = FitArg::Cover)]
        fit: FitArg,
    },
    /// Turn the screen off. The next picture, pattern or theme wakes it.
    Off {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
    },
    /// Show the machine's sensors: CPU, GPU, memory, disks, network, board.
    /// Rates and usages are measured between two samples 250 ms apart, so
    /// the first output takes a quarter of a second.
    Sensors(SensorsArgs),
    /// Render one frame of a theme to a PNG of its canvas size, with this
    /// machine's sensors (the demo values with --fake).
    Render {
        /// A .bezeltheme file or theme folder, the name of a bundled theme, or
        /// another app's theme to preview (.turtheme, theme.yaml or a
        /// turing-smart-screen-python theme folder).
        theme: PathBuf,
        /// The PNG to write.
        #[arg(long, short = 'o', value_name = "PNG")]
        output: PathBuf,
    },
    /// Show a theme on the screen with live sensors until Ctrl+C, then hand
    /// the screen back to its standalone mode.
    Run {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// A .bezeltheme file or theme folder, the name of a bundled theme, or
        /// another app's theme (converted on the fly).
        theme: PathBuf,
        /// Stop after N frames.
        #[arg(long, value_name = "N", hide = true)]
        frames: Option<u64>,
        /// The ffmpeg program (or its folder) that decodes a video background
        /// for screens that cannot play it themselves; default: ffmpeg on the
        /// PATH.
        #[arg(long, value_name = "PATH")]
        ffmpeg: Option<PathBuf>,
    },
    /// Convert another app's theme (.turtheme, theme.yaml or a
    /// turing-smart-screen-python theme folder) into a native Bezel theme.
    Import {
        /// The theme to convert.
        source: PathBuf,
        /// A .bezeltheme file to write, or a folder for any other name.
        #[arg(long, short = 'o', value_name = "DEST")]
        output: PathBuf,
    },
    /// The files a screen stores (internal flash and memory card): list,
    /// send, delete, play, and what it shows on its own after power-up.
    Storage(StorageArgs),
}

impl Command {
    /// The theme a `render` or `run` command names.
    pub fn theme(&self) -> Option<&Path> {
        match self {
            Command::Render { theme, .. } | Command::Run { theme, .. } => Some(theme),
            _ => None,
        }
    }

    /// The ffmpeg a `run` or `storage put` command names with `--ffmpeg`.
    pub fn ffmpeg(&self) -> Option<&Path> {
        match self {
            Command::Run { ffmpeg, .. } => ffmpeg.as_deref(),
            Command::Storage(args) => args.ffmpeg(),
            _ => None,
        }
    }
}

/// What `render` and `run` draw with, built by the composition root.
pub struct Rendering<'a> {
    /// Reads native themes.
    pub store: &'a dyn ThemeStore,
    /// Draws frames.
    pub renderer: &'a mut dyn FrameRenderer,
    /// Measures the machine.
    pub sensors: &'a mut dyn SensorSource,
    /// The local wall-clock time.
    pub clock: &'a dyn Fn() -> LocalTime,
    /// Language of day and month names.
    pub language: Language,
    /// Folder of the themes that ship with Bezel, when installed.
    pub bundled: Option<&'a Path>,
}

/// Runs `render`, `run` or `import` and returns what should be printed on
/// stdout; `run` reports progress on `log`, waits with `pace` and decodes a
/// video background with `media` for screens that cannot play it.
pub fn run_theme_command<B, C>(
    cli: &Cli,
    bus: &B,
    connector: &C,
    kit: &mut Rendering<'_>,
    media: &mut dyn MediaTranscoder,
    pace: &mut dyn Pace,
    log: &mut dyn Write,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    match &cli.command {
        Command::Render { theme, output } => {
            let path = theme::resolve(theme, kit.bundled)?;
            theme::render(kit, &path, output, &mut |d| pace.wait(d), log)
        }
        Command::Run {
            target,
            theme,
            frames,
            ..
        } => {
            let path = theme::resolve(theme, kit.bundled)?;
            let request = RunRequest {
                target,
                theme: &path,
                max_frames: *frames,
            };
            live::run(bus, connector, kit, request, media, pace, log)
        }
        Command::Import { source, output } => theme::import(kit.store, source, output),
        _ => anyhow::bail!("not a theme command"),
    }
}

/// Runs a parsed command and returns what should be printed on stdout.
/// `renderer` draws pictures and themes for the screen.
pub fn run<B, C>(
    cli: &Cli,
    bus: &B,
    connector: &C,
    renderer: &mut dyn FrameRenderer,
) -> anyhow::Result<String>
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
        Command::Show {
            target,
            image,
            orientation,
            fit,
        } => screen::show(
            bus,
            connector,
            renderer,
            target,
            image,
            orientation.map(Into::into),
            (*fit).into(),
        ),
        Command::Off { target } => screen::off(bus, connector, target),
        // Streams its output and needs a sensor source: see `run_sensors`.
        Command::Sensors(_) => anyhow::bail!("`sensors` runs through run_sensors"),
        // Need sensors and themes: see `run_theme_command`.
        Command::Render { .. } | Command::Run { .. } | Command::Import { .. } => {
            anyhow::bail!("theme commands run through run_theme_command")
        }
        // Need a media transcoder and a cancel token: see `run_storage_command`.
        Command::Storage(_) => anyhow::bail!("storage commands run through run_storage_command"),
        // Needs the HID port: see `run_monitor_mode`.
        Command::MonitorMode { .. } => {
            anyhow::bail!("`monitor-mode` runs through run_monitor_mode")
        }
        // Writes the install command to stderr: see `udev_rules::run`.
        Command::UdevRules => anyhow::bail!("`udev-rules` runs through udev_rules::run"),
    }
}

/// Runs `monitor-mode` (anything else is an error): switches a panel in
/// desktop mode back to USB monitor mode through `hid`, only with `--yes`.
/// Says what it is about to do on `log`; returns what goes to stdout.
pub fn run_monitor_mode<B, H>(
    cli: &Cli,
    bus: &B,
    hid: &H,
    log: &mut dyn Write,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    H: DesktopModeHid + ?Sized,
{
    match &cli.command {
        Command::MonitorMode { target, yes } => {
            devices::monitor_mode(bus, hid, target.screen.as_deref(), *yes, log)
        }
        _ => anyhow::bail!("not the monitor-mode command"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_render::{SkiaRenderer, SystemFonts};

    fn run_args(args: &[&str]) -> anyhow::Result<String> {
        let cli = Cli::try_parse_from(args)?;
        let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
        run(
            &cli,
            &FakeBus::turing_88(),
            &FakeConnector::default(),
            &mut renderer,
        )
    }

    #[test]
    fn run_dispatches_screen_commands_and_leaves_sensors_to_run_sensors() {
        let listed = run_args(&["bezel", "--fake", "devices"]).unwrap();
        assert!(listed.starts_with("1. Turing"), "{listed}");
        let err = run_args(&["bezel", "--fake", "sensors"]).unwrap_err();
        assert!(err.to_string().contains("run_sensors"), "{err}");
        let err = run_args(&["bezel", "--fake", "storage", "info"]).unwrap_err();
        assert!(err.to_string().contains("run_storage_command"), "{err}");
        let err = run_args(&["bezel", "--fake", "monitor-mode", "--yes"]).unwrap_err();
        assert!(err.to_string().contains("run_monitor_mode"), "{err}");
        let err = run_args(&["bezel", "udev-rules"]).unwrap_err();
        assert!(err.to_string().contains("udev_rules::run"), "{err}");
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

    /// Never waits and is never stopped (`--frames` ends the run).
    struct NoWait;

    impl Pace for NoWait {
        fn wait(&mut self, _: Duration) {}

        fn stopped(&self) -> bool {
            false
        }
    }

    fn theme_command(args: &[&str]) -> anyhow::Result<(String, FakeConnector)> {
        let cli = Cli::try_parse_from(args)?;
        let themes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
        let mut sensors = bezel_sensors::FakeSensors::demo();
        let now = || bezel_core::domain::clock::LocalTime {
            year: 2026,
            month: 9,
            day: 30,
            hour: 21,
            minute: 5,
            second: 0,
            weekday: 2,
        };
        let mut kit = Rendering {
            store: &bezel_themes::FsThemeStore,
            renderer: &mut renderer,
            sensors: &mut sensors,
            clock: &now,
            language: Language::English,
            bundled: Some(&themes),
        };
        let connector = FakeConnector::default();
        let out = run_theme_command(
            &cli,
            &FakeBus::turing_88(),
            &connector,
            &mut kit,
            &mut storage::doubles::StubMedia::missing(),
            &mut NoWait,
            &mut Vec::new(),
        )?;
        Ok((out, connector))
    }

    #[test]
    fn theme_commands_run_through_run_theme_command() {
        let png = std::env::temp_dir().join(format!("bezel-lib-{}.png", std::process::id()));
        let png_arg = png.display().to_string();
        let err = run_args(&["bezel", "--fake", "render", "x", "-o", &png_arg]).unwrap_err();
        assert!(err.to_string().contains("run_theme_command"), "{err}");
        assert!(theme_command(&["bezel", "devices"]).is_err());

        let (out, _) =
            theme_command(&["bezel", "render", "turing-2.1-round", "-o", &png_arg]).unwrap();
        assert!(out.contains("(480x480 vertical)"), "{out}");
        let (out, connector) =
            theme_command(&["bezel", "run", "turing-8.8-vertical", "--frames", "1"]).unwrap();
        assert!(out.contains("1 frames"), "{out}");
        assert_eq!(connector.log().releases, 1);
        let folder = std::env::temp_dir().join(format!("bezel-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/turing-2.1-round");
        let (out, _) = theme_command(&[
            "bezel",
            "import",
            &src.display().to_string(),
            "-o",
            &folder.display().to_string(),
        ])
        .unwrap();
        assert!(
            out.contains("Midnight 2.1\" round (480x480 vertical)"),
            "{out}"
        );
        assert!(folder.join("theme.json").is_file());
        let cli = Cli::try_parse_from(["bezel", "run", "a", "--ffmpeg", "/opt/ffmpeg"]).unwrap();
        assert_eq!(cli.command.theme(), Some(Path::new("a")));
        assert_eq!(cli.command.ffmpeg(), Some(Path::new("/opt/ffmpeg")));
        let cli = Cli::try_parse_from(["bezel", "import", "a", "-o", "b"]).unwrap();
        assert_eq!(cli.command.theme(), None);
        assert_eq!(cli.command.ffmpeg(), None);
        let _ = std::fs::remove_dir_all(folder);
        let _ = std::fs::remove_file(png);
    }

    #[test]
    fn orientations_accept_vertical_and_horizontal() {
        for (name, expected) in [
            ("vertical", OrientationArg::Portrait),
            ("horizontal", OrientationArg::Landscape),
            ("horizontal-flipped", OrientationArg::ReverseLandscape),
            ("reverse-portrait", OrientationArg::ReversePortrait),
        ] {
            let cli =
                Cli::try_parse_from(["bezel", "test-pattern", "--orientation", name]).unwrap();
            let Command::TestPattern { orientation, .. } = cli.command else {
                unreachable!("parsed as test-pattern")
            };
            assert_eq!(orientation, expected, "{name}");
            let back = OrientationArg::from(Orientation::from(expected));
            assert_eq!(back, expected);
            // The hints name orientations the way the command line reads them.
            let hinted = OrientationArg::from_str(back.cli_name(), false).unwrap();
            assert_eq!(hinted, expected, "{name}");
        }
        assert_eq!(Fit::from(FitArg::Contain), Fit::Contain);
        let off = run_args(&["bezel", "--fake", "off"]).unwrap();
        assert!(off.contains("off"), "{off}");
        assert!(run_args(&["bezel", "--fake", "show", "/nope.png"]).is_err());
    }
}
