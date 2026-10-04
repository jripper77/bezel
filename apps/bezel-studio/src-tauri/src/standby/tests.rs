//! "When the computer shuts down" on the fake 8.8" (DoD row 4): the
//! commands' backend methods with the catalog on disk, in a folder another
//! writer (the CLI) opens too, and a scripted media converter.

use std::path::{Path, PathBuf};

use bezel_core::domain::archive::{Catalog, EntryState};
use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::device::{ModelId, Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::standby::{PlanB, SleepMinutes};
use bezel_core::domain::storage::{BootMedia, StartMode};
use bezel_core::ports::{ArchiveStore, DeviceBus as _};
use bezel_devices::FakeBus;
use bezel_devices::fake::{FakeStorage, StorageCall};
use bezel_media::archive::DiskArchive;
use image::{Rgb, RgbImage};

use super::*;
use crate::manager::Copies;
use crate::storage::tests::{FakeMedia, Fixture, KEY, TIME, fixture_on, remote_path};

/// The fake 8.8"'s MCU: the screen's one port while it sleeps.
const MCU: &str = "/dev/ttyACM0";

/// The 8.8"'s model: the studio keys its screens' choices by model.
const MODEL: ModelId = ModelId("turing-8.8");

/// A card of 1 GiB.
const CARD: u64 = 1 << 30;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// The studio of a test on a fake bus, its catalog on disk in a folder of
/// its own (removed when dropped) that "the CLI" opens too.
struct Setup {
    f: Fixture,
    catalog: PathBuf,
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.catalog);
    }
}

