//! Path geometry: rounded rectangles, arcs in gauge angles and blocks.
//!
//! Gauge angles are degrees with 0 = 12 o'clock and clockwise positive.

use bezel_core::domain::theme::Segments;
use tiny_skia::{Path, PathBuilder, Rect};

/// Distance of a cubic control point for a quarter circle of radius 1.
const KAPPA: f32 = 0.552_284_8;

/// True for a positive number (false for NaN).
pub(crate) fn positive(v: f32) -> bool {
    v > 0.0
}

/// A rectangle path; `None` when empty or not finite.
pub(crate) fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    Rect::from_xywh(x, y, w, h).map(PathBuilder::from_rect)
}

/// A rectangle with rounded corners (radius clamped to half the shorter side).
pub(crate) fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let r = radius.min(w / 2.0).min(h / 2.0);
    if !positive(r) {
        return rect(x, y, w, h);
    }
    let k = r * (1.0 - KAPPA);
    let (l, t, rr, b) = (x, y, x + w, y + h);
    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rr - r, t);
    pb.cubic_to(rr - k, t, rr, t + k, rr, t + r);
    pb.line_to(rr, b - r);
    pb.cubic_to(rr, b - k, rr - k, b, rr - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + k, b, l, b - k, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + k, l + k, t, l + r, t);
    pb.close();
    pb.finish()
}

