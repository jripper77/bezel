//! Media files for screens: what a local file is (as probed), what a screen
//! family stores and plays (its [`UploadProfile`]), and the conversion that
//! brings a video into that profile.
//!
//! Profiles come from `docs/reverse-engineering/video.md` and decision
//! D-2026-09-30-storage-video-3. Converting and decoding are adapters behind
//! the `MediaTranscoder` port; this module only decides.

use std::fmt;
use std::time::Duration;

use super::device::{DeviceModel, Family};
use super::frame::Rect;
use super::framing::{FramingGeometry, Pad, ResolvedFraming, geometry};
use super::geometry::{Orientation, Size};
use super::storage::{FileName, MAX_UPLOAD_BYTES, REV_C_MAX_UPLOAD_BYTES};
use super::theme::AssetRef;

/// Still picture or moving picture. Also names the two folders every storage
/// medium of a screen has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MediaKind {
    /// A still image (GIF included: screens store and show it as an image).
    Image,
    /// A video.
    Video,
}

impl MediaKind {
    /// Both kinds.
    pub const ALL: [MediaKind; 2] = [MediaKind::Image, MediaKind::Video];

    /// Stable machine name (`image`, `video`).
    pub const fn slug(self) -> &'static str {
        match self {
            MediaKind::Image => "image",
            MediaKind::Video => "video",
        }
    }

    /// The kind named by `slug`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.slug() == slug)
    }
}

impl fmt::Display for MediaKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// File formats Bezel tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaFormat {
    /// MP4 container.
    Mp4,
    /// A raw H.264 elementary stream in Annex-B form (`.h264`).
    H264,
    /// JPEG image.
    Jpeg,
    /// PNG image.
    Png,
    /// BMP image.
    Bmp,
    /// GIF image (possibly animated).
    Gif,
    /// Any other container or image format.
    Other,
}

impl MediaFormat {
    /// File-name extensions of the format, preferred first; none for `Other`.
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            MediaFormat::Mp4 => &["mp4"],
            MediaFormat::H264 => &["h264"],
            MediaFormat::Jpeg => &["jpg", "jpeg"],
            MediaFormat::Png => &["png"],
            MediaFormat::Bmp => &["bmp"],
            MediaFormat::Gif => &["gif"],
            MediaFormat::Other => &[],
        }
    }

    /// True for the still-image formats.
    pub const fn is_still(self) -> bool {
        matches!(
            self,
            MediaFormat::Jpeg | MediaFormat::Png | MediaFormat::Bmp | MediaFormat::Gif
        )
    }

    const fn name(self) -> &'static str {
        match self {
            MediaFormat::Mp4 => "MP4",
            MediaFormat::H264 => "H.264 stream",
            MediaFormat::Jpeg => "JPEG",
            MediaFormat::Png => "PNG",
            MediaFormat::Bmp => "BMP",
            MediaFormat::Gif => "GIF",
            MediaFormat::Other => "another format",
        }
    }
}

impl fmt::Display for MediaFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Video compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    /// H.264 / AVC.
    H264,
    /// Anything else.
    Other,
}

/// Layout of decoded video pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoPixelFormat {
    /// 8-bit YUV with 4:2:0 chroma subsampling, planar.
    Yuv420p,
    /// Anything else.
    Other,
}

/// Frames per second as a ratio (`30000/1001` for 29.97).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    /// Numerator (frames).
    pub num: u32,
    /// Denominator (seconds).
    pub den: u32,
}

impl FrameRate {
    /// A rate; `None` when either term is zero.
    pub const fn new(num: u32, den: u32) -> Option<Self> {
        if num == 0 || den == 0 {
            None
        } else {
            Some(Self { num, den })
        }
    }

    /// Frames per second.
    pub fn fps(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

/// The video track of a media file. Unknown properties are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoTrack {
    /// Compression.
    pub codec: VideoCodec,
    /// Decoded pixel layout.
    pub pixel_format: Option<VideoPixelFormat>,
    /// Whether the stream uses B-frames.
    pub b_frames: Option<bool>,
    /// Frame rate.
    pub frame_rate: Option<FrameRate>,
    /// Play time.
    pub duration: Option<Duration>,
}

/// What a local media file is, as probed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInfo {
    /// Container or image format.
    pub format: MediaFormat,
    /// File size in bytes.
    pub bytes: u64,
    /// Picture size in pixels, when known.
    pub dimensions: Option<Size>,
    /// The video track; `None` for still images.
    pub video: Option<VideoTrack>,
    /// True when the file carries an audio track.
    pub has_audio: bool,
}

impl MediaInfo {
    /// `Image` for still formats, `Video` for a file with a video track,
    /// `None` for anything else.
    pub fn kind(&self) -> Option<MediaKind> {
        if self.format.is_still() {
            Some(MediaKind::Image)
        } else if self.video.is_some() {
            Some(MediaKind::Video)
        } else {
            None
        }
    }

    /// Whether the file can go to a folder of `kind`: a still format as an
    /// image, anything that moves as a video. An animated GIF (a still
    /// format with a video track) can go to both: shown as it is in the
    /// image folder, converted in the video folder.
    pub fn stores_as(&self, kind: MediaKind) -> bool {
        match kind {
            MediaKind::Image => self.format.is_still(),
            MediaKind::Video => self.video.is_some(),
        }
    }
}

/// Whether the external media converter can be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaTools {
    /// Found and runnable: videos can be converted and decoded on the host.
    Ready {
        /// Version the tool reported.
        version: String,
    },
    /// Not found: images still work and a video already in the screen's
    /// profile still uploads; everything else explains how to install it.
    Missing {
        /// Install commands for the user's system, most likely first.
        install_hints: Vec<String>,
    },
}

