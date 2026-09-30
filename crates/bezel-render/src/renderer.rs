//! [`SkiaRenderer`]: the [`FrameRenderer`] adapter.

use std::collections::{BTreeMap, HashMap, HashSet};

use bezel_core::domain::clock::format_clock;
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::sensor::{format_reading, fraction};
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Element, ElementId, ElementKind, Fit, TextContent, Theme,
};
use bezel_core::ports::{FrameRenderer, RenderContext};
use bezel_core::{BezelError, Result};
use tiny_skia::{Color, IntRect, Pixmap, PixmapMut, Rect, Transform};

use crate::composite::{self, Sprite, Target};
use crate::diagnostics::Diagnostics;
use crate::gauges::{self, BarStyle, NeedlePose, RingStyle};
use crate::graph::{self, GraphSpec};
use crate::images::ImageCache;
use crate::layer::Layer;
use crate::paint;
use crate::quantity::quantity_of;
use crate::shape;
use crate::text::{SystemFonts, TextEngine, TextJob};

/// Largest canvas side rendered, pixels.
const MAX_CANVAS_SIDE: u32 = 8192;
/// Shown under a video background that has no poster.
const VIDEO_PLACEHOLDER: Rgba = Rgba::opaque(16, 17, 22);

/// Draws themes with tiny-skia (paths, gradients, compositing), cosmic-text
/// (shaping) and image (PNG, JPEG, GIF). Build it once and reuse it: fonts,
/// glyphs and decoded images stay cached between frames.
pub struct SkiaRenderer {
    text: TextEngine,
    images: ImageCache,
    diagnostics: Diagnostics,
    /// Layer pixels of the element being drawn (reused allocation).
    scratch: Vec<u8>,
    /// The previous canvas, reused when the size matches.
    canvas: Option<Pixmap>,
    /// Drawn layers of elements that look the same on every frame, by
    /// z-index and id (ids may repeat in imported themes).
    layers: HashMap<LayerKey, CachedLayer>,
    used_layers: HashSet<LayerKey>,
}

/// Where an element sits in its theme: z-index and id.
type LayerKey = (usize, ElementId);

/// The drawn layer of a static element.
struct CachedLayer {
    element: Element,
    bounds: IntRect,
    pixels: Vec<u8>,
    opaque: bool,
}

/// Elements whose pixels depend on nothing but themselves (no sensor, no
/// clock, no asset), so their layer can be kept between frames.
fn is_static(element: &Element) -> bool {
    match &element.kind {
        ElementKind::Shape { .. } => true,
        ElementKind::Text {
            content: TextContent::Static(_),
            style,
        } => style.font.asset.is_none(),
        _ => false,
    }
}

impl std::fmt::Debug for SkiaRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkiaRenderer").finish_non_exhaustive()
    }
}

impl Default for SkiaRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl SkiaRenderer {
    /// A renderer that also knows the installed fonts. Scanning them takes a
    /// moment, so build one renderer and keep it.
    pub fn new() -> Self {
        Self::with_fonts(Vec::new(), SystemFonts::Load)
    }

    /// A renderer with `fonts` (TTF/OTF/TTC bytes) and, when `system` says
    /// so, the installed fonts. The first font found among the usual sans
    /// families (or else the first font) is the default.
    pub fn with_fonts(fonts: Vec<Vec<u8>>, system: SystemFonts) -> Self {
        Self {
            text: TextEngine::new(fonts, system),
            images: ImageCache::default(),
            diagnostics: Diagnostics::default(),
            scratch: Vec::new(),
            canvas: None,
            layers: HashMap::new(),
            used_layers: HashSet::new(),
        }
    }
}

impl FrameRenderer for SkiaRenderer {
    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame> {
        let size = theme.canvas;
        if size.area() == 0 {
            return Ok(Frame::filled(size, Rgba::default()));
        }
        let mut canvas = match self.canvas.take() {
            Some(c) if (c.width(), c.height()) == (size.width, size.height) => c,
            _ => new_canvas(size)?,
        };
        self.draw_background(&mut canvas, &theme.background, assets, &context);
        for (index, element) in theme.elements.iter().enumerate() {
            if element.visible {
                let key = (index, element.id);
                self.draw_element(&mut canvas, key, element, assets, &context);
            }
        }
        self.images.sweep();
        let used = std::mem::take(&mut self.used_layers);
        self.layers.retain(|key, _| used.contains(key));
        let frame = composite::to_frame(&canvas, size);
        self.canvas = Some(canvas);
        Ok(frame)
    }
}

fn new_canvas(size: Size) -> Result<Pixmap> {
    let too_large = || {
        BezelError::Transport(format!(
            "cannot render a {}x{} canvas (the limit is {MAX_CANVAS_SIDE} per side)",
            size.width, size.height
        ))
    };
    if size.width > MAX_CANVAS_SIDE || size.height > MAX_CANVAS_SIDE {
        return Err(too_large());
    }
    Pixmap::new(size.width, size.height).ok_or_else(too_large)
}

