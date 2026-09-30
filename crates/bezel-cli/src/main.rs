//! `bezel`: command-line control of USB smart screens. Composition root.
#![forbid(unsafe_code)]

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bezel_cli::theme::{bundled_candidates, data_home, first_dir, font_dirs, resolve};
use bezel_cli::{
    Cli, Command, Rendering, SensorsArgs, SleepPace, WatchStyle, clock, run, run_sensors,
    run_theme_command,
};
use bezel_core::ports::SensorSource;
use bezel_devices::{FakeBus, FakeConnector, SystemBus, SystemConnector};
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::{FakeSensors, SystemSensors};
use bezel_themes::FsThemeStore;
use clap::Parser;

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
    let mut kit = Rendering {
        store: &FsThemeStore,
        renderer: &mut renderer,
        sensors: sensors.as_mut(),
        clock: &clock::now,
        language: clock::language(),
        bundled: bundled.as_deref(),
    };
    let result = if cli.fake {
        let (bus, connector) = (FakeBus::turing_88(), FakeConnector::default());
        run_theme_command(cli, &bus, &connector, &mut kit, &mut pace, &mut log)
    } else {
        run_theme_command(
            cli,
            &SystemBus,
            &SystemConnector,
            &mut kit,
            &mut pace,
            &mut log,
        )
    };
    for problem in renderer.problems() {
        eprintln!("bezel: warning: {problem}");
    }
    result
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                "bezel=debug,bezel_devices=debug,bezel_core=debug,bezel_sensors=debug,bezel_render=debug,bezel_themes=debug",
            )
            .with_writer(std::io::stderr)
            .init();
    }
    let result = match &cli.command {
        Command::Sensors(args) => sensors(args, cli.fake),
        Command::Render { .. } | Command::Run { .. } | Command::Import { .. } => themes(&cli),
        _ if cli.fake => run(
            &cli,
            &FakeBus::turing_88(),
            &FakeConnector::default(),
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
            ExitCode::FAILURE
        }
    }
}