impl MediaTools {
    /// Whether a conversion can run.
    pub fn converter(&self) -> Converter {
        match self {
            MediaTools::Ready { .. } => Converter::Available,
            MediaTools::Missing { .. } => Converter::Missing,
        }
    }
}

/// Whether a conversion can run (an input of the upload preflight).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Converter {
    /// Conversions can run.
    Available,
    /// No converter: only files already in the profile can be uploaded.
    Missing,
}

/// Whether a video may contain B-frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BFrames {
    /// Any GOP structure.
    Allowed,
    /// The screen's decoder takes none (TUR_USB).
    Forbidden,
}

/// Colour treatment of a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    /// Colours unchanged.
    #[default]
    Natural,
    /// The vendor's TUR_USB darkening (`brightness -0.1, contrast 0.9`).
    Darkened,
}

/// Adjustments the user asks for when a video is converted. The default
/// changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConvertOptions {
    /// Clockwise quarter turns applied to the source first (0..=3). For a
    /// theme video: `theme.orientation.quarter_turns_to(model.native_orientation)`.
    pub quarter_turns: u8,
    /// Part of the (rotated) source to keep, in source pixels; `None` keeps
    /// the whole picture. The result is then scaled to the panel.
    pub crop: Option<Rect>,
    /// Where the scaled picture sits on the panel when its framing fits it
    /// inside (D-2026-10-01-video-background-framing-3); `None`, today's
    /// chain: it is scaled to the whole panel.
    pub pad: Option<Pad>,
    /// Output frame rate (the vendor's optional 24 fps); `None` keeps the
    /// source's.
    pub frame_rate: Option<u32>,
    /// Colour treatment.
    pub tone: Tone,
}

/// The centered part of a `source` picture that, turned `quarter_turns`
/// clockwise, fills `target` without distortion ("cover"): what a video of
/// another shape keeps before it is scaled to the panel. `None` when the
/// shapes already match (the vendor then only scales). Edges are even, as
/// yuv420p needs. The crop of the plain framing's [`geometry`].
pub fn cover_crop(source: Size, quarter_turns: u8, target: Size) -> Option<Rect> {
    geometry(source, &ResolvedFraming::plain(quarter_turns), target).crop
}

/// The conversion that makes a video stand on `model`'s panel the way it
/// stands in `orientation`: turned by the quarter turns from `orientation`
/// to the panel's native one, then cropped to cover the panel without
/// distortion ([`cover_crop`]; no crop when its size is unknown or already
/// has the panel's shape). Still pictures, and screens that store no media,
/// need no adjustment; an animated GIF is fitted like a video. The frame
/// rate and tone are left to the caller. [`framed_options`] with the plain
/// framing.
pub fn fitting_options(
    model: &DeviceModel,
    orientation: Orientation,
    media: &MediaInfo,
) -> ConvertOptions {
    framed_options(model, orientation, media, &ResolvedFraming::plain(0))
}

/// The conversion that puts a theme's video `media` on `model`'s panel as
/// `framing` (resolved for the theme's canvas in `orientation`) frames it on
/// the canvas: the framing turned to the panel ([`ResolvedFraming::turned`])
/// and its [`geometry`] onto the panel's native picture. A video of unknown
/// size is only turned. Still pictures, and screens that store no media,
/// need no adjustment. The frame rate and tone are left to the caller.
pub fn framed_options(
    model: &DeviceModel,
    orientation: Orientation,
    media: &MediaInfo,
    framing: &ResolvedFraming,
) -> ConvertOptions {
    let Some(profile) = UploadProfile::for_model(model) else {
        return ConvertOptions::default();
    };
    if media.video.is_none() {
        return ConvertOptions::default();
    }
    let on_panel = framing.turned(orientation.quarter_turns_to(model.native_orientation));
    let framed = match media.dimensions {
        Some(size) => geometry(size, &on_panel, profile.video_size),
        None => FramingGeometry::turning(on_panel.turns, profile.video_size),
    };
    ConvertOptions::from_geometry(&framed)
}

impl ConvertOptions {
    /// The options that turn, crop and pad as `geometry` says, the rest
    /// left as it is.
    pub fn from_geometry(geometry: &FramingGeometry) -> Self {
        Self {
            quarter_turns: geometry.turns,
            crop: geometry.crop,
            pad: geometry.pad,
            ..Self::default()
        }
    }

    /// True when the options leave the picture as it is.
    pub fn is_identity(&self) -> bool {
        self.quarter_turns.is_multiple_of(4)
            && self.crop.is_none()
            && self.pad.is_none()
            && self.frame_rate.is_none()
            && self.tone == Tone::Natural
    }

    /// These options for converting `media`. An animated GIF shows each
    /// picture for its own delay, but a screen's video plays at one rate:
    /// without a rate asked for, its pictures are resampled at the rate of
    /// its shortest delay (rounded up, at most [`MAX_RESAMPLED_FPS`]), so
    /// every delay keeps its length. Other sources keep their own rate.
    pub fn for_source(self, media: &MediaInfo) -> Self {
        let resampled = media
            .video
            .filter(|_| media.format == MediaFormat::Gif)
            .and_then(|track| track.frame_rate)
            .map(|rate| rate.num.div_ceil(rate.den).clamp(1, MAX_RESAMPLED_FPS));
        Self {
            frame_rate: self.frame_rate.or(resampled),
            ..self
        }
    }
}

/// Highest frame rate an animated GIF is resampled at for a screen
/// ([`ConvertOptions::for_source`]).
pub const MAX_RESAMPLED_FPS: u32 = 30;

