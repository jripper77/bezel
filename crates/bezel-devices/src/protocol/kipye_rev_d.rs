//! Kipye Qiye 3.5" (rev D) wire format.
//!
//! Spec: `docs/reverse-engineering/protocol-kipye-rev-d.md`. Commands are four
//! bytes whose opcodes are ASCII letters (`C` = 0x43, `D`, `A`, `G`); the
//! drawing-window command is ten bytes. Values are big-endian. Pixels follow a
//! window as 64-byte packets: `P` (0x50) and up to 63 bytes of RGB565 BE.
//! The device acknowledges commands in an unknown format; nothing is parsed.
//!
//! This module is pure: it only builds bytes.

use bezel_core::domain::frame::{RGBA_BYTES, Rect};
use bezel_core::domain::geometry::Orientation;

/// Size of a pixel data packet on the wire.
pub const PACKET: usize = 64;
/// Pixel bytes carried by one data packet (after the `P` marker).
pub const PACKET_PAYLOAD: usize = PACKET - 1;
/// Highest backlight value (SETBL takes 0..=500).
pub const MAX_BACKLIGHT: u16 = 500;
/// How many times a backlight command is sent: a single write is not always
/// applied, so the reference repeats it.
pub const BACKLIGHT_REPEATS: usize = 2;

/// Command bytes of the family.
pub mod op {
    /// `G`: device information. The answer format is unknown; never sent.
    pub const GET_INFO: [u8; 4] = [0x47, 0, 0, 0];
    /// `C H`: portrait (the device's upright orientation).
    pub const SET_ORG: [u8; 4] = [0x43, 0x48, 0, 0];
    /// `C G`: reverse portrait (the device turns its output 180°).
    pub const SET_180: [u8; 4] = [0x43, 0x47, 0, 0];
    /// `C D`: portrait mirrored horizontally. Unused by the reference; never sent.
    pub const SET_HF: [u8; 4] = [0x43, 0x44, 0, 0];
    /// `C F`: probably a vertical flip. Unused by the reference; never sent.
    pub const SET_VF: [u8; 4] = [0x43, 0x46, 0, 0];
    /// `C C` + BE16 level (0..=500): backlight.
    pub const SET_BL: [u8; 2] = [0x43, 0x43];
    /// `C B` + BE16 RGB565: fills the whole screen with one colour.
    pub const DISP_COLOR: [u8; 2] = [0x43, 0x42];
    /// `C A` + BE16 x0, x1, y0, y1: the drawing window (inclusive).
    pub const BLOCK_WRITE: [u8; 2] = [0x43, 0x41];
    /// `D`: pixel packets follow.
    pub const INTO_PIC_MODE: [u8; 4] = [0x44, 0, 0, 0];
    /// `A`: the pixel packets are over.
    pub const OUT_PIC_MODE: [u8; 4] = [0x41, 0, 0, 0];
    /// `P`: first byte of every pixel packet.
    pub const DATA: u8 = 0x50;
}

fn with_value(prefix: [u8; 2], value: u16) -> [u8; 4] {
    let v = value.to_be_bytes();
    [prefix[0], prefix[1], v[0], v[1]]
}

/// The backlight value for a level in percent: `percent × 5` (0..=500).
pub fn backlight_level(percent: u8) -> u16 {
    u16::from(percent.min(100)) * 5
}

/// SETBL with a raw level, clamped to [`MAX_BACKLIGHT`].
pub fn set_backlight(level: u16) -> [u8; 4] {
    with_value(op::SET_BL, level.min(MAX_BACKLIGHT))
}

/// DISPCOLOR: the whole screen in one RGB565 colour.
pub fn display_color(rgb565: u16) -> [u8; 4] {
    with_value(op::DISP_COLOR, rgb565)
}

/// The reference's `Clear` and `Reset`: the whole screen white. Destructive;
/// never sent implicitly.
pub fn clear() -> [u8; 4] {
    display_color(0xFFFF)
}

