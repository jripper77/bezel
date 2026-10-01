//! The poster of a theme's video background: the still picture previews draw
//! under the elements, and a screen shows until the video plays
//! (D-2026-09-30-storage-video-4). Taking it is the media converter's job
//! (`MediaTranscoder::poster`); this module decides which picture and how it
//! is framed.

use std::time::Duration;

use super::frame::Rect;
use super::geometry::Size;
use super::media::{MediaFormat, MediaInfo, cover_crop};

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
    /// The centered part of the video that covers the canvas, in video
    /// pixels ([`cover_crop`], as a conversion for a screen crops it).
    /// `None` when the shapes already match or the video's size is unknown:
    /// the converter scales the picture to cover the canvas and keeps its
    /// middle.
    pub crop: Option<Rect>,
}

impl PosterSpec {
    /// The poster of `media` for a theme whose canvas is `canvas`: the
    /// picture at [`POSTER_AT`], or the first one of an animated GIF and of
    /// a clip shorter than twice that (or of unknown length). The video is
    /// taken to stand like the theme, as a conversion for a screen takes it:
    /// it is cropped to the canvas's shape, never turned.
    pub fn for_canvas(canvas: Size, media: &MediaInfo) -> Self {
        let long = media
            .video
            .and_then(|track| track.duration)
            .is_some_and(|d| d >= POSTER_AT * 2);
        let at = if long && media.format != MediaFormat::Gif {
            POSTER_AT
        } else {
            Duration::ZERO
        };
        Self {
            size: canvas,
            at,
            crop: media
                .dimensions
                .and_then(|size| cover_crop(size, 0, canvas)),
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
