//! Driver for Turing/TURZX USB screens (VID 0x1CBE, the vendor app's "207" family).
//!
//! Every command is one USB write (512-byte encrypted packet plus an optional
//! payload) followed by a 512-byte reply and a flush of stale input. Frames
//! are whole images: the canvas is rotated to the panel's native orientation
//! and sent as a PNG, or as a JPEG when the PNG exceeds 1 MiB. There is no
//! partial update on this transport.
//!
//! Only sync, brightness, frames and stop-stream are sent automatically;
//! reboot, rotation, persistent settings and firmware commands exist only as
//! encoders (D-2026-09-30-device-protocols-2).
//!
//! Storage (spec § 6) is golden-only (D-2026-09-30-storage-video-1: no
//! capture of a real screen, `hardware_validated = false`) and goes out only
//! from the [`ScreenStorage`] methods, which the core's use cases call:
//! STORAGE_INFO (100), LIST_DIR (99), OPEN_FILE (38) + WRITE_CHUNK (39),
//! PLAY_VIDEO (110) and SHOW_IMAGE (113), with the vendor's stop-and-wait
//! (111, 112) and clear PNG (102) before them. WRITE_FILE (40) and FILE_SIZE
//! (98) are never sent (the references disagree on their meaning, spec § 3),
//! so this link knows the size of a stored file only when it wrote it;
//! deleting (42), the start mode (125) and the boot logo are not offered.

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::Job;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::domain::standby::PlanB;
use bezel_core::domain::storage::{
    Confirmed, FileName, Medium, RemotePath, Repeat, StorageInfo, StorageLocation,
};
use bezel_core::ports::{ScreenLink, ScreenStorage};
use bezel_core::{BezelError, Result};
use chrono::Timelike;

use crate::driver::{
    Pause, RealTime, Sent, StorageRoots, check_frame, io_err, parse_listing, send_in_chunks,
    upload_size,
};
use crate::protocol::turing_usb::{self as proto, Header, op, root};
use crate::usb::Endpoints;
use crate::wire::Wire;

/// Interface and endpoints the vendor app uses (bulk OUT 0x01, IN 0x81).
pub const ENDPOINTS: Endpoints = Endpoints {
    interface: 0,
    out: proto::EP_OUT,
    input: proto::EP_IN,
};

/// How long a reply may take (vendor app and Python reference: 2 s).
const REPLY_TIMEOUT: Duration = Duration::from_millis(2000);
/// Sync attempts before giving up (the vendor app tries twice).
const SYNC_TRIES: usize = 2;
/// Pause between sync attempts.
const SYNC_RETRY_PAUSE: Duration = Duration::from_millis(200);
/// Level restored after [`ScreenLink::screen_off`] when no brightness was set:
/// the vendor app's default slider value 170 of 255, divided by 2.5 as it does.
const DEFAULT_LEVEL: u8 = 68;
/// LIST_DIR sends per listing: the vendor app sends it 20 times and reads the
/// 20 replies as one text (spec § 6). A missing reply ends the listing early.
const LIST_REPLIES: usize = 20;
/// PLAYBACK_BUSY polls after STOP_PLAYBACK (spec § 6).
const STOP_POLLS: usize = 10;
/// Pause between two PLAYBACK_BUSY polls (spec § 6).
const STOP_POLL_PAUSE: Duration = Duration::from_millis(100);
/// File bytes per WRITE_CHUNK. The vendor app always sends a full 1 MiB
/// payload; the tail of a shorter last chunk is zero-filled here (the
/// vendor's holds stale data) and the header gives the real length.
const UPLOAD_CHUNK: usize = proto::MAX_IMAGE;

/// The storage roots of spec § 6.
const ROOTS: StorageRoots = StorageRoots {
    internal: root::INTERNAL,
    card: root::CARD,
};

/// Milliseconds since local midnight, stamped into every header. Tests
/// inject a fixed clock.
pub trait Clock: Send {
    /// Milliseconds since local midnight.
    fn millis_since_midnight(&self) -> u32;
}

/// The host's local wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalClock;

impl Clock for LocalClock {
    fn millis_since_midnight(&self) -> u32 {
        let now = chrono::Local::now();
        // A leap second shows as nanosecond >= 1e9; keep it inside the second.
        now.num_seconds_from_midnight() * 1000 + (now.nanosecond() / 1_000_000).min(999)
    }
}

/// A connected Turing USB screen.
pub struct TuringUsb<W: Wire, C: Clock = LocalClock, P: Pause = RealTime> {
    wire: W,
    clock: C,
    pause: P,
    identity: ScreenIdentity,
    orientation: Orientation,
    /// Last brightness level sent (0..=102).
    level: Option<u8>,
    /// Turned off by `screen_off`; the next frame turns it back on.
    off: bool,
    /// Bytes this link wrote to each stored file and the screen acknowledged:
    /// the only file sizes it knows, since FILE_SIZE (98) is never sent.
    written: BTreeMap<RemotePath, u64>,
}

/// One command: a single write of the packet and its payload, then the
/// 512-byte reply (empty when none came) and a flush of anything after it.
fn exchange<W: Wire>(wire: &mut W, header: &Header, payload: &[u8]) -> Result<Vec<u8>> {
    wire.send(&proto::packet_and_payload(header, payload))
        .map_err(io_err)?;
    let reply = wire
        .receive(proto::PACKET_LEN, REPLY_TIMEOUT)
        .map_err(io_err)?;
    wire.discard_input().map_err(io_err)?;
    Ok(reply)
}

impl<W: Wire, P: Pause + Clone> TuringUsb<W, LocalClock, P> {
    /// Syncs over `wire` using the local clock. `candidates` are the models
    /// discovery allowed; the USB product id names exactly one.
    pub fn connect(wire: W, pause: &P, candidates: &[&'static DeviceModel]) -> Result<Self> {
        Self::connect_with_clock(wire, LocalClock, pause, candidates)
    }
}

impl<W: Wire, C: Clock, P: Pause + Clone> TuringUsb<W, C, P> {
    /// [`TuringUsb::connect`] with an explicit clock.
    pub fn connect_with_clock(
        mut wire: W,
        clock: C,
        pause: &P,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        let model = pick_model(candidates)?;
        wire.discard_input().map_err(io_err)?;
        let version = sync(&mut wire, &clock, pause)?;
        tracing::debug!(model = %model.id, version = %version, "Turing USB sync");
        Ok(Self {
            wire,
            clock,
            pause: pause.clone(),
            identity: ScreenIdentity {
                model,
                firmware: (!version.is_empty()).then_some(version),
            },
            orientation: Orientation::Portrait,
            level: None,
            off: false,
            written: BTreeMap::new(),
        })
    }
}

impl<W: Wire, C: Clock, P: Pause> TuringUsb<W, C, P> {
    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    fn now(&self) -> u32 {
        self.clock.millis_since_midnight()
    }

