//! `bezel run`: a theme on a screen with live sensors, one frame every
//! `refresh_seconds`, until Ctrl+C; then the screen goes back to its
//! standalone mode.
//!
//! A video background follows the core's `ThemeRuntime::start_video`
//! (D-2026-09-30-storage-video-4): a screen that stores the video loops it
//! under the theme; one that could but does not store it shows the poster
//! and the command says how to send it (`bezel storage put`); a screen that
//! cannot play videos gets them decoded on this computer (ffmpeg), at up to
//! [`HOST_VIDEO_FPS`] pictures per second.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use bezel_core::app::{HostVideo, MissingVideo, ThemeRuntime, VideoState, open_screen};
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::theme::{AssetRef, Background, Theme, refresh_interval};
use bezel_core::ports::{DeviceBus, MediaLocation, MediaTranscoder, ScreenConnector, ScreenLink};
use bezel_themes::native::{MANIFEST, safe_asset_path};

use crate::messages::Messages;
use crate::theme::{Loaded, describe, load, warning_lines};
use crate::{OrientationArg, Rendering, Target};

/// Slowest refresh, seconds (the fastest is the core's
/// `MIN_REFRESH_SECONDS`).
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

/// Seconds between frames for a theme's `refresh_seconds`, at most
/// [`MAX_REFRESH`].
pub fn interval(refresh_seconds: f32) -> Duration {
    refresh_interval(refresh_seconds, MAX_REFRESH)
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
    if let Some(panel) = theme.misfit(model.panel) {
        anyhow::bail!(
            "{} is made for a {}x{} canvas but the {} is {}x{} that way up; pick a theme for this screen",
            theme.name,
            theme.canvas.width,
            theme.canvas.height,
            model.name,
            panel.width,
            panel.height
        );
    }
    link.set_orientation(theme.orientation)?;
    Ok(link)
}

/// Pictures per second of a video background decoded on this computer (the
/// core's pace for screens that cannot play videos themselves).
pub use bezel_core::app::HOST_VIDEO_FPS;

/// The theme's video file on this computer, for host decoding: the file in
/// a theme folder, or a temporary copy of the asset (zipped and converted
/// themes), deleted when dropped.
struct HostSource {
    location: MediaLocation,
    temporary: Option<PathBuf>,
}

