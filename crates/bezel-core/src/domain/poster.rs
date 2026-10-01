//! The poster of a theme's video background: the still picture previews draw
//! under the elements, and a screen shows until the video plays
//! (D-2026-09-30-storage-video-4). Taking it is the media converter's job
//! (`MediaTranscoder::poster`); this module decides which picture and how it
//! is framed.

use std::time::Duration;

use super::frame::Rect;
use super::framing::{FramingGeometry, Pad, ResolvedFraming, geometry};
use super::geometry::Size;
use super::media::{MediaFormat, MediaInfo};

/// How far into a video its poster is taken: past the fade from black many
/// clips open with.
pub const POSTER_AT: Duration = Duration::from_secs(1);

/// Which picture of a video becomes a theme's poster, and how it is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosterSpec {
    /// Size of the poster: the theme's canvas.
    pub size: Size,
    /// The picture shown this long after the start.
    pub at: Duration,
    /// Clockwise quarter turns applied to the video first (0..=3): the
    /// theme's framing (D-2026-10-01-video-background-framing-3).
    pub quarter_turns: u8,
    /// The part of the turned video kept, in its pixels, as a conversion for
    /// a screen crops it (the plain framing: the centered part that covers
    /// the canvas, [`super::media::cover_crop`]). `None` when the shapes
    /// already match or the video's size is unknown: the converter scales
    /// the picture to cover the canvas and keeps its middle.
    pub crop: Option<Rect>,
    /// Where the scaled picture sits when the framing fits it inside the
    /// canvas; `None`: it covers the canvas.
    pub pad: Option<Pad>,
}

impl PosterSpec {
    /// The poster of `media` for a theme whose canvas is `canvas`: the
    /// picture at [`POSTER_AT`], or the first one of an animated GIF and of
    /// a clip shorter than twice that (or of unknown length). The video is
    /// taken to stand like the theme, as a conversion for a screen takes it:
    /// it is cropped to the canvas's shape, never turned (the plain framing,
    /// [`Self::framed`]).
    pub fn for_canvas(canvas: Size, media: &MediaInfo) -> Self {
        Self::framed(canvas, media, &ResolvedFraming::plain(0))
    }

    /// The poster of `media` framed on the canvas by `framing` (resolved for
    /// the theme's canvas): the picture of [`Self::for_canvas`], turned,
    /// cropped, scaled and padded by the framing's [`geometry`]. A video of
    /// unknown size is only turned.
    pub fn framed(canvas: Size, media: &MediaInfo, framing: &ResolvedFraming) -> Self {
        let long = media
            .video
            .and_then(|track| track.duration)
            .is_some_and(|d| d >= POSTER_AT * 2);
        let at = if long && media.format != MediaFormat::Gif {
            POSTER_AT
        } else {
            Duration::ZERO
        };
        let framed = media.dimensions.map_or_else(
            || FramingGeometry::turning(framing.turns, canvas),
            |size| geometry(size, framing, canvas),
        );
        Self {
            size: canvas,
            at,
            quarter_turns: framed.turns,
            crop: framed.crop,
            pad: framed.pad,
        }
    }

    /// How the video becomes the poster: turned, cropped, scaled to the
    /// canvas or padded into it.
    pub fn geometry(&self) -> FramingGeometry {
        FramingGeometry {
            turns: self.quarter_turns % 4,
            crop: self.crop,
            size: self.size,
            pad: self.pad,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::media::tests::{animated_gif, mp4, still};
    use crate::domain::media::{MediaInfo, VideoTrack};

    fn lasting(media: MediaInfo, duration: Option<Duration>) -> MediaInfo {
        let track = media.video.map(|t| VideoTrack { duration, ..t });
        MediaInfo {
            video: track,
            ..media
        }
    }

    #[test]
    fn the_poster_is_one_second_in_and_covers_the_canvas() {
        let wide = Size::new(1920, 480);
        let clip = mp4(Size::new(1920, 1080), 1000);
        let spec = PosterSpec::for_canvas(wide, &clip);
        assert_eq!(spec.size, wide);
        assert_eq!(spec.at, POSTER_AT);
        // A 16:9 picture on a 4:1 canvas keeps a 1920x480 band in the middle.
        assert_eq!(spec.crop, Some(Rect::new(0, 300, 1920, 480)));
        // The same shape needs no crop.
        let fitting = mp4(Size::new(3840, 960), 1000);
        assert_eq!(PosterSpec::for_canvas(wide, &fitting).crop, None);
        let unknown = MediaInfo {
            dimensions: None,
            ..clip.clone()
        };
        assert_eq!(PosterSpec::for_canvas(wide, &unknown).crop, None);
        assert_eq!((spec.quarter_turns, spec.pad), (0, None));
    }

    #[test]
    fn the_poster_follows_the_framing() {
        use crate::domain::framing::{VideoFit, VideoFraming};
        use crate::domain::geometry::Orientation;

        // Dragon Ball: the pre-turned 480x1920 video stands on its 1920x480
        // canvas, turned back and nothing cut.
        let canvas = Size::new(1920, 480);
        let dragon = mp4(Size::new(480, 1920), 2_588_343);
        let panel = crate::domain::framing::PanelLayout::for_canvas(canvas);
        let auto =
            VideoFraming::default().resolve(dragon.dimensions, Orientation::Landscape, panel);
        let spec = PosterSpec::framed(canvas, &dragon, &auto);
        assert_eq!((spec.quarter_turns, spec.crop, spec.pad), (3, None, None));
        assert_eq!(
            spec.geometry(),
            geometry(Size::new(480, 1920), &auto, canvas)
        );
        // Fitted: padded on the canvas.
        let clip = mp4(Size::new(1920, 1080), 1000);
        let fitted = ResolvedFraming {
            fit: VideoFit::Contain,
            ..ResolvedFraming::plain(0)
        };
        let spec = PosterSpec::framed(canvas, &clip, &fitted);
        assert_eq!(
            spec.pad.map(|p| (p.scaled, p.x)),
            Some((Size::new(852, 480), 534))
        );
        // Unknown size: turned only.
        let unknown = MediaInfo {
            dimensions: None,
            ..clip
        };
        let spec = PosterSpec::framed(canvas, &unknown, &ResolvedFraming::plain(1));
        assert_eq!((spec.quarter_turns, spec.crop, spec.pad), (1, None, None));
    }

    #[test]
    fn short_clips_and_gifs_show_their_first_picture() {
        let canvas = Size::new(480, 1920);
        let at = |media: &MediaInfo| PosterSpec::for_canvas(canvas, media).at;
        let clip = mp4(Size::new(480, 1920), 1000);
        assert_eq!(at(&lasting(clip.clone(), Some(POSTER_AT * 2))), POSTER_AT);
        let short = Duration::from_millis(1500);
        assert_eq!(at(&lasting(clip.clone(), Some(short))), Duration::ZERO);
        assert_eq!(at(&lasting(clip, None)), Duration::ZERO, "unknown length");
        let gif = animated_gif(Size::new(480, 1920), 10);
        assert_eq!(at(&gif), Duration::ZERO, "a GIF's first picture");
        assert_eq!(at(&still(MediaFormat::Png, 1)), Duration::ZERO);
    }
}
