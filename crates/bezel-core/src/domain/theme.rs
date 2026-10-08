//! Themes: what a screen shows. A theme is a canvas (the panel in one
//! orientation) with a background and a z-ordered list of elements; each
//! element has a box and a kind (text, image, bar, ring, needle, graph, …)
//! and may be bound to a sensor.
//!
//! The model is plain data so the editor can change it freely and every
//! adapter (renderer, file formats, importers) reads the same thing.

use std::collections::BTreeSet;
use std::time::Duration;

use super::clock::{ClockCase, Language};
use super::frame::Rgba;
use super::framing::VideoFraming;
use super::geometry::{Orientation, Size};
use super::sensor::{DisplayFormat, SensorKey};

/// Identifies an element inside its theme (stable across edits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId(pub u32);

/// Identifies an asset (image, font, video) bundled with the theme, e.g.
/// `assets/background.png`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetRef(pub String);

/// An element's box in canvas pixels. May extend past the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxF {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl BoxF {
    /// Builds a box.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Center point.
    pub fn center(self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// A color or a linear gradient.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// One color.
    Solid(Rgba),
    /// Gradient along a ring scale, independent of its current value.
    Arc {
        /// Color at zero.
        start: Rgba,
        /// Color at full scale.
        end: Rgba,
        /// Position of the half-color blend, 0..=1.
        transition: f32,
    },
    /// A linear gradient across the element's box.
    Linear {
        /// Direction in degrees: 0 = left→right, 90 = top→bottom.
        angle: f32,
        /// Color stops, positions 0..=1 in order.
        stops: Vec<(f32, Rgba)>,
    },
}

impl Paint {
    /// A solid paint.
    pub const fn solid(color: Rgba) -> Self {
        Paint::Solid(color)
    }
}

/// How an image fills its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Stretch to the box.
    #[default]
    Fill,
    /// Scale to fit inside, keeping proportions.
    Contain,
    /// Scale to cover, keeping proportions, cropping the overflow.
    Cover,
    /// Original size, anchored top-left.
    None,
}

/// What the canvas shows under the elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    /// Video already stored on the screen, played without host decoding.
    DeviceVideo {
        /// Exact internal/SD video path.
        path: super::storage::RemotePath,
        /// Loop or play once.
        repeat: super::storage::Repeat,
        /// Color outside video windows (ignored when there are no windows).
        color: Rgba,
    },
    /// A solid color.
    Color(Rgba),
    /// A still image.
    Image {
        /// Image asset.
        asset: AssetRef,
        /// How it fills the canvas.
        fit: Fit,
    },
    /// A video: played by the screen itself when it can (the theme is then
    /// an overlay), otherwise decoded and streamed by the host.
    Video {
        /// Video asset.
        asset: AssetRef,
        /// Frame shown in previews and when playback is unavailable.
        poster: Option<AssetRef>,
        /// How the video is turned, fitted, zoomed and placed on the canvas;
        /// `None` is [`VideoFraming::default`] (Auto rotation, cover,
        /// centered; D-2026-10-01-video-background-framing-2).
        framing: Option<VideoFraming>,
    },
}

/// A font face by family name, as installed or bundled.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontSpec {
    /// Family name, e.g. `Inter`, or a bundled asset.
    pub family: String,
    /// CSS-like weight, 100..=900.
    pub weight: u16,
    /// Italic.
    pub italic: bool,
    /// A bundled font file, preferred over the system family when present.
    pub asset: Option<AssetRef>,
}

impl Default for FontSpec {
    fn default() -> Self {
        Self {
            family: "Inter".to_string(),
            weight: 400,
            italic: false,
            asset: None,
        }
    }
}

/// Horizontal text alignment inside the element box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    /// Left.
    #[default]
    Left,
    /// Center.
    Center,
    /// Right.
    Right,
}

/// Vertical text alignment inside the element box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    /// Top.
    #[default]
    Top,
    /// Middle.
    Middle,
    /// Bottom.
    Bottom,
}

