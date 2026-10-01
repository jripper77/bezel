//! The storage manager's use cases through the adapters' fakes: a Turing
//! 8.8" (rev C) and a Turing USB 8.8" with in-memory storage
//! (`bezel_devices::fake`), and the local copies in memory
//! (`bezel_media::archive::MemoryArchive`). Local files are a double of this
//! file.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::app::manager::{
    self, Batch, Cleared, Halt, Inventory, Manager, ManagerError, Stage, StepProgress,
    TransferReport, Undeleted,
};
use bezel_core::app::open_screen;
use bezel_core::app::storage::{self, PreparedUpload, UploadRequest, Uploaded};
use bezel_core::domain::archive::{
    ArchiveEntry, Catalog, Clear, ContentId, Deletes, EntryState, PlanRefusal, ScreenKey, Skip,
    Step, Transfer, TransferPlan, Warning,
};
use bezel_core::domain::cleanup::Code;
use bezel_core::domain::device::{ModelId, Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::job::{CancelToken, Job, JobPhase, Progress};
use bezel_core::domain::media::{
    ConvertOptions, FrameRate, MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget,
    VideoCodec, VideoPixelFormat, VideoTrack,
};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{
    BootMedia, Confirmed, FileEntry, MIB, Medium, Operation, Refusal, RemotePath,
};
use bezel_core::domain::theme::AssetRef;
use bezel_core::ports::{ArchiveStore, MediaLocation, MediaTranscoder, ScreenLink, VideoFrames};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FAKE_UPLOAD_CHUNK, FakeStorage, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_media::archive::{MemoryArchive, content_id};

/// The user's card, 29.7 GiB.
const CARD: u64 = 31_890_132_172;
/// Another card the catalog knows.
const OTHER_CARD: u64 = 7_948_206_080;
/// The time the use cases record, seconds since the Unix epoch.
const NOW: u64 = 1_790_000_000;
/// One chunk of the simulated upload, in bytes.
const CHUNK: u64 = FAKE_UPLOAD_CHUNK as u64;
/// The 8.8" panel in its native orientation: the size its videos have.
const NATIVE: Size = Size::new(480, 1920);

fn remote(text: &str) -> RemotePath {
    RemotePath::parse(text).expect("path")
}

fn local(text: &str) -> MediaLocation {
    MediaLocation(text.into())
}

/// `len` bytes that differ from another `seed`'s.
fn clip(seed: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8 ^ seed).collect()
}

/// An H.264 MP4 of `bytes` the 8.8" plays as it is.
fn mp4(bytes: usize) -> MediaInfo {
    MediaInfo {
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
    }
}

/// The rev C 8.8"'s catalog key.
fn key() -> ScreenKey {
    ScreenKey::new(ModelId("turing-8.8"))
}

/// Keeps each file's bytes in `store` and catalogs it as stored on `key`'s
/// screen, sent at 1, 2, ... in order.
fn seed(store: &mut MemoryArchive, key: &ScreenKey, files: &[(&str, Option<u64>, &[u8])]) {
    let mut catalog = store.load().expect("load");
    for (n, (path, card, bytes)) in files.iter().enumerate() {
        let content = store.keep(bytes).expect("keep");
        let size = bytes.len() as u64;
        let sent_at = n as u64 + 1;
        let mut entry = ArchiveEntry::pending(remote(path), *card, size, content.clone(), sent_at);
        entry.state = EntryState::Stored;
        catalog.copies.insert(content);
        catalog.screen_mut(key).record(entry);
    }
    store.save(&catalog).expect("save");
}

fn saved(store: &MemoryArchive) -> Catalog {
    store.saved().cloned().expect("a saved catalog")
}

/// The entry at `path` of `key`'s record, deleted or not.
fn entry_at(catalog: &Catalog, key: &ScreenKey, path: &str) -> Option<ArchiveEntry> {
    let record = catalog.screen(key)?;
    let path = remote(path);
    record.entries.iter().find(|e| e.path == path).cloned()
}

fn state_at(catalog: &Catalog, path: &str) -> Option<EntryState> {
    entry_at(catalog, &key(), path).map(|e| e.state)
}

fn open(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    open_screen(&FakeBus::turing_88(), connector, None).expect("opens the fake 8.8\"")
}

/// The fake Turing USB 8.8": it cannot delete through Bezel.
fn open_usb(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    let bus = FakeBus::new(vec![Endpoint {
        address: DeviceAddress("usb:3-1".into()),
        transport: Transport::UsbBulk,
        usb: UsbId::new(0x1cbe, 0x0088),
        serial_number: None,
        manufacturer: None,
        product: None,
        location: None,
    }]);
    open_screen(&bus, connector, None).expect("opens the fake Turing USB 8.8\"")
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

/// The uploads, the size checks of the steps' targets and the deletes since
/// call number `from`, in order.
fn trace(connector: &FakeConnector, steps: &[Step], from: usize) -> Vec<String> {
    let targets: Vec<&RemotePath> = steps.iter().map(|s| &s.target).collect();
    let calls = calls(connector);
    let traced = calls[from..].iter().filter_map(|call| match call {
        StorageCall::Upload(path, _) => Some(format!("upload {path}")),
        StorageCall::Size(path) if targets.contains(&path) => Some(format!("size {path}")),
        StorageCall::Delete(path) => Some(format!("delete {path}")),
        _ => None,
    });
    traced.collect()
}

fn targets(steps: &[Step]) -> Vec<String> {
    steps.iter().map(|s| s.target.to_string()).collect()
}

/// Writes `data` at `path` through the port, as another program would.
fn port_upload(link: &mut dyn ScreenLink, path: &RemotePath, data: &[u8]) {
    let token = CancelToken::new();
    let mut sink = |_| {};
    let mut job = Job::new(&token, &mut sink);
    let storage = link.storage().expect("storage");
    storage.upload(path, data, &mut job).expect("stored");
}

/// Deletes `path` through the port, as another program would.
fn port_delete(link: &mut dyn ScreenLink, path: &RemotePath) {
    let confirmed = Confirmed::require(Confirm::Yes, &Operation::Delete(path.clone()));
    let storage = link.storage().expect("storage");
    storage
        .delete(path, confirmed.expect("confirmed"))
        .expect("deleted");
}

/// When a batch is cancelled.
#[derive(Clone, Copy)]
enum Cancel {
    Never,
    /// Before it starts.
    First,
    /// Once `step`'s upload reported `done` bytes.
    During {
        step: usize,
        done: u64,
    },
    /// Once `step`'s copy was verified, before its source is deleted.
    AfterVerify(usize),
}

/// Runs `plan`; returns the report and the progress the batch reported.
fn run(
    manager: &mut Manager<'_>,
    plan: &TransferPlan,
    confirm: Confirm,
    cancel: Cancel,
) -> (
    std::result::Result<TransferReport, ManagerError>,
    Vec<StepProgress>,
) {
    let token = CancelToken::new();
    if matches!(cancel, Cancel::First) {
        token.cancel();
    }
    let remote = token.clone();
    let mut seen = Vec::new();
    let mut sink = |p: StepProgress| {
        seen.push(p);
        let hit = match cancel {
            Cancel::During { step, done } => {
                p.step == step && p.progress.phase == JobPhase::Upload && p.progress.done >= done
            }
            Cancel::AfterVerify(step) => {
                p.step == step && p.progress == Progress::new(JobPhase::Verify, 1, 1)
            }
            Cancel::Never | Cancel::First => false,
        };
        if hit {
            remote.cancel();
        }
    };
    let mut batch = Batch::new(&token, &mut sink);
    let result = manager.run(plan, confirm, NOW, &mut batch);
    (result, seen)
}

/// Local files held in memory, without a converter. Every load is counted.
struct LocalFiles {
    files: BTreeMap<String, (MediaInfo, Vec<u8>)>,
}

impl LocalFiles {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    /// With the MP4 `name` holding `data`.
    fn with(mut self, name: &str, data: Vec<u8>) -> Self {
        self.files.insert(name.into(), (mp4(data.len()), data));
        self
    }

    fn file(&self, source: &MediaLocation) -> Result<&(MediaInfo, Vec<u8>)> {
        self.files
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
        Err(BezelError::Unsupported(
            "no converter in these tests".into(),
        ))
    }
    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        Ok(self.file(source)?.1.clone())
    }
    fn stream(&mut self, _: &MediaLocation, _: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        Err(BezelError::Unsupported("no decoding in these tests".into()))
    }
}

