//! In-memory bus and screens for tests and demos: a fixed list of
//! endpoints, and screens that record what they are asked to do. Screens of
//! the families with storage (rev C, TUR_USB) also simulate their stored
//! files and device-side playback ([`FakeStorage`]). [`FakeHid`] stands for
//! the HID interface of a panel in desktop mode.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use bezel_core::domain::device::{Family, Transport, UsbId};
use bezel_core::domain::discovery::{
    DesktopModePanel, DeviceAddress, Endpoint, MonitorModeConfirmed, Screen, UsbLocation,
};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::Job;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::domain::standby::PlanB;
use bezel_core::domain::storage::{
    Capacity, Confirmed, FileName, Medium, RemotePath, Repeat, StartMode, StorageInfo,
    StorageLocation,
};
use bezel_core::ports::{DesktopModeHid, DeviceBus, ScreenConnector, ScreenLink, ScreenStorage};
use bezel_core::{BezelError, Result};

use crate::driver::{Sent, send_in_chunks};
use crate::hid_desktop;
use crate::wire::ScriptedWire;

/// Usable internal flash of a simulated screen (vendor reserve already
/// off): 1 GiB.
pub const FAKE_FLASH_BYTES: u64 = 1 << 30;
/// File bytes a simulated screen accepts between two progress reports.
pub const FAKE_UPLOAD_CHUNK: usize = 64 * 1024;

/// A [`DeviceBus`] that reports a fixed set of endpoints.
#[derive(Debug, Clone, Default)]
pub struct FakeBus {
    endpoints: Vec<Endpoint>,
}

impl FakeBus {
    /// A bus reporting exactly `endpoints`.
    pub fn new(endpoints: Vec<Endpoint>) -> Self {
        Self { endpoints }
    }

    /// A Turing 8.8" rev C: the CT88INCH MCU and the sunxi SoC behind one hub,
    /// exactly as the reference hardware enumerates on Linux.
    pub fn turing_88() -> Self {
        Self::new(vec![
            serial_endpoint(
                "/dev/ttyACM0",
                UsbId::new(0x1a86, 0xca88),
                Some("CT88INCH"),
                &[1, 1],
            ),
            serial_endpoint("/dev/ttyACM1", UsbId::new(0x0525, 0xa4a7), None, &[1, 2]),
        ])
    }

    /// A Turing USB panel in desktop mode: its HID interface (1a86:ad11) at
    /// `hid:/dev/hidraw7`, as the HID stack lists it on Linux.
    pub fn desktop_mode() -> Self {
        Self::new(vec![Endpoint {
            address: DeviceAddress(format!("{}/dev/hidraw7", hid_desktop::ADDRESS_PREFIX)),
            transport: Transport::Hid,
            usb: UsbId::new(0x1a86, 0xad11),
            serial_number: None,
            manufacturer: None,
            product: None,
            location: None,
        }])
    }

    /// This bus with `other`'s endpoints after its own.
    pub fn and(mut self, other: FakeBus) -> Self {
        self.endpoints.extend(other.endpoints);
        self
    }
}

fn serial_endpoint(port: &str, usb: UsbId, serial: Option<&str>, ports: &[u8]) -> Endpoint {
    Endpoint {
        address: DeviceAddress(port.to_string()),
        transport: Transport::Serial,
        usb,
        serial_number: serial.map(str::to_string),
        manufacturer: None,
        product: None,
        location: Some(UsbLocation {
            bus: "3".to_string(),
            ports: ports.to_vec(),
        }),
    }
}

impl DeviceBus for FakeBus {
    fn endpoints(&self) -> Result<Vec<Endpoint>> {
        Ok(self.endpoints.clone())
    }
}

/// One call that reached a simulated panel in desktop mode: the reports
/// written to it, byte for byte as they go to the HID stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidCall {
    /// The panel's HID address.
    pub address: DeviceAddress,
    /// The reports, in order (report id first).
    pub reports: Vec<Vec<u8>>,
}

/// A [`DesktopModeHid`] for tests and demos: it runs the real report
/// sequences against a scripted wire, answers the model query with a set
/// model byte (or not at all) and records every call. Like the real one, it
/// refuses a panel whose USB id is not one of desktop mode.
#[derive(Debug, Clone, Default)]
pub struct FakeHid {
    model_byte: Option<u8>,
    calls: Arc<Mutex<Vec<HidCall>>>,
}

impl FakeHid {
    /// A panel that answers the model query with `model_byte`.
    pub fn answering(model_byte: u8) -> Self {
        Self {
            model_byte: Some(model_byte),
            ..Self::default()
        }
    }