    fn command(&mut self, header: &Header, payload: &[u8]) -> Result<Vec<u8>> {
        exchange(&mut self.wire, header, payload)
    }

    fn send_level(&mut self, level: u8) -> Result<()> {
        let h = proto::set_brightness(self.now(), level);
        self.command(&h, &[])?;
        Ok(())
    }

    /// Sends a native-orientation RGBA frame as PNG (JPEG above 1 MiB).
    fn send_image(&mut self, native: &Frame) -> Result<()> {
        let size = native.size();
        let image =
            proto::encode_frame(native.as_rgba(), size.width, size.height, proto::MAX_IMAGE)
                .map_err(|e| BezelError::Transport(e.to_string()))?;
        let len = image.bytes().len();
        let h = proto::show_image_data(self.now(), image.opcode(), len as u32);
        let reply = self.command(&h, image.bytes())?;
        tracing::trace!(
            opcode = image.opcode(),
            bytes = len,
            ok = proto::resp_ok(&reply),
            "frame"
        );
        Ok(())
    }

    /// A fully transparent PNG at the panel's native size: the vendor app's
    /// "clear" (spec § 5.2, § 6).
    fn clear(&mut self) -> Result<()> {
        let model = self.identity.model;
        let size = model.panel.in_orientation(model.native_orientation);
        self.send_image(&Frame::filled(size, Rgba::default()))
    }

    fn native(&self, frame: &Frame) -> Result<Frame> {
        let model = self.identity.model;
        check_frame(model, self.orientation, frame)?;
        let turns = self.orientation.quarter_turns_to(model.native_orientation);
        Ok(frame.rotated(turns))
    }

    /// A command naming `target` on the device.
    fn path_header(&self, cmd: u8, target: &str) -> Result<Header> {
        proto::path_command(cmd, self.now(), target).ok_or_else(|| {
            BezelError::InvalidInput(format!("{target}: not a path a command can carry"))
        })
    }

    /// Sends `header` and requires the vendor's success status in the reply
    /// (`what` names the request in errors).
    fn expect_ok(&mut self, header: &Header, what: &str) -> Result<()> {
        let reply = self.command(header, &[])?;
        if proto::accepted(header[0], &reply) {
            return Ok(());
        }
        tracing::debug!(what, bytes = reply.len(), "command not accepted");
        if reply.is_empty() {
            return Err(no_answer(what));
        }
        Err(BezelError::Transport(format!("the screen refused {what}")))
    }

    /// STOP_PLAYBACK, then PLAYBACK_BUSY until the screen reports idle, as the
    /// vendor app does before playing (spec § 6).
    fn stop_playback(&mut self) -> Result<()> {
        let stop = proto::simple(op::STOP_PLAYBACK, self.now());
        self.command(&stop, &[])?;
        for poll in 1..=STOP_POLLS {
            let busy = proto::simple(op::PLAYBACK_BUSY, self.now());
            if proto::playback_idle(&self.command(&busy, &[])?) {
                return Ok(());
            }
            if poll < STOP_POLLS {
                self.pause.pause(STOP_POLL_PAUSE);
            }
        }
        // The vendor app goes on after its polls too.
        tracing::debug!("playback still busy after STOP_PLAYBACK; continuing");
        Ok(())
    }

    /// LIST_DIR `folder` [`LIST_REPLIES`] times, the replies read as one
    /// text (spec § 6; whether each reply carries header bytes is unknown).
    fn list_folder(&mut self, folder: &str) -> Result<Vec<FileName>> {
        let mut text = Vec::new();
        for _ in 0..LIST_REPLIES {
            let header = self.path_header(op::LIST_DIR, folder)?;
            let reply = self.command(&header, &[])?;
            if reply.is_empty() {
                break;
            }
            text.extend_from_slice(&reply);
        }
        parse_listing(&text).ok_or_else(|| no_answer(&format!("LIST_DIR {folder}")))
    }

    /// The data phase: WRITE_CHUNK per 1 MiB, `[16]` = 1 on the last one,
    /// each acknowledged by a reply before the next (spec § 6).
    fn send_chunks(&mut self, target: &str, data: &[u8], job: &mut Job<'_>) -> Result<Sent> {
        let (wire, clock) = (&mut self.wire, &self.clock);
        let mut payload = vec![0u8; UPLOAD_CHUNK];
        let mut offset = 0;
        send_in_chunks(data, UPLOAD_CHUNK, job, |chunk| {
            offset += chunk.len();
            let last = offset == data.len();
            let header =
                proto::write_chunk(clock.millis_since_midnight(), chunk.len() as u32, last);
            payload[..chunk.len()].copy_from_slice(chunk);
            payload[chunk.len()..].fill(0);
            if exchange(wire, &header, &payload)?.is_empty() {
                return Err(no_answer(&format!("WRITE_CHUNK {target}")));
            }
            Ok(())
        })
    }

    /// After a cancelled upload: SYNC (the vendor's reconnect) checks the
    /// link, then a size query finds what is left. Never deletes.
    fn recover_after_cancel(&mut self, path: &RemotePath) -> BezelError {
        if let Err(e) = sync(&mut self.wire, &self.clock, &self.pause) {
            tracing::warn!(error = %e, %path, "no sync answer after a cancelled upload");
            return BezelError::Timeout(format!(
                "the screen after a cancelled upload; reconnect it and check {path}"
            ));
        }
        match self.size(path) {
            Ok(partial) => BezelError::Cancelled { partial },
            Err(e) => e,
        }
    }
}

impl<W: Wire, C: Clock, P: Pause> ScreenLink for TuringUsb<W, C, P> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        let level = proto::brightness_level(brightness.percent());
        self.send_level(level)?;
        self.level = Some(level);
        self.off = false;
        Ok(())
    }

    /// The host rotates every frame; the device-side rotation (13) is persistent
    /// and never sent implicitly.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.orientation = orientation;
        Ok(())
    }

    /// Frames keep their alpha (PNG): over a video the screen plays, A = 0
    /// shows the video (spec § 5.2).
    fn present(&mut self, frame: &Frame) -> Result<()> {
        let native = self.native(frame)?;
        self.send_image(&native)?;
        if self.off {
            let level = self.level.unwrap_or(DEFAULT_LEVEL);
            self.send_level(level)?;
            self.off = false;
        }
        Ok(())
    }

    /// No on/off command is known: like the Python reference, clear the panel
    /// with a transparent PNG and set brightness 0 (the vendor app's shutdown
    /// also ends with brightness 0).
    fn screen_off(&mut self) -> Result<()> {
        self.clear()?;
        self.send_level(0)?;
        self.off = true;
        Ok(())
    }

    /// Stop-stream (123), which the vendor app sends whenever its theme loop
    /// ends; the device keeps showing what it has.
    fn release(&mut self) -> Result<()> {
        let h = proto::simple(op::STOP_STREAM, self.now());
        self.command(&h, &[])?;
        Ok(())
    }

    fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
        Some(self)
    }
}

