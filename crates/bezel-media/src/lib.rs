//! Bezel media: the driven adapter behind the core's
//! [`MediaTranscoder`](bezel_core::ports::MediaTranscoder) port.
//!
//! [`FfmpegTranscoder`] inspects local media files, converts videos into a
//! screen's profile and decodes videos into frames on the host, around an
//! external ffmpeg/ffprobe that is looked up, never bundled
//! (D-2026-09-30-storage-video-2):
//! - lookup: the path from the settings (the program or its folder), then
//!   `PATH`; ffprobe next to that ffmpeg, then `PATH`. The build must have
//!   libx264 (the vendor's encoder);
//! - without it the adapter degrades, never panics: `tools()` reports
//!   `Missing` with install commands for the host (dnf/apt/winget), MP4
//!   headers and still images are still read natively, and conversions or
//!   host decoding are `Unsupported`;
//! - processes get an argument vector (no shell) with `file:` URLs;
//!   conversions report progress from `-progress pipe:1`, and cancelling
//!   kills ffmpeg and deletes the partial output;
//! - conversion outputs live in a private temporary folder, removed with the
//!   transcoder; only the latest output is kept;
//! - a theme video's framing becomes one ffmpeg filter chain, built purely
//!   from the core's geometry in the vendor's order (turns, crop, scale,
//!   pad, square pixels), for the conversion and for the poster; the plain
//!   framing gives today's chains (D-2026-10-01-video-background-framing-3);
//! - a theme's poster is one picture of a video or an animated GIF, framed
//!   by the core's `PosterSpec`; animated GIFs are read natively as moving
//!   pictures, so a GIF can be a video background and is converted for a
//!   screen like a video;
//! - decoding on the host hands over the raw source picture (never turned or
//!   cropped, the caller frames it) at most 15 times a second, on two
//!   decoder threads (D-2026-10-01-video-background-framing-5).
//!
//! The adapter decides nothing: whether a file is in a screen's profile is
//! the core's `UploadProfile::mismatches`.
//!
//! [`archive`] holds the local copies of what Bezel sends to screens, behind
//! the core's [`ArchiveStore`](bezel_core::ports::ArchiveStore) port: on
//! disk in `<data>/bezel/storage/` ([`archive::DiskArchive`]: the catalog,
//! one file per content, thumbnails made on demand), or in memory for tests.
//!
//! [`collection`] holds the user's collection of GIFs and stickers, behind
//! the core's [`GifCollection`](bezel_core::ports::GifCollection) port, and
//! the fake GIF provider the tests search through.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod archive;
pub mod collection;
mod framing;
mod gif;
mod mp4;
mod poster;
mod probe;
mod process;
mod stream;
mod transcode;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bezel_core::domain::frame::Frame;
use bezel_core::domain::job::Job;
use bezel_core::domain::media::{MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::ports::{MediaLocation, MediaTranscoder, VideoFrames};
use bezel_core::{BezelError, Result};
use tempfile::TempDir;

use crate::probe::{HostSystem, Lookup, Tools};
use crate::stream::{FfmpegDecoder, Looping};

/// The [`MediaTranscoder`] around an external ffmpeg and ffprobe.
pub struct FfmpegTranscoder {
    lookup: Lookup,
    hints: Vec<String>,
    found: Option<Tools>,
    work: Option<TempDir>,
    last_output: Option<PathBuf>,
    outputs: u64,
}

impl std::fmt::Debug for FfmpegTranscoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FfmpegTranscoder")
            .field("configured", &self.lookup.configured)
            .field("found", &self.found.as_ref().map(|t| &t.ffmpeg))
            .finish_non_exhaustive()
    }
}

impl FfmpegTranscoder {
    /// A transcoder that looks for ffmpeg at `ffmpeg` first (the program or
    /// the folder holding it, from the CLI flag or the studio settings;
    /// `None` when unset), then on this process's `PATH`. Nothing runs
    /// until a method needs the tools.
    pub fn new(ffmpeg: Option<PathBuf>) -> Self {
        Self::with_lookup(Lookup::from_env(ffmpeg), HostSystem::detect())
    }

    pub(crate) fn with_lookup(lookup: Lookup, system: HostSystem) -> Self {
        Self {
            lookup,
            hints: probe::install_hints(system),
            found: None,
            work: None,
            last_output: None,
            outputs: 0,
        }
    }

    /// Changes the configured ffmpeg location (the settings' Locate button);
    /// the next call looks the tools up again.
    pub fn set_ffmpeg_path(&mut self, ffmpeg: Option<PathBuf>) {
        self.lookup.configured = ffmpeg;
        self.found = None;
    }

