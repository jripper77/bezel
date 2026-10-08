//! What the studio does when the computer shuts down
//! (D-2026-10-03-power-off-standby-3; DoD rows 2 and 3).
//!
//! `linux_*`: the app's real `setup`, run by Tauri's mock runtime, its fake
//! 8.8" reached through a connector that notes every call ([`Recording`]),
//! its files in a temporary folder, and logind a fake one on a private
//! `dbus-daemon` ([`LogindBus`]): never the machine's system bus. They fail,
//! never skip, without `dbus-daemon` (package `dbus`). The others run on
//! every system, Windows included, without the mock runtime, on the
//! backend the composition root makes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use bezel_core::app::discover_screens;
use bezel_core::domain::archive::ScreenKey;
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::device::ModelId;
use bezel_core::domain::discovery::Screen;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::Job;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::domain::standby::{PlanB, SleepMinutes, Standby, StoredPlanB};
use bezel_core::domain::storage::{
    Confirmed, FileName, RemotePath, Repeat, StartMode, StorageInfo, StorageLocation,
};
use bezel_core::ports::{ArchiveStore, DeviceBus as _, ScreenConnector, ScreenLink, ScreenStorage};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FakeStorage, StorageCall};
use bezel_devices::{FakeBus, FakeConnector, FakeHid};
use bezel_media::archive::{DiskArchive, storage_dir};
use bezel_sensors::FakeSensors;

use super::*;
use crate::settings::SettingsFile;
use crate::tests::temp_root;
use crate::{Adapters, Folders, compose};

#[cfg(target_os = "linux")]
use bezel_core::domain::device::{Transport, UsbId};
#[cfg(target_os = "linux")]
use bezel_core::domain::discovery::{DeviceAddress, Endpoint, UsbLocation};
#[cfg(target_os = "linux")]
use bezel_core::ports::GifSource;
#[cfg(target_os = "linux")]
use bezel_media::collection::FakeGifSource;
#[cfg(target_os = "linux")]
use bezel_power::fake::{BusMessage, BusMonitor, FakeLogind, LogindCall, MessageKind, PrivateBus};
#[cfg(target_os = "linux")]
use tauri::test::{MockRuntime, mock_builder};
#[cfg(target_os = "linux")]
use tauri::{AppHandle, Manager as _, RunEvent};

#[cfg(target_os = "linux")]
use crate::gifs::{KlipyKey, SourceFactory, UserAsked};
#[cfg(target_os = "linux")]
use crate::tests::context_with_the_window;
#[cfg(target_os = "linux")]
use crate::texts::Texts;
#[cfg(target_os = "linux")]
use crate::{LiveSync, MAIN_WINDOW, Start, setup};

const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 10,
    day: 3,
    hour: 23,
    minute: 59,
    second: 0,
    weekday: 5,
};

/// The fake 8.8"'s display (its SoC's port): the key it goes live by.
pub(crate) const DISPLAY: &str = "/dev/ttyACM1";

/// The fake 8.8"'s MCU, the one port of the screen when it sleeps.
const MCU: &str = "/dev/ttyACM0";

/// The 8.8"'s model: the studio knows its screens by model.
const MODEL: ModelId = ModelId("turing-8.8");

/// How long a test waits for what must happen.
const PATIENCE: Duration = Duration::from_secs(10);

/// logind's delay where the test does not wait for it: long, so that only
/// a screen that hangs meets the deadline.
#[cfg(target_os = "linux")]
const LONG_DELAY: Duration = Duration::from_secs(30);

/// `off` as the CLI records it (5 minutes of plan B).
pub(crate) fn off() -> Standby {
    Standby::Off(SleepMinutes::SUGGESTED)
}

/// Records `standby` for the 8.8" in the catalog under `data`, as another
/// writer (the CLI) does while the studio runs.
fn record(data: &Path, standby: Standby) {
    record_for(data, MODEL, standby);
}

/// Records `standby` for the screens of `model` in the catalog under
/// `data`.
fn record_for(data: &Path, model: ModelId, standby: Standby) {
    let mut archive = DiskArchive::open(storage_dir(data)).unwrap();
    let mut catalog = archive.load().unwrap();
    catalog.screen_mut(&ScreenKey::new(model)).standby = standby;
    archive.save(&catalog).unwrap();
}

/// The folders of the test `name` (its root, removed when dropped), the
/// 8.8"'s choice `standby` recorded and its display remembered as the live
/// screen (`liveScreen`), as the app leaves them.
pub(crate) fn folders_for(name: &str, standby: Standby) -> (Root, Folders) {
    let root = Root(temp_root(name));
    let folders = Folders::under(&root.0);
    record(&folders.data, standby);
    settings(&folders.config).update(|s| s.live_screen = Some(DISPLAY.into()));
    (root, folders)
}

/// A test's temporary folder, removed when dropped.
pub(crate) struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The app's settings in `config`.
fn settings(config: &Path) -> SettingsFile {
    SettingsFile::new(config.join("settings.json"))
}

/// The machine of the tests: `bus`'s screens, reached through `connector`,
/// and scripted sensors.
pub(crate) fn adapters_over(
    bus: FakeBus,
    connector: impl ScreenConnector + Send + Sync + 'static,
) -> Adapters {
    Adapters {
        bus: Arc::new(bus),
        connector: Arc::new(connector),
        hid: Arc::new(FakeHid::answering(0x88)),
        sensors: Arc::new(|_| Box::new(FakeSensors::demo())),
    }
}

/// The backend the composition root makes on `folders`, over a fake 8.8"
/// (`bus`) reached through `connector`, its catalog on disk.
fn backend_on(
    folders: &Folders,
    bus: FakeBus,
    connector: impl ScreenConnector + Send + Sync + 'static,
) -> Shared {
    Arc::new(compose(folders, adapters_over(bus, connector), false))
}

/// A 5" rev C screen's display (its SoC's port): the second screen of the
/// tests that need one awake but not live.
#[cfg(target_os = "linux")]
const FIVE_DISPLAY: &str = "/dev/ttyACM3";

/// The 5"'s model, whose choice is its own (the studio knows its screens by
/// model).
#[cfg(target_os = "linux")]
const FIVE: ModelId = ModelId("turing-5");