/// How text looks.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Face.
    pub font: FontSpec,
    /// Size in pixels.
    pub size: f32,
    /// Fill.
    pub paint: Paint,
    /// Horizontal alignment.
    pub align: HAlign,
    /// Vertical alignment.
    pub valign: VAlign,
    /// Extra space between characters, pixels.
    pub letter_spacing: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: FontSpec::default(),
            size: 24.0,
            paint: Paint::solid(Rgba::WHITE),
            align: HAlign::Left,
            valign: VAlign::Top,
            letter_spacing: 0.0,
        }
    }
}

/// A sensor binding with the range that maps it to 0..=1 for bars, rings,
/// needles and graphs.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The sensor.
    pub key: SensorKey,
    /// Value shown as empty.
    pub min: f64,
    /// Value shown as full.
    pub max: f64,
}

/// What a text element prints.
#[derive(Debug, Clone, PartialEq)]
pub enum TextContent {
    /// Current weather at a saved location.
    Weather(super::weather::Weather),
    /// Fixed text.
    Static(String),
    /// A sensor value, formatted, with optional text around it.
    Sensor {
        /// The sensor.
        key: SensorKey,
        /// Number formatting.
        format: DisplayFormat,
        /// Text before the value.
        prefix: String,
        /// Text after the value.
        suffix: String,
    },
    /// The current date/time rendered with a pattern (see [`super::clock`]).
    Clock {
        /// Pattern, e.g. `%H:%M`.
        pattern: String,
        /// None follows the runtime system language.
        language: Option<Language>,
        /// Letter case of the resulting text.
        casing: ClockCase,
    },
}

/// Line ends of rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cap {
    /// Flat.
    #[default]
    Butt,
    /// Rounded.
    Round,
}

/// Splits a bar or ring into blocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segments {
    /// Number of blocks.
    pub count: u16,
    /// Gap between blocks, pixels for bars, degrees for rings.
    pub gap: f32,
}

/// Direction a bar fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// From the left edge.
    #[default]
    LeftToRight,
    /// From the right edge.
    RightToLeft,
    /// From the bottom edge.
    BottomToTop,
    /// From the top edge.
    TopToBottom,
}

/// How a graph draws its history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphStyle {
    /// A line.
    #[default]
    Line,
    /// A line with the area under it filled.
    Area,
    /// One bar per sample.
    Bars,
}

/// Simple shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeKind {
    /// Rectangle with rounded corners (radius 0 = square corners).
    Rect {
        /// Corner radius, pixels.
        radius: f32,
    },
    /// Ellipse inscribed in the box.
    Ellipse,
}

