//! Running a theme: sample the sensors, keep graph histories, render a frame
//! and show it. The caller owns the cadence (`Theme::refresh_seconds`).
//!
//! A video background (D-2026-09-30-storage-video-4) reaches a screen in one
//! of three ways, chosen by [`ThemeRuntime::start_video`] once the screen is
//! open: the screen loops the stored file and every frame is an overlay whose
//! alpha lets it show through; the host decodes the video and every frame
//! draws one of its pictures under the elements (screens that cannot play
//! stored videos); or the poster. The runtime queries and plays stored files
//! but never sends one: putting the video on the screen is an explicit user
//! action ([`MissingVideo::upload_request`]).
//!
//! An editor drives the same runtime: it swaps every edit in place
//! ([`ThemeRuntime::replace_theme`], histories kept), shows the readings of
//! the last sample ([`ThemeRuntime::snapshot`]) and previews frames over the
//! poster ([`ThemeRuntime::render_with`]) while the screen gets what
//! [`ThemeRuntime::render`] draws for it.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use crate::Result;
use crate::app::storage::{Presence, UploadRequest, presence};
use crate::domain::clock::{Language, LocalTime};
use crate::domain::frame::Frame;
use crate::domain::geometry::Orientation;
use crate::domain::history::Histories;
use crate::domain::media::{ConvertOptions, MediaKind, MediaTools, StreamSpec, UploadProfile};
use crate::domain::sensor::{Quantities, SensorInfo, Snapshot};
use crate::domain::storage::{FileName, Medium, RemotePath, Repeat, StorageLocation};
use crate::domain::theme::{AssetRef, Background, Theme};
use crate::ports::{
    Backdrop, FrameRenderer, MediaLocation, MediaTranscoder, RenderContext, ScreenLink,
    ScreenStorage, SensorSource, VideoFrames,
};

/// How the theme's video background is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoState {
    /// The theme has no video background.
    NoVideo,
    /// Not started on a screen ([`ThemeRuntime::start_video`]): frames show
    /// the poster. Previews stay here.
    NotStarted,
    /// The screen loops this stored file; frames are overlays on a
    /// transparent base (A = 0 shows the video).
    OnDevice(RemotePath),
    /// The screen can play the video but does not store it: frames show the
    /// poster until the user sends it ("Send to screen") and the video is
    /// started again.
    VideoMissing(MissingVideo),
    /// The screen cannot play stored videos: the host decodes the video and
    /// frames draw its pictures under the elements.
    Host,
    /// The screen cannot play stored videos and the host's converter is
    /// missing: frames show the poster.
    NoConverter {
        /// How to install the converter, most likely first.
        install_hints: Vec<String>,
    },
    /// The screen cannot play stored videos and no host decoding was offered
    /// (the video file is not on this computer): frames show the poster.
    NoPlayback,
}

/// A theme video the screen could play but does not store, and what sending
/// it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingVideo {
    /// The theme asset holding the video.
    pub asset: AssetRef,
    /// Where it belongs: [`device_video_name`] in the card's video folder
    /// when a card is present, else in the internal one (the vendor's
    /// choice). The runtime looks for that name in both.
    pub path: RemotePath,
    /// The conversion that fits the video to the panel: the quarter turns
    /// from the theme's orientation to the panel's native one.
    pub options: ConvertOptions,
}

impl MissingVideo {
    /// The upload that puts the video where the runtime looks for it, from
    /// `source` (the asset's file on the host). Still an explicit action:
    /// the caller runs it through `app::storage::prepare_upload` and
    /// `app::storage::upload`, then starts the video again.
    pub fn upload_request(&self, source: MediaLocation) -> UploadRequest {
        UploadRequest {
            source,
            name: self.path.name.to_string(),
            location: self.path.location,
            options: self.options,
        }
    }
}

/// Pictures per second of a video background decoded on the host (for
/// screens that cannot play videos themselves); a slow link shows fewer.
pub const HOST_VIDEO_FPS: u32 = 10;

