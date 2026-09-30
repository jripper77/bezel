//! turing-smart-screen-python themes: a folder with `theme.yaml` and its
//! images; fonts live in the Python repository's `res/fonts/` and the ones a
//! theme references are bundled with it
//! (docs/reverse-engineering/themes-python-yaml.md).
//!
//! The Python app paints every widget over a copy of the background picture
//! (or a solid box) because it cannot blend; Bezel composes with real alpha,
//! so `BACKGROUND_IMAGE` means "transparent" and a missing one becomes a
//! solid shape under the widget.

use std::path::{Path, PathBuf};

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::sensor::{ByteUnits, DisplayFormat, SensorKey, TemperatureUnit};
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Cap, Direction, ElementKind, Fit, FontSpec, GraphStyle,
    HAlign, Paint, Segments, ShapeKind, TextContent, TextStyle, Theme, VAlign,
};

use super::colors::named_color;
use super::yaml::{self, Node};
use super::{
    Builder, Imported, covering_fit, image_size, line_height, read_inside, read_limited,
    relative_inside, text_width,
};
use crate::color::from_hex;

/// Font of text widgets without `FONT`.
const DEFAULT_FONT: &str = "roboto-mono/RobotoMono-Regular.ttf";
/// Largest image or font bundled.
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MIB: f64 = 1024.0 * 1024.0;
const NOT_MEASURED: &str = "Bezel does not measure this yet; the widget shows it as unavailable";

/// Imports the theme folder `dir` (its `theme.yaml`).
pub fn import_dir(dir: &Path) -> Result<Imported, String> {
    import_file(&dir.join("theme.yaml"), dir)
}

/// Imports `file`, resolving images against `dir` and fonts against the
/// Python repository layout (`<repo>/res/themes/<theme>` → `<repo>/res/fonts`).
pub fn import_file(file: &Path, dir: &Path) -> Result<Imported, String> {
    let bytes = read_limited(file, yaml::MAX_BYTES as u64)?;
    let text = String::from_utf8(bytes).map_err(|_| "theme.yaml is not UTF-8".to_string())?;
    let doc = yaml::parse(&text)?;
    if !matches!(doc, Node::Map(_)) {
        return Err("theme.yaml is not a mapping".into());
    }
    let fonts = [dir.join("../../fonts"), dir.join("fonts")]
        .into_iter()
        .find(|p| p.is_dir());
    let name = dir
        .canonicalize()
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Imported theme".to_string());
    let mut im = Importer {
        dir: dir.to_path_buf(),
        fonts,
        b: Builder::default(),
        canvas: Size::new(320, 480),
        background: None,
        intervals: Vec::new(),
    };
    let (canvas, orientation) = im.display(doc.get("display"));
    im.canvas = canvas;
    // The Python app draws the static images, then the static texts, then
    // the widgets, whatever the order of the keys in the file.
    if let Some(images) = doc.get("static_images") {
        im.static_images(images);
    }
    if let Some(texts) = doc.get("static_text") {
        im.static_text(texts);
    }
    if let Some(stats) = doc.get("STATS") {
        im.stats(stats);
    }
    for (key, _) in doc.entries() {
        if !["display", "author", "static_images", "static_text", "STATS"].contains(&key) {
            im.warn(format!("the top-level key {key} is not used"));
        }
    }
    let refresh = im
        .intervals
        .iter()
        .copied()
        .filter(|i| *i > 0.0)
        .reduce(f64::min)
        .unwrap_or(1.0)
        .clamp(0.1, 3600.0);
    let theme = Theme {
        name,
        canvas,
        orientation,
        background: im
            .background
            .take()
            .unwrap_or(Background::Color(Rgba::BLACK)),
        elements: im.b.elements,
        refresh_seconds: refresh as f32,
    };
    Ok((theme, im.b.assets, im.b.report))
}

/// Logical portrait size of each `DISPLAY_SIZE` (`library/display.py`).
fn panel(size: &str) -> Option<Size> {
    Some(match size.trim().trim_end_matches('"').trim() {
        "0.96" => Size::new(80, 160),
        "2.1" | "2.8" => Size::new(480, 480),
        "3.5" => Size::new(320, 480),
        "4.6" => Size::new(320, 960),
        "5" => Size::new(480, 800),
        "5.2" => Size::new(720, 1280),
        "8" => Size::new(800, 1280),
        "8.8" | "9.2" => Size::new(480, 1920),
        "12.3" => Size::new(720, 1920),
        _ => return None,
    })
}

