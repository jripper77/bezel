//! A showcase theme for the 8.8" portrait panel (480x1920): title, clock,
//! CPU ring gauge, GPU bars, RAM graph and a needle gauge, ~30 elements,
//! with fixed sensor values. Shared by `examples/showcase.rs` and the timing
//! test.

use std::collections::BTreeMap;
use std::io::Cursor;

use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::Rgba;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::history::Histories;
use bezel_core::domain::sensor::{DisplayFormat, Quantities, Reading, SensorKey, Snapshot, keys};

/// Tests format sensor text by the keys' well-known units.
static NO_QUANTITIES: Quantities = Quantities::new();
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Cap, Direction, Element, ElementId, ElementKind, Fit,
    FontSpec, GraphStyle, HAlign, Paint, Segments, ShapeKind, TextContent, TextStyle, Theme,
    VAlign,
};
use bezel_core::ports::{Backdrop, RenderContext};

const MUTED: Rgba = Rgba::opaque(140, 150, 175);
const CYAN: Rgba = Rgba::opaque(0, 229, 255);
const VIOLET: Rgba = Rgba::opaque(170, 90, 255);
const TRACK: Rgba = Rgba {
    r: 255,
    g: 255,
    b: 255,
    a: 24,
};
const ICON: &str = "assets/chip.png";

/// Wednesday 2026-09-30 21:05:42.
pub const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 42,
    weekday: 2,
};

fn key(k: &str) -> Option<SensorKey> {
    SensorKey::new(k)
}

fn linear(angle: f32, from: Rgba, to: Rgba) -> Paint {
    Paint::Linear {
        angle,
        stops: vec![(0.0, from), (1.0, to)],
    }
}

fn binding(k: &str, max: f64) -> Option<Binding> {
    Some(Binding {
        key: key(k)?,
        min: 0.0,
        max,
    })
}

fn style(size: f32, weight: u16, paint: Paint, align: HAlign) -> TextStyle {
    TextStyle {
        font: FontSpec {
            family: "Inter".into(),
            weight,
            ..FontSpec::default()
        },
        size,
        paint,
        align,
        valign: VAlign::Middle,
        letter_spacing: 0.0,
    }
}

fn spaced(mut style: TextStyle, spacing: f32) -> TextStyle {
    style.letter_spacing = spacing;
    style
}

fn text(content: TextContent, style: TextStyle) -> ElementKind {
    ElementKind::Text { content, style }
}

fn label(s: &str) -> TextContent {
    TextContent::Static(s.into())
}

fn sensor(k: &str) -> Option<TextContent> {
    Some(TextContent::Sensor {
        key: key(k)?,
        format: DisplayFormat::default(),
        prefix: String::new(),
        suffix: String::new(),
    })
}

fn card(y: f32, height: f32) -> (BoxF, ElementKind) {
    let kind = ElementKind::Shape {
        shape: ShapeKind::Rect { radius: 28.0 },
        fill: Some(Paint::solid(Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 12,
        })),
        stroke: Some((
            Rgba {
                r: 255,
                g: 255,
                b: 255,
                a: 30,
            },
            1.5,
        )),
    };
    (BoxF::new(24.0, y, 432.0, height), kind)
}

