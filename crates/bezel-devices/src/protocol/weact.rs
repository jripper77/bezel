//! WeAct Studio Display FS V1 wire format (3.5" 320x480 and 0.96" 80x160).
//!
//! Spec: `docs/reverse-engineering/protocol-weact.md`. Both sizes speak the
//! same protocol (the 0.96" has no humidity sensor). Commands are an opcode,
//! little-endian values and `0x0A`; read commands set bit 7 of the opcode.
//! Bitmaps are a 10-byte window header followed by raw RGB565 LE, in the
//! coordinates of the current orientation (the device rotates).
//!
//! This module is pure: it only builds and parses bytes.

use bezel_core::domain::frame::{RGBA_BYTES, Rect};
use bezel_core::domain::geometry::{Orientation, Size};

/// Last byte of every command.
pub const END: u8 = 0x0A;
/// Bit set on an opcode to read instead of write.
pub const READ: u8 = 0x80;
/// Length of the SYSTEM_VERSION answer.
pub const VERSION_REPLY_LEN: usize = 19;
/// The LE16 field of SET_BRIGHTNESS; 1000 ms, presumably a fade time.
pub const BRIGHTNESS_FADE_MS: u16 = 1000;
/// Shortest humidity-report period the device accepts (0 turns reports off).
pub const MIN_SENSOR_PERIOD_MS: u16 = 500;

/// Opcodes of the family.
pub mod op {
    /// Unused by the reference; its layout is unknown. Never sent.
    pub const WHO_AM_I: u8 = 0x81;
    /// `02 <orientation 0..3> 0a`: on-device rotation.
    pub const SET_ORIENTATION: u8 = 0x02;
    /// `03 <level 0..255> LE16(1000) 0a`: backlight.
    pub const SET_BRIGHTNESS: u8 = 0x03;
    /// `04 <window> LE16 RGB565 0a`: fills the whole screen with one colour.
    pub const FULL: u8 = 0x04;
    /// `05 LE16 x0 y0 x1 y1 0a`, then raw RGB565 LE pixels.
    pub const SET_BITMAP: u8 = 0x05;
    /// As SET_BITMAP, then FastLZ chunks. Disabled in the reference; never sent.
    pub const SET_BITMAP_WITH_FASTLZ: u8 = 0x15;
    /// `06 LE16 period_ms 0a` (3.5" only): periodic temperature/humidity reports.
    pub const ENABLE_HUMITURE_REPORT: u8 = 0x06;
    /// `07 0a`: the last command the reference sends when it stops (exact
    /// meaning unknown).
    pub const FREE: u8 = 0x07;
    /// Read with `c2 0a`: the 19-byte version answer.
    pub const SYSTEM_VERSION: u8 = 0x42;
    /// First byte of a device-to-host humidity report (`ENABLE_HUMITURE_REPORT | READ`).
    pub const HUMITURE_REPORT: u8 = ENABLE_HUMITURE_REPORT | super::READ;
}

/// A read command: `opcode | 0x80`, `0a`.
pub fn read(opcode: u8) -> [u8; 2] {
    [opcode | READ, END]
}

/// The SYSTEM_VERSION read, `c2 0a`.
pub fn system_version() -> [u8; 2] {
    read(op::SYSTEM_VERSION)
}

/// The firmware version in a SYSTEM_VERSION answer: bytes 1..9 as ASCII,
/// trimmed. `None` unless the answer has its 19 bytes and a printable version.
pub fn parse_version(reply: &[u8]) -> Option<String> {
    if reply.len() != VERSION_REPLY_LEN {
        return None;
    }
    let text: String = reply[1..9]
        .iter()
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| char::from(b))
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// On-device orientation (0 portrait, 1 reverse portrait, 2 landscape,
/// 3 reverse landscape).
pub fn set_orientation(orientation: Orientation) -> [u8; 3] {
    [op::SET_ORIENTATION, orientation.index(), END]
}

