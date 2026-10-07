//! The editing session behind the window: the core's [`ThemeRuntime`] with
//! the theme being edited, its assets, the latest readings and the graph
//! histories, and the screen showing it live. It is the same runtime as
//! `bezel run` (D-2026-09-30-studio-app-4): every refresh samples through
//! it, the live screen gets what it renders, and every committed edit swaps
//! the theme in place with the histories kept. Everything goes through the
//! core's ports, so the whole session runs on fakes in tests.
//!
//! A theme with a video background (D-2026-09-30-storage-video-4): the live
//! screen gets what the runtime's [`ThemeRuntime::start_video`] chose: the
//! stored video looping under overlays, the video decoded on this computer
//! for screens that cannot play videos (a copy of the asset is decoded by the
//! media converter the storage tab shares), or the poster
//! ([`VideoState::VideoMissing`] carries what sending it takes).
//!
//! The video is framed (D-2026-10-01-video-background-framing-2 to -5): a
//! copy of it is probed once (an MP4's header needs no ffmpeg) and what it
//! said goes to the runtime, which decides Auto, the copy a screen looks for
//! and how each decoded picture is framed. The preview plays the video when
//! motion is allowed ([`Motion`]): one decoder per session reads the raw
//! source at most [`PREVIEW_FPS`] pictures a second, the runtime frames each
//! picture as the theme says now (a framing edit never restarts it, another
//! video does), and it ends when no picture is asked for during
//! [`PREVIEW_IDLE`], to resume from the clock at the next one. A decoder that
//! fails is tried again after [`PREVIEW_RETRY`], by itself up to
//! [`PREVIEW_ATTEMPTS`] in a row. Without ffmpeg, with motion reduced or a
//! hidden window, the preview shows the poster. The poster is taken again on
//! save when the theme frames its video otherwise than the poster shows.
//!
//! Probing the video and taking a poster run external programs (ffprobe for
//! a container that is not MP4 or GIF, ffmpeg for a poster): the session
//! hands them out ([`VideoProbe`], [`PosterRetake`]) so the caller runs them
//! outside its lock, and takes their outcome back only for the video, poster
//! and framing they were for. Until the video is probed the preview shows
//! the poster and the live screen does not start it.
//!
//! The sensors measure what is shown (D-2026-09-30-release-polish-11): the
//! theme, which the preview always shows, and the sensors the library's
//! list shows ([`Studio::show_sensors`]); `net.ping` sends packets only
//! while one of them uses it.
//!
//! The screen's I/O happens outside the session: a frame is rendered in the
//! session, then the live link leaves it with the frame ([`Delivery`]) and
//! comes back once the screen showed it ([`Studio::presented`]). Previews
//! render meanwhile; whoever needs the link waits for it
//! ([`Studio::presenting`]).
//!
//! Frames come when the runtime says they are due (T-7.11): every refresh,
//! and at a visible animated GIF's frame times; the preview reports when its
//! GIFs change next so the UI can draw them too ([`Studio::preview`]). A
//! live link that fails (the screen stopped reading, a cable glitch) is
//! dropped and the screen connected again after 2, 5 and 10 s, outside the
//! session ([`Studio::reconnect_due`], [`Studio::reconnected`]); turning
//! live mode off meanwhile ends it at once.
//!
//! When the computer shuts down (D-2026-10-03-power-off-standby-3) the
//! session enters a final state ([`Studio::enter_final_state`]) where no
//! frame is drawn, no link is lent or connected again and no screen goes
//! live; the shutdown takes the live link to apply the screen's choice
//! through it ([`Studio::take_for_shutdown`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use bezel_core::app::{HOST_VIDEO_FPS, HostVideo, MissingVideo, ThemeRuntime, VideoState};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::discovery::Screen;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::framing::{PanelLayout, VideoFraming, auto_turns};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::media::{MediaInfo, MediaTools, PREVIEW_FPS};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::domain::reconnect::{Reconnect, worth_reconnecting};
use bezel_core::domain::screen::Brightness;
use bezel_core::domain::sensor::{Quantities, SensorInfo, Snapshot, Wanted};
use bezel_core::domain::theme::{AssetRef, Background, Theme, refresh_interval};
use bezel_core::ports::{
    FrameRenderer, MediaLocation, MediaTranscoder, ScreenLink, SensorSource, ThemeLocation,
    ThemeStore, VideoFrames,
};
use bezel_core::{BezelError, Result};

use crate::diag::{self, DiagCode};
use crate::media::png_of;
use crate::messages::{ErrorCode, UiError};
use crate::storage::MediaSetup;

/// Slowest refresh, seconds (sensors still update the UI this often; the
/// fastest is the core's `MIN_REFRESH_SECONDS`).
pub const MAX_REFRESH: f32 = 2.0;

/// The media converter the storage tab shares with the session.
pub type SharedMedia = Arc<Mutex<Box<dyn MediaSetup>>>;

/// Decoding a theme's video on this computer, for screens that cannot play
/// videos: the converter and where the copy of the video it reads goes.
struct HostDecoding {
    media: SharedMedia,
    dir: PathBuf,
}

/// A copy of the theme's video for the converter, removed when dropped.
struct VideoCopy(PathBuf);

impl VideoCopy {
    fn write(dir: &Path, asset: &AssetRef, bytes: &[u8]) -> std::io::Result<Self> {
        let name = asset.0.rsplit(['/', '\\']).next().unwrap_or("video");
        let file = dir.join(name);
        std::fs::create_dir_all(dir)?;
        std::fs::write(&file, bytes)?;
        Ok(Self(file))
    }
}

impl Drop for VideoCopy {
    fn drop(&mut self) {
        if std::fs::remove_file(&self.0).is_err() {
            diag::report(DiagCode::VideoCopyNotRemoved);
        }
    }
}

/// The video decoded on this computer for the live screen.
struct HostPlayback {
    /// The file the converter reads (kept while it plays).
    _copy: VideoCopy,
    started: Instant,
}

/// A preview decoder no picture is asked of for this long ends
/// (D-2026-10-01-video-background-framing-5): a hidden window, or motion
/// reduced, asks for none.
pub const PREVIEW_IDLE: Duration = Duration::from_secs(2);

/// After a preview decoder could not start or stopped (no ffmpeg, a file it
/// cannot read), the next attempt waits this long; the preview shows the
/// poster.
pub const PREVIEW_RETRY: Duration = Duration::from_secs(2);

/// Decoders of the same video that may fail in a row before the preview
/// stops asking for another by itself (the next render, after an edit, still
/// tries one).
pub const PREVIEW_ATTEMPTS: u32 = 3;

/// While a storage job holds the converter, the preview shows the poster
/// and asks again this soon.
pub(crate) const CONVERTER_BUSY: Duration = Duration::from_millis(500);

/// Folder (under the decoding folder) of the copy of the theme's video the
/// session probes and previews: apart from the live screen's copy, which
/// comes and goes with it.
const PREVIEW_DIR: &str = "preview";

/// Whether work on the media converter waits for a storage job that holds
/// it (a conversion takes minutes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// It waits: the caller needs the answer (Auto's angle).
    Yes,
    /// It does not: the preview shows the poster meanwhile and asks again
    /// soon, the live screen tries at its next frame, a save keeps the
    /// poster.
    No,
}

/// The media converter, unless a storage job holds it and `wait` says not
/// to wait for it.
fn converter(media: &SharedMedia, wait: Wait) -> Option<MutexGuard<'_, Box<dyn MediaSetup>>> {
    match wait {
        Wait::Yes => Some(media.lock().unwrap_or_else(PoisonError::into_inner)),
        Wait::No => match media.try_lock() {
            Ok(media) => Some(media),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        },
    }
}

/// Probing the theme's video, handed out of the session so that it runs
/// outside the session's lock ([`Studio::video_to_probe`]): the header of an
/// MP4 or a GIF is read in Rust, any other container by ffprobe.
pub struct VideoProbe {
    /// Which of the session's videos it is ([`ThemeVideo::serial`]).
    serial: u64,
    location: MediaLocation,
    media: SharedMedia,
}

impl VideoProbe {
    /// Probes the video; `None` when a storage job holds the converter and
    /// `wait` says not to wait for it. A probe that fails is said in the log
    /// and leaves the video unknown (Auto turns nothing).
    pub fn run(self, wait: Wait) -> Option<Probed> {
        let probed = converter(&self.media, wait)?.probe(&self.location);
        let info = probed
            .inspect_err(|_| diag::report(DiagCode::VideoNotProbed))
            .ok();
        Some(Probed {
            serial: self.serial,
            info,
        })
    }
}

/// What probing the theme's video said ([`Studio::probed`]).
pub struct Probed {
    serial: u64,
    info: Option<MediaInfo>,
}

/// Taking the video background's poster again, handed out of the session
/// so that ffmpeg runs outside the session's lock ([`Studio::poster_to_take`]).
pub struct PosterRetake {
    /// Which of the session's videos it is ([`ThemeVideo::serial`]).
    serial: u64,
    poster: AssetRef,
    spec: PosterSpec,
    location: MediaLocation,
    media: SharedMedia,
}

impl PosterRetake {
    /// Takes the poster, as a PNG; `None` without ffmpeg or while a storage
    /// job holds the converter (the poster stays as it is), or when it fails
    /// (said in the log).
    pub fn run(self) -> Option<TakenPoster> {
        let mut media = converter(&self.media, Wait::No)?;
        if let MediaTools::Missing { .. } = media.tools() {
            return None;
        }
        let taken = media.poster(&self.location, self.spec);
        drop(media);
        let frame = taken
            .inspect_err(|_| diag::report(DiagCode::PosterNotRetaken))
            .ok()?;
        Some(TakenPoster {
            serial: self.serial,
            poster: self.poster,
            spec: self.spec,
            png: png_of(&frame)?,
        })
    }
}

/// A poster taken again ([`Studio::poster_taken`]).
pub struct TakenPoster {
    serial: u64,
    poster: AssetRef,
    spec: PosterSpec,
    png: Vec<u8>,
}

/// Whether the preview may move (D-2026-10-01-video-background-framing-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// The window is shown and motion is not reduced: a video background
    /// plays.
    Allowed,
    /// Motion is reduced or the window is hidden: the poster, and no
    /// decoder.
    Reduced,
}

/// The preview's decoder of the theme's video.
struct PreviewDecoder {
    frames: Box<dyn VideoFrames>,
    /// When a picture was last asked of it ([`PREVIEW_IDLE`]).
    asked: Instant,
}

/// What a preview of a video background can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Playing {
    /// The decoder's pictures.
    Video,
    /// The poster: no video background, no motion, no ffmpeg, no decoder.
    Poster,
    /// The poster for now: a storage job holds the converter, or the video
    /// is not probed yet.
    Busy,
    /// The poster until another decoder is tried, this soon: the last one
    /// failed.
    Again(Duration),
}

/// The theme's video background as the session handles it on this computer:
/// a copy for the converter (probed once, read by the preview's decoder, the
/// source of a new poster) and the preview's decoder.
struct ThemeVideo {
    /// Tells this video from any other the session had, even one of the
    /// same name in another theme: work handed out for it comes back to it
    /// only.
    serial: u64,
    asset: AssetRef,
    /// The preview's decoder: at most one, for this video. Declared before
    /// `copy`, so it stops before its file goes.
    decoder: Option<PreviewDecoder>,
    /// The copy the converter reads, written when first needed.
    copy: Option<VideoCopy>,
    /// Whether the copy was probed (the runtime has what it said).
    probed: bool,
    /// When the preview started playing it: its pictures follow the clock
    /// from then on, whichever decoder shows them.
    epoch: Option<Instant>,
    /// No decoder starts before then (the last attempt failed).
    retry: Option<Instant>,
    /// Decoders that failed in a row ([`PREVIEW_ATTEMPTS`]).
    failures: u32,
}

impl ThemeVideo {
    fn new(asset: AssetRef, serial: u64) -> Self {
        Self {
            serial,
            asset,
            decoder: None,
            copy: None,
            probed: false,
            epoch: None,
            retry: None,
            failures: 0,
        }
    }

    /// The preview's decoder could not start, or stopped, at `now`: the
    /// next one waits [`PREVIEW_RETRY`].
    fn failed(&mut self, now: Instant) {
        self.decoder = None;
        self.failures = self.failures.saturating_add(1);
        self.retry = Some(now + PREVIEW_RETRY);
    }

    /// What the preview shows at `now` while no decoder may start: the
    /// poster, asked again when the next one is due after a failure (up to
    /// [`PREVIEW_ATTEMPTS`] failures in a row).
    fn waiting(&self, now: Instant) -> Playing {
        match self.retry {
            Some(at) if (1..PREVIEW_ATTEMPTS).contains(&self.failures) => {
                Playing::Again(at.saturating_duration_since(now))
            }
            _ => Playing::Poster,
        }
    }

    /// Where the converter reads the video: its copy in `dir`, written from
    /// `assets` the first time.
    fn file(&mut self, dir: &Path, assets: &BTreeMap<AssetRef, Vec<u8>>) -> Result<MediaLocation> {
        let copy = match self.copy.take() {
            Some(copy) => copy,
            None => {
                let bytes = assets.get(&self.asset).ok_or_else(|| {
                    BezelError::InvalidInput(format!("{} is not in the theme", self.asset.0))
                })?;
                let dir = dir.join(PREVIEW_DIR);
                VideoCopy::write(&dir, &self.asset, bytes)
                    .map_err(|e| BezelError::Transport(format!("{}: {e}", dir.display())))?
            }
        };
        let location = MediaLocation(copy.0.display().to_string());
        self.copy = Some(copy);
        Ok(location)
    }

    /// The time into the video its preview shows at `now`: since it started
    /// playing, within one pass when its length (`info`) is known, so a
    /// decoder started again seeks no further than one pass.
    fn elapsed(&mut self, now: Instant, info: Option<&MediaInfo>) -> Duration {
        let epoch = *self.epoch.get_or_insert(now);
        let elapsed = now.saturating_duration_since(epoch);
        let length = info
            .and_then(|info| info.video.as_ref())
            .and_then(|track| track.duration)
            .filter(|length| !length.is_zero());
        length.map_or(elapsed, |length| {
            let into = elapsed.as_nanos() % length.as_nanos();
            Duration::from_nanos(u64::try_from(into).unwrap_or(u64::MAX))
        })
    }
}

/// How long after the picture due `elapsed` into a video the next one is
/// due, on the [`PREVIEW_FPS`] grid: whole milliseconds, rounded up so the
/// next render meets the new picture.
fn next_picture(elapsed: Duration) -> Duration {
    let period = Duration::from_secs(1).as_nanos() / u128::from(PREVIEW_FPS);
    let left = period - elapsed.as_nanos() % period;
    let millis = left.div_ceil(1_000_000);
    Duration::from_millis(u64::try_from(millis).unwrap_or(u64::MAX))
}