fn header() -> Vec<(BoxF, ElementKind)> {
    let full = BoxF::new(0.0, 0.0, 480.0, 1920.0);
    let backdrop = ElementKind::Shape {
        shape: ShapeKind::Rect { radius: 0.0 },
        fill: Some(linear(
            90.0,
            Rgba::opaque(22, 26, 48),
            Rgba::opaque(6, 7, 12),
        )),
        stroke: None,
    };
    let title = spaced(
        style(64.0, 800, linear(0.0, CYAN, VIOLET), HAlign::Center),
        12.0,
    );
    let subtitle = spaced(style(20.0, 500, Paint::solid(MUTED), HAlign::Center), 3.0);
    let clock = style(132.0, 700, Paint::solid(Rgba::WHITE), HAlign::Center);
    let date = style(
        28.0,
        400,
        Paint::solid(Rgba::opaque(170, 180, 205)),
        HAlign::Center,
    );
    let at = |x, y, w, h| BoxF::new(x, y, w, h);
    vec![
        (full, backdrop),
        (at(24.0, 36.0, 432.0, 80.0), text(label("BEZEL"), title)),
        (
            at(24.0, 112.0, 432.0, 30.0),
            text(label("SMART SCREEN · 8.8\""), subtitle),
        ),
        (
            at(24.0, 170.0, 432.0, 150.0),
            text(
                TextContent::Clock {
                    pattern: "%H:%M".into(),
                },
                clock,
            ),
        ),
        (
            at(24.0, 318.0, 432.0, 40.0),
            text(
                TextContent::Clock {
                    pattern: "%A, %e %B".into(),
                },
                date,
            ),
        ),
    ]
}

fn cpu(top: f32) -> Option<Vec<(BoxF, ElementKind)>> {
    let at = |x, y, w, h| BoxF::new(x, y, w, h);
    let caption = spaced(style(26.0, 700, Paint::solid(MUTED), HAlign::Left), 4.0);
    let warm = style(
        26.0,
        600,
        Paint::solid(Rgba::opaque(255, 170, 90)),
        HAlign::Right,
    );
    let ring = ElementKind::Ring {
        binding: binding(keys::CPU_USAGE, 100.0)?,
        start_angle: -135.0,
        sweep: 270.0,
        thickness: 28.0,
        clockwise: true,
        fill: linear(0.0, CYAN, VIOLET),
        track: Some(Paint::solid(TRACK)),
        cap: Cap::Round,
        segments: None,
    };
    let icon = ElementKind::Image {
        asset: AssetRef(ICON.into()),
        fit: Fit::Contain,
    };
    let big = style(76.0, 800, Paint::solid(Rgba::WHITE), HAlign::Center);
    let small = style(24.0, 500, Paint::solid(MUTED), HAlign::Center);
    Some(vec![
        card(top, 380.0),
        (at(56.0, top + 32.0, 28.0, 28.0), icon),
        (
            at(94.0, top + 28.0, 200.0, 36.0),
            text(label("CPU"), caption),
        ),
        (
            at(224.0, top + 28.0, 200.0, 36.0),
            text(sensor(keys::CPU_TEMPERATURE)?, warm),
        ),
        (at(90.0, top + 80.0, 300.0, 300.0), ring),
        (
            at(90.0, top + 175.0, 300.0, 100.0),
            text(sensor(keys::CPU_USAGE)?, big),
        ),
        (
            at(90.0, top + 265.0, 300.0, 36.0),
            text(sensor(keys::CPU_FREQUENCY)?, small),
        ),
    ])
}

fn gpu(top: f32) -> Option<Vec<(BoxF, ElementKind)>> {
    let at = |x, y, w, h| BoxF::new(x, y, w, h);
    let caption = spaced(style(26.0, 700, Paint::solid(MUTED), HAlign::Left), 4.0);
    let value = style(26.0, 700, Paint::solid(Rgba::WHITE), HAlign::Right);
    let usage = ElementKind::Bar {
        binding: binding(keys::GPU_USAGE, 100.0)?,
        direction: Direction::LeftToRight,
        fill: linear(0.0, Rgba::opaque(80, 230, 140), Rgba::opaque(250, 210, 80)),
        track: Some(Paint::solid(TRACK)),
        radius: 15.0,
        segments: None,
    };
    let temperature = ElementKind::Bar {
        binding: binding(keys::GPU_TEMPERATURE, 100.0)?,
        direction: Direction::LeftToRight,
        fill: linear(0.0, Rgba::opaque(80, 200, 255), Rgba::opaque(255, 90, 120)),
        track: Some(Paint::solid(TRACK)),
        radius: 3.0,
        segments: Some(Segments {
            count: 16,
            gap: 4.0,
        }),
    };
    let small = style(22.0, 500, Paint::solid(MUTED), HAlign::Left);
    let small_value = style(
        22.0,
        600,
        Paint::solid(Rgba::opaque(255, 170, 90)),
        HAlign::Right,
    );
    Some(vec![
        card(top, 250.0),
        (
            at(56.0, top + 28.0, 200.0, 36.0),
            text(label("GPU"), caption),
        ),
        (
            at(224.0, top + 28.0, 200.0, 36.0),
            text(sensor(keys::GPU_USAGE)?, value),
        ),
        (at(56.0, top + 80.0, 368.0, 30.0), usage),
        (at(56.0, top + 140.0, 368.0, 22.0), temperature),
        (
            at(56.0, top + 176.0, 200.0, 40.0),
            text(label("Temperature"), small),
        ),
        (
            at(224.0, top + 176.0, 200.0, 40.0),
            text(sensor(keys::GPU_TEMPERATURE)?, small_value),
        ),
    ])
}

