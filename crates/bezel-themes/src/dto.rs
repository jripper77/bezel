//! `theme.json`, schema 1: serde shapes and their mapping to the core model.

use bezel_core::domain::clock::{ClockCase, Language};
use bezel_core::domain::frame::Rgba;
use bezel_core::domain::framing::{FramingPosition, Permille, VideoFit, VideoFraming, Zoom};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::sensor::{ByteUnits, DisplayFormat, SensorKey, TemperatureUnit};
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Cap, Direction, Element, ElementId, ElementKind, Fit,
    FontSpec, GraphStyle, HAlign, Paint, Segments, ShapeKind, TextContent, TextStyle, Theme,
    VAlign,
};
use serde::{Deserialize, Serialize};

use crate::color::{from_hex, to_hex};

/// The schema this build writes and the newest it reads.
pub const SCHEMA: u32 = 1;

/// Why a `theme.json` could not become a theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DtoError(pub String);

type R<T> = std::result::Result<T, DtoError>;

fn err<T>(msg: impl Into<String>) -> R<T> {
    Err(DtoError(msg.into()))
}

fn color(s: &str) -> R<Rgba> {
    from_hex(s).ok_or_else(|| DtoError(format!("bad color {s:?}")))
}

fn key(s: &str) -> R<SensorKey> {
    SensorKey::new(s).ok_or_else(|| DtoError(format!("bad sensor key {s:?}")))
}

/// Root object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDto {
    /// Schema version.
    pub schema: u32,
    /// Display name.
    pub name: String,
    /// Canvas size.
    pub canvas: SizeDto,
    /// `portrait`, `reverse-portrait`, `landscape`, `reverse-landscape`.
    pub orientation: String,
    /// Seconds between refreshes.
    #[serde(default = "one")]
    pub refresh_seconds: f32,
    /// Background.
    pub background: BackgroundDto,
    /// Elements, bottom to top.
    #[serde(default)]
    pub elements: Vec<ElementDto>,
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

fn one_f64() -> f64 {
    1.0
}

fn half() -> f64 {
    0.5
}

fn black() -> String {
    to_hex(Rgba::BLACK)
}

/// Width and height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SizeDto {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// Background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BackgroundDto {
    /// Video already stored on the screen.
    #[serde(rename_all = "camelCase")]
    DeviceVideo {
        /// Exact internal/SD video path.
        path: String,
        /// Replay at the end.
        #[serde(default = "yes")]
        looping: bool,
        /// Color outside video windows.
        #[serde(default = "black")]
        color: String,
    },
    /// A color.
    Color {
        /// `#rrggbbaa`.
        color: String,
    },
    /// An image.
    Image {
        /// Asset path.
        asset: String,
        /// Fit.
        #[serde(default)]
        fit: FitDto,
    },
    /// A video.
    Video {
        /// Asset path.
        asset: String,
        /// Poster asset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        poster: Option<String>,
        /// How the video is framed; omitted when it is the default, so a
        /// theme without a framing is written as before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        framing: Option<FramingDto>,
    },
}

/// A video background's framing (D-2026-10-01-video-background-framing-2).
/// A missing key takes its default; numbers out of range are clamped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FramingDto {
    /// Clockwise degrees: 0, 90, 180 or 270; absent is Auto. Any number is
    /// read, so another one is refused by name rather than by type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<serde_json::Number>,
    /// Fill or fit.
    #[serde(default)]
    pub fit: VideoFitDto,
    /// Zoom on top of the fit's scale, 1.0 to 4.0.
    #[serde(default = "one_f64")]
    pub zoom: f64,
    /// Where the picture sits, 0..=1 on each axis (CSS `object-position`).
    #[serde(default)]
    pub position: PositionDto,
    /// `#rrggbbaa` of the area a fitted picture leaves.
    #[serde(default = "black")]
    pub pad_color: String,
}

impl FramingDto {
    /// Clockwise quarter turns of its `rotation` (`None`: Auto). Only 0,
    /// 90, 180 and 270 degrees are rotations; another number is refused,
    /// for every reader of a theme (its file, the studio's commands).
    pub fn quarter_turns(&self) -> Result<Option<u8>, DtoError> {
        self.rotation.as_ref().map(quarter_turns).transpose()
    }
}

/// Video fit names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VideoFitDto {
    /// Fill: covers the canvas, the overflow cut off.
    #[default]
    Cover,
    /// Fit: inside the canvas, the rest painted with the pad color.
    Contain,
}

/// A place on each axis, 0 = start, 0.5 = middle, 1 = end.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PositionDto {
    /// Across.
    #[serde(default = "half")]
    pub x: f64,
    /// Down.
    #[serde(default = "half")]
    pub y: f64,
}

impl Default for PositionDto {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Fit names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitDto {
    /// Stretch.
    #[default]
    Fill,
    /// Inside.
    Contain,
    /// Cover.
    Cover,
    /// Original size.
    None,
}

/// A paint: a color string or a gradient object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PaintDto {
    /// `#rrggbbaa`.
    Solid(String),
    /// Gradient along a ring scale.
    Arc {
        /// Must be `arc`.
        scale: String,
        /// Color at zero.
        start: String,
        /// Color at full scale.
        end: String,
        /// Position of the half-color blend, 0..=100.
        transition: f32,
    },
    /// A linear gradient.
    Linear {
        /// Angle, degrees.
        angle: f32,
        /// `[position, color]` pairs.
        stops: Vec<(f32, String)>,
    },
}