    /// The configured ffmpeg location, as given.
    pub fn ffmpeg_path(&self) -> Option<&Path> {
        self.lookup.configured.as_deref()
    }

    /// The ffmpeg that is used (configured or found on `PATH`), or `None`
    /// when no usable one exists.
    pub fn ffmpeg_in_use(&mut self) -> Option<PathBuf> {
        self.resolve().ok().map(|tools| tools.ffmpeg)
    }

    /// The tools, found once and kept while their files exist.
    fn resolve(&mut self) -> std::result::Result<Tools, probe::Absence> {
        if let Some(tools) = &self.found
            && tools.ffmpeg.is_file()
            && tools.ffprobe.is_file()
        {
            return Ok(tools.clone());
        }
        self.found = None;
        let tools = probe::locate(&self.lookup)?;
        self.found = Some(tools.clone());
        Ok(tools)
    }

    /// The tools for `task`, or `Unsupported` naming why and how to install.
    fn require(&mut self, task: &str) -> Result<Tools> {
        self.resolve().map_err(|why| {
            BezelError::Unsupported(format!(
                "{task} needs ffmpeg with libx264 ({why}); install it with: {}",
                self.hints.join(" ; ")
            ))
        })
    }

    /// A fresh output path in the private folder. The previous output is
    /// deleted unless it is `source`.
    fn next_output(&mut self, format: MediaFormat, source: &Path) -> Result<PathBuf> {
        if let Some(previous) = self.last_output.take()
            && previous != source
        {
            transcode::discard(&previous);
        }
        let work = match self.work.take() {
            Some(work) => work,
            None => tempfile::Builder::new()
                .prefix("bezel-media-")
                .tempdir()
                .map_err(|e| {
                    BezelError::Transport(format!(
                        "cannot create a folder for converted videos: {e}"
                    ))
                })?,
        };
        self.outputs += 1;
        let extension = format.extensions().first().copied().unwrap_or("bin");
        let path = work
            .path()
            .join(format!("converted-{}.{extension}", self.outputs));
        self.work = Some(work);
        self.last_output = Some(path.clone());
        Ok(path)
    }

    /// The source's duration, when it can be probed: for progress and for
    /// the bitrate that keeps the output under the screen's limit.
    fn duration(source: &Path, tools: &Tools) -> Option<Duration> {
        probe::probe_file(source, || Some(tools.ffprobe.clone()))
            .ok()
            .and_then(|info| info.video)
            .and_then(|track| track.duration)
    }
}

fn existing_file(source: &MediaLocation) -> Result<&Path> {
    let path = Path::new(&source.0);
    if !path.is_file() {
        return Err(BezelError::InvalidInput(format!(
            "{} is not a file",
            source.0
        )));
    }
    Ok(path)
}

impl MediaTranscoder for FfmpegTranscoder {
    fn tools(&mut self) -> MediaTools {
        match self.resolve() {
            Ok(tools) => MediaTools::Ready {
                version: tools.version,
            },
            Err(why) => {
                tracing::info!("media converter unavailable: {why}");
                MediaTools::Missing {
                    install_hints: self.hints.clone(),
                }
            }
        }
    }

    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
        probe::probe_file(Path::new(&source.0), || {
            self.resolve().ok().map(|tools| tools.ffprobe)
        })
    }

    fn transcode(
        &mut self,
        source: &MediaLocation,
        target: &TranscodeTarget,
        job: &mut Job<'_>,
    ) -> Result<MediaLocation> {
        job.checkpoint()?;
        let tools = self.require("Converting a video")?;
        let source = existing_file(source)?;
        let duration = Self::duration(source, &tools);
        let total_ms = duration.map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let output = self.next_output(target.format, source)?;
        let location = output.to_str().map(str::to_string).ok_or_else(|| {
            BezelError::Transport(format!("{} is not a UTF-8 path", output.display()))
        })?;
        let args = transcode::arguments(source, target, &output, duration)?;
        transcode::run(&tools.ffmpeg, &args, &output, total_ms, job)?;
        Ok(MediaLocation(location))
    }

    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        let path = Path::new(&source.0);
        fs::read(path).map_err(|e| probe::unreadable(path, &e))
    }

    fn stream(&mut self, source: &MediaLocation, spec: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        let tools = self.require("Playing a video on the computer")?;
        let spec = stream::playable(spec)?;
        let source = existing_file(source)?;
        let decoder = FfmpegDecoder::new(tools.ffmpeg, stream::arguments(source, spec)?, spec);
        Ok(Box::new(Looping::new(decoder, spec)))
    }

    fn poster(&mut self, source: &MediaLocation, spec: PosterSpec) -> Result<Frame> {
        let tools = self.require("Taking a poster from a video")?;
        let source = existing_file(source)?;
        poster::take(&tools.ffmpeg, &poster::arguments(source, spec)?, spec)
    }
}