/// Host decoding offered for screens that cannot play stored videos.
pub struct HostVideo<'a> {
    /// Decodes the video.
    pub media: &'a mut dyn MediaTranscoder,
    /// The theme's video file on the host.
    pub source: MediaLocation,
    /// Pictures per second to decode (the caller caps it to the link's rate).
    pub fps: u32,
}

impl fmt::Debug for HostVideo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostVideo")
            .field("source", &self.source)
            .field("fps", &self.fps)
            .finish_non_exhaustive()
    }
}

/// The name a theme's video has on a screen: the asset's file name, the
/// vendor's suffix for a copy turned to the panel (`_90`, `_180`, `_270`
/// clockwise) and the screen's video extension, as an upload name
/// (`assets/AMD.mp4` turned once for an MP4 screen: `amd_90.mp4`).
pub fn device_video_name(asset: &AssetRef, quarter_turns: u8, profile: &UploadProfile) -> FileName {
    let file = asset.0.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let turned = match quarter_turns % 4 {
        1 => "_90",
        2 => "_180",
        3 => "_270",
        _ => "",
    };
    let extension = profile.video_format.extensions().first().copied();
    let extension = extension.unwrap_or_default();
    FileName::suggest(&format!("{stem}{turned}.{extension}"), extension)
}

/// What a frame is drawn from: the theme with its assets and the readings.
struct Scene {
    theme: Theme,
    assets: BTreeMap<AssetRef, Vec<u8>>,
    histories: Histories,
    quantities: Quantities,
    snapshot: Snapshot,
    language: Language,
}

impl Scene {
    /// Renders the theme from the last sample over `backdrop`.
    fn draw(
        &self,
        renderer: &mut dyn FrameRenderer,
        time: LocalTime,
        backdrop: Backdrop<'_>,
    ) -> Result<Frame> {
        let context = RenderContext {
            snapshot: &self.snapshot,
            histories: &self.histories,
            quantities: &self.quantities,
            time,
            language: self.language,
            backdrop,
        };
        renderer.render(&self.theme, &self.assets, context)
    }
}

/// A theme being shown.
pub struct ThemeRuntime {
    scene: Scene,
    video: VideoState,
    /// Pictures of the host-decoded video ([`VideoState::Host`]).
    host: Option<Box<dyn VideoFrames>>,
    /// The stored video this runtime told the screen to loop.
    playing: Option<RemotePath>,
}

impl fmt::Debug for ThemeRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThemeRuntime")
            .field("theme", &self.scene.theme.name)
            .field("video", &self.video)
            .finish_non_exhaustive()
    }
}

/// The theme's video asset and orientation, when its background is a video.
fn video_of(theme: &Theme) -> Option<(&AssetRef, Orientation)> {
    match &theme.background {
        Background::Video { asset, .. } => Some((asset, theme.orientation)),
        _ => None,
    }
}

/// How a theme's video is shown before it is started on a screen.
fn unstarted(theme: &Theme) -> VideoState {
    match video_of(theme) {
        Some(_) => VideoState::NotStarted,
        None => VideoState::NoVideo,
    }
}

/// Where a screen stores the theme's video.
enum Lookup {
    /// Stored there.
    Stored(RemotePath),
    /// Not stored; it belongs there.
    Absent(RemotePath),
}

/// Looks for `name` in the internal and (with a card) the card video folder
/// with size queries only: listing a folder creates it. A file whose size
/// the screen cannot report is there.
fn find_video(storage: &mut dyn ScreenStorage, name: FileName) -> Result<Lookup> {
    let card = storage.info()?.card.is_some();
    let media: &[Medium] = if card {
        &[Medium::Internal, Medium::Card]
    } else {
        &[Medium::Internal]
    };
    let at = |medium| StorageLocation::new(medium, MediaKind::Video);
    for medium in media {
        let path = RemotePath::new(at(*medium), name.clone());
        if presence(storage, &path)? != Presence::Absent {
            return Ok(Lookup::Stored(path));
        }
    }
    let target = if card { Medium::Card } else { Medium::Internal };
    Ok(Lookup::Absent(RemotePath::new(at(target), name)))
}

