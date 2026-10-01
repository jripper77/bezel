//! The theme runtime's video background (D-2026-09-30-storage-video-4)
//! and its framing (D-2026-10-01-video-background-framing-2 to -4) through
//! the adapters' fakes: the device fake's 8.8" (storage and device-side
//! playback) and WeAct 0.96" (neither), and the sensor fake. The renderer
//! and the host decoder are recording doubles defined here: no adapter
//! ships a fake of them.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::app::{HostVideo, ThemeRuntime, VideoState, open_screen};
use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::device::{ModelId, Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::frame::{Frame, Rect, Rgba};
use bezel_core::domain::framing::{Pad, VideoFit, VideoFraming, Zoom};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::job::Job;
use bezel_core::domain::media::{
    ConvertOptions, FrameRate, MediaFormat, MediaInfo, MediaTools, PREVIEW_FPS, StreamSpec,
    TranscodeTarget, VideoCodec, VideoPixelFormat, VideoTrack, framed_options,
};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::domain::storage::{RemotePath, Repeat};
use bezel_core::domain::theme::{AssetRef, Background, Theme};
use bezel_core::ports::{
    Backdrop, FrameRenderer, MediaLocation, MediaTranscoder, RenderContext, ScreenLink, VideoFrames,
};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FakeStorage, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_sensors::FakeSensors;

const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 0,
    weekday: 2,
};

/// A renderer that records what each frame was drawn over (and the
/// pictures of a video drawn) and returns a blank frame of the theme's
/// canvas.
#[derive(Default)]
struct Recorder {
    seen: Vec<String>,
    pictures: Vec<Frame>,
}

impl FrameRenderer for Recorder {
    fn render(
        &mut self,
        theme: &Theme,
        _: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame> {
        self.seen.push(match context.backdrop {
            Backdrop::Poster => "poster".into(),
            Backdrop::OnDevice => "on-device".into(),
            Backdrop::Frame(f) => {
                self.pictures.push(f.clone());
                format!("picture {}", f.pixel(0, 0).map_or(0, |p| p.r))
            }
        });
        Ok(Frame::filled(theme.canvas, Rgba::default()))
    }
}

/// A decoded picture of `size` that shows where its pixels came from: red
/// `shade`, green 0 on its top half and 255 below, blue 0 on its left half
/// and 255 right of it.
fn quadrants(size: Size, shade: u8) -> Frame {
    let mut frame = Frame::filled(size, Rgba::opaque(shade, 0, 0));
    let (w, h) = (size.width, size.height);
    frame.fill_rect(
        Rect::new(0, h / 2, w, h - h / 2),
        Rgba::opaque(shade, 255, 0),
    );
    frame.fill_rect(
        Rect::new(w / 2, 0, w - w / 2, h / 2),
        Rgba::opaque(shade, 0, 255),
    );
    frame.fill_rect(
        Rect::new(w / 2, h / 2, w - w / 2, h - h / 2),
        Rgba::opaque(shade, 255, 255),
    );
    frame
}

/// The green and blue of `frame` at (x, y): which quadrant of the decoded
/// picture it shows.
fn quadrant(frame: &Frame, x: u32, y: u32) -> (u8, u8) {
    let p = frame.pixel(x, y).expect("inside");
    (p.g, p.b)
}

/// A host decoder whose videos alternate two pictures of the size asked
/// for ([`quadrants`], red 10 then red 20).
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
    fn stream(&mut self, source: &MediaLocation, spec: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        self.opened.push((source.clone(), spec));
        Ok(Box::new(Clip {
            pictures: vec![quadrants(spec.size, 10), quadrants(spec.size, 20)],
            fps: spec.fps,
        }))
    }
}

/// Pictures shown in turn at `fps`.
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

/// A landscape theme over the video `asset` for the 8.8" (a 1920x480 canvas
/// on a panel whose native orientation is portrait).
fn video_theme(asset: &str) -> Theme {
    let mut theme = Theme::blank("clip", Size::new(480, 1920), Orientation::Landscape);
    theme.background = Background::Video {
        asset: AssetRef(asset.into()),
        poster: None,
        framing: None,
    };
    theme
}

