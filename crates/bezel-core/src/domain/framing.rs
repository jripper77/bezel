//! Framing a theme's video background (D-2026-10-01-video-background-
//! framing-2 to -4): how its picture is turned, fitted, zoomed and placed on
//! a target, the theme's canvas or, for a conversion, the panel.
//!
//! A theme keeps a [`VideoFraming`] whose rotation may be Auto. Wherever the
//! video is used, Auto is decided from the video's probed size
//! ([`VideoFraming::resolve`]), and the [`ResolvedFraming`] gives one pure
//! [`geometry`] to every user: the conversion for a screen, the poster and
//! the pictures decoded on the host. The converter turns and scales the
//! pixels of a conversion or a poster; the pictures decoded on the host are
//! framed here ([`frame_picture`]), so a framing edit never restarts the
//! decoder.

use super::catalog::MODELS;
use super::device::DeviceModel;
use super::frame::{Rect, Rgba};
use super::geometry::{Orientation, Size};

mod picture;

pub use picture::frame_picture;

/// How the picture fills its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VideoFit {
    /// Fill: scaled to cover the target, the overflow cut off (the default).
    #[default]
    Cover,
    /// Fit: scaled to fit inside the target, the area it leaves painted with
    /// the pad color.
    Contain,
}

impl VideoFit {
    /// Both fits.
    pub const ALL: [VideoFit; 2] = [VideoFit::Cover, VideoFit::Contain];

    /// Stable machine name (`cover`, `contain`).
    pub const fn slug(self) -> &'static str {
        match self {
            VideoFit::Cover => "cover",
            VideoFit::Contain => "contain",
        }
    }

    /// The fit named by `slug`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.slug() == slug)
    }
}

/// Zoom on top of the fit's scale, in whole percent: 100 (none) to 400.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Zoom(u16);

impl Zoom {
    /// No zoom: the fit's own scale.
    pub const NONE: Zoom = Zoom(100);
    /// The largest zoom, 4x.
    pub const MAX: Zoom = Zoom(400);

    /// A zoom of `percent`, clamped to 100..=400.
    pub fn from_percent(percent: u32) -> Self {
        let clamped = percent.clamp(u32::from(Self::NONE.0), u32::from(Self::MAX.0));
        Self(u16::try_from(clamped).unwrap_or(Self::MAX.0))
    }

    /// A zoom `factor` (1.0 = none), rounded to whole percent and clamped;
    /// not a number is no zoom.
    pub fn from_factor(factor: f64) -> Self {
        if factor.is_nan() {
            return Self::NONE;
        }
        let percent = (factor * 100.0).round().clamp(100.0, 400.0);
        // In 100..=400 after the clamp: the cast is exact.
        Self::from_percent(percent as u32)
    }

    /// The zoom in percent (100..=400).
    pub const fn percent(self) -> u16 {
        self.0
    }

    /// The zoom as a factor (1.0..=4.0).
    pub fn factor(self) -> f64 {
        f64::from(self.0) / 100.0
    }
}

impl Default for Zoom {
    fn default() -> Self {
        Self::NONE
    }
}

/// A place along one axis in thousandths, CSS `object-position` style:
/// 0 = start, 500 = center, 1000 = end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Permille(u16);

impl Permille {
    /// The start (left or top).
    pub const START: Permille = Permille(0);
    /// The middle.
    pub const CENTER: Permille = Permille(500);
    /// The end (right or bottom).
    pub const END: Permille = Permille(1000);

    /// `permille` thousandths, clamped to 0..=1000.
    pub fn from_permille(permille: u32) -> Self {
        let clamped = permille.min(u32::from(Self::END.0));
        Self(u16::try_from(clamped).unwrap_or(Self::END.0))
    }

    /// A `fraction` of the axis (0.0..=1.0), rounded to thousandths and
    /// clamped; not a number is the middle.
    pub fn from_fraction(fraction: f64) -> Self {
        if fraction.is_nan() {
            return Self::CENTER;
        }
        let permille = (fraction * 1000.0).round().clamp(0.0, 1000.0);
        // In 0..=1000 after the clamp: the cast is exact.
        Self::from_permille(permille as u32)
    }

