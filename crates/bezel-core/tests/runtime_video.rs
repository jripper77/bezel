//! The theme runtime's video background (D-2026-09-30-storage-video-4)
//! through the adapters' fakes: the device fake's 8.8" (storage and
//! device-side playback) and WeAct 0.96" (neither), and the sensor fake. The
//! renderer and the host decoder are recording doubles defined here: no
//! adapter ships a fake of them.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::app::{HostVideo, ThemeRuntime, VideoState, open_screen};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::job::Job;
use bezel_core::domain::media::{MediaInfo, MediaTools, StreamSpec, TranscodeTarget};
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

/// A renderer that records what each frame was drawn over and returns a
/// blank frame of the theme's canvas.
#[derive(Default)]
struct Recorder {
    seen: Vec<String>,
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
            Backdrop::Frame(f) => format!("picture {}", f.pixel(0, 0).map_or(0, |p| p.r)),
        });
        Ok(Frame::filled(theme.canvas, Rgba::default()))
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
    fn stream(&mut self, source: &MediaLocation, spec: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        self.opened.push((source.clone(), spec));
        let picture = |r| Frame::filled(Size::new(1, 1), Rgba::opaque(r, 0, 0));
        Ok(Box::new(Clip {
            pictures: vec![picture(10), picture(20)],
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