/// A conversion into a screen's video profile. The output is always H.264
/// with yuv420p pixels, no audio, exactly `size` pixels (the vendor chain:
/// rotate, crop, scale, pad, square pixels, CRF 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscodeTarget {
    /// Output container: [`MediaFormat::Mp4`] or [`MediaFormat::H264`].
    pub format: MediaFormat,
    /// Output picture size: the panel in its native orientation.
    pub size: Size,
    /// Whether the encoder may use B-frames.
    pub b_frames: BFrames,
    /// Clockwise quarter turns applied to the source first (0..=3).
    pub quarter_turns: u8,
    /// Crop in source pixels after the rotation; `None` = whole picture.
    pub crop: Option<Rect>,
    /// Where the scaled picture sits in `size`, and the color around it;
    /// `None` = scaled to the whole `size` (today's chain).
    pub pad: Option<Pad>,
    /// Output frame rate; `None` keeps the source's.
    pub frame_rate: Option<u32>,
    /// Colour treatment.
    pub tone: Tone,
    /// Largest output the screen takes, in bytes: the converter caps the
    /// bitrate from the source's duration so that the output fits (CRF 20
    /// stays the quality ceiling). `None`, or a source of unknown duration:
    /// no cap.
    pub max_bytes: Option<u64>,
}

impl TranscodeTarget {
    /// How the source becomes the output picture: turned, cropped, scaled to
    /// `size` or padded into it.
    pub fn geometry(&self) -> FramingGeometry {
        FramingGeometry {
            turns: self.quarter_turns % 4,
            crop: self.crop,
            size: self.size,
            pad: self.pad,
        }
    }
}

/// Highest rate a studio preview decodes a video background at
/// (D-2026-10-01-video-background-framing-5).
pub const PREVIEW_FPS: u32 = 15;

/// Host-side decoding of a video or animated GIF into frames: for screens
/// that cannot play stored videos, and for previews. The decoder hands over
/// the whole source picture, never turned or cropped, scaled to `size`; the
/// caller frames each picture with the [`geometry`] of its framing, so a
/// framing edit never restarts the decoder
/// (D-2026-10-01-video-background-framing-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSpec {
    /// Size of the frames produced: the whole source picture scaled to it.
    pub size: Size,
    /// Frames per second produced; callers cap it to the link's rate.
    pub fps: u32,
}

impl StreamSpec {
    /// Decoding a `source` picture shown on `canvas`: its own shape, scaled
    /// down (never up) so that its longer side is at most twice the
    /// canvas's longer side, at `fps`.
    pub fn raw(source: Size, canvas: Size, fps: u32) -> Self {
        let longest = u64::from(source.width.max(source.height));
        let limit = 2 * u64::from(canvas.width.max(canvas.height));
        if longest <= limit || longest == 0 {
            return Self { size: source, fps };
        }
        let side = |v: u32| {
            let scaled = (u64::from(v) * limit + longest / 2) / longest;
            u32::try_from(scaled.max(1)).unwrap_or(u32::MAX)
        };
        Self {
            size: Size::new(side(source.width), side(source.height)),
            fps,
        }
    }
}

/// One way a file differs from what the screen accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// Not one of the accepted formats.
    Format {
        /// The file's format.
        found: MediaFormat,
        /// What the screen takes for this kind.
        accepted: &'static [MediaFormat],
    },
    /// The video is not H.264.
    Codec(VideoCodec),
    /// The pixels are not yuv420p (`None`: unknown).
    PixelFormat(Option<VideoPixelFormat>),
    /// The video uses (or may use) B-frames where the screen takes none.
    BFrames,
    /// The file has an audio track; screens play none.
    Audio,
    /// The picture is not exactly the panel's native size.
    Resolution {
        /// The panel in its native orientation.
        expected: Size,
        /// The file's picture size, when known.
        found: Option<Size>,
    },
}

fn size_text(size: Option<Size>) -> String {
    size.map_or_else(
        || "an unknown size".to_string(),
        |s| format!("{}x{}", s.width, s.height),
    )
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mismatch::Format { found, accepted } => {
                let names: Vec<&str> = accepted.iter().map(|a| a.name()).collect();
                write!(f, "{found} (expected {})", names.join(", "))
            }
            Mismatch::Codec(_) => f.write_str("the video is not H.264"),
            Mismatch::PixelFormat(Some(_)) => f.write_str("the pixels are not yuv420p"),
            Mismatch::PixelFormat(None) => f.write_str("the pixel format is unknown"),
            Mismatch::BFrames => f.write_str("the video may use B-frames"),
            Mismatch::Audio => f.write_str("the file has an audio track"),
            Mismatch::Resolution { expected, found } => write!(
                f,
                "{} instead of {}",
                size_text(*found),
                size_text(Some(*expected))
            ),
        }
    }
}

const STILLS: &[MediaFormat] = &[
    MediaFormat::Jpeg,
    MediaFormat::Png,
    MediaFormat::Bmp,
    MediaFormat::Gif,
];
const MP4_ONLY: &[MediaFormat] = &[MediaFormat::Mp4];
const H264_ONLY: &[MediaFormat] = &[MediaFormat::H264];

/// What a screen family stores and plays, checked before any conversion or
/// upload (D-2026-09-30-storage-video-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadProfile {
    /// Container of stored videos: [`MediaFormat::Mp4`] (rev C) or
    /// [`MediaFormat::H264`] (TUR_USB).
    pub video_format: MediaFormat,
    /// Exact picture size of stored videos: the panel in its native orientation.
    pub video_size: Size,
    /// Whether stored videos may use B-frames.
    pub b_frames: BFrames,
    /// Still-image formats the screen shows (the vendor device page's list).
    pub image_formats: &'static [MediaFormat],
    /// Largest file the screen takes, in bytes: rev C
    /// [`REV_C_MAX_UPLOAD_BYTES`] (D-2026-09-30-release-polish-12), TUR_USB
    /// the vendor's [`MAX_UPLOAD_BYTES`]. Always below
    /// [`super::storage::DEVICE_SIZE_LIMIT`].
    pub max_upload_bytes: u64,
}

