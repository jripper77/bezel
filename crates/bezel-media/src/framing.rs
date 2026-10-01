//! The ffmpeg filter chain of a framing (D-2026-10-01-video-background-
//! framing-3): one pure function from the core's [`FramingGeometry`], used by
//! the conversion for a screen and by the poster, in the vendor's order
//! (docs/reverse-engineering/video.md § 3 and § 4): the turns (`transpose`),
//! the crop in turned source pixels, `scale`, the `pad` with its color, then
//! square pixels (`setsar`). The plain framing gives today's chains; host
//! decoding asks for the raw picture (no turns, no crop), which is the chain
//! of [`FramingGeometry::turning`] with no turns.

use bezel_core::domain::frame::Rgba;
use bezel_core::domain::framing::FramingGeometry;
use bezel_core::domain::geometry::Size;
use bezel_core::{BezelError, Result};

/// Rotation prefixes of the vendor's adjust dialog, by clockwise quarter turns.
const ROTATIONS: [&str; 4] = [
    "",
    "transpose=1,",
    "transpose=1,transpose=1,",
    "transpose=2,",
];

/// How the kept part of the picture is scaled to its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scaling {
    /// Exactly to the size, then `setsar=1:1`: the vendor's conversion (the
    /// geometry already has the size's shape; a video of unknown size is
    /// stretched, as the vendor does).
    Exact,
    /// Covering the size, its middle kept, then `setsar=1`: a picture shown
    /// on the host (the poster), which never shows a distorted or empty edge,
    /// even for a video of unknown size (the last crop only trims a rounding
    /// pixel otherwise).
    Cover,
}

/// ffmpeg's `-vf` making a video's picture into `geometry`'s: turned,
/// cropped, scaled as `scaling` says, padded with the pad's opaque color.
/// An empty size or crop, or a pad that does not fit, is refused.
pub(crate) fn filter_chain(geometry: &FramingGeometry, scaling: Scaling) -> Result<String> {
    check(geometry)?;
    let mut chain = ROTATIONS[usize::from(geometry.turns % 4)].to_string();
    if let Some(crop) = geometry.crop {
        chain.push_str(&format!(
            "crop={}:{}:{}:{},",
            crop.width, crop.height, crop.x, crop.y
        ));
    }
    let Size { width, height } = geometry.scaled();
    chain.push_str(&match scaling {
        Scaling::Exact => format!("scale={width}:{height}"),
        Scaling::Cover => format!(
            "scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height}"
        ),
    });
    if let Some(pad) = geometry.pad {
        let Rgba { r, g, b, .. } = pad.color;
        chain.push_str(&format!(
            ",pad={}:{}:{}:{}:color=0x{r:02x}{g:02x}{b:02x}",
            geometry.size.width, geometry.size.height, pad.x, pad.y
        ));
    }
    chain.push_str(match scaling {
        Scaling::Exact => ",setsar=1:1",
        Scaling::Cover => ",setsar=1",
    });
    Ok(chain)
}

