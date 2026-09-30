//! Text: shaped with cosmic-text (HarfBuzz-compatible shaping, fallback to
//! other fonts for missing glyphs), rasterized with swash into a coverage
//! mask, then filled with the element's paint so gradients cover the text.
//!
//! Fonts come from the theme's bundled font assets first, then the installed
//! family, then a default sans.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bezel_core::domain::theme::{AssetRef, BoxF, FontSpec, HAlign, TextStyle, VAlign};
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent,
    SwashImage, Weight, Wrap,
};
use tiny_skia::{IntSize, Mask, Pixmap, Rect};

use crate::composite::premultiply;
use crate::diagnostics::{Diagnostics, Stamp};
use crate::layer::Layer;
use crate::paint::paint_for;

/// Whether the renderer may use the fonts installed on the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SystemFonts {
    /// Scan and use the installed fonts (the default for the app).
    #[default]
    Load,
    /// Only bundled fonts: reproducible output (tests, golden images).
    Skip,
}

/// Line height as a multiple of the font size.
const LINE_HEIGHT: f32 = 1.2;
/// Largest font size drawn, pixels.
const MAX_FONT_SIZE: f32 = 1024.0;
/// Families tried, in order, for the default sans.
const DEFAULT_FAMILIES: [&str; 10] = [
    "Inter",
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Open Sans",
    "Roboto",
    "Cantarell",
    "Segoe UI",
    "Arial",
    "Helvetica",
];

struct AssetFont {
    stamp: Stamp,
    ids: Vec<fontdb::ID>,
    family: Option<String>,
}

/// A text to draw.
pub(crate) struct TextJob<'a> {
    /// The text (may hold several lines).
    pub text: &'a str,
    /// How it looks.
    pub style: &'a TextStyle,
    /// The element box it is aligned in and clipped to.
    pub area: BoxF,
}

/// Fonts, shaping and glyph rasterization, cached across frames.
pub(crate) struct TextEngine {
    fonts: FontSystem,
    glyphs: SwashCache,
    default_family: Option<String>,
    installed: HashMap<String, Option<String>>,
    asset_fonts: HashMap<AssetRef, AssetFont>,
}

fn has_family(db: &fontdb::Database, name: &str) -> Option<String> {
    db.faces()
        .flat_map(|face| face.families.iter())
        .find(|(family, _)| family.eq_ignore_ascii_case(name))
        .map(|(family, _)| family.clone())
}

impl TextEngine {
    /// An engine with `fonts` (font file bytes) and optionally the installed fonts.
    pub fn new(fonts: Vec<Vec<u8>>, system: SystemFonts) -> Self {
        let mut db = fontdb::Database::new();
        if system == SystemFonts::Load {
            db.load_system_fonts();
        }
        for bytes in fonts {
            db.load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
        }
        let default_family = DEFAULT_FAMILIES
            .iter()
            .find_map(|name| has_family(&db, name))
            .or_else(|| {
                db.faces()
                    .find_map(|face| face.families.first().map(|(f, _)| f.clone()))
            });
        if let Some(family) = &default_family {
            db.set_sans_serif_family(family.clone());
        }
        Self {
            fonts: FontSystem::new_with_locale_and_db("en-US".to_string(), db),
            glyphs: SwashCache::new(),
            default_family,
            installed: HashMap::new(),
            asset_fonts: HashMap::new(),
        }
    }

    fn installed(&mut self, family: &str) -> Option<String> {
        let key = family.to_lowercase();
        if let Some(hit) = self.installed.get(&key) {
            return hit.clone();
        }
        let found = has_family(self.fonts.db(), family);
        self.installed.insert(key, found.clone());
        found
    }