impl<W: Wire, C: Clock, P: Pause> ScreenStorage for TuringUsb<W, C, P> {
    /// STORAGE_INFO: six LE32 KiB values; the reply must echo 100.
    fn info(&mut self) -> Result<StorageInfo> {
        let h = proto::simple(op::STORAGE_INFO, self.now());
        let reply = self.command(&h, &[])?;
        let report =
            proto::StorageInfo::from_reply(&reply).ok_or_else(|| no_answer("STORAGE_INFO"))?;
        tracing::debug!(?report, "STORAGE_INFO");
        Ok(report.info())
    }

    fn list(&mut self, location: StorageLocation) -> Result<Vec<FileName>> {
        self.list_folder(&ROOTS.folder(location))
    }

    /// Without FILE_SIZE (98): LIST_DIR of the file's folder decides whether
    /// it is stored (a card folder only when STORAGE_INFO reports a card, so
    /// that nothing is created on a missing card), and the size is the one
    /// this link wrote. A stored file this link did not write is
    /// `Unsupported`: its size cannot be read.
    fn size(&mut self, path: &RemotePath) -> Result<Option<u64>> {
        if path.location.medium == Medium::Card && self.info()?.card.is_none() {
            return Ok(None);
        }
        if !self.list(path.location)?.contains(&path.name) {
            return Ok(None);
        }
        match self.written.get(path) {
            Some(&bytes) => Ok((bytes > 0).then_some(bytes)),
            None => Err(BezelError::Unsupported(format!(
                "{path} is on the screen, but a Turing USB screen cannot report its size \
                 (command 98 is never sent)"
            ))),
        }
    }

    /// Stop and wait, OPEN_FILE (its reply must carry 0xC8), then WRITE_CHUNK
    /// per 1 MiB (spec § 6). Progress and cancellation per chunk; the use
    /// case verifies with [`ScreenStorage::size`].
    fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()> {
        let size = upload_size(data)?;
        let target = ROOTS.path(path)?;
        let open = self.path_header(op::OPEN_FILE, &target)?;
        job.checkpoint()?;
        self.stop_playback()?;
        job.checkpoint()?;
        self.written.remove(path);
        self.expect_ok(&open, &format!("OPEN_FILE {target}"))?;
        tracing::info!(%target, size, "upload");
        match self.send_chunks(&target, data, job)? {
            Sent::All => {
                self.written.insert(path.clone(), u64::from(size));
                Ok(())
            }
            Sent::Cancelled { accepted } => {
                tracing::info!(%target, accepted, "upload cancelled");
                self.written.insert(path.clone(), accepted);
                Err(self.recover_after_cancel(path))
            }
        }
    }

    /// Not offered: DELETE_FILE (42) is outside the golden-only command set
    /// of D-2026-09-30-storage-video-1. Nothing is sent.
    fn delete(&mut self, path: &RemotePath, _confirmed: Confirmed) -> Result<()> {
        Err(BezelError::Unsupported(format!(
            "deleting {path}: a Turing USB screen's files cannot be deleted by Bezel yet"
        )))
    }

    /// Stop and wait, clear, then PLAY_VIDEO (its reply must carry 0xC8).
    /// The firmware always loops: [`Repeat::Once`] is `Unsupported` and
    /// nothing is sent.
    fn play_video(&mut self, path: &RemotePath, repeat: Repeat) -> Result<()> {
        if repeat == Repeat::Once {
            return Err(BezelError::Unsupported(format!(
                "{path}: a Turing USB screen always loops a stored video"
            )));
        }
        let target = ROOTS.path(path)?;
        let play = self.path_header(op::PLAY_VIDEO, &target)?;
        self.stop_playback()?;
        self.clear()?;
        self.expect_ok(&play, &format!("PLAY_VIDEO {target}"))
    }

    /// Stop and wait, clear, then SHOW_IMAGE (0xC8 at `[1]` of its reply).
    fn play_image(&mut self, path: &RemotePath) -> Result<()> {
        let target = ROOTS.path(path)?;
        let show = self.path_header(op::SHOW_IMAGE, &target)?;
        self.stop_playback()?;
        self.clear()?;
        self.expect_ok(&show, &format!("SHOW_IMAGE {target}"))
    }

    fn stop(&mut self) -> Result<()> {
        self.stop_playback()
    }

    /// Not offered: the start mode lives in SAVE_SETTINGS (125, persistent,
    /// never sent) and the boot logo is golden-only
    /// (D-2026-09-30-storage-video-5); the plan B is rev C's
    /// (D-2026-10-03-power-off-standby-2). Nothing is sent.
    fn set_options(&mut self, _plan: PlanB, _confirmed: Confirmed) -> Result<()> {
        Err(BezelError::Unsupported(
            "a Turing USB screen's boot media cannot be set by Bezel yet".into(),
        ))
    }
}

/// No usable reply to `what`.
fn no_answer(what: &str) -> BezelError {
    BezelError::Timeout(format!("the screen: no valid answer to {what}"))
}

/// SYNC until the reply echoes the command id; returns the version string.
fn sync<W: Wire, C: Clock, P: Pause>(wire: &mut W, clock: &C, pause: &P) -> Result<String> {
    for attempt in 0..SYNC_TRIES {
        if attempt > 0 {
            pause.pause(SYNC_RETRY_PAUSE);
        }
        let reply = exchange(wire, &proto::sync(clock.millis_since_midnight()), &[])?;
        if let Some(version) = proto::sync_version(&reply) {
            return Ok(version);
        }
        tracing::debug!(attempt, bytes = reply.len(), "no sync answer");
    }
    Err(BezelError::Timeout("the screen did not answer sync".into()))
}