    /// A panel that never answers the model query.
    pub fn silent() -> Self {
        Self::default()
    }

    /// Every call so far (shared by clones).
    pub fn calls(&self) -> Vec<HidCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn record(&self, panel: &DesktopModePanel, wire: ScriptedWire) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(HidCall {
                address: panel.address().clone(),
                reports: wire.sent,
            });
    }
}

fn hid_failure(panel: &DesktopModePanel, e: &std::io::Error) -> BezelError {
    BezelError::Transport(format!("{}: {e}", panel.address()))
}

impl DesktopModeHid for FakeHid {
    fn query_model(
        &self,
        panel: &DesktopModePanel,
        _confirmed: &MonitorModeConfirmed,
    ) -> Result<Option<u8>> {
        hid_desktop::ensure_desktop_mode(&panel.address().0, panel.hid.usb)?;
        let answer = self.model_byte.map(hid_desktop::simulated_answer);
        let mut wire = ScriptedWire::with_replies(answer);
        let model = hid_desktop::ask_model(&mut wire).map_err(|e| hid_failure(panel, &e))?;
        self.record(panel, wire);
        Ok(model)
    }

    fn back_to_monitor(
        &self,
        panel: &DesktopModePanel,
        _confirmed: MonitorModeConfirmed,
    ) -> Result<()> {
        hid_desktop::ensure_desktop_mode(&panel.address().0, panel.hid.usb)?;
        let mut wire = ScriptedWire::default();
        hid_desktop::switch_back(&mut wire).map_err(|e| hid_failure(panel, &e))?;
        self.record(panel, wire);
        Ok(())
    }
}

/// What a [`FakeScreen`] was asked to do, shared with the test that built it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FakeLog {
    /// Frames presented, in order.
    pub frames: Vec<Frame>,
    /// Brightness levels set.
    pub brightness: Vec<Brightness>,
    /// Orientations set.
    pub orientations: Vec<Orientation>,
    /// `screen_off` calls.
    pub offs: usize,
    /// `release` calls.
    pub releases: usize,
    /// Connections opened (`ScreenConnector::connect` that succeeded).
    pub connects: usize,
    /// Screens restarted through the connector (`ScreenConnector::restart`),
    /// by the address they had.
    pub restarts: Vec<String>,
    /// The simulated storage, shared by every screen of the connector.
    pub storage: FakeStorage,
    /// What the screens keep for how they start, in the order it reached
    /// them: one list across the link's levels and the storage's OPTIONS.
    pub kept: Vec<Kept>,
}

/// What reached a simulated screen that it keeps for how it starts: each
/// backlight level set and each plan B written, in order (`FakeLog::kept`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// `set_brightness`.
    Brightness(Brightness),
    /// `set_options`: the plan B, and the backlight level its OPTIONS
    /// carries, as the rev C driver writes it: the last level its link set
    /// (`None`: none yet, so the vendor's default).
    Options(PlanB, Option<Brightness>),
}

/// One call that reached a simulated screen's storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageCall {
    /// `info`.
    Info,
    /// `list` of a folder.
    List(StorageLocation),
    /// `size` of a file.
    Size(RemotePath),
    /// `upload` of that many bytes.
    Upload(RemotePath, usize),
    /// `delete`.
    Delete(RemotePath),
    /// `play_video`.
    PlayVideo(RemotePath, Repeat),
    /// `play_image`.
    PlayImage(RemotePath),
    /// `stop`.
    Stop,
    /// `set_options`: the plan B written (OPTIONS).
    Options(PlanB),
    /// `restart` of the screen's system.
    Restart,
    /// The link's `turn_off_now`, logged here so that a shutdown action's
    /// whole sequence reads in one list.
    TurnOffNow,
}

impl StorageCall {
    /// True for calls that change what the screen stores, shows or keeps
    /// (everything but the three queries).
    pub fn changes_the_screen(&self) -> bool {
        !matches!(
            self,
            StorageCall::Info | StorageCall::List(_) | StorageCall::Size(_)
        )
    }
}

/// What a simulated screen plays on its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Playback {
    /// Nothing.
    #[default]
    Idle,
    /// A stored video.
    Video(RemotePath, Repeat),
    /// A stored image.
    Image(RemotePath),
}