impl Setup {
    /// The test `name`'s studio over `bus`, its screens storing `storage`.
    fn on(name: &str, bus: FakeBus, storage: FakeStorage) -> Self {
        let catalog =
            std::env::temp_dir().join(format!("bezel-standby-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&catalog);
        let copies = Copies::on_disk(DiskArchive::open(&catalog).unwrap());
        let f = fixture_on(
            &format!("standby-{name}"),
            bus,
            storage,
            FakeMedia::ready(),
            copies,
        );
        Self { f, catalog }
    }

    /// The test `name`'s studio on the fake 8.8" storing `storage`.
    fn new(name: &str, storage: FakeStorage) -> Self {
        Self::on(name, FakeBus::turing_88(), storage)
    }

    /// The catalog as another writer (the CLI) opens it.
    fn cli(&self) -> DiskArchive {
        DiskArchive::open(&self.catalog).unwrap()
    }

    /// The catalog on disk now.
    fn catalog(&self) -> Catalog {
        self.cli().load().unwrap()
    }

    /// What the catalog on disk records for the 8.8" now.
    fn recorded(&self) -> Standby {
        let record = self.catalog().screen(&ScreenKey::new(MODEL)).cloned();
        record.map(|r| r.standby).unwrap_or_default()
    }

    /// Records `edit` of the 8.8"'s record as the CLI does, the studio open.
    fn cli_records(&self, edit: impl FnOnce(&mut bezel_core::domain::archive::ScreenRecord)) {
        let mut archive = self.cli();
        let mut catalog = archive.load().unwrap();
        edit(catalog.screen_mut(&ScreenKey::new(MODEL)));
        archive.save(&catalog).unwrap();
    }

    fn overview(&self, screen: &str) -> UiResult<StandbyDto> {
        self.f.backend.standby_overview(screen, TIME)
    }

    fn set(&self, asked: &Asked, confirm: Confirm) -> UiResult<StandbyDto> {
        self.f.backend.set_standby(KEY, asked, confirm, TIME)
    }

    /// Adds `photo` to the album as `name`, `confirm` being the answer to
    /// the dialog; a photo of that name is not replaced.
    fn add(
        &self,
        photo: &Path,
        fit: &str,
        name: &str,
        confirm: Confirm,
    ) -> UiResult<AlbumAddedDto> {
        let asked = asked_photo(photo, fit, name, Confirm::No);
        self.f.backend.album_add(KEY, &asked, confirm, TIME)
    }

    /// Adds `photo` to the album as `name`, the dialog and the replacement
    /// of a photo of that name both confirmed.
    fn replace(&self, photo: &Path, name: &str) -> UiResult<AlbumAddedDto> {
        let asked = asked_photo(photo, "cover", name, Confirm::Yes);
        self.f.backend.album_add(KEY, &asked, Confirm::Yes, TIME)
    }

    /// Storage calls that change what the screen stores, shows or keeps.
    fn writes(&self) -> Vec<StorageCall> {
        self.f.writes()
    }

    /// Remembers `orientation` as the one last used with the 8.8".
    fn stand(&self, orientation: Orientation) {
        self.f
            .backend
            .settings
            .update(|s| s.remember_orientation(KEY, orientation));
    }

    /// A 400x200 photo on the PC, its left half red and its right half
    /// blue, as a PNG called `name`.
    fn wide_photo(&self, name: &str) -> PathBuf {
        let mut photo = RgbImage::from_pixel(400, 200, Rgb([0, 0, 255]));
        for (x, _, pixel) in photo.enumerate_pixels_mut() {
            if x < 200 {
                *pixel = Rgb([255, 0, 0]);
            }
        }
        let path = self.f.root.join("photos").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        photo.save(&path).unwrap();
        path
    }
}

/// A photo as the window asks to add it.
fn asked_photo(photo: &Path, fit: &str, name: &str, replace: Confirm) -> PhotoAsked {
    PhotoAsked {
        source: photo.to_path_buf(),
        fit: fit.into(),
        name: name.into(),
        replace,
    }
}

fn ask(choice: &str, sleep_minutes: Option<u32>, file: Option<&str>) -> Asked {
    Asked {
        choice: choice.into(),
        sleep_minutes,
        file: file.map(str::to_string),
    }
}

fn eight_eight() -> &'static DeviceModel {
    model_by_id(MODEL).unwrap()
}

/// A card, a video on each medium.
fn with_videos() -> FakeStorage {
    FakeStorage::default()
        .with_card(CARD)
        .with_file(remote_path("internal/video/clip.mp4"), vec![1; 64])
        .with_file(remote_path("sd/video/rain.mp4"), vec![2; 64])
}

/// The picture a PNG holds, as a frame.
fn picture(png: &[u8]) -> Frame {
    let rgba = image::load_from_memory(png).unwrap().to_rgba8();
    let size = Size::new(rgba.width(), rgba.height());
    Frame::from_rgba(size, rgba.into_raw()).unwrap()
}

/// The picture of a PNG `data:` URL.
fn previewed(url: &str) -> Frame {
    let encoded = url.strip_prefix("data:image/png;base64,").unwrap();
    picture(&STANDARD.decode(encoded).unwrap())
}

fn at(frame: &Frame, x: u32, y: u32) -> [u8; 4] {
    let p = frame.pixel(x, y).unwrap();
    [p.r, p.g, p.b, p.a]
}

fn codes(options: &[crate::dto::StandbyOptionDto]) -> Vec<(&str, bool, Option<&str>)> {
    options
        .iter()
        .map(|o| (o.choice, o.enabled, o.reason))
        .collect()
}

/// D-2026-10-03-power-off-standby-2 (3): without the user's confirmation no
/// change reaches the screen, not even its opening, and the catalog on disk
/// stays as it was; nor does a photo for the album.
#[test]
fn without_confirmation_nothing_reaches_the_screen_nor_the_catalog() {
    let s = Setup::new("unconfirmed", with_videos());
    s.cli_records(|r| r.standby = Standby::Off(SleepMinutes::new(3).unwrap()));
    let before = s.catalog();
    let photo = s.wide_photo("beach.png");
    let asked = [
        ask("keep", None, None),
        ask("off", Some(5), None),
        ask("video", None, Some("sd/video/rain.mp4")),
        ask("album", None, None),
    ];
    for asked in &asked {
        let err = s.set(asked, Confirm::No).unwrap_err();
        assert_eq!(err.code(), "notConfirmed", "{asked:?}: {err}");
    }
    let err = s
        .add(&photo, "cover", "beach.png", Confirm::No)
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed", "{err}");
    assert_eq!(
        err.to_string(),
        "adding beach.png to the album needs confirmation"
    );
    let log = s.f.connector.log();
    assert_eq!(log.connects, 0, "the screen was not even opened");
    assert!(log.storage.calls.is_empty(), "{:?}", log.storage.calls);
    assert!(log.frames.is_empty());
    assert_eq!(s.catalog(), before, "the catalog is as it was");
    assert_eq!(s.recorded(), Standby::Off(SleepMinutes::new(3).unwrap()));
}

/// A confirmed choice writes its plan B (one OPTIONS) and is recorded;
/// `keep` undoes it (the boot media's start mode, no timer), and `keep`
/// again sends nothing. The overview has the UI's shape.
#[test]
fn a_choice_writes_its_plan_b_and_keep_undoes_it() {
    let s = Setup::new("choose", with_videos());
    let dto = s.overview(KEY).unwrap();
    assert_eq!(
        (dto.choice, dto.sleep_minutes, dto.file.as_deref(), dto.card),
        ("keep", None, None, true)
    );
    assert_eq!(
        codes(&dto.options),
        [
            ("keep", true, None),
            ("off", true, None),
            ("video", true, None),
            ("album", true, None)
        ]
    );
    let videos: Vec<&str> = dto.videos.iter().map(|v| v.path.as_str()).collect();
    assert_eq!(videos, ["internal/video/clip.mp4", "sd/video/rain.mp4"]);
    assert_eq!(dto.orientation, "landscape", "the 8.8\"'s own");
    assert!(s.writes().is_empty(), "the overview only asks");
    let json = serde_json::to_value(&dto).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "card",
            "choice",
            "file",
            "options",
            "orientation",
            "sleepMinutes",
            "videos"
        ]
    );
    assert_eq!(
        json["options"][2],
        serde_json::json!({"choice": "video", "enabled": true, "reason": null})
    );
    // With its size, as the demo lists it (review W9 of iteration 1).
    assert_eq!(
        json["videos"][1],
        serde_json::json!({"path": "sd/video/rain.mp4", "medium": "sd", "kind": "video",
            "name": "rain.mp4", "size": 64})
    );

    let off = s.set(&ask("off", Some(5), None), Confirm::Yes).unwrap();
    assert_eq!((off.choice, off.sleep_minutes), ("off", Some(5)));
    assert_eq!(s.recorded(), Standby::Off(SleepMinutes::new(5).unwrap()));
    let video = s
        .set(&ask("video", None, Some("sd/video/rain.mp4")), Confirm::Yes)
        .unwrap();
    assert_eq!(
        (video.choice, video.sleep_minutes, video.file.as_deref()),
        ("video", None, Some("sd/video/rain.mp4"))
    );
    assert_eq!(
        s.recorded(),
        Standby::Video(remote_path("sd/video/rain.mp4"))
    );
    let keep = s.set(&ask("keep", None, None), Confirm::Yes).unwrap();
    assert_eq!((keep.choice, keep.file), ("keep", None));
    assert_eq!(s.recorded(), Standby::Keep);
    assert_eq!(
        s.writes(),
        [
            StorageCall::Options(PlanB::new(StartMode::Default, 5)),
            StorageCall::Options(PlanB::new(StartMode::Video, 0)),
            StorageCall::Options(PlanB::new(StartMode::Default, 0)),
        ],
        "one OPTIONS each, keep undoing the others"
    );
    s.set(&ask("keep", None, None), Confirm::Yes).unwrap();
    assert_eq!(s.writes().len(), 3, "keep to keep sends nothing");
}

