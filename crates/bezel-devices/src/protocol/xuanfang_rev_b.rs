//! XuanFang rev B wire format (3.5" rev B and "flagship").
//!
//! Spec: `docs/reverse-engineering/protocol-xuanfang-rev-b.md`. Every command
//! is ten bytes `[op] [payload, zero-padded to 8] [op]`, no checksum. A bitmap
//! is a DISPLAY_BITMAP command followed by RGB565 big-endian pixels in chunks
//! of four display rows. The device switches between portrait and landscape
//! itself; the reverse orientations are done by the host (pixels turned 180°,
//! rectangle mirrored). Only HELLO has an answer.
//!
//! This module is pure: it only builds and parses bytes.

use bezel_core::domain::device::ModelId;
use bezel_core::domain::frame::Rect;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::screen::Brightness;

/// Size of every command.
pub const PACKET_LEN: usize = 10;
/// Payload bytes between the two opcode copies.
pub const PAYLOAD_LEN: usize = 8;
/// Bytes read back after HELLO.
pub const HELLO_REPLY_LEN: usize = 10;
/// HELLO's payload, echoed by the device.
pub const HELLO_PAYLOAD: [u8; 5] = *b"HELLO";
/// Byte 6 of a HELLO answer that carries a sub-revision in byte 7.
pub const VERSION_MARK: u8 = 0x0A;
pub use super::rgb565::PIXEL_BYTES;
/// Portrait panel size of every rev B variant.
pub const PANEL: Size = Size::new(320, 480);

/// Opcodes (`lcd_comm_rev_b.py:29-34`).
pub mod op {
    /// Handshake; the device answers with its sub-revision.
    pub const HELLO: u8 = 0xCA;
    /// Device-side orientation: 0 portrait, 1 landscape.
    pub const SET_ORIENTATION: u8 = 0xCB;
    /// Rectangle as BE16 `x0 y0 x1 y1` (inclusive), followed by RGB565 BE pixels.
    pub const DISPLAY_BITMAP: u8 = 0xCC;
    /// Backplate RGB LEDs (flagship only).
    pub const SET_LIGHTING: u8 = 0xCD;
    /// Backlight level.
    pub const SET_BRIGHTNESS: u8 = 0xCE;
}

/// Builds a packet. `None` when `payload` exceeds [`PAYLOAD_LEN`].
pub fn packet(opcode: u8, payload: &[u8]) -> Option<[u8; PACKET_LEN]> {
    if payload.len() > PAYLOAD_LEN {
        return None;
    }
    let mut packet = [0u8; PACKET_LEN];
    packet[0] = opcode;
    packet[1..1 + payload.len()].copy_from_slice(payload);
    packet[PACKET_LEN - 1] = opcode;
    Some(packet)
}

fn fixed(opcode: u8, payload: &[u8]) -> [u8; PACKET_LEN] {
    // Only called with payloads of at most eight bytes.
    packet(opcode, payload).unwrap_or([0; PACKET_LEN])
}

/// HELLO.
pub fn hello() -> [u8; PACKET_LEN] {
    fixed(op::HELLO, &HELLO_PAYLOAD)
}

/// SET_BRIGHTNESS with a raw level (see [`SubRevision::brightness_level`]).
pub fn set_brightness(level: u8) -> [u8; PACKET_LEN] {
    fixed(op::SET_BRIGHTNESS, &[level])
}

/// SET_LIGHTING: backplate LED colour (flagship only; `0, 0, 0` = off).
pub fn set_lighting(r: u8, g: u8, b: u8) -> [u8; PACKET_LEN] {
    fixed(op::SET_LIGHTING, &[r, g, b])
}

/// The value the device knows: 0 for both portraits, 1 for both landscapes.
pub const fn device_orientation(orientation: Orientation) -> u8 {
    if orientation.is_landscape() { 1 } else { 0 }
}

/// Whether the host turns pixels 180° itself (the reverse orientations).
pub const fn is_software_reversed(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::ReversePortrait | Orientation::ReverseLandscape
    )
}

/// SET_ORIENTATION (`cb 00` or `cb 01`).
pub fn set_orientation(orientation: Orientation) -> [u8; PACKET_LEN] {
    fixed(op::SET_ORIENTATION, &[device_orientation(orientation)])
}