/// The different element kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    /// Text: static, sensor value or clock.
    Text {
        /// What to print.
        content: TextContent,
        /// How it looks.
        style: TextStyle,
    },
    /// A still or animated (GIF) image.
    Image {
        /// Image asset.
        asset: AssetRef,
        /// How it fills the box.
        fit: Fit,
    },
    /// A filled shape.
    Shape {
        /// Clear this shape to reveal the device video underneath.
        video_window: bool,
        /// Linear opacity across the whole shape, including its outline.
        fade: Option<super::gradient::Fade>,
        /// Geometry.
        shape: ShapeKind,
        /// Fill (none = outline only).
        fill: Option<Paint>,
        /// Outline color and width.
        stroke: Option<(Rgba, f32)>,
    },
    /// A progress bar.
    Bar {
        /// Value and range.
        binding: Binding,
        /// Fill direction.
        direction: Direction,
        /// Filled part.
        fill: Paint,
        /// Empty track (none = transparent).
        track: Option<Paint>,
        /// Corner radius.
        radius: f32,
        /// Optional blocks.
        segments: Option<Segments>,
    },
    /// A ring/arc gauge inscribed in the box.
    Ring {
        /// Temporarily show the full scale to inspect its appearance.
        test_full: bool,
        /// Value and range.
        binding: Binding,
        /// Start angle in degrees, 0 = 12 o'clock, clockwise positive.
        start_angle: f32,
        /// Total sweep in degrees (360 = full circle).
        sweep: f32,
        /// Ring thickness in pixels.
        thickness: f32,
        /// Fill clockwise from the start angle.
        clockwise: bool,
        /// Filled arc.
        fill: Paint,
        /// Track under it (none = transparent).
        track: Option<Paint>,
        /// Line ends.
        cap: Cap,
        /// Optional blocks.
        segments: Option<Segments>,
    },
    /// A rotating needle (image or drawn line) for analog gauges.
    Needle {
        /// Value and range.
        binding: Binding,
        /// Needle image pointing to 12 o'clock at rest; none = a drawn line.
        asset: Option<AssetRef>,
        /// Pivot inside the box, 0..=1 of width/height.
        pivot: (f32, f32),
        /// Angle at `min`, degrees, 0 = 12 o'clock, clockwise positive.
        start_angle: f32,
        /// Angle travelled from `min` to `max`.
        sweep: f32,
        /// Drawn-line color and width when there is no image.
        color: Rgba,
        /// Drawn-line width.
        width: f32,
    },
    /// A history graph.
    Graph {
        /// Value and range.
        binding: Binding,
        /// Samples kept (one per refresh).
        history: u16,
        /// Drawing style.
        style: GraphStyle,
        /// Line and bar color.
        color: Rgba,
        /// Area fill.
        fill: Option<Paint>,
        /// Line width.
        line_width: f32,
        /// Scale to the visible samples instead of the binding range.
        autoscale: bool,
    },
}

/// One thing drawn on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// Optional multi-face container configuration.
    pub card: Option<Card>,
    /// Container and face owning this object; none means an independent object.
    pub card_member: Option<CardMember>,
    /// Stable id.
    pub id: ElementId,
    /// Name shown in the layer list.
    pub name: String,
    /// Box in canvas pixels.
    pub frame: BoxF,
    /// 0..=1.
    pub opacity: f32,
    /// Hidden elements are neither drawn nor hit-tested.
    pub visible: bool,
    /// Locked elements cannot be moved in the editor.
    pub locked: bool,
    /// What it is.
    pub kind: ElementKind,
}

/// A card base is drawn as a normal shape; face objects keep stable theme IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    /// Face names, in presentation order.
    pub faces: Vec<String>,
    /// Face shown by the renderer.
    pub active_face: usize,
    /// Optional face transition; absent means an immediate switch.
    pub transition: Option<CardTransition>,
}

/// Face transition style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardEffect {
    /// Immediate switch.
    None,
    /// Crossfade.
    Fade,
    /// Sliding faces.
    Slide,
    /// Turn around the center axis.
    Flip,
}
/// Direction of a slide or flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardDirection {
    /// Left.
    Left,
    /// Right.
    Right,
    /// Up.
    Up,
    /// Down.
    Down,
}
/// Saved card animation settings (animation progress is never saved).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardTransition {
    /// Effect.
    pub effect: CardEffect,
    /// Movement direction.
    pub direction: CardDirection,
    /// Duration in milliseconds, 150 to 3000.
    pub duration_ms: u32,
    /// Move the common base with the face.
    pub include_base: bool,
}

/// Membership in a card shared base or one face.
#[derive(Debug, Clone, PartialEq)]
pub struct CardMember {
    /// Card element ID.
    pub parent: ElementId,
    /// None belongs to the shared base, otherwise a face index.
    pub face: Option<usize>,
}

/// A complete theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Display name.
    pub name: String,
    /// Canvas size = panel size in `orientation`.
    pub canvas: Size,
    /// How the user looks at the screen.
    pub orientation: Orientation,
    /// Under everything.
    pub background: Background,
    /// Bottom to top.
    pub elements: Vec<Element>,
    /// Seconds between refreshes (sensor sampling and frames).
    pub refresh_seconds: f32,
}