/// A linear transparency mask in a shape's box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FadeDto {
    /// Radial center X/Y and radius in normalized box coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radial: Option<[f32; 3]>,
    /// Direction in degrees.
    pub angle: f32,
    /// Start opacity, 0..=1.
    pub start: f32,
    /// End opacity, 0..=1.
    pub end: f32,
}

/// One element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementDto {
    /// Ordinary group container; omitted in older themes.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_group: bool,
    /// Ordinary group owning this object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_parent: Option<u32>,
    /// Optional card container, absent in existing themes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<CardDto>,
    /// Optional membership of a shared base or face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_member: Option<CardMemberDto>,
    /// Id.
    pub id: u32,
    /// Name.
    pub name: String,
    /// Box.
    pub frame: BoxDto,
    /// Opacity 0..=1.
    #[serde(default = "one")]
    pub opacity: f32,
    /// Visible.
    #[serde(default = "yes")]
    pub visible: bool,
    /// Locked in the editor.
    #[serde(default)]
    pub locked: bool,
    /// Kind.
    pub kind: KindDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDto {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<CardTriggerDto>,
    pub faces: Vec<String>,
    #[serde(default)]
    pub active_face: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<CardTransitionDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardTriggerDto {
    pub source: String,
    #[serde(default)]
    pub app: String,
    pub face: usize,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub return_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardTransitionDto {
    pub effect: String,
    pub direction: String,
    pub duration_ms: u32,
    #[serde(default)]
    pub include_base: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardMemberDto {
    pub parent: u32,
    pub face: Option<usize>,
}

/// A box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxDto {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// Sensor binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingDto {
    /// Sensor key.
    pub key: String,
    /// Empty value.
    pub min: f64,
    /// Full value.
    pub max: f64,
}

/// Blocks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SegmentsDto {
    /// Number of blocks.
    pub count: u16,
    /// Gap.
    pub gap: f32,
}

/// Number formatting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatDto {
    /// Decimal places (none = automatic).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    /// Append the unit.
    #[serde(default = "yes")]
    pub show_unit: bool,
    /// Show °F instead of °C.
    #[serde(default)]
    pub fahrenheit: bool,
    /// Use 1000-based byte units.
    #[serde(default)]
    pub decimal_bytes: bool,
}

/// Text content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentDto {
    Player {
        #[serde(default)]
        source: String,
        #[serde(default = "yes", rename = "showCover")]
        show_cover: bool,
        #[serde(default = "yes", rename = "showProgress")]
        show_progress: bool,
        #[serde(default, rename = "showSource")]
        show_source: bool,
        #[serde(default, rename = "hideWhenStopped")]
        hide_when_stopped: bool,
        #[serde(default, rename = "emptyText")]
        empty_text: String,
    },
    /// Current weather at a saved location.
    Weather {
        /// Displayed city.
        city: String,
        /// Latitude.
        latitude: f64,
        /// Longitude.
        longitude: f64,
        /// Condition language; absent follows the system.
        #[serde(default)]
        language: Option<String>,
        /// Fahrenheit instead of Celsius.
        #[serde(default)]
        fahrenheit: bool,
        /// Show the vector condition icon.
        #[serde(default = "yes", rename = "showIcon")]
        show_icon: bool,
        /// Outline (default) or filled condition icon.
        #[serde(default = "outline_icon", rename = "iconStyle")]
        icon_style: String,
        /// Distance from icon to text, pixels; absent is automatic.
        #[serde(default, rename = "iconGap", skip_serializing_if = "Option::is_none")]
        icon_gap: Option<f32>,
        /// Icon side, pixels; absent follows text size.
        #[serde(default, rename = "iconSize", skip_serializing_if = "Option::is_none")]
        icon_size: Option<f32>,
    },
    /// Fixed text.
    Static {
        /// The text.
        text: String,
    },
    /// Sensor value.
    Sensor {
        /// Key.
        key: String,
        /// Formatting.
        format: FormatDto,
        /// Before the value.
        #[serde(default)]
        prefix: String,
        /// After the value.
        #[serde(default)]
        suffix: String,
    },
    /// Clock.
    Clock {
        /// Pattern.
        pattern: String,
        /// Explicit clock language; absent follows the system.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        /// Normal (absent), upper or title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        casing: Option<String>,
    },
}

/// Font face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontDto {
    /// Family.
    pub family: String,
    /// Weight.
    pub weight: u16,
    /// Italic.
    #[serde(default)]
    pub italic: bool,
    /// Bundled font asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

/// Text style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyleDto {
    /// Face.
    pub font: FontDto,
    /// Size, px.
    pub size: f32,
    /// Fill.
    pub paint: PaintDto,
    /// `left`, `center`, `right`.
    pub align: String,
    /// `top`, `middle`, `bottom`.
    pub valign: String,
    /// Extra letter spacing, px.
    #[serde(default)]
    pub letter_spacing: f32,
}