/// The device-side part of an orientation: the reverse orientations turn the
/// output 180° on the device; landscape is done by the host
/// ([`window`] and a 90° clockwise rotation of the pixels).
pub fn set_orientation(orientation: Orientation) -> [u8; 4] {
    match orientation {
        Orientation::ReversePortrait | Orientation::ReverseLandscape => op::SET_180,
        Orientation::Portrait | Orientation::Landscape => op::SET_ORG,
    }
}

/// An inclusive drawing window in panel (portrait) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Left column.
    pub x0: u16,
    /// Right column (inclusive).
    pub x1: u16,
    /// Top row.
    pub y0: u16,
    /// Bottom row (inclusive).
    pub y1: u16,
}

/// The window that shows `rect`, given in the canvas of `orientation`, on a
/// panel whose portrait width is `panel_width` (320).
///
/// Portrait orientations use the rectangle as is. Landscape ones expect the
/// pixels turned 90° clockwise, and the window becomes
/// `x0 = panel_width - y - h`, `x1 = panel_width - y - 1`, `y0 = x`,
/// `y1 = x + w - 1` (`w`, `h` = the rectangle before rotation).
///
/// `None` for an empty rectangle, a landscape rectangle below the panel
/// width, or coordinates beyond 16 bits.
pub fn window(rect: Rect, orientation: Orientation, panel_width: u32) -> Option<Window> {
    if rect.is_empty() {
        return None;
    }
    let (x0, x1, y0, y1) = if orientation.is_landscape() {
        let left = panel_width.checked_sub(rect.bottom())?;
        (left, left + rect.height - 1, rect.x, rect.right() - 1)
    } else {
        (rect.x, rect.right() - 1, rect.y, rect.bottom() - 1)
    };
    Some(Window {
        x0: u16::try_from(x0).ok()?,
        x1: u16::try_from(x1).ok()?,
        y0: u16::try_from(y0).ok()?,
        y1: u16::try_from(y1).ok()?,
    })
}

