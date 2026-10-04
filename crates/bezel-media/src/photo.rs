//! Photos for a screen's album (D-2026-10-03-power-off-standby-4): a JPEG,
//! PNG, BMP or GIF (its first picture) decoded on the host, natively and
//! without ffmpeg, with its EXIF orientation applied, so that a phone photo
//! arrives standing up; framed by the core's [`album_frame`] into the shape
//! the screen has as the user looks at it, turned to the panel's native
//! orientation; and encoded as a PNG of the panel's native size (480x1920 on
//! the 8.8"), the format measured in the album.
//!
//! Reading the photo is the only I/O here; deciding where the PNG goes (the
//! card's `sd/image`, through the core's upload) is the caller's.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::framing::VideoFit;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::standby::album_frame;
use bezel_core::{BezelError, Result};
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, RgbaImage};

use crate::probe::unreadable;

/// The formats a photo may come in.
const PHOTO_FORMATS: [ImageFormat; 4] = [
    ImageFormat::Jpeg,
    ImageFormat::Png,
    ImageFormat::Bmp,
    ImageFormat::Gif,
];

/// The photo in the file at `path`, as [`decode`] reads it.
pub fn open(path: &Path) -> Result<Frame> {
    let bytes = fs::read(path).map_err(|e| unreadable(path, &e))?;
    decode(&bytes).map_err(|e| match e {
        BezelError::InvalidInput(why) => {
            BezelError::InvalidInput(format!("{}: {why}", path.display()))
        }
        other => other,
    })
}

/// The photo `bytes` hold, as RGBA, upright: a JPEG, PNG, BMP or GIF (its
/// first picture), with the orientation its EXIF data gives applied
/// (turned and mirrored as the camera says). Anything else, or a file that
/// cannot be decoded, is `InvalidInput` saying so.
pub fn decode(bytes: &[u8]) -> Result<Frame> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| not_a_photo(&e))?;
    if !reader.format().is_some_and(|f| PHOTO_FORMATS.contains(&f)) {
        return Err(BezelError::InvalidInput(
            "not a photo Bezel reads (JPEG, PNG, BMP or GIF)".into(),
        ));
    }
    let mut decoder = reader.into_decoder().map_err(|e| not_a_photo(&e))?;
    let orientation = decoder.orientation().map_err(|e| not_a_photo(&e))?;
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|e| not_a_photo(&e))?;
    picture.apply_orientation(orientation);
    let rgba = picture.into_rgba8();
    let size = Size::new(rgba.width(), rgba.height());
    Frame::from_rgba(size, rgba.into_raw())
        .ok_or_else(|| BezelError::InvalidInput("the photo is incomplete".into()))
}

fn not_a_photo(e: &dyn std::fmt::Display) -> BezelError {
    BezelError::InvalidInput(format!("not a readable photo: {e}"))
}

/// The PNG of `picture` as the album of a `model` screen stores it:
/// [`album_frame`] (framed by `fit` into the screen's shape in
/// `orientation`, turned to the panel's native orientation, opaque over
/// black), the panel's native size, RGB (nothing in it is transparent).
pub fn album_png(
    picture: &Frame,
    model: &DeviceModel,
    orientation: Orientation,
    fit: VideoFit,
) -> Result<Vec<u8>> {
    let framed = rgba_image(&album_frame(picture, model, orientation, fit))?;
    png(&DynamicImage::ImageRgb8(
        DynamicImage::ImageRgba8(framed).to_rgb8(),
    ))
}

/// `frame` as a PNG (RGBA, 8 bits per channel), e.g. a preview.
pub fn encode_png(frame: &Frame) -> Result<Vec<u8>> {
    png(&DynamicImage::ImageRgba8(rgba_image(frame)?))
}

fn rgba_image(frame: &Frame) -> Result<RgbaImage> {
    let size = frame.size();
    RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())
        .ok_or_else(|| BezelError::InvalidInput("the picture is incomplete".into()))
}

