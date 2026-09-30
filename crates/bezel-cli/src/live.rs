//! `bezel run`: a theme on a screen with live sensors, one frame every
//! `refresh_seconds`, until Ctrl+C; then the screen goes back to its
//! standalone mode.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use bezel_core::app::{ThemeRuntime, open_screen};
use bezel_core::domain::theme::Theme;
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};

use crate::theme::{describe, load, warning_lines};
use crate::{Rendering, Target};

/// Fastest refresh, seconds.
pub const MIN_REFRESH: f32 = 0.25;
/// Slowest refresh, seconds.
pub const MAX_REFRESH: f32 = 60.0;

/// The cadence of `bezel run`: waiting between frames and knowing when the
/// user asked to stop.
pub trait Pace {
    /// Waits `duration`, or less once a stop is asked.
    fn wait(&mut self, duration: Duration);
    /// True once the user asked to stop (Ctrl+C).
    fn stopped(&self) -> bool;
}

/// Sleeps for real, waking early when `stop` is set (by the Ctrl+C handler).
#[derive(Debug, Clone, Default)]
pub struct SleepPace {
    stop: Arc<AtomicBool>,
}

/// Longest sleep between two looks at the stop flag.
const SLICE: Duration = Duration::from_millis(50);

impl SleepPace {
    /// A pace that stops once `stop` is set.
    pub fn new(stop: Arc<AtomicBool>) -> Self {
        Self { stop }
    }
}