/// D-2026-10-03-power-off-standby-2 (2): the catalog on disk is read at
/// every call, so what the CLI records while the studio runs shows, and the
/// studio's choice keeps the CLI's boot media and is what the CLI reads.
#[test]
fn the_catalog_another_writer_shares_is_read_at_every_call() {
    let s = Setup::new("shared", with_videos());
    s.cli_records(|r| r.standby = Standby::Video(remote_path("internal/video/clip.mp4")));
    let dto = s.overview(KEY).unwrap();
    assert_eq!(
        (dto.choice, dto.file.as_deref()),
        ("video", Some("internal/video/clip.mp4"))
    );
    s.cli_records(|r| {
        r.standby = Standby::Off(SleepMinutes::new(7).unwrap());
        r.set_boot(&BootMedia::File(remote_path("internal/video/clip.mp4")));
    });
    let dto = s.overview(KEY).unwrap();
    assert_eq!((dto.choice, dto.sleep_minutes), ("off", Some(7)));

    let keep = s.set(&ask("keep", None, None), Confirm::Yes).unwrap();
    assert_eq!(keep.choice, "keep");
    assert_eq!(
        s.writes(),
        [StorageCall::Options(PlanB::new(StartMode::Video, 0))],
        "keep goes back to the start mode of the boot media the CLI set"
    );
    s.set(&ask("album", None, None), Confirm::Yes).unwrap();
    let catalog = s.catalog();
    let record = catalog.screen(&ScreenKey::new(MODEL)).unwrap();
    assert_eq!(record.standby, Standby::Album, "what the CLI reads");
    assert_eq!(
        record.boot,
        Some(remote_path("internal/video/clip.mp4")),
        "the other writer's boot media stays"
    );
}