/// The preflight of sending the local `source` as `name` into `folder`.
fn prepare(
    link: &mut dyn ScreenLink,
    files: &mut LocalFiles,
    source: &str,
    name: &str,
    folder: &str,
) -> PreparedUpload {
    let request = UploadRequest {
        source: local(source),
        name: name.into(),
        location: remote(&format!("{folder}/x")).location,
        options: ConvertOptions::default(),
    };
    storage::prepare_upload(link, files, &request).expect("passes the preflight")
}

/// A recorded upload, cancelled once the transfer reported `cancel_at`
/// bytes.
fn upload(
    manager: &mut Manager<'_>,
    files: &mut LocalFiles,
    prepared: &PreparedUpload,
    cancel_at: Option<u64>,
) -> Result<Uploaded> {
    let token = CancelToken::new();
    let remote = token.clone();
    let mut sink = |p: Progress| {
        if cancel_at.is_some_and(|at| p.phase == JobPhase::Upload && p.done >= at) {
            remote.cancel();
        }
    };
    let mut job = Job::new(&token, &mut sink);
    manager.upload(files, prepared, Confirm::No, NOW, &mut job)
}

/// The memory store, noting how many storage calls the screen had received
/// each time the catalog was saved.
struct Spy {
    inner: MemoryArchive,
    connector: FakeConnector,
    saves: Vec<(usize, Catalog)>,
}

impl ArchiveStore for Spy {
    fn load(&mut self) -> Result<Catalog> {
        self.inner.load()
    }
    fn save(&mut self, catalog: &Catalog) -> Result<()> {
        let calls = calls(&self.connector).len();
        self.saves.push((calls, catalog.clone()));
        self.inner.save(catalog)
    }
    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId> {
        self.inner.keep(bytes)
    }
    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        self.inner.read(content)
    }
    fn discard(&mut self, content: &ContentId) -> Result<()> {
        self.inner.discard(content)
    }
}

#[test]
fn move_deletes_the_source_only_after_the_copy_is_verified() {
    let (a, b) = (clip(1, 3000), clip(2, 5000));
    let card = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote("sd/video/A.mp4"), a.clone())
        .with_file(remote("sd/video/b.mp4"), b.clone());
    let connector = FakeConnector::with_storage(card.clone());
    let mut store = MemoryArchive::new();
    let sent: [(&str, Option<u64>, &[u8]); 2] = [
        ("sd/video/A.mp4", Some(CARD), &a),
        ("sd/video/b.mp4", Some(CARD), &b),
    ];
    seed(&mut store, &key(), &sent);
    let mut link = open(&connector);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let sources = [remote("sd/video/A.mp4"), remote("sd/video/b.mp4")];
    let plan = manager
        .plan_move(&sources, Medium::Internal, &[])
        .expect("plans");
    assert_eq!(
        targets(&plan.steps),
        ["internal/video/a.mp4", "internal/video/b.mp4"],
        "upload names"
    );
    assert!(writes(&connector).is_empty(), "planning only asks");

    // Without the confirmation the screen is not called at all.
    let asked = calls(&connector).len();
    let (refused, _) = run(&mut manager, &plan, Confirm::No, Cancel::Never);
    let Err(ManagerError::Failed(BezelError::NotConfirmed(what))) = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(what, "moving 2 files");
    assert_eq!(calls(&connector).len(), asked);

    let (report, progress) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let report = report.expect("runs");
    assert!(report.completed());
    assert_eq!(report.done, plan.steps);
    assert!(report.not_started.is_empty() && report.skipped.is_empty());
    // One file at a time: its copy is sent, its stored size checked, and
    // only then is its source deleted.
    assert_eq!(
        trace(&connector, &plan.steps, asked),
        [
            "upload internal/video/a.mp4",
            "size internal/video/a.mp4",
            "delete sd/video/A.mp4",
            "upload internal/video/b.mp4",
            "size internal/video/b.mp4",
            "delete sd/video/b.mp4",
        ]
    );
    let files = connector.log().storage.files;
    assert_eq!(files.get(&remote("internal/video/a.mp4")), Some(&a));
    assert_eq!(files.get(&remote("internal/video/b.mp4")), Some(&b));
    assert!(!files.contains_key(&sources[0]) && !files.contains_key(&sources[1]));
    let mut steps: Vec<usize> = progress.iter().map(|p| p.step).collect();
    steps.dedup();
    assert_eq!(steps, [0, 1]);
    assert!(progress.iter().all(|p| p.steps == 2));
    assert_eq!(
        progress.last().map(|p| p.progress),
        Some(Progress::new(JobPhase::Verify, 1, 1))
    );

    // The catalog follows: the files live at their targets, from the same
    // copies; the sources are forgotten.
    let catalog = saved(&store);
    let moved = entry_at(&catalog, &key(), "internal/video/a.mp4").expect("entry");
    assert_eq!(
        (moved.state, moved.card, moved.sent_at),
        (EntryState::Stored, None, NOW)
    );
    assert_eq!(moved.content, content_id(&a));
    assert_eq!(
        state_at(&catalog, "internal/video/b.mp4"),
        Some(EntryState::Stored)
    );
    assert_eq!(state_at(&catalog, "sd/video/A.mp4"), None);
    assert_eq!(catalog.screen(&key()).expect("record").entries.len(), 2);
    assert_eq!((catalog.copies.len(), store.copies()), (2, 2));

    // A stored size that differs from the copy's stops before the delete:
    // the source stays, and the file left at the target is offered for a
    // confirmed delete.
    let short = FakeConnector::with_storage(FakeStorage {
        short_by: 1,
        ..card
    });
    let mut store = MemoryArchive::new();
    seed(&mut store, &key(), &sent);
    let mut link = open(&short);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let plan = manager
        .plan_move(&sources[..1], Medium::Internal, &[])
        .expect("plans");
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let stopped = report.expect("a report").stopped.expect("stopped");
    let target = remote("internal/video/a.mp4");
    assert_eq!(stopped.stage, Stage::Verify);
    assert_eq!(
        stopped.halt,
        Halt::Failed(BezelError::SizeMismatch {
            path: target.clone(),
            sent: 3000,
            stored: 2999
        })
    );
    assert!(!stopped.copied());
    assert_eq!(
        stopped.leftover(),
        Some(FileEntry {
            path: target,
            size: Some(2999)
        })
    );
    let deletes = writes(&short);
    assert!(!deletes.iter().any(|c| matches!(c, StorageCall::Delete(_))));
    assert_eq!(short.log().storage.files.get(&sources[0]), Some(&a));
    let catalog = saved(&store);
    assert_eq!(
        state_at(&catalog, "internal/video/a.mp4"),
        Some(EntryState::Pending)
    );
    assert_eq!(
        state_at(&catalog, "sd/video/A.mp4"),
        Some(EntryState::Stored)
    );
}