/// Element kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KindDto {
    /// Text.
    Text {
        /// Content.
        content: ContentDto,
        /// Style.
        style: TextStyleDto,
    },
    /// Image.
    Image {
        /// Optional image opacity gradient.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fade: Option<FadeDto>,
        /// Asset.
        asset: String,
        /// Fit.
        #[serde(default)]
        fit: FitDto,
    },
    /// Shape.
    #[serde(rename_all = "camelCase")]
    Shape {
        /// Reveal the device video in this shape.
        #[serde(default, rename = "videoWindow")]
        video_window: bool,
        /// Linear transparency, including the outline.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fade: Option<FadeDto>,
        /// `rect` or `ellipse`.
        shape: String,
        /// Corner radius for rects.
        #[serde(default)]
        radius: f32,
        /// Fill.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<PaintDto>,
        /// Outline color.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<String>,
        /// Outline width.
        #[serde(default)]
        stroke_width: f32,
    },
    /// Progress bar.
    Bar {
        /// Binding.
        binding: BindingDto,
        /// `leftToRight`, `rightToLeft`, `bottomToTop`, `topToBottom`.
        direction: String,
        /// Filled part.
        fill: PaintDto,
        /// Track.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<PaintDto>,
        /// Corner radius.
        #[serde(default)]
        radius: f32,
        /// Blocks.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segments: Option<SegmentsDto>,
    },
    /// Ring gauge.
    #[serde(rename_all = "camelCase")]
    Ring {
        /// Show the complete ring for appearance testing.
        #[serde(default)]
        test_full: bool,
        /// Binding.
        binding: BindingDto,
        /// Start angle.
        start_angle: f32,
        /// Sweep.
        sweep: f32,
        /// Thickness.
        thickness: f32,
        /// Clockwise.
        #[serde(default = "yes")]
        clockwise: bool,
        /// Filled arc.
        fill: PaintDto,
        /// Track.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<PaintDto>,
        /// `butt` or `round`.
        #[serde(default)]
        round_caps: bool,
        /// Blocks.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segments: Option<SegmentsDto>,
    },
    /// Needle.
    #[serde(rename_all = "camelCase")]
    Needle {
        /// Binding.
        binding: BindingDto,
        /// Needle image.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset: Option<String>,
        /// Pivot, 0..=1.
        pivot: (f32, f32),
        /// Angle at min.
        start_angle: f32,
        /// Sweep.
        sweep: f32,
        /// Drawn line color.
        color: String,
        /// Drawn line width.
        width: f32,
    },
    /// Graph.
    #[serde(rename_all = "camelCase")]
    Graph {
        /// Binding.
        binding: BindingDto,
        /// Samples.
        history: u16,
        /// `line`, `area`, `bars`.
        style: String,
        /// Line color.
        color: String,
        /// Area fill.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<PaintDto>,
        /// Line width.
        line_width: f32,
        /// Autoscale.
        #[serde(default)]
        autoscale: bool,
    },
}

fn orientation_name(o: Orientation) -> &'static str {
    match o {
        Orientation::Portrait => "portrait",
        Orientation::ReversePortrait => "reverse-portrait",
        Orientation::Landscape => "landscape",
        Orientation::ReverseLandscape => "reverse-landscape",
    }
}

fn orientation(s: &str) -> R<Orientation> {
    Ok(match s {
        "portrait" => Orientation::Portrait,
        "reverse-portrait" => Orientation::ReversePortrait,
        "landscape" => Orientation::Landscape,
        "reverse-landscape" => Orientation::ReverseLandscape,
        other => return err(format!("unknown orientation {other:?}")),
    })
}

fn fit_dto(f: Fit) -> FitDto {
    match f {
        Fit::Fill => FitDto::Fill,
        Fit::Contain => FitDto::Contain,
        Fit::Cover => FitDto::Cover,
        Fit::None => FitDto::None,
    }
}

fn fit(f: FitDto) -> Fit {
    match f {
        FitDto::Fill => Fit::Fill,
        FitDto::Contain => Fit::Contain,
        FitDto::Cover => Fit::Cover,
        FitDto::None => Fit::None,
    }
}

/// The framing as written: `None` (no `framing` key) when it is the default.
fn framing_dto(f: Option<&VideoFraming>) -> Option<FramingDto> {
    let f = f.filter(|f| !f.is_default())?;
    Some(FramingDto {
        rotation: f
            .rotation
            .map(|turns| serde_json::Number::from(u16::from(turns % 4) * 90)),
        fit: match f.fit {
            VideoFit::Cover => VideoFitDto::Cover,
            VideoFit::Contain => VideoFitDto::Contain,
        },
        zoom: f.zoom.factor(),
        position: PositionDto {
            x: f.position.x.fraction(),
            y: f.position.y.fraction(),
        },
        pad_color: to_hex(f.pad),
    })
}

/// Clockwise quarter turns of a framing's `rotation` in degrees.
fn quarter_turns(degrees: &serde_json::Number) -> R<u8> {
    [(0.0, 0), (90.0, 1), (180.0, 2), (270.0, 3)]
        .into_iter()
        .find(|(whole, _)| degrees.as_f64() == Some(*whole))
        .map(|(_, turns)| turns)
        .ok_or_else(|| {
            DtoError(format!(
                "video framing rotation {degrees} is not 0, 90, 180 or 270"
            ))
        })
}

/// The framing read: `None` when it is the default. Numbers are clamped; a
/// rotation other than 0, 90, 180 or 270 is refused.
fn framing(d: Option<&FramingDto>) -> R<Option<VideoFraming>> {
    let Some(d) = d else {
        return Ok(None);
    };
    let framing = VideoFraming {
        rotation: d.quarter_turns()?,
        fit: match d.fit {
            VideoFitDto::Cover => VideoFit::Cover,
            VideoFitDto::Contain => VideoFit::Contain,
        },
        zoom: Zoom::from_factor(d.zoom),
        position: FramingPosition {
            x: Permille::from_fraction(d.position.x),
            y: Permille::from_fraction(d.position.y),
        },
        pad: color(&d.pad_color)
            .map_err(|e| DtoError(format!("video framing padColor: {}", e.0)))?,
    };
    Ok((!framing.is_default()).then_some(framing))
}