/// The backlight value for a level in percent, truncated as the reference
/// does: `percent × 255 / 100` (25 % is 63).
pub fn brightness_level(percent: u8) -> u8 {
    (u16::from(percent.min(100)) * 255 / 100) as u8
}

/// SET_BRIGHTNESS with a raw level (0..=255).
pub fn set_brightness(level: u8) -> [u8; 5] {
    let fade = BRIGHTNESS_FADE_MS.to_le_bytes();
    [op::SET_BRIGHTNESS, level, fade[0], fade[1], END]
}

/// FULL: the whole `canvas` (current orientation) in one RGB565 colour.
///
/// The window's far corner is `LE16(W - 1)`, `LE16(H - 1)`. The reference
/// writes the high bytes as `W >> 8` and `H >> 8`, which differs only when a
/// side is a multiple of 256; no WeAct panel is. `None` for an empty canvas or
/// a side beyond 65,536.
pub fn full(canvas: Size, rgb565: u16) -> Option<[u8; 12]> {
    let x1 = u16::try_from(canvas.width.checked_sub(1)?).ok()?;
    let y1 = u16::try_from(canvas.height.checked_sub(1)?).ok()?;
    let mut out = [0u8; 12];
    out[0] = op::FULL;
    out[5..7].copy_from_slice(&x1.to_le_bytes());
    out[7..9].copy_from_slice(&y1.to_le_bytes());
    out[9..11].copy_from_slice(&rgb565.to_le_bytes());
    out[11] = END;
    Some(out)
}

/// The reference's `Clear`: the whole canvas black. Never sent implicitly.
pub fn clear(canvas: Size) -> Option<[u8; 12]> {
    full(canvas, 0)
}

fn window_header(opcode: u8, rect: Rect, canvas: Size) -> Option<[u8; 10]> {
    if rect.is_empty() || rect.right() > canvas.width || rect.bottom() > canvas.height {
        return None;
    }
    let corners = [rect.x, rect.y, rect.right() - 1, rect.bottom() - 1];
    let mut out = [0u8; 10];
    out[0] = opcode;
    for (i, v) in corners.into_iter().enumerate() {
        out[1 + i * 2..3 + i * 2].copy_from_slice(&u16::try_from(v).ok()?.to_le_bytes());
    }
    out[9] = END;
    Some(out)
}

/// SET_BITMAP header for `rect` (inclusive corners x0, y0, x1, y1). As the
/// reference asserts, the rectangle must fit the canvas: `None` when it is
/// empty or does not fit (there is no clipping).
pub fn bitmap_header(rect: Rect, canvas: Size) -> Option<[u8; 10]> {
    window_header(op::SET_BITMAP, rect, canvas)
}

/// SET_BITMAP_WITH_FASTLZ header; same rules as [`bitmap_header`]. Bezel has
/// no FastLZ encoder and never sends it (the reference has it disabled).
pub fn fastlz_bitmap_header(rect: Rect, canvas: Size) -> Option<[u8; 10]> {
    window_header(op::SET_BITMAP_WITH_FASTLZ, rect, canvas)
}

/// One FastLZ chunk after a [`fastlz_bitmap_header`]: `LE16 raw_len`,
/// `LE16 compressed_len`, the compressed block. `None` beyond 16-bit lengths.
pub fn fastlz_chunk(raw_len: usize, compressed: &[u8]) -> Option<Vec<u8>> {
    let raw = u16::try_from(raw_len).ok()?;
    let len = u16::try_from(compressed.len()).ok()?;
    let mut out = Vec::with_capacity(4 + compressed.len());
    out.extend_from_slice(&raw.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(compressed);
    Some(out)
}

/// Bytes per pixel write after a bitmap header: the reference writes the
/// pixels in chunks of `W × 4` bytes (two canvas rows). Never zero.
pub fn data_chunk_len(canvas: Size) -> usize {
    (canvas.width as usize * 4).max(1)
}

/// One colour as RGB565: `(R >> 3) << 11 | (G >> 2) << 5 | B >> 3`.
pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}

