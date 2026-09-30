//! Test support: a renderer with only the bundled test font, scene builders,
//! in-memory PNG/GIF encoders and pixel probes.

use std::collections::BTreeMap;
use std::io::Cursor;

use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::history::Histories;
use bezel_core::domain::sensor::{Reading, SensorKey, Snapshot};
use bezel_core::domain::theme::{
    AssetRef, Background, BoxF, Element, ElementId, ElementKind, Theme,
};
use bezel_core::ports::{FrameRenderer, RenderContext};
use image::codecs::gif::GifEncoder;
use image::{Delay, ImageFormat, RgbaImage};

use crate::{SkiaRenderer, SystemFonts};

/// JetBrains Mono NL Regular (SIL OFL 1.1, see `tests/fonts/OFL.txt`).
pub(crate) const FONT: &[u8] = include_bytes!("../tests/fonts/JetBrainsMonoNL-Regular.ttf");

/// Family name of [`FONT`].
pub(crate) const FAMILY: &str = "JetBrains Mono NL";

/// Wednesday 2026-09-30 21:05:00.
pub(crate) const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 0,
    weekday: 2,
};

/// A renderer that only knows the bundled test font.
pub(crate) fn renderer() -> SkiaRenderer {
    SkiaRenderer::with_fonts(vec![FONT.to_vec()], SystemFonts::Skip)
}

/// A sensor key.
pub(crate) fn key(k: &str) -> SensorKey {
    SensorKey::new(k).expect("valid key")
}

/// A visible, opaque element.
pub(crate) fn element(frame: BoxF, kind: ElementKind) -> Element {
    Element {
        id: ElementId(1),
        name: "e".into(),
        frame,
        opacity: 1.0,
        visible: true,
        locked: false,
        kind,
    }
}

/// A `w`x`h` theme.
pub(crate) fn theme(w: u32, h: u32, background: Background, elements: Vec<Element>) -> Theme {
    let mut theme = Theme::blank("golden", Size::new(w, h).portrait(), Orientation::Portrait);
    theme.canvas = Size::new(w, h);
    theme.background = background;
    theme.elements = elements;
    theme
}

/// Everything a frame reads besides the theme.
pub(crate) struct Scene {
    pub snapshot: Snapshot,
    pub histories: Histories,
    pub assets: BTreeMap<AssetRef, Vec<u8>>,
    pub time: LocalTime,
}

impl Scene {
    /// No readings, no assets, [`TIME`].
    pub fn empty() -> Self {
        Self {
            snapshot: Snapshot::default(),
            histories: Histories::default(),
            assets: BTreeMap::new(),
            time: TIME,
        }
    }

    /// With `key` reading `value`.
    pub fn with(mut self, k: &str, value: f64) -> Self {
        self.snapshot.insert(key(k), Reading::Value(value));
        self
    }

    /// With the asset `name` holding `bytes`.
    pub fn asset(mut self, name: &str, bytes: Vec<u8>) -> Self {
        self.assets.insert(AssetRef(name.into()), bytes);
        self
    }

    /// With the history of `k` (oldest first, `None` = gap).
    pub fn history(mut self, k: &str, values: &[Option<f64>]) -> Self {
        let mut histories = Histories::new(&[(key(k), values.len())]);
        for v in values {
            let mut s = Snapshot::default();
            if let Some(v) = v {
                s.insert(key(k), Reading::Value(*v));
            }
            histories.push(&s);
        }
        self.histories = histories;
        self
    }
}

/// Renders `theme` in `scene`.
pub(crate) fn render(renderer: &mut SkiaRenderer, theme: &Theme, scene: &Scene) -> Frame {
    let context = RenderContext {
        snapshot: &scene.snapshot,
        histories: &scene.histories,
        time: scene.time,
        language: Language::English,
    };
    renderer
        .render(theme, &scene.assets, context)
        .expect("render")
}

/// Encodes a `w`x`h` PNG whose pixels come from `color(x, y)`.
pub(crate) fn png(w: u32, h: u32, color: impl Fn(u32, u32) -> Rgba) -> Vec<u8> {
    let image = RgbaImage::from_fn(w, h, |x, y| {
        let c = color(x, y);
        image::Rgba([c.r, c.g, c.b, c.a])
    });
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, ImageFormat::Png).expect("png");
    out.into_inner()
}

/// Encodes a `w`x`h` GIF with one solid frame per `(color, delay ms)`.
pub(crate) fn gif(w: u32, h: u32, frames: &[(Rgba, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut out);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Infinite)
            .expect("repeat");
        for (c, ms) in frames {
            let buffer = RgbaImage::from_pixel(w, h, image::Rgba([c.r, c.g, c.b, c.a]));
            let frame = image::Frame::from_parts(buffer, 0, 0, Delay::from_numer_denom_ms(*ms, 1));
            encoder.encode_frame(frame).expect("gif frame");
        }
    }
    out
}

/// The pixel at `(x, y)`.
pub(crate) fn px(frame: &Frame, x: u32, y: u32) -> Rgba {
    frame.pixel(x, y).expect("inside the frame")
}

/// True when every channel of `a` is within `tolerance` of `b`.
pub(crate) fn close(a: Rgba, b: Rgba, tolerance: u8) -> bool {
    let d = |x: u8, y: u8| x.abs_diff(y) <= tolerance;
    d(a.r, b.r) && d(a.g, b.g) && d(a.b, b.b) && d(a.a, b.a)
}

/// Asserts the pixel at `(x, y)` is `expected` within `tolerance`.
#[track_caller]
pub(crate) fn assert_px(frame: &Frame, x: u32, y: u32, expected: Rgba, tolerance: u8) {
    let got = px(frame, x, y);
    assert!(
        close(got, expected, tolerance),
        "pixel ({x}, {y}) is {got:?}, expected {expected:?} ±{tolerance}"
    );
}

/// Pixels in `[x0, x1) x [y0, y1)` matching `pred`.
pub(crate) fn count(
    frame: &Frame,
    (x0, y0, x1, y1): (u32, u32, u32, u32),
    pred: impl Fn(Rgba) -> bool,
) -> usize {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| pred(px(frame, x, y)))
        .count()
}

/// Bounds `(left, top, right, bottom)` inclusive of the pixels that differ
/// from `background` by more than `tolerance`.
pub(crate) fn ink(frame: &Frame, background: Rgba, tolerance: u8) -> Option<(u32, u32, u32, u32)> {
    let size = frame.size();
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..size.height {
        for x in 0..size.width {
            if close(px(frame, x, y), background, tolerance) {
                continue;
            }
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
            });
        }
    }
    bounds
}