fn paint_dto(p: &Paint) -> PaintDto {
    match p {
        Paint::Solid(c) => PaintDto::Solid(to_hex(*c)),
        Paint::Arc {
            start,
            end,
            transition,
        } => PaintDto::Arc {
            scale: "arc".into(),
            start: to_hex(*start),
            end: to_hex(*end),
            transition: transition * 100.0,
        },
        Paint::Linear { angle, stops } => PaintDto::Linear {
            angle: *angle,
            stops: stops.iter().map(|(pos, c)| (*pos, to_hex(*c))).collect(),
        },
    }
}

fn paint(p: &PaintDto) -> R<Paint> {
    Ok(match p {
        PaintDto::Solid(c) => Paint::Solid(color(c)?),
        PaintDto::Arc {
            scale,
            start,
            end,
            transition,
        } => {
            if scale != "arc" || !transition.is_finite() || !(0.0..=100.0).contains(transition) {
                return Err(DtoError("invalid arc gradient scale or transition".into()));
            }
            Paint::Arc {
                start: color(start)?,
                end: color(end)?,
                transition: transition / 100.0,
            }
        }
        PaintDto::Linear { angle, stops } => Paint::Linear {
            angle: *angle,
            stops: stops
                .iter()
                .map(|(pos, c)| Ok((*pos, color(c)?)))
                .collect::<R<_>>()?,
        },
    })
}

fn opt_paint(p: &Option<PaintDto>) -> R<Option<Paint>> {
    p.as_ref().map(paint).transpose()
}

fn binding_dto(b: &Binding) -> BindingDto {
    BindingDto {
        key: b.key.as_str().to_string(),
        min: b.min,
        max: b.max,
    }
}

fn binding(b: &BindingDto) -> R<Binding> {
    Ok(Binding {
        key: key(&b.key)?,
        min: b.min,
        max: b.max,
    })
}

fn segments_dto(s: Option<Segments>) -> Option<SegmentsDto> {
    s.map(|s| SegmentsDto {
        count: s.count,
        gap: s.gap,
    })
}

fn segments(s: Option<SegmentsDto>) -> Option<Segments> {
    s.map(|s| Segments {
        count: s.count,
        gap: s.gap,
    })
}

fn format_dto(f: DisplayFormat) -> FormatDto {
    FormatDto {
        decimals: f.decimals,
        show_unit: f.show_unit,
        fahrenheit: f.temperature == TemperatureUnit::Fahrenheit,
        decimal_bytes: f.bytes == ByteUnits::Decimal,
    }
}

fn format(f: FormatDto) -> DisplayFormat {
    DisplayFormat {
        decimals: f.decimals,
        show_unit: f.show_unit,
        temperature: if f.fahrenheit {
            TemperatureUnit::Fahrenheit
        } else {
            TemperatureUnit::Celsius
        },
        bytes: if f.decimal_bytes {
            ByteUnits::Decimal
        } else {
            ByteUnits::Binary
        },
    }
}

fn style_dto(s: &TextStyle) -> TextStyleDto {
    TextStyleDto {
        font: FontDto {
            family: s.font.family.clone(),
            weight: s.font.weight,
            italic: s.font.italic,
            asset: s.font.asset.as_ref().map(|a| a.0.clone()),
        },
        size: s.size,
        paint: paint_dto(&s.paint),
        align: match s.align {
            HAlign::Left => "left",
            HAlign::Center => "center",
            HAlign::Right => "right",
        }
        .to_string(),
        valign: match s.valign {
            VAlign::Top => "top",
            VAlign::Middle => "middle",
            VAlign::Bottom => "bottom",
        }
        .to_string(),
        letter_spacing: s.letter_spacing,
    }
}

fn style(s: &TextStyleDto) -> R<TextStyle> {
    Ok(TextStyle {
        font: FontSpec {
            family: s.font.family.clone(),
            weight: s.font.weight,
            italic: s.font.italic,
            asset: s.font.asset.clone().map(AssetRef),
        },
        size: s.size,
        paint: paint(&s.paint)?,
        align: match s.align.as_str() {
            "left" => HAlign::Left,
            "center" => HAlign::Center,
            "right" => HAlign::Right,
            other => return err(format!("unknown align {other:?}")),
        },
        valign: match s.valign.as_str() {
            "top" => VAlign::Top,
            "middle" => VAlign::Middle,
            "bottom" => VAlign::Bottom,
            other => return err(format!("unknown valign {other:?}")),
        },
        letter_spacing: s.letter_spacing,
    })
}

fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::LeftToRight => "leftToRight",
        Direction::RightToLeft => "rightToLeft",
        Direction::BottomToTop => "bottomToTop",
        Direction::TopToBottom => "topToBottom",
    }
}

fn direction(s: &str) -> R<Direction> {
    Ok(match s {
        "leftToRight" => Direction::LeftToRight,
        "rightToLeft" => Direction::RightToLeft,
        "bottomToTop" => Direction::BottomToTop,
        "topToBottom" => Direction::TopToBottom,
        other => return err(format!("unknown direction {other:?}")),
    })
}

fn graph_style_name(s: GraphStyle) -> &'static str {
    match s {
        GraphStyle::Line => "line",
        GraphStyle::Area => "area",
        GraphStyle::Bars => "bars",
    }
}

fn graph_style(s: &str) -> R<GraphStyle> {
    Ok(match s {
        "line" => GraphStyle::Line,
        "area" => GraphStyle::Area,
        "bars" => GraphStyle::Bars,
        other => return err(format!("unknown graph style {other:?}")),
    })
}