/// The fake 8.8" and an awake 5" rev C (its `USB7INCH` MCU and its SoC
/// behind another hub): a second screen the shutdown may open.
#[cfg(target_os = "linux")]
fn with_a_five() -> FakeBus {
    let at = |port: &str, usb: UsbId, serial: Option<&str>, last: u8| Endpoint {
        address: DeviceAddress(port.into()),
        transport: Transport::Serial,
        usb,
        serial_number: serial.map(str::to_string),
        manufacturer: None,
        product: None,
        location: Some(UsbLocation {
            bus: "3".into(),
            ports: vec![2, last],
        }),
    };
    FakeBus::turing_88().and(FakeBus::new(vec![
        at(
            "/dev/ttyACM2",
            UsbId::new(0x1a86, 0x5722),
            Some("USB7INCH"),
            1,
        ),
        at(FIVE_DISPLAY, UsbId::new(0x1d6b, 0x0106), None, 2),
    ]))
}

/// A link that hangs: the error that makes a live screen be connected
/// again.
fn hung() -> BezelError {
    BezelError::Hung("it stopped reading what was sent".into())
}

/// Waits until `done`, failing the test with `what` after [`PATIENCE`].
#[cfg(target_os = "linux")]
fn wait_until(what: &str, done: impl Fn() -> bool) {
    let until = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < until, "never happened: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

// --------------------------------------------------- what screens heard --

/// Every call that reached the fake screens, by name, in order (never a
/// frame's pixels), and what a test notes among them.
#[derive(Debug, Clone, Default)]
struct Heard(Arc<Mutex<Vec<&'static str>>>);

impl Heard {
    fn note(&self, call: &'static str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }

    fn all(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What was heard after the first `count` calls.
    fn since(&self, count: usize) -> Vec<&'static str> {
        self.all().get(count..).unwrap_or_default().to_vec()
    }

    fn count(&self, call: &str) -> usize {
        self.all().iter().filter(|heard| **heard == call).count()
    }

    #[cfg(target_os = "linux")]
    fn position(&self, call: &str) -> Option<usize> {
        self.all().iter().position(|heard| *heard == call)
    }
}

/// How a screen's `turn_off_now` goes.
#[derive(Clone)]
enum Hold {
    /// At once.
    Never,
    /// Once the test says so, or drops its sender: a screen that hangs.
    Until(Arc<Mutex<mpsc::Receiver<()>>>),
}

impl Hold {
    /// Held until `go` says so (or is dropped).
    fn until() -> (mpsc::Sender<()>, Self) {
        let (go, through) = mpsc::channel();
        (go, Hold::Until(Arc::new(Mutex::new(through))))
    }
}

/// A connector over a [`FakeConnector`] whose screens note every call in
/// [`Heard`] (the connection too) and hold `turn_off_now` as [`Hold`] says.
#[derive(Clone)]
struct Recording {
    inner: FakeConnector,
    heard: Heard,
    hold: Hold,
}

impl Recording {
    fn over(inner: FakeConnector, heard: &Heard, hold: Hold) -> Self {
        Self {
            inner,
            heard: heard.clone(),
            hold,
        }
    }
}

impl ScreenConnector for Recording {
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        self.heard.note("connect");
        let inner = self.inner.connect(screen)?;
        Ok(Box::new(RecordingLink {
            inner,
            heard: self.heard.clone(),
            hold: self.hold.clone(),
        }))
    }

    fn restart(&self, screen: &Screen) -> Result<()> {
        self.heard.note("restart");
        self.inner.restart(screen)
    }
}

/// A [`Recording`] connector's screen.
struct RecordingLink {
    inner: Box<dyn ScreenLink>,
    heard: Heard,
    hold: Hold,
}

impl RecordingLink {
    fn store(&mut self, call: &'static str) -> Result<&mut dyn ScreenStorage> {
        self.heard.note(call);
        self.inner
            .storage()
            .ok_or_else(|| BezelError::Unsupported("no storage".into()))
    }
}

impl ScreenLink for RecordingLink {
    fn identity(&self) -> &ScreenIdentity {
        self.inner.identity()
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.heard.note("set_brightness");
        self.inner.set_brightness(brightness)
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.heard.note("set_orientation");
        self.inner.set_orientation(orientation)
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        self.heard.note("present");
        self.inner.present(frame)
    }

    fn screen_off(&mut self) -> Result<()> {
        self.heard.note("screen_off");
        self.inner.screen_off()
    }

    fn turn_off_now(&mut self) -> Result<()> {
        self.heard.note("turn_off_now");
        if let Hold::Until(through) = &self.hold {
            let _ = through
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .recv();
        }
        self.inner.turn_off_now()
    }

    fn release(&mut self) -> Result<()> {
        self.heard.note("release");
        self.inner.release()
    }

    fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
        if self.inner.storage().is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl ScreenStorage for RecordingLink {
    fn info(&mut self) -> Result<StorageInfo> {
        self.store("storage.info")?.info()
    }

    fn list(&mut self, location: StorageLocation) -> Result<Vec<FileName>> {
        self.store("storage.list")?.list(location)
    }

    fn size(&mut self, path: &RemotePath) -> Result<Option<u64>> {
        self.store("storage.size")?.size(path)
    }

    fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()> {
        self.store("storage.upload")?.upload(path, data, job)
    }

    fn delete(&mut self, path: &RemotePath, confirmed: Confirmed) -> Result<()> {
        self.store("storage.delete")?.delete(path, confirmed)
    }

    fn play_video(&mut self, path: &RemotePath, repeat: Repeat) -> Result<()> {
        self.store("storage.play_video")?.play_video(path, repeat)
    }

    fn play_image(&mut self, path: &RemotePath) -> Result<()> {
        self.store("storage.play_image")?.play_image(path)
    }

    fn stop(&mut self) -> Result<()> {
        self.store("storage.stop")?.stop()
    }

    fn set_options(&mut self, plan: PlanB, confirmed: Confirmed) -> Result<()> {
        self.store("storage.set_options")?
            .set_options(plan, confirmed)
    }

    fn restart(&mut self, confirmed: Confirmed) -> Result<()> {
        self.store("storage.restart")?.restart(confirmed)
    }
}

/// A connector that reaches the screen whose display is at `address`
/// through `other` and every other screen through `main`: what each screen
/// heard, apart.
#[cfg(target_os = "linux")]
#[derive(Clone)]
struct Apart {
    main: Recording,
    address: &'static str,
    other: Recording,
}

#[cfg(target_os = "linux")]
impl Apart {
    fn route(&self, screen: &Screen) -> &Recording {
        if screen.address().is_some_and(|a| a.0 == self.address) {
            &self.other
        } else {
            &self.main
        }
    }
}

#[cfg(target_os = "linux")]
impl ScreenConnector for Apart {
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        self.route(screen).connect(screen)
    }

    fn restart(&self, screen: &Screen) -> Result<()> {
        self.route(screen).restart(screen)
    }
}

// ------------------------------------------------------- every system --

#[test]
fn the_exit_says_whether_the_session_ends() {
    assert_eq!(Exit::of(true), Exit::SessionEnding);
    assert_eq!(Exit::of(false), Exit::Quit);
    // The session is not ending while the tests run, on Windows too.
    assert_eq!(Exit::of(bezel_power::session_ending()), Exit::Quit);
}

/// Without a bus or logind the studio says why, each cause with its own
/// fixed code (D-2026-10-01-gif-sticker-search-12), and each screen keeps
/// its plan B; off Linux there is no logind and nothing is said.
#[test]
fn a_shutdown_not_watched_is_said_with_a_fixed_code() {
    let no_bus = PowerError::NoBus {
        address: "unix:path=/x".into(),
        reason: "no such file".into(),
    };
    let refused = PowerError::Call {
        call: "Inhibit",
        reason: "org.freedesktop.DBus.Error.AccessDenied: no".into(),
    };
    for (error, code) in [
        (no_bus, Some(DiagCode::NoSystemBus)),
        (PowerError::NoLogind, Some(DiagCode::NoLogind)),
        (refused, Some(DiagCode::ShutdownDelayRefused)),
        (PowerError::Disconnected, Some(DiagCode::SystemBusLost)),
        (PowerError::Unsupported, None),
    ] {
        assert_eq!(unwatched(&error), code, "{error:?}");
    }
    for code in [
        DiagCode::NoSystemBus,
        DiagCode::NoLogind,
        DiagCode::ShutdownDelayRefused,
        DiagCode::SystemBusLost,
    ] {
        assert!(
            code.text()
                .ends_with("each screen keeps its plan B at shutdown")
        );
    }
    // Off Linux, connecting is `Unsupported`: nothing to say.
    #[cfg(not(target_os = "linux"))]
    assert_eq!(
        hold(&BusAddress::new("unix:path=/nonexistent")).err(),
        Some(None)
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        hold(&BusAddress::new("unix:path=/nonexistent/bezel-studio/bus")).err(),
        Some(Some(DiagCode::NoSystemBus))
    );
}

/// D-2026-10-03-power-off-standby-3 (2) and -7 (DoD row 3): what
/// `RunEvent::Exit` does ([`at_exit`]), on the backend the composition root
/// makes and without Tauri's mock runtime, so it runs on Windows too.
/// Quitting the app sends nothing; the end of the session applies the
/// choice the catalog records (written by another writer, read again
/// then) through the live screen's open link, and nothing else; then the
/// final state holds.
#[test]
fn a_session_end_applies_the_choice_and_a_quit_does_not() {
    let (_root, folders) = folders_for("power-exit", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake.clone(), &heard, Hold::Never);
    let backend = backend_on(&folders, FakeBus::turing_88(), connector);
    backend.set_live(true, Some(DISPLAY), TIME).unwrap();
    let live = heard.all();
    assert_eq!(heard.count("present"), 1, "{live:?}");

    assert_eq!(at_exit(Some(&backend), Exit::Quit), None);
    let exit = Exit::of(bezel_power::session_ending());
    assert_eq!(
        at_exit(Some(&backend), exit),
        None,
        "a quit, while the tests run"
    );
    assert_eq!(at_exit(None, Exit::SessionEnding), None, "no backend yet");
    assert_eq!(heard.all(), live, "quitting sent something");
    assert!(!backend.studio().shutting_down());

    let asked = Instant::now();
    assert_eq!(
        at_exit(Some(&backend), Exit::SessionEnding),
        Some(Ending::Done)
    );
    assert!(asked.elapsed() < SESSION_END);
    assert_eq!(
        heard.since(live.len()),
        ["turn_off_now"],
        "through the live link"
    );
    assert_eq!(
        fake.log().storage.calls.last(),
        Some(&StorageCall::TurnOffNow)
    );
    assert!(backend.studio().shutting_down(), "the final state holds");
    assert_eq!(
        backend.studio().live_key(),
        Some(DISPLAY),
        "live mode stays"
    );
    backend.tick_and_wait(TIME, Instant::now() + Duration::from_secs(60));
    assert!(backend.set_live(true, Some(DISPLAY), TIME).is_err());
    assert_eq!(heard.since(live.len()), ["turn_off_now"]);
    assert_eq!(
        settings(&folders.config).load().live_screen.as_deref(),
        Some(DISPLAY)
    );
}

/// D-2026-10-03-power-off-standby-3 (2): when Windows ends the session the
/// sequence is waited for, [`SESSION_END`] at most: a screen that hangs
/// does not hold the app's end longer.
#[test]
fn a_session_end_waits_four_seconds_at_most() {
    let (_root, folders) = folders_for("power-exit-hung", off());
    let heard = Heard::default();
    let (go, hold) = Hold::until();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let backend = backend_on(
        &folders,
        FakeBus::turing_88(),
        Recording::over(fake, &heard, hold),
    );
    backend.set_live(true, Some(DISPLAY), TIME).unwrap();
    let asked = Instant::now();
    assert_eq!(
        at_exit(Some(&backend), Exit::SessionEnding),
        Some(Ending::Deadline)
    );
    let took = asked.elapsed();
    assert!(
        took >= SESSION_END && took < SESSION_END + Duration::from_secs(1),
        "{took:?}"
    );
    assert_eq!(heard.count("turn_off_now"), 1);
    drop(go);
}

/// Review W1 of iteration 3: the shutdown holds the catalog only while it
/// reads or saves it, never while a screen carries out its choice. A
/// screen that hangs in its action (a write stalls until it gives up),
/// past the deadline and after the shutdown is cancelled, holds no command
/// that reads the catalog.
#[test]
fn a_hung_screen_holds_no_command_on_the_catalog() {
    let (_root, folders) = folders_for("power-hung-catalog", off());
    let heard = Heard::default();
    let (go, hold) = Hold::until();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let backend = backend_on(
        &folders,
        FakeBus::turing_88(),
        Recording::over(fake, &heard, hold),
    );
    backend.set_live(true, Some(DISPLAY), TIME).unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    assert_eq!(shut_down(&backend, deadline), Ending::Deadline);
    assert_eq!(heard.count("turn_off_now"), 1, "the screen hangs in it");
    // logind cancelled the shutdown: the commands are taken again.
    backend.leave_final_state();

    let (said, answer) = mpsc::channel();
    let reader = Arc::clone(&backend);
    std::thread::spawn(move || {
        let _ = said.send(reader.cache_info().is_ok());
    });
    let read = answer.recv_timeout(Duration::from_secs(2));
    drop(go);
    assert_eq!(read, Ok(true), "the catalog waited for the hung screen");
}

/// Review W2 of iteration 3: what the studio says of each screen's action
/// at shutdown is what happened. An album the screen restarted into whose
/// record could not be saved is said with its own fixed code, never as a
/// failed choice; a choice applied says nothing; a failure is a failure.
#[test]
fn an_album_not_recorded_is_not_said_as_a_failed_choice() {
    let full = BezelError::Transport("no space left on the disk".into());
    let video = RemotePath::parse("sd/video/loop.mp4").unwrap();
    for (applied, said) in [
        (
            Ok(Applied::AlbumNotRecorded(full.clone())),
            Some(DiagCode::ShutdownAlbumNotRecorded),
        ),
        (Ok(Applied::Album), None),
        (Ok(Applied::Nothing), None),
        (Ok(Applied::TurnedOff), None),
        (Ok(Applied::Video(video)), None),
        (Err(full), Some(DiagCode::ShutdownChoiceFailed)),
    ] {
        assert_eq!(said_of(&applied), said, "{applied:?}");
    }
    assert_ne!(
        DiagCode::ShutdownAlbumNotRecorded.text(),
        DiagCode::ShutdownChoiceFailed.text()
    );
}

/// D-2026-10-03-power-off-standby-3 (1): the final state, entered where
/// every link is opened or lent: once in it, nothing reaches a screen.
/// Neither a refresh nor a reconnection long due (the live screen's link
/// failed), nor any command (live mode on or off, brightness, release, a
/// restart, a storage operation), nor lending the live link, opening a
/// screen or claiming the screens. Live mode and the remembered live
/// screen stay; leaving the final state lets the reconnection go on.
#[test]
fn the_final_state_lets_nothing_reach_a_screen() {
    let (_root, folders) = folders_for("power-final", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default()).breaking_after(1, hung());
    let backend = backend_on(
        &folders,
        FakeBus::turing_88(),
        Recording::over(fake, &heard, Hold::Never),
    );
    backend.set_live(true, Some(DISPLAY), TIME).unwrap();
    let later = Instant::now() + Duration::from_secs(60);
    backend.tick_and_wait(TIME, later);
    assert!(
        backend.studio().reconnecting().is_some(),
        "{:?}",
        heard.all()
    );

    backend.enter_final_state();
    let before = heard.all();
    let long_due = later + Duration::from_secs(60);
    for _ in 0..3 {
        backend.tick_and_wait(TIME, long_due);
    }
    assert!(backend.studio().reconnect_due(long_due).is_none());
    assert!(backend.studio().tick(TIME, long_due).unwrap().is_none());
    assert!(backend.set_live(true, Some(DISPLAY), TIME).is_err());
    assert!(backend.set_live(false, None, TIME).is_err());
    assert!(backend.set_brightness(DISPLAY, 50).is_err());
    assert!(backend.release(DISPLAY).is_err());
    assert!(backend.restart_screen(MCU, TIME).is_err());
    assert!(backend.storage_overview(DISPLAY, TIME).is_err());
    assert!(backend.studio().lend_live_link(DISPLAY).is_err());
    assert!(backend.connect(DISPLAY).is_err());
    assert!(backend.storage.claim().is_err());
    assert!(backend.storage.ensure_idle().is_err());
    assert_eq!(
        heard.all(),
        before,
        "a screen heard something in the final state"
    );
    assert_eq!(backend.studio().live_key(), Some(DISPLAY));
    assert_eq!(
        settings(&folders.config).load().live_screen.as_deref(),
        Some(DISPLAY)
    );

    backend.leave_final_state();
    backend.tick_and_wait(TIME, long_due);
    assert_eq!(heard.since(before.len()).first(), Some(&"connect"));
    assert!(backend.storage.claim().is_ok());
}

/// D-2026-10-03-power-off-standby-3 (1): the session's own gates. In the
/// final state a link opened just before it (a screen going live as the
/// shutdown starts) is closed, never shown; the live link is not lent, not
/// handed out by stopping live mode, not used for the brightness, and no
/// frame is drawn: the shutdown alone takes it, once. Leaving the final
/// state stops live mode, which the caller shows again as at the start.
#[test]
fn the_session_hands_its_live_link_to_the_shutdown_only() {
    let (_root, folders) = folders_for("power-session", Standby::Keep);
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let backend = backend_on(&folders, FakeBus::turing_88(), connector.clone());
    backend.set_live(true, Some(DISPLAY), TIME).unwrap();
    let screen = discover_screens(&FakeBus::turing_88()).unwrap().remove(0);
    let opened = connector.connect(&screen).unwrap();
    let mut studio = backend.studio();
    assert!(
        matches!(studio.take_for_shutdown(), ForShutdown::Nothing),
        "outside the final state"
    );

    studio.enter_final_state();
    let before = heard.all();
    assert!(
        !studio.go_live(DISPLAY.into(), opened),
        "a screen went live"
    );
    assert!(studio.lend_live_link(DISPLAY).is_err());
    assert!(studio.stop_live().is_none());
    assert_eq!(studio.live_key(), Some(DISPLAY), "live mode stays");
    let brightness = Brightness::new(40).unwrap();
    assert!(studio.live_brightness(DISPLAY, brightness).is_err());
    assert!(
        studio
            .frame_for_screen(TIME, Instant::now())
            .unwrap()
            .is_none()
    );
    let ForShutdown::Link(link) = studio.take_for_shutdown() else {
        panic!("the shutdown did not get the live link");
    };
    assert_eq!(link.identity().model.id, MODEL);
    assert!(
        matches!(studio.take_for_shutdown(), ForShutdown::Nothing),
        "taken once"
    );
    drop(link);
    assert_eq!(heard.all(), before, "a screen heard something");

    assert!(studio.leave_final_state().is_none());
    assert_eq!(studio.live_key(), None);
    assert!(!studio.shutting_down());
}

/// D-2026-10-03-power-off-standby-3 (1): with no screen live, the shutdown
/// opens each awake rev C screen whose choice is not `keep` and applies it
/// (`album`: OPTIONS start mode 1 and RESTART, with a card); a screen
/// asleep (only its MCU on the bus) is never woken, and nothing is sent
/// to it. The catalog on disk then says the album's plan B was stored
/// last, where a boot media set after the choice had stored start mode 2
/// (review W5 of iteration 2).
#[test]
fn a_shutdown_opens_the_awake_screens_and_wakes_none() {
    let (_root, folders) = folders_for("power-awake", Standby::Album);
    let stored = |data: &Path| {
        let mut archive = DiskArchive::open(storage_dir(data)).unwrap();
        let catalog = archive.load().unwrap();
        catalog
            .screen(&ScreenKey::new(MODEL))
            .and_then(|r| r.stored)
    };
    let video = StoredPlanB {
        plan: PlanB::new(StartMode::Video, 0),
        brightness: None,
    };
    {
        let mut archive = DiskArchive::open(storage_dir(&folders.data)).unwrap();
        let mut catalog = archive.load().unwrap();
        catalog.screen_mut(&ScreenKey::new(MODEL)).stored = Some(video);
        archive.save(&catalog).unwrap();
    }
    let heard = Heard::default();
    let card = FakeStorage::default().with_card(1 << 30);
    let fake = FakeConnector::with_storage(card);
    let connector = Recording::over(fake.clone(), &heard, Hold::Never);
    let backend = backend_on(&folders, FakeBus::turing_88(), connector);
    assert_eq!(shut_down(&backend, Instant::now() + PATIENCE), Ending::Done);
    assert_eq!(
        heard.all(),
        [
            "connect",
            "storage.info",
            "storage.set_options",
            "storage.restart"
        ]
    );
    assert_eq!(
        fake.log().storage.options,
        Some(PlanB::new(StartMode::Image, 0))
    );
    let album = StoredPlanB {
        plan: PlanB::new(StartMode::Image, 0),
        brightness: None,
    };
    assert_eq!(stored(&folders.data), Some(album));

    let (_root, folders) = folders_for("power-asleep", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let only_the_mcu = FakeBus::turing_88()
        .endpoints()
        .unwrap()
        .into_iter()
        .filter(|endpoint| endpoint.address.0 == MCU)
        .collect();
    let backend = backend_on(
        &folders,
        FakeBus::new(only_the_mcu),
        Recording::over(fake, &heard, Hold::Never),
    );
    assert_eq!(shut_down(&backend, Instant::now() + PATIENCE), Ending::Done);
    assert!(heard.all().is_empty(), "{:?}", heard.all());
}

// ------------------------------------------- Linux: logind on a private bus --

/// A private `dbus-daemon` with a fake logind on it (none for a machine
/// without logind) and a monitor of the whole bus: where a test's app
/// watches for shutdowns.
#[cfg(target_os = "linux")]
pub(crate) struct LogindBus {
    // Declared in the order they stop: the monitor and the fake before the
    // bus.
    monitor: BusMonitor,
    fake: Option<FakeLogind>,
    bus: PrivateBus,
}

#[cfg(target_os = "linux")]
#[allow(clippy::expect_used, reason = "the helper of a failing test panics")]
impl LogindBus {
    /// The bus, with a fake logind whose delay is `delay` (`None`: no
    /// logind on it).
    pub(crate) fn start(delay: Option<Duration>) -> Self {
        Self::start_slow(delay, Duration::ZERO)
    }

    /// [`LogindBus::start`], the fake logind answering the read of its
    /// delay `slowness` after it is asked.
    fn start_slow(delay: Option<Duration>, slowness: Duration) -> Self {
        let bus = PrivateBus::start()
            .expect("dbus-daemon (package `dbus`) runs these tests; they fail without it");
        let fake =
            delay.map(|delay| FakeLogind::start_slow(&bus.address(), delay, slowness).unwrap());
        let monitor = BusMonitor::start(&bus.address()).unwrap();
        Self { monitor, fake, bus }
    }

    pub(crate) fn address(&self) -> BusAddress {
        self.bus.address()
    }

    fn logind(&self) -> &FakeLogind {
        self.fake.as_ref().expect("a fake logind on the bus")
    }

    /// Every message the studio's connection sent, as (kind, destination,
    /// interface, member, arguments): the studio's connection is the first
    /// that asked for `Inhibit`.
    fn said_by_the_studio(&self) -> Vec<(MessageKind, String, String, String, Vec<String>)> {
        let seen = self.monitor.messages();
        let studio = seen
            .iter()
            .find(|m| m.member.as_deref() == Some("Inhibit"))
            .and_then(|m| m.sender.clone());
        assert!(studio.is_some(), "the studio asked for no lock: {seen:#?}");
        let text = |field: &Option<String>| field.clone().unwrap_or_default();
        seen.iter()
            .filter(|m| m.sender == studio)
            .map(|m: &BusMessage| {
                (
                    m.kind,
                    text(&m.destination),
                    text(&m.interface),
                    text(&m.member),
                    m.args.clone(),
                )
            })
            .collect()
    }

    /// D-2026-10-03-power-off-standby-3 and -6: the studio's one connection
    /// to the bus said exactly `Hello` and the `AddMatch` of logind's
    /// `PrepareForShutdown` to the bus, then `Inhibit` of a shutdown delay
    /// to logind, and nothing else to anyone; with logind there, one lock
    /// is held.
    pub(crate) fn saw_the_studio_take_the_lock_and_say_nothing_else(&self) {
        if let Some(logind) = &self.fake {
            assert!(logind.wait_for_inhibitors(1, PATIENCE), "no lock taken");
            assert_eq!(logind.held(), 1);
            assert!(matches!(
                &logind.calls()[..],
                [LogindCall::Inhibit { what, who, why, mode, .. }]
                    if (what.as_str(), who.as_str(), why.as_str(), mode.as_str())
                        == ("shutdown", "Bezel", REASON, "delay")
            ));
        }
        assert!(self.monitor.wait_for(PATIENCE, |seen| {
            seen.iter().any(|m| m.member.as_deref() == Some("Inhibit"))
        }));
        let said = self.said_by_the_studio();
        let bus = "org.freedesktop.DBus";
        let calls: Vec<(MessageKind, &str, &str, &str)> = said
            .iter()
            .map(|(kind, to, interface, member, _)| {
                (*kind, to.as_str(), interface.as_str(), member.as_str())
            })
            .collect();
        assert_eq!(
            calls,
            [
                (MessageKind::MethodCall, bus, bus, "Hello"),
                (MessageKind::MethodCall, bus, bus, "AddMatch"),
                (
                    MessageKind::MethodCall,
                    "org.freedesktop.login1",
                    "org.freedesktop.login1.Manager",
                    "Inhibit"
                ),
            ],
            "{said:#?}"
        );
        let rule = said[1].4.join(" ");
        for part in [
            "type='signal'",
            "sender='org.freedesktop.login1'",
            "member='PrepareForShutdown'",
        ] {
            assert!(rule.contains(part), "{rule}");
        }
        assert_eq!(said[2].4, ["shutdown", "Bezel", REASON, "delay"]);
    }
}

/// A GIF source factory the tests never reach (no GIF command runs).
#[cfg(target_os = "linux")]
fn no_gifs() -> SourceFactory {
    Arc::new(
        |_: &UserAsked, _: &KlipyKey, _: &str| -> Arc<dyn GifSource> {
            Arc::new(FakeGifSource::new())
        },
    )
}

/// Runs the app's real `setup` (Tauri's mock runtime) on `folders`, its
/// fake 8.8" reached through `connector`, watching logind on `bus`; once the
/// runtime is up, `body` gets the app's backend, then the window closes and
/// the app ends. A panic in `body` fails the test once the app ended.
#[cfg(target_os = "linux")]
fn run_app(
    folders: Folders,
    connector: Recording,
    bus: BusAddress,
    body: impl FnOnce(&Shared) + 'static,
) {
    run_app_on(folders, FakeBus::turing_88(), connector, bus, body);
}

/// [`run_app`] with the screens of `screens`.
#[cfg(target_os = "linux")]
fn run_app_on(
    folders: Folders,
    screens: FakeBus,
    connector: impl ScreenConnector + Send + Sync + 'static,
    bus: BusAddress,
    body: impl FnOnce(&Shared) + 'static,
) {
    let start = Start {
        simulate: false,
        adapters: adapters_over(screens, connector),
        hidden: true,
        folders: Box::new(move |_: &AppHandle<MockRuntime>| Ok(folders)),
        gif_source: no_gifs(),
        tray: Box::new(|_: &AppHandle<MockRuntime>, _, _: &Texts| {
            let synced: LiveSync = Box::new(|_| {});
            Ok(synced)
        }),
        logind: bus,
    };
    let app = mock_builder()
        .setup(move |app| setup(app, start))
        .build(context_with_the_window())
        .unwrap();
    let mut body = Some(body);
    let (outcome, outcome_rx) = mpsc::channel();
    app.run_return(move |app, event| {
        if !matches!(event, RunEvent::Ready) {
            return;
        }
        let Some(body) = body.take() else {
            return;
        };
        let backend = Arc::clone(app.state::<Shared>().inner());
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&backend)));
        let _ = outcome.send(ran);
        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
            let _ = window.destroy();
        }
    });
    if let Err(panic) = outcome_rx.recv().unwrap() {
        std::panic::resume_unwind(panic);
    }
}

