//! `bezel`: command-line control of USB smart screens. Composition root.
#![forbid(unsafe_code)]

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bezel_cli::theme::{bundled_candidates, data_home, first_dir, font_dirs, resolve};
use bezel_cli::{
    Cli, Command, ProgressStyle, Rendering, SensorsArgs, SleepPace, StorageArgs, StorageKit,
    WatchStyle, clock, run, run_monitor_mode, run_sensors, run_storage_command, run_theme_command,
    udev_rules,
};
use bezel_core::domain::job::CancelToken;
use bezel_core::domain::storage::RemotePath;
use bezel_core::ports::SensorSource;
use bezel_devices::fake::FakeStorage;
use bezel_devices::{FakeBus, FakeConnector, FakeHid, SystemBus, SystemConnector, SystemHid};
use bezel_media::FfmpegTranscoder;
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::{FakeSensors, SystemSensors};
use bezel_themes::FsThemeStore;
use clap::Parser;

/// Files on the simulated 8.8" of `--fake` (fresh in every process), so
/// that `bezel --fake storage ls` has something to show.
const DEMO_FILES: &[(&str, usize)] = &[
    ("internal/image/bezel_demo.png", 48 * 1024),
    ("internal/video/bezel_demo.mp4", 2_400 * 1024),
    ("sd/video/bezel_loop.mp4", 750 * 1024),
];

/// Usable space of the simulated memory card: 8 GiB.
const DEMO_CARD_BYTES: u64 = 8 << 30;

/// The simulated bus of `--fake`: a Turing 8.8" and a Turing USB panel in
/// desktop mode.
fn fake_bus() -> FakeBus {
    FakeBus::turing_88().and(FakeBus::desktop_mode())
}

/// The model byte the simulated panel in desktop mode answers: an 8.8".
const FAKE_DESKTOP_MODEL: u8 = 0x88;

/// The simulated screen of `--fake`: a Turing 8.8" with a few demo files
/// and a memory card.
fn fake_connector() -> FakeConnector {
    let mut storage = FakeStorage::default().with_card(DEMO_CARD_BYTES);
    for (path, bytes) in DEMO_FILES {
        if let Ok(path) = RemotePath::parse(path) {
            storage = storage.with_file(path, vec![0x5a; *bytes]);
        }
    }
    FakeConnector::with_storage(storage)
}

fn sensor_source(fake: bool) -> Box<dyn SensorSource> {
    if fake {
        Box::new(FakeSensors::demo())
    } else {
        Box::new(SystemSensors::new())
    }
}

/// `bezel sensors`, streamed straight to stdout.
fn sensors(args: &SensorsArgs, fake: bool) -> anyhow::Result<String> {
    let mut source = sensor_source(fake);
    let mut stdout = std::io::stdout().lock();
    let style = if stdout.is_terminal() {
        WatchStyle::Redraw
    } else {
        WatchStyle::Append
    };
    run_sensors(args, source.as_mut(), &mut stdout, style)?;
    Ok(String::new())
}

/// The folder of the themes that ship with Bezel.
fn bundled_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    first_dir(&bundled_candidates(
        std::env::var_os("BEZEL_THEMES_DIR").map(PathBuf::from),
        exe.as_deref(),
        data_home(|name| std::env::var_os(name)),
    ))
}

/// A renderer with the bundled fonts (and those next to the theme) first,
/// then the installed ones.
fn renderer_for(cli: &Cli, bundled: Option<&Path>) -> SkiaRenderer {
    let fonts: Vec<Vec<u8>> = cli
        .command
        .theme()
        .and_then(|arg| resolve(arg, bundled).ok())
        .map(|path| font_dirs(&path, bundled))
        .unwrap_or_default()
        .iter()
        .flat_map(|dir| font_files(dir))
        .collect();
    SkiaRenderer::with_fonts(fonts, SystemFonts::Load)
}