fn runtime(theme: Theme) -> ThemeRuntime {
    ThemeRuntime::new(theme, BTreeMap::new(), Language::English)
}

fn path(text: &str) -> RemotePath {
    RemotePath::parse(text).expect("path")
}

/// The fake 8.8" with `storage`, turned like `video_theme` (as `bezel run`
/// does before the first frame).
fn turing_88(storage: FakeStorage) -> (FakeConnector, Box<dyn ScreenLink>) {
    let connector = FakeConnector::with_storage(storage);
    let mut link =
        open_screen(&FakeBus::turing_88(), &connector, None).expect("opens the fake 8.8\"");
    link.set_orientation(Orientation::Landscape)
        .expect("turns the fake 8.8\"");
    (connector, link)
}

/// A stored video of 1000 bytes.
fn stored(text: &str) -> FakeStorage {
    FakeStorage::default().with_file(path(text), vec![7; 1000])
}

/// The fake WeAct 0.96": no storage, no device-side playback.
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
    let link = open_screen(&bus, &connector, None).expect("opens the fake WeAct");
    (connector, link)
}

fn calls(connector: &FakeConnector) -> Vec<StorageCall> {
    connector.log().storage.calls
}

/// An 8 GB memory card.
const CARD: u64 = 8_000_000_000;

#[test]
fn a_stored_video_is_looped_once_and_frames_become_overlays() {
    let (connector, mut screen) = turing_88(stored("internal/video/clip_90.mp4"));
    let mut rt = runtime(video_theme("assets/Clip.MP4"));
    let (mut sensors, mut r) = (FakeSensors::default(), Recorder::default());
    assert_eq!(rt.video(), &VideoState::NotStarted);
    rt.frame(&mut sensors, &mut r, TIME).expect("preview");

    let clip = path("internal/video/clip_90.mp4");
    let state = rt
        .start_video(screen.as_mut(), None)
        .expect("start")
        .clone();
    assert_eq!(state, VideoState::OnDevice(clip.clone()));
    let started = vec![
        StorageCall::Info,
        StorageCall::Size(clip.clone()),
        StorageCall::PlayVideo(clip, Repeat::Loop),
    ];
    assert_eq!(
        calls(&connector),
        started,
        "size queries only, then one loop"
    );
    for _ in 0..3 {
        rt.show(&mut sensors, &mut r, screen.as_mut(), TIME)
            .expect("show");
    }
    assert_eq!(calls(&connector), started, "started once");
    assert_eq!(connector.log().frames.len(), 3);
    assert_eq!(r.seen, ["poster", "on-device", "on-device", "on-device"]);
    assert!(format!("{rt:?}").contains("OnDevice"));
}

#[test]
fn a_stored_video_of_unknown_size_is_looped() {
    // A TUR_USB screen cannot report the size of a file Bezel did not write
    // (D-2026-09-30-storage-video-7): it is there all the same.
    let clip = path("internal/video/clip_90.mp4");
    let storage = FakeStorage::default().with_file_of_unknown_size(clip.clone(), vec![7; 1000]);
    let (connector, mut screen) = turing_88(storage);
    let mut rt = runtime(video_theme("assets/Clip.MP4"));
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(clip.clone()));
    assert_eq!(
        calls(&connector),
        [
            StorageCall::Info,
            StorageCall::Size(clip.clone()),
            StorageCall::PlayVideo(clip, Repeat::Loop),
        ]
    );
}

#[test]
fn the_card_is_searched_after_the_flash() {
    let (connector, mut screen) = turing_88(stored("sd/video/clip_90.mp4").with_card(CARD));
    let mut rt = runtime(video_theme("assets/Clip.MP4"));
    let on_card = path("sd/video/clip_90.mp4");
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(on_card.clone()));
    assert_eq!(
        calls(&connector),
        [
            StorageCall::Info,
            StorageCall::Size(path("internal/video/clip_90.mp4")),
            StorageCall::Size(on_card.clone()),
            StorageCall::PlayVideo(on_card, Repeat::Loop),
        ]
    );
}