impl ThemeRuntime {
    /// Starts running `theme` with its asset bytes. A video background shows
    /// its poster until [`Self::start_video`].
    pub fn new(theme: Theme, assets: BTreeMap<AssetRef, Vec<u8>>, language: Language) -> Self {
        let histories = Histories::new(&theme.history_lengths());
        let video = unstarted(&theme);
        Self {
            scene: Scene {
                theme,
                assets,
                histories,
                quantities: Quantities::default(),
                snapshot: Snapshot::default(),
                language,
            },
            video,
            host: None,
            playing: None,
        }
    }

    /// The theme being shown.
    pub fn theme(&self) -> &Theme {
        &self.scene.theme
    }

    /// The theme's asset bytes.
    pub fn assets(&self) -> &BTreeMap<AssetRef, Vec<u8>> {
        &self.scene.assets
    }

    /// Adds `asset` for the theme to use (new bytes for one it has replace
    /// them); the other assets stay.
    pub fn add_asset(&mut self, asset: AssetRef, bytes: Vec<u8>) {
        self.scene.assets.insert(asset, bytes);
    }

    /// How the theme's video background is shown.
    pub fn video(&self) -> &VideoState {
        &self.video
    }

    /// The readings of the last [`Self::sample`] (empty before the first).
    pub fn snapshot(&self) -> &Snapshot {
        &self.scene.snapshot
    }

    /// What each sensor measures (the units of sensor text): from the last
    /// [`Self::use_catalog`], else from the catalog read at the first sample.
    pub fn quantities(&self) -> &Quantities {
        &self.scene.quantities
    }

    /// Takes what each sensor measures from `catalog`, read again when
    /// sensors come and go.
    pub fn use_catalog(&mut self, catalog: &[SensorInfo]) {
        self.scene.quantities = Quantities::from_catalog(catalog);
    }

    /// Swaps in an edited theme, keeping the history of sensors still graphed.
    /// Another video (a different file or orientation) goes back to
    /// [`VideoState::NotStarted`] (or [`VideoState::NoVideo`]): start it
    /// again with [`Self::start_video`].
    pub fn replace(&mut self, theme: Theme, assets: BTreeMap<AssetRef, Vec<u8>>) {
        self.scene.assets = assets;
        self.replace_theme(theme);
    }

    /// [`Self::replace`] keeping the assets: an edit of the theme.
    pub fn replace_theme(&mut self, theme: Theme) {
        let mut histories = Histories::new(&theme.history_lengths());
        histories.adopt(&self.scene.histories);
        if video_of(&theme) != video_of(&self.scene.theme) {
            self.video = unstarted(&theme);
            self.host = None;
        }
        self.scene.theme = theme;
        self.scene.histories = histories;
    }

    /// Forgets the screen the video was started on (closed, handed back or
    /// lost): frames show the poster again ([`VideoState::NotStarted`]) and
    /// the host decoding stops. Nothing is sent: the screen is gone.
    pub fn forget_screen(&mut self) {
        self.video = unstarted(&self.scene.theme);
        self.host = None;
        self.playing = None;
    }