/// The files and playback of a simulated screen with storage (a Turing 8.8"
/// by default: 1 GiB of internal flash, no card, nothing stored). Sizes are
/// bytes, as the port speaks them; a file of 0 bytes reads as absent like
/// on the real screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeStorage {
    /// Usable internal flash.
    pub internal_total: u64,
    /// Usable card space; `None` without a card.
    pub card_total: Option<u64>,
    /// Stored files and their bytes.
    pub files: BTreeMap<RemotePath, Vec<u8>>,
    /// What plays on the screen.
    pub playback: Playback,
    /// The start mode of the last plan B written, if any.
    pub start_mode: Option<StartMode>,
    /// The last plan B written (OPTIONS), if any.
    pub options: Option<PlanB>,
    /// Every storage call, in order.
    pub calls: Vec<StorageCall>,
    /// Bytes every upload loses at its end (0: none), like a transfer the
    /// screen stored short: its stored size then fails verification.
    pub short_by: usize,
    /// Stored files whose size a query cannot report, like the files a
    /// TUR_USB screen holds that Bezel did not write: the query answers
    /// `Unsupported` until an upload replaces the file.
    pub size_unknown: BTreeSet<RemotePath>,
}

impl Default for FakeStorage {
    fn default() -> Self {
        Self {
            internal_total: FAKE_FLASH_BYTES,
            card_total: None,
            files: BTreeMap::new(),
            playback: Playback::Idle,
            start_mode: None,
            options: None,
            calls: Vec::new(),
            short_by: 0,
            size_unknown: BTreeSet::new(),
        }
    }
}

impl FakeStorage {
    /// With a memory card of `total` usable bytes.
    pub fn with_card(mut self, total: u64) -> Self {
        self.card_total = Some(total);
        self
    }

    /// With a stored file.
    pub fn with_file(mut self, path: RemotePath, data: Vec<u8>) -> Self {
        self.files.insert(path, data);
        self
    }

    /// With a stored file whose size a query cannot report
    /// ([`Self::size_unknown`]).
    pub fn with_file_of_unknown_size(mut self, path: RemotePath, data: Vec<u8>) -> Self {
        self.size_unknown.insert(path.clone());
        self.with_file(path, data)
    }

    /// Capacity and use, as `ScreenStorage::info` reports them.
    pub fn info(&self) -> StorageInfo {
        StorageInfo {
            internal: self.capacity(Medium::Internal, self.internal_total),
            card: self.card_total.map(|t| self.capacity(Medium::Card, t)),
        }
    }

    /// Size of a stored file; `None` when absent or empty.
    pub fn size(&self, path: &RemotePath) -> Option<u64> {
        let bytes = self.files.get(path).map_or(0, Vec::len) as u64;
        (bytes > 0).then_some(bytes)
    }

    /// What a size query answers: [`Self::size`], or `Unsupported` for a
    /// stored file of [`Self::size_unknown`].
    fn size_query(&self, path: &RemotePath) -> Result<Option<u64>> {
        if self.size_unknown.contains(path) && self.files.contains_key(path) {
            return Err(BezelError::Unsupported(format!(
                "the simulated screen cannot report the size of {path}"
            )));
        }
        Ok(self.size(path))
    }

    fn capacity(&self, medium: Medium, total: u64) -> Capacity {
        let used = self
            .files
            .iter()
            .filter(|(p, _)| p.location.medium == medium)
            .map(|(_, d)| d.len() as u64)
            .sum::<u64>()
            .min(total);
        Capacity {
            total,
            used,
            free: total - used,
        }
    }

    /// Checks space and card, stops playback and creates (or truncates) the
    /// file, as the real screens do when they accept an UPLOAD_FILE header.
    fn begin_upload(&mut self, path: &RemotePath, bytes: usize) -> Result<()> {
        let Some(capacity) = self.info().capacity(path.location.medium) else {
            return Err(BezelError::Transport(
                "the simulated screen has no card".into(),
            ));
        };
        let replaced = self.size(path).unwrap_or(0);
        if bytes as u64 >= capacity.free + replaced {
            return Err(BezelError::Transport(format!(
                "no room for {bytes} bytes on the simulated screen"
            )));
        }
        self.playback = Playback::Idle;
        self.size_unknown.remove(path);
        self.files.insert(path.clone(), Vec::with_capacity(bytes));
        Ok(())
    }

    /// Drops the last [`Self::short_by`] bytes of a finished upload.
    fn end_upload(&mut self, path: &RemotePath) {
        if let Some(data) = self.files.get_mut(path) {
            data.truncate(data.len().saturating_sub(self.short_by));
        }
    }

    /// What a cancelled upload left: the bytes received, or nothing.
    fn after_cancel(&mut self, path: &RemotePath) -> Option<u64> {
        let partial = self.size(path);
        if partial.is_none() {
            self.files.remove(path);
        }
        partial
    }

