//! Framing the pictures of a video decoded on the host
//! (D-2026-10-01-video-background-framing-3). The decoder hands over the
//! whole source picture, never turned or cropped; [`frame_picture`] turns,
//! crops, scales and pads it into its target as [`geometry`] says, the same
//! geometry the conversion for a screen and the poster use, so a framing
//! edit never restarts the decoder. One pass, bilinear, in integer
//! arithmetic: the same picture on every system.

use std::borrow::Cow;

use super::{FramingGeometry, ResolvedFraming, geometry};
use crate::domain::frame::{Frame, RGBA_BYTES, Rect, Rgba};
use crate::domain::geometry::Size;

/// `picture`, a whole decoded picture of the video, framed into a `target`
/// picture as `framing` says ([`geometry`] of the picture's own size): the
/// picture itself when it already is that picture (nothing turned, cropped,
/// scaled or padded), else a new one.
pub fn frame_picture<'p>(
    picture: &'p Frame,
    framing: &ResolvedFraming,
    target: Size,
) -> Cow<'p, Frame> {
    let geometry = geometry(picture.size(), framing, target);
    if geometry.is_identity(picture.size()) {
        Cow::Borrowed(picture)
    } else {
        Cow::Owned(geometry.apply(picture))
    }
}

/// Fixed-point steps per pixel of the sampling positions (and of the
/// weights).
const ONE: i64 = 256;

/// A source pixel's byte offset as a linear function of one axis of the
/// turned picture: `(base + step * i) * 4`.
#[derive(Clone, Copy)]
struct Linear {
    base: i64,
    step: i64,
}

impl Linear {
    fn at(self, i: i64) -> usize {
        usize::try_from(self.base + self.step * i).unwrap_or(0) * RGBA_BYTES
    }
}

/// How the turned picture's columns and rows map to the source picture's
/// pixels, for a source of `width` x `height` turned `turns` clockwise.
fn layout(source: Size, turns: u8) -> (Linear, Linear) {
    let (w, h) = (i64::from(source.width), i64::from(source.height));
    let line = |base, step| Linear { base, step };
    match turns % 4 {
        // Turned (i, j) is source (j, h - 1 - i).
        1 => (line((h - 1) * w, -w), line(0, 1)),
        // Turned (i, j) is source (w - 1 - i, h - 1 - j).
        2 => (line(w - 1, -1), line((h - 1) * w, -w)),
        // Turned (i, j) is source (w - 1 - j, i).
        3 => (line(0, w), line(w - 1, -1)),
        _ => (line(0, 1), line(0, w)),
    }
}

/// Where each pixel of the produced picture samples the source along one
/// axis: the byte offsets of its two neighbouring source pixels and the
/// weight of the second (0..[`ONE`]).
struct Axis {
    near: Vec<usize>,
    far: Vec<usize>,
    weight: Vec<u32>,
}

impl Axis {
    /// `out` samples spread over the `kept` turned pixels from `from`, each
    /// at the centre of its output pixel; the last kept pixel repeats past
    /// the edge.
    fn new(from: u32, kept: u32, out: u32, offset: Linear) -> Self {
        let (from, kept, out) = (i64::from(from), i64::from(kept), i64::from(out));
        let last = from + kept - 1;
        let len = usize::try_from(out).unwrap_or(0);
        let mut axis = Axis {
            near: Vec::with_capacity(len),
            far: Vec::with_capacity(len),
            weight: Vec::with_capacity(len),
        };
        for o in 0..out {
            let centre = from * ONE + (2 * o + 1) * kept * ONE / (2 * out) - ONE / 2;
            let at = centre.clamp(from * ONE, last * ONE);
            let i = at / ONE;
            axis.near.push(offset.at(i));
            axis.far.push(offset.at((i + 1).min(last)));
            axis.weight.push(u32::try_from(at % ONE).unwrap_or(0));
        }
        axis
    }
}

