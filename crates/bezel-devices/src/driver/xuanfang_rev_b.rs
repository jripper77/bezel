//! Driver for XuanFang rev B screens (3.5" rev B and "flagship").
//!
//! The device switches between portrait and landscape itself (`cb 00/01`);
//! the reverse orientations are done here by turning each bitmap 180° and
//! mirroring its rectangle. Every update is a DISPLAY_BITMAP: the whole screen
//! for the first frame, then the core's dirty rectangles, each followed by the
//! reference's 50 ms cooldown (the device is never listened to).

use std::time::Duration;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::{Frame, Rect, Rgba, dirty_rects};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};

use crate::driver::turing_rev_c::Pause;
use crate::protocol::xuanfang_rev_b::{self as proto, Hello, SubRevision};
use crate::wire::Wire;

/// How long the device may take to answer HELLO (the reference's read timeout).
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
/// Pause after every bitmap; fewer corrupted bitmaps (reference commit `953daec`).
pub const BITMAP_COOLDOWN: Duration = Duration::from_millis(50);
/// Level that turns the backlight back on when none was set: the reference's
/// `ScreenOn()` default.
pub const WAKE_BRIGHTNESS: Brightness = percent(25);
/// The reference's `ScreenOff()`: there is no off command, only brightness 0.
const OFF: Brightness = percent(0);

/// A constant level (`p` is always ≤ 100 here).
const fn percent(p: u8) -> Brightness {
    match Brightness::new(p) {
        Some(b) => b,
        None => Brightness::MAX,
    }
}

/// A connected rev B screen.
pub struct XuanFangRevB<W: Wire, P: Pause> {
    wire: W,
    pause: P,
    identity: ScreenIdentity,
    sub_revision: SubRevision,
    orientation: Orientation,
    /// Value (0 portrait / 1 landscape) the device was last told; `None` until sent.
    device_orientation: Option<u8>,
    /// Last level asked for through `set_brightness`.
    brightness: Option<Brightness>,
    /// The backlight is at the asked level; false at start and after `screen_off`.
    lit: bool,
    /// The frame the panel shows; `None` forces the next frame to be full.
    last: Option<Frame>,
}

fn io_err(e: std::io::Error) -> BezelError {
    BezelError::Transport(e.to_string())
}

impl<W: Wire, P: Pause + Clone> XuanFangRevB<W, P> {
    /// Handshakes over `wire`. `candidates` are the models discovery allowed;
    /// the HELLO sub-revision picks rev B or flagship. Only HELLO is sent: no
    /// clear (D-2026-09-30-device-protocols-2). `pause` paces the bitmaps.
    pub fn connect(mut wire: W, pause: &P, candidates: &[&'static DeviceModel]) -> Result<Self> {
        wire.discard_input().map_err(io_err)?;
        wire.send(&proto::hello()).map_err(io_err)?;
        let reply = wire
            .receive(proto::HELLO_REPLY_LEN, REPLY_TIMEOUT)
            .map_err(io_err)?;
        wire.discard_input().map_err(io_err)?;
        if reply.is_empty() {
            return Err(BezelError::Timeout(
                "the screen did not answer HELLO".into(),
            ));
        }
        let hello = Hello::parse(&reply);
        if !hello.is_well_formed() {
            tracing::warn!(reply = ?hello.raw, "unexpected HELLO framing");
        }
        let sub_revision = hello.sub_revision.unwrap_or_else(|| {
            tracing::warn!(reply = ?hello.raw, "no known sub-revision in HELLO; assuming A01");
            SubRevision::DEFAULT
        });
        tracing::debug!(reply = ?hello.raw, sub_revision = sub_revision.name(), "HELLO");
        let model = pick_model(sub_revision, candidates).ok_or_else(|| {
            BezelError::Transport(format!(
                "unexpected screen model: {} ({})",
                sub_revision.name(),
                sub_revision.model_id()
            ))
        })?;
        Ok(Self {
            wire,
            pause: pause.clone(),
            identity: ScreenIdentity {
                model,
                firmware: hello.sub_revision.map(|s| s.name().to_string()),
            },
            sub_revision,
            orientation: Orientation::Portrait,
            device_orientation: None,
            brightness: None,
            lit: false,
            last: None,
        })
    }
}

impl<W: Wire, P: Pause> XuanFangRevB<W, P> {
    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    /// The variant HELLO revealed (flagship, brightness range).
    pub fn sub_revision(&self) -> SubRevision {
        self.sub_revision
    }