/// Waits until the remembered screen is live and showed a frame, as the
/// app's start does.
#[cfg(target_os = "linux")]
fn wait_live(backend: &Shared, heard: &Heard) {
    wait_until("the remembered screen went live", || {
        backend.studio().live_key() == Some(DISPLAY) && heard.count("present") >= 1
    });
}

/// What a screen heard but frames (and the turn before one), which the
/// refresh loop may send until the final state starts.
#[cfg(target_os = "linux")]
fn but_frames(heard: &[&'static str]) -> Vec<&'static str> {
    heard
        .iter()
        .copied()
        .filter(|call| !matches!(*call, "present" | "set_orientation"))
        .collect()
}

/// DoD row 2: at start the studio holds one *delay* lock of logind, and
/// its one connection to the bus says nothing but what takes it.
#[cfg(target_os = "linux")]
#[test]
fn linux_the_studio_holds_one_delay_lock_and_speaks_only_to_logind() {
    let logind = LogindBus::start(Some(LONG_DELAY));
    let (_root, folders) = folders_for("power-lock", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let seen = heard.clone();
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
    });
    logind.saw_the_studio_take_the_lock_and_say_nothing_else();
    assert_eq!(heard.count("turn_off_now"), 0, "{:?}", heard.all());
}

/// DoD row 2: `PrepareForShutdown(true)` applies the recorded choice
/// (`off`: TURNOFF through the live screen's open link, nothing else) and
/// only then closes the lock's descriptor: while the screen's action runs,
/// the lock is held. `liveScreen` stays in the settings, so the next start
/// shows the theme again.
#[cfg(target_os = "linux")]
#[test]
fn linux_true_applies_the_choice_and_only_then_closes_the_fd() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-true", off());
    let config = folders.config.clone();
    let heard = Heard::default();
    let (go, hold) = Hold::until();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake.clone(), &heard, hold);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        let before = seen.all().len();
        bus.logind().prepare_for_shutdown(true).unwrap();
        wait_until("TURNOFF reached the live screen", || {
            seen.count("turn_off_now") == 1
        });
        assert!(
            !bus.logind().wait_for_release(0, Duration::from_millis(500)),
            "the lock was released while the screen's action ran"
        );
        go.send(()).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        assert_eq!(but_frames(&seen.since(before)), ["turn_off_now"]);
        assert_eq!(seen.since(before).last(), Some(&"turn_off_now"));
        assert!(backend.studio().shutting_down());
    });
    assert_eq!(
        fake.log().storage.calls.last(),
        Some(&StorageCall::TurnOffNow)
    );
    assert!(matches!(
        &logind.logind().calls()[..],
        [LogindCall::Inhibit { .. }, LogindCall::Get { property, .. }]
            if property == "InhibitDelayMaxUSec"
    ));
    assert_eq!(
        settings(&config).load().live_screen.as_deref(),
        Some(DISPLAY)
    );
}

