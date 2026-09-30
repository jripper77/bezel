//! Driver for the Kipye Qiye 3.5" (rev D).
//!
//! Choices, all from `docs/reverse-engineering/protocol-kipye-rev-d.md`:
//! - There is no handshake: connecting only drops pending input. It never
//!   clears, resets or re-orients the screen (D-2026-09-30-device-protocols-2).
//! - Acknowledgements are never read; input is dropped after every command
//!   (not after pixel packets), as the reference does.
//! - Brightness is `percent × 5`, sent twice.
//! - Reverse orientations are turned 180° by the device (SET180); landscape is
//!   done here: pixels turned 90° clockwise and the window mapped
//!   ([`proto::window`]).
//! - The first frame, and the first after an orientation change or
//!   `screen_off`, covers the whole canvas; later frames send one window per
//!   dirty rectangle. A window is always sent as one sequence (BLOCKWRITE,
//!   INTOPICMODE, packets, OUTPICMODE), several 64-byte packets per write so
//!   every packet still starts on a USB packet boundary.
//! - `screen_off` is the reference's: brightness 0. The next frame restores the
//!   last level set (25 %, the reference's `ScreenOn` level, when none was).
//! - `release` sends nothing: rev D has no standalone mode and the reference
//!   sends nothing when it stops. The panel keeps the last frame.

use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::frame::{Frame, Rect, dirty_rects};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};

use crate::driver::{Pause, check_frame_size, io_err};
use crate::protocol::kipye_rev_d::{self as proto, op};
use crate::wire::Wire;

/// Pixel packets per write: 4 KiB, a whole number of 64-byte packets.
const PACKETS_PER_WRITE: usize = 64;
/// Level (percent) restored after `screen_off` when none was set (the
/// reference's `ScreenOn`).
const WAKE_PERCENT: u8 = 25;

/// A connected rev D screen.
pub struct KipyeRevD<W: Wire> {
    wire: W,
    identity: ScreenIdentity,
    orientation: Orientation,
    last: Option<Frame>,
    /// Last level set, in percent.
    brightness: Option<u8>,
    /// Off by `screen_off`; the next frame restores the backlight.
    dark: bool,
}

impl<W: Wire> KipyeRevD<W> {
    /// Takes over a rev D screen. There is no handshake: only pending input is
    /// dropped, and nothing is written. `candidates` are the models discovery
    /// allowed; the first rev D one is used. The pause is unused (no step of
    /// this family waits); it keeps the drivers' `connect` calls alike.
    pub fn connect<P: Pause>(
        mut wire: W,
        _pause: &P,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        let model = candidates
            .iter()
            .copied()
            .find(|m| m.family == Family::KipyeRevD)
            .ok_or_else(|| BezelError::Transport("no Kipye rev D model to connect".into()))?;
        wire.discard_input().map_err(io_err)?;
        Ok(Self {
            wire,
            identity: ScreenIdentity {
                model,
                firmware: None,
            },
            orientation: Orientation::Portrait,
            last: None,
            brightness: None,
            dark: false,
        })
    }

    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    /// A command write, then the acknowledgement (if any) is dropped.
    fn command(&mut self, bytes: &[u8]) -> Result<()> {
        self.wire.send(bytes).map_err(io_err)?;
        self.wire.discard_input().map_err(io_err)
    }

    fn backlight(&mut self, percent: u8) -> Result<()> {
        let packet = proto::set_backlight(proto::backlight_level(percent));
        for _ in 0..proto::BACKLIGHT_REPEATS {
            self.command(&packet)?;
        }
        Ok(())
    }

    fn canvas(&self) -> Size {
        self.identity.model.panel.in_orientation(self.orientation)
    }

    /// One window: BLOCKWRITE, INTOPICMODE, the pixel packets, OUTPICMODE.
    fn bitmap(&mut self, frame: &Frame, rect: Rect) -> Result<()> {
        let panel_width = self.identity.model.panel.width;
        let window = proto::window(rect, self.orientation, panel_width).ok_or_else(|| {
            BezelError::Transport(format!(
                "rectangle {}x{} at ({}, {}) is outside the panel",
                rect.width, rect.height, rect.x, rect.y
            ))
        })?;
        let mut pixels = frame.crop(rect);
        if self.orientation.is_landscape() {
            pixels = pixels.rotated(1);
        }
        let packets = proto::data_packets(&proto::rgb565_be(pixels.as_rgba()));
        self.command(&proto::block_write(window))?;
        self.command(&op::INTO_PIC_MODE)?;
        for batch in packets.chunks(proto::PACKET * PACKETS_PER_WRITE) {
            self.wire.send(batch).map_err(io_err)?;
        }
        self.command(&op::OUT_PIC_MODE)
    }
}