/// The live screen is changed through its open link (by either of its
/// ports): no other connection. Once a shutdown starts, nothing reaches it
/// (`busy`) and the choice stays (D-2026-10-03-power-off-standby-3).
#[test]
fn the_live_screen_is_changed_through_its_link_until_a_shutdown_starts() {
    let s = Setup::new("live", with_videos());
    let photo = s.wide_photo("beach.png");
    s.f.backend.set_live(true, Some(KEY), TIME).unwrap();
    assert_eq!(s.f.connector.log().connects, 1);
    s.set(&ask("off", Some(2), None), Confirm::Yes).unwrap();
    let dto = s.overview(MCU).unwrap();
    assert_eq!((dto.choice, dto.sleep_minutes), ("off", Some(2)));
    assert_eq!(
        s.f.connector.log().connects,
        1,
        "through the live link, never opened again"
    );
    assert_eq!(
        s.writes(),
        [StorageCall::Options(PlanB::new(StartMode::Default, 2))]
    );

    s.f.backend.enter_final_state();
    let writes = s.writes();
    let refused = [
        s.overview(KEY).map(|_| ()),
        s.set(&ask("album", None, None), Confirm::Yes).map(|_| ()),
        s.add(&photo, "cover", "beach.png", Confirm::Yes)
            .map(|_| ()),
    ];
    for result in refused {
        assert_eq!(result.unwrap_err().code(), "busy");
    }
    assert_eq!(s.writes(), writes, "nothing reached the screen");
    assert_eq!(s.f.connector.log().connects, 1);
    assert_eq!(s.recorded(), Standby::Off(SleepMinutes::new(2).unwrap()));
}

