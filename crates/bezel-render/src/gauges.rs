//! Value gauges: bars, rings and needles. Each takes the value as a fraction
//! (`None` when the sensor is unavailable, drawn as empty / at rest).

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::gradient::arc_color;
use bezel_core::domain::theme::{BoxF, Cap, Direction, Paint, Segments};
use tiny_skia::{LineCap, Path, PathBuilder, Pixmap, Rect, Stroke};

use crate::layer::Layer;
use crate::paint::{paint_for, solid};
use crate::path::{blocks, polar, positive, push_arc, rounded_rect};

/// How a bar looks.
pub(crate) struct BarStyle<'a> {
    /// Fill direction.
    pub direction: Direction,
    /// Filled part.
    pub fill: &'a Paint,
    /// Empty track.
    pub track: Option<&'a Paint>,
    /// Corner radius of the bar or of each block.
    pub radius: f32,
    /// Optional blocks.
    pub segments: Option<Segments>,
}

/// The rectangle of the span `[a, b]` along the bar, measured from where it
/// starts filling.
fn span(area: BoxF, direction: Direction, a: f32, b: f32) -> Option<Rect> {
    let len = b - a;
    match direction {
        Direction::LeftToRight => Rect::from_xywh(area.x + a, area.y, len, area.height),
        Direction::RightToLeft => {
            Rect::from_xywh(area.x + area.width - b, area.y, len, area.height)
        }
        Direction::TopToBottom => Rect::from_xywh(area.x, area.y + a, area.width, len),
        Direction::BottomToTop => {
            Rect::from_xywh(area.x, area.y + area.height - b, area.width, len)
        }
    }
}

/// Draws a bar: the blocks in the track paint, then the filled length in the
/// fill paint clipped to the blocks (a partly filled block is cut).
pub(crate) fn draw_bar(
    layer: &mut Layer<'_>,
    area: BoxF,
    fraction: Option<f64>,
    style: &BarStyle<'_>,
) {
    let length = match style.direction {
        Direction::LeftToRight | Direction::RightToLeft => area.width,
        Direction::TopToBottom | Direction::BottomToTop => area.height,
    };
    let mut pb = PathBuilder::new();
    for (a, b) in blocks(length, style.segments, false) {
        let block = span(area, style.direction, a, b)
            .and_then(|r| rounded_rect(r.x(), r.y(), r.width(), r.height(), style.radius));
        if let Some(block) = block {
            pb.push_path(&block);
        }
    }
    let Some(shape) = pb.finish() else {
        return;
    };
    if let Some(track) = style.track.and_then(|p| paint_for(p, area)) {
        layer.fill(&shape, &track);
    }
    let filled = fraction.unwrap_or(0.0) as f32 * length;
    let (Some(rect), Some(paint), Some(mask)) = (
        span(area, style.direction, 0.0, filled).filter(|_| filled > 0.0),
        paint_for(style.fill, area),
        layer.mask_of(&shape),
    ) else {
        return;
    };
    layer.fill_rect_masked(rect, &paint, &mask);
}

/// How a ring looks.
pub(crate) struct RingStyle<'a> {
    /// Start angle, 0 = 12 o'clock, clockwise positive.
    pub start_angle: f32,
    /// Total sweep in degrees.
    pub sweep: f32,
    /// Ring thickness in pixels.
    pub thickness: f32,
    /// Fill (and track) run clockwise from the start.
    pub clockwise: bool,
    /// Filled arc.
    pub fill: &'a Paint,
    /// Track under it.
    pub track: Option<&'a Paint>,
    /// Line ends.
    pub cap: Cap,
    /// Optional blocks (gap in degrees).
    pub segments: Option<Segments>,
}

