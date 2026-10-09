//! [`SkiaRenderer`]: the [`FrameRenderer`] adapter.

use std::collections::{BTreeMap, HashMap, HashSet};

use bezel_core::domain::animation::Timeline;
use bezel_core::domain::clock::{clock_case, format_clock};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::sensor::{format_reading, fraction};
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Element, ElementId, ElementKind, Fit, TextContent, Theme,
};
use bezel_core::ports::{Backdrop, FrameRenderer, RenderContext};
use bezel_core::{BezelError, Result};
use image::RgbaImage;
use image::imageops::{self, FilterType};
use tiny_skia::{Color, IntRect, Pixmap, PixmapMut, Rect, Transform};

use crate::composite::{self, Sprite, Target};
use crate::diagnostics::Diagnostics;
use crate::gauges::{self, BarStyle, NeedlePose, RingStyle};
use crate::graph::{self, GraphSpec};
use crate::images::{self, ImageCache};
use crate::layer::Layer;
use crate::paint;
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
    cards: crate::cards::Cards,
    card_buffers: Vec<Pixmap>,
    face_cache: HashMap<FaceKey, CachedFace>,
    content_revision: u64,
    face_cache_enabled: bool,
    render_stats: bezel_core::ports::RenderStats,
    card_scene: u64,
    card_scenes: HashMap<u64, crate::cards::Cards>,
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

type FaceKey = (u64, ElementId, usize, bool, bool);

/// Face surfaces never contain the background; video cutouts have their own mask.
struct CachedFace {
    revision: u64,
    elements: Vec<(usize, Element)>,
    time: bezel_core::domain::clock::LocalTime,
    language: bezel_core::domain::clock::Language,
    snapshot: bezel_core::domain::sensor::Snapshot,
    histories: bezel_core::domain::history::Histories,
    quantities: bezel_core::domain::sensor::Quantities,
    pixels: Pixmap,
    window: Option<Pixmap>,
}
const FACE_CACHE_BYTES: usize = 32 * 1024 * 1024;

/// Elements whose pixels depend on nothing but themselves (no sensor, no
/// clock, no asset), so their layer can be kept between frames.
fn is_static(element: &Element) -> bool {
    match &element.kind {
        ElementKind::Shape { video_window, .. } => !video_window,
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
            cards: crate::cards::Cards::new(),
            card_buffers: Vec::new(),
            face_cache: HashMap::new(),
            content_revision: 0,
            face_cache_enabled: std::env::var_os("BEZEL_CARD_FACE_CACHE").is_some_and(|v| v == "1"),
            render_stats: bezel_core::ports::RenderStats::default(),
            card_scene: 0,
            card_scenes: HashMap::new(),
        }
    }
}

impl SkiaRenderer {
    /// Family names of the fonts a theme can use (bundled and installed).
    pub fn font_families(&self) -> Vec<String> {
        self.text.families()
    }

    /// The distinct problems met while drawing so far (a missing asset, an
    /// unknown font family, an unreadable image), sorted. Each is also logged
    /// once with `tracing`.
    pub fn problems(&self) -> Vec<String> {
        self.diagnostics.messages()
    }
}

impl FrameRenderer for SkiaRenderer {
    /// The frame times of a GIF, from its decoded frames (cached for
    /// drawing); delays of 10 ms or less play at 100 ms, as browsers do.
    fn animation(
        &mut self,
        asset: &AssetRef,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
    ) -> Option<Timeline> {
        self.images.timeline(asset, assets, &mut self.diagnostics)
    }