/// The model: the USB product id maps to exactly one catalog model.
fn pick_model(candidates: &[&'static DeviceModel]) -> Result<&'static DeviceModel> {
    let models: Vec<&'static DeviceModel> = candidates
        .iter()
        .copied()
        .filter(|m| m.family == Family::TuringUsb)
        .collect();
    match models.as_slice() {
        [only] => Ok(only),
        _ => Err(BezelError::Transport(format!(
            "expected one Turing USB model, got {}",
            models.len()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use super::*;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::{MODELS, model_by_id};
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::job::{CancelToken, Progress};
    use bezel_core::domain::screen::Confirm;
    use bezel_core::domain::storage::{BootMedia, Capacity, Operation, StartMode};
    use cbc::cipher::{Block, BlockModeDecrypt, KeyIvInit};

    #[derive(Clone)]
    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

    /// Records every pause.
    #[derive(Clone, Default)]
    struct Pauses(Arc<Mutex<Vec<Duration>>>);
    impl Pause for Pauses {
        fn pause(&self, d: Duration) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(d);
        }
    }
    impl Pauses {
        fn take(&self) -> Vec<Duration> {
            std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
        }
    }

    type Screen<P = NoPause> = TuringUsb<ScriptedWire, FixedClock, P>;

    struct FixedClock(u32);
    impl Clock for FixedClock {
        fn millis_since_midnight(&self) -> u32 {
            self.0
        }
    }

    const TS: u32 = 0x0102_0304;
    const RED: Rgba = Rgba::opaque(255, 0, 0);

    fn model(id: &'static str) -> &'static DeviceModel {
        model_by_id(ModelId(id)).unwrap()
    }

    fn sync_reply(version: &str) -> Vec<u8> {
        let mut r = vec![0u8; proto::PACKET_LEN];
        r[0] = op::SYNC;
        r[8..8 + version.len()].copy_from_slice(version.as_bytes());
        r
    }

    fn connected_with<P: Pause + Clone>(pause: &P, id: &'static str) -> Screen<P> {
        let wire = ScriptedWire::with_replies([sync_reply("TURZX_1_123")]);
        TuringUsb::connect_with_clock(wire, FixedClock(TS), pause, &[model(id)]).unwrap()
    }

    fn connected(id: &'static str) -> Screen {
        connected_with(&NoPause, id)
    }

    /// The plaintext header of a sent write.
    fn plain(write: &[u8]) -> Header {
        let mut ct = [0u8; proto::CIPHERTEXT_LEN];
        ct.copy_from_slice(&write[..proto::CIPHERTEXT_LEN]);
        let (blocks, _) = Block::<cbc::Decryptor<des::Des>>::slice_as_chunks_mut(&mut ct);
        cbc::Decryptor::<des::Des>::new(&proto::KEY.into(), &proto::KEY.into())
            .decrypt_blocks(blocks);
        assert_eq!(&ct[proto::HEADER_LEN..], &[4, 4, 4, 4], "PKCS#7 padding");
        assert_eq!(&write[504..512], &[0, 0, 0, 0, 0, 0, 0xA1, 0x1A]);
        let mut h = [0u8; proto::HEADER_LEN];
        h.copy_from_slice(&ct[..proto::HEADER_LEN]);
        h
    }

    fn opcodes(wire: &ScriptedWire) -> Vec<u8> {
        wire.sent.iter().map(|w| plain(w)[0]).collect()
    }

    fn decode_png(bytes: &[u8]) -> (Size, Vec<u8>) {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes.to_vec()));
        let mut reader = decoder.read_info().unwrap();
        let mut out = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut out).unwrap();
        (Size::new(info.width, info.height), out)
    }

    #[test]
    fn connect_syncs_once_with_the_clock_timestamp() {
        let s = connected("turing-usb-8.8");
        assert_eq!(s.wire().sent.len(), 1);
        let h = plain(&s.wire().sent[0]);
        assert_eq!(&h[..8], &[10, 0, 0x1a, 0x6d, 4, 3, 2, 1]);
        assert_eq!(s.wire().sent[0].len(), proto::PACKET_LEN);
        assert_eq!(s.identity().model.id, ModelId("turing-usb-8.8"));
        assert_eq!(s.identity().firmware.as_deref(), Some("TURZX_1_123"));
        // One discard before sync, one after its reply.
        assert_eq!(s.wire().discards, 2);
    }

    #[test]
    fn sync_is_retried_once_then_times_out() {
        let wire = ScriptedWire::with_replies([vec![0u8; 512], sync_reply("")]);
        let s = TuringUsb::connect_with_clock(
            wire,
            FixedClock(1),
            &NoPause,
            &[model("turing-usb-5.2")],
        )
        .unwrap();
        assert_eq!(opcodes(s.wire()), vec![op::SYNC, op::SYNC]);
        assert_eq!(s.identity().firmware, None, "empty version string");

        let err = TuringUsb::connect_with_clock(
            ScriptedWire::default(),
            FixedClock(1),
            &NoPause,
            &[model("turing-usb-5.2")],
        )
        .err()
        .unwrap();
        assert!(matches!(err, BezelError::Timeout(_)));
    }

    #[test]
    fn the_model_comes_from_the_single_candidate() {
        for candidates in [
            vec![],
            vec![model("turing-usb-8"), model("turing-usb-8.8")],
            vec![model("turing-8.8")],
        ] {
            let wire = ScriptedWire::with_replies([sync_reply("x")]);
            let err = TuringUsb::connect_with_clock(wire, FixedClock(0), &NoPause, &candidates)
                .err()
                .unwrap();
            assert!(matches!(err, BezelError::Transport(_)));
        }
        let mixed = [model("turing-8.8"), model("turing-usb-9.2")];
        assert_eq!(pick_model(&mixed).unwrap().id, ModelId("turing-usb-9.2"));
    }

    #[test]
    fn portrait_frames_are_turned_180_and_sent_as_png() {
        let mut s = connected("turing-usb-8.8");
        let panel = s.identity().model.panel;
        let mut frame = Frame::filled(panel, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), RED);
        s.present(&frame).unwrap();
        let write = s.wire().sent.last().unwrap();
        let h = plain(write);
        assert_eq!(h[0], op::SHOW_PNG);
        let png = &write[proto::PACKET_LEN..];
        assert_eq!(&h[8..12], &(png.len() as u32).to_be_bytes());
        assert!(png.len() <= proto::MAX_IMAGE);
        let (size, rgba) = decode_png(png);
        assert_eq!(size, Size::new(480, 1920));
        // Portrait on a reverse-portrait panel: the red top-left pixel is the
        // last native pixel.
        assert_eq!(&rgba[rgba.len() - 4..], &[255, 0, 0, 255]);
        assert_eq!(&rgba[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn landscape_frames_are_turned_a_quarter_clockwise() {
        let mut s = connected("turing-usb-4.6");
        s.set_orientation(Orientation::Landscape).unwrap();
        let size = s
            .identity()
            .model
            .panel
            .in_orientation(Orientation::Landscape);
        assert_eq!(size, Size::new(960, 320));
        let mut frame = Frame::filled(size, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), RED);
        s.present(&frame).unwrap();
        let (native, rgba) = decode_png(&s.wire().sent.last().unwrap()[proto::PACKET_LEN..]);
        assert_eq!(native, Size::new(320, 960));
        // Landscape -> reverse portrait is one clockwise quarter turn: the
        // top-left pixel ends up at the top-right corner.
        let top_right = (320 - 1) * 4;
        assert_eq!(&rgba[top_right..top_right + 4], &[255, 0, 0, 255]);

        let wrong = Frame::filled(Size::new(320, 960), Rgba::BLACK);
        assert!(s.present(&wrong).is_err());
    }

    #[test]
    fn frames_above_one_mib_go_out_as_jpeg() {
        let mut s = connected("turing-usb-8");
        let panel = s.identity().model.panel;
        // Low-amplitude noise: incompressible enough to push the PNG past
        // 1 MiB, smooth enough for a JPEG to fit.
        let mut state = 0x9e37_79b9u32;
        let rgba: Vec<u8> = (0..panel.area() * 4)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if i % 4 == 3 {
                    255
                } else {
                    124 + (state & 7) as u8
                }
            })
            .collect();
        let frame = Frame::from_rgba(panel, rgba).unwrap();
        s.present(&frame).unwrap();
        let write = s.wire().sent.last().unwrap();
        let h = plain(write);
        assert_eq!(h[0], op::SHOW_JPEG);
        let jpeg = &write[proto::PACKET_LEN..];
        assert_eq!(&h[8..12], &(jpeg.len() as u32).to_be_bytes());
        assert!(jpeg.len() <= proto::MAX_IMAGE);
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn brightness_screen_off_and_release() {
        let mut s = connected("turing-usb-2.8-round");
        s.set_brightness(Brightness::new(50).unwrap()).unwrap();
        let h = plain(s.wire().sent.last().unwrap());
        assert_eq!(&h[..9], &[14, 0, 0x1a, 0x6d, 4, 3, 2, 1, 51]);

        s.screen_off().unwrap();
        let n = s.wire().sent.len();
        let clear = &s.wire().sent[n - 2];
        assert_eq!(plain(clear)[0], op::SHOW_PNG);
        let (size, rgba) = decode_png(&clear[proto::PACKET_LEN..]);
        assert_eq!(size, Size::new(480, 480));
        assert!(rgba.iter().all(|&b| b == 0), "fully transparent");
        assert_eq!(plain(&s.wire().sent[n - 1])[8], 0, "brightness 0");

        // The next frame turns the backlight back to the last level.
        let frame = Frame::filled(Size::new(480, 480), Rgba::WHITE);
        s.present(&frame).unwrap();
        let ops: Vec<u8> = opcodes(s.wire()).split_off(n);
        assert_eq!(ops, vec![op::SHOW_PNG, op::SET_BRIGHTNESS]);
        assert_eq!(plain(s.wire().sent.last().unwrap())[8], 51);
        s.present(&frame).unwrap();
        assert_eq!(plain(s.wire().sent.last().unwrap())[0], op::SHOW_PNG);

        s.release().unwrap();
        assert_eq!(plain(s.wire().sent.last().unwrap())[0], op::STOP_STREAM);
    }

    #[test]
    fn screen_off_without_a_known_level_restores_the_vendor_default() {
        let mut s = connected("turing-usb-12.3");
        s.screen_off().unwrap();
        let panel = s.identity().model.panel;
        s.present(&Frame::filled(panel, Rgba::BLACK)).unwrap();
        assert_eq!(plain(s.wire().sent.last().unwrap())[8], DEFAULT_LEVEL);
        // An explicit level while off wins, and the next frame leaves it alone.
        s.screen_off().unwrap();
        s.set_brightness(Brightness::new(10).unwrap()).unwrap();
        s.present(&Frame::filled(panel, Rgba::BLACK)).unwrap();
        assert_eq!(plain(s.wire().sent.last().unwrap())[0], op::SHOW_PNG);
    }

    #[test]
    fn nothing_disruptive_is_ever_sent_implicitly() {
        let mut s = connected("turing-usb-9.2");
        let panel = s.identity().model.panel;
        for o in Orientation::ALL {
            s.set_orientation(o).unwrap();
            s.present(&Frame::filled(panel.in_orientation(o), RED))
                .unwrap();
        }
        s.set_brightness(Brightness::MAX).unwrap();
        s.screen_off().unwrap();
        s.release().unwrap();
        let forbidden = [
            op::RESTART,
            op::SET_ROTATION,
            op::OPEN_FILE,
            op::WRITE_CHUNK,
            op::WRITE_FILE,
            op::DELETE_FILE,
            op::SAVE_SETTINGS,
            op::STORAGE_INFO,
            op::LIST_DIR,
            op::FILE_SIZE,
            op::PLAY_VIDEO,
            op::SHOW_IMAGE,
            op::STOP_PLAYBACK,
            op::PLAYBACK_BUSY,
        ];
        let sent = opcodes(s.wire());
        assert!(sent.len() > 6, "{sent:?}");
        for code in sent {
            assert!(!forbidden.contains(&code), "sent {code}");
        }
    }

    // Storage (spec § 6, golden-only).

    /// A 512-byte reply: `[0]` = `cmd`, `[at]` = `value`.
    fn answer(cmd: u8, at: usize, value: u8) -> Vec<u8> {
        let mut r = vec![0u8; proto::PACKET_LEN];
        r[0] = cmd;
        r[at] = value;
        r
    }

    fn ok(cmd: u8) -> Vec<u8> {
        answer(cmd, proto::STATUS_AT, proto::STATUS_OK)
    }

    /// STOP_PLAYBACK's reply, then a PLAYBACK_BUSY reply saying idle.
    fn stopped() -> [Vec<u8>; 2] {
        [
            answer(op::STOP_PLAYBACK, 8, 0),
            answer(op::PLAYBACK_BUSY, 8, 0),
        ]
    }

    /// STORAGE_INFO reply with the six KiB fields.
    fn storage_reply(kib: [u32; 6]) -> Vec<u8> {
        let mut r = answer(op::STORAGE_INFO, 0, op::STORAGE_INFO);
        for (i, v) in kib.iter().enumerate() {
            r[8 + 4 * i..12 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        r
    }

    /// The 20 replies of a listing: `text`, then NUL-filled replies.
    fn listing(text: &str) -> Vec<Vec<u8>> {
        let mut first = vec![0u8; proto::PACKET_LEN];
        first[..text.len()].copy_from_slice(text.as_bytes());
        let mut replies = vec![first];
        replies.resize(LIST_REPLIES, vec![0u8; proto::PACKET_LEN]);
        replies
    }

    fn script<P: Pause>(s: &mut Screen<P>, replies: impl IntoIterator<Item = Vec<u8>>) {
        for r in replies {
            s.wire.reply(&r);
        }
    }

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).unwrap()
    }

    /// Headers of the writes after the first `from`.
    fn headers_since<P: Pause>(s: &Screen<P>, from: usize) -> Vec<Header> {
        s.wire().sent[from..].iter().map(|w| plain(w)).collect()
    }

    fn ops_since<P: Pause>(s: &Screen<P>, from: usize) -> Vec<u8> {
        headers_since(s, from).iter().map(|h| h[0]).collect()
    }

    /// The path a path command carries.
    fn path_of(h: &Header) -> String {
        let len = u32::from_be_bytes([h[8], h[9], h[10], h[11]]) as usize;
        String::from_utf8(h[16..16 + len].to_vec()).unwrap()
    }

    fn confirmed() -> Confirmed {
        Confirmed::require(Confirm::Yes, &Operation::Boot(BootMedia::Default)).unwrap()
    }

    /// Runs an upload whose job cancels once `cancel_at` bytes were
    /// reported; returns the result and the `(done, total)` reports.
    fn run_upload<P: Pause>(
        s: &mut Screen<P>,
        target: &RemotePath,
        data: &[u8],
        cancel_at: Option<u64>,
    ) -> (Result<()>, Vec<(u64, u64)>) {
        let token = CancelToken::new();
        let remote = token.clone();
        let mut seen = Vec::new();
        let mut sink = |p: Progress| {
            seen.push((p.done, p.total));
            if cancel_at.is_some_and(|at| p.done >= at) {
                remote.cancel();
            }
        };
        let mut job = Job::new(&token, &mut sink);
        let result = s.upload(target, data, &mut job);
        (result, seen)
    }

    fn test_file(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn storage_queries_map_folders_and_parse_replies() {
        let mut s = connected("turing-usb-8.8");
        assert!(s.storage().is_some());
        let before = s.wire().sent.len();
        script(&mut s, [storage_reply([0, 0, 0, 262_144, 65_536, 196_608])]);
        let info = s.info().unwrap();
        assert_eq!(
            info.internal,
            Capacity {
                total: 262_144 * 1024,
                used: 65_536 * 1024,
                free: 196_608 * 1024,
            }
        );
        assert_eq!(info.card, None);
        assert_eq!(ops_since(&s, before), [op::STORAGE_INFO]);
        script(&mut s, [ok(op::SYNC)]);
        assert!(matches!(s.info(), Err(BezelError::Timeout(_))), "no echo");

        // A listing is 20 LIST_DIR of the folder, read as one text.
        let before = s.wire().sent.len();
        script(&mut s, listing("file:88.h264/clip.h264/"));
        let internal_video = path("internal/video/x").location;
        let names: Vec<String> = s
            .list(internal_video)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(names, ["88.h264", "clip.h264"]);
        let sent = headers_since(&s, before);
        assert_eq!(sent.len(), LIST_REPLIES);
        assert!(sent.iter().all(|h| h[0] == op::LIST_DIR));
        assert!(sent.iter().all(|h| path_of(h) == "/usr/data/video/"));

        // A missing reply ends it; the card roots are the § 6 ones.
        let before = s.wire().sent.len();
        s.wire.reply(b"nodir-createdone");
        assert!(s.list(path("sd/image/x").location).unwrap().is_empty());
        let sent = headers_since(&s, before);
        assert_eq!(sent.len(), 2);
        assert_eq!(path_of(&sent[0]), "/tmp/sdcard/mmcblk0p1/img/");
        let err = s.list(internal_video).unwrap_err();
        assert!(
            err.to_string().contains("LIST_DIR /usr/data/video/"),
            "{err}"
        );

        // Sizes: absent, on a missing card (nothing listed), unknown.
        let clip = path("internal/video/clip.h264");
        script(&mut s, listing("file:88.h264/"));
        assert_eq!(s.size(&clip).unwrap(), None);
        let before = s.wire().sent.len();
        script(&mut s, [storage_reply([0; 6])]);
        assert_eq!(s.size(&path("sd/video/clip.h264")).unwrap(), None);
        assert_eq!(ops_since(&s, before), [op::STORAGE_INFO]);
        let before = s.wire().sent.len();
        script(&mut s, [storage_reply([1024, 0, 1024, 0, 0, 0])]);
        script(&mut s, listing("file:clip.h264/"));
        let err = s.size(&path("sd/video/clip.h264")).unwrap_err();
        assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        assert!(err.to_string().contains("sd/video/clip.h264"), "{err}");
        let sent = headers_since(&s, before);
        assert_eq!(sent[0][0], op::STORAGE_INFO);
        assert_eq!(path_of(&sent[1]), "/tmp/sdcard/mmcblk0p1/video/");
    }

    #[test]
    fn upload_reports_progress_and_can_be_cancelled() {
        // A whole upload: stop and wait, OPEN_FILE, 1 MiB chunks.
        let mut s = connected("turing-usb-8.8");
        let data = test_file(UPLOAD_CHUNK * 2 + 1000);
        let total = data.len() as u64;
        let clip = path("internal/video/clip.h264");
        let before = s.wire().sent.len();
        script(&mut s, stopped());
        script(&mut s, [ok(op::OPEN_FILE)]);
        script(
            &mut s,
            [
                ok(op::WRITE_CHUNK),
                ok(op::WRITE_CHUNK),
                ok(op::WRITE_CHUNK),
            ],
        );
        let (result, progress) = run_upload(&mut s, &clip, &data, None);
        result.unwrap();
        let sent = headers_since(&s, before);
        let ops: Vec<u8> = sent.iter().map(|h| h[0]).collect();
        assert_eq!(
            ops,
            [
                op::STOP_PLAYBACK,
                op::PLAYBACK_BUSY,
                op::OPEN_FILE,
                op::WRITE_CHUNK,
                op::WRITE_CHUNK,
                op::WRITE_CHUNK
            ]
        );
        assert_eq!(path_of(&sent[2]), "/usr/data/video/clip.h264");
        let chunk = UPLOAD_CHUNK as u64;
        for (h, (len, last)) in sent[3..].iter().zip([(chunk, 0), (chunk, 0), (1000, 1)]) {
            assert_eq!(h[8..12], (proto::MAX_IMAGE as u32).to_be_bytes());
            assert_eq!(
                u64::from(u32::from_be_bytes([h[12], h[13], h[14], h[15]])),
                len
            );
            assert_eq!(h[16], last);
        }
        let writes = &s.wire().sent[before + 3..];
        assert!(
            writes
                .iter()
                .all(|w| w.len() == proto::PACKET_LEN + UPLOAD_CHUNK)
        );
        let payload: Vec<u8> = writes
            .iter()
            .flat_map(|w| w[proto::PACKET_LEN..].to_vec())
            .collect();
        assert_eq!(payload[..data.len()], data[..], "the file, in order");
        assert!(payload[data.len()..].iter().all(|&b| b == 0), "zero tail");
        assert_eq!(
            progress,
            [
                (0, total),
                (chunk, total),
                (2 * chunk, total),
                (total, total)
            ]
        );
        // The use case's check: listed, and this link knows what it wrote.
        script(&mut s, listing("file:clip.h264/"));
        assert_eq!(s.size(&clip).unwrap(), Some(total));

        // Cancelled after the first chunk: no more data, SYNC checks the
        // link, the size query reports what the screen acknowledged, and
        // nothing is deleted.
        let mut s = connected("turing-usb-8.8");
        let before = s.wire().sent.len();
        script(&mut s, stopped());
        script(&mut s, [ok(op::OPEN_FILE), ok(op::WRITE_CHUNK)]);
        script(&mut s, [sync_reply("TURZX_1_123")]);
        script(&mut s, listing("file:clip.h264/"));
        let (result, progress) = run_upload(&mut s, &clip, &data, Some(1));
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(chunk)
            })
        );
        assert_eq!(progress, [(0, total), (chunk, total)]);
        let ops = ops_since(&s, before);
        assert_eq!(
            ops[..6],
            [
                op::STOP_PLAYBACK,
                op::PLAYBACK_BUSY,
                op::OPEN_FILE,
                op::WRITE_CHUNK,
                op::SYNC,
                op::LIST_DIR
            ]
        );
        assert_eq!(ops.len(), 5 + LIST_REPLIES);
        assert!(!ops.contains(&op::DELETE_FILE));

        // Cancelled before the first chunk: the empty file reads as absent.
        let mut s = connected("turing-usb-8.8");
        script(&mut s, stopped());
        script(&mut s, [ok(op::OPEN_FILE), sync_reply("x")]);
        script(&mut s, listing("file:clip.h264/"));
        let (result, _) = run_upload(&mut s, &clip, &data, Some(0));
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));

        // Cancelled before anything: nothing is sent at all.
        let mut s = connected("turing-usb-8.8");
        let before = s.wire().sent.len();
        let token = CancelToken::new();
        token.cancel();
        let mut sink = |_: Progress| {};
        let mut job = Job::new(&token, &mut sink);
        let result = s.upload(&clip, &data, &mut job);
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        assert_eq!(s.wire().sent.len(), before);
    }

    #[test]
    fn upload_failures() {
        let clip = path("sd/image/logo.png");
        let data = test_file(1000);

        // The screen refuses OPEN_FILE: no data follows.
        let mut s = connected("turing-usb-8.8");
        let before = s.wire().sent.len();
        script(&mut s, stopped());
        script(&mut s, [answer(op::OPEN_FILE, 8, 0x01)]);
        let (result, _) = run_upload(&mut s, &clip, &data, None);
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Transport(_)), "{err}");
        assert!(
            err.to_string()
                .contains("OPEN_FILE /tmp/sdcard/mmcblk0p1/img/logo.png"),
            "{err}"
        );
        assert!(!ops_since(&s, before).contains(&op::WRITE_CHUNK));

        // No answer to OPEN_FILE, then an empty file.
        script(&mut s, stopped());
        let (result, _) = run_upload(&mut s, &clip, &data, None);
        assert!(matches!(result, Err(BezelError::Timeout(_))), "{result:?}");
        let (empty, _) = run_upload(&mut s, &clip, &[], None);
        assert!(matches!(empty, Err(BezelError::InvalidInput(_))));

        // A chunk without an answer fails the upload; its size is unknown.
        let mut s = connected("turing-usb-8.8");
        script(&mut s, stopped());
        script(&mut s, [ok(op::OPEN_FILE)]);
        let (result, progress) = run_upload(&mut s, &clip, &data, None);
        let err = result.unwrap_err();
        assert!(err.to_string().contains("WRITE_CHUNK"), "{err}");
        assert_eq!(progress, [(0, 1000)]);
        script(&mut s, [storage_reply([1024, 0, 1024, 0, 0, 0])]);
        script(&mut s, listing("file:logo.png/"));
        assert!(matches!(s.size(&clip), Err(BezelError::Unsupported(_))));

        // The screen does not answer SYNC after a cancel: reconnect.
        let mut s = connected("turing-usb-8.8");
        let before = s.wire().sent.len();
        script(&mut s, stopped());
        script(&mut s, [ok(op::OPEN_FILE)]);
        let (result, _) = run_upload(&mut s, &clip, &data, Some(0));
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert!(err.to_string().contains("sd/image/logo.png"), "{err}");
        let syncs = ops_since(&s, before)
            .into_iter()
            .filter(|o| *o == op::SYNC)
            .count();
        assert_eq!(syncs, SYNC_TRIES);
    }

    #[test]
    fn playback_stops_and_clears_first_and_checks_the_status() {
        let pauses = Pauses::default();
        let mut s = connected_with(&pauses, "turing-usb-8.8");
        let video = path("sd/video/88.h264");
        let before = s.wire().sent.len();
        script(&mut s, stopped());
        script(&mut s, [ok(op::SHOW_PNG), ok(op::PLAY_VIDEO)]);
        s.play_video(&video, Repeat::Loop).unwrap();
        let sent = headers_since(&s, before);
        let ops: Vec<u8> = sent.iter().map(|h| h[0]).collect();
        assert_eq!(
            ops,
            [
                op::STOP_PLAYBACK,
                op::PLAYBACK_BUSY,
                op::SHOW_PNG,
                op::PLAY_VIDEO
            ]
        );
        assert_eq!(path_of(&sent[3]), "/tmp/sdcard/mmcblk0p1/video/88.h264");
        let (size, rgba) = decode_png(&s.wire().sent[before + 2][proto::PACKET_LEN..]);
        assert_eq!(size, Size::new(480, 1920), "native panel size");
        assert!(rgba.iter().all(|&b| b == 0), "fully transparent");
        assert!(pauses.take().is_empty(), "idle at the first poll");

        // Refused or unanswered: an error naming the command.
        script(&mut s, stopped());
        script(&mut s, [ok(op::SHOW_PNG), answer(op::PLAY_VIDEO, 1, 0xC8)]);
        let err = s.play_video(&video, Repeat::Loop).unwrap_err();
        assert!(
            err.to_string()
                .contains("PLAY_VIDEO /tmp/sdcard/mmcblk0p1/video/88.h264"),
            "{err}"
        );
        // The firmware always loops: playing once is refused, nothing sent.
        let before = s.wire().sent.len();
        let err = s.play_video(&video, Repeat::Once).unwrap_err();
        assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        assert_eq!(s.wire().sent.len(), before);

        // SHOW_IMAGE reports its status at [1].
        let image = path("internal/image/logo.png");
        let before = s.wire().sent.len();
        script(&mut s, stopped());
        script(&mut s, [ok(op::SHOW_PNG), answer(op::SHOW_IMAGE, 1, 0xC8)]);
        s.play_image(&image).unwrap();
        let sent = headers_since(&s, before);
        assert_eq!(sent[3][0], op::SHOW_IMAGE);
        assert_eq!(path_of(&sent[3]), "/usr/data/img/logo.png");
        script(&mut s, stopped());
        script(&mut s, [ok(op::SHOW_PNG), ok(op::SHOW_IMAGE)]);
        assert!(matches!(
            s.play_image(&image),
            Err(BezelError::Transport(_))
        ));

        // Stop polls PLAYBACK_BUSY until idle, 100 ms apart...
        let before = s.wire().sent.len();
        script(&mut s, [answer(op::STOP_PLAYBACK, 8, 0)]);
        script(&mut s, [answer(op::PLAYBACK_BUSY, 8, 1), vec![]]);
        script(&mut s, [answer(op::PLAYBACK_BUSY, 8, 0)]);
        s.stop().unwrap();
        assert_eq!(
            ops_since(&s, before),
            [
                op::STOP_PLAYBACK,
                op::PLAYBACK_BUSY,
                op::PLAYBACK_BUSY,
                op::PLAYBACK_BUSY
            ]
        );
        assert_eq!(pauses.take(), [STOP_POLL_PAUSE; 2]);
        // ...at most 10 times, then goes on like the vendor app.
        let before = s.wire().sent.len();
        s.stop().unwrap();
        assert_eq!(ops_since(&s, before).len(), 1 + STOP_POLLS);
        assert_eq!(pauses.take(), [STOP_POLL_PAUSE; STOP_POLLS - 1]);
    }

    #[test]
    fn delete_and_boot_media_are_not_offered() {
        let mut s = connected("turing-usb-8.8");
        let before = s.wire().sent.len();
        let image = path("internal/image/logo.png");
        let err = s.delete(&image, confirmed()).unwrap_err();
        assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        assert!(err.to_string().contains("internal/image/logo.png"), "{err}");
        for mode in [StartMode::Default, StartMode::Image, StartMode::Video] {
            let err = s.set_options(PlanB::new(mode, 0), confirmed()).unwrap_err();
            assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        }
        let err = s.restart(confirmed()).unwrap_err();
        assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        assert_eq!(s.wire().sent.len(), before, "nothing sent");
    }

    #[test]
    fn storage_sends_only_the_decided_commands() {
        // D-2026-09-30-storage-video-1: 100/99/38/39/110/113, with the
        // vendor's stop-and-wait, clear and sync; never 40 or 98.
        let mut s = connected("turing-usb-5.2");
        script(&mut s, [storage_reply([1024, 0, 1024, 0, 0, 0])]);
        s.info().unwrap();
        script(&mut s, listing("file:"));
        s.list(path("sd/video/x").location).unwrap();
        script(&mut s, [storage_reply([1024, 0, 1024, 0, 0, 0])]);
        script(&mut s, listing("file:"));
        s.size(&path("sd/video/a.h264")).unwrap();
        script(&mut s, stopped());
        script(&mut s, [ok(op::OPEN_FILE), ok(op::WRITE_CHUNK)]);
        let (result, _) = run_upload(&mut s, &path("sd/video/a.h264"), &[1, 2, 3], None);
        result.unwrap();
        script(&mut s, stopped());
        script(&mut s, [ok(op::SHOW_PNG), ok(op::PLAY_VIDEO)]);
        s.play_video(&path("sd/video/a.h264"), Repeat::Loop)
            .unwrap();
        script(&mut s, stopped());
        script(&mut s, [ok(op::SHOW_PNG), answer(op::SHOW_IMAGE, 1, 0xC8)]);
        s.play_image(&path("internal/image/b.png")).unwrap();
        script(&mut s, stopped());
        s.stop().unwrap();
        let _ = s.delete(&path("sd/video/a.h264"), confirmed());
        let _ = s.set_options(PlanB::new(StartMode::Video, 0), confirmed());
        let _ = s.restart(confirmed());
        let allowed = [
            op::SYNC,
            op::STORAGE_INFO,
            op::LIST_DIR,
            op::OPEN_FILE,
            op::WRITE_CHUNK,
            op::PLAY_VIDEO,
            op::SHOW_IMAGE,
            op::STOP_PLAYBACK,
            op::PLAYBACK_BUSY,
            op::SHOW_PNG,
        ];
        let sent = opcodes(s.wire());
        for code in &sent {
            assert!(allowed.contains(code), "sent {code}");
        }
        for code in [
            op::STORAGE_INFO,
            op::LIST_DIR,
            op::OPEN_FILE,
            op::WRITE_CHUNK,
        ] {
            assert!(sent.contains(&code), "{code} used");
        }
        assert!(!sent.contains(&op::WRITE_FILE) && !sent.contains(&op::FILE_SIZE));
    }

    #[test]
    fn turing_usb_models_have_storage_but_are_not_hardware_validated() {
        let models: Vec<&DeviceModel> = MODELS
            .iter()
            .filter(|m| m.family == Family::TuringUsb)
            .collect();
        assert!(!models.is_empty());
        for m in models {
            assert!(!m.hardware_validated, "{}", m.id);
            assert!(m.capabilities.storage, "{}", m.id);
        }
    }

    #[test]
    fn local_clock_is_within_a_day() {
        assert!(LocalClock.millis_since_midnight() < 86_400_000);
        RealTime.pause(Duration::ZERO);
    }
}