/// A color as Pillow reads it: `r, g, b` text, `[r, g, b]`, `#rgb`,
/// `#rrggbb`, `rgb(r, g, b)` or a CSS name. Alpha is dropped like Pillow's
/// RGB conversion does.
fn parse_color(node: &Node) -> Option<Rgba> {
    let channels = |parts: Vec<Option<f64>>| -> Option<Rgba> {
        let [r, g, b] = parts.try_into().ok()?;
        let c = |v: Option<f64>| v.map(|v| v.clamp(0.0, 255.0) as u8);
        Some(Rgba::opaque(c(r)?, c(g)?, c(b)?))
    };
    match node {
        Node::Seq(items) if items.len() == 3 => channels(items.iter().map(Node::as_f64).collect()),
        Node::Str(text) => {
            let t = text.trim();
            if t.starts_with('#') {
                return from_hex(t).map(|c| Rgba { a: 255, ..c });
            }
            let inner = t
                .strip_prefix("rgb(")
                .and_then(|r| r.strip_suffix(')'))
                .unwrap_or(t);
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 3 {
                return channels(parts.iter().map(|p| p.trim().parse().ok()).collect());
            }
            named_color(t)
        }
        _ => None,
    }
}

/// Splits a font file stem into family, weight and italic
/// (`JetBrainsMono-Bold` → `JetBrainsMono`, 700).
fn font_face(stem: &str) -> (String, u16, bool) {
    let lower = stem.to_ascii_lowercase();
    let weight = [
        ("extrabold", 800),
        ("semibold", 600),
        ("extralight", 200),
        ("black", 900),
        ("bold", 700),
        ("medium", 500),
        ("light", 300),
        ("thin", 100),
    ]
    .iter()
    .find(|(word, _)| lower.contains(word))
    .map_or(400, |(_, w)| *w);
    let italic = lower.contains("italic");
    let family = match stem.rsplit_once('-') {
        Some((family, style))
            if [
                "regular",
                "bold",
                "black",
                "medium",
                "light",
                "thin",
                "italic",
                "extrabold",
                "semibold",
                "extralight",
                "bolditalic",
            ]
            .contains(&style.to_ascii_lowercase().as_str()) =>
        {
            family
        }
        _ => stem,
    };
    (family.to_string(), weight, italic)
}

/// A path under a base as an asset path with `/` separators.
fn slash_path(prefix: &str, rel: &Path) -> String {
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    format!("{prefix}/{}", parts.join("/"))
}

/// A Bezel sensor for a Python metric.
struct Source {
    key: String,
    /// Multiplier from the theme's `MIN_VALUE`/`MAX_VALUE` unit to Bezel's.
    scale: f64,
    decimals: Option<u8>,
    bytes: ByteUnits,
    /// Characters of a typical value (for the text box).
    chars: usize,
    /// Clock kind for `DATE` widgets: `Some(true)` = date, `Some(false)` = time.
    clock: Option<bool>,
    note: Option<&'static str>,
}

