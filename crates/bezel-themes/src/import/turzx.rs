//! The vendor app's `.turtheme` themes (TURZX V3.07): an MS-NRBF graph of one
//! `UsbMonitorL.Theme`, read with the whitelisting [`nrbf`](super::nrbf)
//! parser and mapped to Bezel's model (docs/reverse-engineering/themes-turzx.md).
//!
//! The author's paths, the stale sensor histories and the preview picture
//! stored in the file are never carried over.

use std::collections::{BTreeSet, HashMap};

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::sensor::{ByteUnits, DisplayFormat, SensorKey, TemperatureUnit, keys};
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Cap, Direction, ElementKind, Fit, FontSpec, GraphStyle,
    HAlign, Paint, Segments, ShapeKind, TextContent, TextStyle, Theme, VAlign,
};

use super::colors::{self, Known};
use super::nrbf::{Class, Graph, Limits, Object, ObjectId, Primitive, Value};
use super::{
    Builder, ImportWarning, Imported, WarningCode as Code, covering_fit, file_name, image_size,
    line_height, text_width,
};

/// Classes a `.turtheme` may contain (themes-turzx.md section 5).
const CLASSES: [&str; 15] = [
    "UsbMonitorL.Theme",
    "UsbMonitorL.GraphItem",
    "UsbMonitorL.GraphImage",
    "UsbMonitorL.GraphAnimation",
    "UsbMonitorL.GraphStatuBar",
    "UsbMonitorL.GraphArchBar",
    "UsbMonitorL.GraphClock",
    "UsbMonitorL.GraphLine",
    "UsbMonitorL.M_Data",
    "UsbMonitorL.FontConfig",
    "UsbMonitorL.TextAlignment",
    "UsbMonitorL.TransFormInfo",
    "System.Drawing.Color",
    "System.Drawing.Bitmap",
    "System.Drawing.Rectangle",
];

/// Generic system collections allowed, with their only allowed argument.
const GENERICS: [(&str, &str); 5] = [
    (
        "System.Collections.ObjectModel.ObservableCollection`1[[",
        "UsbMonitorL.GraphItem, UsbMonitorL,",
    ),
    (
        "System.Collections.ObjectModel.ObservableCollection`1+SimpleMonitor[[",
        "UsbMonitorL.GraphItem, UsbMonitorL,",
    ),
    (
        "System.Collections.Generic.List`1[[",
        "UsbMonitorL.GraphItem, UsbMonitorL,",
    ),
    (
        "System.Collections.Generic.List`1[[",
        "System.String, mscorlib,",
    ),
    (
        "System.Collections.Generic.Queue`1[[",
        "System.String, mscorlib,",
    ),
];

/// True for the class names a vendor theme may contain.
pub fn allowed_class(name: &str) -> bool {
    CLASSES.contains(&name)
        || GENERICS.iter().any(|(prefix, argument)| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with(argument) && rest.ends_with("]]"))
        })
}

/// True when `bytes` start like a `.turtheme` (an NRBF 1.0 header).
pub fn looks_like_turtheme(bytes: &[u8]) -> bool {
    bytes.len() > 17 && bytes[0] == 0 && bytes[9..17] == [1, 0, 0, 0, 0, 0, 0, 0]
}

const MIB: f64 = 1024.0 * 1024.0;
const GIB: f64 = 1024.0 * MIB;
const PURPLE: Rgba = Rgba::opaque(128, 0, 128);
const DEEP_PINK: Rgba = Rgba::opaque(255, 20, 147);
const LIGHT_GRAY: Rgba = Rgba::opaque(211, 211, 211);
const SKY_BLUE: Rgba = Rgba::opaque(135, 206, 235);
const TEXT_GRAY: Rgba = Rgba::opaque(128, 128, 128);
const TRANSPARENT: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// How a vendor data name maps to a Bezel sensor.
struct Spec {
    /// Vendor `DataName`.
    name: &'static str,
    /// Bezel key; `{}` is replaced by the `SubName` (drive, disk index).
    key: &'static str,
    /// Multiplier from the vendor unit to the Bezel unit.
    scale: f64,
    /// Vendor value shown as a full bar (the app's `Rate` divisor).
    full: f64,
    /// Decimals of the vendor's text.
    decimals: Option<u8>,
    /// Reported once per theme when the source is bound.
    note: Option<Code>,
}

const fn spec(name: &'static str, key: &'static str, scale: f64, full: f64) -> Spec {
    Spec {
        name,
        key,
        scale,
        full,
        decimals: Some(0),
        note: None,
    }
}

