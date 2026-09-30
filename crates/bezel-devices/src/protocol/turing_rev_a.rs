//! Turing rev A wire format (Turing 3.5" and UsbPCMonitor 3.5"/5"/7").
//!
//! Spec: `docs/reverse-engineering/protocol-turing-rev-a.md`. Every command is
//! six bytes: the four 10-bit fields `x y ex ey` packed MSB first into five
//! bytes, then the opcode. Orientation uses a 16-byte variant. A bitmap is a
//! DISPLAY_BITMAP command followed by RGB565 little-endian pixels written in
//! chunks of four display rows. Nothing is acknowledged; only HELLO has an
//! answer (and the Turing 3.5" does not even answer that).
//!
//! This module is pure: it only builds and parses bytes.

use bezel_core::domain::device::ModelId;
use bezel_core::domain::frame::Rect;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::screen::Brightness;

/// Size of every command but SET_ORIENTATION.
pub const COMMAND_LEN: usize = 6;
/// Size of the SET_ORIENTATION packet (newer 3.5" units need all 16 bytes).
pub const ORIENTATION_LEN: usize = 16;
/// Largest value a 10-bit coordinate field holds.
pub const MAX_COORD: u32 = 0x3FF;
/// Bytes read back after HELLO.
pub const HELLO_REPLY_LEN: usize = 6;
pub use super::rgb565::PIXEL_BYTES;

/// Opcodes (`lcd_comm_rev_a.py:32-47`).
pub mod op {
    /// Reboots the device; it re-enumerates, maybe under another port name (disruptive).
    pub const RESET: u8 = 0x65;
    /// Paints the screen white; needs portrait first.
    pub const CLEAR: u8 = 0x66;
    /// Paints the screen black ("NOT TESTED" in the reference).
    pub const TO_BLACK: u8 = 0x67;
    /// Panel off.
    pub const SCREEN_OFF: u8 = 0x6C;
    /// Panel on.
    pub const SCREEN_ON: u8 = 0x6D;
    /// Backlight in the `x` field, 0 = brightest, 255 = darkest.
    pub const SET_BRIGHTNESS: u8 = 0x6E;
    /// Device-side rotation (16-byte packet).
    pub const SET_ORIENTATION: u8 = 0x79;
    /// Mirrored rendering (payload unknown).
    pub const SET_MIRROR: u8 = 0x7A;
    /// Non-contiguous pixel list (payload unknown).
    pub const DISPLAY_PIXELS: u8 = 0xC3;
    /// Rectangle `x0 y0 x1 y1` (inclusive), followed by RGB565 LE pixels.
    pub const DISPLAY_BITMAP: u8 = 0xC5;
    /// Unknown ("?" in the reference).
    pub const LCD_28: u8 = 0x28;
    /// Unknown ("?" in the reference).
    pub const LCD_29: u8 = 0x29;
    /// Model query, sent as six copies of the opcode.
    pub const HELLO: u8 = 0x45;
}

/// Builds a 6-byte command. `None` when a field does not fit 10 bits.
pub fn command(opcode: u8, x: u32, y: u32, ex: u32, ey: u32) -> Option<[u8; COMMAND_LEN]> {
    if [x, y, ex, ey].iter().any(|&v| v > MAX_COORD) {
        return None;
    }
    // The four fields form one 40-bit big-endian bit string.
    let bits = (u64::from(x) << 30) | (u64::from(y) << 20) | (u64::from(ex) << 10) | u64::from(ey);
    let mut packet = [0u8; COMMAND_LEN];
    packet[..5].copy_from_slice(&bits.to_be_bytes()[3..]);
    packet[5] = opcode;
    Some(packet)
}

/// A command whose four fields are zero.
pub const fn simple(opcode: u8) -> [u8; COMMAND_LEN] {
    [0, 0, 0, 0, 0, opcode]
}

/// HELLO: six copies of the opcode (not a packed command).
pub const fn hello() -> [u8; COMMAND_LEN] {
    [op::HELLO; COMMAND_LEN]
}

/// Panel on.
pub const fn screen_on() -> [u8; COMMAND_LEN] {
    simple(op::SCREEN_ON)
}