#[test]
fn a_missing_video_keeps_the_poster_and_says_where_to_send_it() {
    for (storage, target) in [
        (FakeStorage::default(), "internal/video/clip_90.mp4"),
        (
            FakeStorage::default().with_card(CARD),
            "sd/video/clip_90.mp4",
        ),
    ] {
        let (connector, mut screen) = turing_88(storage);
        let mut rt = runtime(video_theme("assets/Clip.MP4"));
        let state = rt
            .start_video(screen.as_mut(), None)
            .expect("start")
            .clone();
        let VideoState::VideoMissing(missing) = state else {
            panic!("{state:?}")
        };
        assert_eq!(missing.path, path(target));
        assert_eq!(missing.asset, AssetRef("assets/Clip.MP4".into()));
        assert_eq!(
            missing.options.quarter_turns, 1,
            "landscape on a portrait panel"
        );
        let calls = calls(&connector);
        assert!(
            !calls.iter().any(StorageCall::changes_the_screen),
            "nothing uploaded, played or deleted: {calls:?}"
        );
        assert!(
            !calls.iter().any(|c| matches!(c, StorageCall::List(_))),
            "listing a folder creates it: {calls:?}"
        );
        let source = MediaLocation("/themes/clip/assets/Clip.MP4".into());
        let request = missing.upload_request(source.clone());
        assert_eq!(request.source, source);
        assert_eq!(request.name, "clip_90.mp4");
        assert_eq!(request.location, path(target).location);
        assert_eq!(request.options, missing.options);

        let mut r = Recorder::default();
        rt.frame(&mut FakeSensors::default(), &mut r, TIME)
            .expect("frame");
        assert_eq!(r.seen, ["poster"]);
    }
}

#[test]
fn screens_without_playback_decode_the_video_on_the_host() {
    let (connector, mut screen) = weact();
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
    let state = rt.start_video(screen.as_mut(), Some(host)).expect("start");
    assert_eq!(state, &VideoState::Host);
    let spec = StreamSpec {
        size: canvas,
        fps: 24,
    };
    assert_eq!(media.opened, [(source, spec)]);

    let mut r = Recorder::default();
    rt.sample(&mut FakeSensors::default()).expect("sample");
    for ms in [0, 50, 90] {
        rt.render(&mut r, TIME, Duration::from_millis(ms))
            .expect("render");
    }
    assert_eq!(r.seen, ["picture 10", "picture 20", "picture 10"]);
    assert!(calls(&connector).is_empty());
}

#[test]
fn without_a_converter_or_a_source_the_poster_stays() {
    let (_, mut screen) = weact();
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
    let state = rt.start_video(screen.as_mut(), Some(host)).expect("start");
    assert_eq!(
        state,
        &VideoState::NoConverter {
            install_hints: hints
        }
    );
    assert!(media.opened.is_empty());
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::NoPlayback);
    let mut r = Recorder::default();
    rt.frame(&mut FakeSensors::default(), &mut r, TIME)
        .expect("frame");
    assert_eq!(r.seen, ["poster"]);
}

#[test]
fn themes_without_a_video_leave_the_screen_alone() {
    let (connector, mut screen) = turing_88(FakeStorage::default());
    let theme = Theme::blank("plain", Size::new(480, 1920), Orientation::Portrait);
    let mut rt = runtime(theme);
    assert_eq!(rt.video(), &VideoState::NoVideo);
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::NoVideo);
    assert!(calls(&connector).is_empty());
}

