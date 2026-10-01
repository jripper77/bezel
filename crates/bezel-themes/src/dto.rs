//! `theme.json`, schema 1: serde shapes and their mapping to the core model.

use bezel_core::domain::frame::Rgba;
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
    },
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
    /// A linear gradient.
    Linear {
        /// Angle, degrees.
        angle: f32,
        /// `[position, color]` pairs.
        stops: Vec<(f32, String)>,
    },
}

/// One element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementDto {
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
        /// Asset.
        asset: String,
        /// Fit.
        #[serde(default)]
        fit: FitDto,
    },
    /// Shape.
    #[serde(rename_all = "camelCase")]
    Shape {
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

fn paint_dto(p: &Paint) -> PaintDto {
    match p {
        Paint::Solid(c) => PaintDto::Solid(to_hex(*c)),
        Paint::Linear { angle, stops } => PaintDto::Linear {
            angle: *angle,
            stops: stops.iter().map(|(pos, c)| (*pos, to_hex(*c))).collect(),
        },
    }
}

fn paint(p: &PaintDto) -> R<Paint> {
    Ok(match p {
        PaintDto::Solid(c) => Paint::Solid(color(c)?),
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
                TextContent::Clock { pattern } => ContentDto::Clock {
                    pattern: pattern.clone(),
                },
            },
            style: style_dto(style),
        },
        ElementKind::Image { asset, fit } => KindDto::Image {
            asset: asset.0.clone(),
            fit: fit_dto(*fit),
        },
        ElementKind::Shape {
            shape,
            fill,
            stroke,
        } => {
            let (name, radius) = match shape {
                ShapeKind::Rect { radius } => ("rect", *radius),
                ShapeKind::Ellipse => ("ellipse", 0.0),
            };
            KindDto::Shape {
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
                ContentDto::Clock { pattern } => TextContent::Clock {
                    pattern: pattern.clone(),
                },
            },
            style: style(s)?,
        },
        KindDto::Image { asset, fit: f } => ElementKind::Image {
            asset: AssetRef(asset.clone()),
            fit: fit(*f),
        },
        KindDto::Shape {
            shape,
            radius,
            fill,
            stroke,
            stroke_width,
        } => ElementKind::Shape {
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
                Background::Color(c) => BackgroundDto::Color { color: to_hex(*c) },
                Background::Image { asset, fit } => BackgroundDto::Image {
                    asset: asset.0.clone(),
                    fit: fit_dto(*fit),
                },
                Background::Video { asset, poster, .. } => BackgroundDto::Video {
                    asset: asset.0.clone(),
                    poster: poster.as_ref().map(|p| p.0.clone()),
                },
            },
            elements: t
                .elements
                .iter()
                .map(|e| ElementDto {
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
            BackgroundDto::Color { color: c } => Background::Color(color(c)?),
            BackgroundDto::Image { asset, fit: f } => Background::Image {
                asset: AssetRef(asset.clone()),
                fit: fit(*f),
            },
            BackgroundDto::Video { asset, poster } => Background::Video {
                asset: AssetRef(asset.clone()),
                poster: poster.clone().map(AssetRef),
                framing: None,
            },
        };
        let elements = d
            .elements
            .iter()
            .map(|e| {
                Ok(Element {
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
