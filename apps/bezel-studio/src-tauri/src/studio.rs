//! The editing session behind the window: the core's [`ThemeRuntime`] with
//! the theme being edited, its assets, the latest readings and the graph
//! histories, and the screen showing it live. It is the same runtime as
//! `bezel run` (D-2026-09-30-studio-app-4): every refresh samples through
//! it, the live screen gets what it renders, and every committed edit swaps
//! the theme in place with the histories kept. Everything goes through the
//! core's ports, so the whole session runs on fakes in tests.
//!
//! A theme with a video background (D-2026-09-30-storage-video-4): the
//! preview always shows the poster; the live screen gets what the runtime's
//! [`ThemeRuntime::start_video`] chose: the stored video looping under
//! overlays, the video decoded on this computer for screens that cannot play
//! videos (a copy of the asset is decoded by the media converter the storage
//! tab shares), or the poster ([`VideoState::VideoMissing`] carries what
//! sending it takes).
//!
//! The screen's I/O happens outside the session: a frame is rendered in the
//! session, then the live link leaves it with the frame ([`Delivery`]) and
//! comes back once the screen showed it ([`Studio::presented`]). Previews
//! render meanwhile; whoever needs the link waits for it
//! ([`Studio::presenting`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use bezel_core::app::{HOST_VIDEO_FPS, HostVideo, MissingVideo, ThemeRuntime, VideoState};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::screen::Brightness;
use bezel_core::domain::sensor::{Quantities, SensorInfo, Snapshot};
use bezel_core::domain::theme::{AssetRef, Background, Theme, refresh_interval};
use bezel_core::ports::{
    Backdrop, FrameRenderer, MediaLocation, MediaTranscoder, ScreenLink, SensorSource,
    ThemeLocation, ThemeStore,
};
use bezel_core::{BezelError, Result};

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
        if let Err(e) = std::fs::remove_file(&self.0) {
            tracing::warn!(file = %self.0.display(), "copy of the theme video not removed: {e}");
        }
    }
}

/// The video decoded on this computer for the live screen.
struct HostPlayback {
    /// The file the converter reads (kept while it plays).
    _copy: VideoCopy,
    started: Instant,
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
}

