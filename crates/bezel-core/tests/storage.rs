//! Storage use cases through the device adapter's fake screen (a Turing 8.8"
//! with in-memory storage).
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::app::open_screen;
use bezel_core::app::storage::{self, PreparedUpload, UploadRequest, Uploaded};
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::job::{CancelToken, Job, JobPhase, Progress};
use bezel_core::domain::media::{
    ConvertOptions, FrameRate, MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget,
    VideoCodec, VideoPixelFormat, VideoTrack,
};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{BootMedia, Refusal, RemotePath, Repeat, StartMode};
use bezel_core::ports::{MediaLocation, MediaTranscoder, ScreenLink, VideoFrames};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FAKE_UPLOAD_CHUNK, FakeStorage, Playback, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};

/// The 8.8" panel in its native orientation: the size its videos must have.
const NATIVE: Size = Size::new(480, 1920);

/// Local files held in memory. Only files already in the screen's profile
/// can be sent: there is no converter.
#[derive(Default)]
struct LocalFiles(BTreeMap<String, (MediaInfo, Vec<u8>)>);

impl LocalFiles {
    /// An MP4 the 8.8" plays as it is.
    fn with_clip(mut self, name: &str, bytes: usize) -> Self {
        let info = MediaInfo {
            format: MediaFormat::Mp4,
            bytes: bytes as u64,
            dimensions: Some(NATIVE),
            video: Some(VideoTrack {
                codec: VideoCodec::H264,
                pixel_format: Some(VideoPixelFormat::Yuv420p),
                b_frames: Some(false),
                frame_rate: FrameRate::new(24, 1),
                duration: Some(Duration::from_secs(1)),
            }),
            has_audio: false,
        };
        let data = (0..bytes).map(|i| (i % 251) as u8).collect();
        self.0.insert(name.to_string(), (info, data));
        self
    }

    fn file(&self, source: &MediaLocation) -> Result<&(MediaInfo, Vec<u8>)> {
        self.0
            .get(&source.0)
            .ok_or_else(|| BezelError::InvalidInput(format!("no file {}", source.0)))
    }
}

impl MediaTranscoder for LocalFiles {
    fn tools(&mut self) -> MediaTools {
        MediaTools::Missing {
            install_hints: Vec::new(),
        }
    }
    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
        Ok(self.file(source)?.0.clone())
    }
    fn transcode(
        &mut self,
        _: &MediaLocation,
        _: &TranscodeTarget,
        _: &mut Job<'_>,
    ) -> Result<MediaLocation> {
        Err(BezelError::Unsupported("no converter".into()))
    }
    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        Ok(self.file(source)?.1.clone())
    }
    fn stream(&mut self, _: &MediaLocation, _: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        Err(BezelError::Unsupported("no converter".into()))
    }
}

fn open(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    open_screen(&FakeBus::turing_88(), connector, None).expect("opens the fake 8.8\"")
}

fn remote(text: &str) -> RemotePath {
    RemotePath::parse(text).expect("path")
}

fn prepare(link: &mut dyn ScreenLink, files: &mut LocalFiles, name: &str) -> PreparedUpload {
    let request = UploadRequest {
        source: MediaLocation(name.into()),
        name: name.into(),
        location: remote("internal/video/x").location,
        options: ConvertOptions::default(),
    };
    storage::prepare_upload(link, files, &request).expect("passes the preflight")
}

/// Runs an upload whose job cancels once `cancel_at` bytes were reported.
fn upload(
    link: &mut dyn ScreenLink,
    files: &mut LocalFiles,
    prepared: &PreparedUpload,
    confirm: Confirm,
    cancel_at: Option<u64>,
) -> (Result<Uploaded>, Vec<Progress>) {
    let token = CancelToken::new();
    let remote = token.clone();
    let mut seen = Vec::new();
    let mut sink = |p: Progress| {
        seen.push(p);
        if cancel_at.is_some_and(|at| p.phase == JobPhase::Upload && p.done >= at) {
            remote.cancel();
        }
    };
    let mut job = Job::new(&token, &mut sink);
    let result = storage::upload(link, files, prepared, confirm, &mut job);
    (result, seen)
}

fn writes(connector: &FakeConnector) -> Vec<StorageCall> {
    let calls = connector.log().storage.calls;
    calls
        .into_iter()
        .filter(StorageCall::changes_the_screen)
        .collect()
}