/// DoD row 2: with `keep` (no choice recorded), the lock is released at
/// once, not at the deadline, and no screen hears anything from then on.
#[cfg(target_os = "linux")]
#[test]
fn linux_keep_releases_the_lock_at_once_and_sends_nothing() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-keep", Standby::Keep);
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        let before = seen.all().len();
        let asked = Instant::now();
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        assert!(
            asked.elapsed() < Duration::from_secs(3),
            "{:?}",
            asked.elapsed()
        );
        let released = seen.all();
        std::thread::sleep(Duration::from_millis(2500));
        assert_eq!(
            seen.all(),
            released,
            "a screen heard something after the lock"
        );
        assert!(but_frames(&seen.since(before)).is_empty(), "{released:?}");
        assert!(backend.studio().shutting_down());
    });
}

/// DoD row 2 (critic of iteration 2): with `keep` the shutdown opens no
/// screen at all. Besides the live 8.8", a 5" rev C is awake but not live:
/// opening it (HELLO, STOP_MEDIA, ...) only to send it nothing would stop
/// what it plays on its own, so its connector hears nothing, not even a
/// connection, and the lock is released at once.
#[cfg(target_os = "linux")]
#[test]
fn linux_keep_opens_no_awake_screen_that_is_not_live() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-keep-five", Standby::Keep);
    record_for(&folders.data, FIVE, Standby::Keep);
    let (heard, five) = (Heard::default(), Heard::default());
    let main = Recording::over(
        FakeConnector::with_storage(FakeStorage::default()),
        &heard,
        Hold::Never,
    );
    let card = FakeStorage::default().with_card(1 << 30);
    let other = Recording::over(FakeConnector::with_storage(card), &five, Hold::Never);
    let connector = Apart {
        main,
        address: FIVE_DISPLAY,
        other,
    };
    let (bus, seen, five_heard) = (Arc::clone(&logind), heard.clone(), five.clone());
    let address = logind.address();
    run_app_on(folders, with_a_five(), connector, address, move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        assert!(five_heard.all().is_empty(), "the 5\" was opened at start");
        let before = seen.all().len();
        let asked = Instant::now();
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        assert!(
            asked.elapsed() < Duration::from_secs(3),
            "{:?}",
            asked.elapsed()
        );
        assert!(backend.studio().shutting_down());
        assert!(
            five_heard.all().is_empty(),
            "the shutdown opened the 5\": {:?}",
            five_heard.all()
        );
        assert!(
            but_frames(&seen.since(before)).is_empty(),
            "{:?}",
            seen.all()
        );
    });
    assert!(five.all().is_empty(), "{:?}", five.all());
}