    /// Backplate LED colour (flagship only; black turns them off).
    pub fn set_backplate_led(&mut self, color: Rgba) -> Result<()> {
        if !self.sub_revision.is_flagship() {
            return Err(BezelError::Transport(format!(
                "sub-revision {} has no backplate LEDs",
                self.sub_revision.name()
            )));
        }
        self.send(&proto::set_lighting(color.r, color.g, color.b))
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.wire.send(bytes).map_err(io_err)
    }

    fn send_brightness(&mut self, brightness: Brightness) -> Result<()> {
        let level = self.sub_revision.brightness_level(brightness);
        if !self.sub_revision.has_brightness_range() {
            tracing::debug!(
                percent = brightness.percent(),
                "sub-revision {} only switches the backlight on or off",
                self.sub_revision.name()
            );
        }
        self.send(&proto::set_brightness(level))
    }

    fn sync_orientation(&mut self) -> Result<()> {
        let value = proto::device_orientation(self.orientation);
        if self.device_orientation != Some(value) {
            self.send(&proto::set_orientation(self.orientation))?;
            self.device_orientation = Some(value);
        }
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

    /// One DISPLAY_BITMAP: header, the pixels in four-row chunks sent back to
    /// back, then the cooldown.
    fn bitmap(&mut self, frame: &Frame, rect: Rect) -> Result<()> {
        let screen = frame.size();
        let header = proto::display_bitmap(rect, screen, self.orientation).ok_or_else(|| {
            BezelError::Transport(format!("rectangle {rect:?} cannot be addressed"))
        })?;
        let reversed = proto::is_software_reversed(self.orientation);
        let pixels = proto::rgb565_be(frame.crop(rect).as_rgba(), reversed);
        self.send(&header)?;
        for chunk in pixels.chunks(proto::chunk_len(screen.width)) {
            self.send(chunk)?;
        }
        self.pause.pause(BITMAP_COOLDOWN);
        Ok(())
    }
}

impl<W: Wire, P: Pause> ScreenLink for XuanFangRevB<W, P> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    /// A01/A02 only switch the backlight: 0 % is off, anything else is full.
    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.send_brightness(brightness)?;
        self.brightness = Some(brightness);
        self.lit = true;
        Ok(())
    }

    /// Portrait/landscape go to the device; the reverse variants only change
    /// how bitmaps are built. The next frame is full either way.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        let value = proto::device_orientation(orientation);
        if orientation == self.orientation && self.device_orientation == Some(value) {
            return Ok(());
        }
        self.orientation = orientation;
        self.last = None;
        self.sync_orientation()
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        self.check_size(frame)?;
        if !self.lit {
            self.send_brightness(self.brightness.unwrap_or(WAKE_BRIGHTNESS))?;
            self.lit = true;
        }
        self.sync_orientation()?;
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

    /// There is no off command: the reference's `ScreenOff()` is brightness 0.
    fn screen_off(&mut self) -> Result<()> {
        self.last = None;
        self.lit = false;
        self.send_brightness(OFF)
    }

    /// Rev B has no standalone mode (no clock, no stored media) and no command
    /// that ends streaming: the panel keeps the last frame until the next host
    /// writes. Nothing is sent; the next frame, if any, is full.
    fn release(&mut self) -> Result<()> {
        self.last = None;
        Ok(())
    }
}

