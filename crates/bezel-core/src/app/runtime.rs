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

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use crate::Result;
use crate::app::storage::UploadRequest;
use crate::domain::clock::{Language, LocalTime};
use crate::domain::frame::Frame;
use crate::domain::geometry::Orientation;
use crate::domain::history::Histories;
use crate::domain::media::{ConvertOptions, MediaKind, MediaTools, StreamSpec, UploadProfile};
use crate::domain::sensor::{Quantities, Snapshot};
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

/// A theme being shown.
pub struct ThemeRuntime {
    theme: Theme,
    assets: BTreeMap<AssetRef, Vec<u8>>,
    histories: Histories,
    quantities: Quantities,
    snapshot: Snapshot,
    language: Language,
    video: VideoState,
    /// Pictures of the host-decoded video ([`VideoState::Host`]).
    host: Option<Box<dyn VideoFrames>>,
    /// The stored video this runtime told the screen to loop.
    playing: Option<RemotePath>,
}

impl fmt::Debug for ThemeRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThemeRuntime")
            .field("theme", &self.theme.name)
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
/// with size queries only: listing a folder creates it.
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
        if storage.size(&path)?.is_some() {
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
            theme,
            assets,
            histories,
            quantities: Quantities::default(),
            snapshot: Snapshot::default(),
            language,
            video,
            host: None,
            playing: None,
        }
    }

    /// The theme being shown.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// How the theme's video background is shown.
    pub fn video(&self) -> &VideoState {
        &self.video
    }

    /// Swaps in an edited theme, keeping the history of sensors still graphed.
    /// Another video (a different file or orientation) goes back to
    /// [`VideoState::NotStarted`] (or [`VideoState::NoVideo`]): start it
    /// again with [`Self::start_video`].
    pub fn replace(&mut self, theme: Theme, assets: BTreeMap<AssetRef, Vec<u8>>) {
        let mut histories = Histories::new(&theme.history_lengths());
        histories.adopt(&self.histories);
        if video_of(&theme) != video_of(&self.theme) {
            self.video = unstarted(&theme);
            self.host = None;
        }
        self.theme = theme;
        self.assets = assets;
        self.histories = histories;
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
        self.video = unstarted(&self.theme);
        let video = match video_of(&self.theme) {
            Some((asset, _)) => asset.clone(),
            None => {
                self.stop_played(screen)?;
                return Ok(&self.video);
            }
        };
        let model = screen.identity().model;
        let turns = self
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
            size: self.theme.canvas,
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
        if self.quantities.is_empty() {
            // Units of sensor text; a catalog failure only costs the units.
            if let Ok(catalog) = sensors.catalog() {
                self.quantities = Quantities::from_catalog(&catalog);
            }
        }
        let snapshot = sensors.sample()?;
        self.histories.push(&snapshot);
        self.snapshot = snapshot;
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
    use super::*;
    use crate::BezelError;
    use crate::app::storage::doubles::{Call, Screen};
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;
    use crate::domain::frame::Rgba;
    use crate::domain::geometry::Size;
    use crate::domain::job::Job;
    use crate::domain::media::{MediaInfo, TranscodeTarget};
    use crate::domain::sensor::SensorInfo;
    use crate::domain::storage::Capacity;

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 21,
        minute: 5,
        second: 0,
        weekday: 2,
    };

    /// What each frame was drawn over.
    #[derive(Default)]
    struct Recorder {
        seen: Vec<String>,
    }

    impl FrameRenderer for Recorder {
        fn render(
            &mut self,
            _: &Theme,
            _: &BTreeMap<AssetRef, Vec<u8>>,
            context: RenderContext<'_>,
        ) -> Result<Frame> {
            self.seen.push(match context.backdrop {
                Backdrop::Poster => "poster".into(),
                Backdrop::OnDevice => "on-device".into(),
                Backdrop::Frame(f) => format!("picture {}", f.pixel(0, 0).map_or(0, |p| p.r)),
            });
            Ok(Frame::filled(Size::new(1, 1), Rgba::default()))
        }
    }

    struct Quiet;

    impl SensorSource for Quiet {
        fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
            Ok(Vec::new())
        }
        fn sample(&mut self) -> Result<Snapshot> {
            Ok(Snapshot::default())
        }
    }

    /// A host decoder whose videos alternate two pictures (red 10, red 20).
    struct Decoder {
        tools: MediaTools,
        opened: Vec<(MediaLocation, StreamSpec)>,
    }

    impl Decoder {
        fn ready() -> Self {
            Self::with(MediaTools::Ready {
                version: "test".into(),
            })
        }
        fn with(tools: MediaTools) -> Self {
            Self {
                tools,
                opened: Vec::new(),
            }
        }
    }

    fn unused<T>() -> Result<T> {
        Err(BezelError::Unsupported("not in this test".into()))
    }

    impl MediaTranscoder for Decoder {
        fn tools(&mut self) -> MediaTools {
            self.tools.clone()
        }
        fn probe(&mut self, _: &MediaLocation) -> Result<MediaInfo> {
            unused()
        }
        fn transcode(
            &mut self,
            _: &MediaLocation,
            _: &TranscodeTarget,
            _: &mut Job<'_>,
        ) -> Result<MediaLocation> {
            unused()
        }
        fn load(&mut self, _: &MediaLocation) -> Result<Vec<u8>> {
            unused()
        }
        fn stream(
            &mut self,
            source: &MediaLocation,
            spec: StreamSpec,
        ) -> Result<Box<dyn VideoFrames>> {
            self.opened.push((source.clone(), spec));
            let picture = |r| Frame::filled(Size::new(1, 1), Rgba::opaque(r, 0, 0));
            Ok(Box::new(Clip {
                pictures: vec![picture(10), picture(20)],
                fps: spec.fps,
            }))
        }
    }

    struct Clip {
        pictures: Vec<Frame>,
        fps: u32,
    }

    impl VideoFrames for Clip {
        fn frame_at(&mut self, elapsed: Duration) -> Result<&Frame> {
            let n = elapsed.as_millis() * u128::from(self.fps) / 1000;
            let n = usize::try_from(n).unwrap_or(0) % self.pictures.len();
            Ok(&self.pictures[n])
        }
    }

    /// A landscape theme over `assets/Clip.MP4` for a 480x1920 panel.
    fn video_theme(asset: &str) -> Theme {
        let mut theme = Theme::blank("clip", Size::new(480, 1920), Orientation::Landscape);
        theme.background = Background::Video {
            asset: AssetRef(asset.into()),
            poster: None,
        };
        theme
    }

    fn runtime(theme: Theme) -> ThemeRuntime {
        ThemeRuntime::new(theme, BTreeMap::new(), Language::English)
    }

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).expect("path")
    }

    fn card() -> Option<Capacity> {
        Some(Capacity {
            total: 8_000_000_000,
            used: 0,
            free: 8_000_000_000,
        })
    }

    #[test]
    fn a_stored_video_is_looped_once_and_frames_become_overlays() {
        let mut screen = Screen::turing_88().with_file("internal/video/clip_90.mp4", 1000);
        let mut rt = runtime(video_theme("assets/Clip.MP4"));
        let mut r = Recorder::default();
        assert_eq!(rt.video(), &VideoState::NotStarted);
        rt.frame(&mut Quiet, &mut r, TIME).expect("preview");

        let stored = path("internal/video/clip_90.mp4");
        let state = rt.start_video(&mut screen, None).expect("start").clone();
        assert_eq!(state, VideoState::OnDevice(stored.clone()));
        let started = vec![
            Call::Info,
            Call::Size(stored.clone()),
            Call::PlayVideo(stored, Repeat::Loop),
        ];
        assert_eq!(screen.calls, started, "size queries only, then one loop");
        for _ in 0..3 {
            rt.show(&mut Quiet, &mut r, &mut screen, TIME)
                .expect("show");
        }
        assert_eq!(screen.calls, started, "started once");
        assert_eq!(r.seen, ["poster", "on-device", "on-device", "on-device"]);
        assert!(format!("{rt:?}").contains("OnDevice"));
    }

    #[test]
    fn the_card_is_searched_after_the_flash() {
        let mut screen = Screen::turing_88().with_file("sd/video/clip_90.mp4", 1000);
        screen.info.card = card();
        let mut rt = runtime(video_theme("assets/Clip.MP4"));
        let on_card = path("sd/video/clip_90.mp4");
        let state = rt.start_video(&mut screen, None).expect("start");
        assert_eq!(state, &VideoState::OnDevice(on_card.clone()));
        assert_eq!(
            screen.calls,
            [
                Call::Info,
                Call::Size(path("internal/video/clip_90.mp4")),
                Call::Size(on_card.clone()),
                Call::PlayVideo(on_card, Repeat::Loop),
            ]
        );
    }

    #[test]
    fn a_missing_video_keeps_the_poster_and_says_where_to_send_it() {
        for (with_card, target) in [
            (false, "internal/video/clip_90.mp4"),
            (true, "sd/video/clip_90.mp4"),
        ] {
            let mut screen = Screen::turing_88();
            screen.info.card = card().filter(|_| with_card);
            let mut rt = runtime(video_theme("assets/Clip.MP4"));
            let state = rt.start_video(&mut screen, None).expect("start").clone();
            let VideoState::VideoMissing(missing) = state else {
                panic!("{state:?}")
            };
            assert_eq!(missing.path, path(target));
            assert_eq!(missing.asset, AssetRef("assets/Clip.MP4".into()));
            assert_eq!(
                missing.options.quarter_turns, 1,
                "landscape on a portrait panel"
            );
            assert!(
                !screen.calls.iter().any(|c| c.changes_the_screen()),
                "nothing uploaded, played or listed: {:?}",
                screen.calls
            );
            assert!(!screen.calls.iter().any(|c| matches!(c, Call::List(_))));
            let source = MediaLocation("/themes/clip/assets/Clip.MP4".into());
            let request = missing.upload_request(source.clone());
            assert_eq!(request.source, source);
            assert_eq!(request.name, "clip_90.mp4");
            assert_eq!(request.location, path(target).location);
            assert_eq!(request.options, missing.options);

            let mut r = Recorder::default();
            rt.frame(&mut Quiet, &mut r, TIME).expect("frame");
            assert_eq!(r.seen, ["poster"]);
        }
    }

    #[test]
    fn screens_without_playback_decode_the_video_on_the_host() {
        let mut screen = Screen::turing_35();
        let theme = video_theme("assets/clip.mp4");
        let canvas = theme.canvas;
        let mut rt = runtime(theme);
        let mut media = Decoder::ready();
        let source = MediaLocation("clip.mp4".into());
        let host = HostVideo {
            media: &mut media,
            source: source.clone(),
            fps: 24,
        };
        assert!(format!("{host:?}").contains("fps: 24"));
        let state = rt.start_video(&mut screen, Some(host)).expect("start");
        assert_eq!(state, &VideoState::Host);
        let spec = StreamSpec {
            size: canvas,
            fps: 24,
        };
        assert_eq!(media.opened, [(source, spec)]);

        let mut r = Recorder::default();
        rt.sample(&mut Quiet).expect("sample");
        for ms in [0, 50, 90] {
            rt.render(&mut r, TIME, Duration::from_millis(ms))
                .expect("render");
        }
        assert_eq!(r.seen, ["picture 10", "picture 20", "picture 10"]);
        assert!(screen.calls.is_empty());
    }

    #[test]
    fn without_a_converter_or_a_source_the_poster_stays() {
        let mut screen = Screen::turing_35();
        let mut rt = runtime(video_theme("assets/clip.mp4"));
        let hints = vec!["sudo dnf install ffmpeg".to_string()];
        let mut media = Decoder::with(MediaTools::Missing {
            install_hints: hints.clone(),
        });
        let host = HostVideo {
            media: &mut media,
            source: MediaLocation("clip.mp4".into()),
            fps: 24,
        };
        let state = rt.start_video(&mut screen, Some(host)).expect("start");
        assert_eq!(
            state,
            &VideoState::NoConverter {
                install_hints: hints
            }
        );
        assert!(media.opened.is_empty());
        let state = rt.start_video(&mut screen, None).expect("start");
        assert_eq!(state, &VideoState::NoPlayback);
        let mut r = Recorder::default();
        rt.frame(&mut Quiet, &mut r, TIME).expect("frame");
        assert_eq!(r.seen, ["poster"]);
    }

    #[test]
    fn themes_without_a_video_leave_the_screen_alone() {
        let mut screen = Screen::turing_88();
        let theme = Theme::blank("plain", Size::new(480, 1920), Orientation::Portrait);
        let mut rt = runtime(theme);
        assert_eq!(rt.video(), &VideoState::NoVideo);
        let state = rt.start_video(&mut screen, None).expect("start");
        assert_eq!(state, &VideoState::NoVideo);
        assert!(screen.calls.is_empty());
    }

    #[test]
    fn another_video_starts_over_and_one_no_longer_shown_is_stopped() {
        let mut screen = Screen::turing_88().with_file("internal/video/clip_90.mp4", 1000);
        let mut rt = runtime(video_theme("assets/clip.mp4"));
        rt.start_video(&mut screen, None).expect("start");

        let mut edited = video_theme("assets/clip.mp4");
        edited.name = "edited".into();
        rt.replace(edited, BTreeMap::new());
        assert!(matches!(rt.video(), VideoState::OnDevice(_)), "same video");

        rt.replace(video_theme("assets/other.mp4"), BTreeMap::new());
        assert_eq!(rt.video(), &VideoState::NotStarted);
        let calls = screen.calls.len();
        let state = rt.start_video(&mut screen, None).expect("start");
        assert!(matches!(state, VideoState::VideoMissing(_)));
        assert_eq!(screen.calls.last(), Some(&Call::Stop), "the old loop stops");

        rt.replace(video_theme("assets/clip.mp4"), BTreeMap::new());
        rt.start_video(&mut screen, None).expect("start");
        let plain = Theme::blank("plain", Size::new(480, 1920), Orientation::Landscape);
        rt.replace(plain, BTreeMap::new());
        assert_eq!(rt.video(), &VideoState::NoVideo);
        rt.start_video(&mut screen, None).expect("start");
        assert_eq!(screen.calls.last(), Some(&Call::Stop));
        let stops = |calls: &[Call]| calls.iter().filter(|c| **c == Call::Stop).count();
        assert_eq!(stops(&screen.calls[calls..]), 2);
        rt.start_video(&mut screen, None).expect("again");
        assert_eq!(stops(&screen.calls[calls..]), 2, "only what it played");
    }

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
}