    /// Thousandths (0..=1000).
    pub const fn permille(self) -> u16 {
        self.0
    }

    /// The place as a fraction (0.0..=1.0).
    pub fn fraction(self) -> f64 {
        f64::from(self.0) / 1000.0
    }

    /// The same place counted from the other end.
    pub const fn mirrored(self) -> Self {
        Self(Self::END.0 - self.0)
    }
}

impl Default for Permille {
    fn default() -> Self {
        Self::CENTER
    }
}

/// Where the picture sits in its target on each axis, with CSS
/// `object-position` semantics: on an axis where the picture overflows it
/// picks the part shown, where it is smaller it places the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FramingPosition {
    /// Across.
    pub x: Permille,
    /// Down.
    pub y: Permille,
}

impl FramingPosition {
    /// Centered on both axes (the default).
    pub const CENTER: FramingPosition = FramingPosition {
        x: Permille::CENTER,
        y: Permille::CENTER,
    };
}

/// A theme's framing of its video background, as the user set it. `None` in
/// [`super::theme::Background::Video`] is [`VideoFraming::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoFraming {
    /// Clockwise quarter turns (0..=3, taken modulo 4) applied to the video
    /// before it is fitted to the canvas; `None` is Auto ([`auto_turns`]).
    pub rotation: Option<u8>,
    /// Fill or fit.
    pub fit: VideoFit,
    /// Zoom on top of the fit's scale.
    pub zoom: Zoom,
    /// Where the picture sits.
    pub position: FramingPosition,
    /// Color of the area a fitted picture leaves (always painted opaque).
    pub pad: Rgba,
}

impl Default for VideoFraming {
    /// Auto rotation, cover, no zoom, centered, black pad.
    fn default() -> Self {
        Self {
            rotation: None,
            fit: VideoFit::Cover,
            zoom: Zoom::NONE,
            position: FramingPosition::CENTER,
            pad: Rgba::BLACK,
        }
    }
}

impl VideoFraming {
    /// Whether this is the default framing (`.bezeltheme` then omits it).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The framing of a video of `video` pixels (`None`: size unknown) on a
    /// theme's canvas in `orientation`, for `panel` (the screen's, else
    /// [`PanelLayout::for_canvas`]): Auto becomes [`auto_turns`], an explicit
    /// rotation is kept.
    pub fn resolve(
        &self,
        video: Option<Size>,
        orientation: Orientation,
        panel: Option<PanelLayout>,
    ) -> ResolvedFraming {
        let turns = match self.rotation {
            Some(turns) => turns % 4,
            None => auto_turns(video, orientation, panel),
        };
        ResolvedFraming {
            turns,
            fit: self.fit,
            zoom: self.zoom,
            position: self.position,
            pad: self.pad,
        }
    }
}

/// A panel as Auto sees it: the picture a video stored for it has, and how
/// its framebuffer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelLayout {
    /// The panel in its native orientation (480x1920 on the 8.8").
    pub native: Size,
    /// The orientation its framebuffer is laid out in.
    pub orientation: Orientation,
}

impl PanelLayout {
    /// The panel of `model`.
    pub fn of(model: &DeviceModel) -> Self {
        Self {
            native: model
                .panel
                .portrait()
                .in_orientation(model.native_orientation),
            orientation: model.native_orientation,
        }
    }

    /// The panel a theme's `canvas` is drawn for when no screen says it: the
    /// one layout every catalog model with that panel shares (a 1920x480
    /// canvas: the 8.8" panels, 480x1920 standing reverse-portrait). `None`
    /// when no model has it or models lay it out differently.
    pub fn for_canvas(canvas: Size) -> Option<Self> {
        let panel = canvas.portrait();
        let mut layouts = MODELS.iter().filter(|m| m.panel == panel).map(Self::of);
        let first = layouts.next()?;
        layouts.all(|layout| layout == first).then_some(first)
    }