/// The vendor data names (sensors.md section 3) and their Bezel keys.
const SENSORS: [Spec; 38] = [
    spec("CPUTEMP", keys::CPU_TEMPERATURE, 1.0, 100.0),
    Spec {
        decimals: None,
        ..spec("CPUCLOCK", keys::CPU_FREQUENCY, 1.0, 6000.0)
    },
    Spec {
        decimals: None,
        ..spec("CPUCLOCK_G", keys::CPU_FREQUENCY, 1000.0, 6.0)
    },
    spec("CPULOAD", keys::CPU_USAGE, 1.0, 100.0),
    spec("CPUPWR", keys::CPU_POWER, 1.0, 500.0),
    spec("CPUFAN", keys::CPU_FAN, 1.0, 8000.0),
    Spec {
        decimals: Some(2),
        ..spec("CPUVOLTAGE", keys::CPU_VOLTAGE, 1.0, 100.0)
    },
    spec("CPUMODEL", keys::CPU_NAME, 1.0, 100.0),
    spec("GPUTEMP", keys::GPU_TEMPERATURE, 1.0, 100.0),
    Spec {
        decimals: None,
        ..spec("GPUCLOCK", keys::GPU_FREQUENCY, 1.0, 6000.0)
    },
    Spec {
        decimals: None,
        ..spec("GPUCLOCK_G", keys::GPU_FREQUENCY, 1000.0, 6.0)
    },
    Spec {
        decimals: None,
        ..spec("GPURAMTOTAL", keys::GPU_MEMORY_TOTAL, MIB, 100.0)
    },
    spec("GPURAMLOAD", keys::GPU_MEMORY_PERCENT, 1.0, 100.0),
    Spec {
        decimals: None,
        ..spec("GPURAM", keys::GPU_MEMORY_USED, MIB, 100.0)
    },
    spec("GPULOAD", keys::GPU_USAGE, 1.0, 100.0),
    spec("GPUPWR", keys::GPU_POWER, 1.0, 500.0),
    Spec {
        note: Some(Code::GpuFanPercent),
        ..spec("GPUFAN", keys::GPU_FAN, 1.0, 100.0)
    },
    Spec {
        decimals: Some(2),
        ..spec("GPUVOLTAGE", keys::GPU_VOLTAGE, 1.0, 100.0)
    },
    spec("GPUMODEL", keys::GPU_NAME, 1.0, 100.0),
    Spec {
        decimals: None,
        ..spec("RAMVALID", keys::MEMORY_AVAILABLE, MIB, 100.0)
    },
    spec("RAMLOAD", keys::MEMORY_PERCENT, 1.0, 100.0),
    Spec {
        decimals: None,
        ..spec("RAM", keys::MEMORY_USED, MIB, 100.0)
    },
    Spec {
        decimals: None,
        ..spec("RAMTOTAL", keys::MEMORY_TOTAL, MIB, 100.0)
    },
    Spec {
        note: Some(Code::MemoryModel),
        ..spec("RAMMODEL", "vendor.RAMMODEL", 1.0, 100.0)
    },
    Spec {
        decimals: Some(1),
        ..spec("RAM_GB", keys::MEMORY_USED, GIB, 100.0)
    },
    Spec {
        decimals: Some(1),
        ..spec("RAMVALID_GB", keys::MEMORY_AVAILABLE, GIB, 100.0)
    },
    Spec {
        decimals: Some(1),
        ..spec("RAMTOTAL_GB", keys::MEMORY_TOTAL, GIB, 100.0)
    },
    Spec {
        note: Some(Code::RebindSensor),
        ..spec("WATERPUMP", keys::FAN_PUMP, 1.0, 8000.0)
    },
    Spec {
        note: Some(Code::RebindSensor),
        ..spec("CASEFAN1", keys::FAN_CASE_1, 1.0, 8000.0)
    },
    Spec {
        note: Some(Code::RebindSensor),
        ..spec("CASEFAN2", keys::FAN_CASE_2, 1.0, 8000.0)
    },
    Spec {
        note: Some(Code::DriveLetters),
        ..spec("DRVLOAD", "disk.{}:.percent", 1.0, 100.0)
    },
    spec("HDDTEMP", "disk.{}.temperature", 1.0, 100.0),
    spec("HDDUSED", "disk.{}.activity", 1.0, 100.0),
    Spec {
        decimals: None,
        ..spec("UPSPEED", keys::NET_UP, 1024.0, 100.0)
    },
    Spec {
        decimals: None,
        ..spec("DOWNDSPEED", keys::NET_DOWN, 1024.0, 100.0)
    },
    Spec {
        note: Some(Code::SensorNotSupported),
        ..spec("Volume", keys::SYSTEM_VOLUME, 1.0, 100.0)
    },
    Spec {
        note: Some(Code::VendorWeather),
        ..spec("Weather", "vendor.Weather", 1.0, 100.0)
    },
    spec("FPS", keys::GPU_FPS, 1.0, 100.0),
];

/// A resolved vendor sensor.
struct Sensor {
    key: SensorKey,
    min: f64,
    max: f64,
    decimals: Option<u8>,
}

/// A key from free text: whitespace becomes `_`.
fn sensor_key(text: &str) -> Option<SensorKey> {
    SensorKey::new(
        text.chars()
            .map(|c| if c.is_whitespace() { '_' } else { c })
            .collect::<String>(),
    )
}

fn fahrenheit_to_celsius(f: f64) -> f64 {
    (f - 32.0) / 1.8
}

/// Coordinates and lengths: finite and bounded.
fn px(v: f64) -> f32 {
    v.clamp(-100_000.0, 100_000.0) as f32
}