impl Pace for SleepPace {
    fn wait(&mut self, duration: Duration) {
        let until = Instant::now() + duration;
        while !self.stopped() {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(left.min(SLICE));
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

/// Seconds between frames for a theme's `refresh_seconds`.
pub fn interval(refresh_seconds: f32) -> Duration {
    let secs = if refresh_seconds.is_finite() {
        refresh_seconds.clamp(MIN_REFRESH, MAX_REFRESH)
    } else {
        1.0
    };
    Duration::from_secs_f32(secs)
}

/// What `bezel run` shows, and where.
#[derive(Debug, Clone, Copy)]
pub struct RunRequest<'a> {
    /// The screen.
    pub target: &'a Target,
    /// The theme file or folder.
    pub theme: &'a Path,
    /// Stop after this many frames (tests and demos); none = until stopped.
    pub max_frames: Option<u64>,
}

/// Opens the screen and turns it the theme's way up, refusing a theme made
/// for another panel.
fn open_for<B, C>(
    bus: &B,
    connector: &C,
    target: &Target,
    theme: &Theme,
) -> anyhow::Result<Box<dyn ScreenLink>>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let mut link = open_screen(bus, connector, target.screen.as_deref())
        .context("could not open the screen")?;
    let model = link.identity().model;
    let panel = model.panel.in_orientation(theme.orientation);
    anyhow::ensure!(
        panel == theme.canvas,
        "{} is made for a {}x{} canvas but the {} is {}x{} that way up; pick a theme for this screen",
        theme.name,
        theme.canvas.width,
        theme.canvas.height,
        model.name,
        panel.width,
        panel.height
    );
    link.set_orientation(theme.orientation)?;
    Ok(link)
}

/// Shows a frame every `every` until `pace` says stop or `max_frames` were
/// shown: how many were shown and how it ended.
fn show_frames(
    runtime: &mut ThemeRuntime,
    kit: &mut Rendering<'_>,
    link: &mut dyn ScreenLink,
    pace: &mut dyn Pace,
    (every, max_frames): (Duration, Option<u64>),
) -> (u64, bezel_core::Result<()>) {
    let mut frames = 0u64;
    while !pace.stopped() {
        let due = Instant::now() + every;
        if let Err(e) = runtime.show(kit.sensors, kit.renderer, link, (kit.clock)()) {
            return (frames, Err(e));
        }
        frames += 1;
        if max_frames.is_some_and(|max| frames >= max) {
            break;
        }
        pace.wait(due.saturating_duration_since(Instant::now()));
    }
    (frames, Ok(()))
}

/// `bezel run`: shows the requested theme on the screen until `pace` says
/// stop (or after `max_frames`), then releases the screen. Progress goes to
/// `log`; the returned text sums the run up.
pub fn run<B, C>(
    bus: &B,
    connector: &C,
    kit: &mut Rendering<'_>,
    request: RunRequest<'_>,
    pace: &mut dyn Pace,
    log: &mut dyn Write,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let loaded = load(kit.store, request.theme)?;
    let _ = write!(log, "{}", warning_lines(&loaded.warnings));
    // Rates and usages need a first sample; take it while the screen wakes.
    kit.sensors.sample().context("cannot read the sensors")?;
    let mut link = open_for(bus, connector, request.target, &loaded.theme)?;
    let screen = link.identity().model.name;
    let every = interval(loaded.theme.refresh_seconds);
    let line = describe(&loaded.theme);
    let _ = writeln!(
        log,
        "{screen}: showing {line}, every {:.2} s; Ctrl+C to stop",
        every.as_secs_f64()
    );
    let mut runtime = ThemeRuntime::new(loaded.theme, loaded.assets, kit.language);
    let started = Instant::now();
    let pacing = (every, request.max_frames);
    let (frames, outcome) = show_frames(&mut runtime, kit, link.as_mut(), pace, pacing);
    // Hand the screen back even when a frame failed.
    let released = link.release();
    outcome.with_context(|| format!("stopped after {frames} frames"))?;
    released.context("could not hand the screen back")?;
    Ok(format!(
        "{screen}: {frames} frames of {line} in {:.1} s, released\n",
        started.elapsed().as_secs_f64()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use bezel_core::domain::clock::{Language, LocalTime};
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_core::domain::sensor::{SensorInfo, Snapshot};
    use bezel_core::domain::theme::Theme;
    use bezel_core::ports::{SensorSource, ThemeLocation, ThemeStore};
    use bezel_core::{BezelError, Result};
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;
    use bezel_themes::FsThemeStore;

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 21,
        minute: 5,
        second: 0,
        weekday: 2,
    };

    fn now() -> LocalTime {
        TIME
    }

    /// Records the waits and asks to stop after `stop_after` of them.
    struct ScriptedPace {
        waits: Vec<Duration>,
        stop_after: usize,
    }

    impl Pace for ScriptedPace {
        fn wait(&mut self, duration: Duration) {
            self.waits.push(duration);
        }

        fn stopped(&self) -> bool {
            self.waits.len() >= self.stop_after
        }
    }

    fn scripted(stop_after: usize) -> ScriptedPace {
        ScriptedPace {
            waits: Vec::new(),
            stop_after,
        }
    }

    /// Sensors whose catalog works and whose samples fail after `ok` of them.
    struct FailingSensors {
        ok: usize,
    }

    impl SensorSource for FailingSensors {
        fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
            Ok(Vec::new())
        }

        fn sample(&mut self) -> Result<Snapshot> {
            if self.ok == 0 {
                return Err(BezelError::Transport("sensor bus gone".into()));
            }
            self.ok -= 1;
            Ok(Snapshot::default())
        }
    }

    /// Saves a blank theme for a panel of `size` (portrait) in `orientation`.
    fn theme_file(name: &str, size: Size, orientation: Orientation, refresh: f32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bezel-live-{}-{name}", std::process::id()));
        let mut theme = Theme::blank(name, size, orientation);
        theme.refresh_seconds = refresh;
        FsThemeStore
            .save(
                &ThemeLocation(dir.display().to_string()),
                &theme,
                &BTreeMap::new(),
            )
            .unwrap();
        dir
    }

    fn run_with(
        theme: &Path,
        sensors: &mut dyn SensorSource,
        pace: &mut dyn Pace,
        max_frames: Option<u64>,
    ) -> (anyhow::Result<String>, FakeConnector, String) {
        let connector = FakeConnector::default();
        let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
        let mut kit = Rendering {
            store: &FsThemeStore,
            renderer: &mut renderer,
            sensors,
            clock: &now,
            language: Language::English,
            bundled: None,
        };
        let target = Target { screen: None };
        let request = RunRequest {
            target: &target,
            theme,
            max_frames,
        };
        let mut log = Vec::new();
        let out = run(
            &FakeBus::turing_88(),
            &connector,
            &mut kit,
            request,
            pace,
            &mut log,
        );
        (out, connector, String::from_utf8(log).unwrap())
    }

    #[test]
    fn runs_frames_in_the_theme_orientation_then_releases() {
        let path = theme_file("wide", Size::new(480, 1920), Orientation::Landscape, 0.5);
        let mut sensors = FakeSensors::demo();
        let mut pace = scripted(usize::MAX);
        let (out, connector, log) = run_with(&path, &mut sensors, &mut pace, Some(3));
        let out = out.unwrap();
        assert!(
            out.contains("3 frames of wide (1920x480 horizontal)"),
            "{out}"
        );
        assert!(out.ends_with("released\n"), "{out}");
        assert!(log.contains("showing wide"), "{log}");
        assert!(log.contains("every 0.50 s"), "{log}");
        let screen = connector.log();
        assert_eq!(screen.orientations, vec![Orientation::Landscape]);
        assert_eq!(screen.frames.len(), 3);
        assert_eq!(screen.frames[0].size(), Size::new(1920, 480));
        assert_eq!(screen.releases, 1);
        // Two waits between three frames, each at most one refresh.
        assert_eq!(pace.waits.len(), 2);
        assert!(pace.waits.iter().all(|w| *w <= Duration::from_millis(500)));
        // A warm-up sample, then one per frame.
        assert_eq!(sensors.samples_taken(), 4);
    }

    #[test]
    fn ctrl_c_stops_the_loop_and_hands_the_screen_back() {
        let path = theme_file("tall", Size::new(480, 1920), Orientation::Portrait, 1.0);
        let mut sensors = FakeSensors::demo();
        let mut pace = scripted(1);
        let (out, connector, _) = run_with(&path, &mut sensors, &mut pace, None);
        assert!(out.unwrap().contains("1 frames"));
        let screen = connector.log();
        assert_eq!((screen.frames.len(), screen.releases), (1, 1));

        let mut stopped = scripted(0);
        let (out, connector, _) = run_with(&path, &mut sensors, &mut stopped, None);
        assert!(out.unwrap().contains("0 frames"));
        assert_eq!(connector.log().releases, 1);
    }

    #[test]
    fn a_theme_for_another_screen_is_refused() {
        let path = theme_file("small", Size::new(320, 480), Orientation::Portrait, 1.0);
        let mut sensors = FakeSensors::demo();
        let (out, connector, _) = run_with(&path, &mut sensors, &mut scripted(9), Some(1));
        let err = out.unwrap_err().to_string();
        assert!(err.contains("made for a 320x480 canvas"), "{err}");
        assert!(err.contains("480x1920"), "{err}");
        assert!(connector.log().frames.is_empty());
    }

    #[test]
    fn a_failing_frame_still_releases_the_screen() {
        let path = theme_file("fails", Size::new(480, 1920), Orientation::Portrait, 1.0);
        let mut sensors = FailingSensors { ok: 2 };
        let (out, connector, _) = run_with(&path, &mut sensors, &mut scripted(9), None);
        let err = format!("{:#}", out.unwrap_err());
        assert!(err.contains("stopped after 1 frames"), "{err}");
        assert!(err.contains("sensor bus gone"), "{err}");
        assert_eq!(connector.log().releases, 1);

        let mut broken = FailingSensors { ok: 0 };
        let (out, connector, _) = run_with(&path, &mut broken, &mut scripted(9), None);
        assert!(format!("{:#}", out.unwrap_err()).contains("cannot read the sensors"));
        assert!(
            connector.log().orientations.is_empty(),
            "screen never opened"
        );
    }

    #[test]
    fn intervals_are_clamped() {
        assert_eq!(interval(1.0), Duration::from_secs(1));
        assert_eq!(interval(0.01), Duration::from_millis(250));
        assert_eq!(interval(3600.0), Duration::from_secs(60));
        assert_eq!(interval(f32::NAN), Duration::from_secs(1));
    }

    #[test]
    fn sleep_pace_wakes_early_when_stopped() {
        let stop = Arc::new(AtomicBool::new(false));
        let mut pace = SleepPace::new(Arc::clone(&stop));
        let started = Instant::now();
        pace.wait(Duration::from_millis(20));
        assert!(started.elapsed() >= Duration::from_millis(20));
        assert!(!pace.stopped());
        stop.store(true, Ordering::SeqCst);
        let started = Instant::now();
        pace.wait(Duration::from_secs(30));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(pace.stopped());
    }
}