impl UploadProfile {
    /// The profile of `model`; `None` for screens that store no media.
    pub fn for_model(model: &DeviceModel) -> Option<Self> {
        if !model.capabilities.storage {
            return None;
        }
        let video_size = model
            .panel
            .portrait()
            .in_orientation(model.native_orientation);
        let (video_format, b_frames, max_upload_bytes) = match model.family {
            Family::TuringRevC => (MediaFormat::Mp4, BFrames::Allowed, REV_C_MAX_UPLOAD_BYTES),
            Family::TuringUsb => (MediaFormat::H264, BFrames::Forbidden, MAX_UPLOAD_BYTES),
            _ => return None,
        };
        Some(Self {
            video_format,
            video_size,
            b_frames,
            image_formats: STILLS,
            max_upload_bytes,
        })
    }

    /// The formats accepted for `kind`.
    pub fn accepted(&self, kind: MediaKind) -> &'static [MediaFormat] {
        match (kind, self.video_format) {
            (MediaKind::Image, _) => self.image_formats,
            (MediaKind::Video, MediaFormat::H264) => H264_ONLY,
            (MediaKind::Video, _) => MP4_ONLY,
        }
    }

    /// Every way `media` differs from what the screen accepts as `kind`
    /// (empty when it can be uploaded as it is). Images are checked by
    /// format only.
    pub fn mismatches(&self, kind: MediaKind, media: &MediaInfo) -> Vec<Mismatch> {
        let mut out = Vec::new();
        let accepted = self.accepted(kind);
        if !accepted.contains(&media.format) {
            out.push(Mismatch::Format {
                found: media.format,
                accepted,
            });
        }
        if kind == MediaKind::Video {
            self.video_mismatches(media, &mut out);
        }
        out
    }

    fn video_mismatches(&self, media: &MediaInfo, out: &mut Vec<Mismatch>) {
        let track = media.video.unwrap_or(VideoTrack {
            codec: VideoCodec::Other,
            pixel_format: None,
            b_frames: None,
            frame_rate: None,
            duration: None,
        });
        if track.codec != VideoCodec::H264 {
            out.push(Mismatch::Codec(track.codec));
        }
        if track.pixel_format != Some(VideoPixelFormat::Yuv420p) {
            out.push(Mismatch::PixelFormat(track.pixel_format));
        }
        if self.b_frames == BFrames::Forbidden && track.b_frames != Some(false) {
            out.push(Mismatch::BFrames);
        }
        if media.has_audio {
            out.push(Mismatch::Audio);
        }
        if media.dimensions != Some(self.video_size) {
            out.push(Mismatch::Resolution {
                expected: self.video_size,
                found: media.dimensions,
            });
        }
    }

    /// The conversion of a video into this profile with the user's
    /// `options`, its output capped at the screen's per-file limit.
    pub fn transcode_target(&self, options: ConvertOptions) -> TranscodeTarget {
        TranscodeTarget {
            format: self.video_format,
            size: self.video_size,
            b_frames: self.b_frames,
            quarter_turns: options.quarter_turns % 4,
            crop: options.crop,
            pad: options.pad,
            frame_rate: options.frame_rate,
            tone: options.tone,
            max_bytes: Some(self.max_upload_bytes),
        }
    }

    /// The extension a stored copy of `media` gets: the profile's video
    /// container for videos (converted or not), the image's own for images.
    /// `None` for files the screen cannot take.
    pub fn extension_for(&self, media: &MediaInfo) -> Option<&'static str> {
        match media.kind()? {
            MediaKind::Video => self.video_format.extensions().first().copied(),
            MediaKind::Image if self.image_formats.contains(&media.format) => {
                media.format.extensions().first().copied()
            }
            MediaKind::Image => None,
        }
    }

    /// A valid upload name for a host file called `host_name` holding `media`.
    pub fn suggest_name(&self, host_name: &str, media: &MediaInfo) -> Option<FileName> {
        self.extension_for(media)
            .map(|ext| FileName::suggest(host_name, ext))
    }
}

/// The name a theme's video has on a screen (D-2026-10-01-video-background-
/// framing-4): the asset's file name, the vendor's suffix for a copy turned
/// to the panel (`_90`, `_180`, `_270` clockwise, from `framing.turns`), for
/// a framing other than the plain one `_f` and its 8 hex
/// [`ResolvedFraming::fingerprint`], and the screen's video extension, as an
/// upload name (`assets/AMD.mp4` turned once for an MP4 screen:
/// `amd_90.mp4`; zoomed: `amd_90_f1a2b3c4.mp4`). `framing` is the theme's
/// resolved framing turned to the panel ([`ResolvedFraming::turned`]): its
/// turns are the total ones (the pre-turned Dragon Ball video: none, so
/// `dragon.mp4`). A long stem is shortened, never the suffixes. The runtime
/// looks for it on a screen; the cleanup assistant never suggests it
/// (D-2026-09-30-storage-manager-9).
pub fn device_video_name(
    asset: &AssetRef,
    framing: &ResolvedFraming,
    profile: &UploadProfile,
) -> FileName {
    video_name(asset, framing.turns, framing.fingerprint(), profile)
}