/// Fake ffmpeg/ffprobe programs (shell scripts) for the process tests.
/// Every script is written once, before any test spawns one, so no child
/// ever inherits a script still open for writing (`ETXTBSY`).
#[cfg(test)]
#[cfg(unix)]
pub(crate) mod fakes {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    const FFMPEG: &str = r#"#!/bin/sh
for last; do :; done
case "$*" in
  *-version*) echo "ffmpeg version 9.9-fake Copyright (c) 2000-2026 the FFmpeg developers" ;;
  *-encoders*) printf ' V....D libx264rgb   libx264 RGB\n V....D libx264   libx264 H.264\n' ;;
  *rawvideo*) printf 'AAAABBBB' ;;
  *-progress*)
    printf 'frame=1\nout_time_us=100000\nout_time=00:00:00.100000\nprogress=continue\n'
    printf 'frame=2\nout_time_us=250000\nprogress=end\n'
    printf 'converted' > "${last#file:}" ;;
esac
"#;
    const FFPROBE: &str = r#"#!/bin/sh
case "$*" in
  *-version*) echo "ffprobe version 9.9-fake Copyright (c) 2007-2026 the FFmpeg developers" ;;
  *) echo '{"streams":[{"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p","width":480,"height":1920,"has_b_frames":0,"avg_frame_rate":"24/1"}],"format":{"format_name":"h264"}}' ;;
esac
"#;
    const NO_X264: &str = r#"#!/bin/sh
case "$*" in
  *-version*) echo "ffmpeg version 7.1-free" ;;
  *-encoders*) echo " V....D libopenh264   OpenH264" ;;
esac
"#;
    const IMPOSTOR: &str = "#!/bin/sh\necho hello\n";
    const SLOW: &str = r#"#!/bin/sh
for last; do :; done
printf 'partial' > "${last#file:}"
printf 'out_time_us=50000\nprogress=continue\n'
exec sleep 30
"#;
    const FAILING: &str = r#"#!/bin/sh
