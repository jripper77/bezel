//! `bezel`: command-line control of USB smart screens. Composition root.
#![forbid(unsafe_code)]

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use bezel_cli::storage::demo;
use bezel_cli::theme::{
    bundled_candidates, data_home, first_dir, font_dirs, resolve, theme_folders, theme_videos,
};
use bezel_cli::{
    Cli, Command, ProgressStyle, Rendering, SensorSettings, SensorsArgs, SleepPace, StandbyArgs,
    StandbyKit, StorageArgs, StorageKit, WatchStyle, clock, hang_hint, run, run_monitor_mode,
    run_restart, run_sensors, run_standby_command, run_storage_command, run_theme_command,
    udev_rules,
};
use bezel_core::domain::job::CancelToken;
use bezel_core::ports::{ArchiveStore, SensorSource};
use bezel_devices::{FakeBus, FakeConnector, FakeHid, SystemBus, SystemConnector, SystemHid};
use bezel_media::FfmpegTranscoder;
use bezel_media::archive::{DiskArchive, MemoryArchive, storage_dir};
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::{FakeSensors, SensorOptions, SystemSensors};
use bezel_themes::FsThemeStore;
use clap::Parser;

#[cfg(windows)]
mod light_tray;

/// The simulated bus of `--fake`: a Turing 8.8" and a Turing USB panel in
/// desktop mode.
fn fake_bus() -> FakeBus {
    FakeBus::turing_88().and(FakeBus::desktop_mode())
}

/// The model byte the simulated panel in desktop mode answers: an 8.8".
const FAKE_DESKTOP_MODEL: u8 = 0x88;

/// The simulated screen of `--fake`: a Turing 8.8" with a few files Bezel
/// sent and a memory card the vendor app filled (`storage::demo`).
fn fake_connector() -> FakeConnector {
    FakeConnector::with_storage(demo::storage())
}

/// The user's data folder, as the studio resolves it.
fn user_data() -> Option<PathBuf> {
    data_home(|name| std::env::var_os(name))
}

/// The machine's sensors (the demo ones with `--fake`), with the ping host
/// and MangoHud folder of `settings` when the command takes them.
fn sensor_source(fake: bool, settings: Option<&SensorSettings>) -> Box<dyn SensorSource> {
    if fake {
        return Box::new(FakeSensors::demo());
    }
    let mut options = SensorOptions::default();
    if let Some(settings) = settings {
        if let Some(host) = &settings.ping_host {
            options.ping_host.clone_from(host);
        }
        options.mangohud_dir.clone_from(&settings.mangohud_dir);
    }
    Box::new(SystemSensors::with_options(options))
}

/// `bezel sensors`, streamed straight to stdout.
fn sensors(args: &SensorsArgs, settings: &SensorSettings, fake: bool) -> anyhow::Result<String> {
    let mut source = sensor_source(fake, Some(settings));
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
        user_data(),
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
    if matches!(cli.command, Command::Run { tray: true, .. }) {
        #[cfg(windows)]
        return light_tray::run(std::env::args_os().collect());
        #[cfg(not(windows))]
        anyhow::bail!("--tray is currently supported on Windows only");
    }
    let stop = Arc::new(AtomicBool::new(false));
    if matches!(cli.command, Command::Run { .. }) {
        let flag = Arc::clone(&stop);
        ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst))?;
    }
    themes_with_stop(cli, stop)
}

fn themes_with_stop(cli: &Cli, stop: Arc<AtomicBool>) -> anyhow::Result<String> {
    themes_with_shutdown(cli, stop, Arc::default())
}