/// Draws a ring inscribed in `area`: the track over the whole sweep, then the
/// arc of the value.
pub(crate) fn draw_ring(
    layer: &mut Layer<'_>,
    area: BoxF,
    fraction: Option<f64>,
    style: &RingStyle<'_>,
) {
    let (cx, cy) = area.center();
    let outer = area.width.min(area.height) / 2.0;
    let sweep = style.sweep.clamp(0.0, 360.0);
    if !(outer > 0.0 && sweep > 0.0 && style.thickness > 0.0) {
        return;
    }
    let thickness = style.thickness.min(outer);
    let circle = (cx, cy, outer - thickness / 2.0);
    let dir = if style.clockwise { 1.0 } else { -1.0 };
    let pieces = blocks(sweep, style.segments, sweep >= 360.0);
    let stroke = Stroke {
        width: thickness,
        line_cap: match style.cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
        },
        ..Stroke::default()
    };
    let arcs = |limit: f32| -> Option<Path> {
        let mut pb = PathBuilder::new();
        for &(a, b) in pieces.iter().take_while(|(a, _)| *a < limit) {
            push_arc(
                &mut pb,
                circle,
                style.start_angle + dir * a,
                dir * (b.min(limit) - a),
            );
        }
        pb.finish()
    };
    if let (Some(track), Some(path)) = (style.track.and_then(|p| paint_for(p, area)), arcs(sweep)) {
        layer.stroke(&path, &track, &stroke);
    }
    let filled = fraction.unwrap_or(0.0) as f32 * sweep;
    let Some(path) = arcs(filled) else {
        return;
    };
    if let Paint::Arc {
        start,
        end,
        transition,
    } = style.fill
    {
        // Stroke once to obtain coverage: no seams between adjacent colors,
        // including translucent rings, round caps and separated blocks.
        let Some(outline) = path.stroke(&stroke, 1.0) else {
            return;
        };
        let Some(mask) = layer.mask_of(&outline) else {
            return;
        };
        let width = layer.pixmap.width();
        let Some(mut image) = Pixmap::new(width, layer.pixmap.height()) else {
            return;
        };
        for (index, (pixel, coverage)) in image
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(mask.data())
            .enumerate()
        {
            if *coverage == 0 {
                continue;
            }
            let x = (index % width as usize) as f32 + layer.x as f32 + 0.5 - cx;
            let y = (index / width as usize) as f32 + layer.y as f32 + 0.5 - cy;
            let mut angle =
                ((y.atan2(x).to_degrees() + 90.0 - style.start_angle) * dir).rem_euclid(360.0);
            // Round caps extend past the angular span; retain the endpoint color.
            if angle > filled {
                angle = if 360.0 - angle < angle - filled {
                    0.0
                } else {
                    filled
                };
            }
            let c = arc_color(*start, *end, angle / sweep, *transition);
            pixel.copy_from_slice(&[
                c.r,
                c.g,
                c.b,
                ((u16::from(c.a) * u16::from(*coverage) + 127) / 255) as u8,
            ]);
        }
        crate::composite::premultiply(image.data_mut());
        layer.blit(image.as_ref(), layer.x, layer.y);
    } else if let Some(fill) = paint_for(style.fill, area) {
        layer.stroke(&path, &fill, &stroke);
    }
}

/// Where a needle turns and how far.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NeedlePose {
    /// Pivot in canvas coordinates.
    pub pivot: (f32, f32),
    /// Rotation from rest (12 o'clock), degrees clockwise.
    pub angle: f32,
}

impl NeedlePose {
    /// The pose for `fraction` of the sweep (at rest when unavailable).
    pub fn new(
        area: BoxF,
        pivot: (f32, f32),
        start: f32,
        sweep: f32,
        fraction: Option<f64>,
    ) -> Self {
        let angle = start + fraction.unwrap_or(0.0) as f32 * sweep;
        Self {
            pivot: (
                area.x + pivot.0 * area.width,
                area.y + pivot.1 * area.height,
            ),
            angle: if angle.is_finite() { angle } else { 0.0 },
        }
    }

    /// Canvas bounds of `area` turned about the pivot, grown by `margin`.
    pub fn bounds(&self, area: BoxF, margin: f32) -> Option<Rect> {
        let corners = [
            (area.x, area.y),
            (area.x + area.width, area.y),
            (area.x, area.y + area.height),
            (area.x + area.width, area.y + area.height),
        ];
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let (px, py) = self.pivot;
        let turned = corners.map(|(x, y)| {
            let (dx, dy) = (x - px, y - py);
            (px + dx * cos - dy * sin, py + dx * sin + dy * cos)
        });
        let fold = |f: fn(f32, f32) -> f32, pick: fn(&(f32, f32)) -> f32| {
            turned.iter().map(pick).fold(pick(&turned[0]), f)
        };
        Rect::from_ltrb(
            fold(f32::min, |p| p.0) - margin,
            fold(f32::min, |p| p.1) - margin,
            fold(f32::max, |p| p.0) + margin,
            fold(f32::max, |p| p.1) + margin,
        )
    }
}

/// Draws a needle as a line from the pivot to the box edge it points at
/// when at rest (the top), with a round hub.
pub(crate) fn draw_line_needle(
    layer: &mut Layer<'_>,
    area: BoxF,
    pose: NeedlePose,
    color: Rgba,
    width: f32,
) {
    if !positive(width) {
        return;
    }
    let (px, py) = pose.pivot;
    let reach = py - area.y;
    let length = if reach >= 1.0 {
        reach
    } else {
        area.width.min(area.height) / 2.0
    };
    let (tx, ty) = polar(px, py, length, pose.angle);
    let mut pb = PathBuilder::new();
    pb.move_to(px, py);
    pb.line_to(tx, ty);
    let paint = solid(color);
    if let Some(line) = pb.finish() {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        layer.stroke(&line, &paint, &stroke);
    }
    if let Some(hub) = PathBuilder::from_circle(px, py, width * 1.25) {
        layer.fill(&hub, &paint);
    }
}