    fn set_scene(&mut self, scene: u64) {
        if scene != self.card_scene {
            let next = match self.card_scenes.remove(&scene) {
                Some(cards) => cards,
                None => crate::cards::Cards::new(), // New scenes enable motion by default.
            };
            let previous = std::mem::replace(&mut self.cards, next);
            self.card_scenes.insert(self.card_scene, previous);
            self.card_scene = scene;
        }
    }
    fn set_content_revision(&mut self, revision: u64) {
        self.content_revision = revision;
    }
    fn render_stats(&self) -> bezel_core::ports::RenderStats {
        self.render_stats
    }
    fn next_change(&self) -> Option<std::time::Duration> {
        self.cards.next
    }
    fn set_motion(&mut self, allowed: bool) {
        self.cards.motion = allowed;
    }

    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame> {
        self.render_stats = bezel_core::ports::RenderStats {
            face_cache_enabled: self.face_cache_enabled,
            ..bezel_core::ports::RenderStats::default()
        };
        let size = theme.canvas;
        if size.area() == 0 {
            return Ok(Frame::filled(size, Rgba::default()));
        }
        let mut canvas = match self.canvas.take() {
            Some(c) if (c.width(), c.height()) == (size.width, size.height) => c,
            _ => new_canvas(size)?,
        };
        self.draw_background(&mut canvas, theme, assets, &context);
        let transitions = self.cards.update(theme, context.animation);
        for (index, element) in theme.elements.iter().enumerate() {
            if element.is_group {
                continue;
            }
            if let Some(transition) = transitions.get(&element.id) {
                self.draw_card(&mut canvas, theme, element, *transition, assets, &context)?;
                continue;
            }
            if element
                .card_member
                .as_ref()
                .is_some_and(|m| transitions.contains_key(&m.parent))
            {
                continue;
            }
            if theme.is_visible(element) {
                let key = (index, element.id);
                if element.card_member.is_some() || element.group_parent.is_some() {
                    let mut shown = element.clone();
                    shown.opacity = theme.rendered_opacity(element);
                    self.draw_element(&mut canvas, key, &shown, assets, &context);
                } else {
                    self.draw_element(&mut canvas, key, element, assets, &context);
                }
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

/// Draws a frame of a host-decoded video over the whole canvas, cropped to
/// the canvas shape when its own differs (like [`Fit::Cover`]). False when
/// the frame is empty.
fn draw_video_frame(canvas: &mut Pixmap, frame: &Frame) -> bool {
    let size = frame.size();
    if (size.width, size.height) == (canvas.width(), canvas.height()) {
        let data = canvas.data_mut();
        data.copy_from_slice(frame.as_rgba());
        composite::premultiply(data);
        return true;
    }
    let area = BoxF::new(0.0, 0.0, canvas.width() as f32, canvas.height() as f32);
    let plan = images::plan(Fit::Cover, (size.width, size.height), area);
    let pixels = RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec());
    let (Some(plan), Some(mut pixels)) = (plan, pixels) else {
        return false;
    };
    composite::premultiply(&mut pixels);
    let (x, y, w, h) = plan.crop;
    let view = imageops::crop_imm(&pixels, x, y, w, h);
    let (width, height) = plan.size;
    let mut scaled = imageops::resize(&*view, width, height, FilterType::CatmullRom).into_raw();
    composite::clamp_premultiplied(&mut scaled);
    let data = canvas.data_mut();
    if scaled.len() != data.len() {
        return false;
    }
    data.copy_from_slice(&scaled);
    true
}

/// The fraction of a binding's range the current reading represents.
fn value_fraction(binding: &Binding, context: &RenderContext<'_>) -> Option<f64> {
    let value = context.snapshot.get(&binding.key).value()?;
    fraction(value, binding.min, binding.max)
}

/// What a text element prints now.
fn text_of(content: &TextContent, context: &RenderContext<'_>) -> String {
    match content {
        TextContent::Player(p) => {
            let Some(m) = bezel_core::domain::playback::session(context.snapshot, &p.source) else {
                return p.empty_text.clone();
            };
            if p.hide_when_stopped && !m.playing {
                return String::new();
            }
            let mut text = format!("{}\n{}", m.title, m.artist);
            if p.show_source {
                text.push_str(&format!("\n{}", m.source));
            }
            if p.show_progress && m.duration > 0. {
                text.push_str(&format!(
                    "\n{}:{:02} / {}:{:02}",
                    m.position as u64 / 60,
                    m.position as u64 % 60,
                    m.duration as u64 / 60,
                    m.duration as u64 % 60
                ));
            }
            text
        }
        TextContent::Static(text) => text.clone(),
        TextContent::Weather(w) => w.text(context.snapshot, context.language),
        TextContent::Sensor {
            key,
            format,
            prefix,
            suffix,
        } => {
            let reading = context.snapshot.get(key);
            let quantity = context.quantities.quantity(key);
            let value = format_reading(&reading, quantity, *format);
            format!("{prefix}{value}{suffix}")
        }
        TextContent::Clock {
            pattern,
            language,
            casing,
        } => clock_case(
            &format_clock(pattern, &context.time, language.unwrap_or(context.language)),
            *casing,
        ),
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
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) {
        let (asset, fit) = match (&theme.background, context.backdrop) {
            (Background::DeviceVideo { color, .. }, backdrop) => {
                let windows = theme.elements.iter().any(|e| {
                    theme.is_visible(e)
                        && opacity_of(e.opacity) > 0
                        && matches!(
                            e.kind,
                            ElementKind::Shape {
                                video_window: true,
                                ..
                            }
                        )
                });
                let color = if windows {
                    paint::color(*color)
                } else if matches!(backdrop, Backdrop::OnDevice) {
                    Color::TRANSPARENT
                } else {
                    paint::color(VIDEO_PLACEHOLDER)
                };
                canvas.fill(color);
                return;
            }
            (Background::Color(color), _) => {
                canvas.fill(paint::color(*color));
                return;
            }
            (Background::Image { asset, fit }, _) => (asset, *fit),
            (Background::Video { .. }, Backdrop::OnDevice) => {
                // The screen plays the video itself: the elements go on a
                // base of A = 0, which lets the video show through.
                canvas.fill(Color::TRANSPARENT);
                return;
            }
            (Background::Video { .. }, Backdrop::Frame(frame)) => {
                if !draw_video_frame(canvas, frame) {
                    canvas.fill(paint::color(VIDEO_PLACEHOLDER));
                }
                return;
            }
            (
                Background::Video {
                    poster: Some(poster),
                    ..
                },
                Backdrop::Poster,
            ) => (poster, Fit::Cover),
            (Background::Video { poster: None, .. }, Backdrop::Poster) => {
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
        let Some(frame) = image.frame_at(context.animation) else {
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

    fn draw_card(
        &mut self,
        canvas: &mut Pixmap,
        theme: &Theme,
        parent: &Element,
        transition: crate::cards::Transition,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: &RenderContext<'_>,
    ) -> Result<()> {
        use bezel_core::domain::theme::CardEffect;
        let started = std::time::Instant::now();
        let draw_before = self.render_stats.face_draw_ms;
        let include = transition.settings.include_base;
        let mut buffers = std::mem::take(&mut self.card_buffers);
        let mut acquire = || -> Result<Pixmap> {
            let mut pixmap = match buffers
                .pop()
                .filter(|p| p.width() == theme.canvas.width && p.height() == theme.canvas.height)
            {
                Some(p) => p,
                None => new_canvas(theme.canvas)?,
            };
            pixmap.fill(Color::TRANSPARENT);
            Ok(pixmap)
        };
        let mut group = acquire()?;
        let mut combined = acquire()?;
        let has_windows = matches!(context.backdrop, Backdrop::OnDevice)
            && theme.elements.iter().any(|e| {
                (e.id == parent.id
                    || e.card_member
                        .as_ref()
                        .is_some_and(|m| m.parent == parent.id))
                    && matches!(
                        e.kind,
                        ElementKind::Shape {
                            video_window: true,
                            ..
                        }
                    )
            });
        let mut window_group = has_windows.then(&mut acquire).transpose()?;
        let mut window_combined = has_windows.then(&mut acquire).transpose()?;
        if !include {
            for (index, e) in theme.elements.iter().enumerate().filter(|(_, e)| {
                e.id == parent.id
                    || e.card_member
                        .as_ref()
                        .is_some_and(|m| m.parent == parent.id && m.face.is_none())
            }) {
                if theme.visible_without_face(e) {
                    let mut shown = e.clone();
                    shown.opacity = theme.rendered_opacity(e);
                    self.draw_element(canvas, (index, e.id), &shown, assets, context);
                }
            }
        }
        for pose in transition.poses(parent.frame) {
            group.fill(Color::TRANSPARENT);
            if let Some(window) = &mut window_group {
                window.fill(Color::TRANSPARENT);
            }
            if !self.face_cache_enabled {
                // Original face drawing path: no key construction, image probing,
                // cache insertion or surface copies. Keep timers for the A/B test.
                self.render_stats.face_misses += 1;
                let drawing = std::time::Instant::now();
                for (index, e) in theme.elements.iter().enumerate() {
                    let selected = e.card_member.as_ref().is_some_and(|m| {
                        m.parent == parent.id
                            && (m.face == Some(pose.face) || include && m.face.is_none())
                    }) || include && e.id == parent.id;
                    if selected && theme.visible_without_face(e) {
                        let mut shown = e.clone();
                        shown.opacity = theme.rendered_opacity(e);
                        self.draw_element(&mut group, (index, e.id), &shown, assets, context);
                        if let Some(window) = &mut window_group
                            && let ElementKind::Shape {
                                video_window: true,
                                shape,
                                fade,
                                ..
                            } = shown.kind
                        {
                            shown.kind = ElementKind::Shape {
                                video_window: false,
                                shape,
                                fade,
                                fill: Some(bezel_core::domain::theme::Paint::solid(Rgba::WHITE)),
                                stroke: None,
                            };
                            self.draw_element(window, (index, e.id), &shown, assets, context);
                        }
                    }
                }
                self.render_stats.face_draw_ms += drawing.elapsed().as_secs_f64() * 1000.0;
            } else {
                let selected: Vec<_> = theme
                    .elements
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        theme.visible_without_face(e)
                            && (e.card_member.as_ref().is_some_and(|m| {
                                m.parent == parent.id
                                    && (m.face == Some(pose.face) || include && m.face.is_none())
                            }) || include && e.id == parent.id)
                    })
                    .map(|(index, e)| {
                        let mut shown = e.clone();
                        shown.opacity = theme.rendered_opacity(e);
                        (index, shown)
                    })
                    .collect();
                // Animated image faces take the original drawing path. Static icons
                // and all sensor/text widgets may reuse surfaces until inputs change.
                let animated = selected.iter().any(|(_, e)| match &e.kind {
                    ElementKind::Image { asset, .. } => self
                        .images
                        .timeline(asset, assets, &mut self.diagnostics)
                        .is_some(),
                    _ => false,
                });
                let key = (self.card_scene, parent.id, pose.face, include, has_windows);
                let cached = self.face_cache.get(&key).filter(|face| {
                    self.content_revision != 0
                        && !animated
                        && face.revision == self.content_revision
                        && face.pixels.width() == theme.canvas.width
                        && face.pixels.height() == theme.canvas.height
                        && face.elements == selected
                        && face.time == context.time
                        && face.language == context.language
                        && face.snapshot == *context.snapshot
                        && face.histories == *context.histories
                        && face.quantities == *context.quantities
                });
                if let Some(face) = cached {
                    group.data_mut().copy_from_slice(face.pixels.data());
                    if let (Some(window), Some(kept)) = (&mut window_group, &face.window) {
                        window.data_mut().copy_from_slice(kept.data());
                    }
                    self.render_stats.face_hits += 1;
                } else {
                    self.render_stats.face_misses += 1;
                    let drawing = std::time::Instant::now();
                    for (index, shown) in &selected {
                        self.draw_element(&mut group, (*index, shown.id), shown, assets, context);
                        if let Some(window) = &mut window_group
                            && let ElementKind::Shape {
                                video_window: true,
                                shape,
                                fade,
                                ..
                            } = shown.kind
                        {
                            let mut mask = shown.clone();
                            mask.kind = ElementKind::Shape {
                                video_window: false,
                                shape,
                                fade,
                                fill: Some(bezel_core::domain::theme::Paint::solid(Rgba::WHITE)),
                                stroke: None,
                            };
                            self.draw_element(window, (*index, shown.id), &mask, assets, context);
                        }
                    }
                    self.render_stats.face_draw_ms += drawing.elapsed().as_secs_f64() * 1000.0;
                    let bytes = group.data().len() * if has_windows { 2 } else { 1 };
                    if self.content_revision != 0 && !animated && bytes <= FACE_CACHE_BYTES {
                        self.face_cache.remove(&key);
                        let used: usize = self
                            .face_cache
                            .values()
                            .map(|f| {
                                f.pixels.data().len()
                                    + f.window.as_ref().map_or(0, |w| w.data().len())
                            })
                            .sum();
                        if used + bytes > FACE_CACHE_BYTES {
                            self.face_cache.clear();
                        }
                        self.face_cache.insert(
                            key,
                            CachedFace {
                                revision: self.content_revision,
                                elements: selected,
                                time: context.time,
                                language: context.language,
                                snapshot: context.snapshot.clone(),
                                histories: context.histories.clone(),
                                quantities: context.quantities.clone(),
                                pixels: group.clone(),
                                window: window_group.clone(),
                            },
                        );
                    }
                }
            }
            if let Some(projection) = pose.projection {
                projection.draw(&group, &mut combined, pose.shade, pose.alpha);
                if let (Some(window), Some(combined)) = (&window_group, &mut window_combined) {
                    projection.draw(window, combined, 1.0, pose.alpha);
                }
                continue;
            }
            if pose.shade < 1.0 {
                // Premultiplied RGB shading preserves transparency.
                for pixel in group.data_mut().as_chunks_mut::<4>().0 {
                    for channel in &mut pixel[..3] {
                        *channel = (f32::from(*channel) * pose.shade).round() as u8;
                    }
                }
            }
            let paint = tiny_skia::PixmapPaint {
                opacity: pose.alpha,
                blend_mode: if transition.settings.effect == CardEffect::Fade {
                    tiny_skia::BlendMode::Plus
                } else {
                    tiny_skia::BlendMode::SourceOver
                },
                quality: tiny_skia::FilterQuality::Bilinear,
            };
            combined.draw_pixmap(0, 0, group.as_ref(), &paint, pose.transform, None);
            if let (Some(window), Some(combined)) = (&window_group, &mut window_combined) {
                combined.draw_pixmap(0, 0, window.as_ref(), &paint, pose.transform, None);
            }
        }
        let mask = if transition.settings.effect == CardEffect::Slide {
            let b = parent.frame;
            let mut mask = tiny_skia::Mask::new(canvas.width(), canvas.height());
            if let (Some(mask), Some(rect)) =
                (&mut mask, Rect::from_xywh(b.x, b.y, b.width, b.height))
            {
                mask.fill_path(
                    &tiny_skia::PathBuilder::from_rect(rect),
                    tiny_skia::FillRule::Winding,
                    true,
                    Transform::identity(),
                );
            }
            mask
        } else {
            None
        };
        if let Some(window) = &window_combined {
            canvas.draw_pixmap(
                0,
                0,
                window.as_ref(),
                &tiny_skia::PixmapPaint {
                    blend_mode: tiny_skia::BlendMode::DestinationOut,
                    ..tiny_skia::PixmapPaint::default()
                },
                Transform::identity(),
                mask.as_ref(),
            );
        }
        canvas.draw_pixmap(
            0,
            0,
            combined.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            mask.as_ref(),
        );
        self.card_buffers = vec![group, combined];
        self.card_buffers.extend(window_group);
        self.card_buffers.extend(window_combined);
        self.render_stats.face_compose_ms += started.elapsed().as_secs_f64() * 1000.0
            - (self.render_stats.face_draw_ms - draw_before);
        Ok(())
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
        if let ElementKind::Shape {
            video_window: true,
            shape,
            fade,
            ..
        } = &element.kind
            && matches!(context.backdrop, Backdrop::OnDevice)
            && let Some(path) = shape::outline_path(element.frame, *shape, 0.0)
            && let Some(mut mask) = tiny_skia::Mask::new(bounds.width(), bounds.height())
        {
            mask.fill_path(
                &path,
                tiny_skia::FillRule::Winding,
                true,
                Transform::from_translate(-(bounds.x() as f32), -(bounds.y() as f32)),
            );
            let width = canvas.width() as usize;
            let frame = element.frame;
            let ramp = fade.map(|f| f.ramp());
            for (index, coverage) in mask.data().iter().enumerate() {
                if *coverage == 0 {
                    continue;
                }
                let x = bounds.x() as usize + index % bounds.width() as usize;
                let y = bounds.y() as usize + index / bounds.width() as usize;
                let strength = ramp.as_ref().map_or(1.0, |f| {
                    f(
                        (x as f32 + 0.5 - frame.x) / frame.width,
                        (y as f32 + 0.5 - frame.y) / frame.height,
                    )
                });
                let retain =
                    1.0 - f32::from(*coverage) / 255.0 * f32::from(opacity) / 255.0 * strength;
                let pixel = &mut canvas.data_mut()[(y * width + x) * 4..][..4];
                for channel in pixel {
                    *channel = (f32::from(*channel) * retain).round() as u8;
                }
            }
        }
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
        if let ElementKind::Shape {
            fade: Some(fade), ..
        }
        | ElementKind::Image {
            fade: Some(fade), ..
        } = &element.kind
        {
            let frame = element.frame;
            let ramp = fade.ramp();
            for (index, pixel) in scratch.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x = bounds.x() as f32 + (index % w as usize) as f32 + 0.5;
                let y = bounds.y() as f32 + (index / w as usize) as f32 + 0.5;
                let strength = ramp((x - frame.x) / frame.width, (y - frame.y) / frame.height);
                for channel in pixel {
                    *channel = (f32::from(*channel) * strength).round() as u8;
                }
            }
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
                let mut area = area;
                if let TextContent::Player(p) = content {
                    let media = bezel_core::domain::playback::session(context.snapshot, &p.source);
                    if p.hide_when_stopped && media.is_none_or(|m| !m.playing) {
                        return;
                    }
                    if let Some(m) = media {
                        if p.show_cover {
                            let gap = p
                                .cover_gap
                                .unwrap_or(style.size * 0.3)
                                .clamp(0., area.width.max(0.));
                            let side = area
                                .height
                                .min(area.width * 0.4)
                                .min((area.width - gap).max(0.));
                            if !m.cover.is_empty() {
                                use std::hash::{Hash, Hasher};
                                let mut hash = std::collections::hash_map::DefaultHasher::new();
                                m.cover.hash(&mut hash);
                                let asset =
                                    AssetRef(format!("runtime/player/{:x}.png", hash.finish()));
                                let covers = BTreeMap::from([(asset.clone(), m.cover.clone())]);
                                self.draw_image(
                                    layer,
                                    &asset,
                                    Fit::Contain,
                                    BoxF::new(area.x, area.y, side, side),
                                    &covers,
                                    context,
                                );
                            }
                            let cover_path = if let Some(r) = p.cover_corners {
                                crate::path::rounded_corners(area.x, area.y, side, side, r)
                            } else if p.cover_radius > 0. {
                                crate::path::rounded_rect(
                                    area.x,
                                    area.y,
                                    side,
                                    side,
                                    p.cover_radius,
                                )
                            } else {
                                None
                            };
                            if let Some(path) = cover_path
                                && let Some(mask) = layer.mask_of(&path)
                            {
                                for (pixel, alpha) in layer
                                    .pixmap
                                    .data_mut()
                                    .as_chunks_mut::<4>()
                                    .0
                                    .iter_mut()
                                    .zip(mask.data())
                                {
                                    for channel in pixel {
                                        *channel = ((u16::from(*channel) * u16::from(*alpha) + 127)
                                            / 255)
                                            as u8;
                                    }
                                }
                            }
                            area.x += side + gap;
                            area.width = (area.width - side - gap).max(0.);
                        }
                        if p.show_progress && m.duration > 0. {
                            let bar = BoxF::new(area.x, area.y + area.height - 4., area.width, 4.);
                            crate::shape::draw(
                                layer,
                                bar,
                                bezel_core::domain::theme::ShapeKind::Rect { radius: 2. },
                                Some(&bezel_core::domain::theme::Paint::solid(Rgba {
                                    r: 100,
                                    g: 100,
                                    b: 100,
                                    a: 100,
                                })),
                                None,
                            );
                            let width = bar.width * (m.position / m.duration).clamp(0., 1.) as f32;
                            crate::shape::draw(
                                layer,
                                BoxF::new(bar.x, bar.y, width, bar.height),
                                bezel_core::domain::theme::ShapeKind::Rect { radius: 2. },
                                Some(&style.paint),
                                None,
                            );
                            area.height = (area.height - 8.).max(0.);
                        }
                    }
                }
                if let TextContent::Weather(w) = content
                    && w.show_icon
                {
                    let gap = w
                        .icon_gap
                        .unwrap_or(style.size * 0.3)
                        .clamp(0.0, area.width.max(0.0));
                    let side = w
                        .icon_size
                        .unwrap_or(style.size * 2.0)
                        .min(area.height)
                        .min((area.width - gap).max(0.0))
                        .min(if w.icon_size.is_none() {
                            area.width * 0.3
                        } else {
                            area.width
                        });
                    let code = match context.snapshot.get(&w.keys()[1]) {
                        bezel_core::domain::sensor::Reading::Value(v) => v as u16,
                        _ => 999,
                    };
                    crate::weather::draw(
                        layer,
                        BoxF::new(area.x, area.y + (area.height - side) / 2.0, side, side),
                        code,
                        &style.paint,
                        w.icon_style,
                    );
                    area.x += side + gap;
                    area.width = (area.width - side - gap).max(0.0);
                }
                let text = text_of(content, context);
                let job = TextJob {
                    text: &text,
                    style,
                    area,
                };
                self.text.draw(layer, &job, assets, &mut self.diagnostics);
            }
            ElementKind::Image { asset, fit, .. } => {
                self.draw_image(layer, asset, *fit, area, assets, context);
            }
            ElementKind::Shape {
                video_window,
                shape,
                fill,
                stroke,
                ..
            } => {
                let placeholder = bezel_core::domain::theme::Paint::solid(VIDEO_PLACEHOLDER);
                let fill = if *video_window {
                    if matches!(context.backdrop, Backdrop::OnDevice) {
                        None
                    } else {
                        Some(&placeholder)
                    }
                } else {
                    fill.as_ref()
                };
                shape::draw(layer, area, *shape, fill, *stroke);
            }
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
        if let Some(frame) = image.frame_at(context.animation) {
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
        if let Some(frame) = image.frame_at(context.animation) {
            let (px, py) = pose.pivot;
            let placement =
                Transform::from_translate(plan.at.0, plan.at.1).post_rotate_at(pose.angle, px, py);
            layer.draw_transformed(frame.as_ref(), placement);
        }
    }
}

fn draw_ring(layer: &mut Layer<'_>, element: &Element, context: &RenderContext<'_>) {
    let ElementKind::Ring {
        test_full,
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
        if *test_full {
            Some(1.0)
        } else {
            value_fraction(binding, context)
        },
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
    use crate::testkit::{self, Scene, count, element, png, px, render_over};
    use bezel_core::domain::theme::{Paint, ShapeKind};

    const CLEAR: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    fn rect(frame: BoxF, color: Rgba, opacity: f32) -> Element {
        let mut e = element(
            frame,
            ElementKind::Shape {
                video_window: false,
                fade: None,
                shape: ShapeKind::Rect { radius: 0.0 },
                fill: Some(Paint::solid(color)),
                stroke: None,
            },
        );
        e.opacity = opacity;
        e
    }

    #[test]
    fn device_video_background_renders_a_transparent_base() {
        let mut r = testkit::renderer();
        let green = Rgba::opaque(0, 255, 0);
        let red = Rgba::opaque(255, 0, 0);
        let white = Rgba::WHITE;
        let scene = Scene::empty().asset("poster.png", png(2, 2, |_, _| green));
        let disc = element(
            BoxF::new(20.0, 0.0, 12.0, 12.0),
            ElementKind::Shape {
                video_window: false,
                fade: None,
                shape: ShapeKind::Ellipse,
                fill: Some(Paint::solid(white)),
                stroke: None,
            },
        );
        let elements = vec![
            rect(BoxF::new(0.0, 0.0, 8.0, 8.0), red, 1.0),
            rect(BoxF::new(10.0, 0.0, 8.0, 8.0), white, 0.5),
            disc,
        ];
        let video = Background::Video {
            asset: AssetRef("assets/clip.mp4".into()),
            poster: Some(AssetRef("poster.png".into())),
            framing: None,
        };
        let theme = testkit::theme(32, 16, video, elements.clone());

        let overlay = render_over(&mut r, &theme, &scene, Backdrop::OnDevice);
        assert_eq!(px(&overlay, 0, 12), CLEAR, "A = 0 lets the video show");
        assert_eq!(count(&overlay, (0, 8, 20, 16), |p| p == CLEAR), 20 * 8);
        assert_eq!(px(&overlay, 4, 4), red, "opaque elements hide the video");
        assert_eq!(
            px(&overlay, 14, 4),
            Rgba { a: 128, ..white },
            "straight alpha"
        );
        let edge = count(&overlay, (20, 0, 32, 12), |p| p.a > 0 && p.a < 255);
        assert!(edge > 0, "anti-aliased edges keep their partial alpha");
        let tinted = count(&overlay, (20, 0, 32, 12), |p| {
            p.a > 0 && p.r.min(p.g).min(p.b) < 250
        });
        assert_eq!(tinted, 0, "partial pixels keep the element's color");

        let poster = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert_eq!(px(&poster, 0, 12), green, "previews show the poster");
        assert_eq!(px(&poster, 4, 4), red);

        for background in [
            Background::Color(Rgba::opaque(1, 2, 3)),
            Background::Image {
                asset: AssetRef("poster.png".into()),
                fit: Fit::Fill,
            },
        ] {
            let still = testkit::theme(32, 16, background, elements.clone());
            let a = render_over(&mut r, &still, &scene, Backdrop::Poster);
            let b = render_over(&mut r, &still, &scene, Backdrop::OnDevice);
            assert_eq!(
                a.as_rgba(),
                b.as_rgba(),
                "other backgrounds ignore the backdrop"
            );
        }
    }

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
    #[test]
    fn groups_render_nested_opacity_visibility_and_card_ownership_once() {
        use bezel_core::domain::theme::{Card, CardMember};
        let black = Rgba::opaque(0, 0, 0);
        let red = Rgba::opaque(255, 0, 0);
        let mut group = rect(BoxF::new(0.0, 0.0, 16.0, 16.0), CLEAR, 0.5);
        group.id = ElementId(1);
        group.is_group = true;
        let mut child = rect(group.frame, red, 1.0);
        child.id = ElementId(2);
        child.group_parent = Some(group.id);
        let mut theme = testkit::theme(16, 16, Background::Color(black), vec![group, child]);
        let mut r = testkit::renderer();
        let scene = Scene::empty();
        let frame = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert!((126..=129).contains(&px(&frame, 4, 4).r));
        theme.elements[0].visible = false;
        assert_eq!(
            px(&render_over(&mut r, &theme, &scene, Backdrop::Poster), 4, 4),
            black
        );
        theme.elements[0].visible = true;
        let mut parent = rect(theme.elements[0].frame, CLEAR, 0.5);
        parent.id = ElementId(3);
        parent.card = Some(Card {
            triggers: Vec::new(),
            faces: vec!["A".into(), "B".into()],
            active_face: 0,
            rotation_seconds: None,
            transition: None,
        });
        for e in &mut theme.elements {
            e.card_member = Some(CardMember {
                parent: parent.id,
                face: Some(0),
            });
        }
        theme.elements.insert(0, parent);
        let frame = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert!((62..=66).contains(&px(&frame, 4, 4).r));
        theme.elements[0].card.as_mut().unwrap().active_face = 1;
        assert_eq!(
            px(&render_over(&mut r, &theme, &scene, Backdrop::Poster), 4, 4),
            black
        );
    }

    #[test]
    fn card_faces_render_shared_base_and_inherit_parent_opacity() {
        use bezel_core::domain::theme::{Card, CardMember};
        let black = Rgba::opaque(0, 0, 0);
        let red = Rgba::opaque(255, 0, 0);
        let green = Rgba::opaque(0, 255, 0);
        let mut parent = rect(BoxF::new(0.0, 0.0, 32.0, 16.0), black, 1.0);
        parent.card = Some(Card {
            triggers: Vec::new(),
            faces: vec!["A".into(), "B".into()],
            active_face: 0,
            rotation_seconds: None,
            transition: None,
        });
        let mut base = rect(BoxF::new(0.0, 0.0, 8.0, 8.0), Rgba::WHITE, 1.0);
        base.id = ElementId(2);
        base.card_member = Some(CardMember {
            parent: parent.id,
            face: None,
        });
        let mut a = rect(BoxF::new(10.0, 0.0, 8.0, 8.0), red, 1.0);
        a.id = ElementId(3);
        a.card_member = Some(CardMember {
            parent: parent.id,
            face: Some(0),
        });
        let mut b = a.clone();
        b.id = ElementId(4);
        b.kind = rect(b.frame, green, 1.0).kind;
        b.card_member.as_mut().unwrap().face = Some(1);
        let mut theme = testkit::theme(32, 16, Background::Color(black), vec![parent, base, a, b]);
        let mut r = testkit::renderer();
        let scene = Scene::empty();
        let frame = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert_eq!(px(&frame, 4, 4), Rgba::WHITE);
        assert_eq!(px(&frame, 14, 4), red);
        theme.elements[0].card.as_mut().unwrap().active_face = 1;
        let frame = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert_eq!(px(&frame, 14, 4), green);
        theme.elements[0].opacity = 0.5;
        let frame = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert!((126..=129).contains(&px(&frame, 14, 4).g));
        theme.elements[0].visible = false;
        let frame = render_over(&mut r, &theme, &scene, Backdrop::Poster);
        assert_eq!(px(&frame, 4, 4), black);
        assert_eq!(px(&frame, 14, 4), black);
    }
    #[test]
    fn card_transitions_schedule_intermediate_frames_and_finish_exactly() {
        use bezel_core::app::ThemeRuntime;
        use bezel_core::domain::clock::Language;
        use bezel_core::domain::theme::{
            Card, CardDirection, CardEffect, CardMember, CardTransition,
        };
        use std::time::Duration;
        let black = Rgba::opaque(0, 0, 0);
        let red = Rgba::opaque(255, 0, 0);
        let green = Rgba::opaque(0, 255, 0);
        for effect in [CardEffect::Fade, CardEffect::Slide, CardEffect::Flip] {
            let mut parent = rect(BoxF::new(0.0, 0.0, 32.0, 16.0), black, 1.0);
            parent.card = Some(Card {
                triggers: Vec::new(),
                faces: vec!["A".into(), "B".into()],
                active_face: 0,
                rotation_seconds: None,
                transition: Some(CardTransition {
                    effect,
                    direction: CardDirection::Left,
                    duration_ms: 600,
                    include_base: false,
                }),
            });
            let mut a = rect(BoxF::new(10.0, 0.0, 8.0, 8.0), red, 1.0);
            a.id = ElementId(2);
            a.card_member = Some(CardMember {
                parent: parent.id,
                face: Some(0),
            });
            let mut b = a.clone();
            b.id = ElementId(3);
            b.kind = rect(b.frame, green, 1.0).kind;
            b.card_member.as_mut().unwrap().face = Some(1);
            let mut theme = testkit::theme(32, 16, Background::Color(black), vec![parent, a, b]);
            let mut runtime = ThemeRuntime::new(theme.clone(), BTreeMap::new(), Language::English);
            let mut renderer = testkit::renderer();
            assert!(
                runtime
                    .preview(&mut renderer, testkit::TIME, Duration::ZERO)
                    .unwrap()
                    .1
                    .is_none()
            );
            theme.elements[0].card.as_mut().unwrap().active_face = 1;
            runtime.replace_theme(theme.clone());
            let (first, due) = runtime
                .preview(&mut renderer, testkit::TIME, Duration::from_millis(100))
                .unwrap();
            assert_eq!(px(&first, 14, 4), red);
            assert_eq!(due, Some(Duration::from_millis(133)));
            let (middle, due) = runtime
                .preview(&mut renderer, testkit::TIME, Duration::from_millis(400))
                .unwrap();
            assert!(due.is_some());
            if effect == CardEffect::Fade {
                let p = px(&middle, 14, 4);
                assert!((126..=129).contains(&p.r) && (126..=129).contains(&p.g));
            } else {
                assert_eq!(px(&middle, 14, 4), black);
            }
            let (last, due) = runtime
                .preview(&mut renderer, testkit::TIME, Duration::from_millis(700))
                .unwrap();
            assert_eq!(px(&last, 14, 4), green);
            assert_eq!(due, None);
            renderer.set_motion(false);
            theme.elements[0].card.as_mut().unwrap().active_face = 0;
            runtime.replace_theme(theme);
            let (still, due) = runtime
                .preview(&mut renderer, testkit::TIME, Duration::from_millis(800))
                .unwrap();
            assert_eq!(px(&still, 14, 4), red);
            assert_eq!(due, None);
            if effect == CardEffect::Flip {
                let mut window_theme = runtime.theme().clone();
                if let ElementKind::Shape { video_window, .. } = &mut window_theme.elements[0].kind
                {
                    *video_window = true;
                }
                window_theme.elements[0]
                    .card
                    .as_mut()
                    .unwrap()
                    .transition
                    .as_mut()
                    .unwrap()
                    .include_base = true;
                renderer.set_motion(true);
                let mut scene = Scene::empty();
                scene.animation = Duration::from_millis(900);
                let first = render_over(&mut renderer, &window_theme, &scene, Backdrop::OnDevice);
                assert_eq!(px(&first, 16, 12).a, 0);
                window_theme.elements[0].card.as_mut().unwrap().active_face = 1;
                scene.animation = Duration::from_millis(1000);
                let first = render_over(&mut renderer, &window_theme, &scene, Backdrop::OnDevice);
                assert_eq!(
                    px(&first, 16, 12).a,
                    0,
                    "device video remains visible when motion starts"
                );
                scene.animation = Duration::from_millis(1150);
                let middle = render_over(&mut renderer, &window_theme, &scene, Backdrop::OnDevice);
                assert_eq!(
                    px(&middle, 16, 12).a,
                    0,
                    "transformed video window remains a cutout"
                );
            }
        }
    }
    #[test]
    fn cached_card_faces_match_fresh_pixels_and_invalidate_inputs() {
        use bezel_core::domain::theme::{
            Card, CardDirection, CardEffect, CardMember, CardTransition,
        };
        use std::time::Duration;
        let mut parent = rect(BoxF::new(0., 0., 32., 16.), Rgba::opaque(20, 30, 40), 1.);
        parent.card = Some(Card {
            triggers: Vec::new(),
            faces: vec!["A".into(), "B".into()],
            active_face: 0,
            rotation_seconds: None,
            transition: Some(CardTransition {
                effect: CardEffect::Flip,
                direction: CardDirection::Left,
                duration_ms: 750,
                include_base: true,
            }),
        });
        let mut member = rect(BoxF::new(4., 4., 12., 8.), Rgba::WHITE, 0.8);
        member.id = ElementId(2);
        member.card_member = Some(CardMember {
            parent: parent.id,
            face: Some(0),
        });
        let mut theme = testkit::theme(
            32,
            16,
            Background::DeviceVideo {
                path: bezel_core::domain::storage::RemotePath::parse("internal/video/a.mp4")
                    .unwrap(),
                repeat: bezel_core::domain::storage::Repeat::Loop,
                color: Rgba::BLACK,
            },
            vec![parent, member],
        );
        let mut scene = Scene::empty();
        let mut cached = testkit::renderer();
        let mut fresh = testkit::renderer();
        cached.face_cache_enabled = true;
        cached.set_content_revision(1);
        for r in [&mut cached, &mut fresh] {
            render_over(r, &theme, &scene, Backdrop::Poster);
        }
        theme.elements[0].card.as_mut().unwrap().active_face = 1;
        for ms in [100, 150, 200, 250, 500, 550, 600] {
            scene.animation = Duration::from_millis(ms);
            let a = render_over(&mut cached, &theme, &scene, Backdrop::Poster);
            let b = render_over(&mut fresh, &theme, &scene, Backdrop::Poster);
            assert_eq!(a, b, "cached pose {ms}");
            if ms == 150 {
                assert_eq!(cached.render_stats().face_hits, 1);
            }
            let a = render_over(&mut cached, &theme, &scene, Backdrop::OnDevice);
            let b = render_over(&mut fresh, &theme, &scene, Backdrop::OnDevice);
            assert_eq!(a, b, "overlay pose {ms}");
            assert_eq!(
                cached.render_stats().face_hits,
                1,
                "same face across backgrounds"
            );
        }
        for change in 0..4 {
            match change {
                0 => {
                    scene.snapshot.insert(
                        testkit::key("cpu.usage"),
                        bezel_core::domain::sensor::Reading::Value(20.),
                    );
                }
                1 => {
                    theme.elements[0].opacity = 0.3;
                }
                2 => {
                    scene.time.second += 1;
                }
                _ => cached.set_content_revision(2),
            }
            let a = render_over(&mut cached, &theme, &scene, Backdrop::OnDevice);
            let b = render_over(&mut fresh, &theme, &scene, Backdrop::OnDevice);
            assert_eq!(a, b);
            assert_eq!(
                cached.render_stats().face_misses,
                1,
                "invalidation {change}"
            );
        }
        // Window masks are reused only with their matching backdrop variant.
        if let ElementKind::Shape { video_window, .. } = &mut theme.elements[0].kind {
            *video_window = true;
        }
        for backdrop in [
            Backdrop::Poster,
            Backdrop::OnDevice,
            Backdrop::Poster,
            Backdrop::OnDevice,
        ] {
            let a = render_over(&mut cached, &theme, &scene, backdrop);
            let b = render_over(&mut fresh, &theme, &scene, backdrop);
            assert_eq!(a, b, "cached window mask");
        }
        // A GIF never reuses a face across animation times.
        theme.elements[1].card_member.as_mut().unwrap().face = Some(1);
        theme.elements[1].kind = ElementKind::Image {
            fade: None,
            asset: AssetRef("a.gif".into()),
            fit: Fit::Fill,
        };
        scene.assets.insert(
            AssetRef("a.gif".into()),
            testkit::gif(2, 2, &[(Rgba::WHITE, 100), (Rgba::BLACK, 100)]),
        );
        cached.set_content_revision(3);
        for ms in [610, 630] {
            scene.animation = Duration::from_millis(ms);
            let a = render_over(&mut cached, &theme, &scene, Backdrop::OnDevice);
            let b = render_over(&mut fresh, &theme, &scene, Backdrop::OnDevice);
            assert_eq!(a, b, "animated face");
            assert_eq!(cached.render_stats().face_hits, 0);
        }
        assert!(
            cached
                .face_cache
                .values()
                .map(|f| f.pixels.data().len() + f.window.as_ref().map_or(0, |w| w.data().len()))
                .sum::<usize>()
                <= FACE_CACHE_BYTES
        );
    }

    /// Run explicitly in release mode; records CPU render cost, excluding serial/IPC.
    #[test]
    #[ignore]
    fn profile_card_flip_480x1920() {
        use bezel_core::domain::theme::{
            Card, CardDirection, CardEffect, CardMember, CardTransition,
        };
        use std::time::{Duration, Instant};
        let mut parent = rect(
            BoxF::new(36.0, 650.0, 408.0, 408.0),
            Rgba::opaque(30, 41, 59),
            1.0,
        );
        parent.card = Some(Card {
            triggers: Vec::new(),
            faces: vec!["A".into(), "B".into()],
            active_face: 0,
            rotation_seconds: None,
            transition: Some(CardTransition {
                effect: CardEffect::Flip,
                direction: CardDirection::Left,
                duration_ms: 1000,
                include_base: true,
            }),
        });
        let mut members = vec![parent];
        for face in 0..2 {
            for i in 0..8 {
                let mut e = rect(
                    BoxF::new(65.0, 690.0 + i as f32 * 38.0, 330.0, 20.0),
                    if face == 0 {
                        Rgba::opaque(34, 211, 238)
                    } else {
                        Rgba::opaque(192, 132, 252)
                    },
                    1.0,
                );
                e.id = ElementId(2 + face as u32 * 8 + i as u32);
                e.card_member = Some(CardMember {
                    parent: ElementId(1),
                    face: Some(face),
                });
                members.push(e);
            }
        }
        let mut theme = testkit::theme(480, 1920, Background::Color(Rgba::BLACK), members);
        let mut renderer = testkit::renderer();
        let mut scene = Scene::empty();
        render_over(&mut renderer, &theme, &scene, Backdrop::Poster);
        renderer.set_content_revision(1);
        theme.elements[0].card.as_mut().unwrap().active_face = 1;
        let mut costs = Vec::new();
        let mut film = RgbaImage::new(480 * 5, 600);
        for i in 0..60 {
            scene.animation = Duration::from_millis(i * 16);
            let started = Instant::now();
            let frame = render_over(&mut renderer, &theme, &scene, Backdrop::Poster);
            costs.push(started.elapsed().as_secs_f64() * 1000.0);
            if [0, 15, 30, 45, 59].contains(&i) {
                let image = RgbaImage::from_raw(480, 1920, frame.as_rgba().to_vec()).unwrap();
                let crop = image::imageops::crop_imm(&image, 0, 560, 480, 600).to_image();
                image::imageops::replace(
                    &mut film,
                    &crop,
                    ([0, 15, 30, 45, 59].iter().position(|v| *v == i).unwrap() * 480) as i64,
                    0,
                );
            }
        }
        costs.sort_by(f64::total_cmp);
        eprintln!(
            "480x1920 Flip CPU: median {:.2} ms, p95 {:.2} ms, max {:.2} ms",
            costs[30], costs[57], costs[59]
        );
        film.save("../../target/card-flip-perspective.png").unwrap();
    }
}
