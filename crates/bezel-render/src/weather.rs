//! Small vector condition icons, independent of installed fonts or assets.
use crate::{layer::Layer, paint::paint_for};
use bezel_core::domain::theme::{BoxF, Paint};
use bezel_core::domain::weather::{IconStyle, family};
use tiny_skia::{PathBuilder, Stroke};

pub(crate) fn draw(layer: &mut Layer<'_>, area: BoxF, code: u16, color: &Paint, style: IconStyle) {
    if matches!(style, IconStyle::Colored | IconStyle::Dimensional) {
        draw_colored(layer, area, code, style == IconStyle::Dimensional);
        return;
    }
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

fn draw_colored(layer: &mut Layer<'_>, area: BoxF, code: u16, depth: bool) {
    use bezel_core::domain::frame::Rgba;
    let color = |r, g, b| Rgba { r, g, b, a: 255 };
    let f = family(code);
    let x = area.x;
    let y = area.y;
    let s = area.width / 24.;
    let component =
        |layer: &mut Layer<'_>, path: tiny_skia::Path, light: Rgba, dark: Rgba, fill: bool| {
            if depth && fill {
                let shadow = crate::paint::solid(Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 65,
                });
                if let Some(p) = path
                    .clone()
                    .transform(tiny_skia::Transform::from_translate(s * 0.5, s * 0.9))
                {
                    layer.fill(&p, &shadow);
                }
            }
            let palette = if depth {
                Paint::Linear {
                    angle: 65.,
                    stops: vec![(0., light), (1., dark)],
                }
            } else {
                Paint::solid(light)
            };
            if let Some(paint) = paint_for(&palette, area) {
                if fill {
                    layer.fill(&path, &paint);
                }
                layer.stroke(
                    &path,
                    &paint,
                    &Stroke {
                        width: (s * 1.5).max(1.),
                        line_cap: tiny_skia::LineCap::Round,
                        ..Stroke::default()
                    },
                );
            }
        };
    if f == 0 || f == 1 {
        let mut p = PathBuilder::new();
        p.push_circle(x + 12. * s, y + 9. * s, 4. * s);
        if let Some(p) = p.finish() {
            component(layer, p, color(255, 213, 79), color(239, 126, 22), true);
        }
        let mut p = PathBuilder::new();
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            p.move_to(x + (12. + a.cos() * 6.) * s, y + (9. + a.sin() * 6.) * s);
            p.line_to(x + (12. + a.cos() * 8.) * s, y + (9. + a.sin() * 8.) * s);
        }
        if let Some(p) = p.finish() {
            component(layer, p, color(255, 193, 7), color(255, 148, 20), false);
        }
    }
    if (1..=6).contains(&f) {
        let mut p = PathBuilder::new();
        p.move_to(x + 5. * s, y + 16. * s);
        p.cubic_to(
            x - s,
            y + 16. * s,
            x + s,
            y + 8. * s,
            x + 7. * s,
            y + 10. * s,
        );
        p.cubic_to(
            x + 8. * s,
            y + 3. * s,
            x + 19. * s,
            y + 5. * s,
            x + 18. * s,
            y + 11. * s,
        );
        p.cubic_to(
            x + 24. * s,
            y + 10. * s,
            x + 24. * s,
            y + 16. * s,
            x + 19. * s,
            y + 16. * s,
        );
        p.close();
        if let Some(p) = p.finish() {
            component(layer, p, color(224, 241, 255), color(103, 146, 188), true);
        }
        let mut p = PathBuilder::new();
        match f {
            3 => {
                for i in 0..2 {
                    let yy = y + (19. + i as f32 * 3.) * s;
                    p.move_to(x + 4. * s, yy);
                    p.line_to(x + 20. * s, yy);
                }
            }
            4 | 5 => {
                for i in 0..3 {
                    let xx = x + (6. + i as f32 * 6.) * s;
                    p.move_to(xx, y + 19. * s);
                    p.line_to(xx - s, y + 22. * s);
                    if f == 5 {
                        p.move_to(xx - 1.5 * s, y + 20. * s);
                        p.line_to(xx + 1.5 * s, y + 20. * s);
                    }
                }
            }
            6 => {
                p.move_to(x + 13. * s, y + 17. * s);
                p.line_to(x + 10. * s, y + 21. * s);
                p.line_to(x + 14. * s, y + 21. * s);
                p.line_to(x + 11. * s, y + 24. * s);
            }
            _ => {}
        }
        let (a, b) = match f {
            6 => (color(255, 224, 66), color(255, 139, 30)),
            5 => (color(215, 244, 255), color(120, 204, 240)),
            _ => (color(55, 192, 245), color(29, 112, 214)),
        };
        if let Some(p) = p.finish() {
            component(layer, p, a, b, false);
        }
    }
    if f == 7 {
        let mut p = PathBuilder::new();
        p.move_to(x + 5. * s, y + 5. * s);
        p.line_to(x + 19. * s, y + 19. * s);
        p.move_to(x + 19. * s, y + 5. * s);
        p.line_to(x + 5. * s, y + 19. * s);
        if let Some(p) = p.finish() {
            component(layer, p, color(160, 174, 192), color(80, 98, 120), false);
        }
    }
}
