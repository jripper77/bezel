//! Driver for Turing rev C screens (2.1"/2.8"/5"/8.8" UART generation).

use std::time::Duration;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::{Frame, RGBA_BYTES};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};

use crate::protocol::turing_rev_c::{self as proto, Hello, PixelFormat, Status, op};
use crate::wire::Wire;

/// How long the device may take to answer HELLO or QUERY_STATUS.
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
/// HELLO attempts before giving up.
const HELLO_TRIES: usize = 3;
/// Pause between failed HELLO attempts (after a resync block).
const HELLO_RETRY_PAUSE: Duration = Duration::from_millis(1000);
/// STOP_MEDIA polls while waiting for `media_stop`.
const STOP_MEDIA_POLLS: usize = 20;

/// Pauses between protocol steps. The fake used in tests does not sleep.
pub trait Pause: Send {
    /// Waits `d`.
    fn pause(&self, d: Duration);
}

/// Real time.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealTime;

impl Pause for RealTime {
    fn pause(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// A connected rev C screen.
pub struct TuringRevC<W: Wire> {
    wire: W,
    identity: ScreenIdentity,
    format: PixelFormat,
    orientation: Orientation,
    last: Option<Vec<u8>>,
    seq: u32,
}

fn io_err(e: std::io::Error) -> BezelError {
    BezelError::Transport(e.to_string())
}

impl<W: Wire> TuringRevC<W> {
    /// Handshakes over `wire` and prepares the screen for streaming.
    /// `candidates` are the models discovery allowed; the HELLO answer picks one.
    pub fn connect<P: Pause>(
        mut wire: W,
        pause: &P,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        let hello = handshake(&mut wire, pause)?;
        tracing::debug!(reply = %hello.raw, rom = hello.rom, "HELLO");
        let model = pick_model(&hello, candidates).ok_or_else(|| {
            BezelError::Transport(format!("unexpected screen model: {}", hello.raw))
        })?;
        let mut screen = Self {
            wire,
            format: hello.partial_format(),
            identity: ScreenIdentity {
                model,
                firmware: Some(hello.raw),
            },
            orientation: Orientation::Portrait,
            last: None,
            seq: 0,
        };
        screen.stop_media()?;
        screen.send(&proto::simple(op::PRE_UPDATE_BITMAP))?;
        Ok(screen)
    }

    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.wire.send(bytes).map_err(io_err)
    }

    fn stop_media(&mut self) -> Result<()> {
        self.send(&proto::simple(op::STOP_VIDEO))?;
        for _ in 0..STOP_MEDIA_POLLS {
            self.send(&proto::simple(op::STOP_MEDIA))?;
            let reply = self.wire.receive(1024, REPLY_TIMEOUT).map_err(io_err)?;
            if String::from_utf8_lossy(&reply).contains("media_stop") {
                return Ok(());
            }
        }
        // Older firmware never answers; streaming still works.
        tracing::debug!("no media_stop answer; continuing");
        Ok(())
    }

    fn native(&self, frame: &Frame) -> Result<Vec<u8>> {
        let model = self.identity.model;
        let expected = model.panel.in_orientation(self.orientation);
        if frame.size() != expected {
            return Err(BezelError::Transport(format!(
                "frame is {}x{}, the screen expects {}x{} in this orientation",
                frame.size().width,
                frame.size().height,
                expected.width,
                expected.height
            )));
        }
        let turns = self.orientation.quarter_turns_to(model.native_orientation);
        Ok(rgba_to_bgra(frame.rotated(turns).as_rgba()))
    }

    fn full_frame(&mut self, bgra: Vec<u8>) -> Result<()> {
        self.send(&proto::start_display_block())?;
        self.send(&proto::full_frame_header(bgra.len() as u32))?;
        self.send(&proto::blocks(&bgra))?;
        // The device may say something after a frame; drain it so it does not
        // pollute the next reply.
        let after = self
            .wire
            .receive(1024, Duration::from_millis(50))
            .map_err(io_err)?;
        tracing::debug!(bytes = bgra.len(), reply = %String::from_utf8_lossy(&after), "full frame");
        self.last = Some(bgra);
        self.seq = 0;
        Ok(())
    }

    fn partial(&mut self, mut list: Vec<u8>, bgra: Vec<u8>) -> Result<()> {
        list.extend_from_slice(&proto::MAGIC);
        self.send(&proto::partial_header(list.len() as u32, self.seq))?;
        self.send(&proto::blocks(&list))?;
        self.seq = self.seq.wrapping_add(1);
        self.last = Some(bgra);
        Ok(())
    }

    /// QUERY_STATUS round-trip; `true` when the device asks for a full frame.
    fn needs_full_frame(&mut self) -> Result<bool> {
        self.send(&proto::simple(op::QUERY_STATUS))?;
        let reply = self.wire.receive(1024, REPLY_TIMEOUT).map_err(io_err)?;
        let status = Status::parse(&reply);
        tracing::debug!(reply = %String::from_utf8_lossy(&reply).trim_end_matches('\0'), seq = self.seq, "QUERY_STATUS");
        Ok(status.is_some_and(|s| s.need_resend))
    }
}

impl<W: Wire> ScreenLink for TuringRevC<W> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        let level = brightness.scaled(255) as u8;
        self.send(&proto::set_brightness(level))
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        if orientation != self.orientation {
            self.orientation = orientation;
            self.last = None;
        }
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        let bgra = self.native(frame)?;
        let Some(last) = self.last.as_ref() else {
            return self.full_frame(bgra);
        };
        match proto::diff_runs(last, &bgra, self.format) {
            None => self.full_frame(bgra),
            Some(list) if list.is_empty() => Ok(()),
            Some(list) => {
                self.partial(list, bgra)?;
                if self.needs_full_frame()? {
                    self.last = None;
                }
                Ok(())
            }
        }
    }

    fn screen_off(&mut self) -> Result<()> {
        self.last = None;
        self.send(&proto::simple(op::TURN_OFF))
    }

    fn release(&mut self) -> Result<()> {
        self.last = None;
        self.send(&proto::simple(op::END_UPDATE_BITMAP))
    }
}