fn themes_with_shutdown(
    cli: &Cli,
    stop: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<String> {
    if let Command::Run {
        screens_config: Some(path),
        ..
    } = &cli.command
    {
        return multi_screens(cli, path, stop, shutdown);
    }
    let sensors = sensor_source(cli.fake, cli.command.sensor_settings());
    themes_with_source(cli, stop, shutdown, sensors)
}

fn multi_screens(
    cli: &Cli,
    path: &Path,
    stop: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<String> {
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let keys = saved["liveScreens"].as_array().cloned().unwrap_or_default();
    if keys.is_empty() {
        return themes_with_source(
            cli,
            stop,
            shutdown,
            sensor_source(cli.fake, cli.command.sensor_settings()),
        );
    }
    let Command::Run {
        frames,
        ffmpeg,
        settings,
        ..
    } = &cli.command
    else {
        anyhow::bail!("not a live command")
    };
    let shared = bezel_sensors::SharedSensors::new(sensor_source(cli.fake, Some(settings)));
    let mut commands = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for key in keys.iter().filter_map(|k| k.as_str()) {
        if !seen.insert(key.to_string()) {
            continue;
        }
        let theme = saved["screenThemes"][key].as_str().or_else(|| {
            (saved["liveScreen"].as_str() == Some(key))
                .then(|| saved["lastTheme"].as_str())
                .flatten()
        });
        let Some(theme) = theme else {
            eprintln!("bezel light: {key} has no saved theme; skipped");
            continue;
        };
        if !Path::new(theme).exists() {
            eprintln!("bezel light: saved theme for {key} is missing; skipped");
            continue;
        }
        commands.push(Cli {
            fake: cli.fake,
            verbose: cli.verbose,
            command: Command::Run {
                tray: false,
                screens_config: None,
                target: bezel_cli::Target {
                    screen: Some(key.into()),
                },
                theme: theme.into(),
                frames: *frames,
                ffmpeg: ffmpeg.clone(),
                settings: settings.clone(),
            },
        });
    }
    anyhow::ensure!(
        !commands.is_empty(),
        "No saved themes for the configured live screens"
    );
    std::thread::scope(|scope| -> anyhow::Result<String> {
        let mut workers = Vec::new();
        for command in commands {
            let sensors = shared.clone();
            let stop = Arc::clone(&stop);
            let shutdown = Arc::clone(&shutdown);
            workers.push(scope.spawn(move || {
                loop {
                    let result = themes_with_source(
                        &command,
                        Arc::clone(&stop),
                        Arc::clone(&shutdown),
                        Box::new(sensors.clone()),
                    );
                    if result.is_ok() || frames.is_some() || stop.load(Ordering::Relaxed) {
                        break result;
                    }
                    if let Err(error) = &result {
                        eprintln!("bezel light: screen worker will reconnect: {error:#}");
                    }
                    for _ in 0..20 {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }));
        }
        let mut summaries = String::new();
        for worker in workers {
            match worker.join() {
                Ok(Ok(summary)) => summaries.push_str(&summary),
                Ok(Err(error)) => eprintln!("bezel light: screen worker failed: {error:#}"),
                Err(_) => eprintln!("bezel light: screen worker panicked"),
            }
        }
        Ok(summaries)
    })
}

fn themes_with_source(
    cli: &Cli,
    stop: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    mut sensors: Box<dyn SensorSource>,
) -> anyhow::Result<String> {
    let bundled = bundled_dir();
    let mut renderer = renderer_for(cli, bundled.as_deref());
    let mut pace = SleepPace::with_shutdown(stop, shutdown);
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

/// A token that the first Ctrl+C cancels, saying `note` on stderr; a second
/// Ctrl+C quits at once. Without a note Ctrl+C keeps ending the program.
fn cancel_on_ctrl_c(note: Option<&'static str>) -> anyhow::Result<CancelToken> {
    let cancel = CancelToken::new();
    if let Some(note) = note {
        let token = cancel.clone();
        ctrlc::set_handler(move || {
            if token.is_cancelled() {
                std::process::exit(130);
            }
            token.cancel();
            eprintln!("\nbezel: {note} (Ctrl+C again quits at once)");
        })?;
    }
    Ok(cancel)
}

/// Bezel's catalog and local copies in `dir` (`<data>/bezel/storage`), the
/// ones the studio reads.
fn disk_archive(dir: Option<&Path>) -> anyhow::Result<DiskArchive> {
    let dir = dir.context(
        "cannot find your data folder for Bezel's local copies: set HOME or \
         XDG_DATA_HOME (APPDATA on Windows)",
    )?;
    DiskArchive::open(dir)
        .with_context(|| format!("cannot open Bezel's local copies in {}", dir.display()))
}

/// Seconds since the Unix epoch: when what is sent now is recorded as sent.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `bezel storage`. Ctrl+C during an upload or a batch cancels it through
/// the job's token (the upload stops at its next block and says what is
/// left; a batch stops there and keeps that file's source); a second Ctrl+C
/// quits at once. Bezel's catalog and local copies live in
/// `<data>/bezel/storage` (in memory, with the demo catalog, for `--fake`).
fn storage(args: &StorageArgs, fake: bool) -> anyhow::Result<String> {
    let mut media = FfmpegTranscoder::new(args.ffmpeg().map(Path::to_path_buf));
    let cancel = cancel_on_ctrl_c(args.cancel_note())?;
    let data = user_data();
    let videos = theme_videos(&theme_folders(data.as_deref(), bundled_dir().as_deref()));
    let dir = data.as_deref().map(storage_dir);
    let mut memory = if fake {
        demo::archive()
    } else {
        MemoryArchive::new()
    };
    let mut disk: DiskArchive;
    let archive: &mut dyn ArchiveStore = if fake || !args.uses_catalog() {
        &mut memory
    } else {
        disk = disk_archive(dir.as_deref())?;
        &mut disk
    };
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
        archive,
        archive_dir: if fake { None } else { dir.as_deref() },
        theme_videos: &videos,
        now: unix_now(),
    };
    if fake {
        run_storage_command(args, &fake_bus(), &fake_connector(), &mut kit)
    } else {
        run_storage_command(args, &SystemBus, &SystemConnector, &mut kit)
    }
}

/// `bezel standby`. The choice and the album's local copies go to Bezel's
/// catalog on disk, `<data>/bezel/storage`, the one Bezel Studio reads at
/// shutdown, also with `--fake` (only the screen is simulated); `set`
/// without `--yes` does not even open it. Ctrl+C during `album add`
/// cancels the upload.
fn standby(args: &StandbyArgs, fake: bool) -> anyhow::Result<String> {
    let cancel = cancel_on_ctrl_c(args.cancel_note())?;
    let dir = user_data().as_deref().map(storage_dir);
    let mut memory = MemoryArchive::new();
    let mut disk: DiskArchive;
    let archive: &mut dyn ArchiveStore = if args.uses_catalog() {
        disk = disk_archive(dir.as_deref())?;
        &mut disk
    } else {
        &mut memory
    };
    let mut media = FfmpegTranscoder::new(None);
    let mut log = std::io::stderr();
    let mut kit = StandbyKit {
        archive,
        archive_dir: dir.as_deref(),
        media: &mut media,
        cancel: &cancel,
        log: &mut log,
        now: unix_now(),
    };
    if fake {
        run_standby_command(args, &fake_bus(), &fake_connector(), &mut kit)
    } else {
        run_standby_command(args, &SystemBus, &SystemConnector, &mut kit)
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

/// `bezel restart`: the real screen, or the simulated one.
fn restart(cli: &Cli) -> anyhow::Result<String> {
    let mut log = std::io::stderr();
    if cli.fake {
        run_restart(cli, &fake_bus(), &fake_connector(), &mut log)
    } else {
        run_restart(cli, &SystemBus, &SystemConnector, &mut log)
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
        Command::Sensors { args, settings } => sensors(args, settings, cli.fake),
        Command::Render { .. } | Command::Run { .. } | Command::Import { .. } => themes(&cli),
        Command::Storage(args) => storage(args, cli.fake),
        Command::Standby(args) => standby(args, cli.fake),
        Command::MonitorMode { .. } => monitor_mode(&cli),
        Command::Restart { .. } => restart(&cli),
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
            // A screen that hung is restarted by the next command, or now.
            if let Some(hint) = hang_hint(&e) {
                eprintln!("{hint}");
            }
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