/// The fraction of a binding's range the current reading represents.
fn value_fraction(binding: &Binding, context: &RenderContext<'_>) -> Option<f64> {
    let value = context.snapshot.get(&binding.key).value()?;
    fraction(value, binding.min, binding.max)
}

/// What a text element prints now.
fn text_of(content: &TextContent, context: &RenderContext<'_>) -> String {
    match content {
        TextContent::Static(text) => text.clone(),
        TextContent::Sensor {
            key,
            format,
            prefix,
            suffix,
        } => {
            let reading = context.snapshot.get(key);
            let value = format_reading(&reading, quantity_of(key), *format);
            format!("{prefix}{value}{suffix}")
        }
        TextContent::Clock { pattern } => format_clock(pattern, &context.time, context.language),
    }
}

/// Element opacity as 0..=255 (non-finite reads as opaque).
fn opacity_of(opacity: f32) -> u8 {
    if opacity.is_finite() {
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8
    } else {
        255
    }
}

/// Composites a layer covering `bounds` onto the canvas.
fn composite_layer(canvas: &mut Pixmap, pixels: &[u8], bounds: IntRect, opacity: u8, opaque: bool) {
    let (width, height) = (canvas.width(), canvas.height());
    let mut target = Target {
        data: canvas.data_mut(),
        width,
        height,
    };
    let sprite = Sprite {
        data: pixels,
        width: bounds.width(),
        height: bounds.height(),
        x: bounds.x(),
        y: bounds.y(),
        opaque,
    };
    composite::blend(&mut target, &sprite, opacity);
}

/// The canvas pixels an element may touch.
fn element_bounds(
    element: &Element,
    context: &RenderContext<'_>,
    canvas: IntRect,
) -> Option<IntRect> {
    let area = element.frame;
    let rect = match &element.kind {
        ElementKind::Needle {
            binding,
            pivot,
            start_angle,
            sweep,
            width,
            ..
        } => {
            let pose = NeedlePose::new(
                area,
                *pivot,
                *start_angle,
                *sweep,
                value_fraction(binding, context),
            );
            pose.bounds(area, width.max(1.0) * 1.5)?
        }
        _ => Rect::from_xywh(area.x, area.y, area.width, area.height)?,
    };
    rect.round_out()?.intersect(&canvas)
}

impl SkiaRenderer {
    fn draw_background(
        &mut self,
        canvas: &mut Pixmap,
        background: &Background,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) {
        let (asset, fit) = match background {
            Background::Color(color) => {
                canvas.fill(paint::color(*color));
                return;
            }
            Background::Image { asset, fit } => (asset, *fit),
            Background::Video {
                poster: Some(poster),
                ..
            } => (poster, Fit::Cover),
            Background::Video { poster: None, .. } => {
                canvas.fill(paint::color(VIDEO_PLACEHOLDER));
                return;
            }
        };
        canvas.fill(Color::TRANSPARENT);
        let area = BoxF::new(0.0, 0.0, canvas.width() as f32, canvas.height() as f32);
        let Some((image, plan)) =
            self.images
                .placed(asset, fit, area, assets, &mut self.diagnostics)
        else {
            return;
        };
        let Some(frame) = image.frame_at(&context.time) else {
            return;
        };
        let (width, height) = (canvas.width(), canvas.height());
        let mut target = Target {
            data: canvas.data_mut(),
            width,
            height,
        };
        let sprite = Sprite {
            data: frame.data(),
            width: frame.width(),
            height: frame.height(),
            x: plan.at.0.round() as i32,
            y: plan.at.1.round() as i32,
            opaque: false,
        };
        composite::blend(&mut target, &sprite, 255);
    }

    /// Draws one element into a scratch layer the size of its bounds (or
    /// reuses the layer kept from an earlier frame), then composites the
    /// layer with the element's opacity.
    fn draw_element(
        &mut self,
        canvas: &mut Pixmap,
        key: LayerKey,
        element: &Element,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) {
        let opacity = opacity_of(element.opacity);
        let Some(canvas_rect) = IntRect::from_xywh(0, 0, canvas.width(), canvas.height()) else {
            return;
        };
        let Some(bounds) = element_bounds(element, context, canvas_rect).filter(|_| opacity > 0)
        else {
            return;
        };
        let cacheable = is_static(element);
        if cacheable {
            self.used_layers.insert(key);
            let kept = self.layers.get(&key);
            if let Some(kept) = kept.filter(|k| k.bounds == bounds && k.element == *element) {
                composite_layer(canvas, &kept.pixels, bounds, opacity, kept.opaque);
                return;
            }
        }
        let (w, h) = (bounds.width(), bounds.height());
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        scratch.resize(w as usize * h as usize * 4, 0);
        if let Some(pixmap) = PixmapMut::from_bytes(&mut scratch, w, h) {
            let mut layer = Layer {
                pixmap,
                x: bounds.x(),
                y: bounds.y(),
            };
            self.draw_kind(&mut layer, element, assets, context);
        }
        let opaque = cacheable && composite::is_opaque(&scratch);
        composite_layer(canvas, &scratch, bounds, opacity, opaque);
        if cacheable {
            let kept = CachedLayer {
                element: element.clone(),
                bounds,
                pixels: scratch.clone(),
                opaque,
            };
            self.layers.insert(key, kept);
        }
        self.scratch = scratch;
    }