#[test]
fn another_video_starts_over_and_one_no_longer_shown_is_stopped() {
    let (connector, mut screen) = turing_88(stored("internal/video/clip_90.mp4"));
    let mut rt = runtime(video_theme("assets/clip.mp4"));
    rt.start_video(screen.as_mut(), None).expect("start");

    let mut edited = video_theme("assets/clip.mp4");
    edited.name = "edited".into();
    rt.replace(edited, BTreeMap::new());
    assert!(matches!(rt.video(), VideoState::OnDevice(_)), "same video");

    rt.replace(video_theme("assets/other.mp4"), BTreeMap::new());
    assert_eq!(rt.video(), &VideoState::NotStarted);
    let before = calls(&connector).len();
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert!(matches!(state, VideoState::VideoMissing(_)));
    assert_eq!(
        calls(&connector).last(),
        Some(&StorageCall::Stop),
        "the old loop stops"
    );

    rt.replace(video_theme("assets/clip.mp4"), BTreeMap::new());
    rt.start_video(screen.as_mut(), None).expect("start");
    let plain = Theme::blank("plain", Size::new(480, 1920), Orientation::Landscape);
    rt.replace(plain, BTreeMap::new());
    assert_eq!(rt.video(), &VideoState::NoVideo);
    rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(calls(&connector).last(), Some(&StorageCall::Stop));
    let stops = || {
        calls(&connector)[before..]
            .iter()
            .filter(|c| **c == StorageCall::Stop)
            .count()
    };
    assert_eq!(stops(), 2);
    rt.start_video(screen.as_mut(), None).expect("again");
    assert_eq!(stops(), 2, "only what it played");
}

#[test]
fn previews_keep_the_poster_while_the_screen_loops_the_video() {
    let (_, mut screen) = turing_88(stored("internal/video/clip_90.mp4"));
    let mut rt = runtime(video_theme("assets/clip.mp4"));
    rt.start_video(screen.as_mut(), None).expect("start");
    let mut r = Recorder::default();
    rt.sample(&mut FakeSensors::default()).expect("sample");
    rt.render(&mut r, TIME, Duration::ZERO).expect("screen");
    rt.render_with(&mut r, TIME, Backdrop::Poster)
        .expect("preview");
    assert_eq!(r.seen, ["on-device", "poster"]);
}

#[test]
fn a_forgotten_screen_leaves_the_poster_and_nothing_to_stop() {
    let (first, mut screen) = turing_88(stored("internal/video/clip_90.mp4"));
    let mut rt = runtime(video_theme("assets/clip.mp4"));
    rt.start_video(screen.as_mut(), None).expect("start");
    let sent = calls(&first);
    rt.forget_screen();
    assert_eq!(rt.video(), &VideoState::NotStarted);
    assert_eq!(calls(&first), sent, "nothing sent to a closed screen");
    let mut r = Recorder::default();
    rt.frame(&mut FakeSensors::default(), &mut r, TIME)
        .expect("frame");
    assert_eq!(r.seen, ["poster"]);

    // Another screen without the video is not told to stop the old loop.
    let (second, mut other) = turing_88(FakeStorage::default());
    let state = rt.start_video(other.as_mut(), None).expect("start");
    assert!(matches!(state, VideoState::VideoMissing(_)));
    assert!(!calls(&second).contains(&StorageCall::Stop));

    // A host-decoded video stops decoding.
    let (_, mut weact) = weact();
    let mut media = Decoder::ready();
    let host = HostVideo {
        media: &mut media,
        source: MediaLocation("clip.mp4".into()),
        fps: 10,
    };
    rt.start_video(weact.as_mut(), Some(host)).expect("start");
    assert_eq!(rt.video(), &VideoState::Host);
    rt.forget_screen();
    rt.render(&mut r, TIME, Duration::from_millis(50))
        .expect("frame");
    assert_eq!(r.seen, ["poster", "poster"]);

    let plain = Theme::blank("plain", Size::new(480, 1920), Orientation::Landscape);
    rt.replace(plain, BTreeMap::new());
    rt.forget_screen();
    assert_eq!(rt.video(), &VideoState::NoVideo);
}