/// The sooner of two times, either unknown.
fn sooner(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// A live screen whose link failed, being connected again.
struct Away {
    attempts: Reconnect,
    /// When the next attempt is due.
    due: Instant,
    /// What stopped the link: live mode stops with it when the screen does
    /// not come back.
    error: BezelError,
    /// An attempt is under way, outside the session.
    trying: bool,
}

/// Where the live screen's link is.
enum Slot {
    /// In the session.
    Here(Box<dyn ScreenLink>),
    /// Out showing a frame ([`Delivery`]).
    Presenting,
    /// Lent to a storage job ([`Studio::lend_live_link`]): frames pause
    /// until it comes back.
    Lent,
    /// Gone: it failed and the screen is being connected again.
    Away(Away),
    /// Taken by the shutdown, which applied the screen's choice through it
    /// ([`Studio::take_for_shutdown`]): the computer is shutting down.
    ShutDown,
}

impl Slot {
    fn link(&mut self) -> Option<&mut Box<dyn ScreenLink>> {
        match self {
            Slot::Here(link) => Some(link),
            Slot::Presenting | Slot::Lent | Slot::Away(_) | Slot::ShutDown => None,
        }
    }

    /// The link when it is here, leaving `next` in its place; nothing
    /// changes while it is out.
    fn take_for(&mut self, next: Slot) -> Option<Box<dyn ScreenLink>> {
        match std::mem::replace(self, next) {
            Slot::Here(link) => Some(link),
            out => {
                *self = out;
                None
            }
        }
    }
}

/// The screen showing the edited theme.
struct Live {
    key: String,
    /// The screen as discovered, to find it again after its link failed
    /// (`None`: it is not connected again).
    screen: Option<Screen>,
    /// Counts the times live mode started or stopped: an attempt to
    /// connect again that ends after it changed is dropped.
    generation: u64,
    slot: Slot,
    /// When its last frame was drawn.
    drawn: Option<Instant>,
    orientation: Option<Orientation>,
    /// Start the video (again) before the next frame: after going live, after
    /// a theme with another video, after a job that changed what plays.
    restart_video: bool,
    /// The video decoded here ([`VideoState::Host`]).
    host: Option<HostPlayback>,
}

impl Live {
    /// Whether `key` reaches this screen: the key it is live under, or
    /// either port of the screen as discovered (its display or its MCU,
    /// [`Screen::answers_to`]; D-2026-10-01-live-screen-controls-3).
    fn answers_to(&self, key: &str) -> bool {
        self.key == key || self.screen.as_ref().is_some_and(|s| s.answers_to(key))
    }
}

/// A frame on its way to the live screen with the screen's link, out of the
/// session so the screen's I/O does not hold it.
pub struct Delivery {
    key: String,
    link: Box<dyn ScreenLink>,
    frame: Frame,
    /// Turn the screen to this orientation first (the theme turned).
    turn: Option<Orientation>,
}

impl Delivery {
    /// Shows the frame on the screen.
    pub fn present(&mut self) -> Result<()> {
        if let Some(orientation) = self.turn {
            self.link.set_orientation(orientation)?;
        }
        self.link.present(&self.frame)
    }
}

/// An attempt to connect the live screen again, made outside the session
/// ([`Studio::reconnect_due`]); its outcome goes to [`Studio::reconnected`].
#[derive(Debug, Clone)]
pub struct Attempt {
    screen: Screen,
    generation: u64,
}

impl Attempt {
    /// The screen to find again and connect.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }
}

/// Where a live screen whose link failed stands, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconnecting {
    /// The attempt under way or next (1 for the first).
    pub attempt: usize,
    /// Attempts in all.
    pub attempts: usize,
}

/// What the shutdown finds in the live screen's place
/// ([`Studio::take_for_shutdown`]).
pub enum ForShutdown {
    /// No live link to apply the choice through: no screen is live, or its
    /// link failed and no attempt to connect it again is under way.
    Nothing,
    /// The live link is out (showing a frame, lent to a storage job, or
    /// being connected again): it comes back soon, ask again.
    Out,
    /// The live link, for the shutdown to apply the screen's choice
    /// through it (its screen's model in its identity).
    Link(Box<dyn ScreenLink>),
}

/// What a borrowed live link resumes when it comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    /// Frames only: the job only asked questions.
    Frames,
    /// Frames, and the theme's video started again: the job may have changed
    /// what the screen plays (an upload stops playback first).
    Video,
}

/// One editing session.
pub struct Studio {
    shared_sensors: bezel_sensors::SharedSensors,
    shared_renderer: SharedRenderer,
    serial_counter: Arc<std::sync::atomic::AtomicU64>,
    selected_screen: Option<String>,
    documents: BTreeMap<String, Studio>,
    sensors: Box<dyn SensorSource>,
    renderer: Box<dyn FrameRenderer>,
    language: Language,
    runtime: ThemeRuntime,
    catalog: Vec<SensorInfo>,
    /// The sensors the library's list shows ([`Self::show_sensors`]).
    listed: Wanted,
    sample_millis: f64,
    location: Option<ThemeLocation>,
    live: Option<Live>,
    /// Counts the times live mode started or stopped.
    generation: u64,
    live_error: Option<UiError>,
    host: Option<HostDecoding>,
    /// What the session learnt about the videos added to it.
    videos: BTreeMap<AssetRef, AddedVideo>,
    /// The theme's video background on this computer (`None`: the theme
    /// has none).
    video: Option<ThemeVideo>,
    /// The framing each poster of the session shows (as loaded, added or
    /// last taken): a poster is taken again on save when the theme frames
    /// its video otherwise.
    posters: BTreeMap<AssetRef, VideoFraming>,
    /// The last [`ThemeVideo::serial`] given.
    serials: u64,
    /// The session's clock starts here: the runtime's cadence and the
    /// animations run on it.
    origin: Instant,
    /// The final state of a shutdown ([`Self::enter_final_state`]): no
    /// frame, no reconnection, no link lent, no screen going live.
    shutting_down: bool,
}

#[derive(Clone)]
struct SharedRenderer(Arc<Mutex<Box<dyn FrameRenderer>>>);
impl FrameRenderer for SharedRenderer {
    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: bezel_core::ports::RenderContext<'_>,
    ) -> Result<Frame> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .render(theme, assets, context)
    }
    fn animation(
        &mut self,
        asset: &AssetRef,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
    ) -> Option<bezel_core::domain::animation::Timeline> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .animation(asset, assets)
    }
}

/// A video (or an animated GIF) added to the session for a background: its
/// poster and how long it plays, as learnt when it was added.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddedVideo {
    /// The poster taken from it (none without ffmpeg).
    pub poster: Option<AssetRef>,
    /// How long it plays, when known.
    pub duration: Option<Duration>,
}

impl Studio {
    /// Switch the editor document without stopping any live screen.
    pub fn select_screen(&mut self, key: &str, blank: Theme) -> bool {
        if self.selected_screen.as_deref() == Some(key) {
            return false;
        }
        let Some(previous) = self.selected_screen.clone() else {
            self.selected_screen = Some(key.into());
            return false;
        };
        let existed = self.documents.contains_key(key);
        let mut next = self.documents.remove(key).unwrap_or_else(|| {
            let mut studio = Self::from_shared(
                self.shared_sensors.clone(),
                self.shared_renderer.clone(),
                self.language,
                blank,
            );
            studio.catalog = self.catalog.clone();
            studio.serial_counter = Arc::clone(&self.serial_counter);
            studio.runtime.use_catalog(&studio.catalog);
            if let Some(host) = &self.host {
                studio.host = Some(HostDecoding {
                    media: Arc::clone(&host.media),
                    dir: host
                        .dir
                        .join(format!("screen-{}", key.replace(['/', '\\', ':'], "_"))),
                });
            }
            studio
        });
        std::mem::swap(&mut self.sensors, &mut next.sensors);
        std::mem::swap(&mut self.runtime, &mut next.runtime);
        std::mem::swap(&mut self.listed, &mut next.listed);
        std::mem::swap(&mut self.sample_millis, &mut next.sample_millis);
        std::mem::swap(&mut self.location, &mut next.location);
        std::mem::swap(&mut self.live, &mut next.live);
        std::mem::swap(&mut self.generation, &mut next.generation);
        std::mem::swap(&mut self.live_error, &mut next.live_error);
        std::mem::swap(&mut self.host, &mut next.host);
        std::mem::swap(&mut self.videos, &mut next.videos);
        std::mem::swap(&mut self.video, &mut next.video);
        std::mem::swap(&mut self.posters, &mut next.posters);
        std::mem::swap(&mut self.serials, &mut next.serials);
        std::mem::swap(&mut self.origin, &mut next.origin);
        self.documents.insert(previous, next);
        for studio in self.documents.values_mut() {
            studio.show_sensors(Wanted::nothing());
            let wanted = if studio.any_live() {
                studio.wanted().clone()
            } else {
                Wanted::nothing()
            };
            studio.sensors.want(&wanted);
        }
        self.selected_screen = Some(key.into());
        !existed
    }

    pub fn selected_screen(&self) -> Option<&str> {
        self.selected_screen.as_deref()
    }
    pub fn any_live(&self) -> bool {
        self.live.is_some() || self.documents.values().any(Self::any_live)
    }
    pub fn stop_all(&mut self) {
        drop(self.stop_live());
        for studio in self.documents.values_mut() {
            studio.stop_all();
        }
    }
    pub fn reconnects_due(&mut self, now: Instant) -> Vec<(Option<String>, Attempt)> {
        let mut attempts = Vec::new();
        if let Some(a) = self.reconnect_due(now) {
            attempts.push((self.selected_screen.clone(), a));
        }
        for (key, studio) in &mut self.documents {
            if let Some(a) = studio.reconnect_due(now) {
                attempts.push((Some(key.clone()), a));
            }
        }
        attempts
    }
    pub fn reconnect_document(
        &mut self,
        key: Option<&str>,
        attempt: Attempt,
        outcome: Result<(Screen, Box<dyn ScreenLink>)>,
        now: Instant,
    ) -> Option<Box<dyn ScreenLink>> {
        if key == self.selected_screen.as_deref() {
            return self.reconnected(attempt, outcome, now);
        }
        if let Some(studio) = key.and_then(|k| self.documents.get_mut(k)) {
            return studio.reconnected(attempt, outcome, now);
        }
        outcome.ok().map(|(_, link)| link)
    }
    pub fn tick_all(&mut self, time: LocalTime, now: Instant) -> Vec<Result<Option<Delivery>>> {
        let mut deliveries = vec![self.tick(time, now)];
        for studio in self.documents.values_mut() {
            if studio.any_live() {
                deliveries.push(studio.tick(time, now));
            }
        }
        deliveries
    }
    pub fn next_due_all(&self) -> Instant {
        self.documents
            .values()
            .filter(|s| s.any_live())
            .map(Self::next_due)
            .fold(self.next_due(), Instant::min)
    }

    /// A session editing `theme`. Without [`Self::with_host_decoding`] a
    /// screen that cannot play videos shows the poster.
    pub fn new(
        sensors: Box<dyn SensorSource>,
        renderer: Box<dyn FrameRenderer>,
        language: Language,
        theme: Theme,
    ) -> Self {
        let shared_sensors = bezel_sensors::SharedSensors::new(sensors);
        let shared_renderer = SharedRenderer(Arc::new(Mutex::new(renderer)));
        Self::from_shared(shared_sensors, shared_renderer, language, theme)
    }

    fn from_shared(
        shared_sensors: bezel_sensors::SharedSensors,
        shared_renderer: SharedRenderer,
        language: Language,
        theme: Theme,
    ) -> Self {
        let posters = posters_of(&theme);
        let mut runtime = ThemeRuntime::new(theme, BTreeMap::new(), language);
        runtime.limit_refresh(MAX_REFRESH);
        let mut studio = Self {
            sensors: Box::new(shared_sensors.clone()),
            renderer: Box::new(shared_renderer.clone()),
            shared_sensors,
            shared_renderer,
            serial_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            selected_screen: None,
            documents: BTreeMap::new(),
            language,
            runtime,
            catalog: Vec::new(),
            listed: Wanted::nothing(),
            sample_millis: 0.0,
            location: None,
            live: None,
            generation: 0,
            live_error: None,
            host: None,
            videos: BTreeMap::new(),
            video: None,
            posters,
            serials: 0,
            origin: Instant::now(),
            shutting_down: false,
        };
        studio.follow_video();
        studio
    }