/// DoD row 2, the other side of the case above: a choice that is not
/// `keep` on an awake rev C screen that is not live (the 5", `off`) opens
/// it and applies it, TURNOFF and nothing else, while the live 8.8"
/// (`keep`) hears nothing.
#[cfg(target_os = "linux")]
#[test]
fn linux_a_choice_opens_an_awake_screen_that_is_not_live() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-off-five", Standby::Keep);
    record_for(&folders.data, FIVE, off());
    let (heard, five) = (Heard::default(), Heard::default());
    let main = Recording::over(
        FakeConnector::with_storage(FakeStorage::default()),
        &heard,
        Hold::Never,
    );
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let other = Recording::over(fake.clone(), &five, Hold::Never);
    let connector = Apart {
        main,
        address: FIVE_DISPLAY,
        other,
    };
    let (bus, seen, five_heard) = (Arc::clone(&logind), heard.clone(), five.clone());
    let address = logind.address();
    run_app_on(folders, with_a_five(), connector, address, move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        let before = seen.all().len();
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        assert_eq!(five_heard.all(), ["connect", "turn_off_now"]);
        assert!(
            but_frames(&seen.since(before)).is_empty(),
            "{:?}",
            seen.all()
        );
    });
    assert_eq!(
        fake.log().storage.calls.last(),
        Some(&StorageCall::TurnOffNow)
    );
    assert_eq!(fake.log().connects, 1);
}