/// Fastest refresh, in seconds: a theme (or a watch of the sensors) never
/// samples and draws more often than this.
pub const MIN_REFRESH_SECONDS: f32 = 0.25;

/// Time between two refreshes of a theme asking for `refresh_seconds`: at
/// least [`MIN_REFRESH_SECONDS`], at most `slowest_seconds` (the driving
/// adapter's limit), one second when it asks for no number.
pub fn refresh_interval(refresh_seconds: f32, slowest_seconds: f32) -> Duration {
    let seconds = if refresh_seconds.is_finite() {
        refresh_seconds
            .min(slowest_seconds)
            .max(MIN_REFRESH_SECONDS)
    } else {
        1.0
    };
    Duration::from_secs_f32(seconds)
}

impl Theme {
    /// An empty theme for a panel whose portrait size is `panel`.
    pub fn blank(name: &str, panel: Size, orientation: Orientation) -> Self {
        Self {
            name: name.to_string(),
            canvas: panel.portrait().in_orientation(orientation),
            orientation,
            background: Background::Color(Rgba::opaque(12, 14, 22)),
            elements: Vec::new(),
            refresh_seconds: 1.0,
        }
    }

    /// Whether the theme fits a panel whose portrait size is `panel`: `None`
    /// when its canvas is that panel turned the theme's way up, else that
    /// size (the canvas a theme for this panel would have).
    pub fn misfit(&self, panel: Size) -> Option<Size> {
        let canvas = panel.in_orientation(self.orientation);
        (canvas != self.canvas).then_some(canvas)
    }

    /// The element with `id`.
    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Visibility includes the owning card and its active face.
    pub fn is_visible(&self, element: &Element) -> bool {
        if !element.visible {
            return false;
        }
        let Some(member) = &element.card_member else {
            return true;
        };
        self.element(member.parent).is_some_and(|parent| {
            parent.visible
                && parent
                    .card
                    .as_ref()
                    .is_some_and(|card| member.face.is_none_or(|face| face == card.active_face))
        })
    }