impl Slot {
    fn link(&mut self) -> Option<&mut Box<dyn ScreenLink>> {
        match self {
            Slot::Here(link) => Some(link),
            Slot::Presenting | Slot::Lent => None,
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
    slot: Slot,
    orientation: Option<Orientation>,
    /// Start the video (again) before the next frame: after going live, after
    /// a theme with another video, after a job that changed what plays.
    restart_video: bool,
    /// The video decoded here ([`VideoState::Host`]).
    host: Option<HostPlayback>,
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
    sensors: Box<dyn SensorSource>,
    renderer: Box<dyn FrameRenderer>,
    language: Language,
    runtime: ThemeRuntime,
    catalog: Vec<SensorInfo>,
    sample_millis: f64,
    /// Refreshes since the last sample (a video decoded here refreshes more
    /// often than the theme samples).
    unsampled: u32,
    location: Option<ThemeLocation>,
    live: Option<Live>,
    live_error: Option<UiError>,
    host: Option<HostDecoding>,
}

impl Studio {
    /// A session editing `theme`. Without [`Self::with_host_decoding`] a
    /// screen that cannot play videos shows the poster.
    pub fn new(
        sensors: Box<dyn SensorSource>,
        renderer: Box<dyn FrameRenderer>,
        language: Language,
        theme: Theme,
    ) -> Self {
        Self {
            sensors,
            renderer,
            language,
            runtime: ThemeRuntime::new(theme, BTreeMap::new(), language),
            catalog: Vec::new(),
            sample_millis: 0.0,
            unsampled: 0,
            location: None,
            live: None,
            live_error: None,
            host: None,
        }
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
        if language == self.language {
            return;
        }
        self.language = language;
        let theme = self.runtime.theme().clone();
        let assets = self.runtime.assets().clone();
        // The old runtime, and a video it decodes here, stop first.
        self.runtime = ThemeRuntime::new(theme, assets, language);
        self.runtime.use_catalog(&self.catalog);
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

    /// Takes a sample and records it in the graph histories.
    pub fn sample(&mut self) -> Result<()> {
        let started = Instant::now();
        self.runtime.sample(self.sensors.as_mut())?;
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
    }

    /// Starts a new document.
    pub fn start(
        &mut self,
        theme: Theme,
        assets: BTreeMap<AssetRef, Vec<u8>>,
        location: Option<ThemeLocation>,
    ) {
        let before = self.runtime.video().clone();
        self.runtime.replace(theme, assets);
        self.video_changed(&before);
        self.location = location;
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

    /// Saves the theme (with only the assets it uses) at `location`.
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

    // ------------------------------------------------------------- frames --

    /// Renders the edited theme with the latest readings, as the preview
    /// shows it (a video background shows its poster).
    pub fn render(&mut self, time: LocalTime) -> Result<Frame> {
        self.runtime
            .render_with(self.renderer.as_mut(), time, Backdrop::Poster)
    }

    /// Key of the screen showing the theme, if any.
    pub fn live_key(&self) -> Option<&str> {
        self.live.as_ref().map(|l| l.key.as_str())
    }

    /// Why the live screen stopped, until the next `go_live`.
    pub fn live_error(&self) -> Option<&UiError> {
        self.live_error.as_ref()
    }

    /// How the theme's video background reaches the live screen (`None`
    /// when no screen is live).
    pub fn live_video(&self) -> Option<&VideoState> {
        self.live.as_ref().map(|_| self.runtime.video())
    }

    /// The theme video the live screen `key` could play but does not store.
    pub fn missing_video(&self, key: &str) -> Option<MissingVideo> {
        self.live.as_ref().filter(|l| l.key == key)?;
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
    }

    /// The live link of `key`, or why it cannot be had.
    fn link_of(&mut self, key: &str) -> Result<Option<&mut Box<dyn ScreenLink>>> {
        let Some(live) = self.live.as_mut().filter(|l| l.key == key) else {
            return Ok(None);
        };
        let holder = match live.slot {
            Slot::Here(_) => return Ok(live.slot.link()),
            Slot::Presenting => LIVE_FRAME,
            Slot::Lent => STORAGE_JOB,
        };
        Err(BezelError::InUse {
            address: key.to_string(),
            holders: vec![holder.to_string()],
        })
    }

    /// Lends the live link of `key` to a storage job: frames pause and the
    /// session stays usable (previews keep rendering) while the job talks to
    /// the screen. `None` when `key` is not live; `InUse` while its link is
    /// out.
    pub fn lend_live_link(&mut self, key: &str) -> Result<Option<Box<dyn ScreenLink>>> {
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
        let Some(live) = self
            .live
            .as_mut()
            .filter(|l| l.key == key && matches!(l.slot, Slot::Lent))
        else {
            return Some(link);
        };
        live.slot = Slot::Here(link);
        live.restart_video |= resume == Resume::Video;
        None
    }

    /// Shows the edited theme on `link` from the next frame on.
    pub fn go_live(&mut self, key: String, link: Box<dyn ScreenLink>) {
        self.runtime.forget_screen();
        self.live = Some(Live {
            key,
            slot: Slot::Here(link),
            orientation: None,
            restart_video: true,
            host: None,
        });
        self.live_error = None;
        self.unsampled = 0;
    }

    /// Stops showing the theme and hands back the screen's link (`None`
    /// while it is out: whoever has it closes it).
    pub fn stop_live(&mut self) -> Option<Box<dyn ScreenLink>> {
        let mut live = self.live.take()?;
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
    /// keeps the poster). A screen that cannot play videos gets it decoded
    /// here; while the converter is busy with a storage job the poster stays
    /// and the start is tried again at the next frame.
    fn start_live_video(&mut self) {
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
                let mut media = match host.media.try_lock() {
                    Ok(media) => media,
                    Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                    Err(TryLockError::WouldBlock) => return,
                };
                let media: &mut dyn MediaTranscoder = media.as_mut();
                decode_here(&mut self.runtime, link.as_mut(), media, &host.dir, &asset)
            }
        };
        live.restart_video = false;
        match started {
            Ok(playback) => live.host = playback,
            Err(e) => tracing::warn!(screen = live.key, "video background not started: {e}"),
        }
    }

    /// Renders the next frame of the live screen and takes its link out of
    /// the session to show it ([`Delivery::present`], then
    /// [`Self::presented`]). `None` while nothing is live or the link is
    /// out. A theme that does not fit the screen stops the live mode, kept
    /// for [`Self::live_error`].
    pub fn frame_for_screen(&mut self, time: LocalTime) -> Result<Option<Delivery>> {
        self.start_live_video();
        let orientation = self.runtime.theme().orientation;
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
            .map_or(Duration::ZERO, |h| h.started.elapsed());
        let frame = match self.runtime.render(self.renderer.as_mut(), time, video) {
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
    /// `outcome`. A failure stops the live mode (kept for
    /// [`Self::live_error`]). Hands the link back when it is not taken (live
    /// mode stopped meanwhile, or it failed): the caller closes it.
    pub fn presented(
        &mut self,
        delivery: Delivery,
        outcome: &Result<()>,
    ) -> Option<Box<dyn ScreenLink>> {
        let Delivery {
            key, link, turn, ..
        } = delivery;
        let Some(live) = self
            .live
            .as_mut()
            .filter(|l| l.key == key && matches!(l.slot, Slot::Presenting))
        else {
            return Some(link);
        };
        if let Err(e) = outcome {
            self.stop_with(UiError::from(e.clone()));
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
        self.live_error = Some(error);
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

    /// One refresh: a sample when one is due (every refresh, or every
    /// theme refresh while a video decoded here sets the pace), then the
    /// frame for the live screen ([`Self::frame_for_screen`]).
    pub fn tick(&mut self, time: LocalTime) -> Result<Option<Delivery>> {
        let (refresh, period) = (self.refresh().as_millis(), self.period().as_millis());
        let every = ((refresh + period / 2) / period.max(1)).max(1);
        let every = u32::try_from(every).unwrap_or(u32::MAX);
        if self.unsampled == 0
            && let Err(e) = self.sample()
        {
            tracing::warn!("sensor sample failed: {e}");
        }
        self.unsampled = (self.unsampled + 1) % every;
        self.frame_for_screen(time)
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
    use crate::storage::tests::{FakeMedia, STREAMED};
    use bezel_core::app::open_screen;
    use bezel_core::domain::device::{Transport, UsbId};
    use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::sensor::SensorKey;
    use bezel_core::domain::storage::RemotePath;
    use bezel_core::domain::theme::{
        Binding, BoxF, Element, ElementId, ElementKind, Fit, GraphStyle,
    };
    use bezel_core::ports::RenderContext;
    use bezel_devices::fake::FakeStorage;
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;

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

    /// Shows what `delivered` carries, as the backend does outside the
    /// session.
    fn show(s: &mut Studio, delivered: Result<Option<Delivery>>) -> Result<()> {
        let Some(mut delivery) = delivered? else {
            return Ok(());
        };
        let outcome = delivery.present();
        drop(s.presented(delivery, &outcome));
        outcome
    }

    fn present(s: &mut Studio) -> Result<()> {
        let delivered = s.frame_for_screen(TIME);
        show(s, delivered)
    }

    fn tick(s: &mut Studio) -> Result<()> {
        let delivered = s.tick(TIME);
        show(s, delivered)
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
        tick(&mut s).unwrap();
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
        tick(&mut s).unwrap();
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
        tick(&mut s).unwrap();
        s.render(TIME).unwrap();
        // An edit swaps the theme in place: the history goes on.
        let mut edited = theme;
        edited.name = "Edited".into();
        s.set_theme(edited);
        tick(&mut s).unwrap();
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
        tick(&mut s).unwrap();
        assert_eq!(s.live_video(), Some(&VideoState::Host));
        let copy = dir.join("Clip.mp4");
        assert_eq!(std::fs::read(&copy).unwrap(), [1, 2, 3]);
        let (source, spec) = streamed.lock().unwrap()[0].clone();
        assert_eq!(source.0, copy.display().to_string());
        assert_eq!((spec.size, spec.fps), (Size::new(80, 160), HOST_VIDEO_FPS));
        assert_eq!(s.period(), Duration::from_millis(100));
        let shown = connector.log().frames.last().cloned().unwrap();
        assert_eq!(shown.pixel(40, 80), Some(STREAMED));
        let preview = s.render(TIME).unwrap();
        assert_ne!(preview.pixel(40, 80), Some(STREAMED), "the poster");

        // Another video starts over; stopping removes the copy.
        let mut other = s.theme().clone();
        other.background = Background::Color(Rgba::BLACK);
        s.set_theme(other);
        tick(&mut s).unwrap();
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
        for _ in 0..20 {
            tick(&mut s).unwrap();
        }
        assert_eq!(connector.log().frames.len(), 21);
        assert_eq!(*samples.lock().unwrap(), 2, "one per second of the theme");
        s.stop_live();
        for _ in 0..3 {
            tick(&mut s).unwrap();
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
        let mut delivery = s.frame_for_screen(TIME).unwrap().unwrap();
        assert!(s.presenting());
        assert!(
            s.frame_for_screen(TIME).unwrap().is_none(),
            "one frame at a time"
        );
        let busy = s.live_brightness("k", Brightness::MAX).unwrap_err();
        assert!(busy.to_string().contains(LIVE_FRAME), "{busy}");
        assert!(s.lend_live_link("k").is_err());
        // Previews render meanwhile.
        s.render(TIME).unwrap();
        delivery.present().unwrap();
        assert!(s.presented(delivery, &Ok(())).is_none(), "taken back");
        assert!(!s.presenting());
        assert!(s.live_brightness("k", Brightness::MAX).unwrap());
        let log = connector.log();
        assert_eq!(log.orientations, vec![Orientation::ReversePortrait]);
        tick(&mut s).unwrap();
        assert_eq!(connector.log().orientations.len(), 1, "turned once");

        // Live mode stopped while the frame was out: the link comes back to
        // be closed.
        let delivery = s.frame_for_screen(TIME).unwrap().unwrap();
        assert!(s.stop_live().is_none());
        assert!(s.presented(delivery, &Ok(())).is_some());
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
        assert!(s.frame_for_screen(TIME).unwrap().is_none());
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
        assert!(s.frame_for_screen(TIME).unwrap().is_none(), "frames pause");
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
}
