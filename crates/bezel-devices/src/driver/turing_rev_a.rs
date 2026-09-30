//! Driver for Turing rev A screens (Turing 3.5", UsbPCMonitor 3.5"/5"/7").
//!
//! The device rotates by itself (16-byte SET_ORIENTATION), so frames are sent
//! in the orientation the user looks at. Every update is a DISPLAY_BITMAP of a
//! rectangle: the whole screen for the first frame, then the core's dirty
//! rectangles. Nothing is ever acknowledged.

use std::time::Duration;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::{Frame, Rect, dirty_rects};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};

use crate::driver::turing_rev_c::Pause;
use crate::protocol::turing_rev_a::{self as proto, SubRevision};
use crate::wire::Wire;

/// How long the device may take to answer HELLO (the reference's read timeout).
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);

/// A connected rev A screen.
pub struct TuringRevA<W: Wire> {
    wire: W,
    identity: ScreenIdentity,
    sub_revision: SubRevision,
    orientation: Orientation,
    /// Orientation the device was last told; `None` until the first packet.
    device_orientation: Option<Orientation>,
    /// SCREEN_ON must precede the next frame (at start, and after `screen_off`).
    needs_screen_on: bool,
    /// The frame the panel shows; `None` forces the next frame to be full.
    last: Option<Frame>,
}

fn io_err(e: std::io::Error) -> BezelError {
    BezelError::Transport(e.to_string())
}

impl<W: Wire> TuringRevA<W> {
    /// Handshakes over `wire`. `candidates` are the models discovery allowed;
    /// the HELLO answer picks one. Only HELLO is sent: no RESET, no CLEAR
    /// (D-2026-09-30-device-protocols-2). Rev A needs no pacing; `_pause`
    /// keeps the serial drivers interchangeable.
    pub fn connect<P: Pause>(
        mut wire: W,
        _pause: &P,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        wire.discard_input().map_err(io_err)?;
        wire.send(&proto::hello()).map_err(io_err)?;
        let reply = wire
            .receive(proto::HELLO_REPLY_LEN, REPLY_TIMEOUT)
            .map_err(io_err)?;
        wire.discard_input().map_err(io_err)?;
        let sub_revision = SubRevision::from_hello(&reply);
        tracing::debug!(reply = ?reply, sub_revision = sub_revision.name(), "HELLO");
        let model = pick_model(sub_revision, candidates).ok_or_else(|| {
            BezelError::Transport(format!(
                "unexpected screen model: {} ({})",
                sub_revision.name(),
                sub_revision.model_id()
            ))
        })?;
        Ok(Self {
            wire,
            identity: ScreenIdentity {
                model,
                // The official Turing 3.5" stays silent: nothing to report.
                firmware: (!reply.is_empty()).then(|| sub_revision.name().to_string()),
            },
            sub_revision,
            orientation: Orientation::Portrait,
            device_orientation: None,
            needs_screen_on: true,
            last: None,
        })
    }

    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    /// The variant HELLO revealed.
    pub fn sub_revision(&self) -> SubRevision {
        self.sub_revision
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.wire.send(bytes).map_err(io_err)
    }

    fn send_orientation(&mut self) -> Result<()> {
        let size = self.identity.model.panel.in_orientation(self.orientation);
        let packet = proto::set_orientation(self.orientation, size).ok_or_else(|| {
            BezelError::Transport(format!(
                "{}x{} does not fit the 10-bit rev A address space",
                size.width, size.height
            ))
        })?;
        self.send(&packet)?;
        self.device_orientation = Some(self.orientation);
        Ok(())
    }

    fn check_size(&self, frame: &Frame) -> Result<()> {
        let expected = self.identity.model.panel.in_orientation(self.orientation);
        if frame.size() == expected {
            return Ok(());
        }
        Err(BezelError::Transport(format!(
            "frame is {}x{}, the screen expects {}x{} in this orientation",
            frame.size().width,
            frame.size().height,
            expected.width,
            expected.height
        )))
    }

