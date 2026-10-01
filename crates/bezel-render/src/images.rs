//! Image assets: decoded once (PNG, JPEG, animated GIF), then cropped and
//! resampled once per target size, kept premultiplied and ready to blit.
//! Entries a frame did not use are dropped after it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use bezel_core::domain::animation::Timeline;
use bezel_core::domain::theme::{AssetRef, BoxF, Fit};
use image::codecs::gif::GifDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, ImageFormat, RgbaImage};
use tiny_skia::{IntSize, Pixmap};

use crate::composite::{clamp_premultiplied, premultiply};
use crate::diagnostics::{Diagnostics, Stamp};

/// Largest image the renderer will hold for one element (pixels).
const MAX_PIXELS: u64 = 8192 * 8192;
/// Budget for the decoded frames of one animation (bytes).
const MAX_ANIMATION_BYTES: usize = 256 << 20;
/// GIF frames with a delay this short or shorter play at 100 ms, as browsers do.
const MIN_GIF_DELAY_MS: u32 = 10;
/// Delay used for such frames.
const DEFAULT_GIF_DELAY_MS: u32 = 100;

/// One decoded frame, premultiplied.
struct DecodedFrame {
    pixels: RgbaImage,
    delay_ms: u32,
}

/// A decoded image or animation.
struct Decoded {
    width: u32,
    height: u32,
    frames: Vec<DecodedFrame>,
    /// When each frame shows (`None`: a still image).
    timeline: Option<Timeline>,
}

impl Decoded {
    fn new(frames: Vec<DecodedFrame>) -> Option<Self> {
        let first = frames.first()?;
        let delays = frames
            .iter()
            .map(|f| Duration::from_millis(u64::from(f.delay_ms)))
            .collect();
        Some(Self {
            width: first.pixels.width(),
            height: first.pixels.height(),
            timeline: Timeline::new(delays),
            frames,
        })
    }
}

/// An image ready to draw at its target size (every frame of an animation).
pub(crate) struct Scaled {
    frames: Vec<Pixmap>,
    timeline: Option<Timeline>,
}

impl Scaled {
    /// The frame shown `animation` after the theme started (animations
    /// loop; the core's [`Timeline`] picks it).
    pub fn frame_at(&self, animation: Duration) -> Option<&Pixmap> {
        let index = self.timeline.as_ref().map_or(0, |t| t.frame_at(animation));
        self.frames.get(index).or(self.frames.first())
    }
}

/// Where and how big an image is drawn for a fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Plan {
    /// Source rectangle used `(x, y, width, height)`.
    pub crop: (u32, u32, u32, u32),
    /// Drawn size in pixels.
    pub size: (u32, u32),
    /// Canvas position of the drawn image's top-left corner.
    pub at: (f32, f32),
}

fn px(v: f32) -> u32 {
    v.round().max(1.0) as u32
}

/// How an `image`-sized picture fills `area` with `fit`. Only the visible
/// part is kept (cover crops the source, none crops to the box).
pub(crate) fn plan(fit: Fit, image: (u32, u32), area: BoxF) -> Option<Plan> {
    let (iw, ih) = (image.0 as f32, image.1 as f32);
    let valid = |v: f32| v.is_finite() && v >= 0.5;
    if !(valid(iw)
        && valid(ih)
        && valid(area.width)
        && valid(area.height)
        && area.x.is_finite()
        && area.y.is_finite())
    {
        return None;
    }
    let full = (0, 0, image.0, image.1);
    let at = (area.x, area.y);
    let plan = match fit {
        Fit::Fill => Plan {
            crop: full,
            size: (px(area.width), px(area.height)),
            at,
        },
        Fit::Contain => {
            let s = (area.width / iw).min(area.height / ih);
            let (w, h) = (iw * s, ih * s);
            let at = (
                area.x + (area.width - w) / 2.0,
                area.y + (area.height - h) / 2.0,
            );
            Plan {
                crop: full,
                size: (px(w), px(h)),
                at,
            }
        }
        Fit::Cover => {
            let s = (area.width / iw).max(area.height / ih);
            let cw = px(area.width / s).min(image.0);
            let ch = px(area.height / s).min(image.1);
            let crop = ((image.0 - cw) / 2, (image.1 - ch) / 2, cw, ch);
            Plan {
                crop,
                size: (px(area.width), px(area.height)),
                at,
            }
        }
        Fit::None => {
            let cw = (area.width.ceil() as u32).clamp(1, image.0);
            let ch = (area.height.ceil() as u32).clamp(1, image.1);
            Plan {
                crop: (0, 0, cw, ch),
                size: (cw, ch),
                at,
            }
        }
    };
    Some(plan)
}

fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    if format == ImageFormat::Gif {
        return decode_gif(bytes);
    }
    let mut pixels = image::load_from_memory_with_format(bytes, format)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    premultiply(&mut pixels);
    let still = DecodedFrame {
        pixels,
        delay_ms: 0,
    };
    Decoded::new(vec![still]).ok_or_else(|| "the image has no pixels".to_string())
}