/// Panel off.
pub const fn screen_off() -> [u8; COMMAND_LEN] {
    simple(op::SCREEN_OFF)
}

/// RESET. Disruptive: never sent implicitly (D-2026-09-30-device-protocols-2).
pub const fn reset() -> [u8; COMMAND_LEN] {
    simple(op::RESET)
}

/// CLEAR (to white). Never sent implicitly (D-2026-09-30-device-protocols-2).
pub const fn clear() -> [u8; COMMAND_LEN] {
    simple(op::CLEAR)
}

/// The device's backlight value for a percentage: the scale is inverted
/// (0 = brightest). Equals the reference's `int(255 - (level / 100) * 255)`.
pub fn brightness_level(brightness: Brightness) -> u8 {
    let scaled = (u16::from(brightness.percent()) * 255).div_ceil(100);
    255 - scaled.min(255) as u8
}

/// SET_BRIGHTNESS with a raw device level (0 = brightest, 255 = darkest).
pub fn set_brightness(level: u8) -> [u8; COMMAND_LEN] {
    let mut packet = simple(op::SET_BRIGHTNESS);
    packet[0] = level >> 2;
    packet[1] = (level & 3) << 6;
    packet
}

/// SET_ORIENTATION: `orientation + 100`, then the width and height the screen
/// has in that orientation. `None` when a dimension exceeds the 10-bit address space.
pub fn set_orientation(orientation: Orientation, size: Size) -> Option<[u8; ORIENTATION_LEN]> {
    let limit = MAX_COORD + 1;
    if size.width > limit || size.height > limit {
        return None;
    }
    let mut packet = [0u8; ORIENTATION_LEN];
    packet[5] = op::SET_ORIENTATION;
    packet[6] = orientation.index() + 100;
    packet[7..9].copy_from_slice(&(size.width as u16).to_be_bytes());
    packet[9..11].copy_from_slice(&(size.height as u16).to_be_bytes());
    Some(packet)
}

/// DISPLAY_BITMAP header for `rect` (current-orientation coordinates; the
/// device rotates). `None` for an empty rectangle or one beyond 10 bits.
pub fn display_bitmap(rect: Rect) -> Option<[u8; COMMAND_LEN]> {
    if rect.is_empty() {
        return None;
    }
    command(
        op::DISPLAY_BITMAP,
        rect.x,
        rect.y,
        rect.right() - 1,
        rect.bottom() - 1,
    )
}

pub use super::rgb565::{le as rgb565_le, pack as rgb565};

/// Bytes per data write: four display rows of the current-orientation width.
pub const fn chunk_len(width: u32) -> usize {
    width as usize * 8
}

/// The hardware variant, told apart by the HELLO answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubRevision {
    /// Official Turing 3.5": does not answer HELLO.
    Turing35,
    /// UsbPCMonitor 3.5" (answers `01` × 6).
    UsbMonitor35,
    /// UsbPCMonitor 5" (answers `02` × 6).
    UsbMonitor5,
    /// UsbPCMonitor 7" (answers `03` × 6).
    UsbMonitor7,
}

impl SubRevision {
    /// Reads the HELLO answer; anything but six equal `01`/`02`/`03` bytes
    /// (including silence) is the Turing 3.5".
    pub fn from_hello(reply: &[u8]) -> SubRevision {
        match reply {
            [1, 1, 1, 1, 1, 1] => SubRevision::UsbMonitor35,
            [2, 2, 2, 2, 2, 2] => SubRevision::UsbMonitor5,
            [3, 3, 3, 3, 3, 3] => SubRevision::UsbMonitor7,
            _ => SubRevision::Turing35,
        }
    }

    /// The catalog model of this variant.
    pub const fn model_id(self) -> ModelId {
        match self {
            SubRevision::Turing35 => ModelId("turing-3.5"),
            SubRevision::UsbMonitor35 => ModelId("usbpcmonitor-3.5"),
            SubRevision::UsbMonitor5 => ModelId("usbpcmonitor-5"),
            SubRevision::UsbMonitor7 => ModelId("usbpcmonitor-7"),
        }
    }

