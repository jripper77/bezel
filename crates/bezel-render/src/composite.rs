//! Pixel math on premultiplied RGBA8 buffers: source-over blending of a
//! layer onto the canvas and the conversion to the straight-alpha [`Frame`].

use bezel_core::domain::frame::{Frame, RGBA_BYTES};
use bezel_core::domain::geometry::Size;
use tiny_skia::Pixmap;

/// A premultiplied RGBA8 image placed at `(x, y)` on a larger buffer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sprite<'a> {
    /// Row-major premultiplied RGBA8 bytes.
    pub data: &'a [u8],
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Left edge on the destination.
    pub x: i32,
    /// Top edge on the destination.
    pub y: i32,
    /// Every pixel is fully opaque (drawn by copying rows).
    pub opaque: bool,
}

/// A premultiplied RGBA8 destination.
#[derive(Debug)]
pub(crate) struct Target<'a> {
    /// Row-major premultiplied RGBA8 bytes.
    pub data: &'a mut [u8],
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// `x / 255` rounded, exact for every `x` in `0..=255 * 255`.
const fn div255(x: u32) -> u32 {
    (x + 128 + ((x + 128) >> 8)) >> 8
}

/// The overlap of the sprite with the target as `(src_x, src_y, dst_x, dst_y, w, h)`.
fn overlap(
    target: &Target<'_>,
    sprite: &Sprite<'_>,
) -> Option<(usize, usize, usize, usize, usize, usize)> {
    let left = sprite.x.max(0);
    let top = sprite.y.max(0);
    let right = (i64::from(sprite.x) + i64::from(sprite.width)).min(i64::from(target.width));
    let bottom = (i64::from(sprite.y) + i64::from(sprite.height)).min(i64::from(target.height));
    if i64::from(left) >= right || i64::from(top) >= bottom {
        return None;
    }
    let w = (right - i64::from(left)) as usize;
    let h = (bottom - i64::from(top)) as usize;
    let src_x = (left - sprite.x) as usize;
    let src_y = (top - sprite.y) as usize;
    Some((src_x, src_y, left as usize, top as usize, w, h))
}

/// Draws `sprite` over `target` (source-over) with `opacity` 0..=255.
pub(crate) fn blend(target: &mut Target<'_>, sprite: &Sprite<'_>, opacity: u8) {
    let Some((sx, sy, dx, dy, w, h)) = overlap(target, sprite) else {
        return;
    };
    let src_stride = sprite.width as usize * RGBA_BYTES;
    let dst_stride = target.width as usize * RGBA_BYTES;
    for row in 0..h {
        let s = (sy + row) * src_stride + sx * RGBA_BYTES;
        let d = (dy + row) * dst_stride + dx * RGBA_BYTES;
        let (Some(src), Some(dst)) = (
            sprite.data.get(s..s + w * RGBA_BYTES),
            target.data.get_mut(d..d + w * RGBA_BYTES),
        ) else {
            return;
        };
        if sprite.opaque && opacity == 255 {
            dst.copy_from_slice(src);
        } else {
            blend_row(dst, src, u32::from(opacity));
        }
    }
}

/// True when every premultiplied pixel is fully opaque.
pub(crate) fn is_opaque(data: &[u8]) -> bool {
    data.as_chunks::<RGBA_BYTES>()
        .0
        .iter()
        .all(|px| px[3] == 255)
}

fn blend_row(dst: &mut [u8], src: &[u8], opacity: u32) {
    for (d, s) in dst
        .as_chunks_mut::<RGBA_BYTES>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<RGBA_BYTES>().0)
    {
        let mut px = [
            u32::from(s[0]),
            u32::from(s[1]),
            u32::from(s[2]),
            u32::from(s[3]),
        ];
        if opacity < 255 {
            px = px.map(|c| div255(c * opacity));
        }
        match px[3] {
            0 => {}
            255 => {
                for (o, c) in d.iter_mut().zip(px) {
                    *o = c as u8;
                }
            }
            a => {
                let inv = 255 - a;
                for (o, c) in d.iter_mut().zip(px) {
                    *o = (c + div255(u32::from(*o) * inv)).min(255) as u8;
                }
            }
        }
    }
}

/// Straight-alpha RGBA of one premultiplied pixel.
pub(crate) fn demultiply(px: [u8; 4]) -> [u8; 4] {
    match px[3] {
        255 => px,
        0 => [0, 0, 0, 0],
        a => {
            let a32 = u32::from(a);
            let un = |c: u8| ((u32::from(c) * 255 + a32 / 2) / a32).min(255) as u8;
            [un(px[0]), un(px[1]), un(px[2]), a]
        }
    }
}