/// RGBA8 pixels as RGB565 little-endian, row-major. Alpha is ignored, as in
/// the reference (no compositing).
pub fn rgb565_le(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 2);
    for px in rgba.as_chunks::<RGBA_BYTES>().0 {
        out.extend_from_slice(&rgb565(px[0], px[1], px[2]).to_le_bytes());
    }
    out
}

/// ENABLE_HUMITURE_REPORT (3.5" only): a report every `period_ms`, or none
/// for 0. `None` for periods the device refuses (1..500 ms), which the
/// reference does not send.
pub fn set_sensor_report(period_ms: u16) -> Option<[u8; 4]> {
    if period_ms != 0 && period_ms < MIN_SENSOR_PERIOD_MS {
        return None;
    }
    let p = period_ms.to_le_bytes();
    Some([op::ENABLE_HUMITURE_REPORT, p[0], p[1], END])
}

/// Humidity reports off, `06 00 00 0a`.
pub fn sensor_report_off() -> [u8; 4] {
    let p = 0u16.to_le_bytes();
    [op::ENABLE_HUMITURE_REPORT, p[0], p[1], END]
}

/// FREE, `07 0a`.
pub fn free() -> [u8; 2] {
    [op::FREE, END]
}

/// A temperature/humidity report from the 3.5" (`86 <LE u16> <LE i16> 0a`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorReport {
    /// Temperature in hundredths of a degree.
    pub temperature_centi: u16,
    /// Relative humidity in hundredths of a percent (sign convention unknown).
    pub humidity_centi: i16,
}

impl SensorReport {
    /// Parses one 6-byte report; `None` for anything else.
    pub fn parse(frame: &[u8]) -> Option<SensorReport> {
        match frame {
            [op::HUMITURE_REPORT, t0, t1, h0, h1, END] => Some(SensorReport {
                temperature_centi: u16::from_le_bytes([*t0, *t1]),
                humidity_centi: i16::from_le_bytes([*h0, *h1]),
            }),
            _ => None,
        }
    }

    /// Temperature in degrees.
    pub fn temperature(self) -> f32 {
        f32::from(self.temperature_centi) / 100.0
    }