fn kind_dto(k: &ElementKind) -> KindDto {
    match k {
        ElementKind::Text { content, style } => KindDto::Text {
            content: match content {
                TextContent::Player(p) => ContentDto::Player {
                    source: p.source.clone(),
                    show_cover: p.show_cover,
                    show_progress: p.show_progress,
                    show_source: p.show_source,
                    hide_when_stopped: p.hide_when_stopped,
                    empty_text: p.empty_text.clone(),
                },
                TextContent::Static(text) => ContentDto::Static { text: text.clone() },
                TextContent::Sensor {
                    key,
                    format,
                    prefix,
                    suffix,
                } => ContentDto::Sensor {
                    key: key.as_str().to_string(),
                    format: format_dto(*format),
                    prefix: prefix.clone(),
                    suffix: suffix.clone(),
                },
                TextContent::Weather(w) => ContentDto::Weather {
                    city: w.city.clone(),
                    latitude: w.latitude,
                    longitude: w.longitude,
                    language: w.language.map(|l| {
                        match l {
                            Language::Italian => "it",
                            Language::English => "en",
                            Language::PortugueseBr => "pt-BR",
                        }
                        .to_string()
                    }),
                    fahrenheit: w.fahrenheit,
                    show_icon: w.show_icon,
                    icon_style: match w.icon_style {
                        bezel_core::domain::weather::IconStyle::Outline => "outline",
                        bezel_core::domain::weather::IconStyle::Filled => "filled",
                        bezel_core::domain::weather::IconStyle::Colored => "colored",
                        bezel_core::domain::weather::IconStyle::Dimensional => "dimensional",
                    }
                    .into(),
                    icon_gap: w.icon_gap,
                    icon_size: w.icon_size,
                },
                TextContent::Clock {
                    pattern,
                    language,
                    casing,
                } => ContentDto::Clock {
                    pattern: pattern.clone(),
                    language: language.map(|l| {
                        match l {
                            Language::English => "en",
                            Language::PortugueseBr => "pt-BR",
                            Language::Italian => "it",
                        }
                        .to_string()
                    }),
                    casing: match casing {
                        ClockCase::Normal => None,
                        ClockCase::Upper => Some("upper".into()),
                        ClockCase::Title => Some("title".into()),
                    },
                },
            },
            style: style_dto(style),
        },
        ElementKind::Image { asset, fit, fade } => KindDto::Image {
            fade: fade.map(fade_dto),
            asset: asset.0.clone(),
            fit: fit_dto(*fit),
        },
        ElementKind::Shape {
            video_window,
            fade,
            shape,
            fill,
            stroke,
        } => {
            let (name, radius) = match shape {
                ShapeKind::Rect { radius } => ("rect", *radius),
                ShapeKind::Ellipse => ("ellipse", 0.0),
            };
            KindDto::Shape {
                video_window: *video_window,
                fade: fade.map(|f| FadeDto {
                    radial: f.radial,
                    angle: f.angle,
                    start: f.start,
                    end: f.end,
                }),
                shape: name.to_string(),
                radius,
                fill: fill.as_ref().map(paint_dto),
                stroke: stroke.map(|(c, _)| to_hex(c)),
                stroke_width: stroke.map_or(0.0, |(_, w)| w),
            }
        }
        ElementKind::Bar {
            binding,
            direction,
            fill,
            track,
            radius,
            segments,
        } => KindDto::Bar {
            binding: binding_dto(binding),
            direction: direction_name(*direction).to_string(),
            fill: paint_dto(fill),
            track: track.as_ref().map(paint_dto),
            radius: *radius,
            segments: segments_dto(*segments),
        },
        ElementKind::Ring {
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
        } => KindDto::Ring {
            test_full: *test_full,
            binding: binding_dto(binding),
            start_angle: *start_angle,
            sweep: *sweep,
            thickness: *thickness,
            clockwise: *clockwise,
            fill: paint_dto(fill),
            track: track.as_ref().map(paint_dto),
            round_caps: *cap == Cap::Round,
            segments: segments_dto(*segments),
        },
        ElementKind::Needle {
            binding,
            asset,
            pivot,
            start_angle,
            sweep,
            color,
            width,
        } => KindDto::Needle {
            binding: binding_dto(binding),
            asset: asset.as_ref().map(|a| a.0.clone()),
            pivot: *pivot,
            start_angle: *start_angle,
            sweep: *sweep,
            color: to_hex(*color),
            width: *width,
        },
        ElementKind::Graph {
            binding,
            history,
            style,
            color,
            fill,
            line_width,
            autoscale,
        } => KindDto::Graph {
            binding: binding_dto(binding),
            history: *history,
            style: graph_style_name(*style).to_string(),
            color: to_hex(*color),
            fill: fill.as_ref().map(paint_dto),
            line_width: *line_width,
            autoscale: *autoscale,
        },
    }
}