    /// Every asset the theme references (for packaging and loading).
    pub fn assets(&self) -> Vec<AssetRef> {
        let mut out = Vec::new();
        match &self.background {
            Background::Image { asset, .. } => out.push(asset.clone()),
            Background::Video { asset, poster, .. } => {
                out.push(asset.clone());
                out.extend(poster.clone());
            }
            Background::Color(_) | Background::DeviceVideo { .. } => {}
        }
        for e in &self.elements {
            match &e.kind {
                ElementKind::Image { asset, .. } => out.push(asset.clone()),
                ElementKind::Needle { asset: Some(a), .. } => out.push(a.clone()),
                ElementKind::Text { style, .. } => out.extend(style.font.asset.clone()),
                _ => {}
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn is_sampled(&self, element: &Element) -> bool {
        self.is_visible(element)
            || (element.visible
                && element.card_member.as_ref().is_some_and(|m| {
                    self.element(m.parent).is_some_and(|parent| {
                        parent.visible
                            && parent.card.as_ref().is_some_and(|card| {
                                card.transition
                                    .is_some_and(|t| t.effect != CardEffect::None)
                            })
                    })
                }))
    }

    /// The sensors the visible elements show: what running this theme
    /// wants measured (hidden elements are not drawn).
    pub fn sensor_keys(&self) -> BTreeSet<SensorKey> {
        let mut keys: BTreeSet<SensorKey> = self
            .elements
            .iter()
            .filter(|e| self.is_sampled(e))
            .filter_map(|e| match &e.kind {
                ElementKind::Text {
                    content: TextContent::Sensor { key, .. },
                    ..
                } => Some(key.clone()),
                ElementKind::Bar { binding, .. }
                | ElementKind::Ring { binding, .. }
                | ElementKind::Needle { binding, .. }
                | ElementKind::Graph { binding, .. } => Some(binding.key.clone()),
                _ => None,
            })
            .collect();
        for e in self.elements.iter().filter(|e| self.is_sampled(e)) {
            if let ElementKind::Text {
                content: TextContent::Weather(weather),
                ..
            } = &e.kind
            {
                keys.extend(weather.keys());
            }
        }
        keys
    }

    /// The images of the image elements a frame shows: visible, not fully
    /// transparent, with a box that reaches into the canvas. An animated
    /// one among them sets the pace of the frames
    /// ([`super::animation`]); hidden ones do not animate.
    pub fn shown_images(&self) -> impl Iterator<Item = &AssetRef> {
        let (width, height) = (self.canvas.width as f32, self.canvas.height as f32);
        self.elements.iter().filter_map(move |e| {
            let ElementKind::Image { asset, .. } = &e.kind else {
                return None;
            };
            let b = e.frame;
            let on_canvas = b.width > 0.0
                && b.height > 0.0
                && b.x < width
                && b.y < height
                && b.x + b.width > 0.0
                && b.y + b.height > 0.0;
            (self.is_visible(e) && e.opacity > 0.0 && on_canvas).then_some(asset)
        })
    }

    /// The graph history length wanted for each bound sensor.
    pub fn history_lengths(&self) -> Vec<(SensorKey, usize)> {
        self.elements
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::Graph {
                    binding, history, ..
                } => Some((binding.key.clone(), usize::from(*history))),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sensor::keys;

    fn key(k: &str) -> SensorKey {
        SensorKey::new(k).expect("valid key")
    }

    fn element(id: u32, kind: ElementKind) -> Element {
        Element {
            card: None,
            card_member: None,
            id: ElementId(id),
            name: format!("e{id}"),
            frame: BoxF::new(0.0, 0.0, 100.0, 40.0),
            opacity: 1.0,
            visible: true,
            locked: false,
            kind,
        }
    }

    fn binding(k: &str) -> Binding {
        Binding {
            key: key(k),
            min: 0.0,
            max: 100.0,
        }
    }

    #[test]
    fn blank_theme_takes_the_orientation() {
        let t = Theme::blank("x", Size::new(480, 1920), Orientation::Landscape);
        assert_eq!(t.canvas, Size::new(1920, 480));
    }

    #[test]
    fn keys_assets_and_histories_are_collected() {
        let mut t = Theme::blank("x", Size::new(480, 1920), Orientation::Portrait);
        t.background = Background::Video {
            asset: AssetRef("assets/bg.mp4".into()),
            poster: Some(AssetRef("assets/bg.png".into())),
            framing: None,
        };
        let mut style = TextStyle::default();
        style.font.asset = Some(AssetRef("assets/font.ttf".into()));
        t.elements = vec![
            element(
                1,
                ElementKind::Text {
                    content: TextContent::Sensor {
                        key: key(keys::CPU_TEMPERATURE),
                        format: DisplayFormat::default(),
                        prefix: String::new(),
                        suffix: String::new(),
                    },
                    style,
                },
            ),
            element(
                2,
                ElementKind::Graph {
                    binding: binding(keys::CPU_USAGE),
                    history: 60,
                    style: GraphStyle::Area,
                    color: Rgba::WHITE,
                    fill: None,
                    line_width: 2.0,
                    autoscale: false,
                },
            ),
            element(
                3,
                ElementKind::Image {
                    asset: AssetRef("assets/logo.png".into()),
                    fit: Fit::Contain,
                },
            ),
            element(
                4,
                ElementKind::Needle {
                    binding: binding(keys::CPU_USAGE),
                    asset: Some(AssetRef("assets/needle.png".into())),
                    pivot: (0.5, 0.9),
                    start_angle: -120.0,
                    sweep: 240.0,
                    color: Rgba::WHITE,
                    width: 3.0,
                },
            ),
        ];
        let mut hidden = element(
            5,
            ElementKind::Bar {
                binding: binding(keys::GPU_USAGE),
                direction: Direction::LeftToRight,
                fill: Paint::solid(Rgba::WHITE),
                track: None,
                radius: 4.0,
                segments: None,
            },
        );
        hidden.visible = false;
        t.elements.push(hidden);

        let assets: Vec<String> = t.assets().into_iter().map(|a| a.0).collect();
        assert_eq!(
            assets,
            vec![
                "assets/bg.mp4",
                "assets/bg.png",
                "assets/font.ttf",
                "assets/logo.png",
                "assets/needle.png"
            ]
        );
        assert_eq!(t.history_lengths(), vec![(key(keys::CPU_USAGE), 60)]);
        assert_eq!(
            t.sensor_keys(),
            BTreeSet::from([key(keys::CPU_TEMPERATURE), key(keys::CPU_USAGE)]),
            "the hidden bar's sensor is not wanted"
        );
        assert!(t.element(ElementId(3)).is_some());
    }

    #[test]
    fn only_images_a_frame_shows_are_shown_images() {
        let mut t = Theme::blank("x", Size::new(480, 1920), Orientation::Portrait);
        let image = |id, name: &str, frame| Element {
            frame,
            ..element(
                id,
                ElementKind::Image {
                    asset: AssetRef(name.into()),
                    fit: Fit::Fill,
                },
            )
        };
        let mut hidden = image(2, "hidden.gif", BoxF::new(0.0, 0.0, 10.0, 10.0));
        hidden.visible = false;
        let mut clear = image(3, "clear.gif", BoxF::new(0.0, 0.0, 10.0, 10.0));
        clear.opacity = 0.0;
        t.elements = vec![
            image(1, "shown.gif", BoxF::new(470.0, 1910.0, 64.0, 64.0)),
            hidden,
            clear,
            image(4, "outside.gif", BoxF::new(480.0, 0.0, 10.0, 10.0)),
            image(5, "above.gif", BoxF::new(0.0, -10.0, 10.0, 10.0)),
            image(6, "empty.gif", BoxF::new(5.0, 5.0, 0.0, 10.0)),
            element(
                7,
                ElementKind::Text {
                    content: TextContent::Static("x".into()),
                    style: TextStyle::default(),
                },
            ),
        ];
        let shown: Vec<&str> = t.shown_images().map(|a| a.0.as_str()).collect();
        assert_eq!(shown, ["shown.gif"]);
    }

    #[test]
    fn box_math() {
        let b = BoxF::new(15.0, 15.0, 100.0, 50.0);
        assert_eq!(b.center(), (65.0, 40.0));
        let bg = Theme::blank("x", Size::new(80, 160), Orientation::Portrait);
        assert!(bg.assets().is_empty());
    }

    #[test]
    fn a_theme_fits_its_panel_turned_its_way_up() {
        let panel = Size::new(480, 1920);
        let landscape = Theme::blank("x", panel, Orientation::Landscape);
        assert_eq!(landscape.misfit(panel), None);
        let upside_down = Theme::blank("x", panel, Orientation::ReversePortrait);
        assert_eq!(upside_down.misfit(panel), None);
        assert_eq!(
            landscape.misfit(Size::new(320, 480)),
            Some(Size::new(480, 320))
        );
        let small = Theme::blank("x", Size::new(320, 480), Orientation::Portrait);
        assert_eq!(small.misfit(panel), Some(panel));
    }

    #[test]
    fn refreshes_stay_between_the_fastest_and_the_slowest() {
        assert_eq!(refresh_interval(1.0, 60.0), Duration::from_secs(1));
        assert_eq!(refresh_interval(0.01, 60.0), Duration::from_millis(250));
        assert_eq!(refresh_interval(3600.0, 60.0), Duration::from_secs(60));
        assert_eq!(refresh_interval(5.0, 2.0), Duration::from_secs(2));
        assert_eq!(refresh_interval(f32::NAN, 2.0), Duration::from_secs(1));
        assert_eq!(
            refresh_interval(f32::INFINITY, 60.0),
            Duration::from_secs(1)
        );
        assert_eq!(refresh_interval(1.0, 0.1), Duration::from_millis(250));
    }
}
