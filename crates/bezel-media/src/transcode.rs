//! Converting a video into a screen's profile with ffmpeg.
//!
//! The argument vector keeps the vendor's chain (docs/reverse-engineering/
//! video.md § 3 and § 4): rotation, crop, `scale=W:H,setsar=1:1`, libx264 at
//! CRF 20, no audio, yuv420p; `-x264opts bframes=0` and the optional `eq`
//! darkening for the TUR_USB Annex-B stream; `-r N` when asked. On top of
//! it: `file:` URLs (a name is never read as an option or a protocol),
//! `-sn -dn` (only the picture reaches the screen), and
//! `-progress pipe:1` for machine-readable progress.

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Child;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;

use bezel_core::domain::job::{Job, JobPhase, Progress};
use bezel_core::domain::media::{BFrames, MediaFormat, Tone, TranscodeTarget};
use bezel_core::{BezelError, Result};

use crate::process::{self, POLL, StderrTail, file_url};

/// Rotation prefixes of the vendor's adjust dialog, by clockwise quarter turns.
const ROTATIONS: [&str; 4] = [
    "",
    "transpose=1,",
    "transpose=1,transpose=1,",
    "transpose=2,",
];
/// The vendor's TUR_USB colour treatment.
const DARKENING: &str = ",eq=brightness=-0.1:contrast=0.9:saturation=1";
/// The vendor's constant quality.
const CRF: &str = "20";

/// ffmpeg's arguments converting `source` into `target`, written to `output`.
pub(crate) fn arguments(
    source: &Path,
    target: &TranscodeTarget,
    output: &Path,
) -> Result<Vec<OsString>> {
    let muxer = match target.format {
        MediaFormat::Mp4 => "mp4",
        MediaFormat::H264 => "h264",
        other => {
            return Err(BezelError::InvalidInput(format!(
                "videos cannot be converted into {other}"
            )));
        }
    };
    let mut args = words(&[
        "-hide_banner",
        "-nostdin",
        "-nostats",
        "-v",
        "error",
        "-progress",
        "pipe:1",
        "-y",
        "-i",
    ]);
    args.push(file_url(source));
    args.extend(words(&["-vf"]));
    args.push(filter_chain(target)?.into());
    args.extend(words(&["-c:v", "libx264"]));
    if target.b_frames == BFrames::Forbidden {
        args.extend(words(&["-x264opts", "bframes=0"]));
    }
    args.extend(words(&["-crf", CRF]));
    if let Some(rate) = target.frame_rate.filter(|r| *r > 0) {
        args.extend(words(&["-r"]));
        args.push(rate.to_string().into());
    }
    args.extend(words(&[
        "-an", "-sn", "-dn", "-pix_fmt", "yuv420p", "-f", muxer,
    ]));
    args.push(file_url(output));
    Ok(args)
}

