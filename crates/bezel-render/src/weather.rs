//! Small vector condition icons, independent of installed fonts or assets.
use crate::{layer::Layer, paint::paint_for};
use bezel_core::domain::theme::{BoxF, Paint};
use bezel_core::domain::weather::{IconStyle, family};
use tiny_skia::{PathBuilder, Stroke};

pub(crate) fn draw(layer: &mut Layer<'_>, area: BoxF, code: u16, color: &Paint, style: IconStyle) {
    let Some(paint) = paint_for(color, area) else {
        return;
    };
    let x = area.x;
    let y = area.y;
    let s = area.width / 24.0;
    let mut path = PathBuilder::new();
    let f = family(code);
    if f == 0 || f == 1 {
        path.push_circle(x + 12.0 * s, y + 10.0 * s, 4.0 * s);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            path.move_to(
                x + (12.0 + a.cos() * 6.0) * s,
                y + (10.0 + a.sin() * 6.0) * s,
            );
            path.line_to(
                x + (12.0 + a.cos() * 8.0) * s,
                y + (10.0 + a.sin() * 8.0) * s,
            );
        }
    }
    if (1..=6).contains(&f) {
        path.move_to(x + 5.0 * s, y + 16.0 * s);
        path.cubic_to(
            x - 1.0 * s,
            y + 16.0 * s,
            x + 1.0 * s,
            y + 8.0 * s,
            x + 7.0 * s,
            y + 10.0 * s,
        );
        path.cubic_to(
            x + 8.0 * s,
            y + 3.0 * s,
            x + 19.0 * s,
            y + 5.0 * s,
            x + 18.0 * s,
            y + 11.0 * s,
        );
        path.cubic_to(
            x + 24.0 * s,
            y + 10.0 * s,
            x + 24.0 * s,
            y + 16.0 * s,
            x + 19.0 * s,
            y + 16.0 * s,
        );
        path.line_to(x + 5.0 * s, y + 16.0 * s);
        if f == 3 {
            for i in 0..2 {
                let py = y + (19.0 + i as f32 * 3.0) * s;
                path.move_to(x + 4.0 * s, py);
                path.line_to(x + 20.0 * s, py);
            }
        }
        if f == 4 || f == 5 {
            for i in 0..3 {
                let px = x + (6.0 + i as f32 * 6.0) * s;
                path.move_to(px, y + 19.0 * s);
                path.line_to(px - 1.0 * s, y + 22.0 * s);
                if f == 5 {
                    path.move_to(px - 1.5 * s, y + 20.0 * s);
                    path.line_to(px + 1.5 * s, y + 20.0 * s);
                }
            }
        }
        if f == 6 {
            path.move_to(x + 13.0 * s, y + 17.0 * s);
            path.line_to(x + 10.0 * s, y + 21.0 * s);
            path.line_to(x + 14.0 * s, y + 21.0 * s);
            path.line_to(x + 11.0 * s, y + 24.0 * s);
        }
    }
    if f == 7 {
        path.move_to(x + 5.0 * s, y + 5.0 * s);
        path.line_to(x + 19.0 * s, y + 19.0 * s);
        path.move_to(x + 19.0 * s, y + 5.0 * s);
        path.line_to(x + 5.0 * s, y + 19.0 * s);
    }
    if let Some(path) = path.finish() {
        if style == IconStyle::Filled {
            layer.fill(&path, &paint);
        }
        layer.stroke(
            &path,
            &paint,
            &Stroke {
                width: (s * 1.6).max(1.0),
                ..Stroke::default()
            },
        );
    }
}
