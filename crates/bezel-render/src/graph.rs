//! History graphs: a line, a filled area or one bar per sample. The newest
//! sample sits on the right edge; gaps (unavailable readings) break the line.

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::sensor::fraction;
use bezel_core::domain::theme::{BoxF, GraphStyle, Paint};
use tiny_skia::{LineCap, LineJoin, PathBuilder, Stroke};

use crate::layer::Layer;
use crate::paint::{paint_for, solid};
use crate::path::{positive, rect};

/// How a graph looks and scales.
pub(crate) struct GraphSpec<'a> {
    /// Samples across the width.
    pub history: u16,
    /// Drawing style.
    pub style: GraphStyle,
    /// Line and bar color.
    pub color: Rgba,
    /// Area fill (default: the line color, faded).
    pub fill: Option<&'a Paint>,
    /// Line width.
    pub line_width: f32,
    /// Scale to the visible samples.
    pub autoscale: bool,
    /// Binding range.
    pub min: f64,
    /// Binding range.
    pub max: f64,
}

/// Alpha of the default area fill.
const AREA_ALPHA: u8 = 72;

/// The value range the graph maps to its height: the binding range, or the
/// visible samples with a 10% margin when autoscaling.
pub(crate) fn value_range(values: &[Option<f64>], spec: &GraphSpec<'_>) -> Option<(f64, f64)> {
    if !spec.autoscale {
        return (spec.max > spec.min).then_some((spec.min, spec.max));
    }
    let mut seen = values.iter().flatten().copied().filter(|v| v.is_finite());
    let first = seen.next()?;
    let (lo, hi) = seen.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let span = hi - lo;
    let margin = if span > 0.0 {
        span * 0.1
    } else {
        (hi.abs() * 0.1).max(1.0)
    };
    Some((lo - margin, hi + margin))
}

/// Canvas points of each unbroken run of samples.
fn runs(
    area: BoxF,
    values: &[Option<f64>],
    spec: &GraphSpec<'_>,
    range: (f64, f64),
) -> Vec<Vec<(f32, f32)>> {
    let slots = usize::from(spec.history).max(values.len()).max(2);
    let step = area.width / (slots - 1) as f32;
    let inset = (spec.line_width / 2.0).clamp(0.0, area.height / 2.0);
    let right = area.x + area.width;
    let bottom = area.y + area.height - inset;
    let span = area.height - 2.0 * inset;
    let mut out: Vec<Vec<(f32, f32)>> = vec![Vec::new()];
    for (i, v) in values.iter().enumerate() {
        let f = v.and_then(|v| fraction(v, range.0, range.1));
        match (f, out.last_mut()) {
            (Some(f), Some(run)) => {
                let x = right - (values.len() - 1 - i) as f32 * step;
                run.push((x, bottom - f as f32 * span));
            }
            _ => out.push(Vec::new()),
        }
    }
    out.retain(|r| !r.is_empty());
    out
}

/// Draws the graph of `values` (oldest first) in `area`.
pub(crate) fn draw(
    layer: &mut Layer<'_>,
    area: BoxF,
    values: &[Option<f64>],
    spec: &GraphSpec<'_>,
) {
    let Some(range) = value_range(values, spec) else {
        return;
    };
    match spec.style {
        GraphStyle::Bars => draw_bars(layer, area, values, spec, range),
        GraphStyle::Line => draw_lines(layer, &runs(area, values, spec, range), spec),
        GraphStyle::Area => {
            let runs = runs(area, values, spec, range);
            draw_area(layer, area, &runs, spec);
            draw_lines(layer, &runs, spec);
        }
    }
}

fn draw_lines(layer: &mut Layer<'_>, runs: &[Vec<(f32, f32)>], spec: &GraphSpec<'_>) {
    let width = spec.line_width;
    if !positive(width) {
        return;
    }
    let paint = solid(spec.color);
    let mut pb = PathBuilder::new();
    for run in runs {
        match run.as_slice() {
            [(x, y)] => pb.push_circle(*x, *y, width / 2.0),
            [(x, y), rest @ ..] => {
                pb.move_to(*x, *y);
                for (x, y) in rest {
                    pb.line_to(*x, *y);
                }
            }
            [] => {}
        }
    }
    let Some(path) = pb.finish() else {
        return;
    };
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    layer.stroke(&path, &paint, &stroke);
}

fn draw_area(layer: &mut Layer<'_>, area: BoxF, runs: &[Vec<(f32, f32)>], spec: &GraphSpec<'_>) {
    let faded = Paint::Solid(Rgba {
        a: (u16::from(spec.color.a) * u16::from(AREA_ALPHA) / 255) as u8,
        ..spec.color
    });
    let Some(paint) = paint_for(spec.fill.unwrap_or(&faded), area) else {
        return;
    };
    let base = area.y + area.height;
    let mut pb = PathBuilder::new();
    for run in runs.iter().filter(|r| r.len() >= 2) {
        let (Some(first), Some(last)) = (run.first(), run.last()) else {
            continue;
        };
        pb.move_to(first.0, base);
        for (x, y) in run {
            pb.line_to(*x, *y);
        }
        pb.line_to(last.0, base);
        pb.close();
    }
    if let Some(path) = pb.finish() {
        layer.fill(&path, &paint);
    }
}

fn draw_bars(
    layer: &mut Layer<'_>,
    area: BoxF,
    values: &[Option<f64>],
    spec: &GraphSpec<'_>,
    range: (f64, f64),
) {
    let slots = usize::from(spec.history).max(values.len()).max(1);
    let slot = area.width / slots as f32;
    let gap = if slot >= 4.0 { slot * 0.2 } else { 0.0 };
    let right = area.x + area.width;
    let bottom = area.y + area.height;
    let mut pb = PathBuilder::new();
    for (i, v) in values.iter().enumerate() {
        let Some(f) = v.and_then(|v| fraction(v, range.0, range.1)) else {
            continue;
        };
        let height = f as f32 * area.height;
        let x = right - (values.len() - i) as f32 * slot + gap / 2.0;
        if let Some(bar) = rect(x, bottom - height, slot - gap, height) {
            pb.push_path(&bar);
        }
    }
    if let Some(path) = pb.finish() {
        layer.fill(&path, &solid(spec.color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(autoscale: bool) -> GraphSpec<'static> {
        GraphSpec {
            history: 4,
            style: GraphStyle::Line,
            color: Rgba::WHITE,
            fill: None,
            line_width: 2.0,
            autoscale,
            min: 0.0,
            max: 100.0,
        }
    }

    #[test]
    fn ranges_come_from_the_binding_or_the_samples() {
        let values = [Some(10.0), None, Some(30.0)];
        assert_eq!(value_range(&values, &spec(false)), Some((0.0, 100.0)));
        assert_eq!(value_range(&values, &spec(true)), Some((8.0, 32.0)));
        assert_eq!(value_range(&[Some(5.0)], &spec(true)), Some((4.0, 6.0)));
        assert_eq!(value_range(&[None], &spec(true)), None);
        let empty = GraphSpec {
            max: 0.0,
            ..spec(false)
        };
        assert_eq!(value_range(&values, &empty), None);
    }

    #[test]
    fn runs_break_at_gaps_and_end_on_the_right() {
        let area = BoxF::new(0.0, 0.0, 30.0, 12.0);
        let values = [Some(0.0), Some(100.0), None, Some(50.0)];
        let runs = runs(area, &values, &spec(false), (0.0, 100.0));
        assert_eq!(
            runs,
            vec![vec![(0.0, 11.0), (10.0, 1.0)], vec![(30.0, 6.0)]]
        );
    }
}