/// The model the sub-revision names among the discovery candidates, else a
/// candidate with the same panel (both variants are 320 x 480).
fn pick_model(
    sub_revision: SubRevision,
    candidates: &[&'static DeviceModel],
) -> Option<&'static DeviceModel> {
    let id = sub_revision.model_id();
    let found = candidates.iter().copied().find(|m| m.id == id);
    found.or_else(|| candidates.iter().copied().find(|m| m.panel == proto::PANEL))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::xuanfang_rev_b::op;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::geometry::Size;
    use std::sync::{Arc, Mutex};

    /// Records every pause instead of sleeping.
    #[derive(Clone, Default)]
    struct Clock(Arc<Mutex<Vec<Duration>>>);
    impl Pause for Clock {
        fn pause(&self, d: Duration) {
            self.0.lock().unwrap().push(d);
        }
    }
    impl Clock {
        fn pauses(&self) -> Vec<Duration> {
            self.0.lock().unwrap().clone()
        }
    }

    const PANEL: Size = Size::new(320, 480);

    fn model(id: &'static str) -> &'static DeviceModel {
        model_by_id(ModelId(id)).unwrap()
    }

    fn family() -> Vec<&'static DeviceModel> {
        vec![model("xuanfang-3.5"), model("xuanfang-3.5-flagship")]
    }

    fn answer(sub: u8) -> Vec<u8> {
        vec![0xca, b'H', b'E', b'L', b'L', b'O', 0x0a, sub, 0x00, 0xca]
    }

    fn connected(sub: u8) -> (XuanFangRevB<ScriptedWire, Clock>, Clock) {
        let clock = Clock::default();
        let wire = ScriptedWire::with_replies([answer(sub)]);
        let s = XuanFangRevB::connect(wire, &clock, &family()).unwrap();
        (s, clock)
    }

    fn sent(s: &XuanFangRevB<ScriptedWire, Clock>) -> &[Vec<u8>] {
        &s.wire().sent
    }

    fn packet(opcode: u8, payload: &[u8]) -> Vec<u8> {
        proto::packet(opcode, payload).unwrap().to_vec()
    }

    #[test]
    fn hello_picks_flagship_and_brightness_range_and_nothing_else_is_sent() {
        for (code, id, name) in [
            (0x01, "xuanfang-3.5", "A01"),
            (0x02, "xuanfang-3.5-flagship", "A02"),
            (0x11, "xuanfang-3.5", "A11"),
            (0x12, "xuanfang-3.5-flagship", "A12"),
        ] {
            let (s, _) = connected(code);
            assert_eq!(s.identity().model.id.0, id);
            assert_eq!(s.identity().firmware.as_deref(), Some(name));
            assert_eq!(s.sub_revision().name(), name);
            assert_eq!(sent(&s), &[proto::hello().to_vec()]);
            assert_eq!(s.wire().discards, 2, "input is flushed before and after");
        }
    }

    #[test]
    fn silence_fails_and_odd_answers_fall_back_to_a01() {
        let wire = ScriptedWire::default();
        let err = XuanFangRevB::connect(wire, &Clock::default(), &family())
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Timeout(_)));

        let wire = ScriptedWire::with_replies([b"garbage".to_vec()]);
        let s = XuanFangRevB::connect(wire, &Clock::default(), &family()).unwrap();
        assert_eq!(s.sub_revision(), SubRevision::A01);
        assert_eq!(s.identity().model.id.0, "xuanfang-3.5");
        assert_eq!(s.identity().firmware, None);
    }

    #[test]
    fn model_resolution_among_narrow_candidates() {
        let plain = model("xuanfang-3.5");
        let flagship = model("xuanfang-3.5-flagship");
        assert_eq!(
            pick_model(SubRevision::A12, &[plain]).map(|m| m.id.0),
            Some("xuanfang-3.5")
        );
        assert_eq!(
            pick_model(SubRevision::A01, &[flagship]).map(|m| m.id.0),
            Some("xuanfang-3.5-flagship")
        );
        let big = model("usbpcmonitor-5");
        assert!(pick_model(SubRevision::A11, &[big]).is_none());
        let wire = ScriptedWire::with_replies([answer(0x11)]);
        let err = XuanFangRevB::connect(wire, &Clock::default(), &[big])
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Transport(ref m) if m.contains("A11")));
    }

    #[test]
    fn first_frame_lights_orients_and_sends_one_full_bitmap() {
        let (mut s, clock) = connected(0x12);
        let mut frame = Frame::filled(PANEL, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        s.present(&frame).unwrap();
        let sent = &sent(&s)[1..];
        // No brightness was asked for: the reference's ScreenOn level, 25 % -> 0x3f.
        assert_eq!(sent[0], packet(op::SET_BRIGHTNESS, &[0x3f]));
        assert_eq!(sent[1], packet(op::SET_ORIENTATION, &[0]));
        // The golden file: cc 00 00 00 00 01 3f 01 df cc, then 120 writes of 2560 bytes.
        assert_eq!(
            sent[2],
            [0xcc, 0x00, 0x00, 0x00, 0x00, 0x01, 0x3f, 0x01, 0xdf, 0xcc]
        );
        let data = &sent[3..];
        assert_eq!(data.len(), 120);
        assert!(data.iter().all(|c| c.len() == 2560));
        assert_eq!(&data[0][..4], &[0xf8, 0x00, 0x00, 0x00], "red first, BE");
        assert_eq!(clock.pauses(), vec![BITMAP_COOLDOWN]);
    }

    #[test]
    fn later_frames_send_dirty_rectangles_each_with_a_cooldown() {
        let (mut s, clock) = connected(0x11);
        s.set_brightness(Brightness::new(50).unwrap()).unwrap();
        let base = Frame::filled(PANEL, Rgba::BLACK);
        s.present(&base).unwrap();
        let before = sent(&s).len();
        s.present(&base).unwrap();
        assert_eq!(sent(&s).len(), before, "an unchanged frame sends nothing");
        assert_eq!(clock.pauses().len(), 1);

        let mut next = base.clone();
        next.fill_rect(Rect::new(0, 0, 3, 3), Rgba::WHITE);
        next.fill_rect(Rect::new(300, 470, 5, 5), Rgba::WHITE);
        s.present(&next).unwrap();
        let sent = &sent(&s)[before..];
        assert_eq!(
            sent[0],
            packet(op::DISPLAY_BITMAP, &[0, 0, 0, 0, 0, 15, 0, 15])
        );
        assert_eq!(sent[1].len(), 16 * 16 * 2);
        assert_eq!(&sent[1][..2], &[0xff, 0xff]);
        assert_eq!(
            sent[2],
            packet(
                op::DISPLAY_BITMAP,
                &[0x01, 0x20, 0x01, 0xd0, 0x01, 0x3f, 0x01, 0xdf]
            )
        );
        assert_eq!(sent[3].len(), 32 * 16 * 2);
        assert_eq!(sent.len(), 4);
        assert_eq!(clock.pauses(), vec![BITMAP_COOLDOWN; 3]);
    }

    #[test]
    fn reverse_orientations_mirror_the_rectangle_and_turn_the_pixels() {
        let (mut s, _) = connected(0x11);
        s.set_orientation(Orientation::ReversePortrait).unwrap();
        assert_eq!(sent(&s).last().unwrap(), &packet(op::SET_ORIENTATION, &[0]));
        let base = Frame::filled(PANEL, Rgba::BLACK);
        s.present(&base).unwrap();
        let mut next = base.clone();
        next.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        let before = sent(&s).len();
        s.present(&next).unwrap();
        let sent_now = &sent(&s)[before..];
        // Top-left tile of the canvas is the device's bottom-right tile (304,464)-(319,479).
        assert_eq!(
            sent_now[0],
            packet(
                op::DISPLAY_BITMAP,
                &[0x01, 0x30, 0x01, 0xd0, 0x01, 0x3f, 0x01, 0xdf]
            )
        );
        let data = &sent_now[1];
        assert_eq!(
            &data[data.len() - 2..],
            &[0xf8, 0x00],
            "red pixel sent last"
        );
        assert!(data[..data.len() - 2].iter().all(|&b| b == 0));

        // Reverse portrait -> portrait: the device value stays 0, nothing is
        // sent until the full frame, which is upright again.
        let n = sent(&s).len();
        s.set_orientation(Orientation::Portrait).unwrap();
        assert_eq!(sent(&s).len(), n);
        s.present(&next).unwrap();
        assert_eq!(sent(&s)[n][0], op::DISPLAY_BITMAP);
        assert_eq!(&sent(&s)[n + 1][..2], &[0xf8, 0x00], "red pixel first");
    }

    #[test]
    fn landscape_goes_to_the_device_and_reverse_landscape_stays_in_software() {
        let (mut s, _) = connected(0x01);
        s.set_orientation(Orientation::Landscape).unwrap();
        assert_eq!(sent(&s).last().unwrap(), &packet(op::SET_ORIENTATION, &[1]));
        let tall = Frame::filled(PANEL, Rgba::BLACK);
        assert!(s.present(&tall).is_err());
        let wide = Frame::filled(PANEL.transposed(), Rgba::WHITE);
        s.present(&wide).unwrap();
        let n = sent(&s).len();
        // 480 x 320 x 2 bytes in chunks of 480 x 8.
        assert!(sent(&s)[n - 80..].iter().all(|c| c.len() == 3840));
        assert_eq!(
            sent(&s)[n - 81],
            packet(op::DISPLAY_BITMAP, &[0, 0, 0, 0, 0x01, 0xdf, 0x01, 0x3f])
        );
        s.set_orientation(Orientation::ReverseLandscape).unwrap();
        assert_eq!(sent(&s).len(), n, "still landscape on the device");
        s.present(&wide).unwrap();
        assert_eq!(sent(&s).len(), n + 81, "full frame after the change");
        s.set_orientation(Orientation::ReverseLandscape).unwrap();
        s.set_orientation(Orientation::Portrait).unwrap();
        assert_eq!(sent(&s).last().unwrap(), &packet(op::SET_ORIENTATION, &[0]));
    }

    #[test]
    fn brightness_follows_the_sub_revision_and_screen_off_is_brightness_zero() {
        let level = |sub: u8, p: u8| {
            let (mut s, _) = connected(sub);
            s.set_brightness(Brightness::new(p).unwrap()).unwrap();
            sent(&s).last().unwrap()[1]
        };
        assert_eq!(level(0x12, 25), 0x3f);
        assert_eq!(level(0x11, 100), 0xff);
        assert_eq!(level(0x01, 25), 0x00);
        assert_eq!(level(0x02, 0), 0x01);

        let (mut s, _) = connected(0x12);
        s.set_brightness(Brightness::new(80).unwrap()).unwrap();
        let frame = Frame::filled(PANEL, Rgba::BLACK);
        s.present(&frame).unwrap();
        s.screen_off().unwrap();
        assert_eq!(sent(&s).last().unwrap(), &packet(op::SET_BRIGHTNESS, &[0]));
        let n = sent(&s).len();
        s.present(&frame).unwrap();
        // Waking restores the asked level (80 % -> 204) and resends the whole frame.
        assert_eq!(sent(&s)[n], packet(op::SET_BRIGHTNESS, &[204]));
        assert_eq!(sent(&s)[n + 1][..2], [op::DISPLAY_BITMAP, 0]);
        assert_eq!(sent(&s).len(), n + 2 + 120);

        let (mut s, _) = connected(0x01);
        s.screen_off().unwrap();
        assert_eq!(sent(&s).last().unwrap(), &packet(op::SET_BRIGHTNESS, &[1]));
        s.present(&frame).unwrap();
        assert_eq!(
            sent(&s)[2],
            packet(op::SET_BRIGHTNESS, &[0]),
            "on/off units wake at full"
        );
    }

    #[test]
    fn leds_release_and_no_clear() {
        let (mut s, _) = connected(0x12);
        s.set_backplate_led(Rgba::opaque(0x11, 0x22, 0x33)).unwrap();
        assert_eq!(
            sent(&s).last().unwrap(),
            &packet(op::SET_LIGHTING, &[0x11, 0x22, 0x33])
        );
        let frame = Frame::filled(PANEL, Rgba::BLACK);
        s.present(&frame).unwrap();
        let n = sent(&s).len();
        s.release().unwrap();
        assert_eq!(sent(&s).len(), n, "release sends nothing");
        s.present(&frame).unwrap();
        assert_eq!(
            sent(&s)[n][0],
            op::DISPLAY_BITMAP,
            "full frame after release"
        );
        assert_eq!(sent(&s).len(), n + 121);

        let (mut plain, _) = connected(0x11);
        assert!(plain.set_backplate_led(Rgba::WHITE).is_err());
        assert_eq!(sent(&plain).len(), 1);
    }
}