#[test]
fn a_failed_or_cancelled_move_keeps_the_source_and_stops_the_batch() {
    let len = 3 * FAKE_UPLOAD_CHUNK;
    let names = ["sd/video/one.mp4", "sd/video/two.mp4", "sd/video/three.mp4"];
    let data: Vec<Vec<u8>> = (1..=3).map(|n| clip(n, len)).collect();
    let on_card = |storage: FakeStorage| {
        let files = names.iter().zip(&data);
        files.fold(storage.with_card(CARD), |s, (n, d)| {
            s.with_file(remote(n), d.clone())
        })
    };
    let seeded = || {
        let mut store = MemoryArchive::new();
        let sent: Vec<(&str, Option<u64>, &[u8])> = names
            .iter()
            .zip(&data)
            .map(|(n, d)| (*n, Some(CARD), d.as_slice()))
            .collect();
        seed(&mut store, &key(), &sent);
        store
    };
    let sources: Vec<RemotePath> = names.iter().map(|n| remote(n)).collect();

    // Cancelled during the second upload: the first file moved, the second
    // and the third stay where they were.
    let connector = FakeConnector::with_storage(on_card(FakeStorage::default()));
    let mut store = seeded();
    let mut link = open(&connector);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let plan = manager
        .plan_move(&sources, Medium::Internal, &[])
        .expect("plans");
    let cancel = Cancel::During { step: 1, done: 1 };
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, cancel);
    let report = report.expect("a report");
    assert_eq!(report.done, plan.steps[..1]);
    assert_eq!(report.not_started, plan.steps[2..]);
    let stopped = report.stopped.expect("stopped");
    assert_eq!(stopped.stage, Stage::Upload);
    assert_eq!(
        stopped.halt,
        Halt::Cancelled {
            partial: Some(CHUNK)
        }
    );
    assert_eq!(stopped.halt.code(), "cancelled");
    let two = remote("internal/video/two.mp4");
    assert_eq!(
        stopped.leftover(),
        Some(FileEntry {
            path: two.clone(),
            size: Some(CHUNK)
        }),
        "the partial file, for a confirmed delete"
    );
    let files = connector.log().storage.files;
    assert!(!files.contains_key(&sources[0]), "the first one moved");
    assert_eq!(
        files.get(&sources[1]),
        Some(&data[1]),
        "the cancelled one stays"
    );
    assert_eq!(files.get(&sources[2]), Some(&data[2]), "the last one stays");
    let deleted: Vec<StorageCall> = writes(&connector)
        .into_iter()
        .filter(|c| matches!(c, StorageCall::Delete(_)))
        .collect();
    assert_eq!(deleted, [StorageCall::Delete(sources[0].clone())]);
    // The partial is cataloged pending, a leftover the cleanup pre-checks;
    // the source keeps its entry.
    let catalog = saved(&store);
    assert_eq!(
        state_at(&catalog, "internal/video/two.mp4"),
        Some(EntryState::Pending)
    );
    assert_eq!(
        state_at(&catalog, "sd/video/two.mp4"),
        Some(EntryState::Stored)
    );

    // Cancelled before a file starts: nothing is asked or sent.
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let plan = manager
        .plan_move(&sources[1..], Medium::Internal, &[])
        .expect("plans");
    assert_eq!(
        plan.skipped[0].skip.code(),
        "conflict",
        "the partial is there"
    );
    let asked = calls(&connector).len();
    let (report, progress) = run(&mut manager, &plan, Confirm::Yes, Cancel::First);
    let stopped = report.expect("a report").stopped.expect("stopped");
    assert_eq!(
        (stopped.stage, stopped.halt),
        (Stage::Preflight, Halt::Cancelled { partial: None })
    );
    assert_eq!(calls(&connector).len(), asked);
    assert!(progress.is_empty());

    // Cancelled once the copy is verified: the source is not deleted.
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::AfterVerify(0));
    let stopped = report.expect("a report").stopped.expect("stopped");
    assert_eq!(stopped.stage, Stage::Delete);
    assert!(stopped.copied() && stopped.leftover().is_none());
    let files = connector.log().storage.files;
    assert_eq!(
        files.get(&remote("internal/video/three.mp4")),
        Some(&data[2])
    );
    assert_eq!(files.get(&sources[2]), Some(&data[2]));
    let catalog = saved(&store);
    assert_eq!(
        state_at(&catalog, "internal/video/three.mp4"),
        Some(EntryState::Stored)
    );
    assert_eq!(
        state_at(&catalog, "sd/video/three.mp4"),
        Some(EntryState::Stored)
    );

    // A failure: the internal flash has room for one file only. The second
    // is refused before a byte is sent, with the files there as candidates.
    let small = FakeStorage {
        internal_total: (len + len / 2) as u64,
        ..FakeStorage::default()
    };
    let connector = FakeConnector::with_storage(on_card(small));
    let mut store = seeded();
    let mut link = open(&connector);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let plan = manager
        .plan_move(&sources, Medium::Internal, &[])
        .expect("plans");
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let report = report.expect("a report");
    assert_eq!(report.done.len(), 1);
    assert_eq!(report.not_started, plan.steps[2..]);
    let stopped = report.stopped.expect("stopped");
    let len = len as u64;
    let one = FileEntry {
        path: remote("internal/video/one.mp4"),
        size: Some(len),
    };
    assert_eq!(stopped.stage, Stage::Preflight);
    assert_eq!(
        stopped.halt,
        Halt::Refused(Refusal::NoSpace {
            needed: len,
            free: len / 2,
            candidates: vec![one]
        })
    );
    assert_eq!(stopped.halt.code(), "refused");
    let files = connector.log().storage.files;
    assert_eq!(files.get(&sources[1]), Some(&data[1]));
    assert!(!files.contains_key(&two), "nothing was sent for it");
    let catalog = saved(&store);
    assert_eq!(state_at(&catalog, "internal/video/two.mp4"), None);
}