/// BLOCKWRITE: sets the drawing window. Note the x0, x1, y0, y1 order.
pub fn block_write(w: Window) -> [u8; 10] {
    let mut out = [0u8; 10];
    out[..2].copy_from_slice(&op::BLOCK_WRITE);
    for (i, v) in [w.x0, w.x1, w.y0, w.y1].into_iter().enumerate() {
        out[2 + i * 2..4 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// One colour as RGB565: `(R >> 3) << 11 | (G >> 2) << 5 | B >> 3`.
pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}

/// RGBA8 pixels as RGB565 big-endian, row-major. Alpha is ignored, as in the
/// reference (no compositing).
pub fn rgb565_be(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 2);
    for px in rgba.as_chunks::<RGBA_BYTES>().0 {
        out.extend_from_slice(&rgb565(px[0], px[1], px[2]).to_be_bytes());
    }
    out
}

/// The pixel data phase: `P` and 63 bytes per packet, the last one shorter.
/// Packets are concatenated; each starts on a 64-byte boundary.
pub fn data_packets(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len().div_ceil(PACKET_PAYLOAD) * PACKET);
    for chunk in pixels.chunks(PACKET_PAYLOAD) {
        out.push(op::DATA);
        out.extend_from_slice(chunk);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::frame::Frame;
    use bezel_core::domain::geometry::Size;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The reference "grad" image: 3x2, row 0 red, green, blue; row 1 white,
    /// black, #123456.
    fn grad() -> Frame {
        let px = [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [255, 255, 255],
            [0, 0, 0],
            [0x12, 0x34, 0x56],
        ];
        let rgba = px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        Frame::from_rgba(Size::new(3, 2), rgba).unwrap()
    }

    /// One bitmap as the reference writes it: window, INTOPICMODE, packets, OUTPICMODE.
    fn bitmap(image: &Frame, at: Rect, orientation: Orientation) -> Vec<String> {
        let pixels = if orientation.is_landscape() {
            image.rotated(1)
        } else {
            image.clone()
        };
        let w = window(at, orientation, 320).unwrap();
        vec![
            hex(&block_write(w)),
            hex(&op::INTO_PIC_MODE),
            hex(&data_packets(&rgb565_be(pixels.as_rgba()))),
            hex(&op::OUT_PIC_MODE),
        ]
    }

    #[test]
    fn packets_match_the_reference_vectors() {
        // docs: protocol-kipye-rev-d.md § 9 (verified against the Python reference).
        assert_eq!(BACKLIGHT_REPEATS, 2, "brightness is sent twice");
        assert_eq!(hex(&set_backlight(backlight_level(0))), "43430000");
        assert_eq!(hex(&set_backlight(backlight_level(25))), "4343007d");
        assert_eq!(hex(&set_backlight(backlight_level(100))), "434301f4");
        assert_eq!(hex(&clear()), "4342ffff");
        assert_eq!(hex(&set_orientation(Orientation::Portrait)), "43480000");
        assert_eq!(hex(&set_orientation(Orientation::Landscape)), "43480000");
        assert_eq!(
            hex(&set_orientation(Orientation::ReversePortrait)),
            "43470000"
        );
        assert_eq!(
            hex(&set_orientation(Orientation::ReverseLandscape)),
            "43470000"
        );

        let at = Rect::new(10, 20, 3, 2);
        let portrait = [
            "4341000a000c00140015",
            "44000000",
            "50f80007e0001fffff000011aa",
            "41000000",
        ];
        let landscape = [
            "4341012a012b000a000c",
            "44000000",
            "50fffff800000007e011aa001f",
            "41000000",
        ];
        for o in [Orientation::Portrait, Orientation::ReversePortrait] {
            assert_eq!(bitmap(&grad(), at, o), portrait, "{o:?}");
        }
        for o in [Orientation::Landscape, Orientation::ReverseLandscape] {
            assert_eq!(bitmap(&grad(), at, o), landscape, "{o:?}");
        }

        // Full 320 x 480 frame: 4876 packets of 64 bytes plus one of 13.
        let frame = data_packets(&vec![0xAB; 320 * 480 * 2]);
        assert_eq!(frame.len(), 4876 * 64 + 13);
        assert!(frame.chunks(PACKET).all(|p| p[0] == op::DATA));
        assert_eq!(frame.chunks(PACKET).last().map(<[u8]>::len), Some(13));
    }

    #[test]
    fn unused_commands_and_limits() {
        assert_eq!(hex(&op::GET_INFO), "47000000");
        assert_eq!(hex(&op::SET_HF), "43440000");
        assert_eq!(hex(&op::SET_VF), "43460000");
        assert_eq!(hex(&display_color(0xF800)), "4342f800");
        assert_eq!(set_backlight(900), set_backlight(MAX_BACKLIGHT));
        assert_eq!(backlight_level(250), MAX_BACKLIGHT);
    }

    #[test]
    fn rgb565_big_endian_ignores_alpha() {
        assert_eq!(rgb565(0x12, 0x34, 0x56), 0x11AA);
        assert_eq!(hex(&rgb565_be(&[0x12, 0x34, 0x56, 0])), "11aa");
        assert!(rgb565_be(&[]).is_empty());
        assert!(data_packets(&[]).is_empty());
    }

    #[test]
    fn whole_panel_windows() {
        let full = window(Rect::new(0, 0, 320, 480), Orientation::Portrait, 320).unwrap();
        assert_eq!(hex(&block_write(full)), "43410000013f000001df");
        // A landscape canvas is 480 x 320; turned clockwise it covers the panel.
        let wide = window(Rect::new(0, 0, 480, 320), Orientation::Landscape, 320).unwrap();
        assert_eq!(wide, full);
        // The bottom-right landscape corner lands on the panel's bottom-left.
        let corner = window(Rect::new(479, 319, 1, 1), Orientation::Landscape, 320).unwrap();
        assert_eq!(
            (corner.x0, corner.x1, corner.y0, corner.y1),
            (0, 0, 479, 479)
        );
    }

    #[test]
    fn impossible_windows_are_refused() {
        assert!(window(Rect::new(0, 0, 0, 5), Orientation::Portrait, 320).is_none());
        assert!(window(Rect::new(0, 300, 5, 30), Orientation::Landscape, 320).is_none());
        assert!(window(Rect::new(70_000, 0, 1, 1), Orientation::Portrait, 320).is_none());
    }
}