fn kind(k: &KindDto) -> R<ElementKind> {
    Ok(match k {
        KindDto::Text { content, style: s } => ElementKind::Text {
            content: match content {
                ContentDto::Player {
                    source,
                    show_cover,
                    show_progress,
                    show_source,
                    hide_when_stopped,
                    empty_text,
                } => {
                    if source.chars().count() > 160 || empty_text.chars().count() > 512 {
                        return err("player text is too long");
                    }
                    TextContent::Player(bezel_core::domain::playback::Player {
                        source: source.clone(),
                        show_cover: *show_cover,
                        show_progress: *show_progress,
                        show_source: *show_source,
                        hide_when_stopped: *hide_when_stopped,
                        empty_text: empty_text.clone(),
                    })
                }
                ContentDto::Static { text } => TextContent::Static(text.clone()),
                ContentDto::Sensor {
                    key: k,
                    format: f,
                    prefix,
                    suffix,
                } => TextContent::Sensor {
                    key: key(k)?,
                    format: format(*f),
                    prefix: prefix.clone(),
                    suffix: suffix.clone(),
                },
                ContentDto::Weather {
                    city,
                    latitude,
                    longitude,
                    language,
                    fahrenheit,
                    show_icon,
                    icon_style,
                    icon_gap,
                    icon_size,
                } => {
                    if !bezel_core::domain::weather::valid(city, *latitude, *longitude) {
                        return err("invalid weather location");
                    }
                    if icon_gap.is_some_and(|v| !v.is_finite() || !(0.0..=256.0).contains(&v))
                        || icon_size.is_some_and(|v| !v.is_finite() || !(1.0..=512.0).contains(&v))
                    {
                        return err("invalid weather icon dimensions");
                    }
                    TextContent::Weather(bezel_core::domain::weather::Weather {
                        city: city.clone(),
                        latitude: *latitude,
                        longitude: *longitude,
                        language: match language.as_deref() {
                            None | Some("") => None,
                            Some("it") => Some(Language::Italian),
                            Some("en") => Some(Language::English),
                            Some("pt-BR") => Some(Language::PortugueseBr),
                            _ => return err("invalid weather language"),
                        },
                        fahrenheit: *fahrenheit,
                        show_icon: *show_icon,
                        icon_style: match icon_style.as_str() {
                            "outline" => bezel_core::domain::weather::IconStyle::Outline,
                            "filled" => bezel_core::domain::weather::IconStyle::Filled,
                            "colored" => bezel_core::domain::weather::IconStyle::Colored,
                            "dimensional" => bezel_core::domain::weather::IconStyle::Dimensional,
                            _ => return err("invalid weather icon style"),
                        },
                        icon_gap: *icon_gap,
                        icon_size: *icon_size,
                    })
                }
                ContentDto::Clock {
                    pattern,
                    language,
                    casing,
                } => TextContent::Clock {
                    pattern: pattern.clone(),
                    language: match language.as_deref() {
                        None | Some("") => None,
                        Some("en") => Some(Language::English),
                        Some("pt-BR") => Some(Language::PortugueseBr),
                        Some("it") => Some(Language::Italian),
                        Some(other) => return err(format!("bad clock language {other:?}")),
                    },
                    casing: match casing.as_deref() {
                        None | Some("normal") => ClockCase::Normal,
                        Some("upper") => ClockCase::Upper,
                        Some("title") => ClockCase::Title,
                        Some(other) => return err(format!("bad clock casing {other:?}")),
                    },
                },
            },
            style: style(s)?,
        },
        KindDto::Image {
            asset,
            fit: f,
            fade,
        } => ElementKind::Image {
            fade: fade.as_ref().map(parse_fade).transpose()?,
            asset: AssetRef(asset.clone()),
            fit: fit(*f),
        },
        KindDto::Shape {
            video_window,
            fade,
            shape,
            radius,
            fill,
            stroke,
            stroke_width,
        } => ElementKind::Shape {
            video_window: *video_window,
            fade: fade.as_ref().map(parse_fade).transpose()?,
            shape: match shape.as_str() {
                "rect" => ShapeKind::Rect { radius: *radius },
                "ellipse" => ShapeKind::Ellipse,
                other => return err(format!("unknown shape {other:?}")),
            },
            fill: opt_paint(fill)?,
            stroke: stroke
                .as_deref()
                .map(color)
                .transpose()?
                .map(|c| (c, *stroke_width)),
        },
        KindDto::Bar {
            binding: b,
            direction: d,
            fill,
            track,
            radius,
            segments: s,
        } => ElementKind::Bar {
            binding: binding(b)?,
            direction: direction(d)?,
            fill: paint(fill)?,
            track: opt_paint(track)?,
            radius: *radius,
            segments: segments(*s),
        },
        KindDto::Ring {
            test_full,
            binding: b,
            start_angle,
            sweep,
            thickness,
            clockwise,
            fill,
            track,
            round_caps,
            segments: s,
        } => ElementKind::Ring {
            test_full: *test_full,
            binding: binding(b)?,
            start_angle: *start_angle,
            sweep: *sweep,
            thickness: *thickness,
            clockwise: *clockwise,
            fill: paint(fill)?,
            track: opt_paint(track)?,
            cap: if *round_caps { Cap::Round } else { Cap::Butt },
            segments: segments(*s),
        },
        KindDto::Needle {
            binding: b,
            asset,
            pivot,
            start_angle,
            sweep,
            color: c,
            width,
        } => ElementKind::Needle {
            binding: binding(b)?,
            asset: asset.clone().map(AssetRef),
            pivot: *pivot,
            start_angle: *start_angle,
            sweep: *sweep,
            color: color(c)?,
            width: *width,
        },
        KindDto::Graph {
            binding: b,
            history,
            style: s,
            color: c,
            fill,
            line_width,
            autoscale,
        } => ElementKind::Graph {
            binding: binding(b)?,
            history: *history,
            style: graph_style(s)?,
            color: color(c)?,
            fill: opt_paint(fill)?,
            line_width: *line_width,
            autoscale: *autoscale,
        },
    })
}