/// DoD row 2: after the action, in the final state, nothing reaches a
/// screen: not the refresh loop's ticks, not the reconnection that was due
/// (the live screen's link had failed: the shutdown opened the screen for
/// its choice instead), not what the window asks. Live mode and
/// `liveScreen` stay. Code written on purpose to get past the final state
/// (another thread opening the port) is left to code review
/// (D-2026-10-03-power-off-standby-6).
#[cfg(target_os = "linux")]
#[test]
fn linux_after_the_action_ticks_and_a_due_reconnection_reach_no_screen() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-after", off());
    let config = folders.config.clone();
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default()).breaking_after(2, hung());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        // The live link fails: an attempt to connect it again is due 2 s
        // later.
        wait_until("the live link failed", || {
            backend.studio().reconnecting().is_some()
        });
        let failed = seen.all().len();
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        let after = seen.all();
        assert_eq!(seen.since(failed), ["connect", "turn_off_now"]);
        // Past the attempt's time, the refresh loop ticking.
        std::thread::sleep(Duration::from_millis(3500));
        assert_eq!(
            seen.all(),
            after,
            "a screen heard something in the final state"
        );
        assert!(backend.set_live(true, Some(DISPLAY), TIME).is_err());
        assert!(backend.set_live(false, None, TIME).is_err());
        assert!(backend.set_brightness(DISPLAY, 40).is_err());
        assert!(backend.release(DISPLAY).is_err());
        assert!(backend.restart_screen(DISPLAY, TIME).is_err());
        assert!(backend.storage_overview(DISPLAY, TIME).is_err());
        assert_eq!(seen.all(), after);
        assert_eq!(backend.studio().live_key(), Some(DISPLAY));
    });
    assert_eq!(
        settings(&config).load().live_screen.as_deref(),
        Some(DISPLAY)
    );
}