fn ram(top: f32) -> Option<Vec<(BoxF, ElementKind)>> {
    let at = |x, y, w, h| BoxF::new(x, y, w, h);
    let caption = spaced(style(26.0, 700, Paint::solid(MUTED), HAlign::Left), 4.0);
    let value = style(26.0, 700, Paint::solid(Rgba::WHITE), HAlign::Right);
    let blue = Rgba::opaque(110, 190, 255);
    let graph = ElementKind::Graph {
        binding: binding(keys::MEMORY_PERCENT, 100.0)?,
        history: 60,
        style: GraphStyle::Area,
        color: blue,
        fill: Some(linear(90.0, Rgba { a: 150, ..blue }, Rgba { a: 0, ..blue })),
        line_width: 3.0,
        autoscale: false,
    };
    Some(vec![
        card(top, 330.0),
        (
            at(56.0, top + 28.0, 200.0, 36.0),
            text(label("RAM"), caption),
        ),
        (
            at(224.0, top + 28.0, 200.0, 36.0),
            text(sensor(keys::MEMORY_USED)?, value),
        ),
        (at(56.0, top + 80.0, 368.0, 220.0), graph),
    ])
}

fn gauge(top: f32) -> Option<Vec<(BoxF, ElementKind)>> {
    let at = |x, y, w, h| BoxF::new(x, y, w, h);
    let dial = ElementKind::Ring {
        binding: binding(keys::CPU_TEMPERATURE, 100.0)?,
        start_angle: -120.0,
        sweep: 240.0,
        thickness: 12.0,
        clockwise: true,
        fill: linear(0.0, Rgba::opaque(80, 230, 140), Rgba::opaque(255, 90, 90)),
        track: Some(Paint::solid(TRACK)),
        cap: Cap::Butt,
        segments: Some(Segments {
            count: 24,
            gap: 2.5,
        }),
    };
    let needle = ElementKind::Needle {
        binding: binding(keys::CPU_TEMPERATURE, 100.0)?,
        asset: None,
        pivot: (0.5, 0.5),
        start_angle: -120.0,
        sweep: 240.0,
        color: Rgba::WHITE,
        width: 5.0,
    };
    let hub = ElementKind::Shape {
        shape: ShapeKind::Ellipse,
        fill: Some(Paint::solid(Rgba::opaque(24, 28, 44))),
        stroke: Some((Rgba::WHITE, 3.0)),
    };
    let value = style(40.0, 700, Paint::solid(Rgba::WHITE), HAlign::Center);
    let caption = spaced(style(20.0, 600, Paint::solid(MUTED), HAlign::Center), 3.0);
    Some(vec![
        card(top, 360.0),
        (at(90.0, top + 40.0, 300.0, 300.0), dial),
        (at(130.0, top + 80.0, 220.0, 220.0), needle),
        (at(226.0, top + 176.0, 28.0, 28.0), hub),
        (
            at(90.0, top + 245.0, 300.0, 56.0),
            text(sensor(keys::CPU_TEMPERATURE)?, value),
        ),
        (
            at(90.0, top + 300.0, 300.0, 30.0),
            text(label("CPU TEMP"), caption),
        ),
    ])
}