    /// One DISPLAY_BITMAP: header, then the pixels in four-row chunks, sent
    /// back to back so nothing can slip in between.
    fn bitmap(&mut self, frame: &Frame, rect: Rect) -> Result<()> {
        let header = proto::display_bitmap(rect).ok_or_else(|| {
            BezelError::Transport(format!("rectangle {rect:?} cannot be addressed"))
        })?;
        let pixels = proto::rgb565_le(frame.crop(rect).as_rgba());
        self.send(&header)?;
        for chunk in pixels.chunks(proto::chunk_len(frame.size().width)) {
            self.send(chunk)?;
        }
        Ok(())
    }
}

impl<W: Wire> ScreenLink for TuringRevA<W> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.send(&proto::set_brightness(proto::brightness_level(brightness)))
    }

    /// The device rotates: the 16-byte packet goes out now, and the next frame
    /// is full since the device may not redraw what it showed.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        if orientation == self.orientation && self.device_orientation == Some(orientation) {
            return Ok(());
        }
        self.orientation = orientation;
        self.last = None;
        self.send_orientation()
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        self.check_size(frame)?;
        if self.needs_screen_on {
            self.send(&proto::screen_on())?;
            self.needs_screen_on = false;
        }
        if self.device_orientation != Some(self.orientation) {
            self.send_orientation()?;
        }
        let rects = match self.last.take() {
            None => vec![Rect::of(frame.size())],
            Some(previous) => dirty_rects(&previous, frame),
        };
        for rect in rects {
            self.bitmap(frame, rect)?;
        }
        self.last = Some(frame.clone());
        Ok(())
    }

    fn screen_off(&mut self) -> Result<()> {
        self.last = None;
        self.needs_screen_on = true;
        self.send(&proto::screen_off())
    }

    /// Rev A has no standalone mode (no clock, no stored media) and no command
    /// that ends streaming: the panel keeps the last frame until the next host
    /// writes. Nothing is sent; the next frame, if any, is full.
    fn release(&mut self) -> Result<()> {
        self.last = None;
        Ok(())
    }
}