/// The device rectangle for `rect` of a `screen`-sized canvas (both in the
/// current orientation): mirrored through the centre in the reverse
/// orientations. `None` when `rect` is empty or leaves the screen.
pub fn window(rect: Rect, screen: Size, orientation: Orientation) -> Option<Rect> {
    if rect.is_empty() || rect.right() > screen.width || rect.bottom() > screen.height {
        return None;
    }
    if !is_software_reversed(orientation) {
        return Some(rect);
    }
    Some(Rect::new(
        screen.width - rect.right(),
        screen.height - rect.bottom(),
        rect.width,
        rect.height,
    ))
}

/// DISPLAY_BITMAP header for `rect` of a `screen`-sized canvas (see [`window`]).
pub fn display_bitmap(
    rect: Rect,
    screen: Size,
    orientation: Orientation,
) -> Option<[u8; PACKET_LEN]> {
    let w = window(rect, screen, orientation)?;
    let mut payload = [0u8; PAYLOAD_LEN];
    for (i, v) in [w.x, w.y, w.right() - 1, w.bottom() - 1]
        .into_iter()
        .enumerate()
    {
        let v = u16::try_from(v).ok()?;
        payload[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
    }
    Some(fixed(op::DISPLAY_BITMAP, &payload))
}

/// A rectangle as big-endian RGB565, pixel order reversed when the panel is
/// driven turned 180°.
pub fn rgb565_be(rgba: &[u8], rotate_180: bool) -> Vec<u8> {
    if rotate_180 {
        super::rgb565::be_reversed(rgba)
    } else {
        super::rgb565::be(rgba)
    }
}

/// Bytes per data write: four display rows of the current-orientation width.
pub const fn chunk_len(width: u32) -> usize {
    width as usize * 8
}

/// Hardware variant, from byte 7 of the HELLO answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubRevision {
    /// Rev B, backlight on/off only (the reference's default before HELLO).
    A01,
    /// Flagship, backlight on/off only.
    A02,
    /// Rev B, backlight 0..=255.
    A11,
    /// Flagship, backlight 0..=255.
    A12,
}

impl SubRevision {
    /// The variant assumed when HELLO does not name one (`lcd_comm_rev_b.py:59`).
    pub const DEFAULT: SubRevision = SubRevision::A01;

    /// The variant a HELLO answer's byte 7 names.
    pub const fn from_code(code: u8) -> Option<SubRevision> {
        match code {
            0x01 => Some(SubRevision::A01),
            0x02 => Some(SubRevision::A02),
            0x11 => Some(SubRevision::A11),
            0x12 => Some(SubRevision::A12),
            _ => None,
        }
    }

    /// Flagship units (A02, A12) have backplate LEDs.
    pub const fn is_flagship(self) -> bool {
        matches!(self, SubRevision::A02 | SubRevision::A12)
    }

    /// A11 and A12 accept a backlight level; A01 and A02 only on or off.
    pub const fn has_brightness_range(self) -> bool {
        matches!(self, SubRevision::A11 | SubRevision::A12)
    }

    /// The SET_BRIGHTNESS value for a percentage: `int(level / 100 * 255)`
    /// with a range, else `1` (off) for 0 % and `0` (full) otherwise.
    pub fn brightness_level(self, brightness: Brightness) -> u8 {
        let percent = brightness.percent();
        if self.has_brightness_range() {
            (u16::from(percent) * 255 / 100).min(255) as u8
        } else {
            u8::from(percent == 0)
        }
    }

    /// The catalog model of this variant.
    pub const fn model_id(self) -> ModelId {
        if self.is_flagship() {
            ModelId("xuanfang-3.5-flagship")
        } else {
            ModelId("xuanfang-3.5")
        }
    }

    /// The reference's name for it.
    pub const fn name(self) -> &'static str {
        match self {
            SubRevision::A01 => "A01",
            SubRevision::A02 => "A02",
            SubRevision::A11 => "A11",
            SubRevision::A12 => "A12",
        }
    }
}

/// A HELLO answer: `ca 'HELLO' 0a <sub> <?> ca` when well formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// The bytes received.
    pub raw: Vec<u8>,
    /// The variant it names, if any.
    pub sub_revision: Option<SubRevision>,
}

impl Hello {
    /// Parses an answer. The sub-revision is read whenever byte 6 is `0a`,
    /// even if the framing is off (the reference only warns about framing).
    pub fn parse(reply: &[u8]) -> Hello {
        let sub_revision = match reply {
            [_, _, _, _, _, _, VERSION_MARK, code, ..] => SubRevision::from_code(*code),
            _ => None,
        };
        Hello {
            raw: reply.to_vec(),
            sub_revision,
        }
    }