for last; do :; done
case "$last" in file:*) printf 'partial' > "${last#file:}" ;; esac
echo "file:/in.mp4: Invalid data found when processing input" >&2
echo "moov atom not found" >&2
exit 1
"#;
    const SILENT: &str = "#!/bin/sh\nexit 0\n";
    const SLEEPER: &str = "#!/bin/sh\nexec sleep 30\n";
    const NOISY: &str = "#!/bin/sh\nhead -c 20000 /dev/zero | tr '\\0' 'e' >&2\nexit 1\n";

    const SCRIPTS: [(&str, &str); 14] = [
        ("ready/ffmpeg", FFMPEG),
        ("ready/ffprobe", FFPROBE),
        ("noprobe/ffmpeg", FFMPEG),
        ("probe-only/ffprobe", FFPROBE),
        ("nox264/ffmpeg", NO_X264),
        ("nox264/ffprobe", FFPROBE),
        ("impostor/ffmpeg", IMPOSTOR),
        ("impostor/ffprobe", IMPOSTOR),
        ("slow/ffmpeg", SLOW),
        ("failing/ffmpeg", FAILING),
        ("failing/ffprobe", FAILING),
        ("silent/ffmpeg", SILENT),
        ("sleeper", SLEEPER),
        ("noisy", NOISY),
    ];

    /// The folder holding the fake programs.
    pub(crate) fn dir() -> &'static Path {
        static DIR: OnceLock<PathBuf> = OnceLock::new();
        DIR.get_or_init(|| {
            let root =
                std::env::temp_dir().join(format!("bezel-media-fakes-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("empty")).unwrap();
            for (name, body) in SCRIPTS {
                let path = root.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, body).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            root
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::job::{CancelToken, JobPhase, Progress};

    use super::*;

    #[test]
    fn configured_paths_are_kept_and_changed() {
        let mut media = FfmpegTranscoder::new(Some(PathBuf::from("/opt/ffmpeg/bin")));
        assert_eq!(media.ffmpeg_path(), Some(Path::new("/opt/ffmpeg/bin")));
        media.set_ffmpeg_path(None);
        assert_eq!(media.ffmpeg_path(), None);
        assert!(format!("{media:?}").contains("FfmpegTranscoder"));
        assert!(existing_file(&MediaLocation("/nonexistent/x.mp4".into())).is_err());
    }

    #[cfg(unix)]
    mod with_fake_tools {
        use std::ffi::OsString;

        use super::*;
        use crate::mp4::fixtures::Movie;

        fn transcoder() -> FfmpegTranscoder {
            let root = fakes::dir();
            let lookup = Lookup {
                configured: None,
                search_path: Some(OsString::from(root.join("ready"))),
            };
            FfmpegTranscoder::with_lookup(lookup, HostSystem::Debian)
        }

        fn source(dir: &Path) -> MediaLocation {
            let path = dir.join("in.mp4");
            fs::write(&path, Movie::in_rev_c_profile().bytes()).unwrap();
            MediaLocation(path.to_str().unwrap().to_string())
        }

        #[test]
        fn tools_found_on_path_are_ready_and_kept() {
            let mut media = transcoder();
            assert_eq!(
                media.tools(),
                MediaTools::Ready {
                    version: "9.9-fake".into()
                }
            );
            assert_eq!(
                media.ffmpeg_in_use(),
                Some(fakes::dir().join("ready/ffmpeg"))
            );
            media.lookup.search_path = Some(OsString::new());
            assert!(
                media.tools().converter() == bezel_core::domain::media::Converter::Available,
                "cached"
            );
            media.set_ffmpeg_path(Some(PathBuf::from("/nonexistent")));
            assert!(matches!(media.tools(), MediaTools::Missing { .. }));
            assert_eq!(media.ffmpeg_in_use(), None);
        }

        #[test]
        fn conversions_land_in_a_private_folder_that_keeps_the_latest() {
            let mut media = transcoder();
            let dir = tempfile::tempdir().unwrap();
            let source = source(dir.path());
            let token = CancelToken::new();
            let mut seen = Vec::new();
            let mut sink = |p: Progress| seen.push(p);
            let mut job = Job::new(&token, &mut sink);
            let target = crate::transcode::tests::rev_c_target();
            let first = media.transcode(&source, &target, &mut job).unwrap();
            assert!(first.0.ends_with("converted-1.mp4"), "{}", first.0);
            assert_eq!(media.load(&first).unwrap(), b"converted");
            let second = media.transcode(&source, &target, &mut job).unwrap();
            assert!(
                !Path::new(&first.0).exists(),
                "the previous output is deleted"
            );
            assert!(Path::new(&second.0).exists());
            let totals: Vec<u64> = seen.iter().map(|p| p.total).collect();
            assert!(totals.iter().all(|t| *t == 10_000), "{totals:?}");
            assert_eq!(
                seen.last(),
                Some(&Progress::new(JobPhase::Convert, 10_000, 10_000))
            );
            // The folder goes away with the transcoder.
            let folder = Path::new(&second.0).parent().unwrap().to_path_buf();
            drop(media);
            assert!(!folder.exists());
        }

        #[test]
        fn probe_and_stream_go_through_the_found_tools() {
            let mut media = transcoder();
            let dir = tempfile::tempdir().unwrap();
            let raw = dir.path().join("clip.h264");
            fs::write(&raw, [0, 0, 0, 1, 0x67]).unwrap();
            let raw = MediaLocation(raw.to_str().unwrap().to_string());
            assert_eq!(media.probe(&raw).unwrap().format, MediaFormat::H264);
            let spec = StreamSpec {
                size: Size::new(1, 1),
                fps: 10,
            };
            let mut frames = media.stream(&source(dir.path()), spec).unwrap();
            assert_eq!(frames.frame_at(Duration::ZERO).unwrap().as_rgba(), b"AAAA");
            assert!(
                media
                    .stream(&MediaLocation("/nonexistent.mp4".into()), spec)
                    .is_err()
            );
            let still = StreamSpec { fps: 0, ..spec };
            assert!(matches!(
                media.stream(&raw, still),
                Err(BezelError::InvalidInput(_))
            ));
            assert!(
                media
                    .load(&MediaLocation("/nonexistent.mp4".into()))
                    .is_err()
            );
        }
    }

    /// Runs the real ffmpeg of this machine: `cargo test -p bezel-media --
    /// --ignored`. Needs ffmpeg and ffprobe with libx264 on `PATH`.
    mod real_ffmpeg {
        use std::process::Command;

        use bezel_core::domain::catalog::model_by_id;
        use bezel_core::domain::device::{DeviceModel, ModelId};
        use bezel_core::domain::frame::Rgba;
        use bezel_core::domain::framing::{PanelLayout, VideoFit, VideoFraming};
        use bezel_core::domain::geometry::Orientation;
        use bezel_core::domain::media::{
            ConvertOptions, MediaKind, PREVIEW_FPS, Tone, UploadProfile, VideoCodec,
            VideoPixelFormat, framed_options,
        };

        use super::*;

        fn profile(id: &'static str) -> UploadProfile {
            UploadProfile::for_model(model_by_id(ModelId(id)).unwrap()).unwrap()
        }

        /// ffmpeg's test pattern with a tone, `seconds` long.
        fn generated(
            dir: &Path,
            name: &str,
            size: &str,
            seconds: u32,
            extra: &[&str],
        ) -> MediaLocation {
            let path = dir.join(name);
            let video = format!("testsrc=size={size}:rate=30:duration={seconds}");
            let audio = format!("sine=frequency=440:duration={seconds}");
            let status = Command::new("ffmpeg")
                .args([
                    "-v", "error", "-y", "-f", "lavfi", "-i", &video, "-f", "lavfi", "-i", &audio,
                ])
                .args(extra)
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
            MediaLocation(path.to_str().unwrap().to_string())
        }

        fn quiet_job<'a>(token: &'a CancelToken, sink: &'a mut dyn FnMut(Progress)) -> Job<'a> {
            Job::new(token, sink)
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_converts_into_the_rev_c_profile() {
            let mut media = FfmpegTranscoder::new(None);
            assert!(matches!(media.tools(), MediaTools::Ready { .. }));
            let dir = tempfile::tempdir().unwrap();
            let source = generated(
                dir.path(),
                "src.mp4",
                "1920x1080",
                2,
                &["-pix_fmt", "yuv444p"],
            );
            let info = media.probe(&source).unwrap();
            assert!(info.has_audio);
            assert_eq!(
                info.video.unwrap().pixel_format,
                Some(VideoPixelFormat::Other)
            );
            let profile = profile("turing-8.8");
            assert!(!profile.mismatches(MediaKind::Video, &info).is_empty());
            let options = ConvertOptions {
                quarter_turns: 1,
                crop: Some(bezel_core::domain::frame::Rect::new(300, 0, 480, 1920)),
                frame_rate: Some(24),
                tone: Tone::Natural,
                ..ConvertOptions::default()
            };
            let token = CancelToken::new();
            let mut last = None;
            let mut sink = |p: Progress| last = Some(p);
            let mut job = quiet_job(&token, &mut sink);
            let output = media
                .transcode(&source, &profile.transcode_target(options), &mut job)
                .unwrap();
            assert_eq!(last, Some(Progress::new(JobPhase::Convert, 2000, 2000)));
            let converted = media.probe(&output).unwrap();
            assert_eq!(profile.mismatches(MediaKind::Video, &converted), vec![]);
            let track = converted.video.unwrap();
            assert_eq!(track.frame_rate.map(|r| r.fps().round()), Some(24.0));
            // ffprobe agrees with the native reader.
            let json = Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-print_format",
                    "json",
                    "-show_format",
                    "-show_streams",
                    &output.0,
                ])
                .output()
                .unwrap();
            let described =
                probe::from_ffprobe(&String::from_utf8_lossy(&json.stdout), converted.bytes)
                    .unwrap();
            assert_eq!(described.dimensions, converted.dimensions);
            let theirs = described.video.unwrap();
            assert_eq!(
                (theirs.codec, theirs.pixel_format),
                (VideoCodec::H264, Some(VideoPixelFormat::Yuv420p))
            );
            assert_eq!(theirs.b_frames, track.b_frames);
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_makes_a_tur_usb_stream_without_b_frames() {
            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            let source = generated(dir.path(), "src.mp4", "640x360", 1, &[]);
            let profile = profile("turing-usb-8.8");
            let options = ConvertOptions {
                quarter_turns: 1,
                tone: Tone::Darkened,
                ..ConvertOptions::default()
            };
            let token = CancelToken::new();
            let mut sink = |_: Progress| {};
            let mut job = quiet_job(&token, &mut sink);
            let output = media
                .transcode(&source, &profile.transcode_target(options), &mut job)
                .unwrap();
            assert!(output.0.ends_with(".h264"));
            let converted = media.probe(&output).unwrap();
            assert_eq!(profile.mismatches(MediaKind::Video, &converted), vec![]);
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_is_killed_on_cancel_and_leaves_nothing() {
            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            let source = generated(
                dir.path(),
                "long.mp4",
                "320x240",
                60,
                &["-preset", "ultrafast"],
            );
            let token = CancelToken::new();
            let remote = token.clone();
            let mut sink = |p: Progress| {
                if p.done > 0 {
                    remote.cancel();
                }
            };
            let mut job = quiet_job(&token, &mut sink);
            let started = std::time::Instant::now();
            let result = media.transcode(
                &source,
                &profile("turing-8.8").transcode_target(ConvertOptions::default()),
                &mut job,
            );
            assert_eq!(result.unwrap_err(), BezelError::Cancelled { partial: None });
            assert!(started.elapsed() < Duration::from_secs(20));
            let folder = media.work.as_ref().unwrap().path();
            assert_eq!(
                fs::read_dir(folder).unwrap().count(),
                0,
                "the partial output is deleted"
            );
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_streams_a_looping_gif() {
            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("anim.gif");
            let status = Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=size=64x48:rate=10:duration=1",
                ])
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
            let gif = MediaLocation(path.to_str().unwrap().to_string());
            assert_eq!(media.probe(&gif).unwrap().format, MediaFormat::Gif);
            let spec = StreamSpec::raw(Size::new(64, 48), Size::new(40, 40), 10);
            let mut frames = media.stream(&gif, spec).unwrap();
            let first = frames.frame_at(Duration::ZERO).unwrap().clone();
            assert_eq!(first.size(), Size::new(64, 48), "the raw picture");
            let later = frames.frame_at(Duration::from_millis(500)).unwrap().clone();
            assert_ne!(first, later);
            // 1 s at 10 fps: 1.2 s is frame 2 of the second pass.
            let looped = frames
                .frame_at(Duration::from_millis(1200))
                .unwrap()
                .clone();
            let again = frames.frame_at(Duration::from_millis(200)).unwrap().clone();
            assert_eq!(looped, again);
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_takes_a_poster_one_second_in_covering_the_canvas() {
            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            let source = generated(dir.path(), "clip.mov", "1920x1080", 3, &[]);
            let info = media.probe(&source).unwrap();
            let canvas = Size::new(1920, 480);
            let spec = PosterSpec::for_canvas(canvas, &info);
            assert_eq!(spec.at, Duration::from_secs(1));
            let poster = media.poster(&source, spec).unwrap();
            assert_eq!(poster.size(), canvas);
            let first = PosterSpec {
                at: Duration::ZERO,
                ..spec
            };
            let first = media.poster(&source, first).unwrap();
            assert_ne!(
                poster, first,
                "the pattern moves: 1 s in is another picture"
            );
            let late = PosterSpec {
                at: Duration::from_secs(60),
                ..spec
            };
            let err = media.poster(&source, late).unwrap_err();
            assert!(err.to_string().contains("no picture 60.0 s in"), "{err}");
        }

        /// A looping GIF of 64x16 pictures shown for `delays_ms` each.
        fn animated_gif(dir: &Path, delays_ms: &[u32]) -> MediaLocation {
            use image::codecs::gif::{GifEncoder, Repeat};
            use image::{Delay, Frame as Picture, Rgba, RgbaImage};

            let path = dir.join("waves.gif");
            let file = fs::File::create(&path).unwrap();
            let mut encoder = GifEncoder::new(file);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            let pictures = delays_ms
                .iter()
                .zip([40u8, 120, 200, 250])
                .map(|(ms, shade)| {
                    let picture =
                        RgbaImage::from_pixel(64, 16, Rgba([shade, 255 - shade, 90, 255]));
                    Picture::from_parts(picture, 0, 0, Delay::from_numer_denom_ms(*ms, 1))
                });
            encoder.encode_frames(pictures).unwrap();
            drop(encoder);
            MediaLocation(path.to_str().unwrap().to_string())
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_turns_an_animated_gif_into_a_rev_c_video() {
            use bezel_core::domain::geometry::Orientation;
            use bezel_core::domain::media::{FrameRate, fitting_options};

            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            let gif = animated_gif(dir.path(), &[100, 70, 300]);
            let info = media.probe(&gif).unwrap();
            let track = info.video.unwrap();
            assert_eq!(track.duration, Some(Duration::from_millis(470)));
            assert_eq!(track.frame_rate, FrameRate::new(100, 7));
            // Its poster is its first picture, covering the canvas.
            let canvas = Size::new(1920, 480);
            let spec = PosterSpec::for_canvas(canvas, &info);
            assert_eq!(spec.at, Duration::ZERO);
            let poster = media.poster(&gif, spec).unwrap();
            assert_eq!(poster.size(), canvas);
            assert_eq!(&poster.as_rgba()[..3], &[40, 215, 90], "the first picture");
            // Sent to the 8.8" standing horizontally: turned, cropped, and
            // resampled at a constant 15 fps (its 70 ms delay) for one pass:
            // ffmpeg plays a looping GIF once.
            let model = model_by_id(ModelId("turing-8.8")).unwrap();
            let options = fitting_options(model, Orientation::Landscape, &info).for_source(&info);
            assert_eq!((options.quarter_turns, options.frame_rate), (1, Some(15)));
            let profile = profile("turing-8.8");
            let token = CancelToken::new();
            let mut last = None;
            let mut sink = |p: Progress| last = Some(p);
            let mut job = quiet_job(&token, &mut sink);
            let output = media
                .transcode(&gif, &profile.transcode_target(options), &mut job)
                .unwrap();
            assert_eq!(last.map(|p| p.total), Some(470), "progress over one pass");
            let converted = media.probe(&output).unwrap();
            assert_eq!(profile.mismatches(MediaKind::Video, &converted), vec![]);
            let track = converted.video.unwrap();
            let rate = track.frame_rate.unwrap();
            assert_eq!(rate.fps().round(), 15.0, "constant 15 fps: {rate:?}");
            let seconds = track.duration.unwrap().as_secs_f64();
            assert!((0.3..1.0).contains(&seconds), "one pass: {seconds} s");
        }

        /// The pad color of the fitted tests, and lavfi's `red`, `green`
        /// and `blue`.
        const RED: [u8; 3] = [200, 30, 40];
        const RED_PURE: [u8; 3] = [255, 0, 0];
        const GREEN: [u8; 3] = [0, 128, 0];
        const BLUE: [u8; 3] = [0, 0, 255];

        /// A video of ffmpeg's filter `graph` (lavfi), in yuv420p.
        fn painted(dir: &Path, name: &str, graph: &str) -> MediaLocation {
            let path = dir.join(name);
            let status = Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-f", "lavfi", "-i", graph])
                .args(["-pix_fmt", "yuv420p"])
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success());
            MediaLocation(path.to_str().unwrap().to_string())
        }

        /// The color of `frame` at (`x`, `y`).
        fn pixel(frame: &Frame, x: u32, y: u32) -> [u8; 3] {
            let at = usize::try_from(y * frame.size().width + x).unwrap() * 4;
            let rgba = &frame.as_rgba()[at..at + 3];
            [rgba[0], rgba[1], rgba[2]]
        }

        /// Whether `frame` shows about `color` at (`x`, `y`): yuv420p and
        /// H.264 move flat colors by a few steps.
        fn shows(frame: &Frame, x: u32, y: u32, color: [u8; 3]) -> bool {
            let seen = pixel(frame, x, y);
            seen.iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 12)
        }

        fn eight_eight() -> (&'static DeviceModel, Option<PanelLayout>) {
            let model = model_by_id(ModelId("turing-8.8")).unwrap();
            (model, Some(PanelLayout::of(model)))
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_fits_a_framed_video_on_the_poster_and_the_panel() {
            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            let source = painted(
                dir.path(),
                "green.mp4",
                "color=c=green:s=1920x1080:r=30:d=2",
            );
            let info = media.probe(&source).unwrap();
            let (model, panel) = eight_eight();
            let fitted = VideoFraming {
                fit: VideoFit::Contain,
                pad: Rgba::opaque(RED[0], RED[1], RED[2]),
                ..VideoFraming::default()
            }
            .resolve(info.dimensions, Orientation::Landscape, panel);
            // The poster: 852x480 of picture in the middle of the 1920x480
            // canvas, the pad color on both sides.
            let canvas = Size::new(1920, 480);
            let spec = PosterSpec::framed(canvas, &info, &fitted);
            let poster = media.poster(&source, spec).unwrap();
            assert_eq!(poster.size(), canvas);
            for x in [0, 500, 1420, 1919] {
                assert!(
                    shows(&poster, x, 240, RED),
                    "x={x}: {:?}",
                    pixel(&poster, x, 240)
                );
            }
            for x in [540, 960, 1380] {
                assert!(
                    shows(&poster, x, 240, GREEN),
                    "x={x}: {:?}",
                    pixel(&poster, x, 240)
                );
            }
            // The conversion for the 8.8": turned to the 480x1920 panel, the
            // picture in rows 534..1386, the pad above and below.
            let options = framed_options(model, Orientation::Landscape, &info, &fitted);
            let profile = profile("turing-8.8");
            let token = CancelToken::new();
            let mut sink = |_: Progress| {};
            let mut job = quiet_job(&token, &mut sink);
            let output = media
                .transcode(&source, &profile.transcode_target(options), &mut job)
                .unwrap();
            let converted = media.probe(&output).unwrap();
            assert_eq!(profile.mismatches(MediaKind::Video, &converted), vec![]);
            let panel_size = Size::new(480, 1920);
            let spec = StreamSpec::raw(panel_size, panel_size, 15);
            let mut frames = media.stream(&output, spec).unwrap();
            let first = frames.frame_at(Duration::ZERO).unwrap();
            assert_eq!(first.size(), panel_size);
            for y in [8, 520, 1400, 1912] {
                assert!(
                    shows(first, 240, y, RED),
                    "y={y}: {:?}",
                    pixel(first, 240, y)
                );
            }
            for y in [548, 960, 1372] {
                assert!(
                    shows(first, 240, y, GREEN),
                    "y={y}: {:?}",
                    pixel(first, 240, y)
                );
            }
        }

        #[test]
        #[ignore = "needs ffmpeg with libx264 on PATH"]
        fn real_ffmpeg_stands_a_panel_native_video_up_and_decodes_it_raw() {
            let mut media = FfmpegTranscoder::new(None);
            let dir = tempfile::tempdir().unwrap();
            // Like Dragon Ball: a 480x1920 video already turned for the
            // 8.8", its top red and its bottom blue.
            let source = painted(
                dir.path(),
                "halves.mp4",
                "color=c=red:s=480x960:r=30:d=1[top];color=c=blue:s=480x960:r=30:d=1[bottom];[top][bottom]vstack",
            );
            check_panel_native(&mut media, &source, true);
            // ffmpeg decodes at most 15 pictures a second: a 1 s pattern at
            // 30 fps asked for at 30 is a pass of 15 pictures.
            let moving = generated(dir.path(), "moving.mp4", "480x1920", 1, &[]);
            let spec = StreamSpec::raw(Size::new(480, 1920), Size::new(1920, 480), 30);
            let mut frames = media.stream(&moving, spec).unwrap();
            let first = frames.frame_at(Duration::ZERO).unwrap().clone();
            let next = frames.frame_at(Duration::from_millis(67)).unwrap().clone();
            assert_ne!(first, next, "1/15 s later is the next picture");
            let looped = frames
                .frame_at(Duration::from_millis(1000))
                .unwrap()
                .clone();
            assert_eq!(looped, first, "15 pictures in the 1 s pass");
            // The vendor's real file, when its path is given
            // (`BEZEL_DRAGON_BALL_MP4=.../video/4801920/dragon.mp4`).
            match std::env::var_os("BEZEL_DRAGON_BALL_MP4") {
                Some(path) => {
                    let dragon = MediaLocation(path.to_string_lossy().into_owned());
                    check_panel_native(&mut media, &dragon, false);
                }
                None => eprintln!("BEZEL_DRAGON_BALL_MP4 unset: the vendor's file is not checked"),
            }
        }

        /// A panel-native 480x1920 video in a landscape theme on the 8.8":
        /// Auto turns it 270 degrees on the canvas and not at all on the
        /// panel (sent as it is), the poster stands it up, and host decoding
        /// hands over the stored picture as it is.
        fn check_panel_native(media: &mut FfmpegTranscoder, source: &MediaLocation, halves: bool) {
            let info = media.probe(source).unwrap();
            let native = Size::new(480, 1920);
            assert_eq!(info.dimensions, Some(native));
            let (model, panel) = eight_eight();
            let auto =
                VideoFraming::default().resolve(info.dimensions, Orientation::Landscape, panel);
            assert_eq!(auto.turns, 3);
            let options = framed_options(model, Orientation::Landscape, &info, &auto);
            assert!(options.is_identity(), "{options:?}");
            let canvas = Size::new(1920, 480);
            let poster = media
                .poster(source, PosterSpec::framed(canvas, &info, &auto))
                .unwrap();
            assert_eq!(poster.size(), canvas);
            let spec = StreamSpec::raw(native, canvas, PREVIEW_FPS);
            assert_eq!(spec.size, native);
            let mut frames = media.stream(source, spec).unwrap();
            let first = frames.frame_at(Duration::ZERO).unwrap().clone();
            assert_eq!(first.size(), native);
            if halves {
                // The stored top is the canvas's left.
                assert!(
                    shows(&poster, 100, 240, RED_PURE),
                    "{:?}",
                    pixel(&poster, 100, 240)
                );
                assert!(
                    shows(&poster, 1800, 240, BLUE),
                    "{:?}",
                    pixel(&poster, 1800, 240)
                );
                assert!(
                    shows(&first, 240, 100, RED_PURE),
                    "{:?}",
                    pixel(&first, 240, 100)
                );
                assert!(
                    shows(&first, 240, 1800, BLUE),
                    "{:?}",
                    pixel(&first, 240, 1800)
                );
            }
        }
    }
}
