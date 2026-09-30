//! Driver for Turing/TURZX USB screens (VID 0x1CBE, the vendor app's "207" family).
//!
//! Every command is one USB write (512-byte encrypted packet plus an optional
//! payload) followed by a 512-byte reply and a flush of stale input. Frames
//! are whole images: the canvas is rotated to the panel's native orientation
//! and sent as a PNG, or as a JPEG when the PNG exceeds 1 MiB. There is no
//! partial update on this transport.
//!
//! Only sync, brightness, frames and stop-stream are sent automatically;
//! reboot, rotation, persistent settings, storage and firmware commands exist
//! only as encoders (D-2026-09-30-device-protocols-2).

use std::time::Duration;

use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};
use chrono::Timelike;

use crate::driver::{Pause, check_frame, io_err};
use crate::protocol::turing_usb::{self as proto, Header, op};
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
pub struct TuringUsb<W: Wire, C: Clock = LocalClock> {
    wire: W,
    clock: C,
    identity: ScreenIdentity,
    orientation: Orientation,
    /// Last brightness level sent (0..=102).
    level: Option<u8>,
    /// Turned off by `screen_off`; the next frame turns it back on.
    off: bool,
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

impl<W: Wire> TuringUsb<W> {
    /// Syncs over `wire` using the local clock. `candidates` are the models
    /// discovery allowed; the USB product id names exactly one.
    pub fn connect<P: Pause>(
        wire: W,
        pause: &P,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        Self::connect_with_clock(wire, LocalClock, pause, candidates)
    }
}

impl<W: Wire, C: Clock> TuringUsb<W, C> {
    /// [`TuringUsb::connect`] with an explicit clock.
    pub fn connect_with_clock<P: Pause>(
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
            identity: ScreenIdentity {
                model,
                firmware: (!version.is_empty()).then_some(version),
            },
            orientation: Orientation::Portrait,
            level: None,
            off: false,
        })
    }

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

    fn native(&self, frame: &Frame) -> Result<Frame> {
        let model = self.identity.model;
        check_frame(model, self.orientation, frame)?;
        let turns = self.orientation.quarter_turns_to(model.native_orientation);
        Ok(frame.rotated(turns))
    }
}

impl<W: Wire, C: Clock> ScreenLink for TuringUsb<W, C> {
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
        let model = self.identity.model;
        let size = model.panel.in_orientation(model.native_orientation);
        let clear = Frame::filled(size, Rgba::default());
        self.send_image(&clear)?;
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
    use super::*;
    use crate::driver::RealTime;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::geometry::Size;
    use cbc::cipher::{Block, BlockModeDecrypt, KeyIvInit};

    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

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

    fn connected(id: &'static str) -> TuringUsb<ScriptedWire, FixedClock> {
        let wire = ScriptedWire::with_replies([sync_reply("TURZX_1_123")]);
        TuringUsb::connect_with_clock(wire, FixedClock(TS), &NoPause, &[model(id)]).unwrap()
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
        ];
        for code in opcodes(s.wire()) {
            assert!(!forbidden.contains(&code), "sent {code}");
        }
    }

    #[test]
    fn local_clock_is_within_a_day() {
        assert!(LocalClock.millis_since_midnight() < 86_400_000);
        RealTime.pause(Duration::ZERO);
    }
}
