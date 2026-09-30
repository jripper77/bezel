//! Storage use cases through the device adapter's fake screens: a Turing
//! 8.8" with in-memory storage, and a WeAct 0.96" without storage. Local
//! files and their conversion are a double of this file: the real
//! converter runs ffmpeg.
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
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::storage::{
    BootMedia, FileEntry, Refusal, RemotePath, Repeat, StartMode, UploadAction,
};
use bezel_core::ports::{MediaLocation, MediaTranscoder, ScreenLink, VideoFrames};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FAKE_UPLOAD_CHUNK, FakeStorage, Playback, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};

/// The 8.8" panel in its native orientation: the size its videos must have.
const NATIVE: Size = Size::new(480, 1920);

/// An H.264 yuv420p MP4 of `size`: in the 8.8"'s profile at [`NATIVE`].
fn mp4(size: Size, bytes: usize) -> MediaInfo {
    MediaInfo {
        format: MediaFormat::Mp4,
        bytes: bytes as u64,
        dimensions: Some(size),
        video: Some(VideoTrack {
            codec: VideoCodec::H264,
            pixel_format: Some(VideoPixelFormat::Yuv420p),
            b_frames: Some(false),
            frame_rate: FrameRate::new(24, 1),
            duration: Some(Duration::from_secs(1)),
        }),
        has_audio: false,
    }
}

/// Local files held in memory, and a converter when `converter` is set: a
/// conversion writes an MP4 of the target's size (`output_size` overrides
/// it) holding `output_bytes` bytes. Every call is recorded.
struct LocalFiles {
    files: BTreeMap<String, (MediaInfo, Vec<u8>)>,
    converter: bool,
    output_size: Option<Size>,
    output_bytes: usize,
    calls: Vec<String>,
}

impl LocalFiles {
    /// No files and no converter.
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            converter: false,
            output_size: None,
            output_bytes: 2000,
            calls: Vec::new(),
        }
    }

    /// No files, with a converter.
    fn converting() -> Self {
        Self {
            converter: true,
            ..Self::new()
        }
    }

    /// With the file `name` described by `info`.
    fn with(mut self, name: &str, info: MediaInfo) -> Self {
        self.put(name, info);
        self
    }

    /// With an MP4 the 8.8" plays as it is.
    fn with_clip(self, name: &str, bytes: usize) -> Self {
        self.with(name, mp4(NATIVE, bytes))
    }

    /// Writes the file `name` (replacing it): `info.bytes` bytes of data.
    fn put(&mut self, name: &str, info: MediaInfo) {
        let len = usize::try_from(info.bytes).expect("a small file");
        let data = (0..len).map(|i| (i % 251) as u8).collect();
        self.files.insert(name.to_string(), (info, data));
    }

    fn file(&self, source: &MediaLocation) -> Result<&(MediaInfo, Vec<u8>)> {
        self.files
            .get(&source.0)
            .ok_or_else(|| BezelError::InvalidInput(format!("no file {}", source.0)))
    }

    /// True once a file was read or converted (not just probed).
    fn used(&self) -> bool {
        let used = |c: &String| c.starts_with("load") || c.starts_with("transcode");
        self.calls.iter().any(used)
    }
}

impl MediaTranscoder for LocalFiles {
    fn tools(&mut self) -> MediaTools {
        if self.converter {
            MediaTools::Ready {
                version: "test".into(),
            }
        } else {
            MediaTools::Missing {
                install_hints: Vec::new(),
            }
        }
    }
    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
        self.calls.push(format!("probe {}", source.0));
        Ok(self.file(source)?.0.clone())
    }
    fn transcode(
        &mut self,
        source: &MediaLocation,
        target: &TranscodeTarget,
        job: &mut Job<'_>,
    ) -> Result<MediaLocation> {
        self.calls.push(format!("transcode {}", source.0));
        job.report(Progress::new(JobPhase::Convert, 0, 10_000));
        job.checkpoint()?;
        let output = format!("{}.converted.mp4", source.0);
        let size = self.output_size.unwrap_or(target.size);
        self.put(&output, mp4(size, self.output_bytes));
        job.report(Progress::new(JobPhase::Convert, 10_000, 10_000));
        Ok(MediaLocation(output))
    }
    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        self.calls.push(format!("load {}", source.0));
        Ok(self.file(source)?.1.clone())
    }
    fn stream(&mut self, _: &MediaLocation, _: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        Err(BezelError::Unsupported(
            "no host decoding in these tests".into(),
        ))
    }
}