fn decode_gif(bytes: &[u8]) -> Result<Decoded, String> {
    let decoder = GifDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut budget = MAX_ANIMATION_BYTES;
    for frame in decoder.into_frames() {
        let frame = frame.map_err(|e| e.to_string())?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay = numer.checked_div(denom).unwrap_or(0);
        let delay_ms = if delay <= MIN_GIF_DELAY_MS {
            DEFAULT_GIF_DELAY_MS
        } else {
            delay
        };
        let mut pixels = frame.into_buffer();
        let cost = pixels.as_raw().len();
        if cost > budget && !frames.is_empty() {
            break;
        }
        budget = budget.saturating_sub(cost);
        premultiply(&mut pixels);
        frames.push(DecodedFrame { pixels, delay_ms });
    }
    Decoded::new(frames).ok_or_else(|| "the GIF has no frames".to_string())
}

fn scale(source: &Decoded, plan: &Plan) -> Option<Scaled> {
    let (cx, cy, cw, ch) = plan.crop;
    let (w, h) = plan.size;
    let mut frames = Vec::with_capacity(source.frames.len());
    for frame in &source.frames {
        let view = imageops::crop_imm(&frame.pixels, cx, cy, cw, ch);
        let resized = if (cw, ch) == (w, h) {
            view.to_image()
        } else {
            imageops::resize(&*view, w, h, FilterType::CatmullRom)
        };
        let mut raw = resized.into_raw();
        clamp_premultiplied(&mut raw);
        frames.push(Pixmap::from_vec(raw, IntSize::from_wh(w, h)?)?);
    }
    Some(Scaled {
        frames,
        timeline: source.timeline.clone(),
    })
}

struct SourceEntry {
    stamp: Stamp,
    image: Option<Arc<Decoded>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ScaledKey {
    asset: AssetRef,
    crop: (u32, u32, u32, u32),
    size: (u32, u32),
}

/// Decoded and resampled images, reused across frames.
#[derive(Default)]
pub(crate) struct ImageCache {
    sources: HashMap<AssetRef, SourceEntry>,
    scaled: HashMap<ScaledKey, Arc<Scaled>>,
    used_sources: HashSet<AssetRef>,
    used_scaled: HashSet<ScaledKey>,
}

impl ImageCache {
    fn source(
        &mut self,
        asset: &AssetRef,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        diagnostics: &mut Diagnostics,
    ) -> Option<Arc<Decoded>> {
        let Some(bytes) = assets.get(asset) else {
            diagnostics.warn(format!("image asset {} is missing", asset.0));
            return None;
        };
        self.used_sources.insert(asset.clone());
        if let Some(entry) = self.sources.get_mut(asset)
            && entry.stamp.matches(bytes)
        {
            return entry.image.clone();
        }
        self.scaled.retain(|k, _| &k.asset != asset);
        let image = match decode(bytes) {
            Ok(decoded) => Some(Arc::new(decoded)),
            Err(e) => {
                diagnostics.warn(format!("image asset {} cannot be decoded: {e}", asset.0));
                None
            }
        };
        let entry = SourceEntry {
            stamp: Stamp::new(bytes),
            image: image.clone(),
        };
        self.sources.insert(asset.clone(), entry);
        image
    }

    /// When the frames of `asset` show, when it is an animation.
    pub fn timeline(
        &mut self,
        asset: &AssetRef,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        diagnostics: &mut Diagnostics,
    ) -> Option<Timeline> {
        self.source(asset, assets, diagnostics)?.timeline.clone()
    }

    /// `asset` fitted into `area`, with where to draw it.
    pub fn placed(
        &mut self,
        asset: &AssetRef,
        fit: Fit,
        area: BoxF,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        diagnostics: &mut Diagnostics,
    ) -> Option<(Arc<Scaled>, Plan)> {
        let source = self.source(asset, assets, diagnostics)?;
        let plan = plan(fit, (source.width, source.height), area)?;
        if u64::from(plan.size.0) * u64::from(plan.size.1) > MAX_PIXELS {
            diagnostics.warn(format!(
                "image asset {} is too large to draw at {:?}",
                asset.0, plan.size
            ));
            return None;
        }
        let key = ScaledKey {
            asset: asset.clone(),
            crop: plan.crop,
            size: plan.size,
        };
        self.used_scaled.insert(key.clone());
        if let Some(scaled) = self.scaled.get(&key) {
            return Some((Arc::clone(scaled), plan));
        }
        let scaled = Arc::new(scale(&source, &plan)?);
        self.scaled.insert(key, Arc::clone(&scaled));
        Some((scaled, plan))
    }

    /// Drops what the last frame did not use.
    pub fn sweep(&mut self) {
        let used = std::mem::take(&mut self.used_sources);
        self.sources.retain(|k, _| used.contains(k));
        let used = std::mem::take(&mut self.used_scaled);
        self.scaled.retain(|k, _| used.contains(k));
    }

