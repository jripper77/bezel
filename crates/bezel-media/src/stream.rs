//! Decoding a video or an animated GIF on the host into looping RGBA frames
//! (the fallback for screens that cannot play stored videos, D-2026-09-30-
//! storage-video-4).
//!
//! ffmpeg writes raw RGBA frames of the requested size to a pipe at exactly
//! `fps` frames per media second (`fps` filter), the picture covering the
//! frame and cropped to fit. A helper thread reads whole frames into a small
//! bounded queue, so ffmpeg never runs far ahead of the caller. The caller
//! owns the clock: frame `n` is shown from `n / fps` seconds on; at the end
//! of the video ffmpeg is started again and the count wraps.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use bezel_core::domain::frame::{Frame, RGBA_BYTES};
use bezel_core::domain::media::StreamSpec;
use bezel_core::ports::VideoFrames;
use bezel_core::{BezelError, Result};

use crate::process::{self, StderrTail, file_url};

/// Frames decoded ahead of the caller.
const QUEUE: usize = 2;
/// Longest wait for ffmpeg's next frame.
const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

/// ffmpeg's arguments decoding `source` into raw RGBA frames of `spec`.
pub(crate) fn arguments(source: &Path, spec: StreamSpec) -> Vec<OsString> {
    let (w, h) = (spec.size.width, spec.size.height);
    let filters = format!(
        "fps={},scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1",
        spec.fps
    );
    let mut args: Vec<OsString> = ["-hide_banner", "-nostdin", "-nostats", "-v", "error", "-i"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(file_url(source));
    for word in [
        "-an", "-sn", "-dn", "-vf", &filters, "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
    ] {
        args.push(word.into());
    }
    args
}

/// A spec frames can be produced for.
pub(crate) fn check(spec: StreamSpec) -> Result<()> {
    if spec.fps == 0 || spec.size.area() == 0 {
        return Err(BezelError::InvalidInput(format!(
            "cannot decode into {}x{} at {} fps",
            spec.size.width, spec.size.height, spec.fps
        )));
    }
    Ok(())
}

/// One pass over a video, frame by frame.
pub(crate) trait Decoder: Send {
    /// Starts over from the first frame.
    fn restart(&mut self) -> Result<()>;
    /// The next frame's RGBA bytes; `None` at the end of the pass.
    fn next_frame(&mut self) -> Result<Option<Vec<u8>>>;
}

/// Frames of `spec` from a [`Decoder`], looping, picked by the caller's clock.
pub(crate) struct Looping<D> {
    decoder: D,
    spec: StreamSpec,
    /// The frame shown now and its index in the pass.
    current: Option<(u64, Frame)>,
    /// Frames in one pass, once a pass has ended.
    pass_len: Option<u64>,
}

impl<D: Decoder> Looping<D> {
    pub(crate) fn new(decoder: D, spec: StreamSpec) -> Self {
        Self {
            decoder,
            spec,
            current: None,
            pass_len: None,
        }
    }

    /// Index of the frame due `elapsed` after the start, over all passes.
    fn due(&self, elapsed: Duration) -> u64 {
        let index = elapsed.as_nanos() * u128::from(self.spec.fps) / 1_000_000_000;
        u64::try_from(index).unwrap_or(u64::MAX)
    }

    fn position(&self) -> Option<u64> {
        self.current.as_ref().map(|(index, _)| *index)
    }

    fn restart(&mut self) -> Result<()> {
        self.current = None;
        self.decoder.restart()
    }

    /// Reads one frame; `false` at the end of the pass.
    fn advance(&mut self) -> Result<bool> {
        let Some(bytes) = self.decoder.next_frame()? else {
            return Ok(false);
        };
        let frame = Frame::from_rgba(self.spec.size, bytes)
            .ok_or_else(|| BezelError::InvalidInput("a decoded frame has the wrong size".into()))?;
        let index = self.position().map_or(0, |i| i + 1);
        self.current = Some((index, frame));
        Ok(true)
    }

    /// Moves to the frame due `elapsed` after the start.
    fn seek(&mut self, elapsed: Duration) -> Result<()> {
        let due = self.due(elapsed);
        let mut wanted = self.pass_len.map_or(due, |len| due % len);
        if self.position().is_none_or(|at| at > wanted) {
            self.restart()?;
        }
        while self.position() != Some(wanted) {
            if self.advance()? {
                continue;
            }
            let len = self.position().map_or(0, |i| i + 1);
            if len == 0 {
                return Err(BezelError::InvalidInput("the video has no frames".into()));
            }
            self.pass_len = Some(len);
            wanted = due % len;
            if self.position() != Some(wanted) {
                self.restart()?;
            }
        }
        Ok(())
    }
}

impl<D: Decoder> VideoFrames for Looping<D> {
    fn frame_at(&mut self, elapsed: Duration) -> Result<&Frame> {
        self.seek(elapsed)?;
        self.current
            .as_ref()
            .map(|(_, frame)| frame)
            .ok_or_else(|| BezelError::InvalidInput("the video has no frames".into()))
    }
}

/// A [`Decoder`] around an ffmpeg process writing raw frames to a pipe.
pub(crate) struct FfmpegDecoder {
    ffmpeg: PathBuf,
    args: Vec<OsString>,
    frame_len: usize,
    running: Option<Running>,
}

struct Running {
    child: Child,
    frames: Receiver<Vec<u8>>,
    stderr: Option<StderrTail>,
    produced: u64,
}

impl FfmpegDecoder {
    pub(crate) fn new(ffmpeg: PathBuf, args: Vec<OsString>, spec: StreamSpec) -> Self {
        let frame_len =
            usize::try_from(spec.size.area()).unwrap_or(usize::MAX / RGBA_BYTES) * RGBA_BYTES;
        Self {
            ffmpeg,
            args,
            frame_len,
            running: None,
        }
    }

    fn start(&mut self) -> Result<()> {
        self.stop();
        let mut child = process::spawn(&self.ffmpeg, &self.args).map_err(|e| {
            BezelError::Unsupported(format!(
                "{} could not be started: {e}",
                self.ffmpeg.display()
            ))
        })?;
        let stderr = Some(StderrTail::collect(child.stderr.take()));
        let (tx, frames) = mpsc::sync_channel(QUEUE);
        let stdout = child.stdout.take();
        let frame_len = self.frame_len;
        thread::spawn(move || {
            let Some(mut stdout) = stdout else { return };
            loop {
                let mut frame = vec![0u8; frame_len];
                if stdout.read_exact(&mut frame).is_err() || tx.send(frame).is_err() {
                    break;
                }
            }
        });
        self.running = Some(Running {
            child,
            frames,
            stderr,
            produced: 0,
        });
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut running) = self.running.take() {
            drop(running.frames);
            process::stop(&mut running.child);
        }
    }
}