fn source(path: &[String], widget: &str) -> Option<Source> {
    let p: Vec<&str> = path.iter().map(String::as_str).collect();
    let text = !matches!(widget, "GRAPH" | "RADIAL" | "LINE_GRAPH");
    let plain = |key: &str| Source {
        key: key.to_string(),
        scale: 1.0,
        decimals: Some(0),
        bytes: ByteUnits::Binary,
        chars: 4,
        clock: None,
        note: None,
    };
    let sized = |key: &str, scale: f64| Source {
        scale,
        decimals: None,
        chars: 9,
        ..plain(key)
    };
    let noted = |key: &str, note: &'static str| Source {
        note: Some(note),
        decimals: None,
        chars: 8,
        ..plain(key)
    };
    let decimal = |key: &str| Source {
        scale: 1e9,
        bytes: ByteUnits::Decimal,
        ..sized(key, 1.0)
    };
    let clock = |date: bool| Source {
        clock: Some(date),
        chars: 12,
        ..plain("clock")
    };
    Some(match (p.as_slice(), widget) {
        (["CPU", "PERCENTAGE"], _) => plain("cpu.usage"),
        (["CPU", "FREQUENCY"], _) => sized("cpu.frequency", 1000.0),
        (["CPU", "LOAD", n], _) => Source {
            decimals: None,
            ..plain(match *n {
                "ONE" => "cpu.load.1",
                "FIVE" => "cpu.load.5",
                "FIFTEEN" => "cpu.load.15",
                _ => return None,
            })
        },
        (["CPU", "TEMPERATURE"], _) => plain("cpu.temperature"),
        (["CPU", "FAN_SPEED"], _) => plain("cpu.fan.percent"),
        (["GPU", "PERCENTAGE"], _) => plain("gpu.usage"),
        (["GPU", "MEMORY_PERCENT"], _) => plain("gpu.memory.percent"),
        (["GPU", "MEMORY"], _) if text => sized("gpu.memory.used", MIB),
        (["GPU", "MEMORY"], _) => plain("gpu.memory.percent"),
        (["GPU", "MEMORY_USED"], _) => sized("gpu.memory.used", MIB),
        (["GPU", "MEMORY_TOTAL"], _) => sized("gpu.memory.total", MIB),
        (["GPU", "TEMPERATURE"], _) => plain("gpu.temperature"),
        (["GPU", "FPS"], _) => noted("gpu.fps", NOT_MEASURED),
        (["GPU", "FAN_SPEED"], _) => plain("gpu.fan.percent"),
        (["GPU", "FREQUENCY"], _) => sized("gpu.frequency", 1000.0),
        (["MEMORY", "SWAP"], _) => plain("memory.swap.percent"),
        (["MEMORY", "VIRTUAL"], "USED") => sized("memory.used", MIB),
        (["MEMORY", "VIRTUAL"], "FREE") => sized("memory.available", MIB),
        (["MEMORY", "VIRTUAL"], "TOTAL") => sized("memory.total", MIB),
        (["MEMORY", "VIRTUAL"], _) => plain("memory.percent"),
        (["DISK", "USED"], "TEXT") => decimal("disk./.used"),
        (["DISK", "USED"], _) => plain("disk./.percent"),
        (["DISK", "TOTAL"], _) => decimal("disk./.total"),
        (["DISK", "FREE"], _) => decimal("disk./.free"),
        (["NET", "WLO" | "ETH", metric], _) => sized(
            match *metric {
                "UPLOAD" => "net.up",
                "DOWNLOAD" => "net.down",
                "UPLOADED" => "net.up.total",
                "DOWNLOADED" => "net.down.total",
                _ => return None,
            },
            1.0,
        ),
        (["DATE", "DAY"], _) => clock(true),
        (["DATE", "HOUR"], _) => clock(false),
        (["UPTIME", "SECONDS" | "FORMATTED"], _) => Source {
            chars: 10,
            ..plain("system.uptime")
        },
        (["WEATHER", what], _) => noted(
            &format!("weather.{}", what.to_ascii_lowercase()),
            "weather is not supported yet",
        ),
        (["PING"], _) => noted("net.ping", NOT_MEASURED),
        (["CUSTOM", class], _) => noted(
            &format!("custom.{class}"),
            "custom Python data classes cannot run in Bezel",
        ),
        _ => return None,
    })
}

/// Keys each widget kind reads; anything else is reported.
fn known_keys(kind: &str) -> &'static [&'static str] {
    match kind {
        "GRAPH" => &[
            "SHOW",
            "X",
            "Y",
            "WIDTH",
            "HEIGHT",
            "MIN_VALUE",
            "MAX_VALUE",
            "BAR_COLOR",
            "BAR_OUTLINE",
            "BACKGROUND_COLOR",
            "BACKGROUND_IMAGE",
            "REVERSE_DIRECTION",
        ],
        "RADIAL" => &[
            "SHOW",
            "X",
            "Y",
            "RADIUS",
            "WIDTH",
            "MIN_VALUE",
            "MAX_VALUE",
            "ANGLE_START",
            "ANGLE_END",
            "ANGLE_STEPS",
            "ANGLE_SEP",
            "CLOCKWISE",
            "BAR_COLOR",
            "BAR_BACKGROUND_COLOR",
            "DRAW_BAR_BACKGROUND",
            "BAR_DECORATION",
            "SHOW_TEXT",
            "SHOW_UNIT",
            "FONT",
            "FONT_SIZE",
            "FONT_COLOR",
            "BACKGROUND_COLOR",
            "BACKGROUND_IMAGE",
            "CUSTOM_BBOX",
            "TEXT_OFFSET",
            "MIN_SIZE",
        ],
        "LINE_GRAPH" => &[
            "SHOW",
            "X",
            "Y",
            "WIDTH",
            "HEIGHT",
            "MIN_VALUE",
            "MAX_VALUE",
            "HISTORY_SIZE",
            "AUTOSCALE",
            "LINE_COLOR",
            "LINE_WIDTH",
            "AXIS",
            "AXIS_COLOR",
            "AXIS_FONT",
            "AXIS_FONT_SIZE",
            "BACKGROUND_COLOR",
            "BACKGROUND_IMAGE",
        ],
        "static_text" => &[
            "TEXT",
            "X",
            "Y",
            "WIDTH",
            "HEIGHT",
            "FONT",
            "FONT_SIZE",
            "FONT_COLOR",
            "BACKGROUND_COLOR",
            "BACKGROUND_IMAGE",
            "ALIGN",
            "ANCHOR",
        ],
        "static_images" => &["PATH", "X", "Y", "WIDTH", "HEIGHT"],
        _ => &[
            "SHOW",
            "SHOW_UNIT",
            "MIN_SIZE",
            "X",
            "Y",
            "WIDTH",
            "HEIGHT",
            "FONT",
            "FONT_SIZE",
            "FONT_COLOR",
            "BACKGROUND_COLOR",
            "BACKGROUND_IMAGE",
            "ALIGN",
            "ANCHOR",
            "FORMAT",
        ],
    }
}