#[test]
fn the_themes_framing_names_the_video_looked_for() {
    // An explicit 270 degrees on the landscape theme turns nothing on the
    // 8.8": the vendor's own name, as the screen stores it.
    let turned_back = VideoFraming {
        rotation: Some(3),
        ..VideoFraming::default()
    };
    let framed = |framing| {
        let mut theme = video_theme("assets/dragon.mp4");
        theme.background = Background::Video {
            asset: AssetRef("assets/dragon.mp4".into()),
            poster: None,
            framing: Some(framing),
        };
        theme
    };
    let (connector, mut screen) = turing_88(stored("internal/video/dragon.mp4"));
    let mut rt = runtime(framed(turned_back));
    let dragon = path("internal/video/dragon.mp4");
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(dragon.clone()));
    assert_eq!(
        calls(&connector),
        [
            StorageCall::Info,
            StorageCall::Size(dragon.clone()),
            StorageCall::PlayVideo(dragon, Repeat::Loop),
        ]
    );
    // Zoomed, it is another file: looked for under its own name.
    let zoomed = VideoFraming {
        zoom: Zoom::from_percent(125),
        ..turned_back
    };
    let (connector, mut screen) = turing_88(stored("internal/video/dragon.mp4"));
    let mut rt = runtime(framed(zoomed));
    let state = rt
        .start_video(screen.as_mut(), None)
        .expect("start")
        .clone();
    let VideoState::VideoMissing(missing) = state else {
        panic!("{state:?}")
    };
    assert_eq!(missing.path, path("internal/video/dragon_f8ca275f8.mp4"));
    assert_eq!(missing.options.quarter_turns, 0);
    assert!(
        !calls(&connector)
            .iter()
            .any(StorageCall::changes_the_screen)
    );
}

#[test]
fn a_converter_that_takes_no_pictures_says_so() {
    let clip = MediaInfo {
        format: bezel_core::domain::media::MediaFormat::Mp4,
        bytes: 1,
        dimensions: Some(Size::new(1920, 1080)),
        video: None,
        has_audio: false,
    };
    let spec = PosterSpec::for_canvas(Size::new(1920, 480), &clip);
    let taken = Decoder::ready().poster(&MediaLocation("clip.mp4".into()), spec);
    assert!(
        matches!(taken, Err(BezelError::Unsupported(_))),
        "{taken:?}"
    );
}

/// Bytes of the vendor's `dragon.mp4`, the theme asset and the copy the
/// user's 8.8" already stores alike.
const DRAGON_BYTES: u64 = 2_588_343;

/// The user's "Dragon Ball" theme: a 1920x480 landscape canvas on the 8.8"
/// over `assets/dragon.mp4`, framed as `framing` says (`None`: Auto).
fn dragon_ball(framing: Option<VideoFraming>) -> Theme {
    let mut theme = Theme::blank("Dragon Ball", Size::new(480, 1920), Orientation::Landscape);
    theme.background = Background::Video {
        asset: AssetRef("assets/dragon.mp4".into()),
        poster: None,
        framing,
    };
    theme
}

/// An H.264 yuv420p MP4 of `size` and `bytes`, 24 fps, 10.2 s, no audio:
/// what probing its header says.
fn mp4(size: Size, bytes: u64) -> MediaInfo {
    MediaInfo {
        format: MediaFormat::Mp4,
        bytes,
        dimensions: Some(size),
        video: Some(VideoTrack {
            codec: VideoCodec::H264,
            pixel_format: Some(VideoPixelFormat::Yuv420p),
            b_frames: Some(true),
            frame_rate: FrameRate::new(24, 1),
            duration: Some(Duration::from_millis(10_200)),
        }),
        has_audio: false,
    }
}

/// The Dragon Ball video as probed: panel-native, 480x1920, turned for the
/// panel by the vendor.
fn dragon_info() -> MediaInfo {
    mp4(Size::new(480, 1920), DRAGON_BYTES)
}

/// A stored copy of the asset itself, `bytes` long.
fn stored_bytes(text: &str, bytes: u64) -> FakeStorage {
    let data = vec![0; usize::try_from(bytes).expect("fits")];
    FakeStorage::default().with_file(path(text), data)
}

/// The calls of `connector` that changed what the screen stores or shows.
fn changes(connector: &FakeConnector) -> Vec<StorageCall> {
    calls(connector)
        .into_iter()
        .filter(StorageCall::changes_the_screen)
        .collect()
}