fn png(picture: &DynamicImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    picture
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e: ImageError| {
            BezelError::InvalidInput(format!("the picture cannot be encoded: {e}"))
        })?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::{Rect, Rgba};
    use image::codecs::gif::GifEncoder;
    use image::codecs::jpeg::JpegEncoder;
    use image::{ExtendedColorType, ImageEncoder, RgbImage};

    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    fn eight_eight() -> &'static DeviceModel {
        model_by_id(ModelId("turing-8.8")).unwrap()
    }

    /// A `width` x `height` picture, its left half red and right half blue.
    fn left_red_right_blue(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, _| {
            image::Rgb(if x < width / 2 { RED } else { BLUE })
        })
    }

    fn jpeg(picture: &RgbImage) -> Vec<u8> {
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 95)
            .write_image(
                picture.as_raw(),
                picture.width(),
                picture.height(),
                ExtendedColorType::Rgb8,
            )
            .unwrap();
        out
    }

    /// `jpeg` with an APP1 Exif segment right after its SOI marker whose
    /// one IFD entry is the orientation tag (0x0112, SHORT) = `orientation`,
    /// big-endian TIFF as phones write it.
    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"MM\0\x2A");
        tiff.extend_from_slice(&8u32.to_be_bytes()); // IFD0 right after
        tiff.extend_from_slice(&1u16.to_be_bytes()); // one entry
        tiff.extend_from_slice(&0x0112u16.to_be_bytes());
        tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_be_bytes()); // one value
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0]); // the value's padding
        tiff.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let length = u16::try_from(payload.len() + 2).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "SOI");
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// Whether `pixel` is close to `color` (JPEG is lossy).
    fn near(pixel: Rgba, color: [u8; 3]) -> bool {
        let close = |a: u8, b: u8| a.abs_diff(b) < 48;
        close(pixel.r, color[0]) && close(pixel.g, color[1]) && close(pixel.b, color[2])
    }

    /// The PNG `bytes` decoded, then turned from the 8.8" panel's native
    /// orientation back to `orientation`: what the user sees.
    fn seen(bytes: &[u8], orientation: Orientation) -> Frame {
        assert_eq!(image::guess_format(bytes).unwrap(), ImageFormat::Png);
        let header = image::load_from_memory(bytes).unwrap();
        assert_eq!(header.color(), image::ColorType::Rgb8, "opaque RGB");
        let native = decode(bytes).unwrap();
        assert_eq!(native.size(), Size::new(480, 1920), "panel-native size");
        let model = eight_eight();
        let turns = orientation.quarter_turns_to(model.native_orientation);
        native.rotated((4 - turns) % 4)
    }

    #[test]
    fn a_phone_photo_with_exif_6_stands_up_on_a_vertical_screen() {
        // The sensor's picture is wide (left red, right blue); EXIF 6 says
        // turn it 90° clockwise to see it: tall, red on top, blue below.
        let stored = left_red_right_blue(200, 100);
        let photo = with_exif_orientation(&jpeg(&stored), 6);
        let upright = decode(&photo).unwrap();
        assert_eq!(upright.size(), Size::new(100, 200), "stands up");
        assert!(near(upright.pixel(50, 20).unwrap(), RED));
        assert!(near(upright.pixel(50, 180).unwrap(), BLUE));
        // Without the tag, the same picture stays wide.
        let wide = decode(&jpeg(&stored)).unwrap();
        assert_eq!(wide.size(), Size::new(200, 100));

        // In the album of an 8.8" standing up (both ways): Contain keeps
        // the whole photo, 480x960 in the middle, black above and below.
        for orientation in [Orientation::Portrait, Orientation::ReversePortrait] {
            let png = album_png(&upright, eight_eight(), orientation, VideoFit::Contain).unwrap();
            let view = seen(&png, orientation);
            assert_eq!(view.size(), Size::new(480, 1920), "{orientation:?}");
            assert_eq!(view.pixel(240, 100), Some(Rgba::BLACK));
            assert_eq!(view.pixel(240, 1820), Some(Rgba::BLACK));
            assert!(near(view.pixel(240, 600).unwrap(), RED), "{orientation:?}");
            assert!(
                near(view.pixel(240, 1300).unwrap(), BLUE),
                "{orientation:?}"
            );
        }
    }

    #[test]
    fn a_horizontal_photo_fills_a_horizontal_screen() {
        let photo = jpeg(&left_red_right_blue(400, 200));
        let picture = decode(&photo).unwrap();
        for orientation in [Orientation::Landscape, Orientation::ReverseLandscape] {
            // Cover: the whole width, top and bottom cut; left stays left.
            let png = album_png(&picture, eight_eight(), orientation, VideoFit::Cover).unwrap();
            let view = seen(&png, orientation);
            assert_eq!(view.size(), Size::new(1920, 480), "{orientation:?}");
            assert!(near(view.pixel(20, 240).unwrap(), RED), "{orientation:?}");
            assert!(
                near(view.pixel(1900, 240).unwrap(), BLUE),
                "{orientation:?}"
            );
            assert!(
                view.as_rgba()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[3] == 255),
                "opaque"
            );
        }
        // Contain: black on both sides of a 960x480 photo.
        let png = album_png(
            &picture,
            eight_eight(),
            Orientation::Landscape,
            VideoFit::Contain,
        )
        .unwrap();
        let view = seen(&png, Orientation::Landscape);
        assert_eq!(view.pixel(100, 240), Some(Rgba::BLACK));
        assert_eq!(view.pixel(1820, 240), Some(Rgba::BLACK));
        assert!(near(view.pixel(600, 240).unwrap(), RED));
        assert!(near(view.pixel(1300, 240).unwrap(), BLUE));
    }

    #[test]
    fn png_bmp_and_the_first_picture_of_a_gif_are_read() {
        let mut frame = Frame::filled(Size::new(3, 2), Rgba::opaque(0, 255, 0));
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(9, 8, 7));
        let png = encode_png(&frame).unwrap();
        assert_eq!(decode(&png).unwrap(), frame, "PNG round trip");

        let mut bmp = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(left_red_right_blue(4, 2))
            .write_to(&mut bmp, ImageFormat::Bmp)
            .unwrap();
        let read = decode(bmp.get_ref()).unwrap();
        assert_eq!(read.size(), Size::new(4, 2));
        assert_eq!(read.pixel(0, 0), Some(Rgba::opaque(255, 0, 0)));
        assert_eq!(read.pixel(3, 1), Some(Rgba::opaque(0, 0, 255)));

        // An animated GIF: red, then blue. The first picture is the photo.
        let mut gif = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut gif);
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                let picture = RgbaImage::from_pixel(4, 4, image::Rgba(color));
                encoder.encode_frame(image::Frame::new(picture)).unwrap();
            }
        }
        let first = decode(&gif).unwrap();
        assert_eq!(first.size(), Size::new(4, 4));
        assert!(near(first.pixel(2, 2).unwrap(), RED));
    }

    #[test]
    fn anything_else_is_invalid_input_naming_the_file() {
        for bytes in [&b""[..], b"not a photo at all", b"\xFF\xD8\xFF\xE0broken"] {
            let err = decode(bytes).unwrap_err();
            assert!(matches!(err, BezelError::InvalidInput(_)), "{err}");
        }
        let err = decode(b"RIFF\0\0\0\0WEBPVP8 ").unwrap_err();
        assert!(err.to_string().contains("JPEG, PNG, BMP or GIF"), "{err}");

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.jpg");
        let err = open(&missing).unwrap_err();
        assert!(err.to_string().contains("missing.jpg"), "{err}");
        let junk = dir.path().join("junk.png");
        fs::write(&junk, b"junk").unwrap();
        let err = open(&junk).unwrap_err();
        assert!(matches!(err, BezelError::InvalidInput(_)), "{err}");
        assert!(err.to_string().contains("junk.png"), "{err}");
        let photo = dir.path().join("photo.jpg");
        fs::write(&photo, jpeg(&left_red_right_blue(8, 4))).unwrap();
        assert_eq!(open(&photo).unwrap().size(), Size::new(8, 4));
    }
}