#[test]
fn restore_checks_space_and_the_cap_before_sending_anything() {
    const SMALL_CARD: u64 = 1_000_000;
    let first = clip(1, 300_000);
    let second = clip(2, 400_000);
    let third = clip(3, 500_000);
    let there = clip(4, 1000);
    let card = FakeStorage::default()
        .with_card(SMALL_CARD)
        .with_file(remote("sd/video/there.mp4"), there.clone());
    let connector = FakeConnector::with_storage(card);
    let mut store = MemoryArchive::new();
    seed(
        &mut store,
        &key(),
        &[
            ("sd/video/first.mp4", Some(OTHER_CARD), &first),
            ("sd/video/second.mp4", Some(OTHER_CARD), &second),
            ("sd/video/third.mp4", Some(OTHER_CARD), &third),
            ("sd/video/there.mp4", Some(OTHER_CARD), &there),
        ],
    );
    let mut link = open(&connector);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let inventory = manager.inventory().expect("lists");
    let selection = inventory.overview.restorable(Medium::Card);
    assert_eq!(selection.len(), 4, "the other card's files");

    // 1.2 MB do not fit in the 999,000 bytes free: refused before a byte.
    assert_eq!(
        manager.plan_restore(&selection, Medium::Card, &[]),
        Err(ManagerError::Refused(PlanRefusal::NoSpace {
            needed: 1_200_000,
            free: 999_000
        }))
    );
    // Nor does a file over the 25 MiB per-file limit of rev C.
    let mut huge = selection[0].clone();
    huge.path = remote("sd/video/huge.mp4");
    huge.size = 25 * MIB + 1;
    let refused = manager.plan_restore(&[huge], Medium::Card, &[]);
    let Err(ManagerError::Refused(refusal)) = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(refusal.code(), "unsendable");
    assert!(writes(&connector).is_empty());

    // Two files fit; the one there with the same size is skipped as present.
    let chosen = [
        selection[0].clone(),
        selection[1].clone(),
        selection[3].clone(),
    ];
    let plan = manager
        .plan_restore(&chosen, Medium::Card, &[])
        .expect("fits");
    assert_eq!(
        targets(&plan.steps),
        ["sd/video/first.mp4", "sd/video/second.mp4"],
        "oldest sent first"
    );
    assert_eq!(plan.skipped[0].skip, Skip::Present);
    assert_eq!(plan.transfer, Transfer::Restore);

    // The card fills up before the run: the space is checked again before
    // the first byte, and nothing is sent.
    port_upload(link.as_mut(), &remote("sd/image/filler.png"), &[7; 400_000]);
    let before = writes(&connector).len();
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let (refused, progress) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    assert_eq!(
        refused,
        Err(ManagerError::Refused(PlanRefusal::NoSpace {
            needed: 700_000,
            free: 599_000
        }))
    );
    assert_eq!(writes(&connector).len(), before);
    assert!(progress.is_empty());

    // With room again it runs, one verified file at a time, and deletes
    // nothing.
    port_delete(link.as_mut(), &remote("sd/image/filler.png"));
    let asked = calls(&connector).len();
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let report = report.expect("restores");
    assert!(report.completed());
    assert_eq!(
        trace(&connector, &plan.steps, asked),
        [
            "upload sd/video/first.mp4",
            "size sd/video/first.mp4",
            "upload sd/video/second.mp4",
            "size sd/video/second.mp4",
        ]
    );
    let files = connector.log().storage.files;
    assert_eq!(files.get(&remote("sd/video/first.mp4")), Some(&first));
    assert_eq!(files.get(&remote("sd/video/second.mp4")), Some(&second));
    // The restored files are cataloged on this card; the other card's
    // entries stay.
    let catalog = saved(&store);
    let record = catalog.screen(&key()).expect("record");
    let on_card = |card| {
        record
            .entries
            .iter()
            .filter(|e| e.card == Some(card))
            .count()
    };
    assert_eq!((on_card(SMALL_CARD), on_card(OTHER_CARD)), (2, 4));
    let restored = record
        .entry(&remote("sd/video/first.mp4"), Some(SMALL_CARD))
        .expect("restored");
    assert_eq!(restored.state, EntryState::Stored);
    assert_eq!(restored.content, content_id(&first));
}

