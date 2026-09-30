//! Driver for WCH-based panels (VID 0x43A8).
//!
//! Commands are 32-byte packets answered by a 32-byte reply; frames are full
//! BGR888 buffers in 512-byte blocks written as 4096-byte transfers, followed
//! by a 32-byte ACK. The host rotates every frame; the device rotation (0x56)
//! is never sent, because whether the firmware applies it too is unknown
//! (spec § 8) and it is persistent (D-2026-09-30-device-protocols-2).

use std::time::Duration;

use bezel_core::domain::device::{DeviceModel, Family, ModelId};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};

use crate::driver::{Pause, check_frame, io_err};
use crate::protocol::wch::{self as proto, Panel, Version, op};
use crate::usb::Endpoints;
use crate::wire::Wire;

/// Interface and endpoints the vendor app uses (bulk OUT 0x02, IN 0x82).
pub const ENDPOINTS: Endpoints = Endpoints {
    interface: 0,
    out: proto::EP_OUT,
    input: proto::EP_IN,
};

/// How long a reply or frame ACK may take.
const REPLY_TIMEOUT: Duration = Duration::from_millis(100);
/// Pause between a write and its reply.
const REPLY_DELAY: Duration = Duration::from_millis(1);
/// Pause before the one retry of a failed version query.
const INIT_RETRY_PAUSE: Duration = Duration::from_millis(200);
/// Pause after the ResetMem that starts streaming.
const START_RESET_PAUSE: Duration = Duration::from_millis(200);
/// Pause after a ResetMem requested by a frame ACK.
const ACK_RESET_PAUSE: Duration = Duration::from_millis(300);
/// Backlight restored after [`ScreenLink::screen_off`] when no level was set:
/// the vendor app's default slider value.
const DEFAULT_LEVEL: u8 = 170;

/// A connected WCH panel.
pub struct Wch<W: Wire, P: Pause> {
    wire: W,
    pause: P,
    identity: ScreenIdentity,
    orientation: Orientation,
    /// Last backlight level sent (0..=255).
    level: Option<u8>,
    /// Turned off by `screen_off`; the next frame turns it back on.
    off: bool,
}

/// One command: drop stale input, write the packet, wait 1 ms and read the
/// 32-byte reply. `None` when nothing came back; else `[2..10]` decrypted.
fn exchange<W: Wire, P: Pause>(
    wire: &mut W,
    pause: &P,
    cmd: u8,
    arg: u8,
) -> Result<Option<[u8; proto::PACKET_LEN]>> {
    wire.discard_input().map_err(io_err)?;
    wire.send(&proto::command(cmd, arg)).map_err(io_err)?;
    pause.pause(REPLY_DELAY);
    let reply = wire
        .receive(proto::PACKET_LEN, REPLY_TIMEOUT)
        .map_err(io_err)?;
    Ok((!reply.is_empty()).then(|| proto::decrypt_reply(&reply)))
}

/// The version query (0x40), retried once after 200 ms as the vendor app does.
fn query_version<W: Wire, P: Pause>(wire: &mut W, pause: &P) -> Result<Version> {
    for attempt in 0..2 {
        if attempt > 0 {
            pause.pause(INIT_RETRY_PAUSE);
        }
        let reply = exchange(wire, pause, op::GET_VERSION, 0)?;
        if let Some(version) = reply.as_ref().and_then(Version::parse) {
            return Ok(version);
        }
        tracing::debug!(attempt, answered = reply.is_some(), "no WCH version answer");
    }
    Err(BezelError::Timeout(
        "the screen did not answer the version query".into(),
    ))
}