/// Imports a `.turtheme`. `video` returns the bytes of the background video
/// file named by the theme, when it can be found next to the theme.
pub fn import(
    bytes: &[u8],
    fallback_name: &str,
    video: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Result<Imported, String> {
    let graph =
        Graph::parse(bytes, &Limits::default(), &allowed_class).map_err(|e| e.to_string())?;
    let root = graph
        .class(graph.root())
        .filter(|c| c.name == "UsbMonitorL.Theme")
        .map(|class| Obj {
            graph: &graph,
            class,
        })
        .ok_or("the root object is not a UsbMonitorL.Theme")?;
    let (canvas, orientation) = canvas(root)?;
    let mut im = Importer {
        b: Builder::default(),
        fonts: BTreeSet::new(),
        bitmaps: HashMap::new(),
    };
    let items = graph_list(root);
    let (background, rest) = im.background(root, &items, canvas, video);
    for item in rest {
        im.element(*item);
    }
    im.report_fonts();
    let name = root
        .text("name")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(fallback_name);
    let theme = Theme {
        name: if name.is_empty() {
            "Imported theme".to_string()
        } else {
            name.to_string()
        },
        canvas,
        orientation,
        background,
        elements: im.b.elements,
        refresh_seconds: 1.0,
    };
    Ok((theme, im.b.assets, im.b.report))
}

/// Canvas size and orientation. The vendor's `isLanscape` flag means "the
/// panel's native orientation" (it is true for 480 x 1920 on the 8.8"), so
/// the orientation comes from the size itself.
fn canvas(root: Obj<'_>) -> Result<(Size, Orientation), String> {
    let side = |name| {
        root.int(name)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| (1..=16_384).contains(v))
    };
    match (side("width"), side("height")) {
        (Some(w), Some(h)) => {
            let orientation = if w > h {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            };
            Ok((Size::new(w, h), orientation))
        }
        _ => Err(format!(
            "bad canvas size {:?} x {:?}",
            root.int("width"),
            root.int("height")
        )),
    }
}

/// `GraphList` = `ObservableCollection { items: List { _items, _size } }`.
fn graph_list<'g>(root: Obj<'g>) -> Vec<Obj<'g>> {
    let Some(collection) = root.obj("GraphList") else {
        return Vec::new();
    };
    let list = collection.obj("items").unwrap_or(collection);
    let size = list
        .int("_size")
        .and_then(|s| usize::try_from(s).ok())
        .unwrap_or(0);
    let Some(Value::Object(id)) = list.class.field("_items") else {
        return Vec::new();
    };
    let Some(Object::Array(items)) = root.graph.object(*id) else {
        return Vec::new();
    };
    items
        .iter()
        .take(size)
        .filter_map(|v| Obj::new(root.graph, v))
        .collect()
}

/// A class instance with typed member access.
#[derive(Clone, Copy)]
struct Obj<'g> {
    graph: &'g Graph,
    class: &'g Class,
}

impl<'g> Obj<'g> {
    fn new(graph: &'g Graph, value: &Value) -> Option<Self> {
        match value {
            Value::Object(id) => graph.class(*id).map(|class| Obj { graph, class }),
            _ => None,
        }
    }