/// [`device_video_name`] from the total `quarter_turns` and the framing's
/// `fingerprint` (`None`: plain).
pub(crate) fn video_name(
    asset: &AssetRef,
    quarter_turns: u8,
    fingerprint: Option<u32>,
    profile: &UploadProfile,
) -> FileName {
    let file = asset.0.rsplit(['/', '\\']).next().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let turned = match quarter_turns % 4 {
        1 => "_90",
        2 => "_180",
        3 => "_270",
        _ => "",
    };
    let framed = fingerprint.map_or_else(String::new, |hash| format!("_f{hash:08x}"));
    let extension = profile.video_format.extensions().first().copied();
    let extension = extension.unwrap_or_default();
    // Every character of the stem becomes at most one of the name: keeping
    // `room` characters leaves room for the suffixes.
    let suffixes = turned.len() + framed.len() + extension.len() + 1;
    let room = FileName::MAX_BYTES.saturating_sub(suffixes);
    let stem: String = stem.chars().take(room).collect();
    FileName::suggest(&format!("{stem}{turned}{framed}.{extension}"), extension)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn cover_crop_keeps_the_middle_of_another_shape() {
        let panel = Size::new(480, 1920);
        // A 1920x1080 landscape clip turned once for the 8.8": 1080x1920 after
        // the turn, wider than 1:4, so the sides go.
        assert_eq!(
            cover_crop(Size::new(1920, 1080), 1, panel),
            Some(Rect {
                x: 300,
                y: 0,
                width: 480,
                height: 1920
            })
        );
        // Unturned 1920x1080 onto 480x1920: keep a 270x1080 column.
        assert_eq!(
            cover_crop(Size::new(1920, 1080), 0, panel),
            Some(Rect {
                x: 824,
                y: 0,
                width: 270,
                height: 1080
            })
        );
        // Taller than the panel: top and bottom go.
        assert_eq!(
            cover_crop(Size::new(480, 2400), 0, panel),
            Some(Rect {
                x: 0,
                y: 240,
                width: 480,
                height: 1920
            })
        );
        assert_eq!(
            cover_crop(Size::new(960, 3840), 0, panel),
            None,
            "same shape: scale only"
        );
        assert_eq!(cover_crop(Size::new(1920, 480), 1, panel), None);
        assert_eq!(cover_crop(Size::new(0, 10), 0, panel), None);
    }

    #[test]
    fn fitting_options_turn_and_cover_crop() {
        let model = |id| model_by_id(ModelId(id)).expect("model");
        // The 8.8" stands natively upside down (reverse portrait): a
        // landscape clip turns once and loses its sides.
        let screen = model("turing-8.8");
        let wide = mp4(Size::new(1920, 1080), 1000);
        let fitted = fitting_options(screen, Orientation::Landscape, &wide);
        assert_eq!(fitted.quarter_turns, 1);
        assert_eq!(
            fitted.crop,
            cover_crop(Size::new(1920, 1080), 1, Size::new(480, 1920))
        );
        assert_eq!((fitted.frame_rate, fitted.tone), (None, Tone::Natural));
        let native = mp4(Size::new(480, 1920), 1000);
        let upright = fitting_options(screen, Orientation::Portrait, &native);
        assert_eq!((upright.quarter_turns, upright.crop), (2, None));
        let unknown = MediaInfo {
            dimensions: None,
            ..wide.clone()
        };
        let turned = fitting_options(screen, Orientation::ReverseLandscape, &unknown);
        assert_eq!((turned.quarter_turns, turned.crop), (3, None));
        // Pictures and screens without storage need nothing.
        let picture = still(MediaFormat::Png, 10);
        let none = ConvertOptions::default();
        assert_eq!(
            fitting_options(screen, Orientation::Landscape, &picture),
            none
        );
        let tiny = model("weact-fs-0.96");
        assert_eq!(fitting_options(tiny, Orientation::Landscape, &wide), none);
    }

    /// An animated GIF of `size` whose shortest delay is `delay_cs`
    /// hundredths of a second, playing 3 s.
    pub(crate) fn animated_gif(size: Size, delay_cs: u32) -> MediaInfo {
        MediaInfo {
            format: MediaFormat::Gif,
            bytes: 1000,
            dimensions: Some(size),
            video: Some(VideoTrack {
                codec: VideoCodec::Other,
                pixel_format: Some(VideoPixelFormat::Other),
                b_frames: Some(false),
                frame_rate: FrameRate::new(100, delay_cs),
                duration: Some(Duration::from_secs(3)),
            }),
            has_audio: false,
        }
    }

    #[test]
    fn an_animated_gif_is_an_image_that_can_be_stored_as_a_video() {
        let gif = animated_gif(Size::new(1920, 480), 10);
        assert_eq!(gif.kind(), Some(MediaKind::Image), "the image folder first");
        assert!(gif.stores_as(MediaKind::Image) && gif.stores_as(MediaKind::Video));
        let still_gif = still(MediaFormat::Gif, 10);
        assert!(still_gif.stores_as(MediaKind::Image));
        assert!(
            !still_gif.stores_as(MediaKind::Video),
            "one picture is no video"
        );
        let clip = mp4(Size::new(480, 1920), 10);
        assert!(clip.stores_as(MediaKind::Video) && !clip.stores_as(MediaKind::Image));
        // Fitted to the 8.8" like a video: turned once and cropped.
        let screen = model_by_id(ModelId("turing-8.8")).expect("model");
        let fitted = fitting_options(screen, Orientation::Landscape, &gif);
        assert_eq!(fitted.quarter_turns, 1);
        assert_eq!(
            fitted.crop,
            cover_crop(Size::new(1920, 480), 1, Size::new(480, 1920))
        );
    }

    #[test]
    fn a_gif_is_resampled_at_its_shortest_delay() {
        let options = ConvertOptions::default();
        let rate = |delay_cs| {
            options
                .for_source(&animated_gif(Size::new(64, 64), delay_cs))
                .frame_rate
        };
        assert_eq!(rate(10), Some(10), "10 fps");
        assert_eq!(rate(7), Some(15), "14.3 fps, rounded up");
        assert_eq!(rate(2), Some(MAX_RESAMPLED_FPS), "50 fps is capped");
        assert_eq!(rate(1000), Some(1), "one picture in 10 s");
        // A rate asked for wins; other sources keep their own.
        let asked = ConvertOptions {
            frame_rate: Some(24),
            ..options
        };
        let gif = animated_gif(Size::new(64, 64), 10);
        assert_eq!(asked.for_source(&gif).frame_rate, Some(24));
        let clip = mp4(Size::new(480, 1920), 10);
        assert_eq!(options.for_source(&clip), options);
        assert_eq!(options.for_source(&still(MediaFormat::Gif, 1)), options);
    }

    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;
    use crate::domain::framing::{
        FramingPosition, PanelLayout, Permille, ResolvedFraming, VideoFit, VideoFraming, Zoom,
    };

    /// An MP4 in the rev C profile of a panel whose native size is `size`.
    pub(crate) fn mp4(size: Size, bytes: u64) -> MediaInfo {
        MediaInfo {
            format: MediaFormat::Mp4,
            bytes,
            dimensions: Some(size),
            video: Some(VideoTrack {
                codec: VideoCodec::H264,
                pixel_format: Some(VideoPixelFormat::Yuv420p),
                b_frames: Some(true),
                frame_rate: FrameRate::new(24, 1),
                duration: Some(Duration::from_secs(10)),
            }),
            has_audio: false,
        }
    }

    /// A still image.
    pub(crate) fn still(format: MediaFormat, bytes: u64) -> MediaInfo {
        MediaInfo {
            format,
            bytes,
            dimensions: Some(Size::new(64, 64)),
            video: None,
            has_audio: false,
        }
    }

    /// The upload profile of the catalog model `id`.
    pub(crate) fn profile(id: &'static str) -> Option<UploadProfile> {
        UploadProfile::for_model(model_by_id(ModelId(id)).expect("model"))
    }

    #[test]
    fn profiles_follow_the_family_and_the_native_panel() {
        let rev_c = profile("turing-8.8").expect("8.8 stores media");
        assert_eq!(rev_c.video_format, MediaFormat::Mp4);
        assert_eq!(rev_c.video_size, Size::new(480, 1920));
        assert_eq!(rev_c.b_frames, BFrames::Allowed);
        assert_eq!(rev_c.accepted(MediaKind::Video), &[MediaFormat::Mp4]);
        assert_eq!(rev_c.accepted(MediaKind::Image).len(), 4);
        // The 5" panel is natively landscape.
        assert_eq!(profile("turing-5").unwrap().video_size, Size::new(800, 480));
        let usb = profile("turing-usb-8.8").expect("TUR_USB stores media");
        assert_eq!(usb.video_format, MediaFormat::H264);
        assert_eq!(usb.b_frames, BFrames::Forbidden);
        assert_eq!(usb.accepted(MediaKind::Video), &[MediaFormat::H264]);
        // Per-file caps (D-2026-09-30-release-polish-12), below the 2 GiB
        // the screens parse as signed numbers.
        assert_eq!(rev_c.max_upload_bytes, 26_214_400);
        assert_eq!(usb.max_upload_bytes, 120_000_000);
        for cap in [rev_c.max_upload_bytes, usb.max_upload_bytes] {
            assert!(cap < crate::domain::storage::DEVICE_SIZE_LIMIT);
        }
        assert!(profile("turing-3.5").is_none(), "rev A has no storage");
        assert!(profile("wch-4.3").is_none(), "WCH plays PC-decoded frames");
    }

    #[test]
    fn an_in_profile_video_has_no_mismatch_and_every_difference_is_named() {
        let p = profile("turing-8.8").unwrap();
        let good = mp4(Size::new(480, 1920), 1000);
        assert!(p.mismatches(MediaKind::Video, &good).is_empty());
        let mut bad = good.clone();
        bad.format = MediaFormat::Other;
        bad.has_audio = true;
        bad.dimensions = Some(Size::new(1920, 480));
        bad.video = Some(VideoTrack {
            codec: VideoCodec::Other,
            pixel_format: Some(VideoPixelFormat::Other),
            ..good.video.unwrap()
        });
        let m = p.mismatches(MediaKind::Video, &bad);
        assert_eq!(
            m,
            vec![
                Mismatch::Format {
                    found: MediaFormat::Other,
                    accepted: MP4_ONLY
                },
                Mismatch::Codec(VideoCodec::Other),
                Mismatch::PixelFormat(Some(VideoPixelFormat::Other)),
                Mismatch::Audio,
                Mismatch::Resolution {
                    expected: Size::new(480, 1920),
                    found: Some(Size::new(1920, 480))
                },
            ]
        );
        let text: Vec<String> = m.iter().map(ToString::to_string).collect();
        assert_eq!(text[0], "another format (expected MP4)");
        assert_eq!(text[4], "1920x480 instead of 480x1920");
        assert_eq!(Mismatch::BFrames.to_string(), "the video may use B-frames");
        assert_eq!(
            Mismatch::PixelFormat(None).to_string(),
            "the pixel format is unknown"
        );
        let unknown = Mismatch::Resolution {
            expected: Size::new(1, 1),
            found: None,
        };
        assert!(unknown.to_string().starts_with("an unknown size"));
    }

    #[test]
    fn tur_usb_wants_a_raw_stream_without_b_frames() {
        let p = profile("turing-usb-8.8").unwrap();
        let mut stream = mp4(Size::new(480, 1920), 10);
        stream.format = MediaFormat::H264;
        assert_eq!(
            p.mismatches(MediaKind::Video, &stream),
            vec![Mismatch::BFrames]
        );
        stream.video = stream.video.map(|v| VideoTrack {
            b_frames: Some(false),
            ..v
        });
        assert!(p.mismatches(MediaKind::Video, &stream).is_empty());
        // A file without a video track is reported as not H.264.
        stream.video = None;
        assert!(
            p.mismatches(MediaKind::Video, &stream)
                .contains(&Mismatch::Codec(VideoCodec::Other))
        );
    }

    #[test]
    fn images_are_checked_by_format_and_kinds_come_from_the_file() {
        let mut p = profile("turing-8.8").unwrap();
        assert!(
            p.mismatches(MediaKind::Image, &still(MediaFormat::Gif, 1))
                .is_empty()
        );
        p.image_formats = &[MediaFormat::Png];
        assert_eq!(
            p.mismatches(MediaKind::Image, &still(MediaFormat::Gif, 1)),
            vec![Mismatch::Format {
                found: MediaFormat::Gif,
                accepted: &[MediaFormat::Png]
            }]
        );
        assert_eq!(still(MediaFormat::Gif, 1).kind(), Some(MediaKind::Image));
        assert_eq!(mp4(Size::new(1, 1), 1).kind(), Some(MediaKind::Video));
        assert_eq!(still(MediaFormat::Other, 1).kind(), None);
        assert_eq!(MediaKind::from_slug("video"), Some(MediaKind::Video));
        assert_eq!(MediaKind::from_slug("audio"), None);
        assert_eq!(MediaKind::Image.to_string(), "image");
        assert_eq!(MediaFormat::H264.to_string(), "H.264 stream");
        assert_eq!(MediaFormat::Other.extensions(), &[] as &[&str]);
    }

    #[test]
    fn transcode_target_carries_the_profile_and_the_options() {
        let p = profile("turing-8.8").unwrap();
        let pad = Pad {
            scaled: Size::new(480, 960),
            x: 0,
            y: 480,
            color: crate::domain::frame::Rgba::BLACK,
        };
        let options = ConvertOptions {
            quarter_turns: 5,
            crop: Some(Rect::new(0, 0, 100, 400)),
            pad: Some(pad),
            frame_rate: Some(24),
            tone: Tone::Darkened,
        };
        assert!(!options.is_identity());
        assert!(ConvertOptions::default().is_identity());
        let padded = ConvertOptions {
            pad: Some(pad),
            ..ConvertOptions::default()
        };
        assert!(!padded.is_identity(), "a pad changes the picture");
        let t = p.transcode_target(options);
        assert_eq!(t.format, MediaFormat::Mp4);
        assert_eq!(t.size, Size::new(480, 1920));
        assert_eq!(t.quarter_turns, 1);
        assert_eq!(t.crop, Some(Rect::new(0, 0, 100, 400)));
        assert_eq!(t.pad, Some(pad));
        let g = t.geometry();
        assert_eq!((g.turns, g.crop, g.size, g.pad), (1, t.crop, t.size, t.pad));
        assert_eq!(t.frame_rate, Some(24));
        assert_eq!(t.tone, Tone::Darkened);
        assert_eq!(t.b_frames, BFrames::Allowed);
        assert_eq!(t.max_bytes, Some(REV_C_MAX_UPLOAD_BYTES));
        let usb = profile("turing-usb-8.8").unwrap();
        assert_eq!(
            usb.transcode_target(ConvertOptions::default()).max_bytes,
            Some(MAX_UPLOAD_BYTES)
        );
    }

    #[test]
    fn device_video_names_carry_the_framing() {
        let rev_c = profile("turing-8.8").unwrap();
        let screen = model_by_id(ModelId("turing-8.8")).expect("model");
        let panel = Some(PanelLayout::of(screen));
        let to_panel = Orientation::Landscape.quarter_turns_to(screen.native_orientation);
        let on_panel = |video: Size| {
            VideoFraming::default()
                .resolve(Some(video), Orientation::Landscape, panel)
                .turned(to_panel)
        };
        let name = |asset: &str, framing: &ResolvedFraming| {
            device_video_name(&AssetRef(asset.into()), framing, &rev_c).to_string()
        };
        // The user's Dragon Ball theme: its pre-turned 480x1920 video is the
        // vendor's `dragon.mp4` the screen already stores.
        let dragon = on_panel(Size::new(480, 1920));
        assert_eq!(dragon.turns, 0);
        assert_eq!(name("assets/dragon.mp4", &dragon), "dragon.mp4");
        // A landscape clip in the same theme is turned once, as before.
        let clip = on_panel(Size::new(1920, 1080));
        assert_eq!(name("assets/AMD.mp4", &clip), "amd_90.mp4");
        // Any other framing gets a file of its own.
        let zoomed = ResolvedFraming {
            zoom: Zoom::from_percent(125),
            ..clip
        };
        assert_eq!(name("assets/AMD.mp4", &zoomed), "amd_90_f8ca275f8.mp4");
        let fitted = ResolvedFraming {
            fit: VideoFit::Contain,
            ..dragon
        };
        assert_eq!(name("assets/dragon.mp4", &fitted), "dragon_f8ec2b24d.mp4");
        // Moved up on the canvas is moved right on the panel turned once.
        let up = VideoFraming {
            position: FramingPosition {
                y: Permille::from_permille(400),
                ..FramingPosition::CENTER
            },
            ..VideoFraming::default()
        };
        let up = up
            .resolve(Some(Size::new(1920, 1080)), Orientation::Landscape, panel)
            .turned(to_panel);
        assert_eq!(up.canonical(), "fit=cover;zoom=100;x=600;y=500");
        let up_name = name("assets/AMD.mp4", &up);
        assert!(up_name.starts_with("amd_90_f") && up_name != "amd_90_f8ca275f8.mp4");
        // TUR_USB keeps its raw stream extension.
        let usb = profile("turing-usb-8.8").unwrap();
        let usb_name = device_video_name(&AssetRef("assets/bg.mp4".into()), &zoomed, &usb);
        assert_eq!(usb_name.as_str(), "bg_90_f8ca275f8.h264");
        // A long stem is shortened, never the suffixes.
        let long = format!("assets/{}.mp4", "x".repeat(300));
        let shortened = name(&long, &zoomed);
        assert_eq!(shortened.len(), FileName::MAX_BYTES);
        assert!(shortened.ends_with("x_90_f8ca275f8.mp4"), "{shortened}");
    }

    #[test]
    fn framed_options_turn_the_framing_to_the_panel() {
        let screen = model_by_id(ModelId("turing-8.8")).expect("model");
        let panel = Some(PanelLayout::of(screen));
        // Dragon Ball goes to the 8.8" as it is.
        let dragon = mp4(Size::new(480, 1920), 2_588_343);
        let auto =
            VideoFraming::default().resolve(dragon.dimensions, Orientation::Landscape, panel);
        let options = framed_options(screen, Orientation::Landscape, &dragon, &auto);
        assert!(options.is_identity(), "{options:?}");
        // Fitted on the canvas: fitted on the panel, turned.
        let wide = mp4(Size::new(1920, 1080), 1000);
        let contain = ResolvedFraming {
            fit: VideoFit::Contain,
            ..ResolvedFraming::plain(0)
        };
        let fitted = framed_options(screen, Orientation::Landscape, &wide, &contain);
        assert_eq!((fitted.quarter_turns, fitted.crop), (1, None));
        let pad = fitted.pad.expect("padded");
        assert_eq!((pad.scaled, pad.x, pad.y), (Size::new(480, 852), 0, 534));
        // Unknown size: turned only; a picture or a screen without storage:
        // nothing.
        let unknown = MediaInfo {
            dimensions: None,
            ..wide.clone()
        };
        let turned = framed_options(screen, Orientation::Landscape, &unknown, &contain);
        assert_eq!(
            turned,
            ConvertOptions {
                quarter_turns: 1,
                ..ConvertOptions::default()
            }
        );
        let picture = still(MediaFormat::Png, 1);
        let none = ConvertOptions::default();
        assert_eq!(
            framed_options(screen, Orientation::Landscape, &picture, &contain),
            none
        );
        let tiny = model_by_id(ModelId("weact-fs-0.96")).expect("model");
        assert_eq!(
            framed_options(tiny, Orientation::Landscape, &wide, &contain),
            none
        );
    }

    #[test]
    fn host_decoding_keeps_the_whole_picture_at_most_twice_the_canvas() {
        let canvas = Size::new(1920, 480);
        let dragon = StreamSpec::raw(Size::new(480, 1920), canvas, PREVIEW_FPS);
        assert_eq!(dragon.size, Size::new(480, 1920), "never scaled up");
        assert_eq!(dragon.fps, 15);
        let big = StreamSpec::raw(Size::new(7680, 4320), canvas, 10);
        assert_eq!(big.size, Size::new(3840, 2160));
        let odd = StreamSpec::raw(Size::new(8001, 3), canvas, 10);
        assert_eq!(odd.size, Size::new(3840, 1));
        let tiny = StreamSpec::raw(Size::new(0, 0), canvas, 10);
        assert_eq!(tiny.size, Size::new(0, 0));
    }

    #[test]
    fn suggested_names_take_the_stored_extension() {
        let p = profile("turing-8.8").unwrap();
        let video = mp4(Size::new(1920, 1080), 1);
        assert_eq!(
            p.suggest_name("My Holiday (1).MOV", &video)
                .unwrap()
                .as_str(),
            "my_holiday_1.mp4"
        );
        assert_eq!(
            p.suggest_name("Logo.JPEG", &still(MediaFormat::Jpeg, 1))
                .unwrap()
                .as_str(),
            "logo.jpg"
        );
        assert!(
            p.suggest_name("x.webp", &still(MediaFormat::Other, 1))
                .is_none()
        );
        let mut narrow = p;
        narrow.image_formats = &[MediaFormat::Png];
        assert!(
            narrow
                .suggest_name("x.gif", &still(MediaFormat::Gif, 1))
                .is_none()
        );
        let usb = profile("turing-usb-8.8").unwrap();
        assert_eq!(usb.extension_for(&video), Some("h264"));
    }

    #[test]
    fn frame_rates_and_tools() {
        assert!(FrameRate::new(0, 1).is_none());
        assert!(FrameRate::new(1, 0).is_none());
        let ntsc = FrameRate::new(30000, 1001).unwrap();
        assert!((ntsc.fps() - 29.97).abs() < 0.01);
        let ready = MediaTools::Ready {
            version: "7.1".into(),
        };
        assert_eq!(ready.converter(), Converter::Available);
        let missing = MediaTools::Missing {
            install_hints: vec!["sudo dnf install ffmpeg".into()],
        };
        assert_eq!(missing.converter(), Converter::Missing);
    }
}
