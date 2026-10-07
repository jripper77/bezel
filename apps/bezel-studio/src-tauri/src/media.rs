//! What the media panel shows of an asset: its kind and a small preview;
//! which files make a video background; and a poster as a PNG.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::theme::AssetRef;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder as _, ImageFormat, RgbaImage};

/// Longest side of a media thumbnail, pixels.
pub const THUMBNAIL_SIDE: u32 = 96;

/// Bound IPC memory usage when opening a video in the native WebView player.
pub const VIDEO_PREVIEW_LIMIT: usize = 64 * 1024 * 1024;

pub fn video_data_url(name: &str, bytes: &[u8]) -> Option<String> {
    if bytes.len() > VIDEO_PREVIEW_LIMIT {
        return None;
    }
    let mime = match extension_of(name).as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "gif" => "image/gif",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

/// Extensions the image picker offers (the formats the renderer decodes).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg"];

/// Extensions the picker of files to send to a screen offers: the images
/// screens show and the videos ffmpeg converts.
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "gif", "mp4", "mov", "m4v", "mkv", "webm", "avi",
];

/// Extensions of the videos a theme's background takes (those ffmpeg
/// converts for a screen).
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi"];

/// Extensions the picker of a video background offers: the videos, and GIFs
/// (an animated one is a video background).
pub const BACKGROUND_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "gif"];

/// The lowercase extension of a file name or asset reference.
pub fn extension_of(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    base.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// `image`, `font`, `video` or `other`, from the file extension.
pub fn kind_of(asset: &AssetRef) -> &'static str {
    let extension = extension_of(&asset.0);
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => "image",
        "ttf" | "otf" | "ttc" => "font",
        e if VIDEO_EXTENSIONS.contains(&e) => "video",
        _ => "other",
    }
}

/// Whether `bytes` are a GIF of more than one picture: a moving picture,
/// which a theme can use as its video background.
pub fn is_animated_gif(bytes: &[u8]) -> bool {
    let Ok(decoder) = GifDecoder::new(Cursor::new(bytes)) else {
        return false;
    };
    decoder.into_frames().take(2).filter(Result::is_ok).count() > 1
}

/// `frame` as a PNG file (a poster asset).
pub fn png_of(frame: &Frame) -> Option<Vec<u8>> {
    let size = frame.size();
    let image = RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())?;
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

/// A PNG data URL of the image scaled to fit [`THUMBNAIL_SIDE`], or `None`
/// when the bytes are not a decodable image.
pub fn thumbnail_data_url(bytes: &[u8]) -> Option<String> {
    if let Some(png) = bezel_render::svg_thumbnail(bytes, THUMBNAIL_SIDE) {
        return Some(format!("data:image/png;base64,{}", STANDARD.encode(png)));
    }
    let image = image::load_from_memory(bytes).ok()?;
    let small = image.thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE);
    let mut png = Cursor::new(Vec::new());
    small.write_to(&mut png, ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn kinds_follow_the_extension() {
        let kind = |s: &str| kind_of(&AssetRef(s.into()));
        assert_eq!(kind("assets/a.PNG"), "image");
        assert_eq!(kind("assets/f.otf"), "font");
        assert_eq!(kind("assets/v.mp4"), "video");
        assert_eq!(kind("assets/v.M4V"), "video");
        assert_eq!(kind("assets/readme"), "other");
        assert_eq!(extension_of("C:\\Clips\\a.b\\Ondas.MOV"), "mov");
        assert_eq!(extension_of("dir.d/noext"), "");
        for video in VIDEO_EXTENSIONS {
            assert!(BACKGROUND_EXTENSIONS.contains(video), "{video}");
            assert!(
                MEDIA_EXTENSIONS.contains(video),
                "sent like any video: {video}"
            );
        }
        assert!(BACKGROUND_EXTENSIONS.contains(&"gif"));
    }

    #[test]
    fn video_preview_rejects_unrelated_assets_and_oversized_payloads() {
        assert_eq!(
            video_data_url("assets/a.MP4", b"clip").unwrap(),
            "data:video/mp4;base64,Y2xpcA=="
        );
        assert!(video_data_url("assets/config.json", b"secret").is_none());
        assert!(video_data_url("assets/movie.mp4", &vec![0; VIDEO_PREVIEW_LIMIT + 1]).is_none());
    }

    /// A GIF of `count` 2x2 pictures.
    pub(crate) fn gif(count: u8) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame as Picture};
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            let pictures = (0..count).map(|i| {
                let picture = RgbaImage::from_pixel(2, 2, Rgba([i * 60, 0, 0, 255]));
                Picture::from_parts(picture, 0, 0, Delay::from_numer_denom_ms(100, 1))
            });
            encoder.encode_frames(pictures).unwrap();
        }
        bytes
    }

    #[test]
    fn only_gifs_of_several_pictures_move() {
        assert!(is_animated_gif(&gif(3)));
        assert!(!is_animated_gif(&gif(1)));
        assert!(!is_animated_gif(b"GIF89a but not really"));
        assert!(!is_animated_gif(b"\x89PNG"));
    }

    #[test]
    fn posters_are_pngs_of_the_frame() {
        use bezel_core::domain::frame::Rgba as Color;
        use bezel_core::domain::geometry::Size;
        let frame = Frame::filled(Size::new(3, 2), Color::opaque(10, 20, 30));
        let png = png_of(&frame).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((back.width(), back.height()), (3, 2));
        assert_eq!(back.get_pixel(2, 1).0, [10, 20, 30, 255]);
    }

    #[test]
    fn thumbnails_fit_the_side_and_reject_garbage() {
        let big = RgbaImage::from_pixel(400, 200, Rgba([255, 0, 0, 255]));
        let mut bytes = Cursor::new(Vec::new());
        big.write_to(&mut bytes, ImageFormat::Png).unwrap();
        let url = thumbnail_data_url(bytes.get_ref()).unwrap();
        let png = STANDARD
            .decode(url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        let small = image::load_from_memory(&png).unwrap();
        assert_eq!((small.width(), small.height()), (96, 48));
        assert_eq!(thumbnail_data_url(b"nope"), None);
    }
}