/// The weighted mean of four source pixels into `out`, channel by channel:
/// `near`/`far` columns, `top`/`bottom` rows.
fn blend(out: &mut [u8], source: &[u8], corners: [usize; 4], wx: u32, wy: u32) {
    let one = ONE as u32;
    let [top_near, top_far, bottom_near, bottom_far] = corners;
    for (k, channel) in out.iter_mut().enumerate() {
        let at = |offset: usize| u32::from(source[offset + k]);
        let top = at(top_near) * (one - wx) + at(top_far) * wx;
        let bottom = at(bottom_near) * (one - wx) + at(bottom_far) * wx;
        let mean = (top * (one - wy) + bottom * wy + one * one / 2) / (one * one);
        *channel = u8::try_from(mean).unwrap_or(u8::MAX);
    }
}

impl FramingGeometry {
    /// `picture` (the whole source picture) turned, cropped, scaled and
    /// padded into a picture of [`Self::size`]: what the conversion for a
    /// screen makes of each picture, for the pictures decoded on the host.
    /// A crop or pad that does not fit is clipped; an empty picture gives
    /// only the pad color (opaque black without a pad).
    pub fn apply(&self, picture: &Frame) -> Frame {
        let color = self.pad.map_or(Rgba::BLACK, |pad| pad.color);
        let mut out = Frame::filled(self.size, color);
        let source = picture.size();
        let turns = self.turns % 4;
        let turned = if turns % 2 == 1 {
            source.transposed()
        } else {
            source
        };
        let crop = self.crop.unwrap_or(Rect::of(turned)).clip(turned);
        let place = self.pad.map_or(Rect::of(self.size), |pad| {
            Rect::new(pad.x, pad.y, pad.scaled.width, pad.scaled.height)
        });
        let place = place.clip(self.size);
        if crop.is_empty() || place.is_empty() {
            return out;
        }
        let (columns, rows) = layout(source, turns);
        let xs = Axis::new(crop.x, crop.width, place.width, columns);
        let ys = Axis::new(crop.y, crop.height, place.height, rows);
        let pixels = picture.as_rgba();
        let stride = self.size.width as usize * RGBA_BYTES;
        let (left, span) = (
            place.x as usize * RGBA_BYTES,
            place.width as usize * RGBA_BYTES,
        );
        let lines = out.as_rgba_mut().chunks_exact_mut(stride);
        let rows = ys.near.iter().zip(&ys.far).zip(&ys.weight);
        for (line, ((&top, &bottom), &wy)) in lines.skip(place.y as usize).zip(rows) {
            let samples = xs.near.iter().zip(&xs.far).zip(&xs.weight);
            let (cells, _) = line[left..left + span].as_chunks_mut::<RGBA_BYTES>();
            for (cell, ((&near, &far), &wx)) in cells.iter_mut().zip(samples) {
                let corners = [top + near, top + far, bottom + near, bottom + far];
                blend(cell, pixels, corners, wx, wy);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::framing::{FramingPosition, Pad, Permille, VideoFit, Zoom};

    /// A picture whose pixel (x, y) is (x, y, 7, 255): where each pixel came
    /// from shows in the result.
    fn coordinates(size: Size) -> Frame {
        let mut pixels = Vec::new();
        for y in 0..size.height {
            for x in 0..size.width {
                pixels.extend([x as u8, y as u8, 7, 255]);
            }
        }
        Frame::from_rgba(size, pixels).expect("pixels")
    }

    /// Where the pixel at (x, y) of `frame` came from.
    fn from(frame: &Frame, x: u32, y: u32) -> (u8, u8) {
        let p = frame.pixel(x, y).expect("inside");
        (p.r, p.g)
    }

    #[test]
    fn turning_matches_the_lossless_rotation() {
        let source = coordinates(Size::new(6, 4));
        for turns in 0..4 {
            let size = source.rotated(turns).size();
            let turned = FramingGeometry::turning(turns, size).apply(&source);
            assert_eq!(turned, source.rotated(turns), "{turns} turns");
        }
    }

    #[test]
    fn the_crop_and_the_pad_place_the_picture() {
        let source = coordinates(Size::new(8, 4));
        // The 4x2 block at (2, 1), unscaled, at (1, 1) of a 6x4 picture.
        let green = Rgba::opaque(0, 200, 0);
        let geometry = FramingGeometry {
            turns: 0,
            crop: Some(Rect::new(2, 1, 4, 2)),
            size: Size::new(6, 4),
            pad: Some(Pad {
                scaled: Size::new(4, 2),
                x: 1,
                y: 1,
                color: green,
            }),
        };
        let out = geometry.apply(&source);
        assert_eq!(out.size(), Size::new(6, 4));
        assert_eq!(out.pixel(0, 0), Some(green));
        assert_eq!(out.pixel(5, 3), Some(green));
        assert_eq!(from(&out, 1, 1), (2, 1));
        assert_eq!(from(&out, 4, 2), (5, 2));
        // Scaled 2x: each source pixel covers two, blended at the seams.
        let doubled = FramingGeometry {
            crop: Some(Rect::new(0, 0, 2, 1)),
            size: Size::new(4, 2),
            pad: None,
            turns: 0,
        };
        let out = doubled.apply(&source);
        let reds: Vec<u8> = (0..4).map(|x| from(&out, x, 0).0).collect();
        assert_eq!(reds, [0, 0, 1, 1]);
        assert_eq!(out.pixel(3, 1).map(|p| p.b), Some(7));
        // Halved: every other pixel, blended in between.
        let halved = FramingGeometry::turning(0, Size::new(4, 2)).apply(&source);
        assert_eq!(from(&halved, 0, 0), (1, 1));
        assert_eq!(from(&halved, 3, 1), (7, 3));
    }

    #[test]
    fn framing_a_picture_follows_the_geometry() {
        // The Dragon Ball picture (panel-native) turned back onto its
        // landscape canvas: 270 degrees, nothing else.
        let source = coordinates(Size::new(4, 16));
        let canvas = Size::new(16, 4);
        let dragon = ResolvedFraming::plain(3);
        let framed = frame_picture(&source, &dragon, canvas);
        assert!(matches!(framed, Cow::Owned(_)));
        assert_eq!(*framed, source.rotated(3));
        // A picture that already is the target is handed back as it is.
        let same = frame_picture(&source, &ResolvedFraming::plain(0), Size::new(4, 16));
        assert!(matches!(same, Cow::Borrowed(_)));
        // Contained in a wider target, at the end of the axis.
        let fit = ResolvedFraming {
            fit: VideoFit::Contain,
            zoom: Zoom::NONE,
            position: FramingPosition {
                x: Permille::END,
                y: Permille::CENTER,
            },
            pad: Rgba::opaque(9, 9, 9),
            turns: 0,
        };
        let contained = frame_picture(&source, &fit, Size::new(8, 16));
        assert_eq!(contained.pixel(0, 0), Some(Rgba::opaque(9, 9, 9)));
        assert_eq!(from(&contained, 4, 0), (0, 0));
        assert_eq!(from(&contained, 7, 15), (3, 15));
    }

    #[test]
    fn empty_pictures_and_odd_geometries_never_panic() {
        let empty = Frame::filled(Size::new(0, 5), Rgba::WHITE);
        let out = FramingGeometry::turning(1, Size::new(3, 2)).apply(&empty);
        assert_eq!(out, Frame::filled(Size::new(3, 2), Rgba::BLACK));
        // A crop past the picture is clipped to it.
        let source = coordinates(Size::new(4, 4));
        let wide = FramingGeometry {
            turns: 0,
            crop: Some(Rect::new(2, 2, 10, 10)),
            size: Size::new(2, 2),
            pad: None,
        };
        let out = wide.apply(&source);
        assert_eq!(from(&out, 0, 0), (2, 2));
        assert_eq!(from(&out, 1, 1), (3, 3));
        let outside = FramingGeometry {
            crop: Some(Rect::new(9, 9, 1, 1)),
            ..wide
        };
        assert_eq!(
            outside.apply(&source),
            Frame::filled(Size::new(2, 2), Rgba::BLACK)
        );
    }
}