impl Drop for HostSource {
    fn drop(&mut self) {
        // Best effort: a copy left in the temporary folder harms nothing,
        // and the run already ended.
        if let Some(file) = &self.temporary {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// The folder of a theme laid out as files (`theme.json` beside its assets).
fn theme_folder(theme: &Path) -> Option<PathBuf> {
    if theme.is_dir() {
        return Some(theme.to_path_buf());
    }
    let manifest = theme.file_name().is_some_and(|n| n == MANIFEST);
    manifest.then(|| theme.parent().unwrap_or(Path::new(".")).to_path_buf())
}

/// `asset` as a file of the theme folder, when the theme is one and has it.
fn asset_file(theme: &Path, asset: &AssetRef) -> Option<PathBuf> {
    let relative = safe_asset_path(asset).ok()?;
    let file = theme_folder(theme)?.join(relative);
    file.is_file().then_some(file)
}

/// The video to decode on this computer, only for a screen that cannot play
/// it itself and a theme with a video background.
fn host_source(theme: &Path, loaded: &Loaded, model: &DeviceModel) -> Option<HostSource> {
    let Background::Video { asset, .. } = &loaded.theme.background else {
        return None;
    };
    if model.capabilities.video_playback {
        return None;
    }
    if let Some(file) = asset_file(theme, asset) {
        let location = MediaLocation(file.to_string_lossy().into_owned());
        return Some(HostSource {
            location,
            temporary: None,
        });
    }
    let bytes = loaded.assets.get(asset)?;
    let name = Path::new(&asset.0).file_name()?.to_string_lossy();
    let file = std::env::temp_dir().join(format!("bezel-video-{}-{name}", std::process::id()));
    std::fs::write(&file, bytes).ok()?;
    Some(HostSource {
        location: MediaLocation(file.to_string_lossy().into_owned()),
        temporary: Some(file),
    })
}

/// The command that puts a missing theme video on the screen.
fn put_hint(theme: &Path, runtime: &ThemeRuntime, missing: &MissingVideo) -> String {
    let orientation = OrientationArg::from(runtime.theme().orientation).cli_name();
    let put = |file: &str| {
        format!(
            "  bezel storage put {file} {} --orientation {orientation}",
            missing.path
        )
    };
    match asset_file(theme, &missing.asset) {
        Some(file) => put(&quoted(&file.to_string_lossy())),
        None => format!(
            "  bezel import {} -o <FOLDER>   # the theme as a folder, to reach its video\n{}",
            quoted(&theme.to_string_lossy()),
            put(&format!("<FOLDER>/{}", missing.asset.0))
        ),
    }
}

/// A path as the platform's shell reads it back: POSIX sh, or PowerShell on
/// Windows (where `\` is plain and a quote inside quotes is doubled).
fn quoted(text: &str) -> String {
    let plain = if cfg!(windows) { "/._-+:\\" } else { "/._-+:" };
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || plain.contains(c))
    {
        text.to_string()
    } else if cfg!(windows) {
        format!("'{}'", text.replace('\'', "''"))
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

/// What `bezel run` says about the video background once it started.
fn video_line(state: &VideoState, theme: &Path, runtime: &ThemeRuntime) -> Option<String> {
    let line = match state {
        VideoState::NoVideo | VideoState::NotStarted => return None,
        VideoState::OnDevice(path) => format!("the screen plays {path} under the theme"),
        VideoState::VideoMissing(missing) => format!(
            "warning: the screen does not store this theme's video yet, so its poster shows. \
             Send it, then run the theme again:\n{}",
            put_hint(theme, runtime, missing)
        ),
        VideoState::Host => format!(
            "the video is decoded on this computer, up to {HOST_VIDEO_FPS} pictures per second"
        ),
        VideoState::NoConverter { install_hints } => format!(
            "warning: its poster shows: decoding the video on this computer needs ffmpeg. \
             Install it with: {} (or pass --ffmpeg PATH)",
            install_hints.join(" ; ")
        ),
        VideoState::NoPlayback => {
            "warning: its poster shows: the theme's video could not be read on this computer"
                .to_string()
        }
    };
    Some(line)
}

/// Starts the theme's video background on the screen (queries and plays
/// stored files, never uploads) and says how it is shown; on failure the
/// poster stays.
fn start_video(
    runtime: &mut ThemeRuntime,
    link: &mut dyn ScreenLink,
    media: &mut dyn MediaTranscoder,
    source: Option<&HostSource>,
    theme: &Path,
    log: &mut Messages<'_>,
) {
    if matches!(runtime.video(), VideoState::NoVideo) {
        return;
    }
    let host = source.map(|s| HostVideo {
        media,
        source: s.location.clone(),
        fps: HOST_VIDEO_FPS,
    });
    let line = match runtime.start_video(link, host) {
        Ok(state) => video_line(&state.clone(), theme, runtime),
        Err(e) => Some(format!(
            "warning: cannot start the theme's video ({e}); its poster shows"
        )),
    };
    if let Some(line) = line {
        writeln!(log, "{line}");
    }
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

/// [`show_frames`] for a video decoded on this computer: a frame every
/// `1 / HOST_VIDEO_FPS`, drawn over the video's picture of that moment, with
/// the sensors sampled every `every`.
fn stream_frames(
    runtime: &mut ThemeRuntime,
    kit: &mut Rendering<'_>,
    link: &mut dyn ScreenLink,
    pace: &mut dyn Pace,
    (every, max_frames): (Duration, Option<u64>),
) -> (u64, bezel_core::Result<()>) {
    let tick = Duration::from_secs(1) / HOST_VIDEO_FPS;
    let started = Instant::now();
    let mut next_sample = started;
    let mut frames = 0u64;
    while !pace.stopped() {
        let now = Instant::now();
        if now >= next_sample {
            if let Err(e) = runtime.sample(kit.sensors) {
                return (frames, Err(e));
            }
            next_sample = now + every;
        }
        let shown = runtime
            .render(kit.renderer, (kit.clock)(), now - started)
            .and_then(|frame| link.present(&frame));
        if let Err(e) = shown {
            return (frames, Err(e));
        }
        frames += 1;
        if max_frames.is_some_and(|max| frames >= max) {
            break;
        }
        pace.wait((now + tick).saturating_duration_since(Instant::now()));
    }
    (frames, Ok(()))
}

/// `bezel run`: shows the requested theme on the screen until `pace` says
/// stop (or after `max_frames`), then releases the screen. A video
/// background starts once the screen is open (`media` decodes it for screens
/// that cannot play it). Progress goes to `log`; the returned text sums the
/// run up. When `log` cannot be written, no frame is shown: the screen is
/// released and that is the error.
pub fn run<B, C>(
    bus: &B,
    connector: &C,
    kit: &mut Rendering<'_>,
    request: RunRequest<'_>,
    media: &mut dyn MediaTranscoder,
    pace: &mut dyn Pace,
    log: &mut dyn Write,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let mut log = Messages::new(log);
    let loaded = load(kit.store, request.theme)?;
    write!(log, "{}", warning_lines(&loaded.warnings));
    log.check()?;
    // Rates and usages need a first sample; take it while the screen wakes.
    kit.sensors.sample().context("cannot read the sensors")?;
    let mut link = open_for(bus, connector, request.target, &loaded.theme)?;
    let screen = link.identity().model.name;
    let every = interval(loaded.theme.refresh_seconds);
    let line = describe(&loaded.theme);
    writeln!(
        log,
        "{screen}: showing {line}, every {:.2} s; Ctrl+C to stop",
        every.as_secs_f64()
    );
    let source = host_source(request.theme, &loaded, link.identity().model);
    let mut runtime = ThemeRuntime::new(loaded.theme, loaded.assets, kit.language);
    start_video(
        &mut runtime,
        link.as_mut(),
        media,
        source.as_ref(),
        request.theme,
        &mut log,
    );
    let started = Instant::now();
    let pacing = (every, request.max_frames);
    let (frames, outcome) = if log.failed() {
        (0, Ok(()))
    } else if matches!(runtime.video(), VideoState::Host) {
        stream_frames(&mut runtime, kit, link.as_mut(), pace, pacing)
    } else {
        show_frames(&mut runtime, kit, link.as_mut(), pace, pacing)
    };
    drop(source);
    // Hand the screen back even when a frame failed.
    let released = link.release();
    outcome.with_context(|| format!("stopped after {frames} frames"))?;
    released.context("could not hand the screen back")?;
    log.check()?;
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
    use bezel_core::domain::storage::{RemotePath, Repeat};
    use bezel_devices::fake::{FakeStorage, Playback};

    use crate::storage::doubles::{STREAMED, StubMedia, weact_bus};
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
        let screen = (FakeBus::turing_88(), connector.clone());
        let (out, log) = run_on(
            &screen,
            theme,
            sensors,
            pace,
            max_frames,
            &mut StubMedia::ready(),
        );
        (out, connector, log)
    }

    /// Runs `theme` on the screen of `bus` and `connector`.
    fn run_on(
        (bus, connector): &(FakeBus, FakeConnector),
        theme: &Path,
        sensors: &mut dyn SensorSource,
        pace: &mut dyn Pace,
        max_frames: Option<u64>,
        media: &mut StubMedia,
    ) -> (anyhow::Result<String>, String) {
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
        let out = run(bus, connector, &mut kit, request, media, pace, &mut log);
        (out, String::from_utf8(log).unwrap())
    }

    /// Saves a theme with a video background (`assets/clip.mp4`) for a panel
    /// of `size`, as a folder or (`zipped`) a `.bezeltheme` file.
    fn video_theme(name: &str, size: Size, orientation: Orientation, zipped: bool) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bezel-live-{}-{name}", std::process::id()));
        let path = if zipped {
            dir.with_extension("bezeltheme")
        } else {
            dir
        };
        let mut theme = Theme::blank(name, size, orientation);
        let clip = AssetRef("assets/clip.mp4".into());
        theme.background = Background::Video {
            asset: clip.clone(),
            poster: None,
        };
        let assets = BTreeMap::from([(clip, vec![1, 2, 3])]);
        FsThemeStore
            .save(&ThemeLocation(path.display().to_string()), &theme, &assets)
            .unwrap();
        path
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
    fn a_status_that_cannot_be_written_shows_no_frame_and_releases_the_screen() {
        let path = theme_file("mute", Size::new(480, 1920), Orientation::Portrait, 1.0);
        let connector = FakeConnector::default();
        let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
        let mut kit = Rendering {
            store: &FsThemeStore,
            renderer: &mut renderer,
            sensors: &mut FakeSensors::demo(),
            clock: &now,
            language: Language::English,
            bundled: None,
        };
        let target = Target { screen: None };
        let request = RunRequest {
            target: &target,
            theme: &path,
            max_frames: None,
        };
        let mut closed = crate::messages::tests::Closing::after(0);
        let out = run(
            &FakeBus::turing_88(),
            &connector,
            &mut kit,
            request,
            &mut StubMedia::ready(),
            &mut scripted(usize::MAX),
            &mut closed,
        );
        assert_eq!(
            format!("{:#}", out.unwrap_err()),
            "could not write to the terminal (stderr): broken pipe"
        );
        let screen = connector.log();
        assert_eq!((screen.frames.len(), screen.releases), (0, 1));
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
    fn a_video_the_screen_stores_loops_under_the_theme() {
        let path = video_theme("stored", Size::new(480, 1920), Orientation::Portrait, false);
        // Vertical on a panel mounted upside down: the copy turned half a turn.
        let stored = RemotePath::parse("internal/video/clip_180.mp4").unwrap();
        let connector = FakeConnector::with_storage(
            FakeStorage::default().with_file(stored.clone(), vec![9; 64]),
        );
        let screen = (FakeBus::turing_88(), connector.clone());
        let mut sensors = FakeSensors::demo();
        let (out, log) = run_on(
            &screen,
            &path,
            &mut sensors,
            &mut scripted(9),
            Some(2),
            &mut StubMedia::ready(),
        );
        assert!(out.unwrap().contains("2 frames"));
        assert!(
            log.contains("the screen plays internal/video/clip_180.mp4 under the theme"),
            "{log}"
        );
        let seen = connector.log();
        assert_eq!(seen.storage.playback, Playback::Video(stored, Repeat::Loop));
        assert_eq!(
            seen.frames[0].pixel(5, 5).map(|p| p.a),
            Some(0),
            "a transparent base"
        );
    }

    #[test]
    fn a_missing_video_shows_the_poster_and_how_to_send_it() {
        let folder = video_theme(
            "missing",
            Size::new(480, 1920),
            Orientation::Landscape,
            false,
        );
        let connector = FakeConnector::default();
        let screen = (FakeBus::turing_88(), connector.clone());
        let mut sensors = FakeSensors::demo();
        let (out, log) = run_on(
            &screen,
            &folder,
            &mut sensors,
            &mut scripted(9),
            Some(1),
            &mut StubMedia::ready(),
        );
        out.unwrap();
        let file = folder.join("assets/clip.mp4");
        assert!(
            log.contains("does not store this theme's video yet"),
            "{log}"
        );
        assert!(
            log.contains(&format!(
                "  bezel storage put {} internal/video/clip_90.mp4 --orientation horizontal",
                quoted(&file.to_string_lossy())
            )),
            "{log}"
        );
        assert!(
            connector
                .log()
                .storage
                .calls
                .iter()
                .all(|c| !c.changes_the_screen())
        );

        let zipped = video_theme(
            "missing",
            Size::new(480, 1920),
            Orientation::Landscape,
            true,
        );
        let (out, log) = run_on(
            &screen,
            &zipped,
            &mut sensors,
            &mut scripted(9),
            Some(1),
            &mut StubMedia::ready(),
        );
        out.unwrap();
        assert!(
            log.contains(&format!(
                "  bezel import {} -o <FOLDER>",
                quoted(&zipped.to_string_lossy())
            )),
            "{log}"
        );
        assert!(
            log.contains("put <FOLDER>/assets/clip.mp4 internal/video/clip_90.mp4"),
            "{log}"
        );
        let _ = std::fs::remove_dir_all(folder);
        let _ = std::fs::remove_file(zipped);
    }

    #[test]
    fn a_screen_without_playback_gets_the_video_decoded_here() {
        let screen = (weact_bus(), FakeConnector::default());
        let mut sensors = FakeSensors::demo();
        let folder = video_theme("host", Size::new(80, 160), Orientation::Portrait, false);
        let mut media = StubMedia::ready();
        let (out, log) = run_on(
            &screen,
            &folder,
            &mut sensors,
            &mut scripted(9),
            Some(3),
            &mut media,
        );
        assert!(out.unwrap().contains("3 frames"));
        assert!(
            log.contains("decoded on this computer, up to 10 pictures"),
            "{log}"
        );
        let (source, spec) = &media.streamed[0];
        assert_eq!(source.0, folder.join("assets/clip.mp4").to_string_lossy());
        assert_eq!((spec.size, spec.fps), (Size::new(80, 160), HOST_VIDEO_FPS));
        let frames = screen.1.log().frames;
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[2].pixel(40, 80), Some(STREAMED));

        // A zipped theme is decoded from a temporary copy, removed after.
        let zipped = video_theme("host", Size::new(80, 160), Orientation::Portrait, true);
        let mut media = StubMedia::ready();
        let (out, _) = run_on(
            &screen,
            &zipped,
            &mut sensors,
            &mut scripted(9),
            Some(1),
            &mut media,
        );
        out.unwrap();
        let copy = PathBuf::from(&media.streamed[0].0.0);
        assert!(copy.starts_with(std::env::temp_dir()), "{}", copy.display());
        assert!(!copy.exists(), "the copy is deleted");

        let (out, log) = run_on(
            &screen,
            &zipped,
            &mut sensors,
            &mut scripted(9),
            Some(1),
            &mut StubMedia::missing(),
        );
        out.unwrap();
        assert!(
            log.contains("needs ffmpeg. Install it with: sudo dnf install ffmpeg"),
            "{log}"
        );
        let _ = std::fs::remove_dir_all(folder);
        let _ = std::fs::remove_file(zipped);
    }

    #[test]
    fn paths_are_quoted_for_the_shell() {
        assert_eq!(quoted("/home/me/clip.mp4"), "/home/me/clip.mp4");
        assert_eq!(quoted("/home/me/my clip.mp4"), "'/home/me/my clip.mp4'");
        if cfg!(windows) {
            assert_eq!(quoted(r"C:\Videos\clip.mp4"), r"C:\Videos\clip.mp4");
            assert_eq!(quoted(r"C:\RUNNER~1\clip.mp4"), r"'C:\RUNNER~1\clip.mp4'");
            assert_eq!(quoted("it's.mp4"), "'it''s.mp4'");
        } else {
            assert_eq!(quoted(r"C:\Videos\clip.mp4"), r"'C:\Videos\clip.mp4'");
            assert_eq!(quoted("it's.mp4"), r"'it'\''s.mp4'");
        }
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