#[test]
fn uploads_are_recorded_pending_then_stored() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let data = clip(7, 3000);
    let big = clip(8, 3 * FAKE_UPLOAD_CHUNK);
    let mut files = LocalFiles::new()
        .with("/pc/Clip.mp4", data.clone())
        .with("/pc/big.mp4", big.clone());
    let mut spy = Spy {
        inner: MemoryArchive::new(),
        connector: connector.clone(),
        saves: Vec::new(),
    };
    let prepared = prepare(
        link.as_mut(),
        &mut files,
        "/pc/Clip.mp4",
        "clip.mp4",
        "internal/video",
    );
    let mut manager = Manager::new(link.as_mut(), &mut spy);
    let uploaded = upload(&mut manager, &mut files, &prepared, None).expect("uploads");
    assert_eq!(uploaded.bytes, 3000);

    // Cataloged pending before the first byte, stored after the size check.
    let path = remote("internal/video/clip.mp4");
    let calls = calls(&connector);
    let sent_at = calls
        .iter()
        .position(|c| *c == StorageCall::Upload(path.clone(), 3000))
        .expect("sent");
    let checked_at = calls
        .iter()
        .rposition(|c| *c == StorageCall::Size(path.clone()))
        .expect("checked");
    let states: Vec<(usize, Option<EntryState>)> = spy
        .saves
        .iter()
        .map(|(at, c)| (*at, state_at(c, "internal/video/clip.mp4")))
        .collect();
    assert_eq!(states.len(), 2, "{states:?}");
    assert_eq!(states[0].1, Some(EntryState::Pending));
    assert!(states[0].0 <= sent_at, "{states:?} / upload at {sent_at}");
    assert_eq!(states[1].1, Some(EntryState::Stored));
    assert!(
        states[1].0 > checked_at,
        "{states:?} / checked at {checked_at}"
    );
    // The exact bytes sent are the local copy.
    let entry = entry_at(&spy.saves[1].1, &key(), "internal/video/clip.mp4").expect("entry");
    assert_eq!(entry.content, content_id(&data));
    assert_eq!(spy.inner.read(&entry.content).expect("read"), Some(data));
    assert_eq!(entry.source.as_deref(), Some("/pc/Clip.mp4"));
    assert_eq!(
        (entry.duration, entry.resolution, entry.sent_at, entry.card),
        (Some(Duration::from_secs(1)), Some(NATIVE), NOW, None)
    );
    assert!(saved(&spy.inner).has_copy(&entry.content));

    // Cancelled before the first byte arrived: nothing is left, so the
    // entry and its copy go.
    let prepared = prepare(
        link.as_mut(),
        &mut files,
        "/pc/big.mp4",
        "big.mp4",
        "internal/video",
    );
    let mut manager = Manager::new(link.as_mut(), &mut spy);
    let cancelled = upload(&mut manager, &mut files, &prepared, Some(0));
    assert_eq!(cancelled, Err(BezelError::Cancelled { partial: None }));
    let catalog = saved(&spy.inner);
    assert_eq!(state_at(&catalog, "internal/video/big.mp4"), None);
    assert!(!spy.inner.holds(&content_id(&big)) && !catalog.has_copy(&content_id(&big)));
    assert_eq!(spy.inner.copies(), 1);

    // Cancelled mid-transfer: the partial stays pending, a leftover the
    // cleanup assistant pre-checks.
    let mut manager = Manager::new(link.as_mut(), &mut spy);
    let cancelled = upload(&mut manager, &mut files, &prepared, Some(1));
    assert_eq!(
        cancelled,
        Err(BezelError::Cancelled {
            partial: Some(CHUNK)
        })
    );
    let cleanup = manager.cleanup().expect("findings");
    let found: Vec<(String, Code)> = cleanup
        .findings
        .iter()
        .map(|f| (f.file.path.to_string(), f.code()))
        .collect();
    assert_eq!(
        found,
        [("internal/video/big.mp4".to_string(), Code::Pending)]
    );
    assert!(spy.inner.holds(&content_id(&big)));

    // The boot media and a delete through the manager are recorded; the
    // deleted file's copy now counts against the cache limit.
    let mut manager = Manager::new(link.as_mut(), &mut spy);
    let boot = BootMedia::File(path.clone());
    manager
        .set_boot_media(&boot, None, Confirm::Yes)
        .expect("sets the boot media");
    let asked = writes(&connector).len();
    let refused = manager.delete(&path, Confirm::No);
    assert!(matches!(refused, Err(BezelError::NotConfirmed(_))));
    assert_eq!(writes(&connector).len(), asked);
    manager.delete(&path, Confirm::Yes).expect("deletes");
    let catalog = saved(&spy.inner);
    assert_eq!(
        state_at(&catalog, "internal/video/clip.mp4"),
        Some(EntryState::Deleted)
    );
    assert_eq!(
        catalog.screen(&key()).and_then(|r| r.boot.clone()),
        Some(path)
    );
    let cache = manager::cache_info(&mut spy).expect("cache");
    assert_eq!((cache.deleted_copies, cache.deleted_bytes), (1, 3000));
}

