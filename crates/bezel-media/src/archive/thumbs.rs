//! Thumbnails of the local copies (D-2026-09-30-storage-manager-10), as PNG
//! fitting [`THUMBNAIL_EDGE`] pixels and never scaled up: an image decoded
//! natively and scaled down; a video's picture at 1 s (the core's
//! `PosterSpec`, the clip's first picture when it is shorter than 2 s) taken
//! by the transcoder's ffmpeg. Without ffmpeg a video has none.

use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::Path;

use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Size;
use bezel_core::domain::poster::PosterSpec;
use bezel_core::ports::{MediaLocation, MediaTranscoder};
use bezel_core::{BezelError, Result};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, RgbaImage};

use super::disk::{failed, format_of};

/// Longest side of a thumbnail, in pixels.
pub const THUMBNAIL_EDGE: u32 = 160;

/// Bytes read to tell an image from a video.
const HEAD_BYTES: u64 = 16;

/// `size` scaled down, keeping its shape, so that its longest side is at
/// most `edge`; a size that fits is kept. No side becomes 0.
pub(super) fn fit(size: Size, edge: u32) -> Size {
    let longest = size.width.max(size.height);
    if longest <= edge {
        return size;
    }
    let scale = |side: u32| {
        let (side, edge, longest) = (u64::from(side), u64::from(edge), u64::from(longest));
        let scaled = (side * edge + longest / 2) / longest;
        u32::try_from(scaled.max(1)).unwrap_or(u32::MAX)
    };
    Size::new(scale(size.width), scale(size.height))
}

/// The thumbnail of the copy in `copy`; `None` when it is a video and
/// `media` cannot read it or take its picture (no ffmpeg).
pub(super) fn make(copy: &Path, media: &mut dyn MediaTranscoder) -> Result<Option<Vec<u8>>> {
    if format_of(&head(copy)?).is_still() {
        let bytes = fs::read(copy).map_err(|e| failed("cannot read", copy, &e))?;
        return of_image(copy, &bytes).map(Some);
    }
    of_video(copy, media)
}

fn head(copy: &Path) -> Result<Vec<u8>> {
    let mut head = Vec::new();
    File::open(copy)
        .and_then(|file| file.take(HEAD_BYTES).read_to_end(&mut head))
        .map_err(|e| failed("cannot read", copy, &e))?;
    Ok(head)
}

fn of_image(copy: &Path, bytes: &[u8]) -> Result<Vec<u8>> {
    let picture = image::load_from_memory(bytes).map_err(|e| {
        BezelError::InvalidInput(format!("{} is not a readable image: {e}", copy.display()))
    })?;
    let size = Size::new(picture.width(), picture.height());
    let fitted = fit(size, THUMBNAIL_EDGE);
    if fitted == size {
        return png(&picture);
    }
    png(&picture.resize_exact(fitted.width, fitted.height, FilterType::Triangle))
}

fn of_video(copy: &Path, media: &mut dyn MediaTranscoder) -> Result<Option<Vec<u8>>> {
    let Some(text) = copy.to_str() else {
        return Err(BezelError::InvalidInput(format!(
            "{} is not a UTF-8 path",
            copy.display()
        )));
    };
    let location = MediaLocation(text.to_string());
    let info = match media.probe(&location) {
        Ok(info) => info,
        Err(BezelError::Unsupported(why)) => return Ok(none(copy, &why)),
        Err(e) => return Err(e),
    };
    let Some(size) = info.dimensions else {
        return Ok(none(copy, "its picture size is unknown"));
    };
    let spec = PosterSpec::for_canvas(fit(size, THUMBNAIL_EDGE), &info);
    match media.poster(&location, spec) {
        Ok(frame) => png_of(&frame).map(Some),
        Err(BezelError::Unsupported(why)) => Ok(none(copy, &why)),
        Err(e) => Err(e),
    }
}

fn none(copy: &Path, why: &str) -> Option<Vec<u8>> {
    tracing::debug!("no thumbnail of {}: {why}", copy.display());
    None
}

fn png_of(frame: &Frame) -> Result<Vec<u8>> {
    let size = frame.size();
    let pixels = RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())
        .ok_or_else(|| BezelError::InvalidInput("the video's picture is incomplete".into()))?;
    png(&DynamicImage::ImageRgba8(pixels))
}

fn png(picture: &DynamicImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    picture
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| BezelError::InvalidInput(format!("the thumbnail cannot be encoded: {e}")))?;
    Ok(out.into_inner())
}
