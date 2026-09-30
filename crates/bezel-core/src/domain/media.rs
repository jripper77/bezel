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
use super::geometry::Size;
use super::storage::FileName;

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
/// yuv420p needs.
pub fn cover_crop(source: Size, quarter_turns: u8, target: Size) -> Option<Rect> {
    let turned = if quarter_turns % 2 == 1 {
        source.transposed()
    } else {
        source
    };
    let (sw, sh) = (u64::from(turned.width), u64::from(turned.height));
    let (tw, th) = (u64::from(target.width), u64::from(target.height));
    if sw == 0 || sh == 0 || tw == 0 || th == 0 || sw * th == sh * tw {
        return None;
    }
    let even = |v: u64| u32::try_from(v & !1).unwrap_or(u32::MAX).max(2);
    let (width, height) = if sw * th > sh * tw {
        (even(sh * tw / th), even(sh))
    } else {
        (even(sw), even(sw * th / tw))
    };
    Some(Rect {
        x: ((turned.width - width) / 2) & !1,
        y: ((turned.height - height) / 2) & !1,
        width,
        height,
    })
}

impl ConvertOptions {
    /// True when the options leave the picture as it is.
    pub fn is_identity(&self) -> bool {
        self.quarter_turns.is_multiple_of(4)
            && self.crop.is_none()
            && self.frame_rate.is_none()
            && self.tone == Tone::Natural
    }
}

/// A conversion into a screen's video profile. The output is always H.264
/// with yuv420p pixels, no audio, exactly `size` pixels (the vendor chain:
/// rotate, crop, scale, square pixels, CRF 20).
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
    /// Output frame rate; `None` keeps the source's.
    pub frame_rate: Option<u32>,
    /// Colour treatment.
    pub tone: Tone,
}

/// Host-side decoding of a video or animated GIF into frames (the fallback
/// for screens that cannot play stored videos).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSpec {
    /// Size of the frames produced (the source covers it, cropped to fit).
    pub size: Size,
    /// Frames per second produced; callers cap it to the link's rate.
    pub fps: u32,
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
        let (video_format, b_frames) = match model.family {
            Family::TuringRevC => (MediaFormat::Mp4, BFrames::Allowed),
            Family::TuringUsb => (MediaFormat::H264, BFrames::Forbidden),
            _ => return None,
        };
        Some(Self {
            video_format,
            video_size,
            b_frames,
            image_formats: STILLS,
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

    /// The conversion of a video into this profile with the user's `options`.
    pub fn transcode_target(&self, options: ConvertOptions) -> TranscodeTarget {
        TranscodeTarget {
            format: self.video_format,
            size: self.video_size,
            b_frames: self.b_frames,
            quarter_turns: options.quarter_turns % 4,
            crop: options.crop,
            frame_rate: options.frame_rate,
            tone: options.tone,
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
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;

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
        let options = ConvertOptions {
            quarter_turns: 5,
            crop: Some(Rect::new(0, 0, 100, 400)),
            frame_rate: Some(24),
            tone: Tone::Darkened,
        };
        assert!(!options.is_identity());
        assert!(ConvertOptions::default().is_identity());
        let t = p.transcode_target(options);
        assert_eq!(t.format, MediaFormat::Mp4);
        assert_eq!(t.size, Size::new(480, 1920));
        assert_eq!(t.quarter_turns, 1);
        assert_eq!(t.crop, Some(Rect::new(0, 0, 100, 400)));
        assert_eq!(t.frame_rate, Some(24));
        assert_eq!(t.tone, Tone::Darkened);
        assert_eq!(t.b_frames, BFrames::Allowed);
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