#[test]
fn renames_and_copies_resend_the_local_copy() {
    let amd = clip(1, 6000);
    let connector = FakeConnector::with_storage(FakeStorage::default().with_card(CARD));
    let mut link = open(&connector);
    let mut store = MemoryArchive::new();
    let mut files = LocalFiles::new().with("/pc/AMD.mp4", amd.clone());
    let prepared = prepare(
        link.as_mut(),
        &mut files,
        "/pc/AMD.mp4",
        "amd.mp4",
        "internal/video",
    );
    let theme = [AssetRef("assets/AMD.mp4".into())];
    let mut manager = Manager::new(link.as_mut(), &mut store).protecting(theme.clone());
    upload(&mut manager, &mut files, &prepared, None).expect("uploads");

    // Renaming a video a theme plays warns, then re-sends the copy under the
    // new name and deletes the old one once verified.
    let old = remote("internal/video/amd.mp4");
    let plan = manager
        .plan_rename(&old, "AMD_Logo.mp4", &[])
        .expect("plans");
    assert_eq!(plan.warnings, [Warning::ThemeVideo(old.clone())]);
    let asked = calls(&connector).len();
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    assert!(report.expect("renames").completed());
    assert_eq!(
        trace(&connector, &plan.steps, asked),
        [
            "upload internal/video/amd_logo.mp4",
            "size internal/video/amd_logo.mp4",
            "delete internal/video/amd.mp4",
        ]
    );
    let catalog = saved(&store);
    let renamed = entry_at(&catalog, &key(), "internal/video/amd_logo.mp4").expect("entry");
    assert_eq!(
        renamed.source.as_deref(),
        Some("/pc/AMD.mp4"),
        "what is known stays"
    );
    assert_eq!(renamed.duration, Some(Duration::from_secs(1)));
    assert_eq!(state_at(&catalog, "internal/video/amd.mp4"), None);

    // Copying to the card keeps the original; copying again needs the
    // overwrite confirmed, and replaces the file there.
    let logo = remote("internal/video/amd_logo.mp4");
    let mut manager = Manager::new(link.as_mut(), &mut store).protecting(theme);
    let plan = manager
        .plan_copy(std::slice::from_ref(&logo), Medium::Card, &[])
        .expect("plans");
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    assert!(report.expect("copies").completed());
    let on_card = remote("sd/video/amd_logo.mp4");
    let files = connector.log().storage.files;
    assert_eq!(files.get(&on_card), Some(&amd));
    assert_eq!(files.get(&logo), Some(&amd), "the original stays");
    let again = manager
        .plan_copy(std::slice::from_ref(&logo), Medium::Card, &[])
        .expect("plans");
    assert!(again.steps.is_empty() && again.skipped[0].skip.code() == "conflict");
    let overwrite = [on_card.clone()];
    let again = manager
        .plan_copy(std::slice::from_ref(&logo), Medium::Card, &overwrite)
        .expect("plans");
    assert_eq!(
        again.steps[0].replaces.as_ref().map(|r| &r.path),
        Some(&on_card)
    );
    let (report, _) = run(&mut manager, &again, Confirm::Yes, Cancel::Never);
    assert!(report.expect("copies").completed());
    let catalog = saved(&store);
    let record = catalog.screen(&key()).expect("record");
    assert_eq!(record.entries.len(), 2, "one entry per place");
    let copied = record.entry(&on_card, Some(CARD)).expect("entry");
    assert_eq!(
        (copied.state, copied.card),
        (EntryState::Stored, Some(CARD))
    );
    assert!(
        !writes(&connector)
            .iter()
            .any(|c| *c == StorageCall::Delete(logo.clone())),
        "a copy deletes nothing"
    );

    // Moving the boot media warns.
    let mut manager = Manager::new(link.as_mut(), &mut store);
    manager
        .set_boot_media(&BootMedia::File(logo.clone()), None, Confirm::Yes)
        .expect("sets the boot media");
    let plan = manager
        .plan_move(std::slice::from_ref(&logo), Medium::Card, &overwrite)
        .expect("plans");
    assert_eq!(plan.warnings, [Warning::BootMedia(logo)]);
}

#[test]
fn a_screen_that_changed_since_the_plan_stops_the_batch() {
    let (a, b) = (clip(1, 2000), clip(2, 2500));
    let card = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote("sd/video/a.mp4"), a.clone())
        .with_file(remote("sd/video/b.mp4"), b.clone());
    let connector = FakeConnector::with_storage(card);
    let mut store = MemoryArchive::new();
    seed(
        &mut store,
        &key(),
        &[
            ("sd/video/a.mp4", Some(CARD), &a),
            ("sd/video/b.mp4", Some(CARD), &b),
        ],
    );
    let mut link = open(&connector);
    let sources = [remote("sd/video/a.mp4"), remote("sd/video/b.mp4")];
    let plan = Manager::new(link.as_mut(), &mut store)
        .plan_move(&sources, Medium::Internal, &[])
        .expect("plans");

    // The source is gone: nothing is sent for it.
    port_delete(link.as_mut(), &sources[0]);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let report = report.expect("a report");
    let stopped = report.stopped.expect("stopped");
    assert_eq!(
        (stopped.stage, stopped.halt.code()),
        (Stage::Preflight, "sourceChanged")
    );
    assert_eq!(report.not_started.len(), 1);

    // A file took the target's name: it is not replaced.
    let target = remote("internal/video/b.mp4");
    let plan = manager
        .plan_move(&sources[1..], Medium::Internal, &[])
        .expect("plans");
    port_upload(link.as_mut(), &target, &[9; 10]);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let stopped = report.expect("a report").stopped.expect("stopped");
    let taken = FileEntry {
        path: target.clone(),
        size: Some(10),
    };
    assert_eq!(stopped.halt, Halt::Conflict(taken));

    // Its replacement confirmed, but the local copy was cleared meanwhile.
    let overwrite = [target.clone()];
    let plan = manager
        .plan_move(&sources[1..], Medium::Internal, &overwrite)
        .expect("plans");
    let cleared = manager::clear_cache(&mut store, Clear::All, Confirm::Yes).expect("clears");
    assert_eq!(
        cleared,
        Cleared {
            copies: 2,
            bytes: 4500
        }
    );
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let (report, _) = run(&mut manager, &plan, Confirm::Yes, Cancel::Never);
    let stopped = report.expect("a report").stopped.expect("stopped");
    assert_eq!(stopped.halt, Halt::NoLocalCopy);
    let files = connector.log().storage.files;
    assert_eq!(files.get(&sources[1]), Some(&b));
    assert_eq!(files.get(&target), Some(&vec![9; 10]));
    // Each reason reads as a sentence.
    let halts = [
        (Halt::Cancelled { partial: Some(5) }, "5 bytes remains"),
        (Halt::SourceChanged, "gone or changed"),
        (stopped.halt.clone(), "local copy is gone"),
        (
            Halt::Refused(Refusal::NoCard),
            "refused: the screen has no memory card",
        ),
        (Halt::Failed(BezelError::Timeout("x".into())), "timeout"),
    ];
    for (halt, text) in halts {
        assert!(halt.to_string().contains(text), "{halt}");
    }
    let steps = [
        Stage::Preflight,
        Stage::Upload,
        Stage::Verify,
        Stage::Delete,
    ]
    .map(Stage::slug);
    assert_eq!(steps, ["preflight", "upload", "verify", "delete"]);
}