    /// Chooses how the theme's video reaches `screen` and starts it. Call it
    /// once the screen is open, after [`Self::replace`] changed the video and
    /// after the video was sent to the screen.
    ///
    /// - A screen that plays stored videos and has storage over this link:
    ///   [`device_video_name`] is looked for in its video folders with size
    ///   queries only (never a listing, never an upload) and looped
    ///   ([`VideoState::OnDevice`]); absent, [`VideoState::VideoMissing`].
    /// - Any other screen: the host decodes the video when `host` is offered
    ///   and its converter is ready ([`VideoState::Host`]); otherwise
    ///   [`VideoState::NoConverter`] or [`VideoState::NoPlayback`].
    ///
    /// A stored video this runtime looped and no longer shows is stopped. On
    /// error the poster stays ([`VideoState::NotStarted`]).
    pub fn start_video(
        &mut self,
        screen: &mut dyn ScreenLink,
        host: Option<HostVideo<'_>>,
    ) -> Result<&VideoState> {
        self.host = None;
        self.video = unstarted(&self.scene.theme);
        let video = match video_of(&self.scene.theme) {
            Some((asset, _)) => asset.clone(),
            None => {
                self.stop_played(screen)?;
                return Ok(&self.video);
            }
        };
        let model = screen.identity().model;
        let turns = self
            .scene
            .theme
            .orientation
            .quarter_turns_to(model.native_orientation);
        let profile = UploadProfile::for_model(model).filter(|_| model.capabilities.video_playback);
        let state = match (profile, screen.storage()) {
            (Some(profile), Some(storage)) => self.on_device(storage, video, turns, &profile)?,
            _ => self.on_host(host)?,
        };
        if !matches!(state, VideoState::OnDevice(_)) {
            self.stop_played(screen)?;
        }
        self.video = state;
        Ok(&self.video)
    }

    /// Loops the stored video, or says where it belongs.
    fn on_device(
        &mut self,
        storage: &mut dyn ScreenStorage,
        asset: AssetRef,
        quarter_turns: u8,
        profile: &UploadProfile,
    ) -> Result<VideoState> {
        let name = device_video_name(&asset, quarter_turns, profile);
        match find_video(storage, name)? {
            Lookup::Stored(path) => {
                storage.play_video(&path, Repeat::Loop)?;
                self.playing = Some(path.clone());
                Ok(VideoState::OnDevice(path))
            }
            Lookup::Absent(path) => Ok(VideoState::VideoMissing(MissingVideo {
                asset,
                path,
                options: ConvertOptions {
                    quarter_turns,
                    ..ConvertOptions::default()
                },
            })),
        }
    }

    /// Decodes the video on the host when offered and possible.
    fn on_host(&mut self, host: Option<HostVideo<'_>>) -> Result<VideoState> {
        let Some(host) = host else {
            return Ok(VideoState::NoPlayback);
        };
        if let MediaTools::Missing { install_hints } = host.media.tools() {
            return Ok(VideoState::NoConverter { install_hints });
        }
        let spec = StreamSpec {
            size: self.scene.theme.canvas,
            fps: host.fps,
        };
        self.host = Some(host.media.stream(&host.source, spec)?);
        Ok(VideoState::Host)
    }

    /// Stops the stored video this runtime looped, if any.
    fn stop_played(&mut self, screen: &mut dyn ScreenLink) -> Result<()> {
        if self.playing.take().is_some()
            && let Some(storage) = screen.storage()
        {
            storage.stop()?;
        }
        Ok(())
    }

    /// Samples the sensors and records the graph histories (once per
    /// `refresh_seconds`).
    pub fn sample(&mut self, sensors: &mut dyn SensorSource) -> Result<()> {
        if self.scene.quantities.is_empty() {
            // Units of sensor text; a catalog failure only costs the units.
            if let Ok(catalog) = sensors.catalog() {
                self.use_catalog(&catalog);
            }
        }
        let snapshot = sensors.sample()?;
        self.scene.histories.push(&snapshot);
        self.scene.snapshot = snapshot;
        Ok(())
    }

    /// Renders one frame from the last sample. `video` is how long the
    /// host-decoded video has played ([`VideoState::Host`]; ignored
    /// otherwise): a loop streaming it renders at the video's rate and
    /// samples at the theme's.
    pub fn render(
        &mut self,
        renderer: &mut dyn FrameRenderer,
        time: LocalTime,
        video: Duration,
    ) -> Result<Frame> {
        let backdrop = match (&self.video, self.host.as_mut()) {
            (VideoState::OnDevice(_), _) => Backdrop::OnDevice,
            (VideoState::Host, Some(frames)) => Backdrop::Frame(frames.frame_at(video)?),
            _ => Backdrop::Poster,
        };
        self.scene.draw(renderer, time, backdrop)
    }