/// The showcase theme (`None` only if a literal sensor key were invalid).
pub fn theme() -> Option<Theme> {
    let mut theme = Theme::blank("Showcase", Size::new(480, 1920), Orientation::Portrait);
    theme.background = Background::Color(Rgba::opaque(6, 7, 12));
    let parts = [
        header(),
        cpu(396.0)?,
        gpu(804.0)?,
        ram(1082.0)?,
        gauge(1440.0)?,
    ];
    theme.elements = parts
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, (frame, kind))| Element {
            id: ElementId(i as u32 + 1),
            name: format!("element {}", i + 1),
            frame,
            opacity: 1.0,
            visible: true,
            locked: false,
            kind,
        })
        .collect();
    Some(theme)
}

/// A 64x64 PNG chip icon: a rounded square with a soft cyan-violet glow.
fn icon() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(64, 64, |x, y| {
        let (dx, dy) = (x as f32 - 31.5, y as f32 - 31.5);
        let d = dx.abs().max(dy.abs());
        let t = x as f32 / 63.0;
        let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - t) + f32::from(b) * t) as u8;
        let alpha = if d < 22.0 {
            255.0
        } else {
            (255.0 * (1.0 - (d - 22.0) / 6.0)).clamp(0.0, 255.0)
        };
        let hole = if d < 12.0 { 0.35 } else { 1.0 };
        image::Rgba([
            mix(CYAN.r, VIOLET.r),
            mix(CYAN.g, VIOLET.g),
            mix(CYAN.b, VIOLET.b),
            (alpha * hole) as u8,
        ])
    });
    let mut out = Cursor::new(Vec::new());
    match image.write_to(&mut out, image::ImageFormat::Png) {
        Ok(()) => out.into_inner(),
        Err(_) => Vec::new(),
    }
}

/// Fixed readings, histories and assets for the showcase.
pub struct Scene {
    /// Current readings.
    pub snapshot: Snapshot,
    /// Graph histories.
    pub histories: Histories,
    /// Asset bytes.
    pub assets: BTreeMap<AssetRef, Vec<u8>>,
}

impl Scene {
    /// The fixed scene.
    pub fn new(theme: &Theme) -> Option<Self> {
        let gib = 1024.0 * 1024.0 * 1024.0;
        let mut snapshot = Snapshot::default();
        for (k, v) in [
            (keys::CPU_USAGE, 63.0),
            (keys::CPU_TEMPERATURE, 58.0),
            (keys::CPU_FREQUENCY, 4725.0),
            (keys::GPU_USAGE, 81.0),
            (keys::GPU_TEMPERATURE, 67.0),
            (keys::MEMORY_USED, 12.4 * gib),
            (keys::MEMORY_PERCENT, 39.0),
        ] {
            snapshot.insert(key(k)?, Reading::Value(v));
        }
        let mut histories = Histories::new(&theme.history_lengths());
        for i in 0..60 {
            let t = f64::from(i);
            let v = 34.0 + 9.0 * (t / 7.0).sin() + 4.0 * (t / 2.3).cos() + t * 0.08;
            let mut s = Snapshot::default();
            s.insert(key(keys::MEMORY_PERCENT)?, Reading::Value(v));
            histories.push(&s);
        }
        let assets = BTreeMap::from([(AssetRef(ICON.into()), icon())]);
        Some(Self {
            snapshot,
            histories,
            assets,
        })
    }

    /// The render context at [`TIME`].
    pub fn context(&self) -> RenderContext<'_> {
        RenderContext {
            snapshot: &self.snapshot,
            histories: &self.histories,
            quantities: &NO_QUANTITIES,
            time: TIME,
            language: Language::English,
            backdrop: Backdrop::Poster,
        }
    }
}