    fn asset_family(&mut self, asset: &AssetRef, bytes: &[u8]) -> Option<String> {
        if let Some(font) = self.asset_fonts.get_mut(asset)
            && font.stamp.matches(bytes)
        {
            return font.family.clone();
        }
        let db = self.fonts.db_mut();
        for id in self
            .asset_fonts
            .remove(asset)
            .into_iter()
            .flat_map(|f| f.ids)
        {
            db.remove_face(id);
        }
        let ids: Vec<fontdb::ID> = db
            .load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())))
            .into_iter()
            .collect();
        let family = ids
            .first()
            .and_then(|id| db.face(*id))
            .and_then(|face| face.families.first())
            .map(|(f, _)| f.clone());
        self.installed.clear();
        let font = AssetFont {
            stamp: Stamp::new(bytes),
            ids,
            family: family.clone(),
        };
        self.asset_fonts.insert(asset.clone(), font);
        family
    }

    /// The family to shape with: the bundled asset, the installed family or
    /// the default sans.
    fn family_for(
        &mut self,
        font: &FontSpec,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        diagnostics: &mut Diagnostics,
    ) -> Option<String> {
        if let Some(asset) = &font.asset {
            let family = assets.get(asset).and_then(|b| self.asset_family(asset, b));
            if family.is_some() {
                return family;
            }
            diagnostics.warn(format!(
                "font asset {} is missing or unreadable; using {}",
                asset.0, font.family
            ));
        }
        if let Some(family) = self.installed(&font.family) {
            return Some(family);
        }
        let fallback = match &self.default_family {
            Some(family) => format!("{family:?}"),
            None => "no font".to_string(),
        };
        diagnostics.warn(format!(
            "font family {:?} is not available; using {fallback}",
            font.family
        ));
        self.default_family.clone()
    }

    /// Family names of every loaded font, sorted without duplicates.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fonts
            .db()
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// Draws `job` into `layer`.
    pub fn draw(
        &mut self,
        layer: &mut Layer<'_>,
        job: &TextJob<'_>,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        diagnostics: &mut Diagnostics,
    ) {
        let size = job.style.size.min(MAX_FONT_SIZE);
        if job.text.is_empty() || size.is_nan() || size < 1.0 {
            return;
        }
        let Some(family) = self.family_for(&job.style.font, assets, diagnostics) else {
            diagnostics.warn("no font is available to draw text".to_string());
            return;
        };
        let buffer = self.shape(job, &family, size);
        let Some((mask, ink, color_glyphs)) = self.rasterize(layer, &buffer, job) else {
            return;
        };
        if let (Some(paint), Some(ink)) = (paint_for(&job.style.paint, job.area), ink) {
            layer.fill_rect_masked(ink, &paint, &mask);
        }
        for (pixmap, x, y) in color_glyphs {
            layer.blit(pixmap.as_ref(), x, y);
        }
    }

    fn shape(&mut self, job: &TextJob<'_>, family: &str, size: f32) -> Buffer {
        let style = job.style;
        let metrics = Metrics::new(size, (size * LINE_HEIGHT).ceil());
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        buffer.set_wrap(Wrap::None);
        buffer.set_size(None, None);
        let mut attrs = Attrs::new()
            .family(Family::Name(family))
            .weight(Weight(style.font.weight.clamp(1, 1000)))
            .style(if style.font.italic {
                Style::Italic
            } else {
                Style::Normal
            });
        if style.letter_spacing.is_finite() && style.letter_spacing != 0.0 {
            attrs = attrs.letter_spacing(style.letter_spacing / size);
        }
        buffer.set_text(job.text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);
        buffer
    }

    /// Glyph coverage in a mask the size of the layer, plus color glyphs
    /// (emoji) as premultiplied images with their canvas position.
    fn rasterize(
        &mut self,
        layer: &Layer<'_>,
        buffer: &Buffer,
        job: &TextJob<'_>,
    ) -> Option<Rasterized> {
        let (w, h) = (layer.pixmap.width(), layer.pixmap.height());
        let mut coverage = Coverage {
            data: vec![0; w as usize * h as usize],
            width: w as i32,
            height: h as i32,
            ink: None,
        };
        let mut color_glyphs = Vec::new();
        let area = job.area;
        let height: f32 = buffer.layout_runs().map(|run| run.line_height).sum();
        let top = area.y
            + match job.style.valign {
                VAlign::Top => 0.0,
                VAlign::Middle => (area.height - height) / 2.0,
                VAlign::Bottom => area.height - height,
            };
        for run in buffer.layout_runs() {
            let left = area.x
                + match job.style.align {
                    HAlign::Left => 0.0,
                    HAlign::Center => (area.width - run.line_w) / 2.0,
                    HAlign::Right => area.width - run.line_w,
                };
            for glyph in run.glyphs {
                let physical = glyph.physical((left, top + run.line_y), 1.0);
                let Some(image) = self.glyphs.get_image(&mut self.fonts, physical.cache_key) else {
                    continue;
                };
                let x = physical.x + image.placement.left;
                let y = physical.y - image.placement.top;
                match image.content {
                    SwashContent::Mask => coverage.add(image, x - layer.x, y - layer.y),
                    SwashContent::Color => {
                        color_glyphs.extend(color_glyph(image).map(|p| (p, x, y)))
                    }
                    SwashContent::SubpixelMask => {}
                }
            }
        }
        let ink = coverage.ink.and_then(|(l, t, r, b)| {
            let (x, y) = (layer.x + l, layer.y + t);
            Rect::from_xywh(x as f32, y as f32, (r - l) as f32, (b - t) as f32)
        });
        let mask = Mask::from_vec(coverage.data, IntSize::from_wh(w, h)?)?;
        Some((mask, ink, color_glyphs))
    }
}

/// Glyph coverage over the layer, the canvas rectangle that holds ink and
/// the color glyphs with their canvas position.
type Rasterized = (Mask, Option<Rect>, Vec<(Pixmap, i32, i32)>);

