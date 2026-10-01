//! The poster of a theme's video background: one picture of a video or an
//! animated GIF, decoded by ffmpeg into raw RGBA of the theme's canvas.
//!
//! The framing is the core's `PosterSpec`: seek to `at` (input seeking,
//! frame-accurate), crop to the canvas's shape like a conversion for a
//! screen, then scale to cover the canvas exactly (the last crop only trims
//! a rounding pixel, or keeps the middle when the video's size was unknown).

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use bezel_core::domain::frame::{Frame, RGBA_BYTES};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::{BezelError, Result};

use crate::process::{self, StderrTail, file_url};

/// Longest ffmpeg may take to hand over the picture.
const TIMEOUT: Duration = Duration::from_secs(30);

/// ffmpeg's arguments decoding the poster of `source` described by `spec`
/// into one raw RGBA picture on stdout.
pub(crate) fn arguments(source: &Path, spec: PosterSpec) -> Result<Vec<OsString>> {
    let (w, h) = (spec.size.width, spec.size.height);
    if spec.size.area() == 0 {
        return Err(BezelError::InvalidInput("the poster size is empty".into()));
    }
    let mut filters = String::new();
    if let Some(crop) = spec.crop {
        if crop.width == 0 || crop.height == 0 {
            return Err(BezelError::InvalidInput("the poster crop is empty".into()));
        }
        filters.push_str(&format!(
            "crop={}:{}:{}:{},",
            crop.width, crop.height, crop.x, crop.y
        ));
    }
    filters.push_str(&format!(
        "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1"
    ));
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-nostats", "-v", "error"]
        .iter()
        .map(OsString::from)
        .collect();
    if !spec.at.is_zero() {
        args.push("-ss".into());
        args.push(format!("{}.{:03}", spec.at.as_secs(), spec.at.subsec_millis()).into());
    }
    args.push("-i".into());
    args.push(file_url(source));
    for word in [
        "-an",
        "-sn",
        "-dn",
        "-frames:v",
        "1",
        "-vf",
        &filters,
        "-pix_fmt",
        "rgba",
        "-f",
        "rawvideo",
        "pipe:1",
    ] {
        args.push(word.into());
    }
    Ok(args)
}

/// Runs `ffmpeg` with `args` (from [`arguments`]) and reads the picture of
/// `spec`. A whole picture is the answer even when ffmpeg then complains;
/// otherwise its error is quoted.
pub(crate) fn take(ffmpeg: &Path, args: &[OsString], spec: PosterSpec) -> Result<Frame> {
    let wanted = spec.size.area().saturating_mul(RGBA_BYTES as u64);
    let mut child = process::spawn(ffmpeg, args).map_err(|e| {
        BezelError::Unsupported(format!("{} could not be started: {e}", ffmpeg.display()))
    })?;
    let stderr = StderrTail::collect(child.stderr.take());
    let stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut picture = Vec::new();
        if let Some(pipe) = stdout {
            // A short read is told apart below; the exit status says why.
            let _ = pipe.take(wanted).read_to_end(&mut picture);
        }
        let _ = tx.send(picture);
    });
    let picture = match rx.recv_timeout(TIMEOUT) {
        Ok(picture) => picture,
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            process::stop(&mut child);
            return Err(BezelError::Timeout(format!(
                "ffmpeg taking a poster ({} s without a picture)",
                TIMEOUT.as_secs()
            )));
        }
    };
    let status = child.wait();
    if let Some(frame) = Frame::from_rgba(spec.size, picture) {
        return Ok(frame);
    }
    let why = match status {
        Ok(status) if status.success() => {
            format!("the video has no picture {:.1} s in", spec.at.as_secs_f32())
        }
        Ok(_) => process::quote(&stderr.finish()),
        Err(e) => e.to_string(),
    };
    Err(BezelError::InvalidInput(format!(
        "ffmpeg could not take a poster from the video: {why}"
    )))
}

#[cfg(test)]
mod tests {
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::geometry::Size;

    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_str().unwrap().to_string())
            .collect()
    }

    fn spec(at: Duration, crop: Option<Rect>) -> PosterSpec {
        PosterSpec {
            size: Size::new(1920, 480),
            at,
            quarter_turns: 0,
            crop,
            pad: None,
        }
    }

    #[test]
    fn arguments_seek_crop_like_a_conversion_and_cover_the_canvas() {
        let crop = Some(Rect::new(0, 300, 1920, 480));
        let args = strings(
            &arguments(
                Path::new("/v/clip.mov"),
                spec(Duration::from_millis(1250), crop),
            )
            .unwrap(),
        );
        assert_eq!(
            args,
            [
                "-hide_banner",
                "-nostdin",
                "-nostats",
                "-v",
                "error",
                "-ss",
                "1.250",
                "-i",
                "file:/v/clip.mov",
                "-an",
                "-sn",
                "-dn",
                "-frames:v",
                "1",
                "-vf",
                "crop=1920:480:0:300,scale=1920:480:force_original_aspect_ratio=increase,crop=1920:480,setsar=1",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ]
        );
    }

    #[test]
    fn the_first_picture_needs_no_seek_and_bad_specs_are_refused() {
        let args = strings(&arguments(Path::new("-a.gif"), spec(Duration::ZERO, None)).unwrap());
        assert!(!args.contains(&"-ss".to_string()));
        assert_eq!(args[5..7], ["-i", "file:-a.gif"]);
        assert_eq!(
            args[13],
            "scale=1920:480:force_original_aspect_ratio=increase,crop=1920:480,setsar=1"
        );
        let empty = PosterSpec {
            size: Size::new(0, 480),
            ..spec(Duration::ZERO, None)
        };
        assert!(arguments(Path::new("a"), empty).is_err());
        let no_crop = spec(Duration::ZERO, Some(Rect::new(0, 0, 0, 4)));
        assert!(arguments(Path::new("a"), no_crop).is_err());
    }

    #[cfg(unix)]
    mod with_fake_ffmpeg {
        use super::*;
        use crate::fakes;

        /// The fake ffmpeg answers `rawvideo` with `AAAABBBB`: two pixels.
        fn two_pixels() -> PosterSpec {
            PosterSpec {
                size: Size::new(2, 1),
                at: Duration::ZERO,
                quarter_turns: 0,
                crop: None,
                pad: None,
            }
        }

        #[test]
        fn the_picture_is_read_from_ffmpegs_pipe() {
            let args = vec![OsString::from("rawvideo")];
            let frame = take(&fakes::dir().join("ready/ffmpeg"), &args, two_pixels()).unwrap();
            assert_eq!(frame.as_rgba(), b"AAAABBBB");
        }

        #[test]
        fn failures_quote_ffmpeg_or_say_what_is_missing() {
            let args = vec![OsString::from("rawvideo")];
            let err = take(&fakes::dir().join("failing/ffmpeg"), &args, two_pixels()).unwrap_err();
            assert!(err.to_string().contains("Invalid data found"), "{err}");
            let bigger = PosterSpec {
                size: Size::new(4, 4),
                at: Duration::from_secs(1),
                ..two_pixels()
            };
            let err = take(&fakes::dir().join("ready/ffmpeg"), &args, bigger).unwrap_err();
            assert!(err.to_string().contains("no picture 1.0 s in"), "{err}");
            let err = take(&fakes::dir().join("missing/ffmpeg"), &args, two_pixels());
            assert!(matches!(err, Err(BezelError::Unsupported(_))));
        }
    }
}