/// Premultiplies straight RGBA8 bytes in place.
pub(crate) fn premultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<RGBA_BYTES>().0 {
        let a = u32::from(px[3]);
        if a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = div255(u32::from(*c) * a) as u8;
        }
    }
}

/// Keeps premultiplied bytes valid (color ≤ alpha) after resampling.
pub(crate) fn clamp_premultiplied(data: &mut [u8]) {
    for px in data.as_chunks_mut::<RGBA_BYTES>().0 {
        let a = px[3];
        for c in &mut px[..3] {
            *c = (*c).min(a);
        }
    }
}

/// The canvas as a straight-alpha frame of `size`.
pub(crate) fn to_frame(canvas: &Pixmap, size: Size) -> Frame {
    let mut pixels = canvas.data().to_vec();
    for px in pixels
        .as_chunks_mut::<RGBA_BYTES>()
        .0
        .iter_mut()
        .filter(|px| px[3] != 255)
    {
        *px = demultiply(*px);
    }
    Frame::from_rgba(size, pixels).unwrap_or_else(|| Frame::filled(size, Default::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div255_is_exact() {
        for x in 0..=(255 * 255) {
            assert_eq!(div255(x), (x as f64 / 255.0).round() as u32, "x = {x}");
        }
    }

    #[test]
    fn blends_with_opacity_and_clips() {
        let mut dst = vec![0u8; 3 * 2 * 4];
        for px in dst.as_chunks_mut::<4>().0 {
            *px = [0, 0, 200, 255];
        }
        // 2x1 sprite: opaque red, half-transparent green (premultiplied).
        let src = [255, 0, 0, 255, 0, 128, 0, 128];
        let mut target = Target {
            data: &mut dst,
            width: 3,
            height: 2,
        };
        let sprite = Sprite {
            data: &src,
            width: 2,
            height: 1,
            x: 2,
            y: 1,
            opaque: false,
        };
        blend(&mut target, &sprite, 255);
        assert_eq!(&dst[(3 + 2) * 4..(3 + 2) * 4 + 4], &[255, 0, 0, 255]);
        assert_eq!(&dst[..4], &[0, 0, 200, 255], "outside the sprite");

        let mut dst = vec![0, 0, 200, 255];
        let mut target = Target {
            data: &mut dst,
            width: 1,
            height: 1,
        };
        let sprite = Sprite {
            data: &src[4..],
            width: 1,
            height: 1,
            x: 0,
            y: 0,
            opaque: false,
        };
        blend(&mut target, &sprite, 255);
        assert_eq!(dst, vec![0, 128, 100, 255]);

        let mut dst = vec![0, 0, 0, 255];
        let mut target = Target {
            data: &mut dst,
            width: 1,
            height: 1,
        };
        let sprite = Sprite {
            data: &src[..4],
            width: 1,
            height: 1,
            x: 0,
            y: 0,
            opaque: false,
        };
        blend(&mut target, &sprite, 128);
        let off = Sprite { x: -5, ..sprite };
        blend(&mut target, &off, 255);
        assert_eq!(
            dst,
            vec![128, 0, 0, 255],
            "half red; the sprite outside changes nothing"
        );
    }

    #[test]
    fn opaque_sprites_are_copied() {
        let mut dst = vec![9u8; 2 * 4];
        let mut target = Target {
            data: &mut dst,
            width: 2,
            height: 1,
        };
        let src = [1, 2, 3, 255];
        let sprite = Sprite {
            data: &src,
            width: 1,
            height: 1,
            x: 1,
            y: 0,
            opaque: true,
        };
        blend(&mut target, &sprite, 255);
        assert_eq!(dst, vec![9, 9, 9, 9, 1, 2, 3, 255]);
    }

    #[test]
    fn multiplies_and_demultiplies() {
        let mut px = [200, 100, 50, 128, 10, 20, 30, 255, 9, 9, 9, 0];
        premultiply(&mut px);
        assert_eq!(&px[..4], &[100, 50, 25, 128]);
        assert_eq!(&px[4..8], &[10, 20, 30, 255]);
        assert_eq!(&px[8..], &[0, 0, 0, 0]);
        assert_eq!(demultiply([100, 50, 25, 128]), [199, 100, 50, 128]);
        assert_eq!(demultiply([1, 2, 3, 0]), [0, 0, 0, 0]);
        assert!(is_opaque(&[1, 2, 3, 255]) && !is_opaque(&[1, 2, 3, 255, 0, 0, 0, 254]));
        let mut bad = [200, 10, 90, 100];
        clamp_premultiplied(&mut bad);
        assert_eq!(bad, [100, 10, 90, 100]);
    }
}
