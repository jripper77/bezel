//! In-memory bus and screens for tests and demos: a fixed list of
//! endpoints, and screens that record what they are asked to do. Screens of
//! the families with storage (rev C, TUR_USB) also simulate their stored
//! files and device-side playback ([`FakeStorage`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use bezel_core::domain::device::{Family, Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint, Screen, UsbLocation};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::Job;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::domain::storage::{
    Capacity, Confirmed, FileName, Medium, RemotePath, Repeat, StartMode, StorageInfo,
    StorageLocation,
};
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink, ScreenStorage};
use bezel_core::{BezelError, Result};

use crate::driver::{Sent, send_in_chunks};

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
    /// The simulated storage, shared by every screen of the connector.
    pub storage: FakeStorage,
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
    /// `set_start_mode`.
    StartMode(StartMode),
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
    /// The last start mode set, if any.
    pub start_mode: Option<StartMode>,
    /// Every storage call, in order.
    pub calls: Vec<StorageCall>,
}

impl Default for FakeStorage {
    fn default() -> Self {
        Self {
            internal_total: FAKE_FLASH_BYTES,
            card_total: None,
            files: BTreeMap::new(),
            playback: Playback::Idle,
            start_mode: None,
            calls: Vec::new(),
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
        self.files.insert(path.clone(), Vec::with_capacity(bytes));
        Ok(())
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

/// Connects to an in-memory screen that records everything.
#[derive(Debug, Clone, Default)]
pub struct FakeConnector {
    log: Arc<Mutex<FakeLog>>,
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
        }
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
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        let model = screen
            .model()
            .ok_or_else(|| BezelError::Transport("simulated screen needs a single model".into()))?;
        Ok(Box::new(FakeScreen {
            identity: ScreenIdentity {
                model,
                firmware: Some("simulated".into()),
            },
            orientation: Orientation::Portrait,
            has_storage: matches!(model.family, Family::TuringRevC | Family::TuringUsb),
            log: Arc::clone(&self.log),
        }))
    }
}

/// An in-memory screen that checks frame sizes like a real one.
#[derive(Debug)]
pub struct FakeScreen {
    identity: ScreenIdentity,
    orientation: Orientation,
    has_storage: bool,
    log: Arc<Mutex<FakeLog>>,
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
        self.record(|l| l.brightness.push(brightness));
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
        self.record(|l| l.frames.push(frame.clone()));
        Ok(())
    }

    fn screen_off(&mut self) -> Result<()> {
        self.record(|l| l.offs += 1);
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
        Ok(self.store(StorageCall::Size(path.clone()), |s| s.size(path)))
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
            Sent::All => Ok(()),
            Sent::Cancelled { .. } => Err(BezelError::Cancelled {
                partial: self.with_storage(|s| s.after_cancel(path)),
            }),
        }
    }

    fn delete(&mut self, path: &RemotePath, _confirmed: Confirmed) -> Result<()> {
        self.store(StorageCall::Delete(path.clone()), |s| s.files.remove(path));
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

    fn set_start_mode(&mut self, mode: StartMode, _confirmed: Confirmed) -> Result<()> {
        self.store(StorageCall::StartMode(mode), |s| s.start_mode = Some(mode));
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
        storage
            .set_start_mode(StartMode::Video, confirmed())
            .unwrap();
        storage.delete(&video, confirmed()).unwrap();
        storage.delete(&video, confirmed()).unwrap();

        let log = connector.log().storage;
        assert_eq!(log.files[&clip], data);
        assert!(!log.files.contains_key(&video));
        assert_eq!(
            (log.playback, log.start_mode),
            (Playback::Idle, Some(StartMode::Video))
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
                &StorageCall::StartMode(StartMode::Video),
                &StorageCall::Delete(video.clone()),
                &StorageCall::Delete(video),
            ]
        );
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