    fn stored(&self, path: &RemotePath) -> Result<()> {
        match self.size(path) {
            Some(_) => Ok(()),
            None => Err(BezelError::Timeout(format!(
                "the simulated screen: no {path} to play"
            ))),
        }
    }
}

/// Failures scripted into a [`FakeConnector`]'s screens.
#[derive(Debug, Default)]
struct Script {
    /// `present` fails once with this error when the screens already showed
    /// that many frames in all.
    break_at: Option<(usize, BezelError)>,
    /// Once this many connections were opened, the next ones fail with
    /// `refusals`, one each, in order.
    refuse_after: usize,
    refusals: VecDeque<BezelError>,
}

/// Connects to an in-memory screen that records everything.
#[derive(Debug, Clone, Default)]
pub struct FakeConnector {
    log: Arc<Mutex<FakeLog>>,
    script: Arc<Mutex<Script>>,
}

impl FakeConnector {
    /// A connector whose screens start with `storage` (the families with
    /// storage only).
    pub fn with_storage(storage: FakeStorage) -> Self {
        let log = FakeLog {
            storage,
            ..FakeLog::default()
        };
        Self {
            log: Arc::new(Mutex::new(log)),
            script: Arc::default(),
        }
    }

    /// Its screens' `present` fails once with `error` when they already
    /// showed `frames` frames in all: a link that breaks mid-run (a hung
    /// screen answers `BezelError::Hung`).
    #[must_use]
    pub fn breaking_after(self, frames: usize, error: BezelError) -> Self {
        self.script().break_at = Some((frames, error));
        self
    }

    /// Once `connects` connections were opened, the next ones fail with
    /// `errors`, one each, in order: a screen that is not back yet, or one
    /// that answers nothing even after its restart.
    #[must_use]
    pub fn refusing_after(self, connects: usize, errors: Vec<BezelError>) -> Self {
        let mut script = self.script();
        script.refuse_after = connects;
        script.refusals = errors.into();
        drop(script);
        self
    }

    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.script.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A snapshot of what the screens were asked to do.
    pub fn log(&self) -> FakeLog {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ScreenConnector for FakeConnector {
    /// Records the restart of a screen that can be restarted (a rev C screen
    /// with its MCU listed); any other is `Unsupported`, like the real one.
    fn restart(&self, screen: &Screen) -> Result<()> {
        if !screen.restartable() {
            return Err(BezelError::Unsupported(
                "the simulated screen cannot be restarted".into(),
            ));
        }
        let address = screen.address().map(|a| a.0.clone()).unwrap_or_default();
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .restarts
            .push(address);
        Ok(())
    }

    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        let model = screen
            .model()
            .ok_or_else(|| BezelError::Transport("simulated screen needs a single model".into()))?;
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        let mut script = self.script();
        if log.connects >= script.refuse_after
            && let Some(refusal) = script.refusals.pop_front()
        {
            return Err(refusal);
        }
        drop(script);
        log.connects += 1;
        drop(log);
        Ok(Box::new(FakeScreen {
            identity: ScreenIdentity {
                model,
                firmware: Some("simulated".into()),
            },
            orientation: Orientation::Portrait,
            brightness: None,
            has_storage: matches!(model.family, Family::TuringRevC | Family::TuringUsb),
            log: Arc::clone(&self.log),
            script: Arc::clone(&self.script),
        }))
    }
}

/// An in-memory screen that checks frame sizes like a real one.
#[derive(Debug)]
pub struct FakeScreen {
    identity: ScreenIdentity,
    orientation: Orientation,
    /// The last level this link set: what its OPTIONS carry.
    brightness: Option<Brightness>,
    has_storage: bool,
    log: Arc<Mutex<FakeLog>>,
    script: Arc<Mutex<Script>>,
}

impl FakeScreen {
    fn record(&self, f: impl FnOnce(&mut FakeLog)) {
        f(&mut self.log.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// Runs `f` on the shared storage after recording `call`.
    fn store<T>(&self, call: StorageCall, f: impl FnOnce(&mut FakeStorage) -> T) -> T {
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        log.storage.calls.push(call);
        f(&mut log.storage)
    }

    /// Runs `f` on the shared storage without recording a call.
    fn with_storage<T>(&self, f: impl FnOnce(&mut FakeStorage) -> T) -> T {
        f(&mut self
            .log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .storage)
    }
}

impl ScreenLink for FakeScreen {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.brightness = Some(brightness);
        self.record(|l| {
            l.brightness.push(brightness);
            l.kept.push(Kept::Brightness(brightness));
        });
        Ok(())
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.orientation = orientation;
        self.record(|l| l.orientations.push(orientation));
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        let expected = self.identity.model.panel.in_orientation(self.orientation);
        if frame.size() != expected {
            return Err(BezelError::Transport(
                "frame size does not match the panel".into(),
            ));
        }
        let shown = self
            .log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .frames
            .len();
        let mut script = self.script.lock().unwrap_or_else(PoisonError::into_inner);
        if script.break_at.as_ref().is_some_and(|(at, _)| shown >= *at)
            && let Some((_, error)) = script.break_at.take()
        {
            return Err(error);
        }
        drop(script);
        self.record(|l| l.frames.push(frame.clone()));
        Ok(())
    }

    fn screen_off(&mut self) -> Result<()> {
        self.record(|l| l.offs += 1);
        Ok(())
    }

    /// Records [`StorageCall::TurnOffNow`] (not an `offs`).
    fn turn_off_now(&mut self) -> Result<()> {
        self.store(StorageCall::TurnOffNow, |_| ());
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        self.record(|l| l.releases += 1);
        Ok(())
    }

    fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
        if self.has_storage { Some(self) } else { None }
    }
}

impl ScreenStorage for FakeScreen {
    fn info(&mut self) -> Result<StorageInfo> {
        Ok(self.store(StorageCall::Info, |s| s.info()))
    }