#[test]
fn the_stored_dragon_ball_video_loops_without_an_upload() {
    let dragon = path("internal/video/dragon.mp4");
    let (connector, mut screen) =
        turing_88(stored_bytes("internal/video/dragon.mp4", DRAGON_BYTES));
    let mut rt = runtime(dragon_ball(None));

    // Without the probed size Auto turns nothing: the copy turned to the
    // panel is looked for, and the poster stays.
    assert_eq!(rt.video_framing().map(|f| f.turns), Some(0));
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    let VideoState::VideoMissing(missing) = state else {
        panic!("{state:?}")
    };
    assert_eq!(missing.path, path("internal/video/dragon_90.mp4"));

    // Probed (the MP4 header), the video is panel-native in a landscape
    // theme: 270 degrees on the canvas, nothing in total on the panel. The
    // screen looks again, for the vendor's own name.
    rt.set_video_info(Some(dragon_info()));
    assert_eq!(rt.video(), &VideoState::NotStarted, "looks again");
    assert_eq!(rt.video_info(), Some(&dragon_info()));
    assert_eq!(rt.video_framing().map(|f| f.turns), Some(3));
    let before = calls(&connector).len();
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(dragon.clone()));
    assert_eq!(
        calls(&connector)[before..],
        [
            StorageCall::Info,
            StorageCall::Size(dragon.clone()),
            StorageCall::PlayVideo(dragon.clone(), Repeat::Loop),
        ],
        "a size query, then the loop: nothing sent"
    );
    let (mut sensors, mut r) = (FakeSensors::default(), Recorder::default());
    for _ in 0..2 {
        rt.show(&mut sensors, &mut r, screen.as_mut(), TIME)
            .expect("show");
    }
    assert_eq!(r.seen, ["on-device", "on-device"]);
    assert_eq!(
        changes(&connector),
        [StorageCall::PlayVideo(dragon, Repeat::Loop)],
        "no upload, no delete"
    );
}

#[test]
fn a_stored_file_of_another_size_is_not_reused() {
    // A copy sent as it is holds the asset's bytes: a `dragon.mp4` of
    // another size is another video. The poster stays, and sending puts
    // the asset in its place as it is.
    let dragon = path("internal/video/dragon.mp4");
    let (connector, mut screen) = turing_88(stored("internal/video/dragon.mp4"));
    let mut rt = runtime(dragon_ball(None));
    rt.set_video_info(Some(dragon_info()));
    let state = rt
        .start_video(screen.as_mut(), None)
        .expect("start")
        .clone();
    let VideoState::VideoMissing(missing) = state else {
        panic!("{state:?}")
    };
    assert_eq!(missing.path, dragon, "the same place");
    assert_eq!(missing.options, ConvertOptions::default());
    assert!(missing.options.is_identity(), "sent as it is");
    let source = MediaLocation("/themes/dragon/assets/dragon.mp4".into());
    assert_eq!(missing.upload_request(source).name, "dragon.mp4");
    assert_eq!(
        calls(&connector),
        [StorageCall::Info, StorageCall::Size(dragon.clone())]
    );
    let mut r = Recorder::default();
    rt.frame(&mut FakeSensors::default(), &mut r, TIME)
        .expect("frame");
    assert_eq!(r.seen, ["poster"]);

    // The asset's copy on the card is found past the other one.
    let on_card = path("sd/video/dragon.mp4");
    let storage = stored("internal/video/dragon.mp4")
        .with_card(CARD)
        .with_file(on_card.clone(), vec![0; DRAGON_BYTES as usize]);
    let (connector, mut screen) = turing_88(storage);
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(on_card.clone()));
    assert_eq!(
        calls(&connector),
        [
            StorageCall::Info,
            StorageCall::Size(dragon.clone()),
            StorageCall::Size(on_card.clone()),
            StorageCall::PlayVideo(on_card, Repeat::Loop),
        ]
    );

    // A size the screen cannot report counts (D-2026-09-30-storage-video-7).
    let unknown = FakeStorage::default().with_file_of_unknown_size(dragon.clone(), vec![7; 10]);
    let (_, mut screen) = turing_88(unknown);
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(dragon));

    // A converted copy keeps the presence check: its size is unknown before
    // converting, so any file under its name is it.
    let mut clip = runtime(video_theme("assets/clip.mp4"));
    clip.set_video_info(Some(mp4(Size::new(1920, 1080), 5_000_000)));
    let (_, mut screen) = turing_88(stored("internal/video/clip_90.mp4"));
    let state = clip.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(
        state,
        &VideoState::OnDevice(path("internal/video/clip_90.mp4"))
    );
}

