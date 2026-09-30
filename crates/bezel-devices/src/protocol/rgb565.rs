//! RGB565, the 16-bit pixel format of the serial families (Turing rev A,
//! XuanFang rev B, Kipye rev D, WeAct): 5 bits of red, 6 of green, 5 of blue;
//! alpha is dropped. Families differ only in byte order.

use bezel_core::domain::frame::RGBA_BYTES;

/// Bytes per RGB565 pixel.
pub const PIXEL_BYTES: usize = 2;

/// Packs one color.
pub const fn pack(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}

fn encode<'a>(
    pixels: impl ExactSizeIterator<Item = &'a [u8; RGBA_BYTES]>,
    bytes: fn(u16) -> [u8; PIXEL_BYTES],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * PIXEL_BYTES);
    for px in pixels {
        out.extend_from_slice(&bytes(pack(px[0], px[1], px[2])));
    }
    out
}

/// RGBA pixels as little-endian RGB565 (rev A, WeAct).
pub fn le(rgba: &[u8]) -> Vec<u8> {
    encode(rgba.as_chunks::<RGBA_BYTES>().0.iter(), u16::to_le_bytes)
}

/// RGBA pixels as big-endian RGB565 (rev B, rev D).
pub fn be(rgba: &[u8]) -> Vec<u8> {
    encode(rgba.as_chunks::<RGBA_BYTES>().0.iter(), u16::to_be_bytes)
}

/// [`be`] with the pixel order reversed: the rectangle turned 180°.
pub fn be_reversed(rgba: &[u8]) -> Vec<u8> {
    encode(
        rgba.as_chunks::<RGBA_BYTES>().0.iter().rev(),
        u16::to_be_bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_orders_bytes() {
        assert_eq!(pack(255, 0, 0), 0xF800);
        assert_eq!(pack(0x12, 0x34, 0x56), 0x11AA);
        let rgba = [255, 0, 0, 9, 0, 0, 255, 9];
        assert_eq!(le(&rgba), vec![0x00, 0xF8, 0x1F, 0x00]);
        assert_eq!(be(&rgba), vec![0xF8, 0x00, 0x00, 0x1F]);
        assert_eq!(be_reversed(&rgba), vec![0x00, 0x1F, 0xF8, 0x00]);
        assert!(le(&[1, 2, 3]).is_empty(), "a partial pixel is dropped");
    }
}