/// A geometry ffmpeg can produce.
fn check(geometry: &FramingGeometry) -> Result<()> {
    let size = geometry.size;
    if size.area() == 0 {
        return Err(BezelError::InvalidInput("the output size is empty".into()));
    }
    if geometry.crop.is_some_and(|c| c.width == 0 || c.height == 0) {
        return Err(BezelError::InvalidInput("the crop is empty".into()));
    }
    let fits =
        |at: u32, extent: u32, limit: u32| u64::from(at) + u64::from(extent) <= u64::from(limit);
    if let Some(pad) = geometry.pad
        && (pad.scaled.area() == 0
            || !fits(pad.x, pad.scaled.width, size.width)
            || !fits(pad.y, pad.scaled.height, size.height))
    {
        return Err(BezelError::InvalidInput(format!(
            "a {}x{} picture at {},{} does not fit {}x{}",
            pad.scaled.width, pad.scaled.height, pad.x, pad.y, size.width, size.height
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::framing::{
        FramingPosition, Pad, PanelLayout, Permille, ResolvedFraming, VideoFit, VideoFraming, Zoom,
        geometry,
    };
    use bezel_core::domain::geometry::Orientation;

    use super::*;

    /// The 8.8" panel in its native orientation, and a landscape canvas on it.
    const PANEL: Size = Size::new(480, 1920);
    const CANVAS: Size = Size::new(1920, 480);

    fn chain(source: Size, framing: &ResolvedFraming, target: Size, scaling: Scaling) -> String {
        filter_chain(&geometry(source, framing, target), scaling).unwrap()
    }

    fn fitted(zoom: u32, x: u32, y: u32) -> ResolvedFraming {
        ResolvedFraming {
            fit: VideoFit::Contain,
            zoom: Zoom::from_percent(zoom),
            position: FramingPosition {
                x: Permille::from_permille(x),
                y: Permille::from_permille(y),
            },
            pad: Rgba {
                r: 0xc8,
                g: 0x1e,
                b: 0x28,
                a: 0,
            },
            ..ResolvedFraming::plain(0)
        }
    }

    #[test]
    fn filter_chains_follow_the_geometry() {
        use Scaling::{Cover, Exact};
        let clip = Size::new(1920, 1080);
        // The plain framing is the vendor's chain: turned, centered cover
        // crop, exact scale, square pixels; the poster's covers the canvas.
        let plain = ResolvedFraming::plain(1);
        assert_eq!(
            chain(clip, &plain, PANEL, Exact),
            "transpose=1,crop=480:1920:300:0,scale=480:1920,setsar=1:1"
        );
        assert_eq!(
            chain(clip, &ResolvedFraming::plain(0), CANVAS, Cover),
            "crop=1920:480:0:300,scale=1920:480:force_original_aspect_ratio=increase,crop=1920:480,setsar=1"
        );
        // Every turn has the vendor's prefix; the panel's own shape is only
        // scaled.
        let turned: Vec<String> = (0..4)
            .map(|turns| filter_chain(&FramingGeometry::turning(turns, PANEL), Exact).unwrap())
            .collect();
        assert_eq!(
            turned,
            [
                "scale=480:1920,setsar=1:1",
                "transpose=1,scale=480:1920,setsar=1:1",
                "transpose=1,transpose=1,scale=480:1920,setsar=1:1",
                "transpose=2,scale=480:1920,setsar=1:1",
            ]
        );
        // Dragon Ball (480x1920, Auto): turned back on the canvas, and the
        // identity on the panel.
        let screen = model_by_id(ModelId("turing-8.8")).unwrap();
        let auto = VideoFraming::default().resolve(
            Some(PANEL),
            Orientation::Landscape,
            Some(PanelLayout::of(screen)),
        );
        assert_eq!(
            chain(PANEL, &auto, CANVAS, Cover),
            "transpose=2,scale=1920:480:force_original_aspect_ratio=increase,crop=1920:480,setsar=1"
        );
        let on_panel = geometry(PANEL, &auto.turned(1), PANEL);
        assert!(on_panel.is_identity(PANEL));
        assert_eq!(
            filter_chain(&on_panel, Exact).unwrap(),
            "scale=480:1920,setsar=1:1"
        );
        // Zoomed and placed: the crop its position picks.
        let zoomed = ResolvedFraming {
            zoom: Zoom::from_percent(200),
            position: FramingPosition {
                x: Permille::END,
                y: Permille::END,
            },
            ..ResolvedFraming::plain(0)
        };
        assert_eq!(
            chain(Size::new(3840, 960), &zoomed, CANVAS, Exact),
            "crop=1920:480:1920:480,scale=1920:480,setsar=1:1"
        );
        // Fit: scaled to the kept size, padded with the opaque pad color, in
        // the vendor's order (turns, crop, scale, pad, setsar).
        assert_eq!(
            chain(clip, &fitted(100, 500, 500), CANVAS, Exact),
            "scale=852:480,pad=1920:480:534:0:color=0xc81e28,setsar=1:1"
        );
        assert_eq!(
            chain(clip, &fitted(100, 500, 500).turned(1), PANEL, Exact),
            "transpose=1,scale=480:852,pad=480:1920:0:534:color=0xc81e28,setsar=1:1"
        );
        assert_eq!(
            chain(clip, &fitted(200, 0, 500), CANVAS, Cover),
            "crop=1920:540:0:270,scale=1706:480:force_original_aspect_ratio=increase,crop=1706:480,pad=1920:480:0:0:color=0xc81e28,setsar=1"
        );
    }

    #[test]
    fn impossible_geometries_are_refused() {
        let mut empty = FramingGeometry::turning(0, Size::new(0, 480));
        assert!(filter_chain(&empty, Scaling::Exact).is_err());
        empty.size = CANVAS;
        empty.crop = Some(Rect::new(0, 0, 0, 4));
        assert!(filter_chain(&empty, Scaling::Cover).is_err());
        let pad = Pad {
            scaled: Size::new(852, 480),
            x: 1200,
            y: 0,
            color: Rgba::BLACK,
        };
        let outside = FramingGeometry {
            pad: Some(pad),
            ..FramingGeometry::turning(0, CANVAS)
        };
        let err = filter_chain(&outside, Scaling::Exact).unwrap_err();
        assert!(err.to_string().contains("does not fit 1920x480"), "{err}");
        let below = FramingGeometry {
            pad: Some(Pad { x: 0, y: 2, ..pad }),
            ..outside
        };
        assert!(filter_chain(&below, Scaling::Exact).is_err());
        let nothing = FramingGeometry {
            pad: Some(Pad {
                scaled: Size::new(0, 480),
                x: 0,
                ..pad
            }),
            ..outside
        };
        assert!(filter_chain(&nothing, Scaling::Exact).is_err());
        let huge = FramingGeometry {
            pad: Some(Pad { x: u32::MAX, ..pad }),
            ..outside
        };
        assert!(filter_chain(&huge, Scaling::Exact).is_err(), "no overflow");
    }
}