    /// Relative humidity in percent.
    pub fn humidity(self) -> f32 {
        f32::from(self.humidity_centi) / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::frame::Frame;

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

    #[test]
    fn packets_match_the_reference_vectors() {
        // docs: protocol-weact.md § 10 (verified against the Python reference).
        assert_eq!(hex(&system_version()), "c20a");
        assert_eq!(hex(&set_brightness(brightness_level(0))), "0300e8030a");
        assert_eq!(hex(&set_brightness(brightness_level(25))), "033fe8030a");
        assert_eq!(hex(&set_brightness(brightness_level(100))), "03ffe8030a");

        let portrait = Size::new(320, 480);
        let red = rgb565(255, 0, 0);
        assert_eq!(
            hex(&full(portrait, red).unwrap()),
            "04000000003f01df0100f80a"
        );
        assert_eq!(hex(&clear(portrait).unwrap()), "04000000003f01df0100000a");
        assert_eq!(
            hex(&full(portrait.transposed(), red).unwrap()),
            "0400000000df013f0100f80a"
        );

        assert_eq!(hex(&set_orientation(Orientation::Portrait)), "02000a");
        assert_eq!(hex(&set_orientation(Orientation::Landscape)), "02020a");
        assert_eq!(
            hex(&set_orientation(Orientation::ReversePortrait)),
            "02010a"
        );
        assert_eq!(
            hex(&set_orientation(Orientation::ReverseLandscape)),
            "02030a"
        );

        assert_eq!(hex(&set_sensor_report(1000).unwrap()), "06e8030a");
        assert_eq!(hex(&free()), "070a");
        // ScreenOff on the 3.5": brightness 0, humidity reports off, FREE.
        let off = [
            set_brightness(brightness_level(0)).to_vec(),
            sensor_report_off().to_vec(),
            free().to_vec(),
        ];
        assert_eq!(off.map(|p| hex(&p)), ["0300e8030a", "0600000a", "070a"]);

        let header = bitmap_header(Rect::new(10, 20, 3, 2), portrait).unwrap();
        assert_eq!(hex(&header), "050a0014000c0015000a");
        assert_eq!(
            hex(&rgb565_le(grad().as_rgba())),
            "00f8e0071f00ffff0000aa11"
        );

        // The 0.96": Full(blue), 80 x 160.
        let blue = rgb565(0, 0, 255);
        assert_eq!(
            hex(&full(Size::new(80, 160), blue).unwrap()),
            "04000000004f009f001f000a"
        );
    }

    #[test]
    fn full_uses_the_real_far_corner() {
        // The reference would write 0x1ff (511) for a 256-wide canvas.
        let p = full(Size::new(256, 512), 0).unwrap();
        assert_eq!(&p[5..9], &[0xFF, 0x00, 0xFF, 0x01]);
        assert!(full(Size::new(0, 10), 0).is_none());
        assert!(full(Size::new(70_000, 10), 0).is_none());
    }

    #[test]
    fn bitmaps_must_fit_the_canvas() {
        let canvas = Size::new(80, 160);
        let whole = bitmap_header(Rect::of(canvas), canvas).unwrap();
        assert_eq!(hex(&whole), "05000000004f009f000a");
        assert!(bitmap_header(Rect::new(79, 0, 2, 1), canvas).is_none());
        assert!(bitmap_header(Rect::new(0, 159, 1, 2), canvas).is_none());
        assert!(bitmap_header(Rect::new(0, 0, 0, 1), canvas).is_none());
        let huge = Size::new(100_000, 10);
        assert!(bitmap_header(Rect::new(70_000, 0, 1, 1), huge).is_none());
        assert_eq!(data_chunk_len(Size::new(320, 480)), 1280);
        assert_eq!(data_chunk_len(Size::new(160, 80)), 640);
        assert_eq!(data_chunk_len(Size::new(0, 0)), 1);
    }

    #[test]
    fn fastlz_framing() {
        let canvas = Size::new(320, 480);
        let h = fastlz_bitmap_header(Rect::new(10, 20, 3, 2), canvas).unwrap();
        assert_eq!(hex(&h), "150a0014000c0015000a");
        assert_eq!(
            hex(&fastlz_chunk(1280, &[1, 2, 3]).unwrap()),
            "00050300010203"
        );
        assert!(fastlz_chunk(70_000, &[]).is_none());
    }

    #[test]
    fn version_answers() {
        let mut reply = vec![0xC2];
        reply.extend_from_slice(b"V1.0.0.0");
        reply.resize(VERSION_REPLY_LEN, 0);
        assert_eq!(parse_version(&reply).as_deref(), Some("V1.0.0.0"));
        assert!(parse_version(&reply[..18]).is_none());
        assert!(parse_version(&[0; VERSION_REPLY_LEN]).is_none());
        assert_eq!(hex(&read(op::WHO_AM_I)), "810a");
    }

    #[test]
    fn sensor_reports() {
        assert!(set_sensor_report(499).is_none());
        assert_eq!(set_sensor_report(0), Some(sensor_report_off()));
        assert_eq!(hex(&set_sensor_report(u16::MAX).unwrap()), "06ffff0a");
        let r = SensorReport::parse(&[0x86, 0x2A, 0x09, 0x9C, 0xFF, 0x0A]).unwrap();
        assert_eq!(r.temperature_centi, 2346);
        assert_eq!(r.humidity_centi, -100);
        assert!((r.temperature() - 23.46).abs() < 1e-4);
        assert!((r.humidity() + 1.0).abs() < 1e-4);
        assert!(SensorReport::parse(&[0x86, 0, 0, 0, 0, 0]).is_none());
        assert!(SensorReport::parse(&[0x86]).is_none());
    }

    #[test]
    fn rgb565_little_endian_ignores_alpha() {
        assert_eq!(hex(&rgb565_le(&[0x12, 0x34, 0x56, 0])), "aa11");
        assert!(rgb565_le(&[]).is_empty());
    }
}