impl<W: Wire> ScreenLink for KipyeRevD<W> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.backlight(brightness.percent())?;
        self.brightness = Some(brightness.percent());
        self.dark = false;
        Ok(())
    }

    /// Always sends SETORG or SET180 (the device state is unknown after
    /// connecting); a change of orientation makes the next frame full.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.command(&proto::set_orientation(orientation))?;
        if orientation != self.orientation {
            self.orientation = orientation;
            self.last = None;
        }
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        let canvas = self.canvas();
        check_frame_size(frame, canvas)?;
        if self.dark {
            self.backlight(self.brightness.unwrap_or(WAKE_PERCENT))?;
            self.dark = false;
        }
        let rects = match self.last.take() {
            Some(last) => dirty_rects(&last, frame),
            None => vec![Rect::of(canvas)],
        };
        for rect in rects {
            self.bitmap(frame, rect)?;
        }
        tracing::trace!("rev D frame sent");
        self.last = Some(frame.clone());
        Ok(())
    }

    fn screen_off(&mut self) -> Result<()> {
        self.backlight(0)?;
        self.dark = true;
        self.last = None;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        self.last = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::Rgba;
    use std::time::Duration;

    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

    fn kipye() -> &'static DeviceModel {
        model_by_id(ModelId("kipye-qiye-3.5")).unwrap()
    }

    fn connected() -> KipyeRevD<ScriptedWire> {
        KipyeRevD::connect(ScriptedWire::default(), &NoPause, &[kipye()]).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Writes since `from`, as hex.
    fn sent_since(s: &KipyeRevD<ScriptedWire>, from: usize) -> Vec<String> {
        s.wire().sent[from..].iter().map(|p| hex(p)).collect()
    }

    /// Every pixel packet of the writes since `from`, re-split on 64-byte boundaries.
    fn packets_since(s: &KipyeRevD<ScriptedWire>, from: usize) -> Vec<Vec<u8>> {
        s.wire().sent[from..]
            .iter()
            .filter(|w| w.first() == Some(&op::DATA))
            .flat_map(|w| w.chunks(proto::PACKET).map(<[u8]>::to_vec))
            .collect()
    }

    #[test]
    fn connect_writes_nothing_and_only_drops_input() {
        let s = connected();
        assert!(s.wire().sent.is_empty(), "no clear, reset or orientation");
        assert_eq!(s.wire().discards, 1);
        assert_eq!(s.identity().model.id, ModelId("kipye-qiye-3.5"));
        assert_eq!(s.identity().firmware, None);
    }

    #[test]
    fn connect_needs_a_rev_d_candidate() {
        let other = model_by_id(ModelId("weact-fs-3.5")).unwrap();
        let err = KipyeRevD::connect(ScriptedWire::default(), &NoPause, &[other])
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Transport(_)));
        let s = KipyeRevD::connect(ScriptedWire::default(), &NoPause, &[other, kipye()]).unwrap();
        assert!(s.wire().sent.is_empty());
    }

    #[test]
    fn brightness_is_sent_twice_with_input_dropped_after_each() {
        let mut s = connected();
        for (percent, packet) in [(0, "43430000"), (25, "4343007d"), (100, "434301f4")] {
            let before = (s.wire().sent.len(), s.wire().discards);
            s.set_brightness(Brightness::new(percent).unwrap()).unwrap();
            assert_eq!(sent_since(&s, before.0), [packet, packet]);
            assert_eq!(s.wire().discards, before.1 + 2);
        }
    }

    #[test]
    fn orientation_commands_are_device_180_only() {
        let mut s = connected();
        for (o, packet) in [
            (Orientation::Portrait, "43480000"),
            (Orientation::ReversePortrait, "43470000"),
            (Orientation::Landscape, "43480000"),
            (Orientation::ReverseLandscape, "43470000"),
        ] {
            let before = s.wire().sent.len();
            s.set_orientation(o).unwrap();
            assert_eq!(sent_since(&s, before), [packet]);
        }
    }

    #[test]
    fn first_frame_is_full_then_only_dirty_windows() {
        let mut s = connected();
        let mut frame = Frame::filled(kipye().panel, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        s.present(&frame).unwrap();
        let sent = sent_since(&s, 0);
        assert_eq!(sent[0], "43410000013f000001df", "window 0..319 x 0..479");
        assert_eq!(sent[1], "44000000");
        assert_eq!(sent.last().unwrap(), "41000000");
        let packets = packets_since(&s, 0);
        assert_eq!(packets.len(), 4877);
        assert!(
            packets[..4876]
                .iter()
                .all(|p| p.len() == 64 && p[0] == op::DATA)
        );
        assert_eq!(packets[4876].len(), 13);
        assert_eq!(&packets[0][..5], &[op::DATA, 0xF8, 0x00, 0x00, 0x00]);
        // Every write of pixels is a whole number of packets (but the last).
        let writes: Vec<usize> = s.wire().sent[2..s.wire().sent.len() - 1]
            .iter()
            .map(Vec::len)
            .collect();
        assert!(writes[..writes.len() - 1].iter().all(|n| n % 64 == 0));
        // Commands are followed by a discard; pixel packets are not.
        assert_eq!(s.wire().discards, 1 + 3);

        // Unchanged frame: nothing is sent.
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(s.wire().sent.len(), before);

        // One changed pixel: one tile-sized window.
        let mut next = frame.clone();
        next.fill_rect(Rect::new(20, 40, 1, 1), Rgba::WHITE);
        s.present(&next).unwrap();
        let sent = sent_since(&s, before);
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[0], "43410010001f0020002f", "16x16 tile at (16, 32)");
        let packets = packets_since(&s, before);
        let payload: usize = packets.iter().map(|p| p.len() - 1).sum();
        assert_eq!(payload, 16 * 16 * 2);
    }

    #[test]
    fn landscape_frames_are_rotated_and_mapped() {
        let mut s = connected();
        s.set_orientation(Orientation::ReverseLandscape).unwrap();
        let wide = kipye().panel.in_orientation(Orientation::Landscape);
        let mut frame = Frame::filled(wide, Rgba::BLACK);
        // Top-left pixel of the landscape canvas becomes the panel's top-right.
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::WHITE);
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        let sent = sent_since(&s, before);
        assert_eq!(sent[0], "43410000013f000001df");
        let data: Vec<u8> = packets_since(&s, before)
            .iter()
            .flat_map(|p| p[1..].to_vec())
            .collect();
        assert_eq!(&data[319 * 2..320 * 2], &[0xFF, 0xFF]);
        assert_eq!(&data[..2], &[0, 0]);

        // A change at (10, 20) of the landscape canvas: tile (0, 16) 16x16.
        let mut next = frame.clone();
        next.fill_rect(Rect::new(10, 20, 3, 2), Rgba::WHITE);
        let before = s.wire().sent.len();
        s.present(&next).unwrap();
        // x0 = 320 - 16 - 16 = 288, x1 = 303, y0 = 0, y1 = 15.
        assert_eq!(sent_since(&s, before)[0], "43410120012f0000000f");

        // A portrait frame no longer fits.
        let tall = Frame::filled(kipye().panel, Rgba::BLACK);
        assert!(s.present(&tall).is_err());
    }

    #[test]
    fn orientation_change_forces_a_full_frame() {
        let mut s = connected();
        let frame = Frame::filled(kipye().panel, Rgba::BLACK);
        s.present(&frame).unwrap();
        s.set_orientation(Orientation::ReversePortrait).unwrap();
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(sent_since(&s, before)[0], "43410000013f000001df");
        // Same orientation again: the command is re-sent, the frame state kept.
        s.set_orientation(Orientation::ReversePortrait).unwrap();
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(s.wire().sent.len(), before);
    }

    #[test]
    fn screen_off_is_brightness_zero_until_the_next_frame() {
        let mut s = connected();
        let frame = Frame::filled(kipye().panel, Rgba::BLACK);
        s.present(&frame).unwrap();
        s.screen_off().unwrap();
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        let sent = sent_since(&s, before);
        // Never set: the reference's ScreenOn level (25 %), then a full frame.
        assert_eq!(&sent[..3], ["4343007d", "4343007d", "43410000013f000001df"]);

        s.set_brightness(Brightness::new(60).unwrap()).unwrap();
        let before = s.wire().sent.len();
        s.screen_off().unwrap();
        assert_eq!(sent_since(&s, before), ["43430000", "43430000"]);
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(&sent_since(&s, before)[..2], ["4343012c", "4343012c"]);

        // Setting a level while dark wakes the panel; no second restore.
        s.screen_off().unwrap();
        s.set_brightness(Brightness::new(100).unwrap()).unwrap();
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(sent_since(&s, before)[0], "43410000013f000001df");
    }

    #[test]
    fn release_sends_nothing_and_forgets_the_frame() {
        let mut s = connected();
        let frame = Frame::filled(kipye().panel, Rgba::BLACK);
        s.present(&frame).unwrap();
        let before = s.wire().sent.len();
        s.release().unwrap();
        assert_eq!(s.wire().sent.len(), before);
        s.present(&frame).unwrap();
        assert_eq!(sent_since(&s, before)[0], "43410000013f000001df");
    }

    #[test]
    fn a_window_outside_the_panel_is_an_error() {
        let mut s = connected();
        let frame = Frame::filled(kipye().panel, Rgba::BLACK);
        s.set_orientation(Orientation::Landscape).unwrap();
        let before = s.wire().sent.len();
        assert!(s.bitmap(&frame, Rect::new(0, 400, 16, 16)).is_err());
        assert_eq!(s.wire().sent.len(), before, "nothing is sent");
        let small = Frame::filled(Size::new(10, 10), Rgba::BLACK);
        assert!(s.present(&small).is_err());
    }
}