/// Glyph coverage accumulated over the layer.
struct Coverage {
    data: Vec<u8>,
    width: i32,
    height: i32,
    /// Layer pixels written, `(left, top, right, bottom)` exclusive.
    ink: Option<(i32, i32, i32, i32)>,
}

impl Coverage {
    /// Adds a glyph mask whose top-left is at layer `(x, y)`.
    fn add(&mut self, image: &SwashImage, x: i32, y: i32) {
        let gw = image.placement.width as i32;
        let gh = image.placement.height as i32;
        let (c0, c1) = ((-x).max(0), gw.min(self.width - x));
        let (r0, r1) = ((-y).max(0), gh.min(self.height - y));
        if c0 >= c1 || r0 >= r1 {
            return;
        }
        let (l, t, r, b) = (x + c0, y + r0, x + c1, y + r1);
        self.ink = Some(match self.ink {
            None => (l, t, r, b),
            Some((l0, t0, r0, b0)) => (l0.min(l), t0.min(t), r0.max(r), b0.max(b)),
        });
        for row in r0..r1 {
            let src = (row * gw + c0) as usize..(row * gw + c1) as usize;
            let dst = ((y + row) * self.width + x + c0) as usize;
            let (Some(src), Some(dst)) = (
                image.data.get(src),
                self.data.get_mut(dst..dst + (c1 - c0) as usize),
            ) else {
                return;
            };
            for (d, &s) in dst.iter_mut().zip(src) {
                // Union of coverages: a + b - ab.
                let sum = u32::from(*d) + u32::from(s);
                *d = (sum - (u32::from(*d) * u32::from(s) + 127) / 255) as u8;
            }
        }
    }
}

/// A color glyph as a premultiplied image.
fn color_glyph(image: &SwashImage) -> Option<Pixmap> {
    let size = IntSize::from_wh(image.placement.width, image.placement.height)?;
    let mut data = image.data.clone();
    premultiply(&mut data);
    Pixmap::from_vec(data, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FONT;

    #[test]
    fn families_resolve_to_assets_installed_or_default() {
        let mut engine = TextEngine::new(vec![FONT.to_vec()], SystemFonts::Skip);
        let mut diagnostics = Diagnostics::default();
        let assets = BTreeMap::from([
            (AssetRef("assets/font.ttf".into()), FONT.to_vec()),
            (AssetRef("assets/bad.ttf".into()), b"not a font".to_vec()),
        ]);
        let spec = |family: &str, asset: Option<&str>| FontSpec {
            family: family.into(),
            asset: asset.map(|a| AssetRef(a.into())),
            ..FontSpec::default()
        };
        let default = Some("JetBrains Mono NL".to_string());
        assert_eq!(engine.default_family, default);
        let installed =
            engine.family_for(&spec("jetbrains mono nl", None), &assets, &mut diagnostics);
        assert_eq!(installed, default);
        assert_eq!(diagnostics.count(), 0);
        let bundled = engine.family_for(
            &spec("Nope", Some("assets/font.ttf")),
            &assets,
            &mut diagnostics,
        );
        assert_eq!(bundled, default);
        assert_eq!(
            engine.asset_family(&AssetRef("assets/font.ttf".into()), FONT),
            default
        );
        let unknown = engine.family_for(
            &spec("Nope", Some("assets/bad.ttf")),
            &assets,
            &mut diagnostics,
        );
        assert_eq!(unknown, default);
        assert!(diagnostics.mentions("unreadable"));
        assert!(diagnostics.mentions("not available"));

        let mut empty = TextEngine::new(Vec::new(), SystemFonts::Skip);
        assert_eq!(
            empty.family_for(&spec("Inter", None), &assets, &mut diagnostics),
            None
        );
    }

    #[test]
    fn coverage_is_a_clipped_union() {
        let mut coverage = Coverage {
            data: vec![0; 4],
            width: 2,
            height: 2,
            ink: None,
        };
        let mut image = SwashImage::new();
        image.placement.width = 2;
        image.placement.height = 2;
        image.data = vec![255, 128, 128, 0];
        coverage.add(&image, 1, 1);
        assert_eq!(coverage.data, vec![0, 0, 0, 255]);
        assert_eq!(coverage.ink, Some((1, 1, 2, 2)));
        coverage.add(&image, 0, 0);
        coverage.add(&image, 0, 0);
        assert_eq!(coverage.data, vec![255, 192, 192, 255]);
        assert_eq!(coverage.ink, Some((0, 0, 2, 2)));
        coverage.add(&image, 5, 0);
        coverage.add(&image, -2, 0);
        assert_eq!(coverage.data, vec![255, 192, 192, 255]);
        image.content = SwashContent::Color;
        image.data = [255, 0, 0, 128].repeat(4);
        let pixmap = color_glyph(&image).expect("glyph");
        assert_eq!(
            pixmap.pixel(0, 0).map(|p| (p.red(), p.alpha())),
            Some((128, 128))
        );
    }
}