impl From<&Theme> for ThemeDto {
    fn from(t: &Theme) -> Self {
        ThemeDto {
            schema: SCHEMA,
            name: t.name.clone(),
            canvas: SizeDto {
                width: t.canvas.width,
                height: t.canvas.height,
            },
            orientation: orientation_name(t.orientation).to_string(),
            refresh_seconds: t.refresh_seconds,
            background: match &t.background {
                Background::DeviceVideo {
                    path,
                    repeat,
                    color,
                } => BackgroundDto::DeviceVideo {
                    path: path.to_string(),
                    looping: *repeat == bezel_core::domain::storage::Repeat::Loop,
                    color: to_hex(*color),
                },
                Background::Color(c) => BackgroundDto::Color { color: to_hex(*c) },
                Background::Image { asset, fit } => BackgroundDto::Image {
                    asset: asset.0.clone(),
                    fit: fit_dto(*fit),
                },
                Background::Video {
                    asset,
                    poster,
                    framing,
                } => BackgroundDto::Video {
                    asset: asset.0.clone(),
                    poster: poster.as_ref().map(|p| p.0.clone()),
                    framing: framing_dto(framing.as_ref()),
                },
            },
            elements: t
                .elements
                .iter()
                .map(|e| ElementDto {
                    card: e.card.as_ref().map(|c| CardDto {
                        triggers: c
                            .triggers
                            .iter()
                            .map(|r| CardTriggerDto {
                                source: match r.source {
                                    bezel_core::domain::playback::TriggerSource::Process => {
                                        "process"
                                    }
                                    bezel_core::domain::playback::TriggerSource::ProcessClosed => {
                                        "processClosed"
                                    }
                                    bezel_core::domain::playback::TriggerSource::Foreground => {
                                        "foreground"
                                    }
                                    bezel_core::domain::playback::TriggerSource::MediaPlaying => {
                                        "mediaPlaying"
                                    }
                                }
                                .into(),
                                app: r.app.clone(),
                                face: r.face,
                                priority: r.priority,
                                return_seconds: r.return_seconds,
                            })
                            .collect(),
                        faces: c.faces.clone(),
                        active_face: c.active_face,
                        rotation_seconds: c.rotation_seconds,
                        transition: c.transition.map(|t| CardTransitionDto {
                            effect: format!("{:?}", t.effect).to_lowercase(),
                            direction: format!("{:?}", t.direction).to_lowercase(),
                            duration_ms: t.duration_ms,
                            include_base: t.include_base,
                        }),
                    }),
                    is_group: e.is_group,
                    group_parent: e.group_parent.map(|id| id.0),
                    card_member: e.card_member.as_ref().map(|m| CardMemberDto {
                        parent: m.parent.0,
                        face: m.face,
                    }),
                    id: e.id.0,
                    name: e.name.clone(),
                    frame: BoxDto {
                        x: e.frame.x,
                        y: e.frame.y,
                        width: e.frame.width,
                        height: e.frame.height,
                    },
                    opacity: e.opacity,
                    visible: e.visible,
                    locked: e.locked,
                    kind: kind_dto(&e.kind),
                })
                .collect(),
        }
    }
}

impl TryFrom<&ThemeDto> for Theme {
    type Error = DtoError;

