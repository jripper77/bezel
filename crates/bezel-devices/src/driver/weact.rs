//! Driver for the WeAct Studio Display FS V1 (3.5" and 0.96").
//!
//! Choices, all from `docs/reverse-engineering/protocol-weact.md`:
//! - Handshake: stale input is drained, `c2 0a` sent, up to 19 bytes read and
//!   the rest of the input dropped; bytes 1..9 are the firmware version. A
//!   missing or short answer is logged and tolerated, as in the reference.
//!   Nothing else is written: connecting never clears or resets the screen
//!   (D-2026-09-30-device-protocols-2).
//! - Brightness is `percent × 255 / 100`, truncated like the reference, and
//!   remembered.
//! - The device rotates (`02 <o> 0a`); frames are sent in the coordinates and
//!   pixel order of the current orientation. A change makes the next frame full.
//! - The first frame, and the first after an orientation change, `screen_off`
//!   or `release`, covers the whole canvas; later frames send one bitmap per
//!   dirty rectangle: the header in its own write, then the RGB565 LE pixels in
//!   writes of `W × 4` bytes, as the reference does. A rectangle that does not
//!   fit the canvas is an error (the reference asserts; nothing is clipped).
//! - `screen_off` is the reference's: brightness 0, humidity reports off (3.5"
//!   only) and FREE. Unlike the reference it keeps the remembered level, and
//!   the next frame restores it (25 % when none was set).
//! - `release` sends FREE only, the last command the reference sends when it
//!   stops; the brightness is left alone.

use std::time::Duration;

use bezel_core::domain::device::{DeviceModel, Family, ModelId};
use bezel_core::domain::frame::{Frame, Rect, dirty_rects};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::ScreenLink;
use bezel_core::{BezelError, Result};

use crate::driver::{Pause, check_frame_size, io_err};
use crate::protocol::weact as proto;
use crate::wire::Wire;

/// How long stale input may keep arriving before it is dropped.
const DRAIN_WAIT: Duration = Duration::from_millis(100);
/// How long the device may take to answer SYSTEM_VERSION (the reference's read timeout).
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
/// Level (percent) restored after `screen_off` when none was set.
const WAKE_PERCENT: u8 = 25;
/// The model with the temperature/humidity sensor.
const SENSOR_MODEL: ModelId = ModelId("weact-fs-3.5");

/// A connected WeAct screen.
pub struct WeAct<W: Wire> {
    wire: W,
    identity: ScreenIdentity,
    humidity_sensor: bool,
    orientation: Orientation,
    last: Option<Frame>,
    /// Last level set, in percent.
    brightness: Option<u8>,
    /// Off by `screen_off`; the next frame restores the backlight.
    dark: bool,
}

impl<W: Wire> WeAct<W> {
    /// Reads the firmware version over `wire`. `candidates` are the models
    /// discovery allowed (the USB serial prefix tells the sizes apart); the
    /// first WeAct one is used.
    pub fn connect<P: Pause>(
        mut wire: W,
        pause: &P,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        let model = candidates
            .iter()
            .copied()
            .find(|m| m.family == Family::WeAct)
            .ok_or_else(|| BezelError::Transport("no WeAct model to connect".into()))?;
        let firmware = handshake(&mut wire, pause)?;
        Ok(Self {
            wire,
            identity: ScreenIdentity { model, firmware },
            humidity_sensor: model.id == SENSOR_MODEL,
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

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.wire.send(bytes).map_err(io_err)
    }

    fn backlight(&mut self, percent: u8) -> Result<()> {
        self.send(&proto::set_brightness(proto::brightness_level(percent)))
    }

    fn canvas(&self) -> Size {
        self.identity.model.panel.in_orientation(self.orientation)
    }

    /// One bitmap: the header, then the pixels in `W × 4`-byte writes.
    fn bitmap(&mut self, frame: &Frame, rect: Rect) -> Result<()> {
        let canvas = self.canvas();
        let header = proto::bitmap_header(rect, canvas).ok_or_else(|| {
            BezelError::Transport(format!(
                "rectangle {}x{} at ({}, {}) does not fit the {}x{} canvas",
                rect.width, rect.height, rect.x, rect.y, canvas.width, canvas.height
            ))
        })?;
        let pixels = proto::rgb565_le(frame.crop(rect).as_rgba());
        self.send(&header)?;
        for chunk in pixels.chunks(proto::data_chunk_len(canvas)) {
            self.send(chunk)?;
        }
        Ok(())
    }
}

impl<W: Wire> ScreenLink for WeAct<W> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.backlight(brightness.percent())?;
        self.brightness = Some(brightness.percent());
        self.dark = false;
        Ok(())
    }

    /// Always sends the orientation (the device state is unknown after
    /// connecting); a change of orientation makes the next frame full.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.send(&proto::set_orientation(orientation))?;
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
        tracing::trace!("WeAct frame sent");
        self.last = Some(frame.clone());
        Ok(())
    }

    fn screen_off(&mut self) -> Result<()> {
        self.backlight(0)?;
        if self.humidity_sensor {
            self.send(&proto::sensor_report_off())?;
        }
        self.send(&proto::free())?;
        self.dark = true;
        self.last = None;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        self.last = None;
        self.send(&proto::free())
    }
}