fn open(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    open_screen(&FakeBus::turing_88(), connector, None).expect("opens the fake 8.8\"")
}

/// The fake WeAct 0.96": no storage.
fn weact(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    let bus = FakeBus::new(vec![Endpoint {
        address: DeviceAddress("/dev/ttyACM0".into()),
        transport: Transport::Serial,
        usb: UsbId::new(0x1a86, 0xfe0c),
        serial_number: Some("AD0001".into()),
        manufacturer: None,
        product: None,
        location: None,
    }]);
    open_screen(&bus, connector, None).expect("opens the fake WeAct")
}

fn remote(text: &str) -> RemotePath {
    RemotePath::parse(text).expect("path")
}

/// An upload of the local file `source` into the internal video folder.
fn request(source: &str, name: &str) -> UploadRequest {
    UploadRequest {
        source: MediaLocation(source.into()),
        name: name.into(),
        location: remote("internal/video/x").location,
        options: ConvertOptions::default(),
    }
}

fn prepare(link: &mut dyn ScreenLink, files: &mut LocalFiles, name: &str) -> PreparedUpload {
    storage::prepare_upload(link, files, &request(name, name)).expect("passes the preflight")
}

/// When an upload's job is cancelled.
#[derive(Clone, Copy)]
enum Cancel {
    Never,
    /// Before the upload starts.
    First,
    /// Once the transfer reported that many bytes.
    At(u64),
}

/// Runs an upload; returns its result and the progress it reported.
fn upload(
    link: &mut dyn ScreenLink,
    files: &mut LocalFiles,
    prepared: &PreparedUpload,
    confirm: Confirm,
    cancel: Cancel,
) -> (Result<Uploaded>, Vec<Progress>) {
    let token = CancelToken::new();
    if matches!(cancel, Cancel::First) {
        token.cancel();
    }
    let remote = token.clone();
    let mut seen = Vec::new();
    let mut sink = |p: Progress| {
        seen.push(p);
        if matches!(cancel, Cancel::At(at) if p.phase == JobPhase::Upload && p.done >= at) {
            remote.cancel();
        }
    };
    let mut job = Job::new(&token, &mut sink);
    let result = storage::upload(link, files, prepared, confirm, &mut job);
    (result, seen)
}

fn calls(connector: &FakeConnector) -> Vec<StorageCall> {
    connector.log().storage.calls
}

/// The storage calls that changed what the screen stores, shows or keeps.
fn writes(connector: &FakeConnector) -> Vec<StorageCall> {
    let calls = calls(connector);
    calls
        .into_iter()
        .filter(StorageCall::changes_the_screen)
        .collect()
}

#[test]
fn an_upload_is_sent_verified_listed_played_and_set_as_boot_media() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let mut files = LocalFiles::new().with_clip("clip.mp4", 3000);
    let info = storage::info(link.as_mut()).expect("info");
    assert_eq!(info.internal.used, 0);
    assert_eq!(info.card, None);

    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    assert_eq!(prepared.plan.action, UploadAction::AsIs { bytes: 3000 });
    assert_eq!(prepared.plan.replaces, None);
    assert!(writes(&connector).is_empty(), "the preflight only asks");
    let (done, progress) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    let clip = remote("internal/video/clip.mp4");
    assert_eq!(
        done.expect("uploads"),
        Uploaded {
            path: clip.clone(),
            bytes: 3000,
            converted: false
        }
    );
    let phases: Vec<(JobPhase, u64, u64)> = progress
        .iter()
        .map(|p| (p.phase, p.done, p.total))
        .collect();
    assert_eq!(
        phases,
        [
            (JobPhase::Upload, 0, 3000),
            (JobPhase::Upload, 3000, 3000),
            (JobPhase::Verify, 0, 1),
            (JobPhase::Verify, 1, 1)
        ]
    );
    assert_eq!(
        calls(&connector).last(),
        Some(&StorageCall::Size(clip.clone())),
        "the stored size is checked last"
    );
    assert_eq!(
        connector.log().storage.files[&clip],
        files.files["clip.mp4"].1
    );

    let listed = storage::list(link.as_mut(), clip.location).expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!((&listed[0].path, listed[0].size), (&clip, Some(3000)));

    storage::play(link.as_mut(), &clip, Repeat::Loop).expect("plays");
    assert_eq!(
        connector.log().storage.playback,
        Playback::Video(clip.clone(), Repeat::Loop)
    );
    storage::stop(link.as_mut()).expect("stops");
    storage::set_boot_media(
        link.as_mut(),
        &BootMedia::File(clip.clone()),
        None,
        Confirm::Yes,
    )
    .expect("sets the boot media");
    let log = connector.log().storage;
    assert_eq!(log.start_mode, Some(StartMode::Video));
    assert_eq!(log.playback, Playback::Video(clip, Repeat::Loop));
}