struct Importer {
    dir: PathBuf,
    fonts: Option<PathBuf>,
    b: Builder,
    canvas: Size,
    background: Option<Background>,
    intervals: Vec<f64>,
}

impl Importer {
    fn warn(&mut self, message: impl Into<String>) {
        self.b.report.warn(message);
    }

    fn num(block: &Node, key: &str, default: f64) -> f64 {
        block.get(key).and_then(Node::as_f64).unwrap_or(default)
    }

    fn flag(block: &Node, key: &str, default: bool) -> bool {
        block.get(key).and_then(Node::as_bool).unwrap_or(default)
    }

    fn text_of(block: &Node, key: &str) -> Option<String> {
        block.get(key).and_then(Node::as_text)
    }

    fn coord(block: &Node, key: &str) -> f32 {
        Self::num(block, key, 0.0).clamp(-100_000.0, 100_000.0) as f32
    }

    fn color(&mut self, block: &Node, key: &str, default: Rgba, name: &str) -> Rgba {
        match block.get(key) {
            None | Some(Node::Null) => default,
            Some(node) => parse_color(node).unwrap_or_else(|| {
                self.warn(format!(
                    "{name}: the {key} {node:?} is not a color; a default is used"
                ));
                default
            }),
        }
    }

    fn check_keys(&mut self, name: &str, block: &Node, kind: &str) {
        let known = known_keys(kind);
        for (key, _) in block.entries() {
            if !known.contains(&key) {
                self.warn(format!("{name}: {key} is not used"));
            }
        }
    }

    fn display(&mut self, display: Option<&Node>) -> (Size, Orientation) {
        let display = display.cloned().unwrap_or(Node::Map(Vec::new()));
        let size = Self::text_of(&display, "DISPLAY_SIZE");
        let panel = match size.as_deref() {
            None => Size::new(320, 480),
            Some(s) => panel(s).unwrap_or_else(|| {
                self.warn(format!("the display size {s} is unknown; 3.5\" is used"));
                Size::new(320, 480)
            }),
        };
        let orientation = match Self::text_of(&display, "DISPLAY_ORIENTATION")
            .map(|o| o.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("portrait") => Orientation::Portrait,
            Some("landscape") => Orientation::Landscape,
            Some("reverse_portrait") => Orientation::ReversePortrait,
            Some("reverse_landscape") => Orientation::ReverseLandscape,
            other => {
                self.warn(format!(
                    "the display orientation {other:?} is unknown; portrait is used"
                ));
                Orientation::Portrait
            }
        };
        if let Some(led) = display.get("DISPLAY_RGB_LED")
            && parse_color(led) != Some(Rgba::WHITE)
        {
            self.warn("the backplate LED color (XuanFang rev B) is not part of a Bezel theme");
        }
        (panel.in_orientation(orientation), orientation)
    }