impl<W: Wire, P: Pause + Clone> Wch<W, P> {
    /// Queries the firmware and panel type, stops any device-side video and
    /// resets the frame buffer, as the vendor app does when a panel appears.
    /// `candidates` are the models discovery allowed (one per product id).
    pub fn connect(mut wire: W, pause: &P, candidates: &[&'static DeviceModel]) -> Result<Self> {
        let models: Vec<&'static DeviceModel> = candidates
            .iter()
            .copied()
            .filter(|m| m.family == Family::Wch)
            .collect();
        if models.is_empty() {
            return Err(BezelError::Transport("no WCH model to connect".into()));
        }
        let version = query_version(&mut wire, pause)?;
        let panel = version.panel();
        let model = pick_model(&models, panel).ok_or_else(|| {
            BezelError::Transport(format!(
                "panel type {} does not tell the WCH models apart",
                version.panel_code
            ))
        })?;
        tracing::debug!(model = %model.id, firmware = %version.firmware(), panel = ?panel, "WCH version");
        let firmware = match panel {
            Some(p) => format!("{} {}", version.firmware(), p.name()),
            None => version.firmware(),
        };
        let mut screen = Self {
            wire,
            pause: pause.clone(),
            identity: ScreenIdentity {
                model,
                firmware: Some(firmware),
            },
            orientation: Orientation::Portrait,
            level: None,
            off: false,
        };
        screen.command(op::STOP_VIDEO, 0)?;
        screen.command(op::RESET_MEM, 0)?;
        screen.pause.pause(START_RESET_PAUSE);
        Ok(screen)
    }
}

impl<W: Wire, P: Pause> Wch<W, P> {
    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    fn command(&mut self, cmd: u8, arg: u8) -> Result<Option<[u8; proto::PACKET_LEN]>> {
        exchange(&mut self.wire, &self.pause, cmd, arg)
    }

    fn native(&self, frame: &Frame) -> Result<Frame> {
        let model = self.identity.model;
        check_frame(model, self.orientation, frame)?;
        let turns = self.orientation.quarter_turns_to(model.native_orientation);
        Ok(frame.rotated(turns))
    }

    /// Writes a full native frame and handles its ACK.
    fn send_frame(&mut self, native: &Frame) -> Result<()> {
        let width = native.size().width as usize;
        let bgr = proto::bgr888(native.as_rgba(), width)
            .ok_or_else(|| BezelError::Transport("frame buffer does not match its size".into()))?;
        let blocks = proto::blocks(&bgr);
        for write in proto::usb_writes(&blocks) {
            self.wire.send(&write).map_err(io_err)?;
        }
        self.pause.pause(REPLY_DELAY);
        let ack = self
            .wire
            .receive(proto::PACKET_LEN, REPLY_TIMEOUT)
            .map_err(io_err)?;
        if !ack.is_empty() && proto::decrypt_reply(&ack)[2] == proto::ACK_RESET {
            tracing::debug!("frame ACK asks for ResetMem");
            self.command(op::RESET_MEM, 0)?;
            self.pause.pause(ACK_RESET_PAUSE);
        }
        Ok(())
    }
}

impl<W: Wire, P: Pause> ScreenLink for Wch<W, P> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    /// 0..=255 on the wire; 0 turns the panel off.
    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        let level = brightness.scaled(255) as u8;
        self.command(op::SET_BRIGHTNESS, level)?;
        self.level = Some(level);
        self.off = false;
        Ok(())
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.orientation = orientation;
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        let native = self.native(frame)?;
        self.send_frame(&native)?;
        if self.off {
            let level = self.level.unwrap_or(DEFAULT_LEVEL);
            self.command(op::SET_BRIGHTNESS, level)?;
            self.off = false;
        }
        Ok(())
    }

    /// Brightness 0, as the vendor app does on shutdown and system sleep.
    fn screen_off(&mut self) -> Result<()> {
        self.command(op::SET_BRIGHTNESS, 0)?;
        self.off = true;
        Ok(())
    }

    /// StopVideo, as the vendor app sends when its monitor loop stops.
    fn release(&mut self) -> Result<()> {
        self.command(op::STOP_VIDEO, 0)?;
        Ok(())
    }
}