#[test]
fn a_reframed_video_is_looked_for_under_its_own_name() {
    // Zoomed 125 % (rotation Auto): another file than the vendor's, looked
    // for under its own name while the vendor's copy stays untouched.
    let zoomed = VideoFraming {
        zoom: Zoom::from_percent(125),
        ..VideoFraming::default()
    };
    let storage = stored_bytes("internal/video/dragon.mp4", DRAGON_BYTES);
    let (connector, mut screen) = turing_88(storage);
    let mut rt = runtime(dragon_ball(Some(zoomed)));
    rt.set_video_info(Some(dragon_info()));
    let state = rt
        .start_video(screen.as_mut(), None)
        .expect("start")
        .clone();
    let VideoState::VideoMissing(missing) = state else {
        panic!("{state:?}")
    };
    assert_eq!(missing.path, path("internal/video/dragon_f8ca275f8.mp4"));
    // Converted from the panel's geometry: the centered 80 % of the
    // panel-native picture, nothing turned.
    let expected = ConvertOptions {
        crop: Some(Rect::new(48, 192, 384, 1536)),
        ..ConvertOptions::default()
    };
    assert_eq!(missing.options, expected);
    let model = model_by_id(ModelId("turing-8.8")).expect("model");
    let framing = rt.video_framing().expect("a video");
    assert_eq!(
        framed_options(model, Orientation::Landscape, &dragon_info(), &framing),
        expected,
        "what \"Send to screen\" converts"
    );
    assert!(changes(&connector).is_empty(), "nothing sent or deleted");

    // Fitted without turning on the canvas: a copy turned to the panel,
    // padded. Editing the framing on a started screen looks again.
    let fitted = VideoFraming {
        rotation: Some(0),
        fit: VideoFit::Contain,
        ..VideoFraming::default()
    };
    rt.replace_theme(dragon_ball(Some(fitted)));
    assert_eq!(rt.video(), &VideoState::NotStarted, "looks again");
    let state = rt
        .start_video(screen.as_mut(), None)
        .expect("start")
        .clone();
    let VideoState::VideoMissing(missing) = state else {
        panic!("{state:?}")
    };
    assert_eq!(missing.path, path("internal/video/dragon_90_f8ec2b24d.mp4"));
    let pad = Pad {
        scaled: Size::new(480, 120),
        x: 0,
        y: 900,
        color: Rgba::BLACK,
    };
    assert_eq!(
        missing.options,
        ConvertOptions {
            quarter_turns: 1,
            pad: Some(pad),
            ..ConvertOptions::default()
        }
    );
    // An edit that frames the video the same keeps what the screen found.
    let mut renamed = dragon_ball(Some(fitted));
    renamed.name = "renamed".into();
    rt.replace_theme(renamed);
    assert!(matches!(rt.video(), VideoState::VideoMissing(_)));

    // Once its own copy is on the screen (converted: any size), it loops.
    let own = path("internal/video/dragon_f8ca275f8.mp4");
    let (_, mut screen) = turing_88(stored("internal/video/dragon_f8ca275f8.mp4"));
    rt.replace_theme(dragon_ball(Some(zoomed)));
    let state = rt.start_video(screen.as_mut(), None).expect("start");
    assert_eq!(state, &VideoState::OnDevice(own));
}