    /// Renders one frame from the last sample over `backdrop`, whatever the
    /// screen shows: an editor's preview passes [`Backdrop::Poster`].
    pub fn render_with(
        &self,
        renderer: &mut dyn FrameRenderer,
        time: LocalTime,
        backdrop: Backdrop<'_>,
    ) -> Result<Frame> {
        self.scene.draw(renderer, time, backdrop)
    }

    /// Samples the sensors, records histories and renders one frame
    /// ([`Self::sample`] then [`Self::render`]; a host-decoded video shows
    /// its first picture).
    pub fn frame(
        &mut self,
        sensors: &mut dyn SensorSource,
        renderer: &mut dyn FrameRenderer,
        time: LocalTime,
    ) -> Result<Frame> {
        self.sample(sensors)?;
        self.render(renderer, time, Duration::ZERO)
    }

    /// One refresh: [`Self::frame`] then present it on `screen`.
    pub fn show(
        &mut self,
        sensors: &mut dyn SensorSource,
        renderer: &mut dyn FrameRenderer,
        screen: &mut dyn ScreenLink,
        time: LocalTime,
    ) -> Result<()> {
        let frame = self.frame(sensors, renderer, time)?;
        screen.present(&frame)
    }
}

#[cfg(test)]
mod tests {
    //! The video decisions run through the adapters' fakes in
    //! `tests/runtime_video.rs`; here only what needs no port.

    use super::*;
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;

    #[test]
    fn device_video_names_follow_the_vendor() {
        let profile = |id| {
            let model = model_by_id(ModelId(id)).expect("model");
            UploadProfile::for_model(model).expect("storage")
        };
        let (rev_c, usb) = (profile("turing-8.8"), profile("turing-usb-8.8"));
        let name = |asset: &str, turns, profile: &UploadProfile| {
            device_video_name(&AssetRef(asset.into()), turns, profile).to_string()
        };
        assert_eq!(name("assets/AMD.mp4", 0, &rev_c), "amd.mp4");
        assert_eq!(name("assets/AMD.mp4", 1, &rev_c), "amd_90.mp4");
        assert_eq!(name("assets/my.clip.MOV", 2, &rev_c), "my_clip_180.mp4");
        assert_eq!(name("assets/bg.mp4", 3, &usb), "bg_270.h264");
        assert_eq!(
            name("C:\\clips\\Fundo Azul.mp4", 4, &rev_c),
            "fundo_azul.mp4"
        );
    }

    #[test]
    fn a_missing_video_names_its_upload() {
        let missing = MissingVideo {
            asset: AssetRef("assets/Clip.MP4".into()),
            path: RemotePath::parse("sd/video/clip_90.mp4").expect("path"),
            options: ConvertOptions {
                quarter_turns: 1,
                ..ConvertOptions::default()
            },
        };
        let source = MediaLocation("/themes/clip/assets/Clip.MP4".into());
        let request = missing.upload_request(source.clone());
        assert_eq!(request.source, source);
        assert_eq!(request.name, "clip_90.mp4");
        assert_eq!(request.location, missing.path.location);
        assert_eq!(request.options, missing.options);
    }

    #[test]
    fn a_new_runtime_shows_the_poster_until_its_video_starts() {
        let plain = Theme::blank(
            "plain",
            crate::domain::geometry::Size::new(480, 1920),
            Orientation::Portrait,
        );
        let mut video = plain.clone();
        video.background = Background::Video {
            asset: AssetRef("assets/clip.mp4".into()),
            poster: None,
        };
        let mut runtime = ThemeRuntime::new(plain.clone(), BTreeMap::new(), Language::English);
        assert_eq!(runtime.video(), &VideoState::NoVideo);
        runtime.replace(video.clone(), BTreeMap::new());
        assert_eq!(runtime.video(), &VideoState::NotStarted);
        assert_eq!(runtime.theme().name, "plain");
        assert!(format!("{runtime:?}").contains("NotStarted"));
        runtime.replace(plain, BTreeMap::new());
        assert_eq!(runtime.video(), &VideoState::NoVideo);
    }
}