    fn static_images(&mut self, images: &Node) {
        for (name, block) in images.entries() {
            let label = format!("static_images.{name}");
            self.check_keys(&label, block, "static_images");
            let Some(path) = Self::text_of(block, "PATH") else {
                self.warn(format!("{label}: no PATH; dropped"));
                continue;
            };
            let Some(rel) = relative_inside(&path) else {
                self.warn(format!(
                    "{label}: the path {path:?} leaves the theme folder; dropped"
                ));
                continue;
            };
            let bytes = match read_inside(&self.dir, &rel, MAX_FILE) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.warn(format!("{label}: {e}; dropped"));
                    continue;
                }
            };
            let (x, y) = (Self::coord(block, "X"), Self::coord(block, "Y"));
            let (w, h) = (Self::coord(block, "WIDTH"), Self::coord(block, "HEIGHT"));
            let size = match image_size(&bytes) {
                _ if w > 0.0 && h > 0.0 => (w, h),
                Some((iw, ih, _)) => (iw as f32, ih as f32),
                None => {
                    self.warn(format!(
                        "{label}: {path} is not a PNG, GIF or JPEG; dropped"
                    ));
                    continue;
                }
            };
            let asset = self.b.asset(slash_path("assets", &rel), || bytes);
            let pixels = (size.0 as u32, size.1 as u32);
            let fit = covering_fit(x, y, pixels, self.canvas)
                .filter(|fit| *fit == Fit::Fill || w <= 0.0 || h <= 0.0);
            if let Some(fit) =
                fit.filter(|_| self.background.is_none() && self.b.elements.is_empty())
            {
                self.background = Some(Background::Image { asset, fit });
            } else {
                let frame = BoxF::new(x, y, size.0, size.1);
                let kind = ElementKind::Image {
                    asset,
                    fit: Fit::Fill,
                };
                self.b.push(format!("Image: {name}"), frame, true, kind);
            }
        }
    }

    fn font(&mut self, block: &Node, key: &str) -> FontSpec {
        let path = Self::text_of(block, key).unwrap_or_else(|| DEFAULT_FONT.to_string());
        let stem = Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (family, weight, italic) = font_face(&stem);
        FontSpec {
            family,
            weight,
            italic,
            asset: self.font_asset(&path),
        }
    }

    /// Bundles a font of the Python repository's `res/fonts/`.
    fn font_asset(&mut self, path: &str) -> Option<AssetRef> {
        let Some(rel) = relative_inside(path) else {
            self.warn(format!(
                "the font path {path:?} leaves the fonts folder; not bundled"
            ));
            return None;
        };
        let asset = AssetRef(slash_path("assets/fonts", &rel));
        if self.b.assets.contains_key(&asset) {
            return Some(asset);
        }
        let Some(root) = self.fonts.clone() else {
            self.warn("the Python fonts folder (res/fonts) was not found; fonts are not bundled");
            return None;
        };
        match read_inside(&root, &rel, MAX_FILE) {
            Ok(bytes) => Some(self.b.asset(asset.0, || bytes)),
            Err(e) => {
                self.warn(format!("the font {path} was not bundled: {e}"));
                None
            }
        }
    }

    fn static_text(&mut self, texts: &Node) {
        for (name, block) in texts.entries() {
            let label = format!("static_text.{name}");
            self.check_keys(&label, block, "static_text");
            let text = Self::text_of(block, "TEXT").unwrap_or_default();
            if text.is_empty() {
                self.warn(format!("{label}: no TEXT; dropped"));
                continue;
            }
            let chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
            let lines = text.lines().count().max(1);
            let content = TextContent::Static(text);
            self.text_box(&format!("Text: {name}"), block, content, chars, lines);
        }
    }

    /// A text widget placed like Pillow's `draw.text` with `ANCHOR`, or in
    /// a fixed box when `WIDTH` (and `HEIGHT`) are set, over a solid box
    /// when there is no `BACKGROUND_IMAGE`.
    fn text_box(
        &mut self,
        name: &str,
        block: &Node,
        content: TextContent,
        chars: usize,
        lines: usize,
    ) {
        let size = Self::num(block, "FONT_SIZE", 10.0).clamp(1.0, 1000.0) as f32;
        let font = self.font(block, "FONT");
        let color = self.color(block, "FONT_COLOR", Rgba::BLACK, name);
        let anchor = Self::text_of(block, "ANCHOR").unwrap_or_else(|| "lt".into());
        let mut anchor_chars = anchor.trim().chars();
        let horizontal = anchor_chars.next().unwrap_or('l');
        let vertical = anchor_chars.last().unwrap_or('t');
        let (x, y) = (Self::coord(block, "X"), Self::coord(block, "Y"));
        let mut w = Self::coord(block, "WIDTH");
        let mut h = Self::coord(block, "HEIGHT");
        if w > 0.0 && h <= 0.0 {
            h = size;
        }
        let align = match horizontal {
            'm' => HAlign::Center,
            'r' => HAlign::Right,
            _ => HAlign::Left,
        };
        let valign = match vertical {
            'm' => VAlign::Middle,
            'b' | 'd' => VAlign::Bottom,
            _ => VAlign::Top,
        };
        let frame = if w > 0.0 && h > 0.0 {
            BoxF::new(x, y, w, h)
        } else {
            w = text_width(chars, size);
            h = line_height(size) * lines as f32;
            let left = match align {
                HAlign::Left => x,
                HAlign::Center => x - w / 2.0,
                HAlign::Right => x - w,
            };
            let top = match vertical {
                'm' => y - h / 2.0,
                's' => y - (size * 0.8).round(),
                'b' | 'd' => y - h,
                _ => y,
            };
            BoxF::new(left, top, w, h)
        };
        let align = if lines > 1 {
            match Self::text_of(block, "ALIGN").as_deref() {
                Some("center") => HAlign::Center,
                Some("right") => HAlign::Right,
                _ => HAlign::Left,
            }
        } else {
            align
        };
        self.solid_box(name, block, frame, Rgba::WHITE);
        let style = TextStyle {
            font,
            size,
            paint: Paint::Solid(color),
            align,
            valign,
            letter_spacing: 0.0,
        };
        self.b
            .push(name, frame, true, ElementKind::Text { content, style });
    }

    /// The box the Python app paints under a widget without a background
    /// picture.
    fn solid_box(&mut self, name: &str, block: &Node, frame: BoxF, default: Rgba) {
        if block
            .get("BACKGROUND_IMAGE")
            .is_some_and(|b| *b != Node::Null)
        {
            return;
        }
        let color = self.color(block, "BACKGROUND_COLOR", default, name);
        self.b.push(
            format!("{name} background"),
            frame,
            true,
            ElementKind::Shape {
                shape: ShapeKind::Rect { radius: 0.0 },
                fill: Some(Paint::Solid(color)),
                stroke: None,
            },
        );
    }

    fn stats(&mut self, stats: &Node) {
        let mut path = Vec::new();
        self.walk(&mut path, stats, 0.0);
    }

    /// Walks the `STATS` tree: a map with `SHOW` is a widget, anything else
    /// a group; `INTERVAL` applies to everything under it.
    fn walk(&mut self, path: &mut Vec<String>, node: &Node, interval: f64) {
        let interval = node
            .get("INTERVAL")
            .and_then(Node::as_f64)
            .unwrap_or(interval);
        for (key, child) in node.entries() {
            if key == "INTERVAL" {
                continue;
            }
            if child.get("SHOW").is_some() {
                self.widget(path, key, child, interval);
            } else if let Node::Map(_) = child {
                path.push(key.to_string());
                self.walk(path, child, interval);
                path.pop();
            } else {
                let at = path.join(".");
                self.warn(format!("STATS.{at}: {key} is not used"));
            }
        }
    }

    fn widget(&mut self, path: &[String], kind: &str, block: &Node, interval: f64) {
        if !Self::flag(block, "SHOW", false) {
            return;
        }
        let name = format!("{}.{kind}", path.join("."));
        let Some(source) = source(path, kind) else {
            self.warn(format!("STATS.{name}: unknown sensor; dropped"));
            return;
        };
        if let Some(note) = source.note {
            self.warn(format!("STATS.{name}: {note}"));
        }
        self.check_keys(&format!("STATS.{name}"), block, kind);
        let Some(key) = SensorKey::new(source.key.replace(char::is_whitespace, "_")) else {
            return;
        };
        self.intervals.push(interval);
        let min = Self::num(block, "MIN_VALUE", 0.0) * source.scale;
        let max = Self::num(block, "MAX_VALUE", 100.0) * source.scale;
        let binding = Binding { key, min, max };
        match kind {
            "GRAPH" => self.bar(&name, block, binding),
            "RADIAL" => self.radial(&name, block, binding, &source),
            "LINE_GRAPH" => self.line_graph(&name, block, binding),
            _ => {
                let content = self.content(block, binding.key, &source);
                self.text_box(&name, block, content, source.chars, 1);
            }
        }
    }

    fn content(&mut self, block: &Node, key: SensorKey, source: &Source) -> TextContent {
        if let Some(date) = source.clock {
            let format = Self::text_of(block, "FORMAT").unwrap_or_else(|| "medium".into());
            return TextContent::Clock {
                pattern: self.cldr(&format, date),
            };
        }
        TextContent::Sensor {
            key,
            format: DisplayFormat {
                decimals: source.decimals,
                show_unit: Self::flag(block, "SHOW_UNIT", true),
                temperature: TemperatureUnit::Celsius,
                bytes: source.bytes,
            },
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    /// Babel's named formats (en_US) and CLDR patterns as Bezel clock
    /// patterns; fields Bezel cannot show are left out and reported.
    fn cldr(&mut self, format: &str, date: bool) -> String {
        let named = match (format.trim(), date) {
            ("short", true) => Some("%m/%d/%y"),
            ("medium", true) => Some("%b %e, %Y"),
            ("long", true) => Some("%B %e, %Y"),
            ("full", true) => Some("%A, %B %e, %Y"),
            ("short", false) => Some("%I:%M %p"),
            ("medium" | "long" | "full", false) => Some("%I:%M:%S %p"),
            _ => None,
        };
        if let Some(pattern) = named {
            return pattern.to_string();
        }
        let chars: Vec<char> = format.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let run = chars[i..].iter().take_while(|x| **x == c).count();
            i += run;
            if c == '\'' {
                if run >= 2 {
                    out.push_str(&"'".repeat(run / 2));
                    if run % 2 == 0 {
                        continue;
                    }
                }
                while let Some(q) = chars.get(i) {
                    i += 1;
                    match (*q, chars.get(i)) {
                        ('\'', Some('\'')) => {
                            out.push('\'');
                            i += 1;
                        }
                        ('\'', _) => break,
                        (q, _) => out.push(q),
                    }
                }
                continue;
            }
            let piece = match c {
                'y' | 'Y' | 'u' if run == 2 => "%y",
                'y' | 'Y' | 'u' => "%Y",
                'M' | 'L' if run <= 2 => "%m",
                'M' | 'L' if run == 3 => "%b",
                'M' | 'L' => "%B",
                'd' if run == 1 => "%e",
                'd' => "%d",
                'E' | 'e' | 'c' if run >= 4 => "%A",
                'E' | 'e' | 'c' => "%a",
                'H' | 'k' => "%H",
                'h' | 'K' => "%I",
                'm' => "%M",
                's' => "%S",
                'a' => "%p",
                '%' => "%%",
                c if c.is_ascii_alphabetic() => {
                    let field: String = std::iter::repeat_n(c, run).collect();
                    self.warn(format!(
                        "the date/time field {field} of {format:?} is not supported and was left out"
                    ));
                    ""
                }
                _ => {
                    out.extend(std::iter::repeat_n(c, run));
                    ""
                }
            };
            out.push_str(piece);
        }
        out.trim().to_string()
    }

    /// `GRAPH`: horizontal when wider than tall, else filling bottom-up.
    fn bar(&mut self, name: &str, block: &Node, binding: Binding) {
        let (x, y) = (Self::coord(block, "X"), Self::coord(block, "Y"));
        let (w, h) = (Self::coord(block, "WIDTH"), Self::coord(block, "HEIGHT"));
        if w <= 0.0 || h <= 0.0 {
            self.warn(format!("STATS.{name}: a bar without a size was dropped"));
            return;
        }
        let reverse = Self::flag(block, "REVERSE_DIRECTION", false);
        let direction = match (w > h, reverse) {
            (true, false) => Direction::LeftToRight,
            (true, true) => Direction::RightToLeft,
            (false, false) => Direction::BottomToTop,
            (false, true) => Direction::TopToBottom,
        };
        let color = self.color(block, "BAR_COLOR", Rgba::BLACK, name);
        let has_picture = block
            .get("BACKGROUND_IMAGE")
            .is_some_and(|b| *b != Node::Null);
        let track = if has_picture {
            None
        } else {
            Some(Paint::Solid(self.color(
                block,
                "BACKGROUND_COLOR",
                Rgba::WHITE,
                name,
            )))
        };
        let frame = BoxF::new(x, y, w, h);
        self.b.push(
            name,
            frame,
            true,
            ElementKind::Bar {
                binding,
                direction,
                fill: Paint::Solid(color),
                track,
                radius: 0.0,
                segments: None,
            },
        );
        if Self::flag(block, "BAR_OUTLINE", false) {
            self.b.push(
                format!("{name} outline"),
                frame,
                true,
                ElementKind::Shape {
                    shape: ShapeKind::Rect { radius: 0.0 },
                    fill: None,
                    stroke: Some((color, 1.0)),
                },
            );
        }
    }

    /// `RADIAL`: angles are Pillow's (0 = 3 o'clock, clockwise) reduced
    /// modulo 361 like the Python app does; Bezel counts from 12 o'clock.
    fn radial(&mut self, name: &str, block: &Node, binding: Binding, source: &Source) {
        let (cx, cy) = (Self::coord(block, "X"), Self::coord(block, "Y"));
        let radius = Self::num(block, "RADIUS", 1.0).clamp(0.0, 100_000.0) as f32;
        if radius <= 0.0 {
            self.warn(format!(
                "STATS.{name}: a radial bar without a radius was dropped"
            ));
            return;
        }
        let thickness = (Self::num(block, "WIDTH", 1.0).clamp(1.0, 100_000.0) as f32).min(radius);
        let start = Self::num(block, "ANGLE_START", 0.0).rem_euclid(361.0);
        let end = Self::num(block, "ANGLE_END", 360.0).rem_euclid(361.0);
        let clockwise = Self::flag(block, "CLOCKWISE", false);
        let sweep = if start == end {
            360.0
        } else if clockwise {
            if end < start {
                360.0 - start + end
            } else {
                end - start
            }
        } else if end < start {
            start - end
        } else {
            360.0 - end + start
        };
        let steps = Self::num(block, "ANGLE_STEPS", 1.0).clamp(1.0, 360.0) as u16;
        let sep = Self::num(block, "ANGLE_SEP", 0.0).clamp(0.0, 360.0) as f32;
        let segments = (sep > 0.0).then_some(Segments {
            count: steps,
            gap: sep,
        });
        let fill = self.color(block, "BAR_COLOR", Rgba::BLACK, name);
        let track = Self::flag(block, "DRAW_BAR_BACKGROUND", false)
            .then(|| self.color(block, "BAR_BACKGROUND_COLOR", Rgba::BLACK, name))
            .map(Paint::Solid);
        let cap = match Self::text_of(block, "BAR_DECORATION")
            .as_deref()
            .map(str::trim)
        {
            Some("Ellipse") => Cap::Round,
            None | Some("" | "Rectangle") => Cap::Butt,
            Some(other) => {
                self.warn(format!(
                    "STATS.{name}: the bar decoration {other:?} is unknown"
                ));
                Cap::Butt
            }
        };
        let frame = BoxF::new(cx - radius, cy - radius, 2.0 * radius, 2.0 * radius);
        let bbox: Vec<f32> = match block.get("CUSTOM_BBOX") {
            Some(Node::Seq(v)) => v
                .iter()
                .filter_map(Node::as_f64)
                .map(|v| v as f32)
                .collect(),
            _ => Vec::new(),
        };
        let square = match bbox.as_slice() {
            [x0, y0, x1, y1] if (x0, y0, x1, y1) != (&0.0, &0.0, &0.0, &0.0) => {
                BoxF::new(frame.x + x0, frame.y + y0, x1 - x0, y1 - y0)
            }
            _ => frame,
        };
        self.solid_box(name, block, square, Rgba::BLACK);
        let key = binding.key.clone();
        self.b.push(
            name,
            frame,
            true,
            ElementKind::Ring {
                binding,
                start_angle: ((start + 90.0).rem_euclid(360.0)) as f32,
                sweep: sweep as f32,
                thickness,
                clockwise,
                fill: Paint::Solid(fill),
                track,
                cap,
                segments,
            },
        );
        if Self::flag(block, "SHOW_TEXT", false) {
            self.radial_text(name, block, key, source, (cx, cy));
        }
    }

    fn radial_text(
        &mut self,
        name: &str,
        block: &Node,
        key: SensorKey,
        source: &Source,
        center: (f32, f32),
    ) {
        let offset: Vec<f32> = match block.get("TEXT_OFFSET") {
            Some(Node::Seq(v)) => v
                .iter()
                .filter_map(Node::as_f64)
                .map(|v| v as f32)
                .collect(),
            _ => Vec::new(),
        };
        let (dx, dy) = match offset.as_slice() {
            [dx, dy] => (*dx, *dy),
            _ => (0.0, 0.0),
        };
        let size = Self::num(block, "FONT_SIZE", 10.0).clamp(1.0, 1000.0) as f32;
        let (w, h) = (text_width(source.chars, size), line_height(size));
        let style = TextStyle {
            font: self.font(block, "FONT"),
            size,
            paint: Paint::Solid(self.color(block, "FONT_COLOR", Rgba::BLACK, name)),
            align: HAlign::Center,
            valign: VAlign::Middle,
            letter_spacing: 0.0,
        };
        let content = self.content(block, key, source);
        self.b.push(
            format!("{name} text"),
            BoxF::new(center.0 + dx - w / 2.0, center.1 + dy - h / 2.0, w, h),
            true,
            ElementKind::Text { content, style },
        );
    }

    fn line_graph(&mut self, name: &str, block: &Node, binding: Binding) {
        let frame = BoxF::new(
            Self::coord(block, "X"),
            Self::coord(block, "Y"),
            Self::num(block, "WIDTH", 1.0).clamp(1.0, 100_000.0) as f32,
            Self::num(block, "HEIGHT", 1.0).clamp(1.0, 100_000.0) as f32,
        );
        if Self::flag(block, "AXIS", false) {
            self.warn(format!(
                "STATS.{name}: line graph axes and labels are not supported"
            ));
        }
        self.solid_box(name, block, frame, Rgba::BLACK);
        let color = self.color(block, "LINE_COLOR", Rgba::BLACK, name);
        self.b.push(
            name,
            frame,
            true,
            ElementKind::Graph {
                binding,
                history: Self::num(block, "HISTORY_SIZE", 10.0).clamp(2.0, 4096.0) as u16,
                style: GraphStyle::Line,
                color,
                fill: None,
                line_width: Self::num(block, "LINE_WIDTH", 2.0).clamp(0.0, 1000.0) as f32,
                autoscale: Self::flag(block, "AUTOSCALE", false),
            },
        );
    }
}

#[cfg(test)]
mod tests;