    /// Ten bytes, framed by the opcode, echoing `HELLO`.
    pub fn is_well_formed(&self) -> bool {
        self.raw.len() == HELLO_REPLY_LEN
            && self.raw[0] == op::HELLO
            && self.raw[HELLO_REPLY_LEN - 1] == op::HELLO
            && self.raw[1..6] == HELLO_PAYLOAD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn pct(p: u8) -> Brightness {
        Brightness::new(p).unwrap()
    }

    /// The 3x2 "grad" image of the reference dump: red, green, blue / white, black, #123456.
    fn grad() -> Vec<u8> {
        [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
            [0, 0, 0, 255],
            [0x12, 0x34, 0x56, 255],
        ]
        .concat()
    }

    #[test]
    fn packets_match_the_reference_vectors() {
        // docs: protocol-xuanfang-rev-b.md § 10 and the analysis Appendix B (verified).
        assert_eq!(hex(&hello()), "ca48454c4c4f000000ca");
        let bright = |s: SubRevision, p| hex(&set_brightness(s.brightness_level(pct(p))));
        assert_eq!(bright(SubRevision::A01, 0), "ce0100000000000000ce");
        assert_eq!(bright(SubRevision::A01, 25), "ce0000000000000000ce");
        assert_eq!(bright(SubRevision::A01, 100), "ce0000000000000000ce");
        assert_eq!(bright(SubRevision::A12, 0), "ce0000000000000000ce");
        assert_eq!(bright(SubRevision::A12, 25), "ce3f00000000000000ce");
        assert_eq!(bright(SubRevision::A12, 100), "ceff00000000000000ce");
        assert_eq!(hex(&set_lighting(0x11, 0x22, 0x33)), "cd1122330000000000cd");

        use Orientation::*;
        assert_eq!(hex(&set_orientation(Portrait)), "cb0000000000000000cb");
        assert_eq!(
            hex(&set_orientation(ReversePortrait)),
            "cb0000000000000000cb"
        );
        assert_eq!(hex(&set_orientation(Landscape)), "cb0100000000000000cb");
        assert_eq!(
            hex(&set_orientation(ReverseLandscape)),
            "cb0100000000000000cb"
        );

        // grad (3x2) at (10,20) in each orientation, on the 320 x 480 panel.
        let panel = Size::new(320, 480);
        let grad_at = |o: Orientation| {
            let header = display_bitmap(Rect::new(10, 20, 3, 2), panel.in_orientation(o), o);
            (
                hex(&header.unwrap()),
                hex(&rgb565_be(&grad(), is_software_reversed(o))),
            )
        };
        let upright = "f80007e0001fffff000011aa";
        let turned = "11aa0000ffff001f07e0f800";
        assert_eq!(
            grad_at(Portrait),
            ("cc000a0014000c0015cc".into(), upright.into())
        );
        assert_eq!(
            grad_at(Landscape),
            ("cc000a0014000c0015cc".into(), upright.into())
        );
        assert_eq!(
            grad_at(ReversePortrait),
            ("cc013301ca013501cbcc".into(), turned.into())
        );
        assert_eq!(
            grad_at(ReverseLandscape),
            ("cc01d3012a01d5012bcc".into(), turned.into())
        );

        // Full 320 x 480 frame, portrait (golden file): header, then 120 writes of 2560 bytes.
        assert_eq!(
            hex(&display_bitmap(Rect::of(panel), panel, Portrait).unwrap()),
            "cc00000000013f01dfcc"
        );
        assert_eq!(chunk_len(320), 2560);
        assert_eq!((320 * 480 * PIXEL_BYTES).div_ceil(chunk_len(320)), 120);
    }

    #[test]
    fn ranged_brightness_matches_the_reference_formula_at_every_percent() {
        // int((level / 100) * 255) for level 0..=100, computed by the Python reference.
        let reference = "000205070a0c0f111416191c1e212326282b2d303335383a3d3f424447494c4f\
                         515456595b5e606366686b6d707275777a7c7f828487898c8e919396999b9ea0\
                         a3a5a8aaadafb2b5b7babcbfc1c4c6c9ccced1d3d6d8dbdde0e2e5e8eaedeff2\
                         f4f7f9fcff";
        for sub in [SubRevision::A11, SubRevision::A12] {
            let ours: Vec<u8> = (0..=100).map(|p| sub.brightness_level(pct(p))).collect();
            assert_eq!(hex(&ours), reference);
        }
        for sub in [SubRevision::A01, SubRevision::A02] {
            assert_eq!(sub.brightness_level(pct(0)), 1);
            assert!((1..=100).all(|p| sub.brightness_level(pct(p)) == 0));
        }
    }

    #[test]
    fn packet_framing() {
        assert_eq!(
            packet(0x42, &[1, 2, 3, 4, 5, 6, 7, 8]),
            Some([0x42, 1, 2, 3, 4, 5, 6, 7, 8, 0x42])
        );
        assert_eq!(
            packet(0x42, &[]),
            Some([0x42, 0, 0, 0, 0, 0, 0, 0, 0, 0x42])
        );
        assert!(packet(0x42, &[0; 9]).is_none());
    }

    #[test]
    fn windows_stay_on_screen_and_mirror_in_reverse() {
        let screen = Size::new(320, 480);
        use Orientation::*;
        assert!(window(Rect::new(0, 0, 0, 4), screen, Portrait).is_none());
        assert!(window(Rect::new(310, 0, 11, 1), screen, Portrait).is_none());
        assert!(window(Rect::new(0, 470, 1, 11), screen, ReversePortrait).is_none());
        assert_eq!(
            window(Rect::of(screen), screen, ReversePortrait),
            Some(Rect::of(screen))
        );
        assert_eq!(
            window(Rect::new(0, 0, 16, 16), screen, ReversePortrait),
            Some(Rect::new(304, 464, 16, 16))
        );
        assert!(display_bitmap(Rect::new(0, 0, 1, 1), Size::new(70_000, 1), Portrait).is_some());
        assert!(
            display_bitmap(Rect::new(65_536, 0, 1, 1), Size::new(70_000, 1), Portrait).is_none()
        );
        assert_eq!(device_orientation(Landscape), 1);
        assert_eq!(device_orientation(ReversePortrait), 0);
        assert!(!is_software_reversed(Landscape));
    }

    #[test]
    fn pixels_are_big_endian_and_turn_by_reversal() {
        assert_eq!(rgb565_be(&[0x12, 0x34, 0x56, 0], false), vec![0x11, 0xaa]);
        let two = [[255, 0, 0, 255], [0, 0, 255, 255]].concat();
        assert_eq!(rgb565_be(&two, false), vec![0xf8, 0x00, 0x00, 0x1f]);
        assert_eq!(rgb565_be(&two, true), vec![0x00, 0x1f, 0xf8, 0x00]);
        assert!(rgb565_be(&[], true).is_empty());
    }

    #[test]
    fn hello_answers() {
        let hello = |sub: u8| [0xca, b'H', b'E', b'L', b'L', b'O', 0x0a, sub, 0x00, 0xca];
        for (code, sub, flagship, ranged, id) in [
            (0x01, SubRevision::A01, false, false, "xuanfang-3.5"),
            (0x02, SubRevision::A02, true, false, "xuanfang-3.5-flagship"),
            (0x11, SubRevision::A11, false, true, "xuanfang-3.5"),
            // The original author's flagship answered `... 0a 12 00`.
            (0x12, SubRevision::A12, true, true, "xuanfang-3.5-flagship"),
        ] {
            let h = Hello::parse(&hello(code));
            assert!(h.is_well_formed());
            assert_eq!(h.sub_revision, Some(sub));
            assert_eq!(sub.is_flagship(), flagship);
            assert_eq!(sub.has_brightness_range(), ranged);
            assert_eq!(sub.model_id().0, id);
            assert_eq!(
                SubRevision::from_code(code).map(SubRevision::name),
                Some(sub.name())
            );
        }
        assert_eq!(Hello::parse(&hello(0x13)).sub_revision, None);
        let mut unmarked = hello(0x12);
        unmarked[6] = 0x0b;
        assert_eq!(Hello::parse(&unmarked).sub_revision, None);
        // Bad framing is tolerated; the sub-revision still counts.
        let mut framed = hello(0x11);
        framed[9] = 0;
        let h = Hello::parse(&framed);
        assert!(!h.is_well_formed());
        assert_eq!(h.sub_revision, Some(SubRevision::A11));
        let mut echo = hello(0x11);
        echo[1] = b'J';
        assert!(!Hello::parse(&echo).is_well_formed());
        let mut start = hello(0x11);
        start[0] = 0;
        assert!(!Hello::parse(&start).is_well_formed());
        let short = Hello::parse(&hello(0x12)[..8]);
        assert!(!short.is_well_formed());
        assert_eq!(short.sub_revision, Some(SubRevision::A12));
        assert_eq!(Hello::parse(&[0xca; 7]).sub_revision, None);
        assert_eq!(Hello::parse(&[]).sub_revision, None);
        assert_eq!(SubRevision::DEFAULT, SubRevision::A01);
    }
}