/// D-2026-10-03-power-off-standby-4 (3): a photo is framed in the shape the
/// screen stands in, horizontal or vertical (the orientation last used with
/// it), previewed as the user will see it, stored as a PNG of the panel's
/// size and recorded with its local copy; the album lists it.
#[test]
fn album_photos_are_framed_for_the_screen_lying_or_standing() {
    let s = Setup::new("album", FakeStorage::default().with_card(CARD));
    let photo = s.wide_photo("Beach Day.png");
    let model = eight_eight();

    // Lying (4:1): Fill keeps the photo's width and cuts its top and bottom.
    s.stand(Orientation::Landscape);
    let preview = previewed(&s.f.backend.album_preview(KEY, &photo, "cover").unwrap());
    assert_eq!(preview.size(), Size::new(1920, 480));
    assert_eq!(
        (at(&preview, 100, 240), at(&preview, 1800, 240)),
        (RED, BLUE)
    );
    let added = s.add(&photo, "cover", "beach.png", Confirm::Yes).unwrap();
    assert_eq!(added.path, "sd/image/beach.png");
    let stored = s.f.storage().files[&remote_path("sd/image/beach.png")].clone();
    assert_eq!(added.bytes, stored.len() as u64);
    let native = picture(&stored);
    assert_eq!(native.size(), Size::new(480, 1920), "the panel's own size");
    let seen = as_seen(&native, model, Orientation::Landscape);
    assert_eq!(seen, preview, "the screen shows what the preview showed");
    assert_eq!((at(&seen, 100, 240), at(&seen, 1800, 240)), (RED, BLUE));

    // Standing (1:4): Fit shows it whole, black above and below.
    s.stand(Orientation::Portrait);
    let preview = previewed(&s.f.backend.album_preview(KEY, &photo, "contain").unwrap());
    assert_eq!(preview.size(), Size::new(480, 1920));
    let added = s
        .add(&photo, "contain", "beach-tall.png", Confirm::Yes)
        .unwrap();
    let stored = s.f.storage().files[&remote_path("sd/image/beach-tall.png")].clone();
    assert_eq!(added.bytes, stored.len() as u64);
    let seen = as_seen(&picture(&stored), model, Orientation::Portrait);
    assert_eq!(seen, preview);
    assert_eq!(
        [
            at(&seen, 100, 960),
            at(&seen, 380, 960),
            at(&seen, 240, 100),
            at(&seen, 240, 1800)
        ],
        [RED, BLUE, BLACK, BLACK]
    );

    let overview = s.f.backend.storage_overview(KEY, TIME).unwrap();
    let album = overview
        .folders
        .iter()
        .find(|f| (f.medium, f.kind) == ("sd", "image"))
        .unwrap();
    let names: Vec<&str> = album.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["beach-tall.png", "beach.png"],
        "the album lists them"
    );
    let catalog = s.catalog();
    let record = catalog.screen(&ScreenKey::new(MODEL)).unwrap();
    for path in ["sd/image/beach.png", "sd/image/beach-tall.png"] {
        let entry = record.entry(&remote_path(path), Some(CARD)).unwrap();
        assert_eq!(entry.state, EntryState::Stored, "{path}");
        assert!(catalog.has_copy(&entry.content), "{path}: its local copy");
    }
}

/// D-2026-10-03-power-off-standby-4 (2): the album lists `sd/image` through
/// the storage manager's overview, whose listing the thumbnails answer for:
/// a photo Bezel sent shows its local copy, one the vendor's app put there
/// shows by its name. The storage tab's listing alone gives no thumbnail.
#[test]
fn the_album_lists_through_the_manager_overview_with_thumbnails() {
    const VENDOR: &str = "sd/image/img_0042.jpg";
    let storage = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote_path(VENDOR), vec![7; 90]);
    let s = Setup::new("album-thumbs", storage);
    let photo = s.wide_photo("beach.png");
    let added = s.add(&photo, "cover", "beach.png", Confirm::Yes).unwrap();
    let backend = &s.f.backend;

    backend.storage_overview(KEY, TIME).unwrap();
    assert_eq!(backend.manager_thumbnail(KEY, &added.path), None);

    let overview = backend.manager_overview(KEY, TIME).unwrap();
    let album: Vec<&str> = overview
        .files
        .iter()
        .filter(|f| (f.file.medium, f.file.kind) == ("sd", "image"))
        .map(|f| f.file.path.as_str())
        .collect();
    assert_eq!(album, [added.path.as_str(), VENDOR]);
    let url = backend.manager_thumbnail(KEY, &added.path).unwrap();
    assert!(url.starts_with("data:image/png;base64,"), "{url}");
    assert_eq!(backend.manager_thumbnail(KEY, VENDOR), None, "by its name");
    assert_eq!(backend.manager_thumbnail("COM9", &added.path), None);
}