fn words(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

/// `-vf`: rotate, crop (in rotated source pixels), scale to the exact panel
/// size with square pixels, then the optional darkening.
fn filter_chain(target: &TranscodeTarget) -> Result<String> {
    let size = target.size;
    if size.area() == 0 {
        return Err(BezelError::InvalidInput("the output size is empty".into()));
    }
    let mut chain = ROTATIONS[usize::from(target.quarter_turns % 4)].to_string();
    if let Some(crop) = target.crop {
        if crop.width == 0 || crop.height == 0 {
            return Err(BezelError::InvalidInput("the crop is empty".into()));
        }
        chain.push_str(&format!(
            "crop={}:{}:{}:{},",
            crop.width, crop.height, crop.x, crop.y
        ));
    }
    chain.push_str(&format!("scale={}:{},setsar=1:1", size.width, size.height));
    if target.tone == Tone::Darkened {
        chain.push_str(DARKENING);
    }
    Ok(chain)
}

/// Reads ffmpeg's `-progress` blocks (`key=value` lines, each block closed
/// by `progress=continue|end`) and yields the media time written, in ms.
#[derive(Debug, Default)]
pub(crate) struct ProgressParser {
    out_ms: Option<u64>,
}

impl ProgressParser {
    /// Feeds one line; returns the media time at the end of a block.
    pub(crate) fn line(&mut self, line: &str) -> Option<u64> {
        let (key, value) = line.trim().split_once('=')?;
        match key {
            // `out_time_ms` is in microseconds too (a historical ffmpeg quirk).
            "out_time_us" | "out_time_ms" => {
                if let Some(us) = value.trim().parse::<i64>().ok().filter(|us| *us >= 0) {
                    self.out_ms = Some(us.unsigned_abs() / 1000);
                }
                None
            }
            "out_time" => {
                if self.out_ms.is_none() {
                    self.out_ms = clock_ms(value.trim());
                }
                None
            }
            "progress" => self.out_ms.take(),
            _ => None,
        }
    }
}

/// `HH:MM:SS.micro` → milliseconds.
fn clock_ms(text: &str) -> Option<u64> {
    let mut parts = text.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some((hours * 3600 + minutes * 60) * 1000 + (seconds * 1000.0) as u64)
}

/// Runs ffmpeg with `args` (which write `output`), reporting
/// [`JobPhase::Convert`] progress in ms of `total_ms`. Cancelling kills
/// ffmpeg, deletes `output` and returns `Cancelled { partial: None }`; a
/// failure deletes `output` and quotes ffmpeg's error.
pub(crate) fn run(
    ffmpeg: &Path,
    args: &[OsString],
    output: &Path,
    total_ms: u64,
    job: &mut Job<'_>,
) -> Result<()> {
    job.checkpoint()?;
    let mut child = process::spawn(ffmpeg, args).map_err(|e| {
        BezelError::Unsupported(format!("{} could not be started: {e}", ffmpeg.display()))
    })?;
    let stderr = StderrTail::collect(child.stderr.take());
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take();
    thread::spawn(move || {
        if let Some(stdout) = stdout {
            forward_progress(stdout, &tx);
        }
    });
    job.report(Progress::new(JobPhase::Convert, 0, total_ms));
    loop {
        match rx.recv_timeout(POLL) {
            Ok(done) => job.report(Progress::new(JobPhase::Convert, done, total_ms)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if job.is_cancelled() {
            return cancelled(&mut child, output);
        }
    }
    let status = child.wait();
    if job.is_cancelled() {
        return cancelled(&mut child, output);
    }
    let failure = match status {
        Ok(status) if status.success() && output.is_file() => None,
        Ok(status) if status.success() => Some("it wrote no output".to_string()),
        Ok(_) => Some(process::quote(&stderr.finish())),
        Err(e) => Some(e.to_string()),
    };
    if let Some(why) = failure {
        discard(output);
        return Err(BezelError::InvalidInput(format!(
            "ffmpeg could not convert the video: {why}"
        )));
    }
    if total_ms > 0 {
        job.report(Progress::new(JobPhase::Convert, total_ms, total_ms));
    }
    Ok(())
}

fn forward_progress<R: Read>(stdout: R, tx: &mpsc::Sender<u64>) {
    let mut parser = ProgressParser::default();
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if let Some(done) = parser.line(&line)
            && tx.send(done).is_err()
        {
            break;
        }
    }
}

fn cancelled(child: &mut Child, output: &Path) -> Result<()> {
    process::stop(child);
    discard(output);
    Err(BezelError::Cancelled { partial: None })
}

/// Deletes a conversion output (always a file in the transcoder's own
/// folder, never a user file). A missing file is fine.
pub(crate) fn discard(output: &Path) {
    if let Err(e) = fs::remove_file(output)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("could not delete {}: {e}", output.display());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::media::{ConvertOptions, UploadProfile};

    use super::*;

    fn profile(id: &'static str) -> UploadProfile {
        UploadProfile::for_model(model_by_id(ModelId(id)).unwrap()).unwrap()
    }

    /// What the core asks for a 8.8" rev C upload with default options.
    pub(crate) fn rev_c_target() -> TranscodeTarget {
        profile("turing-8.8").transcode_target(ConvertOptions::default())
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn builds_the_vendor_argument_vector_for_rev_c() {
        let args = arguments(
            Path::new("/videos/in.mov"),
            &rev_c_target(),
            Path::new("/tmp/out.mp4"),
        )
        .unwrap();
        assert_eq!(
            strings(&args),
            [
                "-hide_banner",
                "-nostdin",
                "-nostats",
                "-v",
                "error",
                "-progress",
                "pipe:1",
                "-y",
                "-i",
                "file:/videos/in.mov",
                "-vf",
                "scale=480:1920,setsar=1:1",
                "-c:v",
                "libx264",
                "-crf",
                "20",
                "-an",
                "-sn",
                "-dn",
                "-pix_fmt",
                "yuv420p",
                "-f",
                "mp4",
                "file:/tmp/out.mp4",
            ]
        );
        // The vendor's rotated, cropped 24 fps variant.
        let options = ConvertOptions {
            quarter_turns: 1,
            crop: Some(Rect::new(300, 0, 480, 1920)),
            frame_rate: Some(24),
            ..ConvertOptions::default()
        };
        let target = profile("turing-8.8").transcode_target(options);
        let args = strings(&arguments(Path::new("a.mp4"), &target, Path::new("b.mp4")).unwrap());
        assert_eq!(
            args[11],
            "transpose=1,crop=480:1920:300:0,scale=480:1920,setsar=1:1"
        );
        assert_eq!(&args[12..18], ["-c:v", "libx264", "-crf", "20", "-r", "24"]);
    }

    #[test]
    fn tur_usb_gets_an_annex_b_stream_without_b_frames() {
        let options = ConvertOptions {
            quarter_turns: 3,
            tone: Tone::Darkened,
            ..ConvertOptions::default()
        };
        let target = profile("turing-usb-8.8").transcode_target(options);
        let args = strings(&arguments(Path::new("a.mp4"), &target, Path::new("b.h264")).unwrap());
        assert_eq!(
            &args[10..],
            [
                "-vf",
                "transpose=2,scale=480:1920,setsar=1:1,eq=brightness=-0.1:contrast=0.9:saturation=1",
                "-c:v",
                "libx264",
                "-x264opts",
                "bframes=0",
                "-crf",
                "20",
                "-an",
                "-sn",
                "-dn",
                "-pix_fmt",
                "yuv420p",
                "-f",
                "h264",
                "file:b.h264",
            ]
        );
        let mut half = target;
        half.quarter_turns = 2;
        half.tone = Tone::Natural;
        let args = strings(&arguments(Path::new("a"), &half, Path::new("b")).unwrap());
        assert_eq!(
            args[11],
            "transpose=1,transpose=1,scale=480:1920,setsar=1:1"
        );
    }

    #[test]
    fn impossible_targets_are_refused_before_ffmpeg_runs() {
        let mut target = rev_c_target();
        target.format = MediaFormat::Png;
        assert!(arguments(Path::new("a"), &target, Path::new("b")).is_err());
        let mut target = rev_c_target();
        target.crop = Some(Rect::new(0, 0, 0, 10));
        assert!(arguments(Path::new("a"), &target, Path::new("b")).is_err());
        let mut target = rev_c_target();
        target.size = Size::new(0, 1920);
        assert!(arguments(Path::new("a"), &target, Path::new("b")).is_err());
        let mut target = rev_c_target();
        target.frame_rate = Some(0);
        assert!(
            !strings(&arguments(Path::new("a"), &target, Path::new("b")).unwrap())
                .contains(&"-r".to_string())
        );
    }

    #[test]
    fn progress_blocks_give_the_media_time_written() {
        let mut parser = ProgressParser::default();
        let block = "frame=6\nfps=0.00\nout_time_us=250000\nout_time_ms=250000\nout_time=00:00:00.250000\nprogress=continue";
        let seen: Vec<u64> = block.lines().filter_map(|l| parser.line(l)).collect();
        assert_eq!(seen, [250]);
        // Before the first frame ffmpeg writes N/A or a negative time.
        assert_eq!(parser.line("out_time_us=N/A"), None);
        assert_eq!(parser.line("out_time_us=-9223372036854775807"), None);
        assert_eq!(parser.line("progress=continue"), None);
        assert_eq!(parser.line("out_time=01:02:03.500000"), None);
        assert_eq!(parser.line("progress=end"), Some(3_723_500));
        assert_eq!(parser.line("garbage"), None);
        assert_eq!(clock_ms("00:00:-1"), None);
        assert_eq!(clock_ms("12"), None);
    }

    #[test]
    fn discarding_a_missing_output_is_fine() {
        discard(Path::new("/nonexistent/bezel/output.mp4"));
        let dir = tempfile::tempdir().unwrap();
        // A folder cannot be removed as a file: logged, not fatal.
        discard(dir.path());
        assert!(dir.path().is_dir());
    }

    #[cfg(unix)]
    mod with_fake_ffmpeg {
        use std::path::PathBuf;
        use std::time::{Duration, Instant};

        use bezel_core::domain::job::CancelToken;

        use super::*;
        use crate::fakes;

        fn output() -> (tempfile::TempDir, PathBuf) {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.mp4");
            (dir, out)
        }

        fn args_for(out: &Path) -> Vec<OsString> {
            arguments(Path::new("/in.mp4"), &rev_c_target(), out).unwrap()
        }

        #[test]
        fn a_conversion_reports_progress_and_leaves_the_output() {
            let (_dir, out) = output();
            let token = CancelToken::new();
            let mut seen = Vec::new();
            let mut sink = |p: Progress| seen.push(p);
            let mut job = Job::new(&token, &mut sink);
            run(
                &fakes::dir().join("ready/ffmpeg"),
                &args_for(&out),
                &out,
                250,
                &mut job,
            )
            .unwrap();
            assert_eq!(fs::read(&out).unwrap(), b"converted");
            let done: Vec<u64> = seen.iter().map(|p| p.done).collect();
            assert_eq!(done, [0, 100, 250, 250]);
            assert!(
                seen.iter()
                    .all(|p| p.phase == JobPhase::Convert && p.total == 250)
            );
        }

        #[test]
        fn cancel_kills_ffmpeg_and_deletes_the_partial_output() {
            let (_dir, out) = output();
            let token = CancelToken::new();
            let remote = token.clone();
            let mut sink = |p: Progress| {
                if p.done > 0 {
                    remote.cancel();
                }
            };
            let mut job = Job::new(&token, &mut sink);
            let started = Instant::now();
            let result = run(
                &fakes::dir().join("slow/ffmpeg"),
                &args_for(&out),
                &out,
                0,
                &mut job,
            );
            assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "ffmpeg was not killed"
            );
            assert!(!out.exists(), "the partial output must be deleted");
            // A job cancelled before the start never spawns ffmpeg.
            let mut job = Job::new(&token, &mut sink);
            let result = run(Path::new("/nonexistent"), &[], &out, 0, &mut job);
            assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        }

        #[test]
        fn a_failed_conversion_quotes_ffmpeg_and_deletes_the_output() {
            let (_dir, out) = output();
            let token = CancelToken::new();
            let mut sink = |_| {};
            let mut job = Job::new(&token, &mut sink);
            let err = run(
                &fakes::dir().join("failing/ffmpeg"),
                &args_for(&out),
                &out,
                0,
                &mut job,
            )
            .unwrap_err();
            assert!(err.to_string().contains("Invalid data found"), "{err}");
            assert!(!out.exists());
            // Success without an output file is a failure too.
            let err = run(
                &fakes::dir().join("silent/ffmpeg"),
                &args_for(&out),
                &out,
                0,
                &mut job,
            )
            .unwrap_err();
            assert!(err.to_string().contains("wrote no output"), "{err}");
            let err = run(Path::new("/nonexistent/ffmpeg"), &[], &out, 0, &mut job).unwrap_err();
            assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        }
    }
}