    /// The screen's panel when there is a screen, else the canvas's
    /// ([`Self::for_canvas`]).
    pub fn for_theme(screen: Option<&DeviceModel>, canvas: Size) -> Option<Self> {
        screen.map_or_else(|| Self::for_canvas(canvas), |model| Some(Self::of(model)))
    }
}

/// Auto rotation: a video of exactly the panel's native size, in a theme an
/// odd number of quarter turns from the panel, is already turned for the
/// panel (the vendor keeps every theme video panel-native), so it gets the
/// turns that cancel the theme-to-panel ones (a landscape theme on the 8.8":
/// 3, so 0 in total). Any other size, an unknown size or panel, or a theme
/// a half turn (or none) from the panel: 0.
pub fn auto_turns(video: Option<Size>, orientation: Orientation, panel: Option<PanelLayout>) -> u8 {
    let (Some(video), Some(panel)) = (video, panel) else {
        return 0;
    };
    let to_panel = orientation.quarter_turns_to(panel.orientation);
    if to_panel % 2 == 1 && video == panel.native {
        (4 - to_panel) % 4
    } else {
        0
    }
}

/// A framing with its rotation decided, relative to one target: the theme's
/// canvas ([`VideoFraming::resolve`]) or the panel ([`Self::turned`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedFraming {
    /// Clockwise quarter turns applied to the video first (0..=3).
    pub turns: u8,
    /// Fill or fit.
    pub fit: VideoFit,
    /// Zoom on top of the fit's scale.
    pub zoom: Zoom,
    /// Where the picture sits in the target.
    pub position: FramingPosition,
    /// Color of the area a fitted picture leaves (always painted opaque).
    pub pad: Rgba,
}

/// FNV-1a, 32 bits.
fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash: u32, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

impl ResolvedFraming {
    /// The default framing after `turns` clockwise quarter turns: cover, no
    /// zoom, centered.
    pub const fn plain(turns: u8) -> Self {
        Self {
            turns: turns % 4,
            fit: VideoFit::Cover,
            zoom: Zoom::NONE,
            position: FramingPosition::CENTER,
            pad: Rgba::BLACK,
        }
    }

    /// The same framing on a target turned `turns` clockwise quarter turns
    /// from this one (a theme's canvas turned to its panel: the theme's
    /// `quarter_turns_to` the panel's native orientation). The video turns
    /// that much more and the position turns with the picture.
    pub fn turned(self, turns: u8) -> Self {
        let mut position = self.position;
        for _ in 0..turns % 4 {
            // Clockwise: the old y axis runs right to left, the old x down.
            position = FramingPosition {
                x: position.y.mirrored(),
                y: position.x,
            };
        }
        Self {
            turns: (self.turns % 4 + turns % 4) % 4,
            position,
            ..self
        }
    }

    /// Whether everything but the turns is the default: cover, no zoom,
    /// centered (a screen then stores the video under the vendor's name).
    pub fn is_plain(&self) -> bool {
        self.fit == VideoFit::Cover
            && self.zoom == Zoom::NONE
            && self.position == FramingPosition::CENTER
    }

    /// Everything but the turns as text, what [`Self::fingerprint`] hashes:
    /// `fit=<slug>;zoom=<percent>;x=<permille>;y=<permille>`, then
    /// `;pad=<rrggbb>` (lower-case hex) when the pad shows, with Fit.
    pub fn canonical(&self) -> String {
        let mut text = format!(
            "fit={};zoom={};x={};y={}",
            self.fit.slug(),
            self.zoom.percent(),
            self.position.x.permille(),
            self.position.y.permille()
        );
        if self.fit == VideoFit::Contain {
            let Rgba { r, g, b, .. } = self.pad;
            text.push_str(&format!(";pad={r:02x}{g:02x}{b:02x}"));
        }
        text
    }

    /// FNV-1a (32 bits) of [`Self::canonical`]; `None` for a plain framing.
    /// A screen stores a video framed otherwise under its own name.
    pub fn fingerprint(&self) -> Option<u32> {
        (!self.is_plain()).then(|| fnv1a(self.canonical().as_bytes()))
    }
}