/// D-2026-10-03-power-off-standby-4 (2): removing a photo is the confirmed
/// delete; without the confirmation the photo stays.
#[test]
fn removing_a_photo_from_the_album_asks_first() {
    const OLD: &str = "sd/image/old.png";
    let storage = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote_path(OLD), vec![5; 80]);
    let s = Setup::new("remove", storage);
    let err =
        s.f.backend
            .delete_stored(KEY, OLD, Confirm::No, TIME)
            .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert!(s.writes().is_empty());
    assert!(s.f.storage().files.contains_key(&remote_path(OLD)));
    s.f.backend
        .delete_stored(KEY, OLD, Confirm::Yes, TIME)
        .unwrap();
    assert_eq!(s.writes(), [StorageCall::Delete(remote_path(OLD))]);
    let overview = s.f.backend.storage_overview(KEY, TIME).unwrap();
    let album = overview
        .folders
        .iter()
        .find(|f| (f.medium, f.kind) == ("sd", "image"))
        .unwrap();
    assert!(album.files.is_empty());
}

/// D-2026-10-03-power-off-standby-4 (3), review B2 of iteration 1: this
/// side decides whether a photo replaces another, from the card's listing,
/// whatever the window knew. A name the album has is refused without the
/// confirmation of replacing it, the dialog's own confirmation given
/// (`notConfirmed`, after queries only: the photo on the card and the
/// catalog stay); with it, the photo replaces its namesake. A screen
/// without a card offers no album and gets nothing.
#[test]
fn a_photo_replaces_its_namesake_only_when_asked_and_nothing_goes_without_a_card() {
    const BEACH: &str = "sd/image/beach.png";
    let storage = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote_path(BEACH), vec![9; 50]);
    let s = Setup::new("replace", storage);
    let photo = s.wide_photo("beach.png");
    let before = s.catalog();
    let err = s
        .add(&photo, "cover", "beach.png", Confirm::Yes)
        .unwrap_err();
    assert_eq!(
        (err.code(), err.value("detail")),
        ("notConfirmed", Some("replacing sd/image/beach.png"))
    );
    assert!(s.writes().is_empty(), "nothing sent: {:?}", s.writes());
    assert_eq!(s.f.storage().files[&remote_path(BEACH)], vec![9; 50]);
    assert_eq!(s.catalog(), before, "nothing recorded");

    s.replace(&photo, "beach.png").unwrap();
    let stored = &s.f.storage().files[&remote_path(BEACH)];
    assert_eq!(picture(stored).size(), Size::new(480, 1920), "replaced");
    assert!(
        s.writes()
            .iter()
            .any(|c| matches!(c, StorageCall::Upload(p, _) if p == &remote_path(BEACH)))
    );

    let s = Setup::new("no-card", with_videos_without_card());
    let photo = s.wide_photo("beach.png");
    let dto = s.overview(KEY).unwrap();
    assert_eq!(
        (dto.card, codes(&dto.options)[3]),
        (false, ("album", false, Some("noCard")))
    );
    let err = s
        .add(&photo, "cover", "beach.png", Confirm::Yes)
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert_eq!(err.value("detail"), Some(NO_CARD));
    let err = s.set(&ask("album", None, None), Confirm::Yes).unwrap_err();
    assert_eq!(
        (err.code(), err.value("detail")),
        ("unsupported", Some(NO_CARD))
    );
    assert!(s.writes().is_empty(), "nothing sent: {:?}", s.writes());
    assert_eq!(s.recorded(), Standby::Keep);
}

fn with_videos_without_card() -> FakeStorage {
    FakeStorage::default().with_file(remote_path("internal/video/clip.mp4"), vec![1; 64])
}