    /// `now` on the session's clock.
    fn clock(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.origin)
    }

    /// Decodes the theme's video with `media` for screens that cannot play
    /// videos, from a copy written in `dir`.
    #[must_use]
    pub fn with_host_decoding(mut self, media: SharedMedia, dir: PathBuf) -> Self {
        self.host = Some(HostDecoding { media, dir });
        self
    }

    /// Draws day and month names in `language` from now on: the runtime
    /// starts again with the same theme and assets (graph histories start
    /// over, and the live screen's video is started again).
    pub fn set_language(&mut self, language: Language) {
        for document in self.documents.values_mut() {
            document.set_language(language);
        }
        if language == self.language {
            return;
        }
        self.language = language;
        let theme = self.runtime.theme().clone();
        let assets = self.runtime.assets().clone();
        let info = self.runtime.video_info().cloned();
        // The old runtime, and a video it decodes here, stop first.
        self.runtime = ThemeRuntime::new(theme, assets, language);
        self.runtime.limit_refresh(MAX_REFRESH);
        self.runtime.use_catalog(&self.catalog);
        self.runtime.want_also(self.listed.clone());
        self.runtime.set_video_info(info);
        if let Some(live) = self.live.as_mut() {
            live.host = None;
            live.restart_video = true;
        }
    }

    /// The language of day and month names.
    pub fn language(&self) -> Language {
        self.language
    }

    // ------------------------------------------------------------ sensors --

    /// Measures with `sensors` from now on (its options changed), and reads
    /// its catalog.
    pub fn replace_sensors(&mut self, sensors: Box<dyn SensorSource>) -> Result<&[SensorInfo]> {
        self.shared_sensors.replace(sensors);
        self.refresh_catalog()
    }

    /// Re-reads the sensor catalog (sensors come and go with hardware).
    pub fn refresh_catalog(&mut self) -> Result<&[SensorInfo]> {
        self.catalog = self.sensors.catalog()?;
        self.runtime.use_catalog(&self.catalog);
        Ok(&self.catalog)
    }

    /// The last catalog read.
    pub fn catalog(&self) -> &[SensorInfo] {
        &self.catalog
    }

    /// What each sensor of the last catalog measures.
    pub fn quantities(&self) -> &Quantities {
        self.runtime.quantities()
    }

    /// The sensors the library's list shows from now on (none while it is
    /// hidden): measured with the theme's from the next sample on.
    pub fn show_sensors(&mut self, listed: Wanted) {
        self.listed = listed.clone();
        self.runtime.want_also(listed);
    }

    /// What each sample asks the sensors to measure: the theme's and the
    /// list's.
    pub fn wanted(&self) -> &Wanted {
        self.runtime.wanted()
    }

    /// Takes a sample and records it in the graph histories.
    pub fn sample(&mut self) -> Result<()> {
        let started = Instant::now();
        self.runtime.sample(self.sensors.as_mut())?;
        let known: std::collections::HashSet<_> =
            self.catalog.iter().map(|info| &info.key).collect();
        if self
            .runtime
            .snapshot()
            .iter()
            .any(|(key, _)| !known.contains(key))
        {
            self.refresh_catalog()?;
        }
        self.sample_millis = started.elapsed().as_secs_f64() * 1000.0;
        Ok(())
    }

    /// The latest readings and how long taking them took.
    pub fn readings(&self) -> (&Snapshot, f64) {
        (self.runtime.snapshot(), self.sample_millis)
    }

    // ----------------------------------------------------------- document --

    /// The theme being edited.
    pub fn theme(&self) -> &Theme {
        self.runtime.theme()
    }

    /// Where the theme was loaded from or saved to.
    pub fn location(&self) -> Option<&ThemeLocation> {
        self.location.as_ref()
    }

    /// The session's assets (used by the theme or added for it).
    pub fn assets(&self) -> &BTreeMap<AssetRef, Vec<u8>> {
        self.runtime.assets()
    }

    /// Replaces the edited theme in place, keeping the history of sensors
    /// still graphed.
    pub fn set_theme(&mut self, theme: Theme) {
        if theme == *self.runtime.theme() {
            return;
        }
        let before = self.runtime.video().clone();
        self.runtime.replace_theme(theme);
        self.video_changed(&before);
        self.follow_video();
    }

    /// Starts a new document.
    pub fn start(
        &mut self,
        theme: Theme,
        assets: BTreeMap<AssetRef, Vec<u8>>,
        location: Option<ThemeLocation>,
    ) {
        let before = self.runtime.video().clone();
        self.posters = posters_of(&theme);
        self.runtime.replace(theme, assets);
        self.video_changed(&before);
        // The same name may hold other bytes now: probed and copied afresh.
        self.video = None;
        self.follow_video();
        self.location = location;
        self.videos.clear();
    }

    /// Follows the theme's video background: another video (or none) drops
    /// what the session held for the last one, its decoder first.
    fn follow_video(&mut self) {
        let asset = match &self.runtime.theme().background {
            Background::Video { asset, .. } => Some(asset),
            _ => None,
        };
        if self.video.as_ref().map(|v| &v.asset) == asset {
            return;
        }
        let asset = asset.cloned();
        // The old copy goes before a new one of the same name is written.
        self.video = None;
        self.video = asset.map(|asset| {
            self.serials = self
                .serial_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            ThemeVideo::new(asset, self.serials)
        });
    }

    /// After a new theme: another video starts again on the live screen.
    fn video_changed(&mut self, before: &VideoState) {
        if let Some(live) = self.live.as_mut()
            && self.runtime.video() != before
        {
            live.restart_video = true;
            live.host = None;
        }
    }

    /// Opens the theme at `location`.
    pub fn open(&mut self, store: &dyn ThemeStore, location: ThemeLocation) -> Result<()> {
        let (theme, assets) = store.load(&location)?;
        self.start(theme, assets, Some(location));
        Ok(())
    }

    /// Saves the theme (with only the assets it uses) at `location`. A video
    /// background's poster that does not show the theme's framing is taken
    /// again before, outside the session's lock ([`Self::poster_to_take`]).
    pub fn save(&mut self, store: &dyn ThemeStore, location: ThemeLocation) -> Result<()> {
        let used: BTreeSet<AssetRef> = self.theme().assets().into_iter().collect();
        let assets: BTreeMap<AssetRef, Vec<u8>> = self
            .assets()
            .iter()
            .filter(|(k, _)| used.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        store.save(&location, self.theme(), &assets)?;
        self.location = Some(location);
        Ok(())
    }

    /// Adds a file to the theme under `assets/`, named after `file_name`.
    /// The same bytes added twice give the same reference.
    pub fn add_asset(&mut self, file_name: &str, bytes: Vec<u8>) -> AssetRef {
        let assets = self.runtime.assets();
        if let Some((existing, _)) = assets.iter().find(|(_, b)| **b == bytes) {
            return existing.clone();
        }
        let (stem, extension) = split_name(file_name);
        let mut n = 1;
        let asset = loop {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            let candidate = AssetRef(format!("assets/{stem}{suffix}{extension}"));
            if !assets.contains_key(&candidate) {
                break candidate;
            }
            n += 1;
        };
        self.runtime.add_asset(asset.clone(), bytes);
        asset
    }

    /// Adds a video (or an animated GIF) for a background, named after
    /// `file_name`, with its poster (a PNG named after the video, taken with
    /// the default framing: a video is added without one) when one was
    /// taken. Returns the video's reference and the poster's.
    pub fn add_video(
        &mut self,
        file_name: &str,
        bytes: Vec<u8>,
        poster_png: Option<Vec<u8>>,
        duration: Option<Duration>,
    ) -> (AssetRef, Option<AssetRef>) {
        let video = self.add_asset(file_name, bytes);
        let poster = poster_png.map(|png| {
            let stem = video.0.rsplit('/').next().unwrap_or_default();
            let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
            self.add_asset(&format!("{stem}-poster.png"), png)
        });
        if let Some(poster) = &poster {
            self.posters.insert(poster.clone(), VideoFraming::default());
        }
        let known = self.videos.entry(video.clone()).or_default();
        known.poster = poster.clone().or(known.poster.take());
        known.duration = duration.or(known.duration);
        (video, known.poster.clone())
    }

    /// What the session learnt about `asset` when it was added as a video.
    pub fn added_video(&self, asset: &AssetRef) -> Option<&AddedVideo> {
        self.videos.get(asset)
    }

    // ----------------------------------------------------- video framing --

    /// The panel Auto frames the theme's video for: the live screen's, else
    /// the one the canvas is drawn for (`None`: neither is known).
    pub fn video_panel(&self) -> Option<PanelLayout> {
        self.runtime.video_panel()
    }

    /// Probing the theme's video while it still has to be probed: the
    /// caller runs it outside the session's lock ([`VideoProbe::run`]; a
    /// storage job may hold the converter for minutes) and hands the
    /// outcome to [`Self::probed`]. `None` once probed, without a video
    /// background or without a converter to probe with. A copy that cannot
    /// be written leaves the video unknown.
    pub fn video_to_probe(&mut self) -> Option<VideoProbe> {
        let host = self.host.as_ref()?;
        let video = self.video.as_mut().filter(|v| !v.probed)?;
        let serial = video.serial;
        match video.file(&host.dir, self.runtime.assets()) {
            Ok(location) => Some(VideoProbe {
                serial,
                location,
                media: Arc::clone(&host.media),
            }),
            Err(_) => {
                diag::report(DiagCode::VideoNotProbed);
                self.record_probe(None);
                None
            }
        }
    }

    /// [`Self::video_to_probe`] when the live screen is to start the video
    /// next (it waits for the probe); `None` otherwise.
    pub fn live_video_to_probe(&mut self) -> Option<VideoProbe> {
        self.live.as_ref().filter(|l| l.restart_video)?;
        self.video_to_probe()
    }

    pub fn live_videos_to_probe(&mut self) -> Vec<VideoProbe> {
        let mut probes: Vec<_> = self.live_video_to_probe().into_iter().collect();
        for document in self.documents.values_mut() {
            probes.extend(document.live_video_to_probe());
        }
        probes
    }

    /// What probing the theme's video said ([`VideoProbe::run`]); nothing
    /// when the theme has another video by then (even under the same name),
    /// or it was probed meanwhile.
    pub fn probed(&mut self, probed: Probed) {
        if !self
            .video
            .as_ref()
            .is_some_and(|v| v.serial == probed.serial)
        {
            if let Some(studio) = self
                .documents
                .values_mut()
                .find(|s| s.video.as_ref().is_some_and(|v| v.serial == probed.serial))
            {
                studio.probed(probed);
                return;
            }
        }
        let current = self
            .video
            .as_ref()
            .is_some_and(|v| v.serial == probed.serial && !v.probed);
        if current {
            self.record_probe(probed.info);
        }
    }

    /// Whether the theme's video is probed, or cannot be (no video, no
    /// converter to probe with): Auto, the live screen's start and the
    /// preview's decoder wait for it.
    fn video_known(&self) -> bool {
        self.host.is_none() || self.video.as_ref().is_none_or(|v| v.probed)
    }

    /// What Auto turns the theme's video, clockwise quarter turns, and the
    /// video's own size as probed (`None` when the probe gave none, or it
    /// was not probed); `None` without a video background. Auto turns a
    /// video of the panel's native size in a theme a quarter turn from the
    /// panel ([`auto_turns`]).
    pub fn video_auto(&self) -> Option<(u8, Option<Size>)> {
        self.video.as_ref()?;
        let size = self.runtime.video_info().and_then(|info| info.dimensions);
        let orientation = self.runtime.theme().orientation;
        Some((auto_turns(size, orientation, self.video_panel()), size))
    }

    /// The theme's video was probed: the runtime frames it from `info`, and
    /// a live screen whose copy to look for changed starts it again.
    fn record_probe(&mut self, info: Option<MediaInfo>) {
        if let Some(video) = self.video.as_mut() {
            video.probed = true;
        }
        let before = self.runtime.video().clone();
        self.runtime.set_video_info(info);
        self.video_changed(&before);
    }

    /// The poster of the theme's video as `framing` frames it now (Auto
    /// resolved for the probed size and the panel); `None` while the video
    /// is unknown.
    fn poster_spec(&self, framing: VideoFraming) -> Option<PosterSpec> {
        let info = self.runtime.video_info()?;
        let theme = self.runtime.theme();
        let resolved = framing.resolve(info.dimensions, theme.orientation, self.video_panel());
        Some(PosterSpec::framed(theme.canvas, info, &resolved))
    }

    /// Taking the video background's poster again, when the theme frames
    /// the video otherwise than the poster shows
    /// (D-2026-10-01-video-background-framing-3): the caller takes it
    /// outside the session's lock ([`PosterRetake::run`]) and hands it to
    /// [`Self::poster_taken`] before saving. `None` when the poster shows
    /// the framing, without a poster or a converter, or while the video is
    /// not probed.
    pub fn poster_to_take(&mut self) -> Option<PosterRetake> {
        let Background::Video {
            poster: Some(poster),
            framing,
            ..
        } = &self.runtime.theme().background
        else {
            return None;
        };
        let (poster, framing) = (poster.clone(), framing.unwrap_or_default());
        let shown = *self.posters.get(&poster)?;
        let spec = self.poster_spec(framing)?;
        if self.poster_spec(shown) == Some(spec) {
            return None;
        }
        let (video, host) = (self.video.as_mut()?, self.host.as_ref()?);
        let location = video
            .file(&host.dir, self.runtime.assets())
            .inspect_err(|_| diag::report(DiagCode::PosterNotRetaken))
            .ok()?;
        Some(PosterRetake {
            serial: video.serial,
            poster,
            spec,
            location,
            media: Arc::clone(&host.media),
        })
    }

    /// Puts a poster taken again ([`PosterRetake::run`]) in the theme under
    /// its own name, so the theme stays as it is, when the theme still has
    /// that video and that poster and frames it as the poster shows;
    /// otherwise (another video, poster or framing meanwhile) it is dropped.
    /// Whether it was put.
    pub fn poster_taken(&mut self, taken: TakenPoster) -> bool {
        if !self
            .video
            .as_ref()
            .is_some_and(|v| v.serial == taken.serial)
        {
            if let Some(studio) = self
                .documents
                .values_mut()
                .find(|s| s.video.as_ref().is_some_and(|v| v.serial == taken.serial))
            {
                return studio.poster_taken(taken);
            }
        }
        let Background::Video {
            poster: Some(poster),
            framing,
            ..
        } = &self.runtime.theme().background
        else {
            return false;
        };
        let framing = framing.unwrap_or_default();
        let current = *poster == taken.poster
            && self
                .video
                .as_ref()
                .is_some_and(|v| v.serial == taken.serial)
            && self.poster_spec(framing) == Some(taken.spec);
        if current {
            self.runtime.add_asset(taken.poster.clone(), taken.png);
            self.posters.insert(taken.poster, framing);
        }
        current
    }

    // ------------------------------------------------------------- frames --

    /// Renders the edited theme with the latest readings over the video
    /// background's poster.
    pub fn render(&mut self, time: LocalTime) -> Result<Frame> {
        let clock = self.clock(Instant::now());
        let (frame, _) = self.runtime.preview(self.renderer.as_mut(), time, clock)?;
        Ok(frame)
    }

    /// The preview at `now`, and how long until it changes by itself
    /// (`None`: nothing moves): when the UI draws its next frame. Its
    /// animated GIFs show as they are then; a video background plays with
    /// [`Motion::Allowed`] (the decoder's picture of `now`, framed as the
    /// theme says now; its next picture is due on the [`PREVIEW_FPS`] grid)
    /// and shows its poster otherwise, without ffmpeg, while the video is
    /// not probed or a storage job holds the converter (asked again soon),
    /// and after a decoder failed (asked again when the next one may start,
    /// [`PREVIEW_ATTEMPTS`] in a row at most).
    pub fn preview(
        &mut self,
        time: LocalTime,
        now: Instant,
        motion: Motion,
    ) -> Result<(Frame, Option<Duration>)> {
        let mut playing = match motion {
            Motion::Allowed if !self.video_known() => Playing::Busy,
            Motion::Allowed => self.start_preview(now),
            Motion::Reduced => {
                self.stop_preview();
                Playing::Poster
            }
        };
        if playing == Playing::Video {
            if let Some(shown) = self.preview_picture(time, now)? {
                return Ok(shown);
            }
            // The decoder just failed.
            playing = self
                .video
                .as_ref()
                .map_or(Playing::Poster, |v| v.waiting(now));
        }
        let clock = self.clock(now);
        let (frame, change) = self.runtime.preview(self.renderer.as_mut(), time, clock)?;
        let gifs = change.map(|at| at.saturating_sub(clock));
        let again = match playing {
            Playing::Busy => Some(CONVERTER_BUSY),
            Playing::Again(wait) => Some(wait),
            Playing::Video | Playing::Poster => None,
        };
        Ok((frame, sooner(gifs, again)))
    }

    /// Starts the preview's decoder of the theme's video when none runs:
    /// the raw source at most [`PREVIEW_FPS`] pictures a second
    /// ([`ThemeRuntime::video_stream`]).
    fn start_preview(&mut self, now: Instant) -> Playing {
        let (Some(video), Some(host)) = (self.video.as_mut(), self.host.as_ref()) else {
            return Playing::Poster;
        };
        if video.decoder.is_some() {
            return Playing::Video;
        }
        if video.retry.is_some_and(|at| now < at) {
            return video.waiting(now);
        }
        let Some(mut media) = converter(&host.media, Wait::No) else {
            return Playing::Busy;
        };
        if let MediaTools::Missing { .. } = media.tools() {
            video.retry = Some(now + PREVIEW_RETRY);
            return Playing::Poster;
        }
        let spec = self.runtime.video_stream(PREVIEW_FPS);
        let started = video
            .file(&host.dir, self.runtime.assets())
            .and_then(|location| media.stream(&location, spec));
        match started {
            Ok(frames) => {
                video.decoder = Some(PreviewDecoder { frames, asked: now });
                Playing::Video
            }
            Err(_) => {
                diag::report(DiagCode::PreviewNotPlayed);
                video.failed(now);
                video.waiting(now)
            }
        }
    }

    /// The preview over the decoder's picture of `now`, and when the next
    /// picture (or a GIF's) is due; `None` when the decoder failed (it is
    /// dropped and the poster shows).
    fn preview_picture(
        &mut self,
        time: LocalTime,
        now: Instant,
    ) -> Result<Option<(Frame, Option<Duration>)>> {
        let clock = self.clock(now);
        let Some(video) = self.video.as_mut() else {
            return Ok(None);
        };
        let elapsed = video.elapsed(now, self.runtime.video_info());
        let Some(decoder) = video.decoder.as_mut() else {
            return Ok(None);
        };
        decoder.asked = now;
        let picture = match decoder.frames.frame_at(elapsed) {
            Ok(picture) => picture,
            Err(_) => {
                diag::report(DiagCode::PreviewStopped);
                video.failed(now);
                return Ok(None);
            }
        };
        video.failures = 0;
        let renderer = self.renderer.as_mut();
        let (frame, change) = self.runtime.preview_video(renderer, time, clock, picture)?;
        let gifs = change.map(|at| at.saturating_sub(clock));
        Ok(Some((frame, sooner(gifs, Some(next_picture(elapsed))))))
    }

    /// Ends the preview's decoder (the next picture asked for starts
    /// another, from the clock).
    fn stop_preview(&mut self) {
        if let Some(video) = self.video.as_mut() {
            video.decoder = None;
        }
    }

    /// Ends the preview's decoder when no picture was asked of it during
    /// [`PREVIEW_IDLE`] before `now`.
    fn stop_idle_preview(&mut self, now: Instant) {
        let idle = self
            .video
            .as_ref()
            .and_then(|v| v.decoder.as_ref())
            .is_some_and(|d| now.saturating_duration_since(d.asked) >= PREVIEW_IDLE);
        if idle {
            self.stop_preview();
        }
    }

    /// Whether the preview's decoder runs.
    pub fn previewing(&self) -> bool {
        self.video.as_ref().is_some_and(|v| v.decoder.is_some())
    }

    /// Key of the screen showing the theme, if any.
    pub fn live_key(&self) -> Option<&str> {
        self.live.as_ref().map(|l| l.key.as_str())
    }

    /// Whether `key` is the screen showing the theme: its live key or
    /// either port of it (a rev C screen answers to its display's and to its
    /// MCU's). Every "is this the live screen?" asks this, so whatever
    /// names the live screen by either port gets its open link and never
    /// opens the port again (D-2026-10-01-live-screen-controls-3).
    pub fn is_live(&self, key: &str) -> bool {
        self.live.as_ref().is_some_and(|l| l.answers_to(key))
            || self.documents.values().any(|s| s.is_live(key))
    }

    /// Why the live screen stopped, until the next `go_live`.
    pub fn live_error(&self) -> Option<&UiError> {
        self.live_error.as_ref()
    }

    /// Where the live screen stands while its link failed and it is being
    /// connected again; `None` otherwise.
    pub fn reconnecting(&self) -> Option<Reconnecting> {
        match &self.live.as_ref()?.slot {
            Slot::Away(away) => Some(Reconnecting {
                attempt: away.attempts.attempt(),
                attempts: Reconnect::attempts(),
            }),
            _ => None,
        }
    }

    /// How the theme's video background reaches the live screen (`None`
    /// when no screen is live).
    pub fn live_video(&self) -> Option<&VideoState> {
        self.live.as_ref().map(|_| self.runtime.video())
    }

    /// The theme video the live screen `key` could play but does not store.
    pub fn missing_video(&self, key: &str) -> Option<MissingVideo> {
        if !self.live.as_ref().is_some_and(|live| live.key == key) {
            if let Some(document) = self
                .documents
                .values()
                .find(|document| document.is_live(key))
            {
                return document.missing_video(key);
            }
        }
        if !self.is_live(key) {
            return None;
        }
        match self.runtime.video() {
            VideoState::VideoMissing(missing) => Some(missing.clone()),
            _ => None,
        }
    }

    /// Whether the live link is out showing a frame: wait for
    /// [`Self::presented`] before asking for it.
    pub fn presenting(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|l| matches!(l.slot, Slot::Presenting))
            || self.documents.values().any(Self::presenting)
    }

    /// The live link of `key`, or why it cannot be had. In the final state
    /// of a shutdown no screen's link can be had, live or not.
    fn link_of(&mut self, key: &str) -> Result<Option<&mut Box<dyn ScreenLink>>> {
        if !self.live.as_ref().is_some_and(|l| l.answers_to(key)) {
            if let Some(studio) = self.documents.values_mut().find(|s| s.is_live(key)) {
                return studio.link_of(key);
            }
        }
        if self.shutting_down {
            return Err(BezelError::InUse {
                address: key.to_string(),
                holders: vec![SHUTTING_DOWN.to_string()],
            });
        }
        if !self.live.as_ref().is_some_and(|l| l.answers_to(key)) {
            return Ok(None);
        }
        let Some(live) = self.live.as_mut() else {
            return Ok(None);
        };
        let holder = match live.slot {
            Slot::Here(_) => return Ok(live.slot.link()),
            Slot::Presenting => LIVE_FRAME,
            Slot::Lent => STORAGE_JOB,
            Slot::Away(_) => RECONNECTING,
            Slot::ShutDown => SHUTTING_DOWN,
        };
        Err(BezelError::InUse {
            address: key.to_string(),
            holders: vec![holder.to_string()],
        })
    }

    /// Whether the session is in the final state of a shutdown.
    pub fn shutting_down(&self) -> bool {
        self.shutting_down
    }

    /// The final state of a shutdown starts (D-2026-10-03-power-off-standby-3):
    /// from now on no frame is drawn for the live screen, its link is
    /// neither lent nor connected again, and no screen goes live. Live mode
    /// stays as it is (the shutdown takes its link,
    /// [`Self::take_for_shutdown`]).
    pub fn enter_final_state(&mut self) {
        self.shutting_down = true;
        for studio in self.documents.values_mut() {
            studio.enter_final_state();
        }
    }

    /// The shutdown was cancelled: the final state ends. A live screen whose
    /// link the shutdown took stops (the caller shows the theme live again,
    /// as at the app's start); one being connected again goes on.
    pub fn leave_final_state(&mut self) -> Option<Box<dyn ScreenLink>> {
        self.shutting_down = false;
        for studio in self.documents.values_mut() {
            drop(studio.leave_final_state());
        }
        if self
            .live
            .as_ref()
            .is_some_and(|l| matches!(l.slot, Slot::ShutDown))
        {
            return self.stop_live();
        }
        None
    }

    /// The live link, for the shutdown to apply the screen's choice through
    /// it; in its place the live screen keeps [`Slot::ShutDown`], so that
    /// nothing else reaches it. Only in the final state: nothing otherwise.
    pub fn take_for_shutdown(&mut self) -> ForShutdown {
        for studio in self.documents.values_mut() {
            match studio.take_for_shutdown() {
                ForShutdown::Nothing => {}
                result => return result,
            }
        }
        if !self.shutting_down {
            return ForShutdown::Nothing;
        }
        let Some(live) = self.live.as_mut() else {
            return ForShutdown::Nothing;
        };
        let out = match &live.slot {
            Slot::Presenting | Slot::Lent => true,
            Slot::Away(away) => away.trying,
            Slot::Here(_) | Slot::ShutDown => false,
        };
        if out {
            return ForShutdown::Out;
        }
        let Some(link) = live.slot.take_for(Slot::ShutDown) else {
            return ForShutdown::Nothing;
        };
        live.host = None;
        self.runtime.forget_screen();
        ForShutdown::Link(link)
    }

    /// Lends the live link of `key` to a storage job: frames pause and the
    /// session stays usable (previews keep rendering) while the job talks to
    /// the screen. `None` when `key` is not live; `InUse` while its link is
    /// out, and in the final state of a shutdown.
    pub fn lend_live_link(&mut self, key: &str) -> Result<Option<Box<dyn ScreenLink>>> {
        if !self.live.as_ref().is_some_and(|l| l.answers_to(key)) {
            if let Some(studio) = self.documents.values_mut().find(|s| s.is_live(key)) {
                return studio.lend_live_link(key);
            }
        }
        if self.link_of(key)?.is_none() {
            return Ok(None);
        }
        Ok(self.live.as_mut().and_then(|l| l.slot.take_for(Slot::Lent)))
    }

    /// Takes back a link lent by [`Self::lend_live_link`] (the next frame
    /// starts the video again after `resume`). Hands the link back when
    /// `key` stopped being live meanwhile: the caller closes it.
    pub fn return_live_link(
        &mut self,
        key: &str,
        link: Box<dyn ScreenLink>,
        resume: Resume,
    ) -> Option<Box<dyn ScreenLink>> {
        if !self.live.as_ref().is_some_and(|l| l.answers_to(key)) {
            if let Some(studio) = self.documents.values_mut().find(|s| s.is_live(key)) {
                return studio.return_live_link(key, link, resume);
            }
        }
        if !self.is_live(key) {
            return Some(link);
        }
        let Some(live) = self.live.as_mut().filter(|l| matches!(l.slot, Slot::Lent)) else {
            return Some(link);
        };
        live.slot = Slot::Here(link);
        live.restart_video |= resume == Resume::Video;
        None
    }

    /// Shows the edited theme on `link` from the next frame on; whether it
    /// does. In the final state of a shutdown no screen goes live: `link` is
    /// closed (`false`).
    pub fn go_live(&mut self, key: String, link: Box<dyn ScreenLink>) -> bool {
        if self.shutting_down {
            drop(link);
            return false;
        }
        self.runtime.forget_screen();
        self.generation += 1;
        self.live = Some(Live {
            key,
            screen: None,
            generation: self.generation,
            slot: Slot::Here(link),
            drawn: None,
            orientation: None,
            restart_video: true,
            host: None,
        });
        self.live_error = None;
        true
    }

    /// [`Self::go_live`] on `screen`, which is connected again when its
    /// link fails and is known by either of its ports ([`Self::is_live`]).
    pub fn go_live_on(&mut self, key: String, screen: Screen, link: Box<dyn ScreenLink>) -> bool {
        if !self.go_live(key, link) {
            return false;
        }
        if let Some(live) = self.live.as_mut() {
            live.screen = Some(screen);
        }
        true
    }

    /// Stops showing the theme and hands back the screen's link (`None`
    /// while it is out: whoever has it closes it). A screen being connected
    /// again stops there. In the final state of a shutdown live mode stays
    /// as it is and no link is handed out (`None`).
    pub fn stop_live(&mut self) -> Option<Box<dyn ScreenLink>> {
        if self.shutting_down {
            return None;
        }
        let mut live = self.live.take()?;
        self.generation += 1;
        // The decoder stops before its copy of the video goes.
        self.runtime.forget_screen();
        live.slot.take_for(Slot::Lent)
    }

    /// Sets the brightness of the live screen when it is `key`; `false` when
    /// that screen is not live. `InUse` while its link is out.
    pub fn live_brightness(&mut self, key: &str, brightness: Brightness) -> Result<bool> {
        match self.link_of(key)? {
            Some(link) => link.set_brightness(brightness).map(|()| true),
            None => Ok(false),
        }
    }

    /// Starts the theme's video on the live screen when it has to (a failure
    /// keeps the poster), once the video is probed
    /// ([`Self::live_video_to_probe`]): its size decides Auto and the copy
    /// the screen looks for; until then the poster stays and the start is
    /// tried again at the next frame. A screen that cannot play videos gets
    /// it decoded here; while the converter is busy with a storage job the
    /// poster stays likewise.
    fn start_live_video(&mut self) {
        let due = self
            .live
            .as_ref()
            .is_some_and(|l| l.restart_video && matches!(l.slot, Slot::Here(_)));
        if !due || !self.video_known() {
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(link) = live.slot.link() else {
            return;
        };
        if !live.restart_video {
            return;
        }
        live.host = None;
        let playback = link.identity().model.capabilities.video_playback;
        let here = match (&self.runtime.theme().background, self.host.as_ref()) {
            (Background::Video { asset, .. }, Some(host)) if !playback => {
                Some((asset.clone(), host))
            }
            _ => None,
        };
        let started = match here {
            None => self.runtime.start_video(link.as_mut(), None).map(|_| None),
            Some((asset, host)) => {
                let Some(mut media) = converter(&host.media, Wait::No) else {
                    return;
                };
                let media: &mut dyn MediaTranscoder = media.as_mut();
                decode_here(&mut self.runtime, link.as_mut(), media, &host.dir, &asset)
            }
        };
        live.restart_video = false;
        match started {
            Ok(playback) => live.host = playback,
            Err(_) => diag::report(DiagCode::VideoBackgroundNotStarted),
        }
    }

    /// Renders the live screen's frame of `now` and takes its link out of
    /// the session to show it ([`Delivery::present`], then
    /// [`Self::presented`]). `None` while nothing is live or the link is
    /// out. A theme that does not fit the screen stops the live mode, kept
    /// for [`Self::live_error`]. Nothing in the final state of a shutdown.
    pub fn frame_for_screen(&mut self, time: LocalTime, now: Instant) -> Result<Option<Delivery>> {
        if self.shutting_down {
            return Ok(None);
        }
        self.start_live_video();
        let orientation = self.runtime.theme().orientation;
        let clock = self.clock(now);
        let Some(live) = self.live.as_mut() else {
            return Ok(None);
        };
        let Some(link) = live.slot.link() else {
            return Ok(None);
        };
        let panel = link.identity().model.panel;
        let video = live
            .host
            .as_ref()
            .map_or(Duration::ZERO, |h| now.saturating_duration_since(h.started));
        live.drawn = Some(now);
        let renderer = self.renderer.as_mut();
        let frame = match self.runtime.render_at(renderer, time, clock, video) {
            Ok(frame) => frame,
            Err(e) => {
                self.stop_with(UiError::from(e.clone()));
                return Err(e);
            }
        };
        if let Some(misfit) = misfit(self.runtime.theme(), panel) {
            let error = BezelError::InvalidInput(misfit.to_string());
            self.stop_with(misfit);
            return Err(error);
        }
        let Some(live) = self.live.as_mut() else {
            return Ok(None);
        };
        let Some(link) = live.slot.take_for(Slot::Presenting) else {
            return Ok(None);
        };
        Ok(Some(Delivery {
            key: live.key.clone(),
            link,
            frame,
            turn: Some(orientation).filter(|o| live.orientation != Some(*o)),
        }))
    }

    /// Takes the link back after `delivery` showed its frame with
    /// `outcome`. A link that failed is dropped: a screen known by identity
    /// is connected again from 2 s after `now` ([`Self::reconnect_due`]);
    /// any other failure stops the live mode (kept for
    /// [`Self::live_error`]). Hands the link back when it is not taken (live
    /// mode stopped meanwhile, or it failed): the caller closes it.
    pub fn presented(
        &mut self,
        delivery: Delivery,
        outcome: &Result<()>,
        now: Instant,
    ) -> Option<Box<dyn ScreenLink>> {
        if !self
            .live
            .as_ref()
            .is_some_and(|l| l.answers_to(&delivery.key))
        {
            if let Some(studio) = self
                .documents
                .values_mut()
                .find(|s| s.is_live(&delivery.key))
            {
                return studio.presented(delivery, outcome, now);
            }
        }
        let Delivery {
            key, link, turn, ..
        } = delivery;
        if !self.is_live(&key) {
            return Some(link);
        }
        let Some(live) = self
            .live
            .as_mut()
            .filter(|l| matches!(l.slot, Slot::Presenting))
        else {
            return Some(link);
        };
        if let Err(e) = outcome {
            if worth_reconnecting(e) && live.screen.is_some() {
                let mut attempts = Reconnect::new();
                let wait = attempts.next_wait().unwrap_or_default();
                diag::report(DiagCode::LiveScreenLost);
                live.slot = Slot::Away(Away {
                    attempts,
                    due: now + wait,
                    error: e.clone(),
                    trying: false,
                });
                live.host = None;
                self.runtime.forget_screen();
            } else {
                self.stop_with(UiError::from(e.clone()));
            }
            return Some(link);
        }
        live.slot = Slot::Here(link);
        if turn.is_some() {
            live.orientation = turn;
        }
        None
    }

    /// Stops the live mode after `error`.
    fn stop_with(&mut self, error: UiError) {
        self.runtime.forget_screen();
        self.live = None;
        self.generation += 1;
        self.live_error = Some(error);
    }

    /// The attempt to connect the live screen again that is due at `now`,
    /// if any: the caller makes it outside the session (it takes seconds,
    /// longer when a hung screen restarts) and reports with
    /// [`Self::reconnected`]. Nothing while one is under way, nor in the
    /// final state of a shutdown.
    pub fn reconnect_due(&mut self, now: Instant) -> Option<Attempt> {
        if self.shutting_down {
            return None;
        }
        let live = self.live.as_mut()?;
        let screen = live.screen.clone()?;
        let Slot::Away(away) = &mut live.slot else {
            return None;
        };
        if away.trying || now < away.due {
            return None;
        }
        away.trying = true;
        Some(Attempt {
            screen,
            generation: live.generation,
        })
    }

    /// The outcome of `attempt`: back, the screen shows the theme again (a
    /// whole frame first, its video started again) under the key it has now;
    /// still away, the next attempt is due after its wait, and after the
    /// last one (or a failure that would repeat) live mode stops with the
    /// error that stopped the link. Hands the link back when live mode
    /// stopped or started again meanwhile: the caller closes it.
    pub fn reconnected(
        &mut self,
        attempt: Attempt,
        outcome: Result<(Screen, Box<dyn ScreenLink>)>,
        now: Instant,
    ) -> Option<Box<dyn ScreenLink>> {
        let Some(live) = self
            .live
            .as_mut()
            .filter(|l| l.generation == attempt.generation && matches!(l.slot, Slot::Away(_)))
        else {
            return outcome.ok().map(|(_, link)| link);
        };
        let Slot::Away(away) = &mut live.slot else {
            return None;
        };
        match outcome {
            Ok((screen, link)) => {
                diag::report(DiagCode::LiveScreenBack);
                live.key = screen
                    .address()
                    .map_or_else(|| live.key.clone(), |a| a.0.clone());
                live.screen = Some(screen);
                live.slot = Slot::Here(link);
                live.orientation = None;
                live.restart_video = true;
                live.drawn = None;
                self.runtime.forget_screen();
            }
            Err(e) => {
                away.trying = false;
                let wait = worth_reconnecting(&e)
                    .then(|| away.attempts.next_wait())
                    .flatten();
                diag::report(DiagCode::LiveScreenNotBack);
                match wait {
                    Some(wait) => away.due = now + wait,
                    None => {
                        let error = away.error.clone();
                        self.stop_with(UiError::from(error));
                    }
                }
            }
        }
        None
    }

    /// Time between two refreshes: the theme's refresh, or a picture of a
    /// video decoded here.
    pub fn period(&self) -> Duration {
        if self.live.as_ref().is_some_and(|l| l.host.is_some()) {
            return Duration::from_secs(1) / HOST_VIDEO_FPS;
        }
        self.refresh()
    }

    /// The theme's refresh, at most [`MAX_REFRESH`].
    fn refresh(&self) -> Duration {
        refresh_interval(self.runtime.theme().refresh_seconds, MAX_REFRESH)
    }

    /// When the live screen's next frame is due: the runtime's (the
    /// refresh, a visible GIF's frames), sooner while a video decoded here
    /// sets the pace. `None` while no frame can be drawn for it.
    fn frame_due(&self) -> Option<Instant> {
        let live = self.live.as_ref()?;
        if !matches!(live.slot, Slot::Here(_)) {
            return None;
        }
        let due = self.origin + self.runtime.next_due();
        let host = live
            .host
            .as_ref()
            .and(live.drawn)
            .map(|at| at + self.period());
        Some(host.map_or(due, |host| due.min(host)))
    }

    /// When the next refresh is due: a sample, a frame of the live screen,
    /// an attempt to connect it again.
    pub fn next_due(&self) -> Instant {
        let sample = self.origin + self.runtime.next_sample();
        let away = self.live.as_ref().and_then(|l| match &l.slot {
            Slot::Away(away) if !away.trying => Some(away.due),
            _ => None,
        });
        [Some(sample), self.frame_due(), away]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(sample)
    }

    /// One refresh at `now`: the preview's decoder ends when no picture was
    /// asked of it during [`PREVIEW_IDLE`], a sample when one is due (once
    /// per refresh, never per animation frame), then the live screen's frame
    /// when it is due ([`Self::frame_for_screen`]). In the final state of a
    /// shutdown, nothing.
    pub fn tick(&mut self, time: LocalTime, now: Instant) -> Result<Option<Delivery>> {
        if self.shutting_down {
            return Ok(None);
        }
        self.stop_idle_preview(now);
        // Before the sample moves the cadence on.
        let due = self.frame_due();
        let (clock, started) = (self.clock(now), Instant::now());
        match self.runtime.sample_on_time(self.sensors.as_mut(), clock) {
            Ok(true) => self.sample_millis = started.elapsed().as_secs_f64() * 1000.0,
            Ok(false) => {}
            Err(_) => diag::report(DiagCode::SensorSampleFailed),
        }
        if due.is_none_or(|due| now < due) {
            return Ok(None);
        }
        self.frame_for_screen(time, now)
    }
}