/// DoD row 2: a screen that hangs during its action does not hold the
/// shutdown: the lock is released at logind's delay less 0.5 s.
#[cfg(target_os = "linux")]
#[test]
fn linux_a_hung_screen_releases_the_lock_at_the_deadline() {
    let delay = Duration::from_secs(3);
    let deadline = delay - MARGIN;
    let logind = Arc::new(LogindBus::start(Some(delay)));
    let (_root, folders) = folders_for("power-hung", off());
    let heard = Heard::default();
    let (go, hold) = Hold::until();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, hold);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        let asked = Instant::now();
        bus.logind().prepare_for_shutdown(true).unwrap();
        wait_until("TURNOFF reached the live screen", || {
            seen.count("turn_off_now") == 1
        });
        let early = deadline - Duration::from_millis(400);
        assert!(
            !bus.logind().wait_for_release(0, early),
            "released before the deadline"
        );
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        let took = asked.elapsed();
        // At logind's delay less 0.5 s, not at the delay itself.
        assert!(
            took >= deadline - Duration::from_millis(100)
                && took < deadline + Duration::from_millis(400),
            "{took:?}"
        );
        assert!(backend.studio().shutting_down());
        drop(go);
    });
}

/// Review W4 of iteration 2: the final state starts as soon as logind
/// announces the shutdown, not once its delay is read (a bus busy at
/// shutdown can take seconds to answer). With a logind that takes 2 s to
/// say its delay, the session is in its final state while the read waits,
/// and from then on the screen hears nothing (no frame of the refresh loop)
/// but its choice, TURNOFF.
#[cfg(target_os = "linux")]
#[test]
fn linux_the_final_state_starts_before_the_delay_is_read() {
    let logind = Arc::new(LogindBus::start_slow(
        Some(LONG_DELAY),
        Duration::from_secs(2),
    ));
    let (_root, folders) = folders_for("power-final-first", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        bus.logind().prepare_for_shutdown(true).unwrap();
        wait_until("the studio asked for logind's delay", || {
            let calls = bus.logind().calls();
            calls.iter().any(|c| matches!(c, LogindCall::Get { .. }))
        });
        assert!(
            backend.studio().shutting_down(),
            "the final state waits for logind's delay"
        );
        let asked = seen.all().len();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        assert_eq!(seen.since(asked), ["turn_off_now"], "{:?}", seen.all());
    });
}