    fn list(&mut self, location: StorageLocation) -> Result<Vec<FileName>> {
        let names = self.store(StorageCall::List(location), |s| {
            let here = s.files.keys().filter(|p| p.location == location);
            here.map(|p| p.name.clone()).collect()
        });
        Ok(names)
    }

    fn size(&mut self, path: &RemotePath) -> Result<Option<u64>> {
        self.store(StorageCall::Size(path.clone()), |s| s.size_query(path))
    }

    /// Accepts the data in chunks of [`FAKE_UPLOAD_CHUNK`] bytes, reporting
    /// after each and checking the cancel token between them. A cancelled
    /// upload keeps what arrived, like the real screens, and says so.
    fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()> {
        let call = StorageCall::Upload(path.clone(), data.len());
        self.store(call, |s| s.begin_upload(path, data.len()))?;
        let sent = send_in_chunks(data, FAKE_UPLOAD_CHUNK, job, |chunk| {
            self.with_storage(|s| s.files.entry(path.clone()).or_default().extend(chunk));
            Ok(())
        })?;
        match sent {
            Sent::All => {
                self.with_storage(|s| s.end_upload(path));
                Ok(())
            }
            Sent::Cancelled { .. } => Err(BezelError::Cancelled {
                partial: self.with_storage(|s| s.after_cancel(path)),
            }),
        }
    }

    fn delete(&mut self, path: &RemotePath, _confirmed: Confirmed) -> Result<()> {
        self.store(StorageCall::Delete(path.clone()), |s| {
            s.size_unknown.remove(path);
            s.files.remove(path)
        });
        Ok(())
    }

    fn play_video(&mut self, path: &RemotePath, repeat: Repeat) -> Result<()> {
        self.store(StorageCall::PlayVideo(path.clone(), repeat), |s| {
            s.stored(path)?;
            s.playback = Playback::Video(path.clone(), repeat);
            Ok(())
        })
    }

    fn play_image(&mut self, path: &RemotePath) -> Result<()> {
        self.store(StorageCall::PlayImage(path.clone()), |s| {
            s.stored(path)?;
            s.playback = Playback::Image(path.clone());
            Ok(())
        })
    }

    fn stop(&mut self) -> Result<()> {
        self.store(StorageCall::Stop, |s| s.playback = Playback::Idle);
        Ok(())
    }

    /// Records the plan B, and in [`FakeLog::kept`] with the level this
    /// link last set.
    fn set_options(&mut self, plan: PlanB, _confirmed: Confirmed) -> Result<()> {
        let level = self.brightness;
        self.record(|l| l.kept.push(Kept::Options(plan, level)));
        self.store(StorageCall::Options(plan), |s| {
            s.start_mode = Some(plan.start_mode);
            s.options = Some(plan);
        });
        Ok(())
    }

