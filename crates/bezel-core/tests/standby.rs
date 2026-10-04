//! What a screen does when the computer shuts down
//! (D-2026-10-03-power-off-standby-2, -3), through the adapters' fakes: a
//! Turing 8.8" (rev C) with in-memory storage, a Turing USB 8.8" and a
//! WeAct 0.96" (families without the choice), and the catalog in memory.
//! The shutdown actions run on the choice as the catalog recorded it.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use bezel_core::app::manager::Manager;
use bezel_core::app::open_screen;
use bezel_core::app::standby::{self, Applied};
use bezel_core::domain::archive::{Catalog, ContentId, ScreenKey};
use bezel_core::domain::device::{ModelId, Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::standby::{Choice, PlanB, SleepMinutes, Standby, StoredPlanB, Unavailable};
use bezel_core::domain::storage::{BootMedia, Refusal, RemotePath, Repeat, StartMode};
use bezel_core::ports::{ArchiveStore, ScreenLink};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FakeStorage, Kept, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_media::archive::MemoryArchive;

/// The user's card, 29.7 GiB.
const CARD: u64 = 31_890_132_172;

fn remote(text: &str) -> RemotePath {
    RemotePath::parse(text).expect("path")
}

fn minutes(n: u8) -> SleepMinutes {
    SleepMinutes::new(n).expect("1 to 10")
}

fn key() -> ScreenKey {
    ScreenKey::named(ModelId("turing-8.8"), "desk")
}

/// The fake 8.8" (rev C).
fn open(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    open_screen(&FakeBus::turing_88(), connector, None).expect("opens the fake 8.8\"")
}

/// A fake screen on one USB endpoint of `usb`.
fn open_at(connector: &FakeConnector, usb: UsbId, transport: Transport) -> Box<dyn ScreenLink> {
    let bus = FakeBus::new(vec![Endpoint {
        address: DeviceAddress("usb:3-1".into()),
        transport,
        usb,
        serial_number: Some("AD0001".into()),
        manufacturer: None,
        product: None,
        location: None,
    }]);
    open_screen(&bus, connector, None).expect("opens the fake screen")
}

/// The fake Turing USB 8.8": storage, but not the choice.
fn open_usb(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    open_at(connector, UsbId::new(0x1cbe, 0x0088), Transport::UsbBulk)
}

/// The fake WeAct 0.96": no storage at all.
fn open_weact(connector: &FakeConnector) -> Box<dyn ScreenLink> {
    open_at(connector, UsbId::new(0x1a86, 0xfe0c), Transport::Serial)
}

fn unsupported<T>(result: &Result<T>) -> bool {
    matches!(result, Err(BezelError::Unsupported(_)))
}

fn calls(connector: &FakeConnector) -> Vec<StorageCall> {
    connector.log().storage.calls
}

/// The calls since the first `from`.
fn since(connector: &FakeConnector, from: usize) -> Vec<StorageCall> {
    calls(connector)[from..].to_vec()
}

/// The catalog in memory, counting loads and saves; with `full`, every
/// save fails (a disk that is full) and the catalog stays as it was.
#[derive(Default)]
struct Counted {
    inner: MemoryArchive,
    loads: usize,
    saves: usize,
    full: bool,
}

impl Counted {
    fn with_catalog(catalog: Catalog) -> Self {
        Self {
            inner: MemoryArchive::with_catalog(catalog),
            ..Self::default()
        }
    }

    /// The choice recorded for [`key`].
    fn recorded(&self) -> Standby {
        let saved = self.inner.saved().cloned().unwrap_or_default();
        saved
            .screen(&key())
            .map(|r| r.standby.clone())
            .unwrap_or_default()
    }
}

impl ArchiveStore for Counted {
    fn load(&mut self) -> Result<Catalog> {
        self.loads += 1;
        self.inner.load()
    }

    fn save(&mut self, catalog: &Catalog) -> Result<()> {
        self.saves += 1;
        if self.full {
            return Err(BezelError::Transport("no space left on the disk".into()));
        }
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

/// A screen with a card, a video on each medium and a photo in the album.
fn stocked() -> FakeStorage {
    FakeStorage::default()
        .with_card(CARD)
        .with_file(remote("internal/video/intro.mp4"), vec![1; 10])
        .with_file(remote("sd/video/loop.mp4"), vec![2; 20])
        .with_file(remote("sd/image/beach.png"), vec![3; 30])
}

/// A catalog that records `standby` for [`key`].
fn on_record(standby: Standby) -> Counted {
    let mut catalog = Catalog::default();
    catalog.screen_mut(&key()).standby = standby;
    Counted::with_catalog(catalog)
}

/// The shutdown of the screen behind `link`: the choice the catalog of
/// `store` records for [`key`], read there and applied.
fn at_shutdown(link: &mut dyn ScreenLink, store: &mut Counted) -> Result<Applied> {
    standby::at_shutdown(link, store, &key())
}

fn choose(
    link: &mut dyn ScreenLink,
    store: &mut Counted,
    standby: Standby,
    confirm: Confirm,
) -> Result<PlanB> {
    standby::choose(link, store, &key(), standby, None, confirm)
}

#[test]
fn without_yes_or_on_another_family_nothing_is_sent_nor_recorded() {
    let mut catalog = Catalog::default();
    catalog.screen_mut(&key()).standby = Standby::Off(minutes(5));
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::with_catalog(catalog.clone());
    let video = remote("sd/video/loop.mp4");
    for choice in [
        Standby::Keep,
        Standby::Off(minutes(3)),
        Standby::Video(video.clone()),
        Standby::Album,
    ] {
        let refused = choose(link.as_mut(), &mut store, choice, Confirm::No);
        assert!(
            matches!(refused, Err(BezelError::NotConfirmed(ref what)) if what.contains("shuts down")),
            "{refused:?}"
        );
    }
    assert!(calls(&connector).is_empty(), "{:?}", calls(&connector));
    assert_eq!((store.loads, store.saves), (0, 0), "not even read");
    assert_eq!(store.inner.saved(), Some(&catalog), "the catalog as it was");

    // Rev C only: a Turing USB screen (with storage) and a WeAct (without)
    // are refused before anything is asked of them, at shutdown too.
    for mut other in [open_usb(&connector), open_weact(&connector)] {
        for choice in [Standby::Keep, Standby::Album, Standby::Video(video.clone())] {
            let refused = choose(other.as_mut(), &mut store, choice.clone(), Confirm::Yes);
            assert!(unsupported(&refused), "{refused:?}");
            let why = refused.map(|_| ()).unwrap_err().to_string();
            assert!(why.contains("rev C screens only"), "{why}");
            let refused = at_shutdown(other.as_mut(), &mut on_record(choice.clone()));
            if choice == Standby::Keep {
                assert_eq!(refused, Ok(Applied::Nothing));
            } else {
                assert!(unsupported(&refused), "{refused:?}");
            }
        }
        let refused = at_shutdown(other.as_mut(), &mut on_record(Standby::Off(minutes(1))));
        assert!(unsupported(&refused), "{refused:?}");
        let shown = standby::show(other.as_mut(), &mut store, &key()).expect("shows");
        let reasons = shown.options.map(|o| o.unavailable);
        assert_eq!(reasons, [Some(Unavailable::Unsupported); 4]);
    }
    assert!(calls(&connector).is_empty(), "{:?}", calls(&connector));
    assert_eq!(store.saves, 0);
    assert_eq!(store.inner.saved(), Some(&catalog));
    assert_eq!(connector.log().offs, 0);
}

#[test]
fn off_album_and_video_write_their_plan_b_and_record_the_choice() {
    // The boot media Bezel set is a video: off keeps its start mode.
    let mut catalog = Catalog::default();
    catalog
        .screen_mut(&key())
        .set_boot(&BootMedia::File(remote("internal/video/intro.mp4")));
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::with_catalog(catalog);
    let video = remote("sd/video/loop.mp4");
    let cases = [
        (
            Standby::Off(minutes(5)),
            PlanB::new(StartMode::Video, 5),
            vec![],
        ),
        (
            Standby::Album,
            PlanB::new(StartMode::Image, 0),
            vec![StorageCall::Info],
        ),
        (
            Standby::Video(video.clone()),
            PlanB::new(StartMode::Video, 0),
            vec![StorageCall::Size(video.clone())],
        ),
    ];
    for (choice, plan, queries) in cases {
        let before = calls(&connector).len();
        let written = choose(link.as_mut(), &mut store, choice.clone(), Confirm::Yes);
        assert_eq!(written, Ok(plan), "{choice:?}");
        let mut expected = queries;
        expected.push(StorageCall::Options(plan));
        assert_eq!(since(&connector, before), expected, "{choice:?}");
        assert_eq!(store.recorded(), choice);
        assert_eq!(connector.log().storage.options, Some(plan));
    }
    // Another screen's record and the boot media are left as they were.
    let saved = store.inner.saved().cloned().expect("saved");
    assert_eq!(saved.screens.len(), 1);
    let record = saved.screen(&key()).expect("record");
    assert_eq!(record.boot, Some(remote("internal/video/intro.mp4")));
    assert_eq!(connector.log().brightness, [], "the link's level stays");
    let shown = standby::show(link.as_mut(), &mut store, &key()).expect("shows");
    assert_eq!(shown.standby.choice(), Choice::Video);
    assert_eq!(shown.plan_b.plan, PlanB::new(StartMode::Video, 0));
}

#[test]
fn keep_undoes_the_plan_b_and_keep_to_keep_sends_nothing() {
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::default();

    // Nothing recorded is keep: keeping it sends and saves nothing.
    let kept = choose(link.as_mut(), &mut store, Standby::Keep, Confirm::Yes);
    assert_eq!(kept, Ok(PlanB::new(StartMode::Default, 0)));
    assert!(calls(&connector).is_empty());
    assert_eq!(store.saves, 0);

    choose(
        link.as_mut(),
        &mut store,
        Standby::Off(minutes(2)),
        Confirm::Yes,
    )
    .expect("off");
    let undone = choose(link.as_mut(), &mut store, Standby::Keep, Confirm::Yes);
    assert_eq!(undone, Ok(PlanB::new(StartMode::Default, 0)));
    assert_eq!(
        calls(&connector),
        [
            StorageCall::Options(PlanB::new(StartMode::Default, 2)),
            StorageCall::Options(PlanB::new(StartMode::Default, 0)),
        ]
    );
    assert_eq!(store.recorded(), Standby::Keep);
    let saves = store.saves;
    choose(link.as_mut(), &mut store, Standby::Keep, Confirm::Yes).expect("keep");
    assert_eq!(calls(&connector).len(), 2, "keep to keep: nothing");
    assert_eq!(store.saves, saves);
    assert_eq!(at_shutdown(link.as_mut(), &mut store), Ok(Applied::Nothing));
    assert_eq!(calls(&connector).len(), 2, "keep at shutdown: nothing");
}

#[test]
fn the_boot_media_and_the_sleep_timer_keep_each_other() {
    // D-2026-10-03-power-off-standby-2 (4): every OPTIONS is written whole
    // from the record; the last explicit action wins and neither rewrites
    // the other's record.
    let boot = remote("internal/video/intro.mp4");
    let logo = remote("internal/image/logo.png");
    let connector = FakeConnector::with_storage(stocked().with_file(logo.clone(), vec![4; 40]));
    let mut link = open(&connector);
    let mut store = Counted::default();
    let options = |connector: &FakeConnector| connector.log().storage.options;

    choose(
        link.as_mut(),
        &mut store,
        Standby::Off(minutes(5)),
        Confirm::Yes,
    )
    .expect("off");
    assert_eq!(options(&connector), Some(PlanB::new(StartMode::Default, 5)));

    // Setting the boot media keeps the timer of off.
    let before = calls(&connector).len();
    Manager::new(link.as_mut(), &mut store)
        .named("desk")
        .set_boot_media(&BootMedia::File(boot.clone()), None, Confirm::Yes)
        .expect("boot media");
    let written: Vec<StorageCall> = since(&connector, before)
        .into_iter()
        .filter(StorageCall::changes_the_screen)
        .collect();
    assert_eq!(
        written,
        [
            StorageCall::PlayVideo(boot.clone(), Repeat::Loop),
            StorageCall::Options(PlanB::new(StartMode::Video, 5)),
        ]
    );
    assert_eq!(
        store.recorded(),
        Standby::Off(minutes(5)),
        "the choice stays"
    );

    // Keep undoes the timer and keeps the boot media's start mode.
    choose(link.as_mut(), &mut store, Standby::Keep, Confirm::Yes).expect("keep");
    assert_eq!(options(&connector), Some(PlanB::new(StartMode::Video, 0)));

    // The album sets its own start mode; a boot media set afterwards wins
    // on the screen and leaves the album recorded.
    choose(link.as_mut(), &mut store, Standby::Album, Confirm::Yes).expect("album");
    assert_eq!(options(&connector), Some(PlanB::new(StartMode::Image, 0)));
    Manager::new(link.as_mut(), &mut store)
        .named("desk")
        .set_boot_media(&BootMedia::Default, None, Confirm::Yes)
        .expect("default boot");
    assert_eq!(options(&connector), Some(PlanB::new(StartMode::Default, 0)));
    assert_eq!(store.recorded(), Standby::Album);
    let record = store.inner.saved().and_then(|c| c.screen(&key()).cloned());
    assert_eq!(record.and_then(|r| r.boot), None);

    // Off then keeps the boot media's start mode again.
    Manager::new(link.as_mut(), &mut store)
        .named("desk")
        .set_boot_media(&BootMedia::File(logo), None, Confirm::Yes)
        .expect("boot image");
    choose(
        link.as_mut(),
        &mut store,
        Standby::Off(minutes(1)),
        Confirm::Yes,
    )
    .expect("off");
    assert_eq!(options(&connector), Some(PlanB::new(StartMode::Image, 1)));
}

/// Review W7 and W4 (iteration 1): the record says which plan B Bezel
/// stored last, the choice's or the boot media's set after it, and `show`
/// says that one (D-2026-10-03-power-off-standby-2 (4)). A level chosen with
/// the plan B (`--brightness`) is set before its OPTIONS, so the OPTIONS
/// carries it, is recorded with it, and the album's restart at shutdown
/// writes it again: the screen starts with it after that restart too.
#[test]
fn the_plan_b_stored_last_shows_and_its_level_holds_at_shutdown() {
    let album = PlanB::new(StartMode::Image, 0);
    let forty = Brightness::new(40).expect("level");
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::default();
    standby::choose(
        link.as_mut(),
        &mut store,
        &key(),
        Standby::Album,
        Some(forty),
        Confirm::Yes,
    )
    .expect("album");
    assert_eq!(
        connector.log().kept,
        [Kept::Brightness(forty), Kept::Options(album, Some(forty))]
    );
    let shown = standby::show(link.as_mut(), &mut store, &key()).expect("shows");
    let stored = StoredPlanB {
        plan: album,
        brightness: Some(forty),
    };
    assert_eq!(shown.plan_b, stored);
    assert_eq!(
        stored.to_string(),
        "start mode 1, sleep timer off, brightness 40%"
    );

    // At shutdown, on a link opened for it (no level set on it yet).
    let restart = |store: &mut Counted| {
        let opened = FakeConnector::with_storage(stocked());
        let mut link = open(&opened);
        let applied = at_shutdown(link.as_mut(), store);
        assert_eq!(applied, Ok(Applied::Album));
        assert_eq!(
            calls(&opened),
            [
                StorageCall::Info,
                StorageCall::Options(album),
                StorageCall::Restart
            ]
        );
        opened.log().kept
    };
    assert_eq!(
        restart(&mut store),
        [Kept::Brightness(forty), Kept::Options(album, Some(forty))]
    );

    // The boot media set afterwards is what the screen keeps, and what
    // `show` says; the choice stays, and its restart takes the link's level.
    Manager::new(link.as_mut(), &mut store)
        .named("desk")
        .set_boot_media(
            &BootMedia::File(remote("internal/video/intro.mp4")),
            None,
            Confirm::Yes,
        )
        .expect("boot media");
    let shown = standby::show(link.as_mut(), &mut store, &key()).expect("shows");
    assert_eq!(shown.standby, Standby::Album);
    assert_eq!(
        shown.plan_b,
        StoredPlanB {
            plan: PlanB::new(StartMode::Video, 0),
            brightness: None
        }
    );
    assert_eq!(restart(&mut store), [Kept::Options(album, None)]);
}

/// Review W5 of iteration 2: the album's restart at shutdown stores start
/// mode 1 on the screen even when a boot media set after the choice stored
/// another plan B; the record then says the album's, so that `show` (the
/// CLI's, the catalog's readers) says what the screen does from then on.
/// When the record already says it, nothing is saved.
#[test]
fn the_album_at_shutdown_records_the_plan_b_it_stores() {
    let album = PlanB::new(StartMode::Image, 0);
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::default();
    choose(link.as_mut(), &mut store, Standby::Album, Confirm::Yes).expect("album");
    Manager::new(link.as_mut(), &mut store)
        .named("desk")
        .set_boot_media(
            &BootMedia::File(remote("internal/video/intro.mp4")),
            None,
            Confirm::Yes,
        )
        .expect("boot media");
    let shown = |store: &mut Counted| {
        let opened = FakeConnector::with_storage(stocked());
        standby::show(open(&opened).as_mut(), store, &key())
            .expect("shows")
            .plan_b
    };
    assert_eq!(shown(&mut store).plan, PlanB::new(StartMode::Video, 0));

    let saves = store.saves;
    let at_shutdown_on = |store: &mut Counted| {
        let opened = FakeConnector::with_storage(stocked());
        let applied = at_shutdown(open(&opened).as_mut(), store);
        assert_eq!(applied, Ok(Applied::Album));
        assert_eq!(opened.log().kept, [Kept::Options(album, None)]);
    };
    at_shutdown_on(&mut store);
    assert_eq!(store.saves, saves + 1, "the album's plan B recorded");
    assert_eq!(
        shown(&mut store),
        StoredPlanB {
            plan: album,
            brightness: None
        }
    );
    let record = store.inner.saved().and_then(|c| c.screen(&key()).cloned());
    let record = record.expect("record");
    assert_eq!(record.standby, Standby::Album, "the choice stays");
    assert_eq!(record.boot, Some(remote("internal/video/intro.mp4")));

    // The record says it now: the next shutdown saves nothing.
    at_shutdown_on(&mut store);
    assert_eq!(store.saves, saves + 1);
}

/// Review W2 of iteration 3: the album's plan B is recorded after the
/// screen restarts into the album, so a catalog that cannot be saved then
/// does not make the action a failure: the screen does restart into its
/// album, and the answer says that the record still says the plan B
/// stored before ([`Applied::AlbumNotRecorded`], with why).
#[test]
fn the_album_at_shutdown_restarts_though_its_record_is_not_saved() {
    let album = PlanB::new(StartMode::Image, 0);
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::default();
    choose(link.as_mut(), &mut store, Standby::Album, Confirm::Yes).expect("album");
    Manager::new(link.as_mut(), &mut store)
        .named("desk")
        .set_boot_media(
            &BootMedia::File(remote("internal/video/intro.mp4")),
            None,
            Confirm::Yes,
        )
        .expect("boot media");
    let before = store.inner.saved().cloned();

    store.full = true;
    let saves = store.saves;
    let opened = FakeConnector::with_storage(stocked());
    let applied = at_shutdown(open(&opened).as_mut(), &mut store);
    let full = BezelError::Transport("no space left on the disk".into());
    assert_eq!(applied, Ok(Applied::AlbumNotRecorded(full)));
    assert_eq!(
        calls(&opened),
        [
            StorageCall::Info,
            StorageCall::Options(album),
            StorageCall::Restart
        ],
        "the screen restarts into the album all the same"
    );
    assert_eq!(store.saves, saves + 1, "the record was tried once");
    assert_eq!(
        store.inner.saved().cloned(),
        before,
        "the catalog as it was"
    );

    // When the record already says the album's plan B, nothing is saved,
    // so nothing can fail.
    store.full = false;
    at_shutdown(open(&opened).as_mut(), &mut store).expect("recorded");
    store.full = true;
    let applied = at_shutdown(open(&opened).as_mut(), &mut store);
    assert_eq!(applied, Ok(Applied::Album));
}

#[test]
fn the_shutdown_actions_follow_the_recorded_choice() {
    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let mut store = Counted::default();
    let video = remote("sd/video/loop.mp4");
    let cases = [
        (Standby::Keep, Applied::Nothing, vec![]),
        (
            Standby::Off(minutes(4)),
            Applied::TurnedOff,
            vec![StorageCall::TurnOffNow],
        ),
        (
            Standby::Video(video.clone()),
            Applied::Video(video.clone()),
            vec![
                StorageCall::Size(video.clone()),
                StorageCall::PlayVideo(video.clone(), Repeat::Loop),
            ],
        ),
        (
            Standby::Album,
            Applied::Album,
            vec![
                StorageCall::Info,
                StorageCall::Options(PlanB::new(StartMode::Image, 0)),
                StorageCall::Restart,
            ],
        ),
    ];
    for (choice, applied, sent) in cases {
        if choice != store.recorded() {
            choose(link.as_mut(), &mut store, choice.clone(), Confirm::Yes).expect("chosen");
        }
        // The shutdown reads the choice back from the catalog.
        assert_eq!(store.recorded(), choice);
        let (before, saves) = (calls(&connector).len(), store.saves);
        assert_eq!(at_shutdown(link.as_mut(), &mut store), Ok(applied));
        assert_eq!(since(&connector, before), sent, "{choice:?}: exactly this");
        assert_eq!(store.saves, saves, "{choice:?}: the catalog is only read");
    }
    let log = connector.log();
    assert_eq!((log.offs, log.releases, log.frames.len()), (0, 0, 0));
}

#[test]
fn a_choice_that_cannot_be_honoured_turns_the_screen_off_instead() {
    // Chosen on a screen with its card and video; at shutdown the card is
    // out and the video gone.
    let video = remote("sd/video/loop.mp4");
    let mut store = Counted::default();
    let stocked_screen = FakeConnector::with_storage(stocked());
    let mut link = open(&stocked_screen);
    let bare = FakeConnector::default();
    let mut bare_link = open(&bare);

    choose(
        link.as_mut(),
        &mut store,
        Standby::Video(video.clone()),
        Confirm::Yes,
    )
    .expect("video");
    let applied = at_shutdown(bare_link.as_mut(), &mut store);
    assert_eq!(applied, Ok(Applied::TurnedOffInstead(Unavailable::NoVideo)));
    assert_eq!(
        calls(&bare),
        [StorageCall::Size(video.clone()), StorageCall::TurnOffNow]
    );

    choose(link.as_mut(), &mut store, Standby::Album, Confirm::Yes).expect("album");
    let before = calls(&bare).len();
    let applied = at_shutdown(bare_link.as_mut(), &mut store);
    assert_eq!(applied, Ok(Applied::TurnedOffInstead(Unavailable::NoCard)));
    assert_eq!(
        since(&bare, before),
        [StorageCall::Info, StorageCall::TurnOffNow]
    );

    // A video choice that points at an image is not played either.
    let image = remote("sd/image/beach.png");
    let before = calls(&stocked_screen).len();
    let applied = at_shutdown(link.as_mut(), &mut on_record(Standby::Video(image)));
    assert_eq!(applied, Ok(Applied::TurnedOffInstead(Unavailable::NoVideo)));
    assert_eq!(since(&stocked_screen, before), [StorageCall::TurnOffNow]);
    assert_eq!(bare.log().storage.options, None, "no plan B at shutdown");
}

#[test]
fn choosing_asks_the_screen_whether_the_choice_can_be_honoured() {
    let connector = FakeConnector::default();
    let mut link = open(&connector);
    let mut store = Counted::default();

    let missing = remote("internal/video/none.mp4");
    let refused = choose(
        link.as_mut(),
        &mut store,
        Standby::Video(missing.clone()),
        Confirm::Yes,
    );
    assert!(
        matches!(refused, Err(BezelError::InvalidInput(_))),
        "{refused:?}"
    );
    assert_eq!(calls(&connector), [StorageCall::Size(missing)]);

    let image = remote("internal/image/logo.png");
    let refused = choose(
        link.as_mut(),
        &mut store,
        Standby::Video(image),
        Confirm::Yes,
    );
    assert!(
        matches!(refused, Err(BezelError::InvalidInput(_))),
        "{refused:?}"
    );
    assert_eq!(
        calls(&connector).len(),
        1,
        "an image is not even looked for"
    );

    let refused = choose(link.as_mut(), &mut store, Standby::Album, Confirm::Yes);
    assert_eq!(refused, Err(BezelError::Refused(Refusal::NoCard)));
    assert_eq!(calls(&connector)[1..], [StorageCall::Info]);

    assert!(
        !calls(&connector)
            .iter()
            .any(StorageCall::changes_the_screen)
    );
    assert_eq!(store.saves, 0);
    assert_eq!(store.recorded(), Standby::Keep);
}

#[test]
fn show_reads_the_choice_and_offers_what_the_screen_has() {
    let mut catalog = Catalog::default();
    catalog.screen_mut(&key()).standby = Standby::Off(minutes(4));
    let mut store = Counted::with_catalog(catalog);

    let connector = FakeConnector::with_storage(stocked());
    let mut link = open(&connector);
    let shown = standby::show(link.as_mut(), &mut store, &key()).expect("shows");
    assert_eq!(shown.key, key());
    assert_eq!(shown.standby, Standby::Off(minutes(4)));
    assert_eq!(shown.plan_b.plan, PlanB::new(StartMode::Default, 4));
    assert!(shown.offer.card);
    // Each video with its size (review W9 of iteration 1: the studio lists
    // them so).
    let videos: Vec<(RemotePath, Option<u64>)> = shown
        .offer
        .videos
        .iter()
        .map(|v| (v.path.clone(), v.size))
        .collect();
    assert_eq!(
        videos,
        [
            (remote("internal/video/intro.mp4"), Some(10)),
            (remote("sd/video/loop.mp4"), Some(20))
        ]
    );
    assert_eq!(shown.options.map(|o| o.choice), Choice::ALL);
    assert_eq!(shown.options.map(|o| o.unavailable), [None; 4]);
    assert!(
        !calls(&connector)
            .iter()
            .any(StorageCall::changes_the_screen)
    );

    // Without a card nor a video: their options say why; the card's folder
    // is not listed (listing creates it).
    let bare = FakeConnector::default();
    let mut link = open(&bare);
    let shown = standby::show(link.as_mut(), &mut store, &key()).expect("shows");
    assert_eq!(
        shown.options.map(|o| o.unavailable),
        [
            None,
            None,
            Some(Unavailable::NoVideo),
            Some(Unavailable::NoCard)
        ]
    );
    assert_eq!(
        calls(&bare),
        [
            StorageCall::Info,
            StorageCall::List(remote("internal/video/x").location)
        ]
    );
    assert_eq!(store.saves, 0, "showing saves nothing");
}