/// Independent corners normalized to avoid overlapping adjacent arcs.
pub(crate) fn rounded_corners(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Option<Path> {
    Rect::from_xywh(x, y, w, h)?;
    let mut r = radii.map(|v| if v.is_finite() { v.max(0.) } else { 0. });
    let scale = [
        w / (r[0] + r[1]),
        w / (r[2] + r[3]),
        h / (r[0] + r[3]),
        h / (r[1] + r[2]),
    ]
    .into_iter()
    .fold(1_f32, f32::min);
    r = r.map(|v| v * scale);
    let [a, b, c, d] = r;
    let [ak, bk, ck, dk] = r.map(|v| v * (1. - KAPPA));
    let (rr, bb) = (x + w, y + h);
    let mut p = PathBuilder::new();
    p.move_to(x + a, y);
    p.line_to(rr - b, y);
    p.cubic_to(rr - bk, y, rr, y + bk, rr, y + b);
    p.line_to(rr, bb - c);
    p.cubic_to(rr, bb - ck, rr - ck, bb, rr - c, bb);
    p.line_to(x + d, bb);
    p.cubic_to(x + dk, bb, x, bb - dk, x, bb - d);
    p.line_to(x, y + a);
    p.cubic_to(x, y + ak, x + ak, y, x + a, y);
    p.close();
    p.finish()
}

/// The point at `angle` on the circle `(cx, cy, r)`.
pub(crate) fn polar(cx: f32, cy: f32, r: f32, angle: f32) -> (f32, f32) {
    let a = angle.to_radians();
    (cx + r * a.sin(), cy - r * a.cos())
}

/// Unit tangent at `angle` when the angle grows (clockwise on screen).
fn tangent(angle: f32) -> (f32, f32) {
    let a = angle.to_radians();
    (a.cos(), a.sin())
}

/// Appends an arc from `start` over `sweep` degrees (negative =
/// counter-clockwise); a sweep of 360° or more is a closed circle.
pub(crate) fn push_arc(pb: &mut PathBuilder, circle: (f32, f32, f32), start: f32, sweep: f32) {
    let (cx, cy, r) = circle;
    if !(r > 0.0 && start.is_finite() && sweep.abs() > 1e-3) {
        return;
    }
    let full = sweep.abs() >= 360.0;
    let sweep = sweep.clamp(-360.0, 360.0);
    let parts = (sweep.abs() / 90.0).ceil().max(1.0) as u16;
    let step = sweep / f32::from(parts);
    let k = 4.0 / 3.0 * (step.to_radians() / 4.0).tan() * r;
    let (x0, y0) = polar(cx, cy, r, start);
    pb.move_to(x0, y0);
    for i in 0..parts {
        let a0 = start + step * f32::from(i);
        let a1 = a0 + step;
        let (p0x, p0y) = polar(cx, cy, r, a0);
        let (p3x, p3y) = polar(cx, cy, r, a1);
        let (t0x, t0y) = tangent(a0);
        let (t1x, t1y) = tangent(a1);
        pb.cubic_to(
            p0x + k * t0x,
            p0y + k * t0y,
            p3x - k * t1x,
            p3y - k * t1y,
            p3x,
            p3y,
        );
    }
    if full {
        pb.close();
    }
}

/// Splits `length` into blocks `(start, end)` measured from the origin.
/// Without segments there is one block. `wrap` puts a gap after the last
/// block too (a full ring, where the last block meets the first).
pub(crate) fn blocks(length: f32, segments: Option<Segments>, wrap: bool) -> Vec<(f32, f32)> {
    let whole = vec![(0.0, length)];
    let Some(s) = segments else {
        return whole;
    };
    let count = s.count.max(1);
    let n = f32::from(count);
    let gap = if s.gap.is_finite() {
        s.gap.max(0.0)
    } else {
        0.0
    };
    let gaps = if wrap { n } else { n - 1.0 };
    let size = (length - gap * gaps) / n;
    if !positive(size) {
        return whole;
    }
    (0..count)
        .map(|i| {
            let a = f32::from(i) * (size + gap);
            (a, a + size)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rects_clamp_the_radius() {
        let p = rounded_rect(0.0, 0.0, 10.0, 4.0, 100.0).expect("path");
        let b = p.bounds();
        assert_eq!(
            (b.left(), b.top(), b.right(), b.bottom()),
            (0.0, 0.0, 10.0, 4.0)
        );
        assert!(rounded_rect(0.0, 0.0, 0.0, 4.0, 1.0).is_none());
        assert!(rounded_rect(0.0, 0.0, 5.0, 4.0, f32::NAN).is_some());
        assert!(rect(0.0, 0.0, f32::INFINITY, 1.0).is_none());
    }

    #[test]
    fn arcs_start_at_twelve_and_turn_clockwise() {
        let (x, y) = polar(50.0, 50.0, 10.0, 0.0);
        assert!((x - 50.0).abs() < 1e-4 && (y - 40.0).abs() < 1e-4);
        let (x, y) = polar(50.0, 50.0, 10.0, 90.0);
        assert!((x - 60.0).abs() < 1e-4 && (y - 50.0).abs() < 1e-4);

        let mut pb = PathBuilder::new();
        push_arc(&mut pb, (50.0, 50.0, 10.0), 0.0, 90.0);
        let b = pb.finish().expect("arc").bounds();
        assert!((b.left() - 50.0).abs() < 1e-3 && (b.bottom() - 50.0).abs() < 1e-3);
        assert!((b.right() - 60.0).abs() < 0.1 && (b.top() - 40.0).abs() < 0.1);

        let mut pb = PathBuilder::new();
        push_arc(&mut pb, (50.0, 50.0, 10.0), 0.0, -720.0);
        let b = pb.finish().expect("circle").bounds();
        assert!((b.width() - 20.0).abs() < 0.1 && (b.height() - 20.0).abs() < 0.1);

        let mut pb = PathBuilder::new();
        push_arc(&mut pb, (50.0, 50.0, 10.0), 0.0, 0.0);
        push_arc(&mut pb, (50.0, 50.0, 0.0), 0.0, 90.0);
        assert!(pb.finish().is_none());
    }

    #[test]
    fn blocks_split_with_gaps() {
        assert_eq!(blocks(10.0, None, false), vec![(0.0, 10.0)]);
        let s = Segments { count: 4, gap: 4.0 };
        assert_eq!(
            blocks(64.0, Some(s), false),
            vec![(0.0, 13.0), (17.0, 30.0), (34.0, 47.0), (51.0, 64.0)]
        );
        let ring = blocks(
            360.0,
            Some(Segments {
                count: 4,
                gap: 10.0,
            }),
            true,
        );
        assert_eq!(ring[3], (270.0, 350.0));
        let crowded = Segments {
            count: 10,
            gap: 20.0,
        };
        assert_eq!(blocks(64.0, Some(crowded), false), vec![(0.0, 64.0)]);
        let nan = Segments {
            count: 0,
            gap: f32::NAN,
        };
        assert_eq!(blocks(8.0, Some(nan), false), vec![(0.0, 8.0)]);
    }
}