/// The model HELLO names among the discovery candidates, else a candidate
/// with the same panel (a silent screen is a 320 x 480 one).
fn pick_model(
    sub_revision: SubRevision,
    candidates: &[&'static DeviceModel],
) -> Option<&'static DeviceModel> {
    let id = sub_revision.model_id();
    candidates.iter().copied().find(|m| m.id == id).or_else(|| {
        candidates
            .iter()
            .copied()
            .find(|m| m.panel == sub_revision.panel())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::turing_rev_a::op;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::geometry::Size;

    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

    fn model(id: &'static str) -> &'static DeviceModel {
        model_by_id(ModelId(id)).unwrap()
    }

    fn family() -> Vec<&'static DeviceModel> {
        [
            "turing-3.5",
            "usbpcmonitor-3.5",
            "usbpcmonitor-5",
            "usbpcmonitor-7",
        ]
        .iter()
        .map(|id| model(id))
        .collect()
    }

    fn connect_with(reply: &[u8]) -> Result<TuringRevA<ScriptedWire>> {
        let wire = ScriptedWire::with_replies([reply.to_vec()]);
        TuringRevA::connect(wire, &NoPause, &family())
    }

    fn connected(reply: &[u8]) -> TuringRevA<ScriptedWire> {
        connect_with(reply).unwrap()
    }

    fn sent(s: &TuringRevA<ScriptedWire>) -> &[Vec<u8>] {
        &s.wire().sent
    }

    #[test]
    fn hello_answers_resolve_the_model_and_nothing_else_is_sent() {
        for (reply, id, firmware) in [
            (vec![1; 6], "usbpcmonitor-3.5", Some("USBMONITOR_3_5")),
            (vec![2; 6], "usbpcmonitor-5", Some("USBMONITOR_5")),
            (vec![3; 6], "usbpcmonitor-7", Some("USBMONITOR_7")),
            (vec![], "turing-3.5", None),
            (vec![0x45, 0x00], "turing-3.5", Some("TURING_3_5")),
        ] {
            let s = connected(&reply);
            assert_eq!(s.identity().model.id.0, id);
            assert_eq!(s.identity().firmware.as_deref(), firmware);
            // Only HELLO: no RESET, no CLEAR (D-2026-09-30-device-protocols-2).
            assert_eq!(sent(&s), &[vec![0x45; 6]]);
            assert_eq!(s.wire().discards, 2, "input is flushed before and after");
        }
        assert_eq!(connected(&[2; 6]).sub_revision(), SubRevision::UsbMonitor5);
    }

    #[test]
    fn model_resolution_among_narrow_candidates() {
        let turing = model("turing-3.5");
        let usb35 = model("usbpcmonitor-3.5");
        let usb5 = model("usbpcmonitor-5");
        // A silent screen keeps a 3.5" model even when discovery ruled out the Turing.
        assert_eq!(
            pick_model(SubRevision::Turing35, &[usb5, usb35]).map(|m| m.id.0),
            Some("usbpcmonitor-3.5")
        );
        assert_eq!(
            pick_model(SubRevision::UsbMonitor35, &[turing]).map(|m| m.id.0),
            Some("turing-3.5")
        );
        assert!(pick_model(SubRevision::UsbMonitor5, &[turing, usb35]).is_none());
        assert!(pick_model(SubRevision::Turing35, &[]).is_none());
        let wire = ScriptedWire::with_replies([vec![3; 6]]);
        let err = TuringRevA::connect(wire, &NoPause, &[turing])
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Transport(ref m) if m.contains("USBMONITOR_7")));
    }

    #[test]
    fn first_frame_is_one_full_screen_bitmap_in_le_rgb565() {
        let mut s = connected(&[]);
        let mut frame = Frame::filled(Size::new(320, 480), Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        frame.fill_rect(Rect::new(319, 479, 1, 1), Rgba::opaque(0, 0, 255));
        s.present(&frame).unwrap();
        let sent = &sent(&s)[1..];
        assert_eq!(sent[0], proto::screen_on());
        // The golden file: orientation packet, header 00 00 04 fd df c5, 120 writes of 2560 bytes.
        assert_eq!(
            sent[1],
            proto::set_orientation(Orientation::Portrait, Size::new(320, 480)).unwrap()
        );
        assert_eq!(sent[2], [0x00, 0x00, 0x04, 0xfd, 0xdf, 0xc5]);
        let data = &sent[3..];
        assert_eq!(data.len(), 120);
        assert!(data.iter().all(|c| c.len() == 2560));
        assert_eq!(&data[0][..4], &[0x00, 0xf8, 0x00, 0x00], "red, then black");
        assert_eq!(&data[119][2558..], &[0x1f, 0x00], "blue bottom-right, LE");
    }

    #[test]
    fn later_frames_send_only_the_dirty_rectangles() {
        let mut s = connected(&[1; 6]);
        let base = Frame::filled(Size::new(320, 480), Rgba::BLACK);
        s.present(&base).unwrap();
        let before = sent(&s).len();
        s.present(&base).unwrap();
        assert_eq!(sent(&s).len(), before, "an unchanged frame sends nothing");

        let mut next = base.clone();
        next.fill_rect(Rect::new(20, 40, 50, 30), Rgba::WHITE);
        next.fill_rect(Rect::new(300, 470, 5, 5), Rgba::WHITE);
        s.present(&next).unwrap();
        let sent = &sent(&s)[before..];
        // Tile-aligned rect (16,32)-(79,79): 64 x 48 px = 6144 bytes = 2 chunks + 1024.
        let first = proto::display_bitmap(Rect::new(16, 32, 64, 48)).unwrap();
        assert_eq!(sent[0], first);
        assert_eq!(sent[1].len(), 2560);
        assert_eq!(sent[2].len(), 2560);
        assert_eq!(sent[3].len(), 1024);
        assert_eq!(&sent[3][1022..], &[0x00, 0x00], "black corner of the tile");
        // Second rect (288,464) 32 x 16 = 1024 bytes, one short chunk.
        let second = proto::display_bitmap(Rect::new(288, 464, 32, 16)).unwrap();
        assert_eq!(sent[4], second);
        assert_eq!(sent[5].len(), 1024);
        assert_eq!(sent.len(), 6);
    }

    #[test]
    fn orientation_is_on_device_and_pixels_are_not_rotated() {
        let mut s = connected(&[2; 6]);
        s.set_orientation(Orientation::Landscape).unwrap();
        let packet = sent(&s).last().unwrap().clone();
        assert_eq!(packet.len(), 16);
        assert_eq!(&packet[5..11], &[0x79, 0x66, 0x03, 0x20, 0x01, 0xe0]);
        let n = sent(&s).len();
        s.set_orientation(Orientation::Landscape).unwrap();
        assert_eq!(sent(&s).len(), n, "same orientation: nothing sent");

        let portrait = Frame::filled(Size::new(480, 800), Rgba::BLACK);
        assert!(s.present(&portrait).is_err());
        let mut wide = Frame::filled(Size::new(800, 480), Rgba::BLACK);
        wide.fill_rect(Rect::new(0, 0, 1, 1), Rgba::WHITE);
        s.present(&wide).unwrap();
        let sent_now = &sent(&s)[n..];
        assert_eq!(sent_now[0], proto::screen_on());
        assert_eq!(
            sent_now[1],
            proto::display_bitmap(Rect::new(0, 0, 800, 480)).unwrap()
        );
        // 800 x 480 x 2 bytes in chunks of 800 x 8.
        assert_eq!(sent_now.len() - 2, 120);
        assert!(sent_now[2..].iter().all(|c| c.len() == 6400));

        // Reverse landscape: the device flips; the same pixels go out, full again.
        s.set_orientation(Orientation::ReverseLandscape).unwrap();
        let m = sent(&s).len();
        assert_eq!(sent(&s)[m - 1][6], 0x67);
        s.present(&wide).unwrap();
        let flipped = &sent(&s)[m..];
        assert_eq!(
            flipped[0],
            proto::display_bitmap(Rect::new(0, 0, 800, 480)).unwrap()
        );
        assert_eq!(
            &flipped[1][..4],
            &[0xff, 0xff, 0x00, 0x00],
            "white stays top-left"
        );
    }

    #[test]
    fn first_frame_sends_the_default_orientation_once() {
        let mut s = connected(&[3; 6]);
        let frame = Frame::filled(Size::new(600, 1024), Rgba::WHITE);
        s.present(&frame).unwrap();
        let orientation: Vec<&Vec<u8>> = sent(&s).iter().filter(|p| p.len() == 16).collect();
        assert_eq!(
            orientation,
            [
                &proto::set_orientation(Orientation::Portrait, Size::new(600, 1024))
                    .unwrap()
                    .to_vec()
            ]
        );
        // 7" full frame: (0,0)-(599,1023).
        assert_eq!(
            sent(&s)[3],
            [0x00, 0x00, 0x09, 0x5f, 0xff, op::DISPLAY_BITMAP]
        );
        s.set_orientation(Orientation::Portrait).unwrap();
        assert_eq!(
            sent(&s).iter().filter(|p| p.len() == 16).count(),
            1,
            "portrait was already sent"
        );
    }

    #[test]
    fn brightness_off_on_and_release() {
        let mut s = connected(&[1; 6]);
        s.set_brightness(Brightness::new(25).unwrap()).unwrap();
        assert_eq!(sent(&s).last().unwrap(), &[0x2f, 0xc0, 0, 0, 0, 0x6e]);
        let frame = Frame::filled(Size::new(320, 480), Rgba::BLACK);
        s.present(&frame).unwrap();
        s.screen_off().unwrap();
        assert_eq!(sent(&s).last().unwrap(), &proto::screen_off());
        let n = sent(&s).len();
        s.present(&frame).unwrap();
        let wake = &sent(&s)[n..];
        assert_eq!(wake[0], proto::screen_on());
        assert_eq!(wake[1][5], op::DISPLAY_BITMAP, "full frame after waking");
        assert_eq!(wake.len(), 1 + 1 + 120);
        let n = sent(&s).len();
        s.release().unwrap();
        assert_eq!(sent(&s).len(), n, "release sends nothing");
        s.present(&frame).unwrap();
        assert_eq!(
            sent(&s)[n][5],
            op::DISPLAY_BITMAP,
            "full frame after release"
        );
        let commands = sent(&s).iter().filter(|p| p.len() == proto::COMMAND_LEN);
        assert!(
            commands
                .map(|p| p[5])
                .all(|o| o != op::RESET && o != op::CLEAR)
        );
    }
}