impl Decoder for FfmpegDecoder {
    fn restart(&mut self) -> Result<()> {
        self.start()
    }

    fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        if self.running.is_none() {
            self.start()?;
        }
        let Some(running) = self.running.as_mut() else {
            return Ok(None);
        };
        match running.frames.recv_timeout(FRAME_TIMEOUT) {
            Ok(frame) => {
                running.produced += 1;
                Ok(Some(frame))
            }
            Err(RecvTimeoutError::Timeout) => Err(BezelError::Timeout(format!(
                "ffmpeg decoding a video ({} s without a frame)",
                FRAME_TIMEOUT.as_secs()
            ))),
            Err(RecvTimeoutError::Disconnected) => ended(running),
        }
    }
}

/// The end of a pass: fine after at least one frame or a clean exit,
/// ffmpeg's error otherwise.
fn ended(running: &mut Running) -> Result<Option<Vec<u8>>> {
    let clean = running.child.wait().is_ok_and(|status| status.success());
    if clean || running.produced > 0 {
        return Ok(None);
    }
    let why = running
        .stderr
        .take()
        .map(StderrTail::finish)
        .unwrap_or_default();
    Err(BezelError::InvalidInput(format!(
        "ffmpeg could not decode the video: {}",
        process::quote(&why)
    )))
}

impl Drop for FfmpegDecoder {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use bezel_core::domain::geometry::Size;

    use super::*;

    fn spec(fps: u32) -> StreamSpec {
        StreamSpec {
            size: Size::new(1, 1),
            fps,
        }
    }

    /// A pass of one-pixel frames whose red channel is the frame number.
    struct Scripted {
        pass: Vec<Vec<u8>>,
        left: VecDeque<Vec<u8>>,
        restarts: usize,
        read: usize,
    }

    impl Scripted {
        fn new(frames: u8) -> Self {
            let pass: Vec<Vec<u8>> = (0..frames).map(|i| vec![i, 0, 0, 255]).collect();
            Self {
                left: VecDeque::new(),
                pass,
                restarts: 0,
                read: 0,
            }
        }
    }

    impl Decoder for Scripted {
        fn restart(&mut self) -> Result<()> {
            self.restarts += 1;
            self.left = self.pass.iter().cloned().collect();
            Ok(())
        }

        fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
            self.read += 1;
            Ok(self.left.pop_front())
        }
    }

    fn red(frames: &mut Looping<Scripted>, ms: u64) -> u8 {
        frames
            .frame_at(Duration::from_millis(ms))
            .unwrap()
            .as_rgba()[0]
    }

    #[test]
    fn frames_follow_the_callers_clock_and_loop() {
        let mut frames = Looping::new(Scripted::new(3), spec(10));
        assert_eq!(red(&mut frames, 0), 0);
        assert_eq!(red(&mut frames, 99), 0, "same frame within 1/fps");
        assert_eq!(red(&mut frames, 250), 2, "frame 1 is skipped");
        assert_eq!(frames.decoder.restarts, 1);
        // Past the end: the pass length is learnt and the count wraps.
        assert_eq!(red(&mut frames, 300), 0);
        assert_eq!(frames.pass_len, Some(3));
        assert_eq!(red(&mut frames, 410), 1);
        // A far jump reads at most one pass.
        let before = frames.decoder.read;
        assert_eq!(red(&mut frames, 3_600_000 + 200), 2);
        assert!(frames.decoder.read - before <= 3);
        // The clock going back starts over.
        assert_eq!(red(&mut frames, 0), 0);
    }

    #[test]
    fn the_last_frame_of_a_pass_can_be_due_when_the_pass_ends() {
        let mut frames = Looping::new(Scripted::new(3), spec(10));
        // Frame 5 = the last of the second pass = the frame in hand when the
        // first pass ends: no second start.
        assert_eq!(red(&mut frames, 500), 2);
        assert_eq!(frames.decoder.restarts, 1);
        assert_eq!(red(&mut frames, 600), 0);
        assert_eq!(frames.decoder.restarts, 2);
    }

    #[test]
    fn a_video_without_frames_is_an_error() {
        let mut frames = Looping::new(Scripted::new(0), spec(10));
        assert!(frames.frame_at(Duration::ZERO).is_err());
        let mut odd = Scripted::new(1);
        odd.pass = vec![vec![1, 2, 3]];
        let mut frames = Looping::new(odd, spec(10));
        assert!(
            frames.frame_at(Duration::ZERO).is_err(),
            "wrong frame length"
        );
    }

    #[test]
    fn arguments_cover_crop_to_the_spec() {
        let spec = StreamSpec {
            size: Size::new(1920, 480),
            fps: 24,
        };
        let args: Vec<String> = arguments(Path::new("/v/clip.gif"), spec)
            .iter()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
        assert_eq!(
            args,
            [
                "-hide_banner",
                "-nostdin",
                "-nostats",
                "-v",
                "error",
                "-i",
                "file:/v/clip.gif",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                "fps=24,scale=1920:480:force_original_aspect_ratio=increase,crop=1920:480,setsar=1",
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ]
        );
        assert!(check(spec).is_ok());
        assert!(check(StreamSpec { fps: 0, ..spec }).is_err());
        assert!(
            check(StreamSpec {
                size: Size::new(0, 4),
                fps: 1
            })
            .is_err()
        );
    }

    #[cfg(unix)]
    mod with_fake_ffmpeg {
        use super::*;
        use crate::fakes;

        fn decoder(script: &str) -> Looping<FfmpegDecoder> {
            let ffmpeg = fakes::dir().join(script);
            let spec = spec(10);
            Looping::new(
                FfmpegDecoder::new(ffmpeg, vec!["rawvideo".into()], spec),
                spec,
            )
        }

        #[test]
        fn frames_are_read_from_ffmpegs_pipe_and_loop() {
            let mut frames = decoder("ready/ffmpeg");
            let pixel = |f: &mut Looping<FfmpegDecoder>, ms| {
                f.frame_at(Duration::from_millis(ms))
                    .unwrap()
                    .as_rgba()
                    .to_vec()
            };
            assert_eq!(pixel(&mut frames, 0), b"AAAA");
            assert_eq!(pixel(&mut frames, 100), b"BBBB");
            assert_eq!(pixel(&mut frames, 200), b"AAAA", "two frames, then over");
            assert_eq!(pixel(&mut frames, 250), b"AAAA");
            assert_eq!(pixel(&mut frames, 300), b"BBBB");
            assert_eq!(frames.pass_len, Some(2));
        }

        #[test]
        fn a_decoding_failure_quotes_ffmpeg() {
            let mut frames = decoder("failing/ffmpeg");
            let err = frames.frame_at(Duration::ZERO).unwrap_err();
            assert!(err.to_string().contains("Invalid data found"), "{err}");
            let mut frames = decoder("missing/ffmpeg");
            assert!(matches!(
                frames.frame_at(Duration::ZERO),
                Err(BezelError::Unsupported(_))
            ));
        }
    }
}