    fn prim(self, name: &str) -> Option<&'g Primitive> {
        match self.class.field(name)? {
            Value::Primitive(p) => Some(p),
            _ => None,
        }
    }

    fn num(self, name: &str) -> Option<f64> {
        self.prim(name)?.as_f64()
    }

    fn int(self, name: &str) -> Option<i64> {
        self.prim(name)?.as_i64()
    }

    fn flag(self, name: &str) -> bool {
        self.prim(name)
            .and_then(Primitive::as_bool)
            .unwrap_or(false)
    }

    fn text(self, name: &str) -> Option<&'g str> {
        match self.class.field(name)? {
            Value::Object(id) => match self.graph.object(*id)? {
                Object::String(s) => Some(s.as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    fn obj(self, name: &str) -> Option<Obj<'g>> {
        Obj::new(self.graph, self.class.field(name)?)
    }

    /// The bytes of a `System.Drawing.Bitmap` member and their object id.
    fn bitmap(self, name: &str) -> Option<(ObjectId, &'g [u8])> {
        match self.obj(name)?.class.field("Data")? {
            Value::Object(id) => match self.graph.object(*id)? {
                Object::Bytes(b) => Some((*id, b.as_slice())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Element kind: `TypeName`, or the class when it is missing.
    fn kind(self) -> &'g str {
        self.text("TypeName")
            .unwrap_or(match self.class.name.as_str() {
                "UsbMonitorL.GraphImage" => "Image",
                "UsbMonitorL.GraphAnimation" => "Animation",
                "UsbMonitorL.GraphStatuBar" => "StatuBar",
                "UsbMonitorL.GraphArchBar" => "ArchBar",
                "UsbMonitorL.GraphClock" => "Clock",
                "UsbMonitorL.GraphLine" => "Chart",
                _ => "Data",
            })
    }

    fn pos(self) -> (f32, f32) {
        (
            px(self.num("posX").unwrap_or(0.0)),
            px(self.num("posY").unwrap_or(0.0)),
        )
    }
}

/// Layer-list label of a data binding, in English.
fn label(md: Obj<'_>) -> String {
    let name = md
        .text("Sanma_Eng_Name")
        .filter(|n| !n.trim().is_empty())
        .or_else(|| md.text("DataName"))
        .unwrap_or("?");
    match md.text("SubName").filter(|s| !s.is_empty()) {
        Some(sub) => format!("{name} {sub}"),
        None => name.to_string(),
    }
}

fn short(text: &str) -> String {
    let one_line = text.lines().next().unwrap_or_default();
    let mut out: String = one_line.chars().take(24).collect();
    if one_line.chars().count() > 24 {
        out.push('…');
    }
    out
}

struct Importer {
    b: Builder,
    fonts: BTreeSet<String>,
    bitmaps: HashMap<ObjectId, AssetRef>,
}

impl Importer {
    fn warn(&mut self, warning: impl Into<ImportWarning>) {
        self.b.report.warn(warning.into());
    }

    /// Background from the first layer: a still image covering the canvas,
    /// or a video (with its poster). Returns the layers left to import.
    ///
    /// A video gets no framing: the vendor zeroes its own `crop`
    /// (`TransFormInfo`) and `direction` when it loads a theme, so the video
    /// is framed by Auto (D-2026-10-01-video-background-framing-2).
    fn background<'g, 'i>(
        &mut self,
        root: Obj<'g>,
        items: &'i [Obj<'g>],
        canvas: Size,
        video: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    ) -> (Background, &'i [Obj<'g>]) {
        let black = Background::Color(Rgba::BLACK);
        let Some((first, rest)) = items.split_first() else {
            self.warn(Code::NoLayers);
            return (black, items);
        };
        let kind = first.kind();
        if first.flag("hide") || !(kind == "Image" || kind == "Animation") {
            return (black, items);
        }
        let theme_video = root
            .text("videoName")
            .or_else(|| root.text("videoPath"))
            .and_then(file_name);
        if kind == "Animation" || theme_video.is_some() {
            let name = theme_video.or_else(|| {
                first
                    .text("videoName")
                    .or_else(|| first.text("FilePath"))
                    .and_then(file_name)
            });
            let poster = self.bitmap_asset(*first, "poster").map(|(a, _, _)| a);
            let background = match (name, poster) {
                (Some(name), poster) => Background::Video {
                    asset: self.video_asset(&name, video),
                    poster,
                    framing: None,
                },
                (None, Some(asset)) => {
                    self.warn(Code::VideoWithoutName);
                    Background::Image {
                        asset,
                        fit: Fit::Fill,
                    }
                }
                (None, None) => black,
            };
            return (background, rest);
        }
        let (x, y) = first.pos();
        let picture = self.bitmap_asset(*first, "background");
        match picture.and_then(|(asset, w, h)| Some((asset, covering_fit(x, y, (w, h), canvas)?))) {
            Some((asset, fit)) => (Background::Image { asset, fit }, rest),
            None => (black, items),
        }
    }

    fn video_asset(
        &mut self,
        name: &str,
        video: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    ) -> AssetRef {
        let path = format!("assets/{name}");
        match video(name) {
            Some(bytes) => self.b.asset(path, || bytes),
            None => {
                self.warn(
                    ImportWarning::new(Code::VideoNotFound)
                        .arg("name", name)
                        .arg("path", &path),
                );
                AssetRef(path)
            }
        }
    }

    /// The picture of an image-like layer as an asset, with its size.
    fn bitmap_asset<'g>(&mut self, item: Obj<'g>, role: &str) -> Option<(AssetRef, u32, u32)> {
        let Some((id, bytes)) = item.bitmap("bitmap").or_else(|| item.bitmap("O_bitmap")) else {
            self.warn(ImportWarning::new(Code::LayerWithoutPicture).arg("layer", role));
            return None;
        };
        let Some((w, h, ext)) = image_size(bytes) else {
            self.warn(ImportWarning::new(Code::UnsupportedPicture).arg("layer", role));
            return None;
        };
        let asset = match self.bitmaps.get(&id) {
            Some(asset) => asset.clone(),
            None => {
                let path = format!("assets/{role}-{}.{ext}", id.unsigned_abs());
                let asset = self.b.asset(path, || bytes.to_vec());
                self.bitmaps.insert(id, asset.clone());
                asset
            }
        };
        Some((asset, w, h))
    }

    fn element<'g>(&mut self, item: Obj<'g>) {
        let visible = !item.flag("hide");
        match item.kind() {
            "Text" | "Data" => self.text(item, visible),
            "Image" => self.image(item, visible),
            "Animation" => {
                self.warn(Code::VideoLayer);
                self.image(item, visible);
            }
            "StatuBar" => self.bar(item, visible),
            "ArchBar" => self.ring(item, visible),
            "Clock" => self.needle(item, visible),
            "Chart" => self.chart(item, visible),
            other => self.warn(ImportWarning::new(Code::UnknownLayer).arg("type", other)),
        }
    }

    fn image<'g>(&mut self, item: Obj<'g>, visible: bool) {
        let Some((asset, w, h)) = self.bitmap_asset(item, "image") else {
            return;
        };
        let (x, y) = item.pos();
        let name = match item.text("ImgName").filter(|n| !n.is_empty()) {
            Some(n) => format!("Image: {n}"),
            None => "Image".to_string(),
        };
        self.b.push(
            name,
            BoxF::new(x, y, w as f32, h as f32),
            visible,
            ElementKind::Image {
                asset,
                fit: Fit::Fill,
            },
        );
    }

    /// `Text` and `Data` layers: `posX` is the left edge, the centre or the
    /// right edge per the alignment; `posY` the top of the line box.
    fn text<'g>(&mut self, item: Obj<'g>, visible: bool) {
        let Some(fc) = item.obj("fontConfig") else {
            self.warn(Code::TextWithoutFont);
            return;
        };
        let md = item.obj("m_data");
        let data = md.and_then(|m| m.text("DataName")).unwrap_or("StaticText");
        let sample = md
            .and_then(|m| m.text("ValueWithUnit").or_else(|| m.text("_Value")))
            .unwrap_or_default();
        let (content, name, chars) = match md {
            Some(md) if item.kind() == "Data" && data != "StaticText" => {
                let Some(content) = self.data_content(md, item.flag("fahrenheit")) else {
                    return;
                };
                let chars = sample.chars().count().max(4) + 3;
                (content, format!("Data: {}", label(md)), chars)
            }
            _ => {
                let chars = sample.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let content = TextContent::Static(sample.to_string());
                (content, format!("Text: {}", short(sample)), chars)
            }
        };
        let style = self.text_style(fc);
        let lines = sample.lines().count().max(1) as f32;
        let width = text_width(chars, style.size);
        let height = line_height(style.size) * lines;
        let (x, y) = item.pos();
        let left = match style.align {
            HAlign::Left => x,
            HAlign::Center => x - width / 2.0,
            HAlign::Right => x - width,
        };
        self.b.push(
            name,
            BoxF::new(left, y, width, height),
            visible,
            ElementKind::Text { content, style },
        );
    }

    fn data_content<'g>(&mut self, md: Obj<'g>, fahrenheit: bool) -> Option<TextContent> {
        let data = md.text("DataName").unwrap_or_default();
        let sub = md.text("SubName");
        if let Some(pattern) = self.clock_pattern(data, sub) {
            return Some(TextContent::Clock {
                pattern,
                language: None,
                casing: Default::default(),
            });
        }
        let sensor = self.sensor(data, sub, fahrenheit)?;
        Some(TextContent::Sensor {
            key: sensor.key,
            format: DisplayFormat {
                decimals: sensor.decimals,
                show_unit: md.flag("ShowUnit"),
                temperature: if fahrenheit {
                    TemperatureUnit::Fahrenheit
                } else {
                    TemperatureUnit::Celsius
                },
                bytes: ByteUnits::Binary,
            },
            prefix: String::new(),
            suffix: String::new(),
        })
    }

    /// The vendor's date/time sub-formats (themes-turzx.md section 5.9) as
    /// Bezel clock patterns; `None` when `data` is not a clock source.
    fn clock_pattern(&mut self, data: &str, sub: Option<&str>) -> Option<String> {
        let sub = sub.unwrap_or_default();
        let known = |p: &str| Some(p.to_string());
        match (data, sub) {
            ("TIME", "h:m:s") => known("%H:%M:%S"),
            ("TIME", "" | "h:m" | "hm" | "HHmm") => known("%H:%M"),
            ("TIME", "m" | "mm") => known("%M"),
            ("TIME", "s" | "ss") => known("%S"),
            ("TIME", "h_24" | "HH") => known("%H"),
            ("TIME", "h_12" | "hh") => known("%I"),
            ("DATE", "" | "Y-M-D" | "yyyy-MM-dd") => known("%Y-%m-%d"),
            ("DATE", "Y" | "yyyy") => known("%Y"),
            ("DATE", "M" | "MM") => known("%m"),
            ("DATE", "M_en") => known("%b"),
            ("DATE", "D" | "dd") => known("%d"),
            ("DAY", "" | "Day_en") => known("%a"),
            ("APM", _) => known("%p"),
            ("DATE", "M_cn") => {
                self.warn(Code::ChineseMonths);
                known("%m月")
            }
            ("DAY", "Day_cn" | "Num_cn" | "Num") => {
                self.warn(ImportWarning::new(Code::WeekdayFormat).arg("format", sub));
                known("%a")
            }
            ("TIME" | "DATE" | "DAY", other) => {
                self.warn(
                    ImportWarning::new(Code::UnknownClockFormat)
                        .arg("field", data)
                        .arg("format", other),
                );
                known(match data {
                    "TIME" => "%H:%M",
                    "DATE" => "%Y-%m-%d",
                    _ => "%a",
                })
            }
            _ => None,
        }
    }

    fn sensor(&mut self, data: &str, sub: Option<&str>, fahrenheit: bool) -> Option<Sensor> {
        let Some(spec) = SENSORS.iter().find(|s| s.name == data) else {
            self.warn(ImportWarning::new(Code::UnknownDataSource).arg("source", data));
            let key = sensor_key(&format!("vendor.{data}"))?;
            return Some(Sensor {
                key,
                min: 0.0,
                max: 100.0,
                decimals: None,
            });
        };
        if let Some(note) = spec.note {
            self.warn(ImportWarning::new(note).arg("source", data));
        }
        let fallback = if data == "DRVLOAD" { "C" } else { "0" };
        let sub = sub
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback);
        let key = sensor_key(&spec.key.replace("{}", sub))?;
        let (min, max) = if spec.key.ends_with("temperature") && fahrenheit {
            (fahrenheit_to_celsius(0.0), fahrenheit_to_celsius(250.0))
        } else {
            (0.0, spec.full * spec.scale)
        };
        Some(Sensor {
            key,
            min,
            max,
            decimals: spec.decimals,
        })
    }

    /// The range bars, rings, needles and charts map to 0..=1. `full`
    /// overrides the vendor's full-scale value (charts carry their own).
    fn binding<'g>(&mut self, md: Obj<'g>, fahrenheit: bool, full: Option<f64>) -> Option<Binding> {
        let data = md.text("DataName").unwrap_or_default();
        let sub = md.text("SubName");
        let percent = |key: &str| {
            sensor_key(key).map(|key| Binding {
                key,
                min: 0.0,
                max: 100.0,
            })
        };
        match data {
            "RAM" | "RAM_GB" => return percent(keys::MEMORY_PERCENT),
            "RAMVALID" | "RAMVALID_GB" => return percent(keys::MEMORY_AVAILABLE_PERCENT),
            "TIME" | "DATE" | "DAY" | "APM" | "StaticText" => {
                self.warn(ImportWarning::new(Code::UnsupportedBinding).arg("source", data));
                return percent(&format!("vendor.{data}"));
            }
            _ => {}
        }
        let sensor = self.sensor(data, sub, fahrenheit)?;
        let scale = SENSORS
            .iter()
            .find(|s| s.name == data)
            .map_or(1.0, |s| s.scale);
        let max = full.map_or(sensor.max, |f| f * scale);
        Some(Binding {
            key: sensor.key,
            min: sensor.min,
            max,
        })
    }

    /// A color member: its default when the member is missing, `None` when
    /// it is `Color.Empty` or cannot be resolved.
    fn color_member<'g>(&mut self, item: Obj<'g>, name: &str, default: Rgba) -> Option<Rgba> {
        match item.obj(name) {
            Some(color) => self.color(color),
            None => Some(default),
        }
    }

    /// `System.Drawing.Color`: `state` 1 = known color, 2 = ARGB value,
    /// 8 = named color, 0 = `Color.Empty`.
    fn color<'g>(&mut self, c: Obj<'g>) -> Option<Rgba> {
        let state = c.int("state").unwrap_or(0);
        if state & 1 != 0 {
            let index = c.int("knownColor").unwrap_or(0);
            return match colors::known_color(index) {
                Some(Known::Web(rgba)) => Some(rgba),
                Some(Known::System(rgba)) => {
                    self.warn(Code::SystemColors);
                    Some(rgba)
                }
                None => {
                    self.warn(
                        ImportWarning::new(Code::UnknownKnownColor).arg("index", index.to_string()),
                    );
                    None
                }
            };
        }
        if state & 2 != 0 {
            let argb = u32::try_from(c.int("value").unwrap_or(0) & 0xffff_ffff).unwrap_or(0);
            return Some(colors::from_argb(argb));
        }
        if state & 8 != 0 {
            let name = c.text("name").unwrap_or_default();
            let found = colors::named_color(name);
            if found.is_none() {
                self.warn(ImportWarning::new(Code::UnknownColorName).arg("color", name));
            }
            return found;
        }
        None
    }

    fn text_style<'g>(&mut self, fc: Obj<'g>) -> TextStyle {
        let family = fc
            .text("name")
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Microsoft YaHei")
            .to_string();
        self.fonts.insert(family.clone());
        let points = fc
            .num("size")
            .filter(|s| *s > 0.0)
            .unwrap_or(24.0)
            .min(1000.0);
        let color = match fc.obj("color") {
            Some(c) => self.color(c).unwrap_or(TRANSPARENT),
            None => TEXT_GRAY,
        };
        let spacing = fc.num("interval").unwrap_or(0.0).clamp(-100.0, 100.0);
        let paint = self.text_paint(fc, color, spacing);
        let align = match fc.obj("alignment").and_then(|a| a.int("index")) {
            Some(1) => HAlign::Center,
            Some(2) => HAlign::Right,
            _ => HAlign::Left,
        };
        TextStyle {
            font: FontSpec {
                family,
                weight: if fc.flag("isBold") { 700 } else { 400 },
                italic: false,
                asset: None,
            },
            size: (points * 96.0 / 72.0) as f32,
            paint,
            align,
            valign: VAlign::Top,
            letter_spacing: spacing as f32,
        }
    }

    /// `GrDirection` 1..=4 = `LinearGradientMode` 0..=3; the vendor ignores
    /// it when letter spacing is on.
    fn text_paint<'g>(&mut self, fc: Obj<'g>, color: Rgba, spacing: f64) -> Paint {
        let angle = match fc.int("GrDirection") {
            Some(1) => 0.0,
            Some(2) => 90.0,
            Some(3) => 45.0,
            Some(4) => 135.0,
            _ => return Paint::Solid(color),
        };
        let second = fc.obj("GrColor").and_then(|c| self.color(c));
        match second {
            Some(second) if spacing == 0.0 => Paint::Linear {
                angle,
                stops: vec![(0.0, color), (1.0, second)],
            },
            _ => Paint::Solid(color),
        }
    }

    fn report_fonts(&mut self) {
        if !self.fonts.is_empty() {
            let list = self.fonts.iter().cloned().collect::<Vec<_>>().join(", ");
            self.warn(ImportWarning::new(Code::FontsNotStored).arg("fonts", list));
        }
    }

    fn data_of<'g>(&mut self, item: Obj<'g>, what: &str) -> Option<Obj<'g>> {
        let md = item.obj("m_data");
        if md.is_none() {
            self.warn(ImportWarning::new(Code::LayerWithoutData).arg("layer", what));
        }
        md
    }

    /// `StatuBar`: `width` is the length and `height` the thickness in every
    /// direction; the vendor's gradient runs across the thickness.
    fn bar<'g>(&mut self, item: Obj<'g>, visible: bool) {
        let Some(md) = self.data_of(item, "bar") else {
            return;
        };
        let Some(binding) = self.binding(md, item.flag("fahrenheit"), None) else {
            return;
        };
        let (x, y) = item.pos();
        let length = px(item.num("width").unwrap_or(200.0).max(0.0));
        let thickness = px(item.num("height").unwrap_or(30.0).max(0.0));
        let direction = item.int("direction").unwrap_or(0);
        let (frame, direction, angle) = match direction {
            1 => (
                BoxF::new(x, y, length, thickness),
                Direction::RightToLeft,
                90.0,
            ),
            2 => (
                BoxF::new(x, y, thickness, length),
                Direction::BottomToTop,
                0.0,
            ),
            3 => (
                BoxF::new(x, y, thickness, length),
                Direction::TopToBottom,
                0.0,
            ),
            _ => (
                BoxF::new(x, y, length, thickness),
                Direction::LeftToRight,
                90.0,
            ),
        };
        let front = self
            .color_member(item, "FrontColor", PURPLE)
            .unwrap_or(TRANSPARENT);
        let back = self.color_member(item, "BackColor", Rgba::BLACK);
        let fill = match self.color_member(item, "GradientColor", DEEP_PINK) {
            Some(second) if item.flag("useGradient") => Paint::Linear {
                angle,
                stops: vec![(0.0, front), (1.0, second)],
            },
            _ => Paint::Solid(front),
        };
        let transparent = item.flag("trBack");
        let track = if !transparent || item.flag("fillBack") {
            back.map(Paint::Solid)
        } else {
            None
        };
        let radius = px(item.num("radius").unwrap_or(0.0).max(0.0));
        let border = px(item.num("lineWidth").unwrap_or(0.0).max(0.0));
        let name = format!("Bar: {}", label(md));
        if let Some(back) = back.filter(|_| !transparent && border > 0.0) {
            let outer = BoxF::new(
                frame.x - border,
                frame.y - border,
                frame.width + 2.0 * border,
                frame.height + 2.0 * border,
            );
            self.b.push(
                format!("{name} border"),
                outer,
                visible,
                ElementKind::Shape {
                    shape: ShapeKind::Rect { radius },
                    fill: Some(Paint::Solid(back)),
                    stroke: None,
                },
            );
        }
        if item.flag("revert") {
            self.warn(Code::InvertedBar);
        }
        let segments = item.flag("useSubsection").then(|| Segments {
            count: 20,
            gap: (length / 100.0).floor().max(1.0),
        });
        self.b.push(
            name,
            frame,
            visible,
            ElementKind::Bar {
                binding,
                direction,
                fill,
                track,
                radius,
                segments,
            },
        );
    }

    /// `ArchBar`: a pen of `archWidth` (rounded up to odd) centred on the
    /// circle of `diameter`; start at `startPer`% of a turn from 12 o'clock.
    fn ring<'g>(&mut self, item: Obj<'g>, visible: bool) {
        let Some(md) = self.data_of(item, "ring") else {
            return;
        };
        let Some(binding) = self.binding(md, item.flag("fahrenheit"), None) else {
            return;
        };
        let (x, y) = item.pos();
        let diameter = px(item.num("diameter").unwrap_or(200.0).max(0.0));
        let width = item
            .num("archWidth")
            .unwrap_or(20.0)
            .clamp(1.0, 10_000.0)
            .ceil();
        let pen = px(if width % 2.0 == 0.0 {
            width + 1.0
        } else {
            width
        });
        let start = item.num("startPer").unwrap_or(0.0).clamp(0.0, 100.0) * 3.6;
        let sweep = match item.num("totalAngel").unwrap_or(0.0) {
            s if s <= 0.0 => 360.0,
            s => s.min(360.0),
        };
        let reverse = item.flag("revert");
        let fill = self
            .color_member(item, "FrontColor", PURPLE)
            .unwrap_or(TRANSPARENT);
        let track = if item.flag("trBack") {
            None
        } else {
            self.color_member(item, "BackColor", LIGHT_GRAY)
                .map(Paint::Solid)
        };
        let segments = item.flag("useBlock").then(|| Segments {
            count: ((sweep / 19.0).round() as u16).max(1),
            gap: 4.0,
        });
        self.b.push(
            format!("Ring: {}", label(md)),
            BoxF::new(x - pen / 2.0, y - pen / 2.0, diameter + pen, diameter + pen),
            visible,
            ElementKind::Ring {
                test_full: false,
                binding,
                start_angle: px(if reverse { start + sweep } else { start }),
                sweep: px(sweep),
                thickness: pen,
                clockwise: !reverse,
                fill: Paint::Solid(fill),
                track,
                cap: if item.flag("round") {
                    Cap::Round
                } else {
                    Cap::Butt
                },
                segments,
            },
        );
    }

    /// `Clock`: the needle picture is drawn at `(posX, posY)` from the pivot
    /// `(centerX, centerY)` in a frame rotated by
    /// `angle + (Rate - offset) * endAngle` degrees. The element box is the
    /// picture at rest, so the pivot may lie outside it; the picture keeps
    /// the direction it was drawn in (it is not re-oriented to 12 o'clock).
    fn needle<'g>(&mut self, item: Obj<'g>, visible: bool) {
        let Some(md) = self.data_of(item, "needle") else {
            return;
        };
        let Some(binding) = self.binding(md, item.flag("fahrenheit"), None) else {
            return;
        };
        let Some((asset, w, h)) = self.bitmap_asset(item, "needle") else {
            return;
        };
        let (dx, dy) = item.pos();
        let cx = px(item.num("centerX").unwrap_or(240.0));
        let cy = px(item.num("centerY").unwrap_or(240.0));
        let (w, h) = (w as f32, h as f32);
        let angle = item.num("angle").unwrap_or(0.0);
        let end = item.num("endAngle").unwrap_or(0.0);
        let offset = item.num("offset").unwrap_or(0.0);
        let (start, sweep) = if item.flag("revert") {
            (angle + offset * end, -end)
        } else {
            (angle - offset * end, end)
        };
        self.b.push(
            format!("Needle: {}", label(md)),
            BoxF::new(cx + dx, cy + dy, w, h),
            visible,
            ElementKind::Needle {
                binding,
                asset: Some(asset),
                pivot: (-dx / w, -dy / h),
                start_angle: px(start),
                sweep: px(sweep),
                color: Rgba::WHITE,
                width: 3.0,
            },
        );
    }

    /// `Chart`: an area graph of `_width / columnWidth` samples, full at
    /// `maxValue`, with an optional border.
    fn chart<'g>(&mut self, item: Obj<'g>, visible: bool) {
        let Some(md) = self.data_of(item, "chart") else {
            return;
        };
        let full = item.num("maxValue").filter(|m| *m > 0.0).unwrap_or(100.0);
        let Some(binding) = self.binding(md, item.flag("fahrenheit"), Some(full)) else {
            return;
        };
        let (x, y) = item.pos();
        let width = item.num("_width").unwrap_or(300.0).max(0.0);
        let height = item.num("_height").unwrap_or(200.0).max(0.0);
        let column = item.num("columnWidth").filter(|c| *c > 0.0).unwrap_or(5.0);
        let history = md
            .int("queueLen")
            .filter(|q| *q > 1)
            .map_or((width / column).floor(), |q| q as f64)
            .clamp(2.0, 4096.0) as u16;
        let line = self
            .color_member(item, "LineColor", SKY_BLUE)
            .unwrap_or(TRANSPARENT);
        let fill = self.color_member(item, "FillColor", Rgba { a: 40, ..SKY_BLUE });
        let frame = BoxF::new(x, y, px(width), px(height));
        let name = format!("Chart: {}", label(md));
        if item.flag("rollDirection") {
            self.warn(Code::ChartDirection);
        }
        self.b.push(
            name.clone(),
            frame,
            visible,
            ElementKind::Graph {
                binding,
                history,
                style: if fill.is_some_and(|c| c.a > 0) {
                    GraphStyle::Area
                } else {
                    GraphStyle::Line
                },
                color: line,
                fill: fill.map(Paint::Solid),
                line_width: px(item.num("lineWidth").unwrap_or(1.0).max(0.0)),
                autoscale: false,
            },
        );
        let border = px(item.num("borderWidth").unwrap_or(1.0).max(0.0));
        if let Some(color) = self
            .color_member(item, "BorderColor", SKY_BLUE)
            .filter(|_| border > 0.0)
        {
            self.b.push(
                format!("{name} border"),
                frame,
                visible,
                ElementKind::Shape {
                    shape: ShapeKind::Rect { radius: 0.0 },
                    fill: None,
                    stroke: Some((color, border)),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests;