    /// Portrait panel size the reference sets for this variant.
    pub const fn panel(self) -> Size {
        match self {
            SubRevision::Turing35 | SubRevision::UsbMonitor35 => Size::new(320, 480),
            SubRevision::UsbMonitor5 => Size::new(480, 800),
            SubRevision::UsbMonitor7 => Size::new(600, 1024),
        }
    }

    /// The reference's name for it.
    pub const fn name(self) -> &'static str {
        match self {
            SubRevision::Turing35 => "TURING_3_5",
            SubRevision::UsbMonitor35 => "USBMONITOR_3_5",
            SubRevision::UsbMonitor5 => "USBMONITOR_5",
            SubRevision::UsbMonitor7 => "USBMONITOR_7",
        }
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
        // docs: protocol-turing-rev-a.md § 11 and the analysis Appendix B (verified).
        let bright = |p| hex(&set_brightness(brightness_level(pct(p))));
        assert_eq!(bright(0), "3fc00000006e");
        assert_eq!(bright(1), "3f000000006e");
        assert_eq!(bright(25), "2fc00000006e");
        assert_eq!(bright(50), "1fc00000006e");
        assert_eq!(bright(100), "00000000006e");
        assert_eq!(hex(&screen_off()), "00000000006c");
        assert_eq!(hex(&screen_on()), "00000000006d");
        assert_eq!(hex(&reset()), "000000000065");
        assert_eq!(hex(&clear()), "000000000066");
        assert_eq!(hex(&hello()), "454545454545");

        use Orientation::*;
        let orient = |o: Orientation, panel: Size| {
            hex(&set_orientation(o, panel.in_orientation(o)).unwrap())
        };
        let small = Size::new(320, 480);
        assert_eq!(orient(Portrait, small), "00000000007964014001e00000000000");
        assert_eq!(orient(Landscape, small), "0000000000796601e001400000000000");
        assert_eq!(
            orient(ReversePortrait, small),
            "00000000007965014001e00000000000"
        );
        assert_eq!(
            orient(ReverseLandscape, small),
            "0000000000796701e001400000000000"
        );
        let seven = Size::new(600, 1024);
        assert_eq!(orient(Portrait, seven), "00000000007964025804000000000000");
        assert_eq!(orient(Landscape, seven), "00000000007966040002580000000000");