/// A screen asleep is never woken and one of another family never opened:
/// their options say why, a change is `unsupported`, and the recorded
/// choice still shows. The preview needs no screen.
#[test]
fn screens_asleep_or_of_another_family_are_never_opened() {
    let all = FakeBus::turing_88().endpoints().unwrap();
    let asleep = FakeBus::new(all.into_iter().filter(|e| e.address.0 == MCU).collect());
    let s = Setup::on("asleep", asleep, with_videos());
    s.cli_records(|r| r.standby = Standby::Off(SleepMinutes::new(4).unwrap()));
    let dto = s.overview(MCU).unwrap();
    assert_eq!((dto.choice, dto.sleep_minutes), ("off", Some(4)));
    assert!(
        dto.options
            .iter()
            .all(|o| (o.enabled, o.reason) == (false, Some("notConnected")))
    );
    assert_eq!((dto.videos.len(), dto.card), (0, false));
    let photo = s.wide_photo("beach.png");
    let err =
        s.f.backend
            .set_standby(MCU, &ask("off", Some(1), None), Confirm::Yes, TIME)
            .unwrap_err();
    assert_eq!(
        (err.code(), err.value("detail")),
        ("unsupported", Some(ASLEEP))
    );
    let asked = asked_photo(&photo, "cover", "beach.png", Confirm::Yes);
    let err =
        s.f.backend
            .album_add(MCU, &asked, Confirm::Yes, TIME)
            .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    let preview = previewed(&s.f.backend.album_preview(MCU, &photo, "cover").unwrap());
    assert_eq!(
        preview.size(),
        Size::new(1920, 480),
        "the 8.8\"'s own shape"
    );
    let log = s.f.connector.log();
    assert_eq!((log.connects, log.storage.calls.len()), (0, 0));

    const USB: &str = "usb:3-4";
    let usb = FakeBus::new(vec![Endpoint {
        address: DeviceAddress(USB.into()),
        transport: Transport::UsbBulk,
        usb: UsbId::new(0x1cbe, 0x0088),
        serial_number: None,
        manufacturer: None,
        product: None,
        location: None,
    }]);
    let s = Setup::on("other-family", usb, FakeStorage::default());
    let dto = s.overview(USB).unwrap();
    assert_eq!(dto.choice, "keep");
    assert!(
        dto.options
            .iter()
            .all(|o| (o.enabled, o.reason) == (false, Some("unsupported")))
    );
    let err =
        s.f.backend
            .set_standby(USB, &ask("off", Some(1), None), Confirm::Yes, TIME)
            .unwrap_err();
    assert_eq!(
        (err.code(), err.value("detail")),
        ("unsupported", Some(OTHER_FAMILY))
    );
    assert_eq!(s.f.connector.log().connects, 0);
    let err = s.overview("/dev/ttyACM9").unwrap_err();
    assert_eq!(err.code(), "screenNotFound");
}

/// What the window sends is checked before anything reaches the screen.
#[test]
fn what_the_window_sends_is_checked_first() {
    let s = Setup::new("checked", with_videos());
    let photo = s.wide_photo("beach.png");
    let text = s.f.local("notes.txt", 40);
    let wrong = [
        ask("sideways", None, None),
        ask("off", None, None),
        ask("off", Some(11), None),
        ask("off", Some(300), None),
        ask("keep", Some(5), None),
        ask("album", None, Some("sd/video/rain.mp4")),
        ask("video", None, Some("internal/image/logo.png")),
        ask("video", None, Some("sd/video/missing.mp4")),
    ];
    for asked in &wrong {
        let err = s.set(asked, Confirm::Yes).unwrap_err();
        assert_eq!(err.code(), "invalidInput", "{asked:?}: {err}");
    }
    let preview = |source: &Path, fit: &str| {
        s.f.backend
            .album_preview(KEY, source, fit)
            .unwrap_err()
            .code()
    };
    assert_eq!(preview(&photo, "stretch"), "invalidInput");
    assert_eq!(preview(&text, "cover"), "invalidInput");
    assert_eq!(preview(&s.f.root.join("gone.png"), "cover"), "fileError");
    for name in ["beach.jpg", ".png", "my beach.png", ""] {
        let err = s.add(&photo, "cover", name, Confirm::Yes).unwrap_err();
        assert_eq!(err.code(), "invalidInput", "{name:?}");
    }
    assert!(s.writes().is_empty(), "{:?}", s.writes());
    assert_eq!(s.recorded(), Standby::Keep);
    assert_eq!(PHOTO_EXTENSIONS, ["jpg", "jpeg", "png", "bmp", "gif"]);
}
