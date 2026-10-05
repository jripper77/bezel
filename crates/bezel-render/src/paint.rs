//! Theme paints (solid colors and linear gradients) as tiny-skia paints.

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::theme::{BoxF, Paint};
use tiny_skia::{Color, GradientStop, LinearGradient, Point, Shader, SpreadMode, Transform};

/// A tiny-skia color.
pub(crate) fn color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// An anti-aliased paint with `shader`.
fn with_shader(shader: Shader<'static>) -> tiny_skia::Paint<'static> {
    tiny_skia::Paint {
        shader,
        anti_alias: true,
        ..tiny_skia::Paint::default()
    }
}

/// An anti-aliased solid paint.
pub(crate) fn solid(c: Rgba) -> tiny_skia::Paint<'static> {
    with_shader(Shader::SolidColor(color(c)))
}

/// The paint of `paint` spread over `area` (canvas coordinates); `None` for
/// a gradient without stops.
pub(crate) fn paint_for(paint: &Paint, area: BoxF) -> Option<tiny_skia::Paint<'static>> {
    match paint {
        Paint::Solid(c) => Some(solid(*c)),
        Paint::Arc { start, .. } => Some(solid(*start)),
        Paint::Linear { angle, stops } => linear(*angle, stops, area).map(with_shader),
    }
}

fn linear(angle: f32, stops: &[(f32, Rgba)], area: BoxF) -> Option<Shader<'static>> {
    let mut sorted: Vec<(f32, Rgba)> = stops
        .iter()
        .filter(|(p, _)| p.is_finite())
        .map(|&(p, c)| (p.clamp(0.0, 1.0), c))
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let last = sorted.last().map(|&(_, c)| Shader::SolidColor(color(c)))?;
    let list: Vec<GradientStop> = sorted
        .iter()
        .map(|&(p, c)| GradientStop::new(p, color(c)))
        .collect();
    let (start, end) = gradient_line(angle, area);
    LinearGradient::new(start, end, list, SpreadMode::Pad, Transform::identity()).or(Some(last))
}

/// Endpoints of a gradient at `angle` degrees (0 = left→right, 90 =
/// top→bottom) through the center of `area`, long enough that the first and
/// last colors reach the corners (like CSS `linear-gradient`).
pub(crate) fn gradient_line(angle: f32, area: BoxF) -> (Point, Point) {
    let radians = if angle.is_finite() {
        angle.to_radians()
    } else {
        0.0
    };
    let (dx, dy) = (radians.cos(), radians.sin());
    let half = (area.width * dx.abs() + area.height * dy.abs()) / 2.0;
    let (cx, cy) = area.center();
    (
        Point::from_xy(cx - dx * half, cy - dy * half),
        Point::from_xy(cx + dx * half, cy + dy * half),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(p: Point, x: f32, y: f32) -> bool {
        (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3
    }

    #[test]
    fn gradient_lines_follow_the_angle() {
        let area = BoxF::new(10.0, 20.0, 100.0, 40.0);
        let (s, e) = gradient_line(0.0, area);
        assert!(near(s, 10.0, 40.0) && near(e, 110.0, 40.0));
        let (s, e) = gradient_line(90.0, area);
        assert!(near(s, 60.0, 20.0) && near(e, 60.0, 60.0));
        let (s, e) = gradient_line(f32::NAN, area);
        assert!(near(s, 10.0, 40.0) && near(e, 110.0, 40.0));
    }

    #[test]
    fn degenerate_gradients_fall_back() {
        let area = BoxF::new(0.0, 0.0, 10.0, 10.0);
        let empty = Paint::Linear {
            angle: 0.0,
            stops: vec![],
        };
        assert!(paint_for(&empty, area).is_none());
        let one = Paint::Linear {
            angle: 0.0,
            stops: vec![(0.5, Rgba::WHITE)],
        };
        let paint = paint_for(&one, area).expect("one stop");
        assert!(paint.is_solid_color());
        let flat = Paint::Linear {
            angle: 0.0,
            stops: vec![
                (1.0, Rgba::WHITE),
                (f32::NAN, Rgba::BLACK),
                (0.0, Rgba::BLACK),
            ],
        };
        let zero = BoxF::new(5.0, 5.0, 0.0, 0.0);
        assert!(paint_for(&flat, zero).expect("degenerate").is_solid_color());
        assert!(!paint_for(&flat, area).expect("gradient").is_solid_color());
    }
}