fn handshake<W: Wire, P: Pause>(wire: &mut W, pause: &P) -> Result<Hello> {
    wire.discard_input().map_err(io_err)?;
    for attempt in 0..HELLO_TRIES {
        wire.send(&proto::hello()).map_err(io_err)?;
        let reply = wire.receive(1024, REPLY_TIMEOUT).map_err(io_err)?;
        if let Some(hello) = Hello::parse(&reply) {
            return Ok(hello);
        }
        tracing::debug!(attempt, "no HELLO answer; resyncing");
        wire.send(&proto::start_display_block()).map_err(io_err)?;
        pause.pause(HELLO_RETRY_PAUSE);
    }
    Err(BezelError::Timeout(
        "the screen did not answer HELLO".into(),
    ))
}

/// The model the HELLO answer names, among the discovery candidates.
/// Only 8.8" answers are trusted to name the size (2.1" units answer `5inch`).
fn pick_model(hello: &Hello, candidates: &[&'static DeviceModel]) -> Option<&'static DeviceModel> {
    if let [only] = candidates {
        return Some(only);
    }
    let wanted = match hello.model.as_str() {
        "88inch" => "turing-8.8",
        "5inch" => "turing-5",
        _ => return None,
    };
    candidates.iter().copied().find(|m| m.id.0 == wanted)
}

/// RGBA8 → BGRA8, the rev C pixel order.
pub fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<RGBA_BYTES>().0 {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::{Rect, Rgba};

    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

    fn m88() -> &'static DeviceModel {
        model_by_id(ModelId("turing-8.8")).unwrap()
    }

    fn connected() -> TuringRevC<ScriptedWire> {
        let wire = ScriptedWire::with_replies([
            b"chs_88inch.dev1_rom1.90\0".to_vec(),
            b"media_stop".to_vec(),
        ]);
        TuringRevC::connect(wire, &NoPause, &[m88()]).unwrap()
    }

    #[test]
    fn connect_handshakes_stops_media_and_enters_streaming() {
        let s = connected();
        let opcodes: Vec<u8> = s.wire().sent.iter().map(|p| p[0]).collect();
        assert_eq!(
            opcodes,
            vec![
                op::HELLO,
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::PRE_UPDATE_BITMAP
            ]
        );
        assert_eq!(
            s.identity().firmware.as_deref(),
            Some("chs_88inch.dev1_rom1.90")
        );
        assert_eq!(s.format, PixelFormat::Bgra);
        assert_eq!(s.wire().discards, 1);
    }

    #[test]
    fn hello_is_retried_with_a_resync_block_then_times_out() {
        let wire = ScriptedWire::default();
        let err = TuringRevC::connect(wire, &NoPause, &[m88()]).err().unwrap();
        assert!(matches!(err, BezelError::Timeout(_)));
        let mut wire = ScriptedWire::with_replies([vec![], b"chs_88inch.dev1_rom1.88".to_vec()]);
        let _ = handshake(&mut wire, &NoPause).unwrap();
        let opcodes: Vec<u8> = wire.sent.iter().map(|p| p[0]).collect();
        assert_eq!(opcodes, vec![op::HELLO, 0x2C, op::HELLO]);
    }

    #[test]
    fn first_frame_is_full_and_native_bgra() {
        let mut s = connected();
        s.set_orientation(Orientation::Portrait).unwrap();
        let mut frame = Frame::filled(m88().panel, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        s.present(&frame).unwrap();
        let sent = &s.wire().sent;
        let n = sent.len();
        assert!(sent[n - 3].iter().all(|&b| b == 0x2C));
        assert_eq!(
            &sent[n - 2][..7],
            &[0xC8, 0xEF, 0x69, 0x00, 0x38, 0x40, 0x00]
        );
        let data = &sent[n - 1];
        assert_eq!(data.len(), 3_701_250);
        // Portrait on a reverse-portrait panel: rotated 180°, so the red
        // top-left pixel is the last native pixel (BGRA 00 00 FF FF).
        let bgra = s.last.as_ref().unwrap();
        assert_eq!(&bgra[bgra.len() - 4..], &[0, 0, 255, 255]);
    }

    #[test]
    fn later_frames_are_partial_then_status_is_queried() {
        let mut s = connected();
        let base = Frame::filled(m88().panel, Rgba::BLACK);
        s.present(&base).unwrap();
        let before = s.wire().sent.len();
        // Unchanged frame: nothing is sent.
        s.present(&base).unwrap();
        assert_eq!(s.wire().sent.len(), before);

        let mut next = base.clone();
        next.fill_rect(Rect::new(10, 10, 3, 1), Rgba::WHITE);
        s.wire.reply(b"needReSend:0|renderCnt:1|theme:");
        s.present(&next).unwrap();
        let sent = &s.wire().sent[before..];
        assert_eq!(sent[0][0], op::UPDATE_BITMAP);
        assert_eq!(sent[2][0], op::QUERY_STATUS);
        let list_len =
            u32::from_be_bytes([sent[0][3], sent[0][4], sent[0][5], sent[0][6]]) as usize;
        assert_eq!(list_len, 5 + 3 * 4 + 2, "one run of 3 BGRA pixels + EF 69");
        assert_eq!(&sent[1][list_len - 2..list_len], &[0xEF, 0x69]);
        assert_eq!(s.seq, 1);
    }

    #[test]
    fn need_resend_forces_the_next_frame_full() {
        let mut s = connected();
        let base = Frame::filled(m88().panel, Rgba::BLACK);
        s.present(&base).unwrap();
        let mut next = base.clone();
        next.fill_rect(Rect::new(0, 0, 1, 1), Rgba::WHITE);
        s.wire.reply(b"needReSend:1|renderCnt:1");
        s.present(&next).unwrap();
        assert!(s.last.is_none());
        s.present(&next).unwrap();
        assert_eq!(s.wire().sent.last().unwrap().len(), 3_701_250);
    }

    #[test]
    fn wrong_size_and_controls() {
        let mut s = connected();
        let small = Frame::filled(bezel_core::domain::geometry::Size::new(10, 10), Rgba::BLACK);
        assert!(s.present(&small).is_err());
        s.set_orientation(Orientation::Landscape).unwrap();
        let wide = Frame::filled(
            m88().panel.in_orientation(Orientation::Landscape),
            Rgba::BLACK,
        );
        s.present(&wide).unwrap();
        s.set_brightness(Brightness::new(25).unwrap()).unwrap();
        s.screen_off().unwrap();
        s.release().unwrap();
        let sent = &s.wire().sent;
        let n = sent.len();
        assert_eq!(
            &sent[n - 3][..11],
            &[0x7B, 0xEF, 0x69, 0, 0, 0, 1, 0, 0, 0, 64]
        );
        assert_eq!(sent[n - 2][0], op::TURN_OFF);
        assert_eq!(sent[n - 1][0], op::END_UPDATE_BITMAP);
    }

    #[test]
    fn model_choice() {
        let two: Vec<&'static DeviceModel> = ["turing-2.1", "turing-2.8"]
            .iter()
            .map(|id| model_by_id(ModelId(id)).unwrap())
            .collect();
        let hello = Hello::parse(b"chs_5inch.dev1_rom1.88").unwrap();
        assert!(pick_model(&hello, &two).is_none());
        let five = model_by_id(ModelId("turing-5")).unwrap();
        assert_eq!(
            pick_model(&hello, &[five, two[0]]).map(|m| m.id.0),
            Some("turing-5")
        );
        let odd = Hello::parse(b"chs_99inch.dev1_rom1.0").unwrap();
        assert!(pick_model(&odd, &two).is_none());
        assert_eq!(rgba_to_bgra(&[1, 2, 3, 4]), vec![3, 2, 1, 4]);
        RealTime.pause(Duration::ZERO);
    }
}