#[test]
fn the_host_decodes_the_video_with_its_framing() {
    // The WeAct 0.96" plays no videos: the host decodes them. A landscape
    // theme over a video of its panel's native size (80x160, turned for the
    // panel) is turned back by Auto, as the Dragon Ball is on the 8.8".
    let mut theme = Theme::blank("clip", Size::new(80, 160), Orientation::Landscape);
    theme.background = Background::Video {
        asset: AssetRef("assets/clip.mp4".into()),
        poster: None,
        framing: None,
    };
    let canvas = theme.canvas;
    assert_eq!(canvas, Size::new(160, 80));
    let native = Size::new(80, 160);
    let mut rt = runtime(theme.clone());
    rt.set_video_info(Some(mp4(native, 1000)));
    let (connector, mut screen) = weact();
    let mut media = Decoder::ready();
    let source = MediaLocation("clip.mp4".into());
    let host = HostVideo {
        media: &mut media,
        source: source.clone(),
        fps: 10,
    };
    let state = rt.start_video(screen.as_mut(), Some(host)).expect("start");
    assert_eq!(state, &VideoState::Host);
    // The decoder hands over the whole source picture, never turned.
    let raw = StreamSpec {
        size: native,
        fps: 10,
    };
    assert_eq!(media.opened, [(source, raw)]);
    assert_eq!(rt.video_stream(10), raw);

    // Each picture is framed in Rust: a quarter turn clockwise, so the
    // source's top half is the canvas's right half.
    let mut r = Recorder::default();
    rt.sample(&mut FakeSensors::default()).expect("sample");
    rt.render(&mut r, TIME, Duration::ZERO).expect("render");
    let turned = &r.pictures[0];
    assert_eq!(turned.size(), canvas);
    assert_eq!(*turned, quadrants(native, 10).rotated(1));
    assert_eq!(
        quadrant(turned, 0, 0),
        (255, 0),
        "bottom-left of the source"
    );
    assert_eq!(quadrant(turned, 159, 79), (0, 255), "its top-right");

    // A framing edit shows at the next picture; the decoder goes on.
    let padded = Rgba::opaque(0, 0, 99);
    let fitted = VideoFraming {
        rotation: Some(0),
        fit: VideoFit::Contain,
        pad: padded,
        ..VideoFraming::default()
    };
    let mut edited = theme.clone();
    edited.background = Background::Video {
        asset: AssetRef("assets/clip.mp4".into()),
        poster: None,
        framing: Some(fitted),
    };
    rt.replace_theme(edited.clone());
    assert_eq!(rt.video(), &VideoState::Host, "still decoding");
    rt.render(&mut r, TIME, Duration::from_millis(100))
        .expect("render");
    assert_eq!(media.opened.len(), 1, "the decoder was not reopened");
    let fitted_picture = &r.pictures[1];
    assert_eq!(fitted_picture.pixel(0, 40), Some(padded));
    assert_eq!(fitted_picture.pixel(159, 40), Some(padded));
    assert_eq!(fitted_picture.pixel(60, 0).map(|p| p.r), Some(20));
    assert_eq!(quadrant(fitted_picture, 60, 0), (0, 0), "top-left");
    assert_eq!(quadrant(fitted_picture, 99, 79), (255, 255), "bottom-right");
    assert!(calls(&connector).is_empty());

    // An editor's preview frames the pictures it decoded the same way,
    // without a screen (the canvas names the WeAct's panel).
    let mut editor = runtime(theme);
    editor.set_video_info(Some(mp4(native, 1000)));
    let spec = editor.video_stream(PREVIEW_FPS);
    assert_eq!(
        spec,
        StreamSpec {
            size: native,
            fps: 15
        }
    );
    let picture = quadrants(spec.size, 30);
    let mut r = Recorder::default();
    editor
        .preview_video(&mut r, TIME, Duration::ZERO, &picture)
        .expect("preview");
    assert_eq!(r.pictures, [picture.rotated(1)]);
    editor.replace_theme(edited);
    let (_, next) = editor
        .preview_video(&mut r, TIME, Duration::from_millis(66), &picture)
        .expect("preview");
    assert_eq!(next, None, "when the next picture is due is the caller's");
    assert_eq!(r.pictures[1].pixel(0, 40), Some(padded));
    assert_eq!(r.seen, ["picture 30", "picture 0"]);
}