    fn draw_kind(
        &mut self,
        layer: &mut Layer<'_>,
        element: &Element,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) {
        let area = element.frame;
        match &element.kind {
            ElementKind::Text { content, style } => {
                let text = text_of(content, context);
                let job = TextJob {
                    text: &text,
                    style,
                    area,
                };
                self.text.draw(layer, &job, assets, &mut self.diagnostics);
            }
            ElementKind::Image { asset, fit } => {
                self.draw_image(layer, asset, *fit, area, assets, context);
            }
            ElementKind::Shape {
                shape,
                fill,
                stroke,
            } => shape::draw(layer, area, *shape, fill.as_ref(), *stroke),
            ElementKind::Bar {
                binding,
                direction,
                fill,
                track,
                radius,
                segments,
            } => {
                let style = BarStyle {
                    direction: *direction,
                    fill,
                    track: track.as_ref(),
                    radius: *radius,
                    segments: *segments,
                };
                gauges::draw_bar(layer, area, value_fraction(binding, context), &style);
            }
            ElementKind::Ring { .. } => draw_ring(layer, element, context),
            ElementKind::Needle { .. } => self.draw_needle(layer, element, assets, context),
            ElementKind::Graph { .. } => draw_graph(layer, element, context),
        }
    }

    fn draw_image(
        &mut self,
        layer: &mut Layer<'_>,
        asset: &AssetRef,
        fit: Fit,
        area: BoxF,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) {
        let Some((image, plan)) =
            self.images
                .placed(asset, fit, area, assets, &mut self.diagnostics)
        else {
            return;
        };
        if let Some(frame) = image.frame_at(&context.time) {
            layer.blit(
                frame.as_ref(),
                plan.at.0.round() as i32,
                plan.at.1.round() as i32,
            );
        }
    }

    fn draw_needle(
        &mut self,
        layer: &mut Layer<'_>,
        element: &Element,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) {
        let ElementKind::Needle {
            binding,
            asset,
            pivot,
            start_angle,
            sweep,
            color,
            width,
        } = &element.kind
        else {
            return;
        };
        let area = element.frame;
        let pose = NeedlePose::new(
            area,
            *pivot,
            *start_angle,
            *sweep,
            value_fraction(binding, context),
        );
        let image = asset.as_ref().and_then(|asset| {
            self.images
                .placed(asset, Fit::Fill, area, assets, &mut self.diagnostics)
        });
        let Some((image, plan)) = image else {
            gauges::draw_line_needle(layer, area, pose, *color, *width);
            return;
        };
        if let Some(frame) = image.frame_at(&context.time) {
            let (px, py) = pose.pivot;
            let placement =
                Transform::from_translate(plan.at.0, plan.at.1).post_rotate_at(pose.angle, px, py);
            layer.draw_transformed(frame.as_ref(), placement);
        }
    }
}

fn draw_ring(layer: &mut Layer<'_>, element: &Element, context: &RenderContext<'_>) {
    let ElementKind::Ring {
        binding,
        start_angle,
        sweep,
        thickness,
        clockwise,
        fill,
        track,
        cap,
        segments,
    } = &element.kind
    else {
        return;
    };
    let style = RingStyle {
        start_angle: *start_angle,
        sweep: *sweep,
        thickness: *thickness,
        clockwise: *clockwise,
        fill,
        track: track.as_ref(),
        cap: *cap,
        segments: *segments,
    };
    gauges::draw_ring(
        layer,
        element.frame,
        value_fraction(binding, context),
        &style,
    );
}

fn draw_graph(layer: &mut Layer<'_>, element: &Element, context: &RenderContext<'_>) {
    let ElementKind::Graph {
        binding,
        history,
        style,
        color,
        fill,
        line_width,
        autoscale,
    } = &element.kind
    else {
        return;
    };
    let spec = GraphSpec {
        history: *history,
        style: *style,
        color: *color,
        fill: fill.as_ref(),
        line_width: *line_width,
        autoscale: *autoscale,
        min: binding.min,
        max: binding.max,
    };
    let values = context.histories.get(&binding.key);
    graph::draw(layer, element.frame, &values, &spec);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_maps_to_bytes() {
        assert_eq!(opacity_of(1.0), 255);
        assert_eq!(opacity_of(0.5), 128);
        assert_eq!(opacity_of(-3.0), 0);
        assert_eq!(opacity_of(f32::NAN), 255);
    }

    #[test]
    fn oversized_canvases_are_refused() {
        assert!(new_canvas(Size::new(MAX_CANVAS_SIDE + 1, 1)).is_err());
        assert!(new_canvas(Size::new(4, 4)).is_ok());
    }
}