#[test]
fn cleanup_deletes_exactly_the_confirmed_list() {
    let card = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote("sd/video/demon.mp4"), clip(1, 4000))
        .with_file(remote("sd/video/demon.mp4.mp4"), clip(1, 4000))
        .with_file(remote("sd/video/NVI.mp4"), clip(2, 5000))
        .with_file(remote("sd/video/NVI.mp427034822.mp4"), clip(3, 5200))
        .with_file(remote("sd/video/amd.mp4"), clip(4, 6000))
        .with_file(remote("sd/video/half.mp4"), clip(5, 700));
    let connector = FakeConnector::with_storage(card);
    let mut store = MemoryArchive::new();
    let mut catalog = Catalog::default();
    let content = store.keep(&clip(5, 7000)).expect("keep");
    catalog.copies.insert(content.clone());
    let half = ArchiveEntry::pending(remote("sd/video/half.mp4"), Some(CARD), 7000, content, 1);
    catalog.screen_mut(&key()).record(half);
    store.save(&catalog).expect("save");
    let mut link = open(&connector);
    let theme = [AssetRef("assets/AMD.mp4".into())];
    let mut manager = Manager::new(link.as_mut(), &mut store).protecting(theme);
    let cleanup = manager.cleanup().expect("findings");
    let found: Vec<(&str, &str, bool)> = cleanup
        .findings
        .iter()
        .map(|f| (f.file.path.name.as_str(), f.code().slug(), f.prechecked()))
        .collect();
    assert_eq!(
        found,
        [
            ("NVI.mp4", "unused", false),
            ("NVI.mp427034822.mp4", "variant", false),
            ("demon.mp4", "unused", false),
            ("demon.mp4.mp4", "duplicate", true),
            ("half.mp4", "pending", true),
        ],
        "the theme's video is never suggested"
    );
    assert!(writes(&connector).is_empty(), "the assistant only asks");

    let chosen = cleanup.prechecked();
    let token = CancelToken::new();
    let refused = manager.delete_files(&chosen, Confirm::No, &token);
    assert_eq!(
        refused,
        Err(BezelError::NotConfirmed("deleting 2 files".into()))
    );
    assert!(writes(&connector).is_empty());
    let report = manager
        .delete_files(&chosen, Confirm::Yes, &token)
        .expect("deletes");
    assert_eq!(report.deleted, chosen);
    assert_eq!((report.freed(), report.stopped), (4700, None));
    assert_eq!(
        writes(&connector),
        [
            StorageCall::Delete(remote("sd/video/demon.mp4.mp4")),
            StorageCall::Delete(remote("sd/video/half.mp4")),
        ]
    );
    assert_eq!(connector.log().storage.files.len(), 4);
    let catalog = saved(&store);
    assert_eq!(
        state_at(&catalog, "sd/video/half.mp4"),
        Some(EntryState::Deleted)
    );

    // A file that changed since the list was confirmed stops the batch, and
    // so does a cancel; nothing else is deleted.
    let stale = [
        FileEntry {
            path: remote("sd/video/NVI.mp4"),
            size: Some(1),
        },
        FileEntry {
            path: remote("sd/video/demon.mp4"),
            size: Some(4000),
        },
    ];
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let report = manager
        .delete_files(&stale, Confirm::Yes, &token)
        .expect("a report");
    assert_eq!(
        report.stopped,
        Some(Undeleted {
            file: stale[0].clone(),
            halt: Halt::SourceChanged
        })
    );
    assert_eq!(report.not_started, stale[1..]);
    token.cancel();
    let report = manager
        .delete_files(&stale[1..], Confirm::Yes, &token)
        .expect("a report");
    let halt = report.stopped.map(|u| u.halt);
    assert_eq!(halt, Some(Halt::Cancelled { partial: None }));
    assert_eq!(writes(&connector).len(), 2);
}

#[test]
fn turing_usb_shows_cataloged_sizes_and_never_deletes() {
    let usb = ScreenKey::new(ModelId("turing-usb-8.8"));
    let ours = clip(1, 2000);
    let vendor = remote("internal/video/vendor.h264");
    let screen = FakeStorage::default()
        .with_card(CARD)
        .with_file_of_unknown_size(remote("internal/video/ours.h264"), ours.clone())
        .with_file_of_unknown_size(vendor.clone(), clip(2, 3000));
    let connector = FakeConnector::with_storage(screen);
    let mut store = MemoryArchive::new();
    seed(
        &mut store,
        &usb,
        &[("internal/video/ours.h264", None, &ours)],
    );
    let mut link = open_usb(&connector);
    let mut manager = Manager::new(link.as_mut(), &mut store);
    assert_eq!(manager.key(), &usb);
    let inventory = manager.inventory().expect("lists");
    assert_eq!(inventory.deletes, Deletes::Unsupported);
    let sizes: Vec<(String, Option<u64>)> = inventory
        .overview
        .listed
        .iter()
        .map(|l| (l.file.path.to_string(), Inventory::size(l)))
        .collect();
    assert_eq!(
        sizes,
        [
            ("internal/video/ours.h264".to_string(), Some(2000)),
            ("internal/video/vendor.h264".to_string(), None),
        ],
        "the catalog's size, or unknown"
    );
    let entry = inventory.overview.listed[0].entry.clone().expect("ours");
    assert!(inventory.has_copy(&entry));
    assert_eq!(inventory.record().map(|r| r.entries.len()), Some(1));
    assert_eq!(inventory.boot(), None);

    // Nothing that ends in a delete runs.
    let ours_path = remote("internal/video/ours.h264");
    let unsupported = |result: Result<()>| matches!(result, Err(BezelError::Unsupported(_)));
    assert!(unsupported(manager.delete(&ours_path, Confirm::Yes)));
    assert!(unsupported(manager.cleanup().map(drop)));
    let token = CancelToken::new();
    let listed = [FileEntry {
        path: vendor.clone(),
        size: None,
    }];
    assert!(unsupported(
        manager
            .delete_files(&listed, Confirm::Yes, &token)
            .map(drop)
    ));
    let moved = manager
        .plan_move(std::slice::from_ref(&ours_path), Medium::Card, &[])
        .expect("plans");
    assert!(moved.steps.is_empty());
    assert_eq!(moved.skipped[0].skip, Skip::DeleteUnsupported);
    let copy = manager
        .plan_copy(std::slice::from_ref(&ours_path), Medium::Card, &[])
        .expect("plans");
    let forced = TransferPlan {
        transfer: Transfer::Move,
        ..copy.clone()
    };
    let (refused, _) = run(&mut manager, &forced, Confirm::Yes, Cancel::Never);
    assert!(matches!(
        refused,
        Err(ManagerError::Failed(BezelError::Unsupported(_)))
    ));
    assert!(writes(&connector).is_empty());

    // Copying to the card runs: the original stays.
    let (report, _) = run(&mut manager, &copy, Confirm::Yes, Cancel::Never);
    assert!(report.expect("copies").completed());
    let files = connector.log().storage.files;
    assert_eq!(files.get(&remote("sd/video/ours.h264")), Some(&ours));
    assert_eq!(files.get(&ours_path), Some(&ours));
    let catalog = saved(&store);
    let copied = entry_at(&catalog, &usb, "sd/video/ours.h264").expect("entry");
    assert_eq!(
        (copied.state, copied.card),
        (EntryState::Stored, Some(CARD))
    );
}