#[test]
fn an_upload_is_sent_listed_played_and_set_as_boot_media() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let mut files = LocalFiles::default().with_clip("clip.mp4", 3000);
    let info = storage::info(link.as_mut()).expect("info");
    assert_eq!(info.internal.used, 0);
    assert_eq!(info.card, None);

    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    assert_eq!(prepared.plan.replaces, None);
    let (done, progress) = upload(link.as_mut(), &mut files, &prepared, Confirm::No, None);
    let clip = remote("internal/video/clip.mp4");
    assert_eq!(
        done.expect("uploads"),
        Uploaded {
            path: clip.clone(),
            bytes: 3000,
            converted: false
        }
    );
    let phases: Vec<(JobPhase, u64)> = progress.iter().map(|p| (p.phase, p.done)).collect();
    assert_eq!(
        phases,
        [
            (JobPhase::Upload, 0),
            (JobPhase::Upload, 3000),
            (JobPhase::Verify, 0),
            (JobPhase::Verify, 1)
        ]
    );
    assert_eq!(connector.log().storage.files[&clip], files.0["clip.mp4"].1);

    let listed = storage::list(link.as_mut(), clip.location).expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!((&listed[0].path, listed[0].size), (&clip, Some(3000)));

    storage::play(link.as_mut(), &clip, Repeat::Loop).expect("plays");
    assert_eq!(
        connector.log().storage.playback,
        Playback::Video(clip.clone(), Repeat::Loop)
    );
    storage::stop(link.as_mut()).expect("stops");
    storage::set_boot_media(link.as_mut(), &BootMedia::File(clip.clone()), Confirm::Yes)
        .expect("sets the boot media");
    let log = connector.log().storage;
    assert_eq!(log.start_mode, Some(StartMode::Video));
    assert_eq!(log.playback, Playback::Video(clip, Repeat::Loop));
}

#[test]
fn replacing_deleting_and_the_boot_slot_need_confirmation() {
    let clip = remote("internal/video/clip.mp4");
    let connector =
        FakeConnector::with_storage(FakeStorage::default().with_file(clip.clone(), vec![9; 10]));
    let mut link = open(&connector);
    let mut files = LocalFiles::default().with_clip("clip.mp4", 3000);

    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    assert_eq!(
        prepared.plan.replaces.as_ref().and_then(|e| e.size),
        Some(10)
    );
    let (refused, _) = upload(link.as_mut(), &mut files, &prepared, Confirm::No, None);
    assert!(
        matches!(refused, Err(BezelError::NotConfirmed(_))),
        "{refused:?}"
    );
    let refused = storage::delete(link.as_mut(), &clip, Confirm::No);
    assert!(matches!(refused, Err(BezelError::NotConfirmed(_))));
    let boot = BootMedia::File(clip.clone());
    let refused = storage::set_boot_media(link.as_mut(), &boot, Confirm::No);
    assert!(matches!(refused, Err(BezelError::NotConfirmed(_))));
    assert!(writes(&connector).is_empty(), "nothing reached the screen");
    assert_eq!(connector.log().storage.files[&clip], vec![9; 10]);

    let (replaced, _) = upload(link.as_mut(), &mut files, &prepared, Confirm::Yes, None);
    assert_eq!(replaced.expect("replaces").bytes, 3000);
    storage::delete(link.as_mut(), &clip, Confirm::Yes).expect("deletes");
    assert_eq!(
        writes(&connector),
        [
            StorageCall::Upload(clip.clone(), 3000),
            StorageCall::Delete(clip.clone())
        ]
    );
    assert!(connector.log().storage.files.is_empty());
}

#[test]
fn a_cancelled_upload_reports_what_it_left_and_deletes_nothing() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let mut files = LocalFiles::default().with_clip("big.mp4", 3 * FAKE_UPLOAD_CHUNK);
    let prepared = prepare(link.as_mut(), &mut files, "big.mp4");
    let (result, _) = upload(link.as_mut(), &mut files, &prepared, Confirm::No, Some(1));
    let chunk = FAKE_UPLOAD_CHUNK as u64;
    assert_eq!(
        result,
        Err(BezelError::Cancelled {
            partial: Some(chunk)
        })
    );
    assert!(
        !writes(&connector)
            .iter()
            .any(|c| matches!(c, StorageCall::Delete(_)))
    );
    // The partial file is there for a confirmed delete.
    let big = remote("internal/video/big.mp4");
    assert_eq!(connector.log().storage.size(&big), Some(chunk));
    storage::delete(link.as_mut(), &big, Confirm::Yes).expect("deletes the partial file");
}

#[test]
fn a_missing_card_is_never_listed_and_other_families_are_unsupported() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let card = remote("sd/video/x").location;
    assert_eq!(
        storage::list(link.as_mut(), card),
        Err(BezelError::Refused(Refusal::NoCard))
    );
    assert_eq!(connector.log().storage.calls, [StorageCall::Info]);

    let with_card = FakeConnector::with_storage(FakeStorage::default().with_card(1 << 30));
    let mut link = open(&with_card);
    assert!(
        storage::list(link.as_mut(), card)
            .expect("lists")
            .is_empty()
    );

    let weact = FakeBus::new(vec![Endpoint {
        address: DeviceAddress("/dev/ttyACM0".into()),
        transport: Transport::Serial,
        usb: UsbId::new(0x1a86, 0xfe0c),
        serial_number: Some("AD0001".into()),
        manufacturer: None,
        product: None,
        location: None,
    }]);
    let mut link = open_screen(&weact, &connector, None).expect("opens the fake WeAct");
    assert!(matches!(
        storage::info(link.as_mut()),
        Err(BezelError::Unsupported(_))
    ));
}