/// Drains stale input, asks SYSTEM_VERSION and returns the version, if any.
fn handshake<W: Wire, P: Pause>(wire: &mut W, pause: &P) -> Result<Option<String>> {
    pause.pause(DRAIN_WAIT);
    wire.discard_input().map_err(io_err)?;
    wire.send(&proto::system_version()).map_err(io_err)?;
    let reply = read_reply(wire, proto::VERSION_REPLY_LEN)?;
    wire.discard_input().map_err(io_err)?;
    let version = proto::parse_version(&reply);
    match &version {
        Some(v) => tracing::debug!(version = %v, "SYSTEM_VERSION"),
        None => tracing::debug!(bytes = reply.len(), "no SYSTEM_VERSION answer; continuing"),
    }
    Ok(version)
}

/// Up to `len` bytes, gathered until the device goes quiet.
fn read_reply<W: Wire>(wire: &mut W, len: usize) -> Result<Vec<u8>> {
    let mut reply = Vec::with_capacity(len);
    while reply.len() < len {
        let chunk = wire
            .receive(len - reply.len(), REPLY_TIMEOUT)
            .map_err(io_err)?;
        if chunk.is_empty() {
            break;
        }
        reply.extend_from_slice(&chunk);
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::frame::Rgba;

    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

    fn big() -> &'static DeviceModel {
        model_by_id(ModelId("weact-fs-3.5")).unwrap()
    }

    fn small() -> &'static DeviceModel {
        model_by_id(ModelId("weact-fs-0.96")).unwrap()
    }

    fn version_reply() -> Vec<u8> {
        let mut reply = vec![0xC2];
        reply.extend_from_slice(b"V1.0.0.0");
        reply.resize(proto::VERSION_REPLY_LEN, 0);
        reply
    }

    fn connected(model: &'static DeviceModel) -> WeAct<ScriptedWire> {
        let wire = ScriptedWire::with_replies([version_reply()]);
        WeAct::connect(wire, &NoPause, &[model]).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Writes since `from`, as hex.
    fn sent_since(s: &WeAct<ScriptedWire>, from: usize) -> Vec<String> {
        s.wire().sent[from..].iter().map(|p| hex(p)).collect()
    }

    #[test]
    fn connect_reads_the_version_and_writes_nothing_else() {
        let s = connected(big());
        assert_eq!(
            sent_since(&s, 0),
            ["c20a"],
            "no clear, reset or orientation"
        );
        assert_eq!(s.wire().discards, 2, "drained before, flushed after");
        assert_eq!(s.identity().firmware.as_deref(), Some("V1.0.0.0"));
        assert_eq!(s.identity().model.id, ModelId("weact-fs-3.5"));
        assert!(s.humidity_sensor);
    }

    #[test]
    fn a_split_or_missing_version_answer() {
        let reply = version_reply();
        let wire = ScriptedWire::with_replies([reply[..10].to_vec(), reply[10..].to_vec()]);
        let s = WeAct::connect(wire, &NoPause, &[small()]).unwrap();
        assert_eq!(s.identity().firmware.as_deref(), Some("V1.0.0.0"));
        assert!(!s.humidity_sensor);

        // No answer: tolerated, as in the reference.
        let s = WeAct::connect(ScriptedWire::default(), &NoPause, &[small()]).unwrap();
        assert_eq!(s.identity().firmware, None);
        assert_eq!(sent_since(&s, 0), ["c20a"]);
    }

    #[test]
    fn connect_needs_a_weact_candidate() {
        let other = model_by_id(ModelId("kipye-qiye-3.5")).unwrap();
        let err = WeAct::connect(ScriptedWire::default(), &NoPause, &[other])
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Transport(_)));
        let s = WeAct::connect(ScriptedWire::default(), &NoPause, &[other, small()]).unwrap();
        assert_eq!(s.identity().model.id, ModelId("weact-fs-0.96"));
    }

    #[test]
    fn brightness_is_scaled_to_255_and_remembered() {
        let mut s = connected(big());
        for (percent, packet) in [(0, "0300e8030a"), (25, "033fe8030a"), (100, "03ffe8030a")] {
            let before = s.wire().sent.len();
            s.set_brightness(Brightness::new(percent).unwrap()).unwrap();
            assert_eq!(sent_since(&s, before), [packet]);
            assert_eq!(s.brightness, Some(percent));
        }
    }

    #[test]
    fn orientation_is_on_the_device() {
        let mut s = connected(big());
        for (o, packet) in [
            (Orientation::Portrait, "02000a"),
            (Orientation::Landscape, "02020a"),
            (Orientation::ReversePortrait, "02010a"),
            (Orientation::ReverseLandscape, "02030a"),
        ] {
            let before = s.wire().sent.len();
            s.set_orientation(o).unwrap();
            assert_eq!(sent_since(&s, before), [packet]);
        }
    }

    #[test]
    fn first_frame_is_full_then_one_bitmap_per_dirty_rect() {
        let mut s = connected(big());
        let mut frame = Frame::filled(big().panel, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        let sent = &s.wire().sent[before..];
        assert_eq!(hex(&sent[0]), "05000000003f01df010a");
        assert_eq!(
            sent.len(),
            1 + 240,
            "320 x 480 x 2 bytes in 1280-byte writes"
        );
        assert!(sent[1..].iter().all(|w| w.len() == 1280));
        assert_eq!(&sent[1][..4], &[0x00, 0xF8, 0x00, 0x00], "RGB565 LE");

        // Unchanged frame: nothing is sent.
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(s.wire().sent.len(), before);

        // Two separate changes: two bitmaps of one tile each.
        let mut next = frame.clone();
        next.fill_rect(Rect::new(20, 40, 1, 1), Rgba::WHITE);
        next.fill_rect(Rect::new(300, 470, 1, 1), Rgba::WHITE);
        s.present(&next).unwrap();
        let sent = sent_since(&s, before);
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[0], "05100020001f002f000a", "tile (16, 32)..(31, 47)");
        assert_eq!(sent[1].len(), 16 * 16 * 2 * 2);
        assert_eq!(
            sent[2], "052001d0012f01df010a",
            "tile (288, 464)..(303, 479)"
        );
    }

    #[test]
    fn landscape_frames_keep_their_own_coordinates() {
        let mut s = connected(big());
        s.set_orientation(Orientation::Landscape).unwrap();
        let wide = big().panel.in_orientation(Orientation::Landscape);
        let mut frame = Frame::filled(wide, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::WHITE);
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        let sent = &s.wire().sent[before..];
        assert_eq!(hex(&sent[0]), "0500000000df013f010a");
        assert_eq!(
            sent.len(),
            1 + 160,
            "480 x 320 x 2 bytes in 1920-byte writes"
        );
        assert_eq!(&sent[1][..4], &[0xFF, 0xFF, 0, 0], "no host rotation");

        // Back to portrait: full frame again, and the wide frame is refused.
        s.set_orientation(Orientation::Portrait).unwrap();
        assert!(s.present(&frame).is_err());
        let tall = Frame::filled(big().panel, Rgba::BLACK);
        let before = s.wire().sent.len();
        s.present(&tall).unwrap();
        assert_eq!(sent_since(&s, before)[0], "05000000003f01df010a");
    }

    #[test]
    fn small_panel_frames() {
        let mut s = connected(small());
        let frame = Frame::filled(small().panel, Rgba::opaque(0, 0, 255));
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        let sent = &s.wire().sent[before..];
        assert_eq!(hex(&sent[0]), "05000000004f009f000a");
        assert_eq!(sent.len(), 1 + 80, "80 x 160 x 2 bytes in 320-byte writes");
        assert_eq!(&sent[1][..2], &[0x1F, 0x00]);
    }

    #[test]
    fn screen_off_then_the_next_frame_wakes_the_panel() {
        let mut s = connected(big());
        let frame = Frame::filled(big().panel, Rgba::BLACK);
        s.present(&frame).unwrap();
        let before = s.wire().sent.len();
        s.screen_off().unwrap();
        assert_eq!(sent_since(&s, before), ["0300e8030a", "0600000a", "070a"]);
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        // Never set: 25 % (63), then a full frame.
        assert_eq!(
            &sent_since(&s, before)[..2],
            ["033fe8030a", "05000000003f01df010a"]
        );

        s.set_brightness(Brightness::new(60).unwrap()).unwrap();
        s.screen_off().unwrap();
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(sent_since(&s, before)[0], "0399e8030a", "60 % = 153");

        // Setting a level while dark wakes the panel; no second restore.
        s.screen_off().unwrap();
        s.set_brightness(Brightness::MAX).unwrap();
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(sent_since(&s, before)[0], "05000000003f01df010a");
    }

    #[test]
    fn the_small_panel_has_no_sensor_to_silence() {
        let mut s = connected(small());
        let before = s.wire().sent.len();
        s.screen_off().unwrap();
        assert_eq!(sent_since(&s, before), ["0300e8030a", "070a"]);
    }

    #[test]
    fn release_sends_free_and_forgets_the_frame() {
        let mut s = connected(big());
        let frame = Frame::filled(big().panel, Rgba::BLACK);
        s.present(&frame).unwrap();
        let before = s.wire().sent.len();
        s.release().unwrap();
        assert_eq!(sent_since(&s, before), ["070a"]);
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(sent_since(&s, before)[0], "05000000003f01df010a");
    }

    #[test]
    fn a_rect_that_does_not_fit_is_an_error_not_a_panic() {
        let mut s = connected(small());
        let frame = Frame::filled(small().panel, Rgba::BLACK);
        let before = s.wire().sent.len();
        assert!(s.bitmap(&frame, Rect::new(70, 0, 16, 16)).is_err());
        assert!(s.bitmap(&frame, Rect::new(0, 0, 0, 0)).is_err());
        assert_eq!(s.wire().sent.len(), before, "nothing is sent");
        let wrong = Frame::filled(Size::new(10, 10), Rgba::BLACK);
        assert!(s.present(&wrong).is_err());
    }
}