    /// Records the restart; what plays stops, what is stored stays.
    fn restart(&mut self, _confirmed: Confirmed) -> Result<()> {
        self.store(StorageCall::Restart, |s| s.playback = Playback::Idle);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::app::{discover_screens, open_screen};
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::job::{CancelToken, Progress};
    use bezel_core::domain::screen::Confirm;
    use bezel_core::domain::storage::{BootMedia, Operation};

    #[test]
    fn fake_hid_runs_the_real_reports_and_records_them() {
        use bezel_core::app::{discover_devices, leave_desktop_mode};

        let bus = FakeBus::turing_88().and(FakeBus::desktop_mode());
        let found = discover_devices(&bus).unwrap();
        assert_eq!(found.screens.len(), 1);
        assert_eq!(found.desktop_mode.len(), 1);

        let hid = FakeHid::answering(0x88);
        assert!(leave_desktop_mode(&bus, &hid, None, Confirm::No).is_err());
        assert!(hid.calls().is_empty(), "nothing without Confirm::Yes");

        let done = leave_desktop_mode(&bus, &hid, None, Confirm::Yes).unwrap();
        assert_eq!(done.model().map(|m| m.id.0), Some("turing-usb-8.8"));
        let calls = hid.clone().calls();
        let [first, second] = hid_desktop::back_to_monitor_reports();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].address.0, "hid:/dev/hidraw7");
        assert_eq!(calls[0].reports, vec![hid_desktop::model_query()]);
        assert_eq!(calls[1].reports, vec![first, second]);

        let silent = FakeHid::silent();
        let done = leave_desktop_mode(&bus, &silent, Some("hid:/dev/hidraw7"), Confirm::Yes);
        assert_eq!(done.unwrap().model_byte, None);
        assert_eq!(silent.calls().len(), 2, "switched back all the same");

        let mut stray = found.desktop_mode[0].clone();
        stray.hid.usb = UsbId::new(0x046d, 0xc52b);
        let confirmed = MonitorModeConfirmed::require(Confirm::Yes).unwrap();
        assert!(silent.query_model(&stray, &confirmed).is_err());
        assert!(silent.back_to_monitor(&stray, confirmed).is_err());
        assert_eq!(silent.calls().len(), 2, "a stray device gets nothing");
    }

    #[test]
    fn fake_screen_records_and_checks_sizes() {
        let connector = FakeConnector::default();
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        assert_eq!(link.identity().model.id.0, "turing-8.8");
        link.set_orientation(Orientation::Landscape).unwrap();
        let wide = Frame::filled(link.identity().model.panel.transposed(), Rgba::BLACK);
        link.present(&wide).unwrap();
        assert!(
            link.present(&Frame::filled(link.identity().model.panel, Rgba::BLACK))
                .is_err()
        );
        link.set_brightness(Brightness::MAX).unwrap();
        link.screen_off().unwrap();
        link.release().unwrap();
        let log = connector.log();
        assert_eq!(log.frames.len(), 1);
        assert_eq!(log.orientations, vec![Orientation::Landscape]);
        assert_eq!((log.offs, log.releases, log.brightness.len()), (1, 1, 1));
    }

    #[test]
    fn scripted_screens_break_once_and_refuse_connections() {
        let hung = BezelError::Hung("stalled".into());
        let away = BezelError::ScreenNotFound("away".into());
        let connector = FakeConnector::default()
            .breaking_after(1, hung.clone())
            .refusing_after(1, vec![away.clone()]);
        let bus = FakeBus::turing_88();
        let frame = Frame::filled(
            bezel_core::domain::geometry::Size::new(480, 1920),
            Rgba::BLACK,
        );
        let mut link = open_screen(&bus, &connector, None).unwrap();
        link.present(&frame).unwrap();
        assert_eq!(link.present(&frame).err(), Some(hung), "the second frame");
        link.present(&frame).unwrap();
        assert_eq!(open_screen(&bus, &connector, None).err(), Some(away));
        assert!(open_screen(&bus, &connector, None).is_ok());
        let log = connector.log();
        assert_eq!((log.connects, log.frames.len()), (2, 2));
    }

    #[test]
    fn ambiguous_screens_cannot_be_simulated() {
        let bus = FakeBus::new(vec![serial_endpoint(
            "COM3",
            UsbId::new(0x1a86, 0xca21),
            Some("CT21INCH"),
            &[1],
        )]);
        assert!(open_screen(&bus, &FakeConnector::default(), None).is_err());
    }

    fn remote(text: &str) -> RemotePath {
        RemotePath::parse(text).unwrap()
    }

    fn confirmed() -> Confirmed {
        Confirmed::require(Confirm::Yes, &Operation::Boot(BootMedia::Default)).unwrap()
    }