/// Where a picture smaller than its target sits, and what is around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pad {
    /// Size the kept part of the video is scaled to, within the target.
    pub scaled: Size,
    /// Left edge of the picture in the target (even).
    pub x: u32,
    /// Top edge of the picture in the target (even).
    pub y: u32,
    /// Opaque color of the rest of the target.
    pub color: Rgba,
}

/// How a video's picture becomes its target's, in order: turned, cropped,
/// scaled, padded ([`geometry`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FramingGeometry {
    /// Clockwise quarter turns applied to the video first (0..=3).
    pub turns: u8,
    /// Part of the turned video kept, in its pixels, with even edges (as
    /// yuv420p needs); `None` keeps the whole picture.
    pub crop: Option<Rect>,
    /// The target's size: the picture produced.
    pub size: Size,
    /// Where the scaled picture sits when it does not cover the target;
    /// `None`: it is scaled to the target's size.
    pub pad: Option<Pad>,
}

impl FramingGeometry {
    /// Only `turns` onto `size`: no crop, no pad (a video of unknown size).
    pub const fn turning(turns: u8, size: Size) -> Self {
        Self {
            turns: turns % 4,
            crop: None,
            size,
            pad: None,
        }
    }

    /// Size the kept part of the video is scaled to.
    pub fn scaled(&self) -> Size {
        self.pad.map_or(self.size, |pad| pad.scaled)
    }

    /// Whether a video of `source` pixels already is the picture: nothing
    /// turned, cropped, scaled or padded.
    pub fn is_identity(&self, source: Size) -> bool {
        self.turns == 0 && self.crop.is_none() && self.pad.is_none() && source == self.size
    }
}

/// The scale from turned-video pixels to target pixels, `num / den`.
#[derive(Clone, Copy)]
struct Scale {
    num: u128,
    den: u128,
}

/// The fit's scale of `turned` onto `target`, times the zoom.
fn scale(turned: Size, target: Size, framing: &ResolvedFraming) -> Scale {
    let (sw, sh) = (u128::from(turned.width), u128::from(turned.height));
    let (tw, th) = (u128::from(target.width), u128::from(target.height));
    // Relatively wider than the target: cover matches the heights, contain
    // the widths (and the other way round for a taller picture).
    let wider = sw * th > sh * tw;
    let by_height = match framing.fit {
        VideoFit::Cover => wider,
        VideoFit::Contain => !wider,
    };
    let (num, den) = if by_height { (th, sh) } else { (tw, sw) };
    Scale {
        num: num * u128::from(framing.zoom.percent()),
        den: den * 100,
    }
}

/// `value` rounded down to even, at least 2 and at most `limit`.
fn even(value: u128, limit: u128) -> u32 {
    let even = (value & !1).max(2).min(limit);
    u32::try_from(even).unwrap_or(u32::MAX)
}

/// One axis of a framing.
struct Span {
    /// Whether the video overflows the target here (and is cut).
    cut: bool,
    /// First video pixel kept.
    from: u32,
    /// Video pixels kept.
    kept: u32,
    /// Extent of the picture in the target.
    out: u32,
    /// Offset of the picture in the target.
    at: u32,
}

/// The axis of `length` video pixels on `target` pixels at `scale`, the
/// picture placed at `position`.
fn span(length: u32, target: u32, scale: Scale, position: Permille) -> Span {
    let (a, t) = (u128::from(length), u128::from(target));
    let p = u128::from(position.permille());
    let to_u32 = |v: u128| u32::try_from(v).unwrap_or(u32::MAX);
    if a * scale.num > t * scale.den {
        // Overflow: keep what the target shows, picked by the position.
        let kept = even(t * scale.den / scale.num, a);
        let from = (p * (a - u128::from(kept)) / 1000) & !1;
        return Span {
            cut: true,
            from: to_u32(from),
            kept,
            out: target,
            at: 0,
        };
    }
    let scaled = a * scale.num / scale.den;
    let (out, at) = if scaled + 2 > t {
        // Within rounding of the target: it covers.
        (target, 0)
    } else {
        let out = even(scaled, t);
        (out, to_u32((p * (t - u128::from(out)) / 1000) & !1))
    };
    Span {
        cut: false,
        from: 0,
        kept: even(a, a),
        out,
        at,
    }
}

