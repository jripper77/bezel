//! What the media panel shows of an asset: its kind and a small preview.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bezel_core::domain::theme::AssetRef;
use image::ImageFormat;

/// Longest side of a media thumbnail, pixels.
pub const THUMBNAIL_SIDE: u32 = 96;

/// Extensions the image picker offers (the formats the renderer decodes).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif"];

/// Extensions the picker of files to send to a screen offers: the images
/// screens show and the videos ffmpeg converts.
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "gif", "mp4", "mov", "m4v", "mkv", "webm", "avi",
];

/// `image`, `font`, `video` or `other`, from the file extension.
pub fn kind_of(asset: &AssetRef) -> &'static str {
    let extension = asset
        .0
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => "image",
        "ttf" | "otf" | "ttc" => "font",
        "mp4" | "webm" | "mkv" | "avi" | "mov" => "video",
        _ => "other",
    }
}

/// A PNG data URL of the image scaled to fit [`THUMBNAIL_SIDE`], or `None`
/// when the bytes are not a decodable image.
pub fn thumbnail_data_url(bytes: &[u8]) -> Option<String> {
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
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn kinds_follow_the_extension() {
        let kind = |s: &str| kind_of(&AssetRef(s.into()));
        assert_eq!(kind("assets/a.PNG"), "image");
        assert_eq!(kind("assets/f.otf"), "font");
        assert_eq!(kind("assets/v.mp4"), "video");
        assert_eq!(kind("assets/readme"), "other");
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