/// The framing the poster of `theme`'s video background shows, as loaded:
/// the theme's own.
fn posters_of(theme: &Theme) -> BTreeMap<AssetRef, VideoFraming> {
    match &theme.background {
        Background::Video {
            poster: Some(poster),
            framing,
            ..
        } => BTreeMap::from([(poster.clone(), framing.unwrap_or_default())]),
        _ => BTreeMap::new(),
    }
}

/// Why `theme` does not fit a panel whose portrait size is `panel`, if it
/// does not.
fn misfit(theme: &Theme, panel: Size) -> Option<UiError> {
    let expected = theme.misfit(panel)?;
    let size = |s: Size| format!("{}x{}", s.width, s.height);
    Some(
        UiError::new(ErrorCode::ThemeMisfit)
            .arg("theme", size(theme.canvas))
            .arg("screen", size(expected)),
    )
}

/// Starts the theme's video `asset` on `link` offering to decode it here
/// with `media`, from a copy written in `dir`: the playback when the runtime
/// chose that (the screen may have no converter).
fn decode_here(
    runtime: &mut ThemeRuntime,
    link: &mut dyn ScreenLink,
    media: &mut dyn MediaTranscoder,
    dir: &Path,
    asset: &AssetRef,
) -> Result<Option<HostPlayback>> {
    let bytes = runtime
        .assets()
        .get(asset)
        .ok_or_else(|| BezelError::InvalidInput(format!("{} is not in the theme", asset.0)))?;
    let copy = VideoCopy::write(dir, asset, bytes)
        .map_err(|e| BezelError::Transport(format!("{}: {e}", dir.display())))?;
    let offer = HostVideo {
        media,
        source: MediaLocation(copy.0.display().to_string()),
        fps: HOST_VIDEO_FPS,
    };
    let playing = matches!(runtime.start_video(link, Some(offer))?, VideoState::Host);
    Ok(playing.then(|| HostPlayback {
        _copy: copy,
        started: Instant::now(),
    }))
}