        // grad (3x2) at (10,20), portrait.
        assert_eq!(
            hex(&display_bitmap(Rect::new(10, 20, 3, 2)).unwrap()),
            "0281403015c5"
        );
        assert_eq!(hex(&rgb565_le(&grad())), "00f8e0071f00ffff0000aa11");
        // 600 x 1024 panel, 3x2 all-red image at (597,1022).
        assert_eq!(
            hex(&display_bitmap(Rect::new(597, 1022, 3, 2)).unwrap()),
            "957fe95fffc5"
        );
        assert_eq!(
            hex(&rgb565_le(&[255, 0, 0, 255].repeat(6))),
            "00f8".repeat(6)
        );
        // Full 320 x 480 frame (golden file): header, then 120 writes of 2560 bytes.
        assert_eq!(
            hex(&display_bitmap(Rect::of(small)).unwrap()),
            "000004fddfc5"
        );
        assert_eq!(chunk_len(320), 2560);
        assert_eq!((320 * 480 * PIXEL_BYTES).div_ceil(chunk_len(320)), 120);
        assert_eq!(chunk_len(480), 3840);
    }

    #[test]
    fn brightness_matches_the_reference_formula_at_every_percent() {
        // int(255 - (level / 100) * 255) for level 0..=100, computed by the Python reference.
        let reference = "fffcf9f7f4f2efedeae8e5e2e0dddbd8d6d3d1ceccc9c6c4c1bfbcbab7b5b2af\
                         adaaa8a5a3a09e9b999693918e8c898784827f7c7a777572706d6b686663605e\
                         5b595654514f4c494744423f3d3a383533302d2b282623211e1c191614110f0c\
                         0a07050200";
        let ours: Vec<u8> = (0..=100).map(|p| brightness_level(pct(p))).collect();
        assert_eq!(hex(&ours), reference);
    }

    #[test]
    fn fields_are_ten_bits_msb_first() {
        assert_eq!(
            command(0x42, MAX_COORD, MAX_COORD, MAX_COORD, MAX_COORD),
            Some([0xff, 0xff, 0xff, 0xff, 0xff, 0x42])
        );
        // One bit per field lands where the reference's shifts put it.
        assert_eq!(command(0, 1, 0, 0, 0), Some([0, 0x40, 0, 0, 0, 0]));
        assert_eq!(command(0, 0, 1, 0, 0), Some([0, 0, 0x10, 0, 0, 0]));
        assert_eq!(command(0, 0, 0, 1, 0), Some([0, 0, 0, 0x04, 0, 0]));
        assert_eq!(command(0, 0, 0, 0, 1), Some([0, 0, 0, 0, 1, 0]));
        assert!(command(0, 1024, 0, 0, 0).is_none());
        assert!(command(0, 0, 0, 0, 1024).is_none());
        assert_eq!(
            Some(set_brightness(255)),
            command(op::SET_BRIGHTNESS, 255, 0, 0, 0)
        );
        assert_eq!(simple(op::TO_BLACK)[5], 0x67);
    }

    #[test]
    fn bitmap_and_orientation_limits() {
        assert!(display_bitmap(Rect::new(0, 0, 0, 5)).is_none());
        assert!(display_bitmap(Rect::new(1000, 0, 25, 1)).is_none());
        assert!(display_bitmap(Rect::new(0, 0, 1024, 1024)).is_some());
        assert!(set_orientation(Orientation::Portrait, Size::new(1024, 1024)).is_some());
        assert!(set_orientation(Orientation::Portrait, Size::new(480, 1025)).is_none());
        assert!(set_orientation(Orientation::Portrait, Size::new(1025, 480)).is_none());
    }

    #[test]
    fn rgb565_colours_of_the_pixel_format_table() {
        // pixel-formats.md § 1.
        assert_eq!(rgb565(255, 0, 0), 0xF800);
        assert_eq!(rgb565(0, 255, 0), 0x07E0);
        assert_eq!(rgb565(0, 0, 255), 0x001F);
        assert_eq!(rgb565(255, 255, 255), 0xFFFF);
        assert_eq!(rgb565(0, 0, 0), 0x0000);
        assert_eq!(rgb565(0x12, 0x34, 0x56), 0x11AA);
        // Alpha is ignored, not composited.
        assert_eq!(rgb565_le(&[255, 0, 0, 0]), vec![0x00, 0xf8]);
        assert!(rgb565_le(&[]).is_empty());
    }

    #[test]
    fn hello_answers_select_the_sub_revision() {
        use SubRevision::*;
        assert_eq!(SubRevision::from_hello(&[1; 6]), UsbMonitor35);
        assert_eq!(SubRevision::from_hello(&[2; 6]), UsbMonitor5);
        assert_eq!(SubRevision::from_hello(&[3; 6]), UsbMonitor7);
        assert_eq!(SubRevision::from_hello(&[]), Turing35);
        assert_eq!(SubRevision::from_hello(&[2; 5]), Turing35);
        assert_eq!(SubRevision::from_hello(&[1, 1, 1, 2, 1, 1]), Turing35);
        assert_eq!(SubRevision::from_hello(&hello()), Turing35);
        let all = [Turing35, UsbMonitor35, UsbMonitor5, UsbMonitor7];
        let ids: Vec<&str> = all.iter().map(|s| s.model_id().0).collect();
        assert_eq!(
            ids,
            [
                "turing-3.5",
                "usbpcmonitor-3.5",
                "usbpcmonitor-5",
                "usbpcmonitor-7"
            ]
        );
        let panels: Vec<Size> = all.iter().map(|s| s.panel()).collect();
        assert_eq!(
            panels,
            [
                Size::new(320, 480),
                Size::new(320, 480),
                Size::new(480, 800),
                Size::new(600, 1024)
            ]
        );
        let names: Vec<&str> = all.iter().map(|s| s.name()).collect();
        assert_eq!(
            names,
            [
                "TURING_3_5",
                "USBMONITOR_3_5",
                "USBMONITOR_5",
                "USBMONITOR_7"
            ]
        );
    }
}