    /// Number of cached `(sources, scaled)` entries (tests).
    #[cfg(test)]
    pub fn len(&self) -> (usize, usize) {
        (self.sources.len(), self.scaled.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(w: f32, h: f32) -> BoxF {
        BoxF::new(10.0, 20.0, w, h)
    }

    #[test]
    fn plans_each_fit() {
        let fill = plan(Fit::Fill, (4, 2), area(8.0, 8.0)).expect("fill");
        assert_eq!(
            (fill.crop, fill.size, fill.at),
            ((0, 0, 4, 2), (8, 8), (10.0, 20.0))
        );
        let contain = plan(Fit::Contain, (4, 2), area(8.0, 8.0)).expect("contain");
        assert_eq!((contain.size, contain.at), ((8, 4), (10.0, 22.0)));
        let cover = plan(Fit::Cover, (4, 2), area(8.0, 8.0)).expect("cover");
        assert_eq!((cover.crop, cover.size), ((1, 0, 2, 2), (8, 8)));
        let none = plan(Fit::None, (40, 20), area(8.0, 8.0)).expect("none");
        assert_eq!((none.crop, none.size), ((0, 0, 8, 8), (8, 8)));
        let small = plan(Fit::None, (4, 2), area(8.0, 8.0)).expect("none");
        assert_eq!(small.size, (4, 2));
        assert!(plan(Fit::Fill, (0, 2), area(8.0, 8.0)).is_none());
        assert!(plan(Fit::Fill, (4, 2), area(0.0, 8.0)).is_none());
        assert!(plan(Fit::Fill, (4, 2), BoxF::new(f32::NAN, 0.0, 1.0, 1.0)).is_none());
    }

    #[test]
    fn animations_pick_the_frame_by_time() {
        let pixmap = || Pixmap::new(1, 1).expect("pixmap");
        let ms = Duration::from_millis;
        let scaled = Scaled {
            frames: vec![pixmap(), pixmap()],
            timeline: Timeline::new(vec![ms(1000), ms(2000)]),
        };
        let ptr = |t| scaled.frame_at(ms(t)).map(|p| p as *const Pixmap);
        let first = Some(&scaled.frames[0] as *const Pixmap);
        let second = Some(&scaled.frames[1] as *const Pixmap);
        assert_eq!(ptr(0), first);
        assert_eq!(ptr(999), first);
        assert_eq!(ptr(1000), second, "to the millisecond");
        assert_eq!(ptr(2999), second);
        assert_eq!(ptr(3000), first);
        let still = Scaled {
            frames: vec![pixmap()],
            timeline: None,
        };
        assert!(still.frame_at(ms(7000)).is_some());
    }

    #[test]
    fn a_gif_tells_its_frame_times() {
        let mut cache = ImageCache::default();
        let mut diagnostics = Diagnostics::default();
        let gif = crate::testkit::gif(
            2,
            2,
            &[
                (bezel_core::domain::frame::Rgba::BLACK, 40),
                (bezel_core::domain::frame::Rgba::WHITE, 5),
            ],
        );
        let png = crate::testkit::png(2, 2, |_, _| bezel_core::domain::frame::Rgba::WHITE);
        let (anim, still) = (AssetRef("a.gif".into()), AssetRef("b.png".into()));
        let assets = BTreeMap::from([(anim.clone(), gif), (still.clone(), png)]);
        let timeline = cache
            .timeline(&anim, &assets, &mut diagnostics)
            .expect("animated");
        let ms = Duration::from_millis;
        // A 5 ms frame plays at 100 ms, as browsers do.
        assert_eq!((timeline.len(), timeline.total()), (2, ms(140)));
        assert_eq!(cache.timeline(&still, &assets, &mut diagnostics), None);
        assert_eq!(cache.len(), (2, 0), "decoded once, kept for drawing");
    }

    #[test]
    fn broken_images_are_reported_and_cached_as_failures() {
        let mut cache = ImageCache::default();
        let mut diagnostics = Diagnostics::default();
        let asset = AssetRef("assets/broken.png".into());
        let assets = BTreeMap::from([(asset.clone(), b"\x89PNG\r\n\x1a\nnot really".to_vec())]);
        let area = BoxF::new(0.0, 0.0, 4.0, 4.0);
        assert!(
            cache
                .placed(&asset, Fit::Fill, area, &assets, &mut diagnostics)
                .is_none()
        );
        assert!(diagnostics.mentions("cannot be decoded"));
        let missing = AssetRef("assets/missing.png".into());
        assert!(
            cache
                .placed(&missing, Fit::Fill, area, &assets, &mut diagnostics)
                .is_none()
        );
        assert!(diagnostics.mentions("missing"));
        assert_eq!(cache.len(), (1, 0));
        cache.sweep();
        assert_eq!(cache.len(), (1, 0), "used in the last frame");
        cache.sweep();
        assert_eq!(cache.len(), (0, 0), "unused since");
        assert!(decode(b"GIF89a").is_err());
        assert!(decode(b"").is_err());
    }
}