    /// Uploads `data`, cancelling once `cancel_at` bytes were reported;
    /// returns the result and the reported byte counts.
    fn upload(
        storage: &mut dyn ScreenStorage,
        path: &RemotePath,
        data: &[u8],
        cancel_at: Option<u64>,
    ) -> (Result<()>, Vec<u64>) {
        let token = CancelToken::new();
        let remote = token.clone();
        let mut seen = Vec::new();
        let mut sink = |p: Progress| {
            seen.push(p.done);
            if cancel_at.is_some_and(|at| p.done >= at) {
                remote.cancel();
            }
        };
        let mut job = Job::new(&token, &mut sink);
        let result = storage.upload(path, data, &mut job);
        (result, seen)
    }

    #[test]
    fn fake_storage_simulates_an_88_inch_screen() {
        let video = remote("internal/video/loop.mp4");
        let connector = FakeConnector::with_storage(
            FakeStorage::default()
                .with_card(1 << 20)
                .with_file(video.clone(), vec![7; 100]),
        );
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let storage = link.storage().expect("the 8.8\" stores files");
        let info = storage.info().unwrap();
        assert_eq!(
            info.internal,
            Capacity {
                total: FAKE_FLASH_BYTES,
                used: 100,
                free: FAKE_FLASH_BYTES - 100
            }
        );
        assert_eq!(info.card.map(|c| c.free), Some(1 << 20));
        assert_eq!(
            storage.list(video.location).unwrap(),
            std::slice::from_ref(&video.name)
        );

        // The theme runtime's flow: look for the video, then loop it.
        assert_eq!(storage.size(&remote("sd/video/loop.mp4")).unwrap(), None);
        assert_eq!(storage.size(&video).unwrap(), Some(100));
        storage.play_video(&video, Repeat::Loop).unwrap();
        let missing = remote("internal/image/none.png");
        assert!(storage.play_image(&missing).is_err());
        assert!(storage.play_video(&missing, Repeat::Once).is_err());

        // Uploads arrive in chunks, stop playback and land on the medium.
        let clip = remote("sd/video/clip.mp4");
        let data = vec![1u8; FAKE_UPLOAD_CHUNK * 2 + 5];
        let (result, seen) = upload(storage, &clip, &data, None);
        result.unwrap();
        let chunk = FAKE_UPLOAD_CHUNK as u64;
        assert_eq!(seen, [0, chunk, 2 * chunk, data.len() as u64]);
        assert_eq!(connector.log().storage.playback, Playback::Idle);
        assert_eq!(
            storage.info().unwrap().card.map(|c| c.used),
            Some(data.len() as u64)
        );
        let image = remote("sd/image/logo.png");
        let (result, _) = upload(storage, &image, b"png", None);
        result.unwrap();
        storage.play_image(&image).unwrap();
        storage.stop().unwrap();
        let plan = PlanB::new(StartMode::Video, 3);
        storage.set_options(plan, confirmed()).unwrap();
        storage.delete(&video, confirmed()).unwrap();
        storage.delete(&video, confirmed()).unwrap();

        let log = connector.log().storage;
        assert_eq!(log.files[&clip], data);
        assert!(!log.files.contains_key(&video));
        assert_eq!(
            (log.playback, log.start_mode, log.options),
            (Playback::Idle, Some(StartMode::Video), Some(plan))
        );
        let writes: Vec<&StorageCall> = log
            .calls
            .iter()
            .filter(|c| c.changes_the_screen())
            .collect();
        assert_eq!(
            writes,
            [
                &StorageCall::PlayVideo(video.clone(), Repeat::Loop),
                &StorageCall::PlayImage(missing.clone()),
                &StorageCall::PlayVideo(missing, Repeat::Once),
                &StorageCall::Upload(clip, data.len()),
                &StorageCall::Upload(image.clone(), 3),
                &StorageCall::PlayImage(image),
                &StorageCall::Stop,
                &StorageCall::Options(plan),
                &StorageCall::Delete(video.clone()),
                &StorageCall::Delete(video),
            ]
        );
    }

    #[test]
    fn fake_screens_record_the_shutdown_actions_in_order() {
        let video = remote("sd/video/loop.mp4");
        let connector = FakeConnector::with_storage(
            FakeStorage::default()
                .with_card(1 << 20)
                .with_file(video.clone(), vec![7; 100]),
        );
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        link.turn_off_now().unwrap();
        let storage = link.storage().unwrap();
        storage.play_video(&video, Repeat::Loop).unwrap();
        let album = PlanB::new(StartMode::Image, 0);
        storage.set_options(album, confirmed()).unwrap();
        storage.restart(confirmed()).unwrap();
        let log = connector.log();
        assert_eq!(log.offs, 0, "turn_off_now is not screen_off");
        assert_eq!(
            log.storage.calls,
            [
                StorageCall::TurnOffNow,
                StorageCall::PlayVideo(video, Repeat::Loop),
                StorageCall::Options(album),
                StorageCall::Restart,
            ]
        );
        assert!(
            log.storage
                .calls
                .iter()
                .all(StorageCall::changes_the_screen)
        );
        assert_eq!(log.storage.playback, Playback::Idle, "the restart stops it");
        assert_eq!(log.storage.options, Some(album));
    }