#[test]
fn associating_an_original_makes_a_vendor_file_movable() {
    let nvi = clip(1, 5000);
    let kept = clip(2, 800);
    let card = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote("sd/video/NVI.mp4"), nvi.clone())
        .with_file(remote("internal/video/kept.mp4"), kept.clone());
    let connector = FakeConnector::with_storage(card);
    let mut store = MemoryArchive::new();
    seed(
        &mut store,
        &key(),
        &[("internal/video/kept.mp4", None, &kept)],
    );
    let mut files = LocalFiles::new()
        .with("/pc/clips/other.mp4", clip(3, 5000))
        .with("/pc/clips/NVI.mp4", nvi.clone())
        .with("/pc/clips/big.mp4", clip(4, 5001));
    let mut link = open(&connector);
    let manager = Manager::new(link.as_mut(), &mut store).named(" desk ");
    assert_eq!(manager.key().name.as_deref(), Some("desk"));
    assert!(format!("{manager:?}").contains("desk"));
    let mut manager = Manager::new(link.as_mut(), &mut store);
    let path = remote("sd/video/NVI.mp4");

    // Without a local copy it cannot move.
    let plan = manager
        .plan_move(std::slice::from_ref(&path), Medium::Internal, &[])
        .expect("plans");
    assert_eq!(plan.skipped[0].skip, Skip::NoLocalCopy);

    // Its candidates: exactly its size and kind, the likeliest name first;
    // what cannot be probed is left out.
    let sources = [
        local("/pc/clips/other.mp4"),
        local("/pc/clips/NVI.mp4"),
        local("/pc/clips/big.mp4"),
        local("/pc/clips/gone.mp4"),
    ];
    let ranked = manager
        .originals(&mut files, &path, &sources)
        .expect("ranks");
    let order: Vec<&str> = ranked.iter().map(|c| c.source.as_str()).collect();
    assert_eq!(order, ["/pc/clips/NVI.mp4", "/pc/clips/other.mp4"]);

    // A file of another size is not its original.
    let wrong = manager.associate(&mut files, &path, &local("/pc/clips/big.mp4"), NOW);
    assert!(matches!(wrong, Err(BezelError::InvalidInput(_))));
    let absent = manager.associate(&mut files, &remote("sd/video/x.mp4"), &sources[1], NOW);
    assert!(matches!(absent, Err(BezelError::InvalidInput(_))));
    let entry = manager
        .associate(&mut files, &path, &sources[1], NOW)
        .expect("associates");
    assert_eq!((entry.state, entry.card), (EntryState::Stored, Some(CARD)));
    assert_eq!(entry.content, content_id(&nvi));
    assert_eq!(entry.source.as_deref(), Some("/pc/clips/NVI.mp4"));
    assert!(writes(&connector).is_empty(), "the screen is only asked");
    let plan = manager
        .plan_move(std::slice::from_ref(&path), Medium::Internal, &[])
        .expect("plans");
    assert_eq!(targets(&plan.steps), ["internal/video/nvi.mp4"]);

    // Forgetting drops the entry and the copy nothing else names.
    let forgotten = manager::forget(&mut store, &key(), &path, Some(CARD)).expect("forgets");
    assert_eq!(forgotten, Some(entry));
    assert!(!store.holds(&content_id(&nvi)));

    // The cache: the copies of deleted files go when over the limit or
    // cleared (with the confirmation); the entries stay.
    let mut manager = Manager::new(link.as_mut(), &mut store);
    manager
        .associate(&mut files, &path, &sources[1], NOW)
        .expect("associates");
    manager.delete(&path, Confirm::Yes).expect("deletes");
    let cache = manager::cache_info(&mut store).expect("cache");
    assert_eq!((cache.copies, cache.bytes), (2, 5800));
    assert_eq!((cache.deleted_copies, cache.deleted_bytes), (1, 5000));
    let refused = manager::clear_cache(&mut store, Clear::Deleted, Confirm::No);
    assert!(matches!(refused, Err(BezelError::NotConfirmed(_))));
    assert_eq!(store.copies(), 2);
    let evicted = manager::set_cache_limit(&mut store, 0).expect("limits");
    assert_eq!(
        evicted,
        Cleared {
            copies: 1,
            bytes: 5000
        }
    );
    assert_eq!(saved(&store).limit, 0);
    let cleared = manager::clear_cache(&mut store, Clear::Deleted, Confirm::Yes).expect("clears");
    assert_eq!(cleared, Cleared::default());
    let cleared = manager::clear_cache(&mut store, Clear::All, Confirm::Yes).expect("clears");
    assert_eq!(
        cleared,
        Cleared {
            copies: 1,
            bytes: 800
        }
    );
    assert_eq!(store.copies(), 0);
    let catalog = saved(&store);
    assert_eq!(catalog.screen(&key()).expect("record").entries.len(), 2);
    assert_eq!(
        state_at(&catalog, "sd/video/NVI.mp4"),
        Some(EntryState::Deleted)
    );
}

#[test]
fn a_batch_reports_its_progress_and_screens_without_storage_are_unsupported() {
    let token = CancelToken::new();
    let mut sink = |_: StepProgress| {};
    let batch = Batch::new(&token, &mut sink);
    assert!(format!("{batch:?}").contains("cancelled: false"));
    let refusal = ManagerError::from(PlanRefusal::NoCard);
    assert_eq!(refusal.to_string(), "the screen has no memory card");
    let failure = ManagerError::from(BezelError::Timeout("the screen".into()));
    assert_eq!(failure.to_string(), "timeout talking to the screen");

    // The WeAct 0.96" stores nothing.
    let connector = FakeConnector::default();
    let bus = FakeBus::new(vec![Endpoint {
        address: DeviceAddress("/dev/ttyACM0".into()),
        transport: Transport::Serial,
        usb: UsbId::new(0x1a86, 0xfe0c),
        serial_number: Some("AD0001".into()),
        manufacturer: None,
        product: None,
        location: None,
    }]);
    let mut link = open_screen(&bus, &connector, None).expect("opens the fake WeAct");
    let mut store = MemoryArchive::new();
    let mut manager = Manager::new(link.as_mut(), &mut store);
    assert!(matches!(
        manager.inventory(),
        Err(BezelError::Unsupported(_))
    ));
    let sources = [remote("sd/video/a.mp4")];
    let plan = manager.plan_move(&sources, Medium::Internal, &[]);
    assert!(matches!(
        plan,
        Err(ManagerError::Failed(BezelError::Unsupported(_)))
    ));
}