/// Who holds a live screen while a storage job borrows its link.
pub const STORAGE_JOB: &str = "a storage job of Bezel";
/// Who holds a live screen while it shows a frame.
pub const LIVE_FRAME: &str = "Bezel's live frame";
/// Who holds a live screen while it is being connected again.
pub const RECONNECTING: &str = "Bezel, connecting it again";
/// Who holds every screen in the final state of a shutdown.
pub const SHUTTING_DOWN: &str = "Bezel, as the computer shuts down";

/// `"My Photo.PNG"` → (`"my-photo"`, `".png"`): safe, lowercase asset names.
fn split_name(file_name: &str) -> (String, String) {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or_default();
    let (stem, extension) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() => (s, format!(".{}", clean(e))),
        _ => (base, String::new()),
    };
    let stem = clean(stem);
    (
        if stem.is_empty() { "file".into() } else { stem },
        extension,
    )
}

fn clean(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    mapped
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::{FakeMedia, STREAMED, STREAMED_END};
    use bezel_core::app::open_screen;
    use bezel_core::app::{choose_screen, discover_screens, reopen_screen};
    use bezel_core::domain::device::{Transport, UsbId};
    use bezel_core::domain::discovery::Screen;
    use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::framing::{FramingPosition, Permille, VideoFit, Zoom};
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::sensor::SensorKey;
    use bezel_core::domain::storage::RemotePath;
    use bezel_core::domain::theme::{
        Binding, BoxF, Element, ElementId, ElementKind, Fit, GraphStyle,
    };
    use bezel_core::ports::{Backdrop, RenderContext, ScreenConnector};
    use bezel_devices::fake::FakeStorage;
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;
    use std::sync::atomic::Ordering;

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 12,
        minute: 0,
        second: 0,
        weekday: 2,
    };

    /// Fills the canvas and counts renders.
    #[derive(Default, Clone)]
    struct Flat(Arc<Mutex<usize>>);

    impl FrameRenderer for Flat {
        fn render(
            &mut self,
            theme: &Theme,
            _: &BTreeMap<AssetRef, Vec<u8>>,
            _: RenderContext<'_>,
        ) -> Result<Frame> {
            *self.0.lock().unwrap() += 1;
            Ok(Frame::filled(theme.canvas, Rgba::BLACK))
        }
    }

    /// `ms` milliseconds into the session, on its clock: no real time
    /// passes in these tests.
    fn at(s: &Studio, ms: u64) -> Instant {
        s.origin + Duration::from_millis(ms)
    }

    /// Shows what `delivered` carries, as the backend does outside the
    /// session, at `now`.
    fn show(s: &mut Studio, delivered: Result<Option<Delivery>>, now: Instant) -> Result<()> {
        let Some(mut delivery) = delivered? else {
            return Ok(());
        };
        let outcome = delivery.present();
        drop(s.presented(delivery, &outcome, now));
        outcome
    }

    /// Probes the theme's video as the backend does before a preview, a
    /// save or Auto, outside the session (here, in its place), without
    /// waiting for a storage job that holds the converter.
    fn learn(s: &mut Studio) {
        if let Some(probed) = s.video_to_probe().and_then(|p| p.run(Wait::No)) {
            s.probed(probed);
        }
    }

    /// [`learn`], as the backend does before a live frame: only when the
    /// live screen is to start the video.
    fn learn_live(s: &mut Studio) {
        if let Some(probed) = s.live_video_to_probe().and_then(|p| p.run(Wait::No)) {
            s.probed(probed);
        }
    }

    /// A frame for the live screen at the session's start.
    fn present(s: &mut Studio) -> Result<()> {
        learn_live(s);
        let now = at(s, 0);
        let delivered = s.frame_for_screen(TIME, now);
        show(s, delivered, now)
    }

    /// The refresh `ms` milliseconds into the session.
    fn tick(s: &mut Studio, ms: u64) -> Result<()> {
        learn_live(s);
        let now = at(s, ms);
        let delivered = s.tick(TIME, now);
        show(s, delivered, now)
    }

    fn go_live(s: &mut Studio, link: Box<dyn ScreenLink>) -> Result<()> {
        s.go_live("k".into(), link);
        present(s)
    }

    fn theme_88() -> Theme {
        Theme::blank("T", Size::new(480, 1920), Orientation::ReversePortrait)
    }

    fn studio() -> Studio {
        Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(Flat::default()),
            Language::English,
            theme_88(),
        )
    }

    #[test]
    fn two_screen_documents_keep_their_theme_and_live_frames() {
        let first = FakeConnector::default();
        let second = FakeConnector::default();
        let mut s = studio();
        s.refresh_catalog().unwrap();
        s.select_screen("one", theme_88());
        s.go_live(
            "one".into(),
            open_screen(&FakeBus::turing_88(), &first, None).unwrap(),
        );
        assert!(s.select_screen(
            "two",
            Theme::blank("Second", Size::new(480, 1920), Orientation::Portrait)
        ));
        s.go_live(
            "two".into(),
            open_screen(&FakeBus::turing_88(), &second, None).unwrap(),
        );
        assert_eq!(s.theme().name, "Second");
        let now = Instant::now();
        for frame in s.tick_all(TIME, now) {
            show(&mut s, frame, now).unwrap();
        }
        assert_eq!(first.log().frames.len(), 1);
        assert_eq!(second.log().frames.len(), 1);
        s.select_screen("one", theme_88());
        assert_eq!(s.theme().name, "T");
        assert_eq!(s.live_key(), Some("one"));
        assert!(s.is_live("two"));
        drop(s.stop_live());
        assert!(s.any_live());
        assert!(!s.is_live("one"));
        s.stop_all();
        assert!(!s.any_live());
    }

    #[test]
    fn samples_feed_the_readings() {
        let mut s = studio();
        assert!(!s.refresh_catalog().unwrap().is_empty());
        s.sample().unwrap();
        s.sample().unwrap();
        let (snapshot, millis) = s.readings();
        assert!(!snapshot.is_empty());
        assert!(millis >= 0.0);
        assert_eq!(s.catalog().len(), s.refresh_catalog().unwrap().len());
    }

    #[test]
    fn live_mode_presents_in_the_theme_orientation_and_stops_on_errors() {
        let connector = FakeConnector::default();
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        go_live(&mut s, link).unwrap();
        assert_eq!(s.live_key(), Some("k"));
        tick(&mut s, 0).unwrap();
        let log = connector.log();
        assert_eq!(log.frames.len(), 2);
        assert_eq!(log.orientations, vec![Orientation::ReversePortrait]);

        assert!(s.live_brightness("k", Brightness::MAX).unwrap());
        assert!(!s.live_brightness("other", Brightness::MAX).unwrap());

        // A theme that does not fit the screen stops the live mode.
        s.set_theme(Theme::blank(
            "Small",
            Size::new(320, 480),
            Orientation::Portrait,
        ));
        let error = present(&mut s).unwrap_err().to_string();
        assert!(error.contains("320x480"), "{error}");
        assert_eq!(s.live_key(), None);
        let why = s.live_error().unwrap();
        assert_eq!(why.code(), "themeMisfit");
        assert_eq!(
            (why.value("theme"), why.value("screen")),
            (Some("320x480"), Some("480x1920"))
        );
        assert!(s.stop_live().is_none());
        present(&mut s).unwrap();
    }

    #[test]
    fn stop_live_hands_back_the_link() {
        let connector = FakeConnector::default();
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        go_live(&mut s, link).unwrap();
        s.stop_live().unwrap().release().unwrap();
        assert_eq!(connector.log().releases, 1);
        tick(&mut s, 0).unwrap();
        assert_eq!(connector.log().frames.len(), 1);
    }

    #[test]
    fn assets_get_safe_unique_names() {
        let mut s = studio();
        assert_eq!(
            s.add_asset("C:\\Photos\\My Photo.PNG", vec![1]).0,
            "assets/my-photo.png"
        );
        assert_eq!(
            s.add_asset("my photo.png", vec![2]).0,
            "assets/my-photo-2.png"
        );
        assert_eq!(s.add_asset("again.png", vec![1]).0, "assets/my-photo.png");
        assert_eq!(s.add_asset("../../.hidden", vec![3]).0, "assets/hidden");
        assert_eq!(s.add_asset("noext", vec![4]).0, "assets/noext");
        assert_eq!(s.assets().len(), 4);
    }

    #[test]
    fn videos_keep_their_poster_and_play_time_for_the_session() {
        let mut s = studio();
        let second = Duration::from_secs(1);
        let (video, poster) = s.add_video(
            "C:\\Clips\\Ondas Mar.MOV",
            vec![9],
            Some(vec![1]),
            Some(second),
        );
        assert_eq!(video.0, "assets/ondas-mar.mov");
        assert_eq!(poster.as_ref().unwrap().0, "assets/ondas-mar-poster.png");
        let known = s.added_video(&video).unwrap();
        assert_eq!(
            (known.poster.as_ref(), known.duration),
            (poster.as_ref(), Some(second))
        );
        // The same file again (now without ffmpeg) keeps what was learnt.
        let again = s.add_video("ondas mar.mov", vec![9], None, None);
        assert_eq!(again, (video.clone(), poster.clone()));
        // Without a poster, none.
        let (gif, none) = s.add_video("loop.gif", vec![7], None, None);
        assert_eq!((gif.0.as_str(), none), ("assets/loop.gif", None));
        assert_eq!(s.assets().len(), 3);
        // Another document forgets them.
        s.start(theme_88(), BTreeMap::new(), None);
        assert_eq!(s.added_video(&video), None);
    }

    type Stored = (Theme, BTreeMap<AssetRef, Vec<u8>>);

    #[derive(Default)]
    struct MemoryStore(Mutex<BTreeMap<ThemeLocation, Stored>>);

    impl ThemeStore for MemoryStore {
        fn load(&self, location: &ThemeLocation) -> Result<Stored> {
            self.0
                .lock()
                .unwrap()
                .get(location)
                .cloned()
                .ok_or_else(|| BezelError::ScreenNotFound(location.0.clone()))
        }

        fn save(
            &self,
            location: &ThemeLocation,
            theme: &Theme,
            assets: &BTreeMap<AssetRef, Vec<u8>>,
        ) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert(location.clone(), (theme.clone(), assets.clone()));
            Ok(())
        }
    }

    #[test]
    fn save_keeps_only_used_assets_and_open_restores_them() {
        let store = MemoryStore::default();
        let mut s = studio();
        let used = s.add_asset("bg.png", vec![1]);
        s.add_asset("unused.png", vec![2]);
        let mut theme = s.theme().clone();
        theme.background = Background::Image {
            asset: used.clone(),
            fit: Fit::Cover,
        };
        s.set_theme(theme.clone());
        let at = ThemeLocation("mem://a".into());
        s.save(&store, at.clone()).unwrap();
        assert_eq!(s.location(), Some(&at));

        let mut other = studio();
        other.open(&store, at.clone()).unwrap();
        assert_eq!(other.theme(), &theme);
        assert_eq!(other.assets().keys().collect::<Vec<_>>(), vec![&used]);
        assert!(
            other
                .open(&store, ThemeLocation("mem://nope".into()))
                .is_err()
        );
    }

    #[test]
    fn render_uses_the_edited_theme() {
        let renders = Flat::default();
        let mut s = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(renders.clone()),
            Language::PortugueseBr,
            theme_88(),
        );
        s.set_theme(s.theme().clone());
        let frame = s.render(TIME).unwrap();
        assert_eq!(frame.size(), Size::new(480, 1920));
        s.start(
            Theme::blank("Wide", Size::new(480, 1920), Orientation::Landscape),
            BTreeMap::new(),
            None,
        );
        assert_eq!(s.render(TIME).unwrap().size(), Size::new(1920, 480));
        assert_eq!(*renders.0.lock().unwrap(), 2);
        assert_eq!(s.location(), None);
    }

    /// Records the language of each render.
    #[derive(Default, Clone)]
    struct Languages(Arc<Mutex<Vec<Language>>>);

    impl FrameRenderer for Languages {
        fn render(
            &mut self,
            theme: &Theme,
            _: &BTreeMap<AssetRef, Vec<u8>>,
            context: RenderContext<'_>,
        ) -> Result<Frame> {
            self.0.lock().unwrap().push(context.language);
            Ok(Frame::filled(theme.canvas, Rgba::BLACK))
        }
    }

    #[test]
    fn day_and_month_names_follow_a_new_language() {
        let seen = Languages::default();
        let mut s = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(seen.clone()),
            Language::English,
            theme_88(),
        );
        s.refresh_catalog().unwrap();
        let asset = s.add_asset("logo.png", vec![1, 2, 3]);
        s.render(TIME).unwrap();
        s.set_language(Language::PortugueseBr);
        s.set_language(Language::PortugueseBr);
        s.render(TIME).unwrap();
        assert_eq!(
            *seen.0.lock().unwrap(),
            [Language::English, Language::PortugueseBr]
        );
        assert_eq!(s.language(), Language::PortugueseBr);
        assert!(s.assets().contains_key(&asset), "the assets stay");
        assert_eq!(s.theme().name, "T", "the theme stays");
        assert!(!s.quantities().is_empty(), "the catalog stays");
    }

    /// What a frame showed under the elements, and how many samples of
    /// `cpu.usage` its graph had.
    #[derive(Default, Clone)]
    struct Probe(Arc<Mutex<Vec<(&'static str, usize)>>>);

    impl FrameRenderer for Probe {
        fn render(
            &mut self,
            theme: &Theme,
            _: &BTreeMap<AssetRef, Vec<u8>>,
            context: RenderContext<'_>,
        ) -> Result<Frame> {
            let backdrop = match context.backdrop {
                Backdrop::Poster => "poster",
                Backdrop::OnDevice => "on-device",
                Backdrop::Frame(_) => "picture",
            };
            let usage = SensorKey::new("cpu.usage").unwrap();
            let samples = context.histories.get(&usage).len();
            self.0.lock().unwrap().push((backdrop, samples));
            Ok(Frame::filled(theme.canvas, Rgba::BLACK))
        }
    }

    /// `theme` graphing `cpu.usage` over 8 samples.
    fn graphing(mut theme: Theme) -> Theme {
        theme.elements.push(Element {
            id: ElementId(7),
            name: "usage".into(),
            frame: BoxF {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 40.0,
            },
            opacity: 1.0,
            visible: true,
            locked: false,
            kind: ElementKind::Graph {
                binding: Binding {
                    key: SensorKey::new("cpu.usage").unwrap(),
                    min: 0.0,
                    max: 100.0,
                },
                history: 8,
                style: GraphStyle::Line,
                color: Rgba::WHITE,
                fill: None,
                line_width: 1.0,
                autoscale: false,
            },
        });
        theme
    }

    fn with_video(mut theme: Theme, asset: &str) -> Theme {
        theme.background = Background::Video {
            asset: AssetRef(asset.into()),
            poster: None,
            framing: None,
        };
        theme
    }

    #[test]
    fn the_live_screen_and_the_preview_share_one_runtime() {
        let stored = RemotePath::parse("internal/video/clip.mp4").unwrap();
        let connector =
            FakeConnector::with_storage(FakeStorage::default().with_file(stored, vec![1; 64]));
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let probe = Probe::default();
        let theme = with_video(graphing(theme_88()), "assets/clip.mp4");
        let mut s = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(probe.clone()),
            Language::English,
            theme.clone(),
        );
        go_live(&mut s, link).unwrap();
        tick(&mut s, 0).unwrap();
        s.render(TIME).unwrap();
        // An edit swaps the theme in place: the history goes on.
        let mut edited = theme;
        edited.name = "Edited".into();
        s.set_theme(edited);
        tick(&mut s, 1_000).unwrap();
        assert_eq!(
            *probe.0.lock().unwrap(),
            [
                ("on-device", 0),
                ("on-device", 1),
                ("poster", 1),
                ("on-device", 2)
            ]
        );
        assert_eq!(
            s.readings().0.len(),
            FakeSensors::demo().catalog().unwrap().len()
        );
        assert_eq!(s.period(), Duration::from_secs(1));
    }

    /// The fake WeAct 0.96": no storage, no playback of stored videos.
    fn weact() -> (FakeConnector, Box<dyn ScreenLink>) {
        let bus = FakeBus::new(vec![Endpoint {
            address: DeviceAddress("/dev/ttyACM0".into()),
            transport: Transport::Serial,
            usb: UsbId::new(0x1a86, 0xfe0c),
            serial_number: Some("AD0001".into()),
            manufacturer: None,
            product: None,
            location: None,
        }]);
        let connector = FakeConnector::default();
        let link = open_screen(&bus, &connector, None).unwrap();
        (connector, link)
    }

    /// A session decoding videos with `media`, copies in a fresh `dir`.
    fn decoding(name: &str, media: FakeMedia) -> (Studio, SharedMedia, PathBuf) {
        let dir = std::env::temp_dir().join(format!("bezel-studio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shared: SharedMedia = Arc::new(Mutex::new(Box::new(media)));
        let theme = with_video(
            Theme::blank("Clip", Size::new(80, 160), Orientation::Portrait),
            "assets/Clip.mp4",
        );
        let mut s = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip)),
            Language::English,
            theme.clone(),
        )
        .with_host_decoding(Arc::clone(&shared), dir.clone());
        let assets = BTreeMap::from([(AssetRef("assets/Clip.mp4".into()), vec![1, 2, 3])]);
        s.start(theme, assets, None);
        (s, shared, dir)
    }

    #[test]
    fn a_screen_without_playback_gets_the_video_decoded_here() {
        let media = FakeMedia::ready();
        let streamed = Arc::clone(&media.streamed);
        let (mut s, shared, dir) = decoding("host", media);
        let (connector, link) = weact();
        {
            // The converter is busy with a storage job: the poster meanwhile.
            let _busy = shared.lock().unwrap();
            go_live(&mut s, link).unwrap();
            assert_eq!(s.live_video(), Some(&VideoState::NotStarted));
        }
        tick(&mut s, 0).unwrap();
        assert_eq!(s.live_video(), Some(&VideoState::Host));
        let copy = dir.join("Clip.mp4");
        assert_eq!(std::fs::read(&copy).unwrap(), [1, 2, 3]);
        let (source, spec) = streamed.lock().unwrap()[0].clone();
        assert_eq!(source.0, copy.display().to_string());
        // The raw 1920x1080 source as probed, its longer side twice the
        // canvas's at most; the runtime frames each picture.
        assert_eq!((spec.size, spec.fps), (Size::new(320, 180), HOST_VIDEO_FPS));
        assert_eq!(s.period(), Duration::from_millis(100));
        let shown = connector.log().frames.last().cloned().unwrap();
        assert_eq!(shown.pixel(40, 80), Some(STREAMED));
        let preview = s.render(TIME).unwrap();
        assert_ne!(preview.pixel(40, 80), Some(STREAMED), "the poster");

        // Another video starts over; stopping removes the copy.
        let mut other = s.theme().clone();
        other.background = Background::Color(Rgba::BLACK);
        s.set_theme(other);
        tick(&mut s, 100).unwrap();
        assert_eq!(s.live_video(), Some(&VideoState::NoVideo));
        assert!(!copy.exists(), "no video, no copy");
        assert_eq!(s.period(), Duration::from_secs(1));
        s.stop_live();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_converter_or_decoding_the_poster_shows() {
        let (mut s, _, dir) = decoding("no-ffmpeg", FakeMedia::missing());
        let (_, link) = weact();
        go_live(&mut s, link).unwrap();
        assert!(matches!(
            s.live_video(),
            Some(VideoState::NoConverter { .. })
        ));
        assert!(!dir.join("Clip.mp4").exists(), "the copy went with it");
        let (connector, link) = weact();
        s.stop_live();
        s.host = None;
        go_live(&mut s, link).unwrap();
        assert_eq!(s.live_video(), Some(&VideoState::NoPlayback));
        assert_eq!(connector.log().frames.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Demo sensors that keep what they were told last where the test sees
    /// it.
    struct Told(FakeSensors, Arc<Mutex<Option<Wanted>>>);

    impl SensorSource for Told {
        fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
            self.0.catalog()
        }

        fn sample(&mut self) -> Result<Snapshot> {
            self.0.sample()
        }

        fn want(&mut self, wanted: &Wanted) {
            *self.1.lock().unwrap() = Some(wanted.clone());
        }
    }

    /// D-2026-09-30-release-polish-11: the preview's theme and the sensor
    /// list say what they show; the ping is wanted only while one shows it.
    #[test]
    fn the_theme_and_the_sensor_list_say_what_they_show() {
        let told = Arc::new(Mutex::new(None));
        let mut s = studio();
        s.sensors = Box::new(Told(FakeSensors::demo(), Arc::clone(&told)));
        let last = |s: &mut Studio| {
            s.sample().unwrap();
            told.lock().unwrap().clone().unwrap()
        };
        let key = |k: &str| SensorKey::new(k).unwrap();
        let ping = || -> Wanted { [key("net.ping")].into_iter().collect() };
        assert_eq!(last(&mut s), Wanted::nothing(), "a blank theme, no list");

        s.show_sensors([key("net.ping"), key("cpu.usage")].into_iter().collect());
        assert!(last(&mut s).contains("net.ping"));
        s.set_language(Language::PortugueseBr);
        assert!(last(&mut s).contains("net.ping"), "kept by the new runtime");
        s.show_sensors(Wanted::nothing());
        assert_eq!(last(&mut s), Wanted::nothing(), "the list is hidden");

        // The preview shows a theme that prints the ping.
        let mut theme = s.theme().clone();
        theme.elements.push(Element {
            id: ElementId(1),
            name: "ping".into(),
            frame: BoxF::new(0.0, 0.0, 100.0, 40.0),
            opacity: 1.0,
            visible: true,
            locked: false,
            kind: ElementKind::Bar {
                binding: Binding {
                    key: key("net.ping"),
                    min: 0.0,
                    max: 100.0,
                },
                direction: Default::default(),
                fill: bezel_core::domain::theme::Paint::solid(Rgba::WHITE),
                track: None,
                radius: 0.0,
                segments: None,
            },
        });
        s.set_theme(theme);
        assert_eq!(last(&mut s), ping());
        assert_eq!(s.wanted(), &ping());
    }

    /// Counts the samples of the demo sensors.
    struct Counted(FakeSensors, Arc<Mutex<usize>>);

    impl SensorSource for Counted {
        fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
            self.0.catalog()
        }

        fn sample(&mut self) -> Result<Snapshot> {
            *self.1.lock().unwrap() += 1;
            self.0.sample()
        }
    }

    #[test]
    fn a_video_decoded_here_sets_the_pace_and_samples_keep_theirs() {
        let (mut s, _, dir) = decoding("pace", FakeMedia::ready());
        let samples = Arc::new(Mutex::new(0));
        s.sensors = Box::new(Counted(FakeSensors::demo(), Arc::clone(&samples)));
        let (connector, link) = weact();
        go_live(&mut s, link).unwrap();
        for k in 1..=20 {
            tick(&mut s, 100 * k).unwrap();
        }
        assert_eq!(connector.log().frames.len(), 21);
        assert_eq!(*samples.lock().unwrap(), 2, "one per second of the theme");
        s.stop_live();
        for k in 0..3 {
            tick(&mut s, 2_100 + 1_000 * k).unwrap();
        }
        assert_eq!(*samples.lock().unwrap(), 5, "every refresh again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A link whose frames never reach the screen.
    struct Unplugged(Box<dyn ScreenLink>);

    impl ScreenLink for Unplugged {
        fn identity(&self) -> &bezel_core::domain::screen::ScreenIdentity {
            self.0.identity()
        }
        fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
            self.0.set_brightness(brightness)
        }
        fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
            self.0.set_orientation(orientation)
        }
        fn present(&mut self, _: &Frame) -> Result<()> {
            Err(BezelError::Transport("the cable is out".into()))
        }
        fn screen_off(&mut self) -> Result<()> {
            self.0.screen_off()
        }
        fn release(&mut self) -> Result<()> {
            self.0.release()
        }
    }

    #[test]
    fn the_link_leaves_the_session_while_the_screen_shows_a_frame() {
        let connector = FakeConnector::default();
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        s.go_live("k".into(), link);
        let now = at(&s, 0);
        let mut delivery = s.frame_for_screen(TIME, now).unwrap().unwrap();
        assert!(s.presenting());
        assert!(
            s.frame_for_screen(TIME, now).unwrap().is_none(),
            "one frame at a time"
        );
        let busy = s.live_brightness("k", Brightness::MAX).unwrap_err();
        assert!(busy.to_string().contains(LIVE_FRAME), "{busy}");
        assert!(s.lend_live_link("k").is_err());
        // Previews render meanwhile.
        s.render(TIME).unwrap();
        delivery.present().unwrap();
        assert!(s.presented(delivery, &Ok(()), now).is_none(), "taken back");
        assert!(!s.presenting());
        assert!(s.live_brightness("k", Brightness::MAX).unwrap());
        let log = connector.log();
        assert_eq!(log.orientations, vec![Orientation::ReversePortrait]);
        tick(&mut s, 0).unwrap();
        assert_eq!(connector.log().orientations.len(), 1, "turned once");

        // Live mode stopped while the frame was out: the link comes back to
        // be closed.
        let delivery = s.frame_for_screen(TIME, now).unwrap().unwrap();
        assert!(s.stop_live().is_none());
        assert!(s.presented(delivery, &Ok(()), now).is_some());
        assert_eq!(s.live_key(), None);
    }

    #[test]
    fn a_frame_the_screen_refuses_stops_the_live_mode() {
        let connector = FakeConnector::default();
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        let error = go_live(&mut s, Box::new(Unplugged(link))).unwrap_err();
        assert!(error.to_string().contains("cable"), "{error}");
        assert_eq!(s.live_key(), None);
        assert_eq!(s.live_error().unwrap().code(), "transport");
        assert!(s.live_error().unwrap().to_string().contains("cable"));
        assert!(s.frame_for_screen(TIME, at(&s, 0)).unwrap().is_none());
    }

    #[test]
    fn a_lent_link_comes_back_and_the_video_starts_again() {
        let stored = RemotePath::parse("internal/video/clip.mp4").unwrap();
        let connector =
            FakeConnector::with_storage(FakeStorage::default().with_file(stored, vec![1; 64]));
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        s.set_theme(with_video(theme_88(), "assets/clip.mp4"));
        go_live(&mut s, link).unwrap();
        assert!(s.lend_live_link("other").unwrap().is_none(), "not live");
        let lent = s.lend_live_link("k").unwrap().unwrap();
        assert!(s.lend_live_link("k").is_err(), "lent once");
        assert!(
            s.frame_for_screen(TIME, at(&s, 0)).unwrap().is_none(),
            "frames pause"
        );
        assert!(s.return_live_link("other", lent, Resume::Video).is_some());
        let lent = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        assert!(s.return_live_link("k", lent, Resume::Video).is_none());
        let plays = |c: &FakeConnector| {
            c.log()
                .storage
                .calls
                .iter()
                .filter(|c| matches!(c, bezel_devices::fake::StorageCall::PlayVideo(..)))
                .count()
        };
        assert_eq!(plays(&connector), 1);
        present(&mut s).unwrap();
        assert_eq!(plays(&connector), 2, "started again after the job");
    }

    /// The fake 8.8" as discovered, with a link from `connector`.
    fn screen_88(connector: &FakeConnector) -> (Screen, Box<dyn ScreenLink>) {
        let found = discover_screens(&FakeBus::turing_88()).unwrap();
        let screen = choose_screen(found, None).unwrap();
        let link = connector.connect(&screen).unwrap();
        (screen, link)
    }

    /// D-2026-10-01-live-screen-controls-3: the live 8.8" is known by its
    /// display's port and by its MCU's: either reaches its open link (the
    /// brightness, a storage job's loan and its return, the video it lacks),
    /// another port does not. Gone live without being discovered, a screen
    /// is known by its key alone.
    #[test]
    fn the_live_screen_is_known_by_either_of_its_ports() {
        const DISPLAY: &str = "/dev/ttyACM1";
        const MCU: &str = "/dev/ttyACM0";
        let media: SharedMedia = Arc::new(Mutex::new(Box::new(FakeMedia::ready())));
        let dir = std::env::temp_dir().join(format!("bezel-studio-ports-{}", std::process::id()));
        let mut s = studio().with_host_decoding(media, dir.clone());
        let clip = AssetRef("assets/clip.mp4".into());
        let assets = BTreeMap::from([(clip.clone(), vec![1, 2, 3])]);
        s.start(with_video(theme_88(), &clip.0), assets, None);
        let connector = FakeConnector::default();
        let (screen, link) = screen_88(&connector);
        assert!(!s.is_live(DISPLAY), "nothing is live");
        s.go_live_on(DISPLAY.into(), screen, link);
        present(&mut s).unwrap();
        for port in [DISPLAY, MCU] {
            assert!(s.is_live(port), "{port}");
            assert!(s.live_brightness(port, Brightness::MAX).unwrap(), "{port}");
            assert_eq!(s.missing_video(port).map(|m| m.asset), Some(clip.clone()));
            let lent = s.lend_live_link(port).unwrap().expect("lent");
            assert!(s.lend_live_link(port).is_err(), "lent once");
            assert!(s.return_live_link(port, lent, Resume::Frames).is_none());
        }
        for other in ["/dev/ttyACM2", ""] {
            assert!(!s.is_live(other), "{other}");
            assert!(!s.live_brightness(other, Brightness::MAX).unwrap());
            assert_eq!(s.missing_video(other), None);
            assert!(s.lend_live_link(other).unwrap().is_none());
        }
        present(&mut s).unwrap();
        let log = connector.log();
        assert_eq!((log.connects, log.brightness.len()), (1, 2));
        assert_eq!(log.frames.len(), 2, "the link came back each time");

        drop(s.stop_live());
        assert!(!s.is_live(DISPLAY) && !s.is_live(MCU));
        let (_, link) = screen_88(&connector);
        s.go_live(MCU.into(), link);
        assert!(s.is_live(MCU) && !s.is_live(DISPLAY));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live on an 8.8" whose second frame fails with `error`: away from 1 s.
    fn failing_at_one_second(error: BezelError) -> (Studio, FakeConnector) {
        let connector = FakeConnector::default().breaking_after(1, error);
        let (screen, link) = screen_88(&connector);
        let mut s = studio();
        s.go_live_on("k".into(), screen, link);
        present(&mut s).unwrap();
        assert!(tick(&mut s, 1_000).is_err());
        (s, connector)
    }

    fn hung() -> BezelError {
        BezelError::Hung("stalled".into())
    }

    /// T-7.11: a failed link is dropped and the screen connected again 2 s
    /// later, outside the session; frames go on at once on the new link.
    #[test]
    fn a_failed_link_is_connected_again_and_frames_go_on() {
        let (mut s, connector) = failing_at_one_second(hung());
        assert_eq!(s.live_key(), Some("k"), "still live");
        assert_eq!(s.live_error(), None);
        let away = Reconnecting {
            attempt: 1,
            attempts: 3,
        };
        assert_eq!(s.reconnecting(), Some(away));
        assert_eq!(s.next_due(), at(&s, 2_000), "the next sample first");
        let busy = s.lend_live_link("k").err().unwrap();
        assert!(busy.to_string().contains(RECONNECTING), "{busy}");
        assert!(s.frame_for_screen(TIME, at(&s, 1_500)).unwrap().is_none());

        assert!(s.reconnect_due(at(&s, 2_999)).is_none(), "2 s after");
        let attempt = s.reconnect_due(at(&s, 3_000)).expect("due");
        assert!(s.reconnect_due(at(&s, 3_000)).is_none(), "one at a time");
        let outcome = reopen_screen(&FakeBus::turing_88(), &connector, attempt.screen());
        assert!(s.reconnected(attempt, outcome, at(&s, 3_100)).is_none());
        assert_eq!(s.reconnecting(), None);
        tick(&mut s, 3_100).unwrap();
        let log = connector.log();
        assert_eq!((log.connects, log.frames.len()), (2, 2));
        assert_eq!(log.orientations.len(), 2, "turned again on the new link");
    }

    /// After the third attempt (2, 5 and 10 s apart) live mode stops with the
    /// error that stopped the link; a failure that would repeat stops it at
    /// the first.
    #[test]
    fn after_the_last_attempt_live_mode_stops_with_the_first_error() {
        let (mut s, connector) = failing_at_one_second(hung());
        let gone = FakeBus::new(Vec::new());
        let mut now = 1_000;
        for wait in [2_000, 5_000, 10_000] {
            now += wait;
            assert!(s.reconnect_due(at(&s, now - 1)).is_none());
            let attempt = s.reconnect_due(at(&s, now)).expect("due");
            let outcome = reopen_screen(&gone, &connector, attempt.screen());
            assert!(s.reconnected(attempt, outcome, at(&s, now)).is_none());
        }
        assert_eq!((s.live_key(), s.reconnecting()), (None, None));
        assert_eq!(s.live_error().unwrap().code(), "hung");

        let (mut s, _) = failing_at_one_second(hung());
        let attempt = s.reconnect_due(at(&s, 3_000)).expect("due");
        let denied = BezelError::AccessDenied {
            address: "/dev/ttyACM1".into(),
            reason: "denied".into(),
        };
        assert!(s.reconnected(attempt, Err(denied), at(&s, 3_000)).is_none());
        assert_eq!(s.live_error().unwrap().code(), "hung");

        // A link that fails for good (a frame of the wrong size) stops it at once.
        let (s, _) = failing_at_one_second(BezelError::InvalidInput("size".into()));
        assert_eq!((s.live_key(), s.reconnecting()), (None, None));
    }

    /// Turning live mode off while the screen is away ends it at once: an
    /// attempt under way hands its link back to be closed.
    #[test]
    fn stopping_live_mode_while_away_drops_the_attempt() {
        let (mut s, connector) = failing_at_one_second(hung());
        let attempt = s.reconnect_due(at(&s, 5_000)).expect("due");
        assert!(s.stop_live().is_none(), "no link while away");
        let outcome = reopen_screen(&FakeBus::turing_88(), &connector, attempt.screen());
        assert!(s.reconnected(attempt, outcome, at(&s, 5_100)).is_some());
        assert_eq!((s.live_key(), s.live_error()), (None, None));
        assert!(s.reconnect_due(at(&s, 60_000)).is_none());
    }

    /// What the theme's own poster shows (before a new one is taken).
    const OLD_POSTER: Rgba = Rgba::opaque(9, 9, 9);
    /// What [`Backdrops`] draws for the poster.
    const POSTER_SHOWN: Rgba = Rgba::opaque(200, 0, 100);

    /// Draws only what is under the elements: the video's framed picture,
    /// [`POSTER_SHOWN`] for the poster.
    struct Backdrops;

    impl FrameRenderer for Backdrops {
        fn render(
            &mut self,
            theme: &Theme,
            _: &BTreeMap<AssetRef, Vec<u8>>,
            context: RenderContext<'_>,
        ) -> Result<Frame> {
            Ok(match context.backdrop {
                Backdrop::Frame(picture) => picture.clone(),
                Backdrop::Poster => Frame::filled(theme.canvas, POSTER_SHOWN),
                Backdrop::OnDevice => Frame::filled(theme.canvas, Rgba::BLACK),
            })
        }
    }

    const DRAGON: &str = "assets/dragon.mp4";
    const DRAGON_POSTER: &str = "assets/poster-195.png";

    /// The Dragon Ball case: a 1920x480 theme for the 8.8" whose video is
    /// the vendor's pre-turned 480x1920 `dragon.mp4`, with its poster; the
    /// session probes (done) and decodes with `media`, copies in a fresh
    /// folder.
    fn dragon_ball(name: &str, media: FakeMedia) -> (Studio, PathBuf) {
        let dir = std::env::temp_dir().join(format!("bezel-studio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let shared: SharedMedia = Arc::new(Mutex::new(Box::new(media)));
        let mut theme = Theme::blank("Dragon Ball", Size::new(480, 1920), Orientation::Landscape);
        theme.background = Background::Video {
            asset: AssetRef(DRAGON.into()),
            poster: Some(AssetRef(DRAGON_POSTER.into())),
            framing: None,
        };
        let poster = png_of(&Frame::filled(Size::new(1920, 480), OLD_POSTER)).unwrap();
        let assets = BTreeMap::from([
            (AssetRef(DRAGON.into()), vec![9; 4096]),
            (AssetRef(DRAGON_POSTER.into()), poster),
        ]);
        let mut s = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(Backdrops),
            Language::English,
            theme.clone(),
        )
        .with_host_decoding(shared, dir.clone());
        s.start(theme, assets, None);
        learn(&mut s);
        (s, dir)
    }

    /// Saves at `place` as the backend does: the poster taken again first,
    /// outside the session (here, in its place).
    fn save(s: &mut Studio, store: &dyn ThemeStore, place: &ThemeLocation) {
        learn(s);
        if let Some(taken) = s.poster_to_take().and_then(PosterRetake::run) {
            assert!(s.poster_taken(taken), "the theme did not change meanwhile");
        }
        s.save(store, place.clone()).unwrap();
    }

    /// The edited theme with its video framed by `framing`.
    fn framed(s: &Studio, framing: Option<VideoFraming>) -> Theme {
        let mut theme = s.theme().clone();
        if let Background::Video { framing: f, .. } = &mut theme.background {
            *f = framing;
        }
        theme
    }

    /// Fit at 125 %, a little above the middle.
    fn fit_125() -> VideoFraming {
        VideoFraming {
            fit: VideoFit::Contain,
            zoom: Zoom::from_percent(125),
            position: FramingPosition {
                x: Permille::CENTER,
                y: Permille::from_permille(400),
            },
            ..VideoFraming::default()
        }
    }

    /// D-2026-10-01-video-background-framing-5: the preview decodes the raw
    /// source at most 15 pictures a second and the runtime turns each one
    /// upright; a framing edit shows at the next picture, same decoder.
    #[test]
    fn the_preview_plays_the_framed_video_at_most_15_fps() {
        let media = FakeMedia::ready().two_tone();
        let (streamed, asked) = (Arc::clone(&media.streamed), Arc::clone(&media.asked));
        let (mut s, dir) = dragon_ball("plays", media);
        assert_eq!(s.video_auto(), Some((3, Some(Size::new(480, 1920)))));

        let (frame, next) = s.preview(TIME, at(&s, 0), Motion::Allowed).unwrap();
        assert_eq!(frame.size(), Size::new(1920, 480));
        // The top of the pre-turned video is the theme's left: upright.
        assert_eq!(frame.pixel(100, 240), Some(STREAMED));
        assert_eq!(frame.pixel(1800, 240), Some(STREAMED_END));
        assert_eq!(next, Some(Duration::from_millis(67)), "1/15 s, rounded up");
        let (source, spec) = streamed.lock().unwrap()[0].clone();
        assert!(source.0.ends_with("dragon.mp4"), "{}", source.0);
        assert_eq!(
            (spec.size, spec.fps),
            (Size::new(480, 1920), PREVIEW_FPS),
            "the raw source, unturned"
        );

        // The UI asks for the next picture when it is due: 15 in a second,
        // each a new picture of the video.
        asked.lock().unwrap().clear();
        let mut now = 0;
        let mut shown = 0;
        while now < 1_000 {
            let (_, next) = s.preview(TIME, at(&s, now), Motion::Allowed).unwrap();
            let next = next.unwrap();
            assert!(next <= Duration::from_millis(67), "{next:?}");
            now += u64::try_from(next.as_millis()).unwrap();
            shown += 1;
        }
        assert_eq!(shown, 15);
        let pictures: Vec<u128> = asked
            .lock()
            .unwrap()
            .iter()
            .map(|t| t.as_nanos() * u128::from(PREVIEW_FPS) / 1_000_000_000)
            .collect();
        assert_eq!(pictures, (0..15).collect::<Vec<u128>>());

        // Zoomed to 200 % on the left edge: the video's top half fills the
        // canvas at the next picture, from the same decoder.
        let zoomed = VideoFraming {
            zoom: Zoom::from_percent(200),
            position: FramingPosition {
                x: Permille::START,
                y: Permille::CENTER,
            },
            ..VideoFraming::default()
        };
        s.set_theme(framed(&s, Some(zoomed)));
        let (frame, _) = s.preview(TIME, at(&s, now), Motion::Allowed).unwrap();
        assert_eq!(frame.pixel(1800, 240), Some(STREAMED));
        assert_eq!(streamed.lock().unwrap().len(), 1, "never restarted");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// D-2026-10-01-video-background-framing-5: no picture asked for during
    /// 2 s ends the decoder (on the injected clock), the next one starts it
    /// again where the clock says; motion reduced ends it at once, another
    /// video starts over.
    #[test]
    fn the_preview_decoder_stops_when_no_frame_is_asked() {
        let media = FakeMedia::ready();
        let streamed = Arc::clone(&media.streamed);
        let (asked, decoding) = (Arc::clone(&media.asked), Arc::clone(&media.decoding));
        let (mut s, dir) = dragon_ball("idle", media);
        let running = || decoding.load(Ordering::SeqCst);
        // A storage job holds the converter: the poster, asked again soon.
        let converter = Arc::clone(&s.host.as_ref().unwrap().media);
        {
            let _job = converter.lock().unwrap();
            let (frame, next) = s.preview(TIME, at(&s, 0), Motion::Allowed).unwrap();
            assert_eq!(frame.pixel(960, 240), Some(POSTER_SHOWN));
            assert_eq!(next, Some(CONVERTER_BUSY));
            assert_eq!(running(), 0);
        }
        s.preview(TIME, at(&s, 0), Motion::Allowed).unwrap();
        assert!(s.previewing());
        assert_eq!(running(), 1);
        tick(&mut s, 1_999).unwrap();
        assert!(s.previewing(), "asked less than 2 s ago");
        tick(&mut s, 2_000).unwrap();
        assert!(!s.previewing());
        assert_eq!(running(), 0, "its ffmpeg ends");

        let (frame, _) = s.preview(TIME, at(&s, 5_000), Motion::Allowed).unwrap();
        assert_eq!(frame.pixel(960, 240), Some(STREAMED));
        assert_eq!((streamed.lock().unwrap().len(), running()), (2, 1));
        assert_eq!(
            asked.lock().unwrap().last(),
            Some(&Duration::from_secs(1)),
            "5 s on the clock, into a 2 s video"
        );

        let (frame, next) = s.preview(TIME, at(&s, 5_100), Motion::Reduced).unwrap();
        assert_eq!((frame.pixel(960, 240), next), (Some(POSTER_SHOWN), None));
        assert_eq!(running(), 0, "motion reduced: no decoder");
        assert_eq!(streamed.lock().unwrap().len(), 2);

        // A framing edit keeps the decoder; another video starts over.
        s.preview(TIME, at(&s, 5_200), Motion::Allowed).unwrap();
        s.set_theme(framed(&s, Some(fit_125())));
        s.preview(TIME, at(&s, 5_300), Motion::Allowed).unwrap();
        assert_eq!((streamed.lock().unwrap().len(), running()), (3, 1));
        let other = s.add_asset("dragon.mp4", vec![7; 2048]);
        let mut theme = s.theme().clone();
        theme.background = Background::Video {
            asset: other.clone(),
            poster: None,
            framing: None,
        };
        s.set_theme(theme);
        // Not probed yet: the poster, asked again soon.
        let (_, next) = s.preview(TIME, at(&s, 5_400), Motion::Allowed).unwrap();
        assert_eq!(next, Some(CONVERTER_BUSY));
        learn(&mut s);
        s.preview(TIME, at(&s, 5_400), Motion::Allowed).unwrap();
        assert_eq!((streamed.lock().unwrap().len(), running()), (4, 1));
        assert!(streamed.lock().unwrap()[3].0.0.ends_with("dragon-2.mp4"));
        assert_eq!(asked.lock().unwrap().last(), Some(&Duration::ZERO));
        drop(s);
        assert_eq!(running(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// D-2026-10-01-video-background-framing-5: without ffmpeg the preview
    /// shows the poster and nothing moves; Auto still knows the video (an
    /// MP4's header needs no ffmpeg).
    #[test]
    fn without_ffmpeg_the_preview_shows_the_poster() {
        let media = FakeMedia::missing();
        let streamed = Arc::clone(&media.streamed);
        let (mut s, dir) = dragon_ball("no-ffmpeg-preview", media);
        let (frame, next) = s.preview(TIME, at(&s, 0), Motion::Allowed).unwrap();
        assert_eq!((frame.pixel(960, 240), next), (Some(POSTER_SHOWN), None));
        assert!(!s.previewing());
        assert!(streamed.lock().unwrap().is_empty());
        assert_eq!(s.video_auto(), Some((3, Some(Size::new(480, 1920)))));
        // A theme without a video has no Auto.
        s.start(theme_88(), BTreeMap::new(), None);
        assert_eq!(s.video_auto(), None);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// D-2026-10-01-video-background-framing-3: saving a theme whose framing
    /// is not the one its poster shows takes the poster again, framed, under
    /// the same name; without ffmpeg the poster stays.
    #[test]
    fn saving_retakes_the_poster_with_the_framing() {
        let media = FakeMedia::ready();
        let posters = Arc::clone(&media.posters);
        let (mut s, dir) = dragon_ball("retake", media);
        let store = MemoryStore::default();
        let place = ThemeLocation("mem://dragon".into());
        let poster = AssetRef(DRAGON_POSTER.into());
        let before = s.assets()[&poster].clone();
        save(&mut s, &store, &place);
        assert!(posters.lock().unwrap().is_empty(), "it shows the framing");

        s.set_theme(framed(&s, Some(fit_125())));
        save(&mut s, &store, &place);
        let taken = posters.lock().unwrap().clone();
        assert_eq!(taken.len(), 1);
        assert!(taken[0].0.0.ends_with("dragon.mp4"), "{}", taken[0].0.0);
        let info = s.runtime.video_info().cloned().unwrap();
        let wanted = fit_125().resolve(info.dimensions, Orientation::Landscape, s.video_panel());
        assert_eq!(wanted.turns, 3, "Auto stands the video upright");
        let spec = PosterSpec::framed(Size::new(1920, 480), &info, &wanted);
        assert_eq!(taken[0].1, spec);
        assert_eq!(spec.quarter_turns, 3);
        assert!(spec.crop.is_some(), "zoomed in: part of the picture");
        let (saved, assets) = store.load(&place).unwrap();
        assert_eq!(saved, *s.theme(), "the theme names the same poster");
        assert_ne!(assets[&poster], before, "a new picture");
        let picture = image::load_from_memory(&assets[&poster])
            .unwrap()
            .to_rgba8();
        assert_eq!(picture.dimensions(), (1920, 480));

        // Saved again: the poster already shows that framing.
        save(&mut s, &store, &place);
        assert_eq!(posters.lock().unwrap().len(), 1);
        // Back to Auto: taken again.
        s.set_theme(framed(&s, None));
        save(&mut s, &store, &place);
        assert_eq!(posters.lock().unwrap().len(), 2);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);

        // Without ffmpeg the poster stays as it is, and the theme is saved.
        let (mut s, dir) = dragon_ball("retake-no-ffmpeg", FakeMedia::missing());
        s.set_theme(framed(&s, Some(fit_125())));
        let place = ThemeLocation("mem://dragon-2".into());
        save(&mut s, &store, &place);
        let (saved, assets) = store.load(&place).unwrap();
        assert_eq!(saved, *s.theme());
        assert_eq!(assets[&poster], before);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A decoder that fails (ffmpeg stopped) is tried again after
    /// [`PREVIEW_RETRY`] by itself, [`PREVIEW_ATTEMPTS`] in a row at most;
    /// then the poster stays until a render asks again (an edit). A decoder
    /// that shows a picture counts the failures from one again.
    #[test]
    fn a_failed_preview_decoder_is_tried_again_a_few_times() {
        let media = FakeMedia::ready().failing(3);
        let (streamed, failing) = (Arc::clone(&media.streamed), Arc::clone(&media.failing));
        let (mut s, dir) = dragon_ball("retry", media);
        let preview = |s: &mut Studio, ms, motion| {
            let (frame, next) = s.preview(TIME, at(s, ms), motion).unwrap();
            (frame.pixel(960, 240), next)
        };
        let retry = Some(PREVIEW_RETRY);
        assert_eq!(
            preview(&mut s, 0, Motion::Allowed),
            (Some(POSTER_SHOWN), retry)
        );
        assert!(!s.previewing());
        let (_, next) = preview(&mut s, 500, Motion::Allowed);
        assert_eq!(next, Some(Duration::from_millis(1_500)), "none before then");
        assert_eq!(preview(&mut s, 2_000, Motion::Allowed).1, retry);
        let (shown, next) = preview(&mut s, 4_000, Motion::Allowed);
        assert_eq!((shown, next), (Some(POSTER_SHOWN), None), "3 in a row");
        assert_eq!(streamed.lock().unwrap().len(), 3);

        // The next render (an edit) still tries one: it plays.
        let (shown, next) = preview(&mut s, 6_000, Motion::Allowed);
        assert_eq!(
            (shown, next),
            (Some(STREAMED), Some(Duration::from_millis(67)))
        );
        failing.store(1, Ordering::SeqCst);
        preview(&mut s, 6_100, Motion::Reduced);
        assert_eq!(preview(&mut s, 6_200, Motion::Allowed).1, retry, "one");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Work handed out of the session to run outside its lock (a probe, a
    /// poster) comes back only to the video, poster and framing it was for.
    #[test]
    fn a_probe_or_a_poster_comes_back_only_to_what_it_was_for() {
        let (mut s, dir) = dragon_ball("recheck", FakeMedia::ready());
        let (theme, assets) = (s.theme().clone(), s.assets().clone());
        // Another theme opened meanwhile, its video of the same name.
        s.start(theme.clone(), assets.clone(), None);
        let probe = s.video_to_probe().expect("not probed");
        s.start(theme, assets, None);
        s.probed(probe.run(Wait::Yes).unwrap());
        assert!(!s.video_known(), "the probe was for the other one");
        learn(&mut s);
        assert_eq!(s.video_auto(), Some((3, Some(Size::new(480, 1920)))));

        // The framing changed again while the poster was taken.
        let poster = AssetRef(DRAGON_POSTER.into());
        let before = s.assets()[&poster].clone();
        s.set_theme(framed(&s, Some(fit_125())));
        let taken = s.poster_to_take().and_then(PosterRetake::run).unwrap();
        s.set_theme(framed(&s, None));
        assert!(!s.poster_taken(taken));
        assert_eq!(s.assets()[&poster], before);
        assert!(s.poster_to_take().is_none(), "the poster shows Auto");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