    #[test]
    fn fake_uploads_can_be_cancelled_and_run_out_of_room() {
        let connector = FakeConnector::with_storage(FakeStorage {
            internal_total: 3 * FAKE_UPLOAD_CHUNK as u64,
            ..FakeStorage::default()
        });
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let storage = link.storage().unwrap();
        let clip = remote("internal/video/clip.mp4");
        let data = vec![1u8; FAKE_UPLOAD_CHUNK + 1];

        // Cancelled after one chunk: what arrived stays, nothing is deleted.
        let (result, _) = upload(storage, &clip, &data, Some(1));
        let chunk = FAKE_UPLOAD_CHUNK as u64;
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(chunk)
            })
        );
        assert_eq!(storage.size(&clip).unwrap(), Some(chunk));

        // Cancelled before any chunk: nothing is left.
        let (result, _) = upload(storage, &clip, &data, Some(0));
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        assert_eq!(storage.size(&clip).unwrap(), None);
        assert!(storage.list(clip.location).unwrap().is_empty());

        // No room, no card.
        let big = vec![0u8; 3 * FAKE_UPLOAD_CHUNK];
        let (result, _) = upload(storage, &clip, &big, None);
        assert!(
            matches!(result, Err(BezelError::Transport(_))),
            "{result:?}"
        );
        let (result, _) = upload(storage, &remote("sd/video/a.mp4"), &data, None);
        assert!(
            matches!(result, Err(BezelError::Transport(_))),
            "{result:?}"
        );
        assert_eq!(storage.info().unwrap().card, None);
    }

    #[test]
    fn fake_uploads_can_arrive_short() {
        let connector = FakeConnector::with_storage(FakeStorage {
            short_by: 2,
            ..FakeStorage::default()
        });
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let storage = link.storage().unwrap();
        let clip = remote("internal/video/clip.mp4");
        let (result, _) = upload(storage, &clip, &[1, 2, 3, 4, 5], None);
        result.unwrap();
        assert_eq!(storage.size(&clip).unwrap(), Some(3));
        assert_eq!(connector.log().storage.files[&clip], [1, 2, 3]);
    }

    #[test]
    fn fake_files_of_unknown_size_answer_unsupported_until_replaced() {
        let (old, gone) = (remote("internal/video/old.mp4"), remote("sd/image/a.png"));
        let connector = FakeConnector::with_storage(
            FakeStorage::default()
                .with_card(1 << 20)
                .with_file_of_unknown_size(old.clone(), vec![7; 10])
                .with_file_of_unknown_size(gone.clone(), vec![7; 10]),
        );
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let storage = link.storage().unwrap();
        let unknown = storage.size(&old).unwrap_err();
        assert!(matches!(unknown, BezelError::Unsupported(_)), "{unknown}");
        let (result, _) = upload(storage, &old, &[1, 2, 3], None);
        result.unwrap();
        assert_eq!(storage.size(&old).unwrap(), Some(3));
        storage.delete(&gone, confirmed()).unwrap();
        assert_eq!(storage.size(&gone).unwrap(), None);
        assert!(connector.log().storage.size_unknown.is_empty());
    }

    #[test]
    fn screens_without_storage_have_none() {
        let weact = FakeBus::new(vec![serial_endpoint(
            "/dev/ttyACM0",
            UsbId::new(0x1a86, 0xfe0c),
            Some("AD0001"),
            &[1],
        )]);
        let connector = FakeConnector::default();
        let mut link = open_screen(&weact, &connector, None).unwrap();
        assert_eq!(link.identity().model.id.0, "weact-fs-0.96");
        assert!(link.storage().is_none());
        assert!(connector.log().storage.calls.is_empty());
    }

    #[test]
    fn turing_88_preset_is_one_awake_screen() {
        let screens = discover_screens(&FakeBus::turing_88()).unwrap();
        assert_eq!(screens.len(), 1);
        assert!(screens[0].display.is_some() && screens[0].wake.is_some());
        assert!(discover_screens(&FakeBus::default()).unwrap().is_empty());
    }
}