/// The catalog model of a vendor panel type.
fn panel_model(panel: Panel) -> ModelId {
    match panel {
        Panel::Rect338 => ModelId("wch-3.38"),
        Panel::Square28 => ModelId("wch-2.8-square"),
        Panel::Rect28 | Panel::Rect24 => ModelId("wch-2.4-2.8-rect"),
    }
}

/// The product id names one model; the panel type only breaks a tie (and is
/// logged when it disagrees, since the resolution always comes from the
/// product id, as in the vendor app).
fn pick_model(
    models: &[&'static DeviceModel],
    panel: Option<Panel>,
) -> Option<&'static DeviceModel> {
    let named = panel.map(panel_model);
    match models {
        [only] => {
            if named.is_some_and(|id| id != only.id) {
                tracing::warn!(model = %only.id, panel = ?panel, "WCH panel type disagrees with the product id");
            }
            Some(only)
        }
        _ => models.iter().copied().find(|m| Some(m.id) == named),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::RealTime;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::frame::{Rect, Rgba};
    use bezel_core::domain::geometry::Size;
    use std::sync::{Arc, Mutex};

    /// Records every pause instead of sleeping.
    #[derive(Clone, Default)]
    struct Log(Arc<Mutex<Vec<Duration>>>);
    impl Pause for Log {
        fn pause(&self, d: Duration) {
            self.0.lock().unwrap().push(d);
        }
    }
    impl Log {
        fn long(&self) -> Vec<Duration> {
            let log = self.0.lock().unwrap();
            log.iter().copied().filter(|d| *d > REPLY_DELAY).collect()
        }
    }

    fn model(id: &'static str) -> &'static DeviceModel {
        model_by_id(ModelId(id)).unwrap()
    }

    /// A raw reply whose `[2..10]` encrypts `plain`.
    fn reply(plain: [u8; 8]) -> Vec<u8> {
        let mut r = vec![0u8; proto::PACKET_LEN];
        r[..2].copy_from_slice(&proto::SYNC);
        r[proto::CIPHER].copy_from_slice(&proto::encrypt_block(plain));
        r[31] = proto::END;
        r
    }

    fn version(major: u8, minor: u8, panel: u8) -> Vec<u8> {
        reply([op::VERSION_REPLY, major, minor, panel, 0, 0, 0, 0])
    }

    /// Plaintext `[cmd, arg]` of each 32-byte command packet sent.
    fn commands(wire: &ScriptedWire) -> Vec<(u8, u8)> {
        wire.sent
            .iter()
            .filter(|p| p.len() == proto::PACKET_LEN)
            .map(|p| {
                let mut c = [0u8; 8];
                c.copy_from_slice(&p[proto::CIPHER]);
                let plain = proto::decrypt_block(c);
                (plain[0], plain[1])
            })
            .collect()
    }

    fn connected(id: &'static str, log: &Log) -> Wch<ScriptedWire, Log> {
        let wire = ScriptedWire::with_replies([version(1, 2, 8)]);
        Wch::connect(wire, log, &[model(id)]).unwrap()
    }

    #[test]
    fn connect_queries_stops_video_and_resets_memory() {
        let log = Log::default();
        let s = connected("wch-2.4-2.8-rect", &log);
        assert_eq!(
            s.wire().sent,
            vec![
                proto::command(op::GET_VERSION, 0).to_vec(),
                proto::command(op::STOP_VIDEO, 0).to_vec(),
                proto::command(op::RESET_MEM, 0).to_vec(),
            ]
        );
        assert_eq!(s.identity().model.id, ModelId("wch-2.4-2.8-rect"));
        assert_eq!(s.identity().firmware.as_deref(), Some("1.2 24_Rect"));
        assert_eq!(log.long(), vec![START_RESET_PAUSE]);
        assert_eq!(
            s.wire().discards,
            3,
            "stale input dropped before each command"
        );
    }

    #[test]
    fn the_version_query_is_retried_once() {
        let log = Log::default();
        let wire = ScriptedWire::with_replies([vec![], version(3, 14, 0)]);
        let s = Wch::connect(wire, &log, &[model("wch-4.3")]).unwrap();
        assert_eq!(
            commands(s.wire()),
            vec![
                (op::GET_VERSION, 0),
                (op::GET_VERSION, 0),
                (op::STOP_VIDEO, 0),
                (op::RESET_MEM, 0)
            ]
        );
        assert_eq!(
            s.identity().firmware.as_deref(),
            Some("3.14"),
            "no type maps to the 4.3\""
        );
        assert_eq!(log.long(), vec![INIT_RETRY_PAUSE, START_RESET_PAUSE]);

        // A reply without 0x41, then silence.
        let wire = ScriptedWire::with_replies([reply([0x57, 0, 0, 0, 0, 0, 0, 0])]);
        let err = Wch::connect(wire, &log, &[model("wch-4.3")]).err().unwrap();
        assert!(matches!(err, BezelError::Timeout(_)));
    }

    #[test]
    fn the_model_comes_from_the_product_id() {
        let log = Log::default();
        // Panel type 6 (square) on the 3.38" product id: the product id wins.
        let wire = ScriptedWire::with_replies([version(1, 0, 6)]);
        let s = Wch::connect(wire, &log, &[model("wch-3.38")]).unwrap();
        assert_eq!(s.identity().model.id, ModelId("wch-3.38"));
        // Without a WCH candidate nothing is sent.
        let wire = ScriptedWire::with_replies([version(1, 0, 6)]);
        let err = Wch::connect(wire, &log, &[model("turing-8.8")])
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Transport(_)));
        // Several candidates: the panel type decides, or nothing does.
        let two = [model("wch-3.38"), model("wch-2.8-square")];
        assert_eq!(
            pick_model(&two, Some(Panel::Square28)).map(|m| m.id),
            Some(ModelId("wch-2.8-square"))
        );
        assert!(pick_model(&two, None).is_none());
        let wire = ScriptedWire::with_replies([version(1, 0, 0)]);
        assert!(Wch::connect(wire, &log, &two).is_err());
        assert_eq!(panel_model(Panel::Rect28), ModelId("wch-2.4-2.8-rect"));
        assert_eq!(panel_model(Panel::Rect338), ModelId("wch-3.38"));
        RealTime.pause(Duration::ZERO);
    }

    #[test]
    fn frames_are_full_bgr_in_sequenced_blocks() {
        let log = Log::default();
        let mut s = connected("wch-2.4-2.8-rect", &log);
        let before = s.wire().sent.len();
        let mut frame = Frame::filled(Size::new(240, 320), Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(0x11, 0x22, 0x33));
        frame.fill_rect(Rect::new(239, 319, 1, 1), Rgba::opaque(0x44, 0x55, 0x66));
        s.present(&frame).unwrap();
        let writes = &s.wire().sent[before..];
        assert_eq!(writes.len(), 60);
        assert!(writes.iter().all(|w| w.len() == proto::USB_CHUNK));
        let wire_bytes = writes.concat();
        assert_eq!(&wire_bytes[..32], &proto::block_header(1));
        assert_eq!(&wire_bytes[32..35], &[0x33, 0x22, 0x11], "B, G, R");
        let last = &wire_bytes[479 * 512..];
        assert_eq!(&last[11..13], &[0x01, 0xE0], "block 480");
        assert_eq!(&last[509..512], &[0x66, 0x55, 0x44]);
        assert_eq!(commands(s.wire()).len(), 3, "a quiet ACK triggers nothing");
    }

    #[test]
    fn an_ack_of_0x60_resets_the_frame_buffer() {
        let log = Log::default();
        let mut s = connected("wch-2.8-square", &log);
        s.wire
            .reply(&reply([proto::ACK_RESET, 0, 0, 0, 0, 0, 0, 0]));
        s.present(&Frame::filled(Size::new(320, 320), Rgba::WHITE))
            .unwrap();
        assert_eq!(s.wire().sent.len(), 3 + 80 + 1);
        assert_eq!(
            s.wire().sent.last().unwrap(),
            &proto::command(op::RESET_MEM, 0).to_vec()
        );
        assert_eq!(log.long(), vec![START_RESET_PAUSE, ACK_RESET_PAUSE]);
        // Any other status is ignored.
        s.wire.reply(&reply([0x01, 0, 0, 0, 0, 0, 0, 0]));
        s.present(&Frame::filled(Size::new(320, 320), Rgba::WHITE))
            .unwrap();
        assert_eq!(s.wire().sent.len(), 3 + 80 + 1 + 80);
    }

    #[test]
    fn frames_are_rotated_to_the_native_orientation() {
        let log = Log::default();
        // The 4.3" is natively landscape (480 x 272).
        let mut s = connected("wch-4.3", &log);
        let before = s.wire().sent.len();
        let mut frame = Frame::filled(Size::new(272, 480), Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(0xFF, 0, 0));
        s.present(&frame).unwrap();
        let wire_bytes = s.wire().sent[before..].concat();
        assert_eq!(wire_bytes.len(), 417_792);
        // Portrait -> landscape is one clockwise quarter turn: the top-left
        // pixel lands at the end of the first native row (x = 479, y = 0).
        let offset = 479 * 3;
        let at = |i: usize| wire_bytes[(i / 480) * 512 + 32 + i % 480];
        assert_eq!([at(offset), at(offset + 1), at(offset + 2)], [0, 0, 0xFF]);
        s.set_orientation(Orientation::Landscape).unwrap();
        assert!(s.present(&frame).is_err(), "landscape expects 480 x 272");
        s.present(&Frame::filled(Size::new(480, 272), Rgba::BLACK))
            .unwrap();
    }

    #[test]
    fn brightness_screen_off_and_release() {
        let log = Log::default();
        let mut s = connected("wch-3.38", &log);
        s.set_brightness(Brightness::new(100).unwrap()).unwrap();
        s.screen_off().unwrap();
        s.present(&Frame::filled(Size::new(180, 640), Rgba::BLACK))
            .unwrap();
        s.present(&Frame::filled(Size::new(180, 640), Rgba::BLACK))
            .unwrap();
        s.release().unwrap();
        assert_eq!(
            commands(s.wire())[3..],
            [
                (op::SET_BRIGHTNESS, 255),
                (op::SET_BRIGHTNESS, 0),
                (op::SET_BRIGHTNESS, 255),
                (op::STOP_VIDEO, 0)
            ]
        );

        let mut s = connected("wch-3.38", &log);
        s.screen_off().unwrap();
        s.present(&Frame::filled(Size::new(180, 640), Rgba::BLACK))
            .unwrap();
        assert_eq!(
            commands(s.wire()).last(),
            Some(&(op::SET_BRIGHTNESS, DEFAULT_LEVEL))
        );
        s.set_brightness(Brightness::new(50).unwrap()).unwrap();
        assert_eq!(commands(s.wire()).last(), Some(&(op::SET_BRIGHTNESS, 128)));
    }

    #[test]
    fn rotation_and_test_mode_are_never_sent() {
        let log = Log::default();
        let mut s = connected("wch-2.8-square", &log);
        for o in Orientation::ALL {
            s.set_orientation(o).unwrap();
            s.present(&Frame::filled(Size::new(320, 320), Rgba::WHITE))
                .unwrap();
        }
        s.screen_off().unwrap();
        s.release().unwrap();
        assert!(
            commands(s.wire())
                .iter()
                .all(|(c, _)| *c != op::SET_ROTATION && *c != op::TEST_MODE)
        );
    }
}