    fn try_from(d: &ThemeDto) -> R<Theme> {
        if d.schema == 0 || d.schema > SCHEMA {
            return err(format!(
                "theme schema {} is not supported (this build reads 1..={SCHEMA})",
                d.schema
            ));
        }
        if d.canvas.width == 0 || d.canvas.height == 0 {
            return err("empty canvas");
        }
        let background = match &d.background {
            BackgroundDto::DeviceVideo {
                path,
                looping,
                color: c,
            } => {
                let path = bezel_core::domain::storage::RemotePath::parse(path)
                    .map_err(|e| DtoError(e.to_string()))?;
                if path.location.kind != bezel_core::domain::media::MediaKind::Video {
                    return err("device background must reference a video");
                }
                Background::DeviceVideo {
                    path,
                    repeat: if *looping {
                        bezel_core::domain::storage::Repeat::Loop
                    } else {
                        bezel_core::domain::storage::Repeat::Once
                    },
                    color: color(c)?,
                }
            }
            BackgroundDto::Color { color: c } => Background::Color(color(c)?),
            BackgroundDto::Image { asset, fit: f } => Background::Image {
                asset: AssetRef(asset.clone()),
                fit: fit(*f),
            },
            BackgroundDto::Video {
                asset,
                poster,
                framing: f,
            } => Background::Video {
                asset: AssetRef(asset.clone()),
                poster: poster.clone().map(AssetRef),
                framing: framing(f.as_ref())?,
            },
        };
        let elements = d
            .elements
            .iter()
            .map(|e| {
                Ok(Element {
                    card: e
                        .card
                        .as_ref()
                        .map(|c| -> R<bezel_core::domain::theme::Card> {
                            use bezel_core::domain::theme::{
                                CardDirection, CardEffect, CardTransition,
                            };
                            if c.rotation_seconds
                                .is_some_and(|seconds| !(5..=3600).contains(&seconds))
                            {
                                return err("card rotation interval must be 5 to 3600 seconds");
                            }
                            if c.triggers.len() > 32 {
                                return err("too many card triggers");
                            }
                            let triggers = c
                                .triggers
                                .iter()
                                .map(|r| {
                                    use bezel_core::domain::playback::{
                                        CardTrigger, TriggerSource,
                                    };
                                    let source = match r.source.as_str() {
                                        "process" => TriggerSource::Process,
                                        "processClosed" => TriggerSource::ProcessClosed,
                                        "foreground" => TriggerSource::Foreground,
                                        "mediaPlaying" => TriggerSource::MediaPlaying,
                                        _ => return err("unknown card trigger"),
                                    };
                                    if r.app.chars().count() > 160
                                        || (r.app.trim().is_empty()
                                            && source != TriggerSource::MediaPlaying)
                                        || r.face >= c.faces.len()
                                        || r.priority > 100
                                        || r.return_seconds > 300
                                    {
                                        return err("invalid card trigger");
                                    }
                                    Ok(CardTrigger {
                                        source,
                                        app: r.app.clone(),
                                        face: r.face,
                                        priority: r.priority,
                                        return_seconds: r.return_seconds,
                                    })
                                })
                                .collect::<R<Vec<_>>>()?;
                            let transition = c
                                .transition
                                .as_ref()
                                .map(|t| -> R<CardTransition> {
                                    let effect = match t.effect.as_str() {
                                        "none" => CardEffect::None,
                                        "fade" => CardEffect::Fade,
                                        "slide" => CardEffect::Slide,
                                        "flip" => CardEffect::Flip,
                                        _ => return err("unknown card transition effect"),
                                    };
                                    let direction = match t.direction.as_str() {
                                        "left" => CardDirection::Left,
                                        "right" => CardDirection::Right,
                                        "up" => CardDirection::Up,
                                        "down" => CardDirection::Down,
                                        _ => return err("unknown card transition direction"),
                                    };
                                    if !(150..=3000).contains(&t.duration_ms) {
                                        return err(
                                            "card transition duration must be 150 to 3000 ms",
                                        );
                                    }
                                    Ok(CardTransition {
                                        effect,
                                        direction,
                                        duration_ms: t.duration_ms,
                                        include_base: t.include_base,
                                    })
                                })
                                .transpose()?;
                            Ok(bezel_core::domain::theme::Card {
                                triggers,
                                faces: c.faces.clone(),
                                active_face: c.active_face,
                                rotation_seconds: c.rotation_seconds,
                                transition,
                            })
                        })
                        .transpose()?,
                    is_group: e.is_group,
                    group_parent: e.group_parent.map(ElementId),
                    card_member: e.card_member.as_ref().map(|m| {
                        bezel_core::domain::theme::CardMember {
                            parent: ElementId(m.parent),
                            face: m.face,
                        }
                    }),
                    id: ElementId(e.id),
                    name: e.name.clone(),
                    frame: BoxF::new(e.frame.x, e.frame.y, e.frame.width, e.frame.height),
                    opacity: e.opacity.clamp(0.0, 1.0),
                    visible: e.visible,
                    locked: e.locked,
                    kind: kind(&e.kind)
                        .map_err(|x| DtoError(format!("element {} ({}): {}", e.id, e.name, x.0)))?,
                })
            })
            .collect::<R<Vec<_>>>()?;
        if elements.iter().any(|e| {
            e.card.is_some() || e.card_member.is_some() || e.is_group || e.group_parent.is_some()
        }) {
            let ids: std::collections::BTreeSet<_> = elements.iter().map(|e| e.id.0).collect();
            if ids.len() != elements.len() {
                return err("duplicate element IDs in card theme");
            }
        }
        for e in &elements {
            if let Some(card) = &e.card
                && (card.faces.is_empty()
                    || card.faces.len() > 16
                    || card.active_face >= card.faces.len()
                    || card
                        .faces
                        .iter()
                        .any(|name| name.trim().is_empty() || name.len() > 128)
                    || e.card_member.is_some()
                    || !matches!(e.kind, ElementKind::Shape { .. }))
            {
                return err("invalid card configuration");
            }
            if let Some(member) = &e.card_member {
                let parent = elements
                    .iter()
                    .find(|parent| parent.id == member.parent && parent.id != e.id);
                if !parent
                    .and_then(|p| p.card.as_ref())
                    .is_some_and(|c| member.face.is_none_or(|f| f < c.faces.len()))
                {
                    return err("invalid card membership");
                }
            }
        }
        for e in &elements {
            if e.is_group
                && (e.card.is_some()
                    || !matches!(
                        e.kind,
                        ElementKind::Shape {
                            fill: None,
                            stroke: None,
                            fade: None,
                            video_window: false,
                            ..
                        }
                    ))
            {
                return err("invalid ordinary group container");
            }
            let mut current = e;
            let mut seen = std::collections::BTreeSet::from([e.id]);
            while let Some(id) = current.group_parent {
                let Some(parent) = elements.iter().find(|p| p.id == id && p.is_group) else {
                    return err("invalid group membership");
                };
                if !seen.insert(id) || seen.len() > 64 || parent.card_member != current.card_member
                {
                    return err("invalid group hierarchy");
                }
                current = parent;
            }
        }
        Ok(Theme {
            name: d.name.clone(),
            canvas: Size::new(d.canvas.width, d.canvas.height),
            orientation: orientation(&d.orientation)?,
            background,
            elements,
            refresh_seconds: if d.refresh_seconds > 0.0 {
                d.refresh_seconds
            } else {
                1.0
            },
        })
    }
}

fn outline_icon() -> String {
    "outline".into()
}

fn is_false(value: &bool) -> bool {
    !value
}

fn fade_dto(f: bezel_core::domain::gradient::Fade) -> FadeDto {
    FadeDto {
        angle: f.angle,
        start: f.start,
        end: f.end,
        radial: f.radial,
    }
}
fn parse_fade(f: &FadeDto) -> R<bezel_core::domain::gradient::Fade> {
    if !f.angle.is_finite()
        || !f.start.is_finite()
        || !f.end.is_finite()
        || !(0.0..=1.0).contains(&f.start)
        || !(0.0..=1.0).contains(&f.end)
        || f.radial.is_some_and(|[x, y, r]| {
            !x.is_finite()
                || !y.is_finite()
                || !r.is_finite()
                || !(0.0..=1.0).contains(&x)
                || !(0.0..=1.0).contains(&y)
                || !(0.01..=2.0).contains(&r)
        })
    {
        return err("invalid opacity gradient");
    }
    Ok(bezel_core::domain::gradient::Fade {
        angle: f.angle,
        start: f.start,
        end: f.end,
        radial: f.radial,
    })
}