/// `bezel render`, `bezel run` and `bezel import`.
fn themes(cli: &Cli) -> anyhow::Result<String> {
    let bundled = bundled_dir();
    let mut renderer = renderer_for(cli, bundled.as_deref());
    let mut sensors = sensor_source(cli.fake);
    let stop = Arc::new(AtomicBool::new(false));
    if matches!(cli.command, Command::Run { .. }) {
        let flag = Arc::clone(&stop);
        ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst))?;
    }
    let mut pace = SleepPace::new(stop);
    let mut log = std::io::stderr();
    let mut media = FfmpegTranscoder::new(cli.command.ffmpeg().map(Path::to_path_buf));
    let mut kit = Rendering {
        store: &FsThemeStore,
        renderer: &mut renderer,
        sensors: sensors.as_mut(),
        clock: &clock::now,
        language: clock::language(),
        bundled: bundled.as_deref(),
    };
    let result = if cli.fake {
        let (bus, connector) = (fake_bus(), fake_connector());
        run_theme_command(
            cli, &bus, &connector, &mut kit, &mut media, &mut pace, &mut log,
        )
    } else {
        run_theme_command(
            cli,
            &SystemBus,
            &SystemConnector,
            &mut kit,
            &mut media,
            &mut pace,
            &mut log,
        )
    };
    for problem in renderer.problems() {
        eprintln!("bezel: warning: {problem}");
    }
    result
}

/// `bezel storage`. Ctrl+C during an upload cancels it through the job's
/// token (the upload stops at its next block and says what is left); a
/// second Ctrl+C quits at once.
fn storage(args: &StorageArgs, fake: bool) -> anyhow::Result<String> {
    let mut media = FfmpegTranscoder::new(args.ffmpeg().map(Path::to_path_buf));
    let cancel = CancelToken::new();
    if args.cancellable() {
        let token = cancel.clone();
        ctrlc::set_handler(move || {
            if token.is_cancelled() {
                std::process::exit(130);
            }
            token.cancel();
            eprintln!("\nbezel: cancelling the upload... (Ctrl+C again quits at once)");
        })?;
    }
    let mut log = std::io::stderr();
    let progress = if log.is_terminal() {
        ProgressStyle::Bar
    } else {
        ProgressStyle::Lines
    };
    let mut kit = StorageKit {
        media: &mut media,
        cancel: &cancel,
        progress,
        log: &mut log,
    };
    if fake {
        run_storage_command(args, &fake_bus(), &fake_connector(), &mut kit)
    } else {
        run_storage_command(args, &SystemBus, &SystemConnector, &mut kit)
    }
}

/// `bezel monitor-mode`: the real HID stack, or a simulated panel.
fn monitor_mode(cli: &Cli) -> anyhow::Result<String> {
    let mut log = std::io::stderr();
    if cli.fake {
        let hid = FakeHid::answering(FAKE_DESKTOP_MODEL);
        run_monitor_mode(cli, &fake_bus(), &hid, &mut log)
    } else {
        run_monitor_mode(cli, &SystemBus, &SystemHid, &mut log)
    }
}

/// `bezel udev-rules`: the install command names the program the way the
/// user started it.
fn print_udev_rules() -> anyhow::Result<String> {
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "bezel".to_string());
    udev_rules::run(&program, &mut std::io::stderr())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                "bezel=debug,bezel_cli=debug,bezel_devices=debug,bezel_core=debug,bezel_sensors=debug,bezel_render=debug,bezel_themes=debug,bezel_media=debug",
            )
            .with_writer(std::io::stderr)
            .init();
    }
    let result = match &cli.command {
        Command::Sensors(args) => sensors(args, cli.fake),
        Command::Render { .. } | Command::Run { .. } | Command::Import { .. } => themes(&cli),
        Command::Storage(args) => storage(args, cli.fake),
        Command::MonitorMode { .. } => monitor_mode(&cli),
        Command::UdevRules => print_udev_rules(),
        _ if cli.fake => run(
            &cli,
            &fake_bus(),
            &fake_connector(),
            &mut SkiaRenderer::new(),
        ),
        _ => run(&cli, &SystemBus, &SystemConnector, &mut SkiaRenderer::new()),
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
            // On Linux a denied device is fixed by the udev rule.
            if cfg!(target_os = "linux")
                && let Some(hint) = udev_rules::access_hint(&e)
            {
                eprintln!("{hint}");
            }
            ExitCode::FAILURE
        }
    }
}
