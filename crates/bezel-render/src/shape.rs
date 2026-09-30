//! Rectangles (optionally rounded) and ellipses with a fill and an outline.
//! The outline is drawn inside the box so a shape never grows past it.

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::theme::{BoxF, Paint, ShapeKind};
use tiny_skia::{Path, PathBuilder, Rect, Stroke};

use crate::layer::Layer;
use crate::paint::{paint_for, solid};
use crate::path::rounded_rect;

/// Draws a shape filling `area`.
pub(crate) fn draw(
    layer: &mut Layer<'_>,
    area: BoxF,
    shape: ShapeKind,
    fill: Option<&Paint>,
    outline: Option<(Rgba, f32)>,
) {
    let width = outline
        .map(|(_, w)| w)
        .filter(|w| w.is_finite() && *w > 0.0)
        .unwrap_or(0.0)
        .min(area.width.min(area.height) / 2.0);
    let Some(path) = outline_path(area, shape, width / 2.0) else {
        return;
    };
    if let Some(paint) = fill.and_then(|p| paint_for(p, area)) {
        layer.fill(&path, &paint);
    }
    if let Some((color, _)) = outline.filter(|_| width > 0.0) {
        let stroke = Stroke {
            width,
            ..Stroke::default()
        };
        layer.stroke(&path, &solid(color), &stroke);
    }
}

/// The shape inset by `inset` on every side.
fn outline_path(area: BoxF, shape: ShapeKind, inset: f32) -> Option<Path> {
    let (x, y) = (area.x + inset, area.y + inset);
    let (w, h) = (area.width - 2.0 * inset, area.height - 2.0 * inset);
    match shape {
        ShapeKind::Rect { radius } => rounded_rect(x, y, w, h, radius - inset),
        ShapeKind::Ellipse => Rect::from_xywh(x, y, w, h).and_then(PathBuilder::from_oval),
    }
}