/// Review W9 of iteration 1: the deadline is counted from logind's
/// announcement, as logind counts its delay, not from the answer to reading
/// the delay. With a logind that takes 1 s to say its 3 s delay, a screen
/// that hangs still releases the lock 2.5 s after `PrepareForShutdown`.
#[cfg(target_os = "linux")]
#[test]
fn linux_the_deadline_counts_from_the_announcement() {
    let delay = Duration::from_secs(3);
    let deadline = delay - MARGIN;
    let logind = Arc::new(LogindBus::start_slow(Some(delay), Duration::from_secs(1)));
    let (_root, folders) = folders_for("power-slow", off());
    let heard = Heard::default();
    let (go, hold) = Hold::until();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, hold);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        let asked = Instant::now();
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        let took = asked.elapsed();
        assert!(
            took >= deadline - Duration::from_millis(100)
                && took < deadline + Duration::from_millis(400),
            "{took:?}"
        );
        assert_eq!(seen.count("turn_off_now"), 1);
        drop(go);
    });
}

/// DoD row 2 (D-2026-10-03-power-off-standby-3 (1)): a storage job in
/// progress is cancelled and waited for before any choice is applied.
#[cfg(target_os = "linux")]
#[test]
fn linux_a_job_in_progress_is_cancelled_and_awaited_first() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-job", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        let (started, running) = mpsc::channel();
        let job = {
            let (backend, seen) = (Arc::clone(backend), seen.clone());
            std::thread::spawn(move || {
                let claim = backend.storage.claim().unwrap();
                let token = backend.storage.start_job();
                started.send(()).unwrap();
                let until = Instant::now() + PATIENCE;
                while !token.is_cancelled() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(10));
                }
                // The job winds down before it lets the screens go.
                std::thread::sleep(Duration::from_millis(300));
                seen.note("job stopped");
                backend.storage.end_job();
                drop(claim);
            })
        };
        running.recv_timeout(PATIENCE).unwrap();
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        job.join().unwrap();
        let stopped = seen.position("job stopped").unwrap();
        let applied = seen.position("turn_off_now").unwrap();
        assert!(stopped < applied, "{:?}", seen.all());
    });
}

/// DoD row 2: `PrepareForShutdown(false)` (the shutdown was cancelled) ends
/// the final state, takes the lock again and shows the theme live again,
/// as at the app's start.
#[cfg(target_os = "linux")]
#[test]
fn linux_false_ends_the_final_state_locks_again_and_resumes_live() {
    let logind = Arc::new(LogindBus::start(Some(LONG_DELAY)));
    let (_root, folders) = folders_for("power-false", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let (bus, seen) = (Arc::clone(&logind), heard.clone());
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(bus.logind().wait_for_inhibitors(1, PATIENCE));
        bus.logind().prepare_for_shutdown(true).unwrap();
        assert!(bus.logind().wait_for_release(0, PATIENCE));
        let applied = seen.all().len();
        assert!(backend.studio().shutting_down());

        bus.logind().prepare_for_shutdown(false).unwrap();
        assert!(bus.logind().wait_for_inhibitors(2, PATIENCE));
        assert_eq!(bus.logind().held(), 1);
        wait_until("live again", || {
            let since = seen.since(applied);
            !backend.studio().shutting_down()
                && since.first() == Some(&"connect")
                && since.contains(&"present")
        });
        assert_eq!(backend.studio().live_key(), Some(DISPLAY));
        assert!(backend.storage.claim().is_ok(), "claims are given again");
    });
    assert!(matches!(
        &logind.logind().calls()[..],
        [
            LogindCall::Inhibit { .. },
            LogindCall::Get { .. },
            LogindCall::Inhibit { .. }
        ]
    ));
}

/// D-2026-10-03-power-off-standby-3 (1): without logind on the bus the
/// studio says so (a fixed code) and runs on: the screen goes live, and
/// each screen keeps the plan B its choice wrote. Its connection asked for
/// the lock and said nothing else.
#[cfg(target_os = "linux")]
#[test]
fn linux_without_logind_the_studio_runs_on_its_plan_b() {
    let logind = LogindBus::start(None);
    let (_root, folders) = folders_for("power-no-logind", off());
    let heard = Heard::default();
    let fake = FakeConnector::with_storage(FakeStorage::default());
    let connector = Recording::over(fake, &heard, Hold::Never);
    let seen = heard.clone();
    run_app(folders, connector, logind.address(), move |backend| {
        wait_live(backend, &seen);
        assert!(!backend.studio().shutting_down());
    });
    logind.saw_the_studio_take_the_lock_and_say_nothing_else();
    assert!(logind.monitor.wait_for(PATIENCE, |seen| {
        seen.iter().any(|m| m.kind == MessageKind::Error)
    }));
    assert_eq!(
        hold(&logind.address()).err(),
        Some(Some(DiagCode::NoLogind))
    );
    assert_eq!(heard.count("turn_off_now"), 0);
}