#[test]
fn listing_reports_sizes_in_name_order() {
    let connector = FakeConnector::with_storage(
        FakeStorage::default()
            .with_file(remote("internal/video/b.mp4"), vec![1; 20])
            .with_file(remote("internal/video/a.mp4"), vec![1; 10])
            .with_file(remote("internal/image/logo.png"), vec![1; 5]),
    );
    let mut link = open(&connector);
    let entries = storage::list(link.as_mut(), remote("internal/video/x").location).expect("lists");
    let listed: Vec<(String, Option<u64>)> = entries
        .iter()
        .map(|e| (e.path.to_string(), e.size))
        .collect();
    assert_eq!(
        listed,
        [
            ("internal/video/a.mp4".to_string(), Some(10)),
            ("internal/video/b.mp4".to_string(), Some(20))
        ]
    );
    assert!(writes(&connector).is_empty());
}

#[test]
fn replacing_deleting_and_the_boot_slot_need_confirmation() {
    let clip = remote("internal/video/clip.mp4");
    let connector =
        FakeConnector::with_storage(FakeStorage::default().with_file(clip.clone(), vec![9; 10]));
    let mut link = open(&connector);

    // Refused before the screen is even asked anything.
    let err = storage::delete(link.as_mut(), &clip, Confirm::No).expect_err("refused");
    assert_eq!(
        err.to_string(),
        "deleting internal/video/clip.mp4 needs confirmation"
    );
    for boot in [BootMedia::File(clip.clone()), BootMedia::Default] {
        let err = storage::set_boot_media(link.as_mut(), &boot, Some(Brightness::MAX), Confirm::No)
            .expect_err("refused");
        assert!(matches!(err, BezelError::NotConfirmed(_)), "{err}");
    }
    assert!(calls(&connector).is_empty(), "{:?}", calls(&connector));
    assert!(
        connector.log().brightness.is_empty(),
        "not even the brightness"
    );

    // Overwrite: the preflight only queries; the refused upload sends,
    // reads and converts nothing.
    let mut files = LocalFiles::new().with_clip("clip.mp4", 3000);
    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    assert_eq!(
        prepared.plan.replaces,
        Some(FileEntry {
            path: clip.clone(),
            size: Some(10)
        })
    );
    assert!(writes(&connector).is_empty());
    let queried = calls(&connector).len();
    let (refused, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    assert_eq!(
        refused,
        Err(BezelError::NotConfirmed(
            "replacing internal/video/clip.mp4".into()
        ))
    );
    assert_eq!(
        calls(&connector).len(),
        queried,
        "nothing after the refusal"
    );
    assert!(!files.used(), "{:?}", files.calls);
    assert_eq!(connector.log().storage.files[&clip], vec![9; 10]);

    // Confirmed, the same operations go through.
    let (replaced, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::Yes,
        Cancel::Never,
    );
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
fn files_of_unknown_size_are_present() {
    // A TUR_USB screen cannot report the size of a file Bezel did not write
    // (D-2026-09-30-storage-video-7): it is listed without a size, plays,
    // can be the boot media and is replaced only with Yes.
    let clip = remote("internal/video/clip.mp4");
    let connector = FakeConnector::with_storage(
        FakeStorage::default().with_file_of_unknown_size(clip.clone(), vec![9; 10]),
    );
    let mut link = open(&connector);
    let unknown = FileEntry {
        path: clip.clone(),
        size: None,
    };
    let listed = storage::list(link.as_mut(), clip.location).expect("lists");
    assert_eq!(listed, std::slice::from_ref(&unknown));
    storage::play(link.as_mut(), &clip, Repeat::Loop).expect("plays");
    let boot = BootMedia::File(clip.clone());
    storage::set_boot_media(link.as_mut(), &boot, None, Confirm::Yes).expect("boots it");

    let mut files = LocalFiles::new().with_clip("clip.mp4", 3000);
    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    assert_eq!(prepared.plan.replaces, Some(unknown));
    let (refused, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    assert!(
        matches!(refused, Err(BezelError::NotConfirmed(_))),
        "{refused:?}"
    );
    // Prepared while the screen held nothing: still not replaced without Yes.
    let before = prepare(
        open(&FakeConnector::default()).as_mut(),
        &mut files,
        "clip.mp4",
    );
    assert_eq!(before.plan.replaces, None);
    let (refused, _) = upload(
        link.as_mut(),
        &mut files,
        &before,
        Confirm::No,
        Cancel::Never,
    );
    assert!(
        matches!(refused, Err(BezelError::NotConfirmed(_))),
        "{refused:?}"
    );
    let uploads = |c: &FakeConnector| {
        let writes = writes(c);
        writes
            .into_iter()
            .filter(|w| matches!(w, StorageCall::Upload(..)))
            .count()
    };
    assert_eq!(uploads(&connector), 0, "nothing sent without Yes");

    let (replaced, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::Yes,
        Cancel::Never,
    );
    assert_eq!(replaced.expect("replaces").bytes, 3000);
    assert_eq!(connector.log().storage.size(&clip), Some(3000));
}

#[test]
fn a_video_off_profile_is_converted_then_checked_again() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let landscape = mp4(Size::new(1920, 1080), 5000);
    let mut files = LocalFiles::converting().with("trip.mov", landscape.clone());
    let name = storage::suggest_name(link.as_ref(), "Trip.MOV", &landscape)
        .expect("the 8.8\" stores videos")
        .expect("a video name");
    let req = request("trip.mov", name.as_str());
    let prepared = storage::prepare_upload(link.as_mut(), &mut files, &req).expect("preflight");
    assert!(matches!(prepared.plan.action, UploadAction::Convert(t) if t.size == NATIVE));
    let (done, progress) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    let done = done.expect("uploads");
    assert!(done.converted);
    assert_eq!(done.bytes, 2000);
    assert_eq!(done.path.to_string(), "internal/video/trip.mp4");
    assert_eq!(progress[0].phase, JobPhase::Convert);
    assert!(
        files
            .calls
            .contains(&"load trip.mov.converted.mp4".to_string())
    );
    assert_eq!(connector.log().storage.size(&done.path), Some(2000));

    // An output that still misses the profile is refused; nothing is sent.
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    files.output_size = Some(Size::new(1, 1));
    let (refused, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    assert!(
        matches!(refused, Err(BezelError::Refused(Refusal::WrongProfile(_)))),
        "{refused:?}"
    );
    assert!(writes(&connector).is_empty());

    // Without a converter the preflight says so.
    let mut bare = LocalFiles::new().with("trip.mov", landscape);
    let refused = storage::prepare_upload(link.as_mut(), &mut bare, &req);
    assert!(
        matches!(
            refused,
            Err(BezelError::Refused(Refusal::NeedsConverter(_)))
        ),
        "{refused:?}"
    );
}

#[test]
fn a_full_medium_lists_sized_candidates_and_deletes_nothing() {
    let connector = FakeConnector::with_storage(
        FakeStorage {
            internal_total: 1300,
            ..FakeStorage::default()
        }
        .with_file(remote("internal/video/old.mp4"), vec![1; 700])
        .with_file(remote("internal/image/logo.png"), vec![1; 100]),
    );
    let mut link = open(&connector);
    let mut files = LocalFiles::new().with_clip("clip.mp4", 1000);
    let refused =
        storage::prepare_upload(link.as_mut(), &mut files, &request("clip.mp4", "clip.mp4"));
    let Err(BezelError::Refused(Refusal::NoSpace {
        needed,
        free,
        candidates,
    })) = refused
    else {
        panic!("expected NoSpace, got {refused:?}");
    };
    assert_eq!((needed, free), (1000, 500));
    let sizes: Vec<(String, Option<u64>)> = candidates
        .iter()
        .map(|c| (c.path.to_string(), c.size))
        .collect();
    assert_eq!(
        sizes,
        [
            ("internal/video/old.mp4".to_string(), Some(700)),
            ("internal/image/logo.png".to_string(), Some(100))
        ]
    );
    assert!(writes(&connector).is_empty());
    assert_eq!(connector.log().storage.files.len(), 2);
}

#[test]
fn a_cancelled_upload_reports_what_it_left_and_deletes_nothing() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let mut files = LocalFiles::new().with_clip("big.mp4", 3 * FAKE_UPLOAD_CHUNK);
    let prepared = prepare(link.as_mut(), &mut files, "big.mp4");

    // Cancelled before anything was sent.
    let (result, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::First,
    );
    assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
    assert!(writes(&connector).is_empty());

    // Cancelled during the transfer: the screen keeps what arrived.
    let (result, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::At(1),
    );
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
fn verification_and_races_are_caught() {
    // A transfer the screen stored short fails its verification.
    let connector = FakeConnector::with_storage(FakeStorage {
        short_by: 1,
        ..FakeStorage::default()
    });
    let mut link = open(&connector);
    let mut files = LocalFiles::new().with_clip("clip.mp4", 1000);
    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    let (result, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    assert_eq!(
        result,
        Err(BezelError::Transport(
            "internal/video/clip.mp4 holds 999 of 1000 bytes after the upload".into()
        ))
    );

    // A file that appeared at the target after the preflight is not
    // replaced without Yes...
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let prepared = prepare(link.as_mut(), &mut files, "clip.mp4");
    assert_eq!(prepared.plan.replaces, None);
    let (first, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    first.expect("the first upload");
    let written = writes(&connector);
    let (again, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::No,
        Cancel::Never,
    );
    assert!(
        matches!(again, Err(BezelError::NotConfirmed(_))),
        "{again:?}"
    );
    assert_eq!(
        writes(&connector),
        written,
        "nothing sent after the refusal"
    );
    // ...with Yes it is.
    let (again, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::Yes,
        Cancel::Never,
    );
    assert_eq!(again.expect("replaces").bytes, 1000);

    // A source that changed since the preflight is refused before sending.
    files.put("clip.mp4", mp4(NATIVE, 10));
    let written = writes(&connector);
    let (changed, _) = upload(
        link.as_mut(),
        &mut files,
        &prepared,
        Confirm::Yes,
        Cancel::Never,
    );
    assert_eq!(
        changed,
        Err(BezelError::InvalidInput(
            "clip.mp4 changed while its upload was prepared".into()
        ))
    );
    assert_eq!(writes(&connector), written);
}

#[test]
fn play_stop_and_boot_media() {
    let video = remote("internal/video/loop.mp4");
    let image = remote("internal/image/logo.png");
    let connector = FakeConnector::with_storage(
        FakeStorage::default()
            .with_file(video.clone(), vec![1; 10])
            .with_file(image.clone(), vec![1; 5]),
    );
    let mut link = open(&connector);
    storage::play(link.as_mut(), &video, Repeat::Once).expect("plays once");
    storage::play(link.as_mut(), &image, Repeat::Loop).expect("shows the image");
    storage::stop(link.as_mut()).expect("stops");
    let missing = remote("internal/video/none.mp4");
    assert!(matches!(
        storage::play(link.as_mut(), &missing, Repeat::Loop),
        Err(BezelError::InvalidInput(_))
    ));
    assert_eq!(
        writes(&connector),
        [
            StorageCall::PlayVideo(video.clone(), Repeat::Once),
            StorageCall::PlayImage(image.clone()),
            StorageCall::Stop
        ],
        "a file that is not stored is not played"
    );

    let played = writes(&connector).len();
    for boot in [
        BootMedia::File(video.clone()),
        BootMedia::File(image.clone()),
        BootMedia::Default,
    ] {
        storage::set_boot_media(link.as_mut(), &boot, None, Confirm::Yes).expect("boot media");
    }
    assert_eq!(
        writes(&connector)[played..],
        [
            StorageCall::PlayVideo(video, Repeat::Loop),
            StorageCall::StartMode(StartMode::Video),
            StorageCall::PlayImage(image),
            StorageCall::StartMode(StartMode::Image),
            StorageCall::StartMode(StartMode::Default)
        ]
    );
    let written = writes(&connector);
    let missing_boot = storage::set_boot_media(
        link.as_mut(),
        &BootMedia::File(missing),
        Some(Brightness::MAX),
        Confirm::Yes,
    );
    assert!(
        matches!(missing_boot, Err(BezelError::InvalidInput(_))),
        "{missing_boot:?}"
    );
    assert_eq!(writes(&connector), written);
    let log = connector.log();
    assert!(log.brightness.is_empty(), "refused before the brightness");
    assert_eq!(log.storage.start_mode, Some(StartMode::Default));
}

#[test]
fn the_boot_media_keeps_the_brightness_it_is_given() {
    let clip = remote("internal/video/clip.mp4");
    let connector =
        FakeConnector::with_storage(FakeStorage::default().with_file(clip.clone(), vec![1; 10]));
    let mut link = open(&connector);
    let level = Brightness::new(40).expect("a level");
    storage::set_boot_media(
        link.as_mut(),
        &BootMedia::File(clip.clone()),
        Some(level),
        Confirm::Yes,
    )
    .expect("boot media");
    let log = connector.log();
    assert_eq!(log.brightness, [level]);
    assert_eq!(log.storage.start_mode, Some(StartMode::Video));
    assert_eq!(log.storage.playback, Playback::Video(clip, Repeat::Loop));

    // Without one, the link's level stays.
    storage::set_boot_media(link.as_mut(), &BootMedia::Default, None, Confirm::Yes)
        .expect("default boot");
    let log = connector.log();
    assert_eq!(log.brightness, [level]);
    assert_eq!(log.storage.start_mode, Some(StartMode::Default));
}

#[test]
fn a_missing_card_is_never_listed() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let card = remote("sd/video/x").location;
    assert_eq!(
        storage::list(link.as_mut(), card),
        Err(BezelError::Refused(Refusal::NoCard))
    );
    assert_eq!(calls(&connector), [StorageCall::Info], "no LIST_DIR");

    let with_card = FakeConnector::with_storage(FakeStorage::default().with_card(1 << 30));
    let mut link = open(&with_card);
    assert!(
        storage::list(link.as_mut(), card)
            .expect("lists")
            .is_empty()
    );
    let info = storage::info(link.as_mut()).expect("info");
    assert_eq!(info.card.map(|c| c.total), Some(1 << 30));
}

#[test]
fn screens_without_storage_are_unsupported() {
    let connector = FakeConnector::default();
    let mut link = weact(&connector);
    let unsupported = |r: Result<()>| matches!(r, Err(BezelError::Unsupported(_)));
    let clip = remote("internal/video/a.mp4");
    assert!(unsupported(storage::info(link.as_mut()).map(|_| ())));
    assert!(unsupported(
        storage::list(link.as_mut(), clip.location).map(|_| ())
    ));
    assert!(unsupported(storage::stop(link.as_mut())));
    assert!(unsupported(storage::play(
        link.as_mut(),
        &clip,
        Repeat::Loop
    )));
    assert!(unsupported(storage::delete(
        link.as_mut(),
        &clip,
        Confirm::Yes
    )));
    let boot = BootMedia::File(clip.clone());
    assert!(unsupported(storage::set_boot_media(
        link.as_mut(),
        &boot,
        Some(Brightness::MAX),
        Confirm::Yes
    )));
    let mut files = LocalFiles::new().with_clip("a.mp4", 1);
    let req = request("a.mp4", "a.mp4");
    assert!(unsupported(
        storage::prepare_upload(link.as_mut(), &mut files, &req).map(|_| ())
    ));
    assert!(unsupported(
        storage::suggest_name(link.as_ref(), "a.mp4", &mp4(NATIVE, 1)).map(|_| ())
    ));
    let name = link.identity().model.name;
    let err = storage::storage_of(link.as_mut())
        .map(|_| ())
        .expect_err("no storage");
    assert_eq!(
        err.to_string(),
        format!("not supported: {name} has no storage")
    );
    assert!(calls(&connector).is_empty());
    assert!(connector.log().brightness.is_empty());
}