/// How a video of `source` pixels becomes a `target` picture under
/// `framing`: turned by its turns, then cropped where the scaled picture
/// overflows (the part its position picks, even edges), scaled, and padded
/// where it is smaller (placed by its position). Cover never leaves an
/// empty edge. The plain framing gives the vendor's centered cover crop
/// ([`super::media::cover_crop`]); a degenerate size gives only the turns.
pub fn geometry(source: Size, framing: &ResolvedFraming, target: Size) -> FramingGeometry {
    let turns = framing.turns % 4;
    let turned = if turns % 2 == 1 {
        source.transposed()
    } else {
        source
    };
    if turned.area() == 0 || target.area() == 0 {
        return FramingGeometry::turning(turns, target);
    }
    let scale = scale(turned, target, framing);
    let x = span(turned.width, target.width, scale, framing.position.x);
    let y = span(turned.height, target.height, scale, framing.position.y);
    let crop = (x.cut || y.cut).then_some(Rect::new(x.from, y.from, x.kept, y.kept));
    let padded = x.out < target.width || y.out < target.height;
    let pad = padded.then_some(Pad {
        scaled: Size::new(x.out, y.out),
        x: x.at,
        y: y.at,
        color: Rgba {
            a: 255,
            ..framing.pad
        },
    });
    FramingGeometry {
        turns,
        crop,
        size: target,
        pad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;
    use crate::domain::media::cover_crop;

    fn model(id: &'static str) -> &'static DeviceModel {
        model_by_id(ModelId(id)).expect("model")
    }

    /// The user's "Dragon Ball" theme: a 1920x480 landscape canvas on the
    /// 8.8", over a pre-turned panel-native H.264 video of 480x1920.
    const DRAGON_BALL_CANVAS: Size = Size::new(1920, 480);
    const DRAGON_BALL_VIDEO: Size = Size::new(480, 1920);

    #[test]
    fn a_panel_native_video_in_a_turned_theme_counts_as_turned() {
        let screen = Some(PanelLayout::of(model("turing-8.8")));
        assert_eq!(
            screen,
            Some(PanelLayout {
                native: DRAGON_BALL_VIDEO,
                orientation: Orientation::ReversePortrait
            })
        );
        let auto = VideoFraming::default();
        let video = Some(DRAGON_BALL_VIDEO);
        // Dragon Ball: 270 degrees on the canvas, nothing in total on the panel.
        let resolved = auto.resolve(video, Orientation::Landscape, screen);
        assert_eq!(resolved.turns, 3);
        let to_panel = Orientation::Landscape.quarter_turns_to(Orientation::ReversePortrait);
        assert_eq!((to_panel, resolved.turned(to_panel).turns), (1, 0));
        // Without a screen the canvas names the 8.8" panel (both 8.8" models
        // lay it out alike).
        let canvas = PanelLayout::for_canvas(DRAGON_BALL_CANVAS);
        assert_eq!(canvas, screen);
        assert_eq!(PanelLayout::for_theme(None, DRAGON_BALL_CANVAS), screen);
        assert_eq!(auto.resolve(video, Orientation::Landscape, canvas).turns, 3);
        // Upside down the other way: 90 degrees.
        let reverse = auto.resolve(video, Orientation::ReverseLandscape, canvas);
        assert_eq!(reverse.turns, 1);
        assert_eq!(reverse.turned(3).turns, 0);
        // Another size, an unknown size, no known panel or a half turn: 0.
        let wide = Some(Size::new(1920, 1080));
        assert_eq!(auto.resolve(wide, Orientation::Landscape, canvas).turns, 0);
        assert_eq!(auto.resolve(None, Orientation::Landscape, canvas).turns, 0);
        assert_eq!(auto.resolve(video, Orientation::Landscape, None).turns, 0);
        assert_eq!(auto.resolve(video, Orientation::Portrait, canvas).turns, 0);
        assert_eq!(
            auto_turns(video, Orientation::ReversePortrait, canvas),
            0,
            "the panel's own way up"
        );
        // An explicit rotation wins over Auto.
        let explicit = VideoFraming {
            rotation: Some(5),
            ..auto
        };
        assert_eq!(
            explicit
                .resolve(video, Orientation::Landscape, canvas)
                .turns,
            1
        );
        // The screen's panel when there is one: the 5" stands landscape, so a
        // portrait theme over an 800x480 video turns it back.
        let five = model("turing-5");
        let panel = PanelLayout::for_theme(Some(five), Size::new(480, 800));
        let landscape_clip = Some(Size::new(800, 480));
        assert_eq!(auto_turns(landscape_clip, Orientation::Portrait, panel), 3);
        // Square panels laid out differently by different models: no panel.
        assert_eq!(PanelLayout::for_canvas(Size::new(480, 480)), None);
        assert_eq!(PanelLayout::for_canvas(Size::new(123, 456)), None);
    }

    fn framed(fit: VideoFit, zoom: u32, x: u32, y: u32) -> ResolvedFraming {
        ResolvedFraming {
            fit,
            zoom: Zoom::from_percent(zoom),
            position: FramingPosition {
                x: Permille::from_permille(x),
                y: Permille::from_permille(y),
            },
            ..ResolvedFraming::plain(0)
        }
    }

    #[test]
    fn geometry_covers_contains_zooms_and_positions_on_even_edges() {
        let canvas = DRAGON_BALL_CANVAS;
        let panel = DRAGON_BALL_VIDEO;
        // The plain framing is the vendor's centered cover crop (what
        // `cover_crop` always gave: the middle, even edges, in turned pixels).
        for (source, turns, target, crop) in [
            (
                Size::new(1920, 1080),
                1,
                panel,
                Some(Rect::new(300, 0, 480, 1920)),
            ),
            (
                Size::new(1920, 1080),
                0,
                panel,
                Some(Rect::new(824, 0, 270, 1080)),
            ),
            (
                Size::new(480, 2400),
                0,
                panel,
                Some(Rect::new(0, 240, 480, 1920)),
            ),
            (Size::new(960, 3840), 0, panel, None),
            (
                Size::new(1921, 1081),
                0,
                canvas,
                Some(Rect::new(0, 300, 1920, 480)),
            ),
            (
                Size::new(1280, 721),
                3,
                canvas,
                Some(Rect::new(0, 550, 720, 180)),
            ),
        ] {
            let g = geometry(source, &ResolvedFraming::plain(turns), target);
            assert_eq!(g.crop, crop, "{source:?}");
            assert_eq!(cover_crop(source, turns, target), crop, "{source:?}");
            assert_eq!((g.turns, g.pad, g.scaled()), (turns, None, target));
        }
        // Dragon Ball on its canvas, then on the panel: nothing to do there.
        let dragon = ResolvedFraming::plain(3);
        let on_canvas = geometry(DRAGON_BALL_VIDEO, &dragon, canvas);
        assert_eq!((on_canvas.crop, on_canvas.pad), (None, None));
        let on_panel = geometry(DRAGON_BALL_VIDEO, &dragon.turned(1), panel);
        assert!(on_panel.is_identity(DRAGON_BALL_VIDEO));
        assert!(!on_canvas.is_identity(DRAGON_BALL_VIDEO), "turned");

        // Cover picks the part its position says, never an empty edge.
        let clip = Size::new(1920, 1080);
        let top = geometry(clip, &framed(VideoFit::Cover, 100, 500, 0), canvas);
        assert_eq!(top.crop, Some(Rect::new(0, 0, 1920, 480)));
        let bottom = geometry(clip, &framed(VideoFit::Cover, 100, 500, 1000), canvas);
        assert_eq!(bottom.crop, Some(Rect::new(0, 600, 1920, 480)));
        assert_eq!(bottom.pad, None);
        // Zoom 2x on the canvas's own shape: a centered half on each axis.
        let same = Size::new(3840, 960);
        let zoomed = geometry(same, &framed(VideoFit::Cover, 200, 500, 500), canvas);
        assert_eq!(zoomed.crop, Some(Rect::new(960, 240, 1920, 480)));
        // ... and positioned at the end: the bottom-right quarter.
        let corner = geometry(same, &framed(VideoFit::Cover, 200, 1000, 1000), canvas);
        assert_eq!(corner.crop, Some(Rect::new(1920, 480, 1920, 480)));

        // Contain fits inside: 1920x1080 on 1920x480 is 852x480, centered
        // on even edges, the rest painted opaque.
        let fit = ResolvedFraming {
            pad: Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 0,
            },
            ..framed(VideoFit::Contain, 100, 500, 500)
        };
        let contained = geometry(clip, &fit, canvas);
        assert_eq!(contained.crop, None);
        let pad = Pad {
            scaled: Size::new(852, 480),
            x: 534,
            y: 0,
            color: Rgba::opaque(10, 20, 30),
        };
        assert_eq!(contained.pad, Some(pad));
        assert_eq!(contained.scaled(), Size::new(852, 480));
        // At the start of the axis.
        let left = geometry(clip, &framed(VideoFit::Contain, 100, 0, 500), canvas);
        assert_eq!(left.pad.map(|p| (p.x, p.y)), Some((0, 0)));
        // Zoomed 2x: too tall (cut, centered) and still too narrow (padded).
        let both = geometry(clip, &framed(VideoFit::Contain, 200, 500, 500), canvas);
        assert_eq!(both.crop, Some(Rect::new(0, 270, 1920, 540)));
        assert_eq!(
            both.pad.map(|p| (p.scaled, p.x)),
            Some((Size::new(1706, 480), 106))
        );
        // Contain of the target's shape covers, within a rounding pixel too.
        let exact = geometry(same, &framed(VideoFit::Contain, 100, 500, 500), canvas);
        assert_eq!((exact.crop, exact.pad), (None, None));
        let nearly = geometry(
            Size::new(1921, 480),
            &framed(VideoFit::Contain, 100, 0, 0),
            canvas,
        );
        assert_eq!((nearly.crop, nearly.pad), (None, None));

        // Every crop and pad edge is even, whatever the sizes.
        for source in [
            Size::new(1279, 719),
            Size::new(333, 1001),
            Size::new(4097, 3),
        ] {
            for zoom in [100, 135, 400] {
                for fit in VideoFit::ALL {
                    let g = geometry(source, &framed(fit, zoom, 333, 777), canvas);
                    if let Some(c) = g.crop {
                        assert!([c.x, c.y].iter().all(|v| v % 2 == 0), "{g:?}");
                        assert!(c.x + c.width <= source.width, "{g:?}");
                        assert!(c.y + c.height <= source.height, "{g:?}");
                    }
                    if let Some(p) = g.pad {
                        let edges = [p.x, p.y, p.scaled.width, p.scaled.height];
                        assert!(edges.iter().all(|v| v % 2 == 0), "{g:?}");
                        assert!(p.x + p.scaled.width <= canvas.width, "{g:?}");
                        assert!(p.y + p.scaled.height <= canvas.height, "{g:?}");
                    }
                    if fit == VideoFit::Cover {
                        assert_eq!(g.pad, None, "cover leaves no empty edge");
                    }
                }
            }
        }
        // Degenerate sizes only turn.
        let empty = geometry(Size::new(0, 10), &ResolvedFraming::plain(1), canvas);
        assert_eq!(empty, FramingGeometry::turning(1, canvas));
    }

    #[test]
    fn turning_the_target_turns_the_position_with_the_picture() {
        let framing = framed(VideoFit::Contain, 150, 100, 300);
        let once = framing.turned(1);
        assert_eq!(once.turns, 1);
        assert_eq!(
            (once.position.x.permille(), once.position.y.permille()),
            (700, 100)
        );
        assert_eq!(framing.turned(4), framing);
        assert_eq!(once.turned(3), framing);
        assert_eq!((once.fit, once.zoom), (framing.fit, framing.zoom));
        let odd = ResolvedFraming {
            turns: u8::MAX,
            ..framing
        };
        assert_eq!(odd.turned(3).turns, 2, "taken modulo 4");
        // The same picture: a cut 10% from the left on the canvas is 10%
        // from the top on a panel turned clockwise.
        let clip = Size::new(1920, 1080);
        let wide = geometry(
            clip,
            &framed(VideoFit::Cover, 100, 100, 500),
            Size::new(400, 1080),
        );
        let tall = geometry(
            clip,
            &framed(VideoFit::Cover, 100, 100, 500).turned(1),
            Size::new(1080, 400),
        );
        let (w, t) = (wide.crop.expect("cut"), tall.crop.expect("cut"));
        assert_eq!((w.x, w.width), (t.y, t.height));
    }

    #[test]
    fn zoom_and_position_are_clamped_and_named() {
        assert_eq!(Zoom::from_percent(50), Zoom::NONE);
        assert_eq!(Zoom::from_percent(1000), Zoom::MAX);
        assert_eq!(Zoom::from_factor(1.25).percent(), 125);
        assert_eq!(Zoom::from_factor(f64::NAN), Zoom::NONE);
        assert_eq!(Zoom::from_factor(f64::INFINITY), Zoom::MAX);
        assert!((Zoom::from_percent(250).factor() - 2.5).abs() < f64::EPSILON);
        assert_eq!(Zoom::default(), Zoom::NONE);
        assert_eq!(Permille::from_fraction(0.4).permille(), 400);
        assert_eq!(Permille::from_fraction(-3.0), Permille::START);
        assert_eq!(Permille::from_fraction(f64::NAN), Permille::CENTER);
        assert_eq!(Permille::from_permille(5000), Permille::END);
        assert!((Permille::END.fraction() - 1.0).abs() < f64::EPSILON);
        assert_eq!(Permille::default(), Permille::CENTER);
        assert_eq!(VideoFit::from_slug("contain"), Some(VideoFit::Contain));
        assert_eq!(VideoFit::from_slug("stretch"), None);
        assert!(VideoFraming::default().is_default());
        let turned = VideoFraming {
            rotation: Some(0),
            ..VideoFraming::default()
        };
        assert!(!turned.is_default());
    }

    #[test]
    fn the_fingerprint_hashes_the_canonical_framing() {
        // Known FNV-1a vectors.
        assert_eq!(fnv1a(b""), 0x811c_9dc5);
        assert_eq!(fnv1a(b"a"), 0xe40c_292c);
        assert_eq!(ResolvedFraming::plain(2).fingerprint(), None);
        let zoomed = framed(VideoFit::Cover, 125, 500, 400);
        assert_eq!(zoomed.canonical(), "fit=cover;zoom=125;x=500;y=400");
        assert_eq!(
            zoomed.fingerprint(),
            Some(fnv1a(b"fit=cover;zoom=125;x=500;y=400"))
        );
        // The pad counts only where it shows, and only its color.
        let other_pad = ResolvedFraming {
            pad: Rgba::WHITE,
            ..zoomed
        };
        assert_eq!(other_pad.fingerprint(), zoomed.fingerprint());
        let fitted = ResolvedFraming {
            pad: Rgba {
                r: 0xab,
                g: 0xcd,
                b: 0xef,
                a: 0x10,
            },
            ..framed(VideoFit::Contain, 100, 500, 500)
        };
        assert_eq!(
            fitted.canonical(),
            "fit=contain;zoom=100;x=500;y=500;pad=abcdef"
        );
        assert_ne!(
            fitted.fingerprint(),
            framed(VideoFit::Contain, 100, 500, 500).fingerprint()
        );
        // The turns are not part of it.
        let turned = ResolvedFraming { turns: 1, ..zoomed };
        assert_eq!(turned.fingerprint(), zoomed.fingerprint());
    }
}
