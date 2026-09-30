//! Finding the external tools and telling what a local file is.
//!
//! Lookup (D-2026-09-30-storage-video-2): the ffmpeg path set in the CLI or
//! studio settings first (the program or its folder), then `PATH`; ffprobe
//! next to the ffmpeg found, then `PATH`. Nothing is bundled. A missing or
//! unusable tool is a reported state with install commands for the host
//! system, never a panic.
//!
//! Files: MP4 headers and still images are read natively, so they need no
//! tool; anything else goes to ffprobe, or is `Unsupported` without it.

use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use bezel_core::domain::geometry::Size;
use bezel_core::domain::media::{
    FrameRate, MediaFormat, MediaInfo, VideoCodec, VideoPixelFormat, VideoTrack,
};
use bezel_core::{BezelError, Result};
use image::{ImageFormat, ImageReader};
use serde::Deserialize;

use crate::mp4;
use crate::process::{self, capture, file_url};

/// Longest a `-version` or `-encoders` query may take.
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest ffprobe may take to read a file's header.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// Bytes read to recognise a file.
const SNIFF_BYTES: usize = 16;
/// The encoder every conversion uses (the vendor's chain).
const ENCODER: &str = "libx264";

/// Where to look for ffmpeg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lookup {
    /// The path from the settings: the program or the folder holding it.
    pub(crate) configured: Option<PathBuf>,
    /// The `PATH` value searched after it.
    pub(crate) search_path: Option<OsString>,
}

impl Lookup {
    /// The settings' path, then this process's `PATH`.
    pub(crate) fn from_env(configured: Option<PathBuf>) -> Self {
        Self {
            configured,
            search_path: std::env::var_os("PATH"),
        }
    }
}

/// ffmpeg and ffprobe, found and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tools {
    pub(crate) ffmpeg: PathBuf,
    pub(crate) ffprobe: PathBuf,
    pub(crate) version: String,
}

/// Why no usable ffmpeg was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Absence {
    /// Neither the settings nor `PATH` name an ffmpeg.
    NotFound,
    /// The program does not run or does not answer like ffmpeg.
    Unusable { ffmpeg: PathBuf, reason: String },
    /// This ffmpeg build has no H.264 encoder (Fedora's `ffmpeg-free`).
    NoEncoder { ffmpeg: PathBuf },
    /// No ffprobe next to ffmpeg nor on `PATH`.
    NoProbe { ffmpeg: PathBuf },
}

impl fmt::Display for Absence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Absence::NotFound => f.write_str("ffmpeg was not found (settings, then PATH)"),
            Absence::Unusable { ffmpeg, reason } => {
                write!(f, "{} cannot be used: {reason}", ffmpeg.display())
            }
            Absence::NoEncoder { ffmpeg } => {
                write!(f, "{} has no {ENCODER} encoder", ffmpeg.display())
            }
            Absence::NoProbe { ffmpeg } => {
                write!(f, "no ffprobe next to {} nor on PATH", ffmpeg.display())
            }
        }
    }
}

/// Finds a usable ffmpeg (with libx264) and its ffprobe. Candidates in
/// order: the configured program (or `ffmpeg` in the configured folder),
/// then the first `ffmpeg` on `PATH`. The reason of the last candidate that
/// failed is returned when none works.
pub(crate) fn locate(lookup: &Lookup) -> std::result::Result<Tools, Absence> {
    let mut absence = Absence::NotFound;
    for ffmpeg in candidates(lookup) {
        match check(&ffmpeg, lookup) {
            Ok(tools) => return Ok(tools),
            Err(why) => {
                tracing::warn!("ffmpeg candidate rejected: {why}");
                absence = why;
            }
        }
    }
    Err(absence)
}

fn program(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn candidates(lookup: &Lookup) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(configured) = &lookup.configured {
        if configured.is_dir() {
            out.push(configured.join(program("ffmpeg")));
        } else {
            out.push(configured.clone());
        }
    }
    if let Some(found) = find_in_path(&program("ffmpeg"), lookup.search_path.as_ref())
        && !out.contains(&found)
    {
        out.push(found);
    }
    out
}

/// The first file called `name` in the folders of `search_path`.
fn find_in_path(name: &str, search_path: Option<&OsString>) -> Option<PathBuf> {
    std::env::split_paths(search_path?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn check(ffmpeg: &Path, lookup: &Lookup) -> std::result::Result<Tools, Absence> {
    let unusable = |reason: String| Absence::Unusable {
        ffmpeg: ffmpeg.to_path_buf(),
        reason,
    };
    if !ffmpeg.is_file() {
        return Err(unusable("no such file".to_string()));
    }
    let version = capture(ffmpeg, &["-hide_banner", "-version"], TOOL_TIMEOUT)
        .map_err(|e| unusable(e.to_string()))?;
    let version = version_of("ffmpeg", &version.stdout)
        .ok_or_else(|| unusable("it does not answer like ffmpeg".to_string()))?;
    let encoders = capture(ffmpeg, &["-hide_banner", "-encoders"], TOOL_TIMEOUT)
        .map_err(|e| unusable(e.to_string()))?;
    if !lists_encoder(&encoders.stdout, ENCODER) {
        return Err(Absence::NoEncoder {
            ffmpeg: ffmpeg.to_path_buf(),
        });
    }
    let ffprobe = find_ffprobe(ffmpeg, lookup).ok_or_else(|| Absence::NoProbe {
        ffmpeg: ffmpeg.to_path_buf(),
    })?;
    tracing::info!(
        "using {} {version} and {}",
        ffmpeg.display(),
        ffprobe.display()
    );
    Ok(Tools {
        ffmpeg: ffmpeg.to_path_buf(),
        ffprobe,
        version,
    })
}

fn find_ffprobe(ffmpeg: &Path, lookup: &Lookup) -> Option<PathBuf> {
    let name = program("ffprobe");
    let sibling = ffmpeg.with_file_name(&name);
    let path = if sibling.is_file() {
        sibling
    } else {
        find_in_path(&name, lookup.search_path.as_ref())?
    };
    let out = capture(&path, &["-version"], TOOL_TIMEOUT).ok()?;
    version_of("ffprobe", &out.stdout).map(|_| path)
}

/// The version from the first line of `<tool> -version`
/// (`ffmpeg version 8.1.3 Copyright ...` → `8.1.3`).
fn version_of(tool: &str, output: &str) -> Option<String> {
    let mut words = output.lines().next()?.split_whitespace();
    if words.next()? != tool || words.next()? != "version" {
        return None;
    }
    words.next().map(str::to_string)
}

/// Whether `ffmpeg -encoders` lists `name` (lines are
/// ` V....D libx264   description`).
fn lists_encoder(output: &str, name: &str) -> bool {
    output
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(name))
}

/// The host, as far as install commands go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostSystem {
    /// Fedora and its derivatives (ffmpeg with libx264 comes from RPM Fusion).
    Fedora,
    /// Debian, Ubuntu and their derivatives.
    Debian,
    /// Another Linux distribution.
    OtherLinux,
    /// Windows.
    Windows,
    /// Anything else.
    Other,
}

impl HostSystem {
    /// This machine.
    pub(crate) fn detect() -> Self {
        let os = std::env::consts::OS;
        let release = if os == "linux" {
            fs::read_to_string("/etc/os-release").ok()
        } else {
            None
        };
        Self::from_parts(os, release.as_deref())
    }

    /// The system named by `std::env::consts::OS` and `/etc/os-release`.
    pub(crate) fn from_parts(os: &str, os_release: Option<&str>) -> Self {
        match os {
            "windows" => return HostSystem::Windows,
            "linux" => {}
            _ => return HostSystem::Other,
        }
        let ids: Vec<String> = os_release
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(key, _)| matches!(key.trim(), "ID" | "ID_LIKE"))
            .flat_map(|(_, value)| {
                value
                    .trim()
                    .trim_matches(['"', '\''])
                    .split_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect::<Vec<_>>()
            })
            .collect();
        let has = |name: &str| ids.iter().any(|id| id == name);
        if has("fedora") {
            HostSystem::Fedora
        } else if has("debian") || has("ubuntu") {
            HostSystem::Debian
        } else {
            HostSystem::OtherLinux
        }
    }
}

/// Enables RPM Fusion, where Fedora's ffmpeg with libx264 lives.
const RPM_FUSION: &str = "sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm";
/// Installs ffmpeg with dnf, replacing `ffmpeg-free` (no libx264) if present.
const DNF: &str = "sudo dnf install ffmpeg --allowerasing";
/// Installs ffmpeg with apt.
const APT: &str = "sudo apt install ffmpeg";
/// Installs a full ffmpeg build (with libx264) with winget.
const WINGET: &str = "winget install --id Gyan.FFmpeg -e";

/// Install commands for `system`, most likely first (Fedora needs both
/// lines, in order).
pub(crate) fn install_hints(system: HostSystem) -> Vec<String> {
    let lines: &[&str] = match system {
        HostSystem::Fedora => &[RPM_FUSION, DNF],
        HostSystem::Debian => &[APT],
        HostSystem::OtherLinux => &[DNF, APT],
        HostSystem::Windows => &[WINGET],
        HostSystem::Other => &[DNF, APT, WINGET],
    };
    lines.iter().map(|line| (*line).to_string()).collect()
}

/// What the file at `path` is. MP4 files and still images are read
/// natively; anything else goes to the ffprobe `ffprobe` returns (asked
/// only then), and is `Unsupported` without one.
pub(crate) fn probe_file(
    path: &Path,
    ffprobe: impl FnOnce() -> Option<PathBuf>,
) -> Result<MediaInfo> {
    let bytes = file_size(path)?;
    let head = read_head(path)?;
    let native = if mp4::sniff(&head) {
        let mut file = BufReader::new(File::open(path).map_err(|e| unreadable(path, &e))?);
        match mp4::probe(&mut file, bytes) {
            Ok(info) => return Ok(info),
            Err(e) => {
                tracing::debug!("{}: not a readable MP4 ({e})", path.display());
                BezelError::InvalidInput(format!("{} is not a readable MP4: {e}", path.display()))
            }
        }
    } else if let Ok(format) = image::guess_format(&head) {
        return still(path, bytes, format);
    } else {
        BezelError::Unsupported(format!(
            "{} is neither an MP4 video nor an image; reading it needs ffmpeg",
            path.display()
        ))
    };
    match ffprobe() {
        Some(ffprobe) => run_ffprobe(&ffprobe, path, bytes),
        None => Err(native),
    }
}

fn file_size(path: &Path) -> Result<u64> {
    let meta = fs::metadata(path).map_err(|e| unreadable(path, &e))?;
    if !meta.is_file() {
        return Err(BezelError::InvalidInput(format!(
            "{} is not a file",
            path.display()
        )));
    }
    Ok(meta.len())
}

fn read_head(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|e| unreadable(path, &e))?;
    let mut head = Vec::with_capacity(SNIFF_BYTES);
    file.take(SNIFF_BYTES as u64)
        .read_to_end(&mut head)
        .map_err(|e| unreadable(path, &e))?;
    Ok(head)
}

pub(crate) fn unreadable(path: &Path, e: &std::io::Error) -> BezelError {
    BezelError::InvalidInput(format!("cannot read {}: {e}", path.display()))
}

/// A still image: format and size from its header, no pixel decoded.
/// Image formats screens do not show (WebP, TIFF...) are `Other`.
fn still(path: &Path, bytes: u64, format: ImageFormat) -> Result<MediaInfo> {
    let format = match format {
        ImageFormat::Png => MediaFormat::Png,
        ImageFormat::Jpeg => MediaFormat::Jpeg,
        ImageFormat::Gif => MediaFormat::Gif,
        ImageFormat::Bmp => MediaFormat::Bmp,
        _ => MediaFormat::Other,
    };
    let dimensions = if format == MediaFormat::Other {
        None
    } else {
        let reader = ImageReader::open(path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|e| unreadable(path, &e))?;
        let (width, height) = reader.into_dimensions().map_err(|e| {
            BezelError::InvalidInput(format!("{} is not a valid image: {e}", path.display()))
        })?;
        Some(Size::new(width, height))
    };
    Ok(MediaInfo {
        format,
        bytes,
        dimensions,
        video: None,
        has_audio: false,
    })
}

fn run_ffprobe(ffprobe: &Path, path: &Path, bytes: u64) -> Result<MediaInfo> {
    let args = [
        OsString::from("-v"),
        "error".into(),
        "-print_format".into(),
        "json".into(),
        "-show_format".into(),
        "-show_streams".into(),
        file_url(path),
    ];
    let out = capture(ffprobe, &args, PROBE_TIMEOUT).map_err(|e| {
        BezelError::Unsupported(format!("ffprobe ({}) failed: {e}", ffprobe.display()))
    })?;
    if !out.success {
        return Err(BezelError::InvalidInput(format!(
            "ffprobe cannot read {}: {}",
            path.display(),
            process::quote(&out.stderr)
        )));
    }
    from_ffprobe(&out.stdout, bytes).ok_or_else(|| {
        BezelError::InvalidInput(format!(
            "ffprobe gave an unreadable description of {}",
            path.display()
        ))
    })
}

#[derive(Deserialize)]
struct Probed {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Container>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    pix_fmt: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    has_b_frames: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    disposition: Option<Disposition>,
}

#[derive(Deserialize)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Deserialize)]
struct Container {
    format_name: Option<String>,
    duration: Option<String>,
}

/// Maps ffprobe's JSON (`-show_format -show_streams`) to a [`MediaInfo`].
/// Only raw H.264 and GIF keep their format: an MP4 that reaches ffprobe
/// failed the native reader and is converted rather than trusted.
pub(crate) fn from_ffprobe(json: &str, bytes: u64) -> Option<MediaInfo> {
    let probed: Probed = serde_json::from_str(json).ok()?;
    let container = probed.format?;
    let name = container.format_name.unwrap_or_default();
    let format = match name.as_str() {
        "h264" => MediaFormat::H264,
        "gif" => MediaFormat::Gif,
        _ => MediaFormat::Other,
    };
    let is_image = name == "image2" || name.ends_with("_pipe");
    let has_audio = probed
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));
    let video = probed.streams.iter().find(|s| {
        s.codec_type.as_deref() == Some("video")
            && s.disposition.as_ref().is_none_or(|d| d.attached_pic == 0)
    });
    let dimensions = video
        .and_then(|s| Some(Size::new(s.width?, s.height?)))
        .filter(|size| size.area() > 0);
    let still = is_image || format == MediaFormat::Gif;
    let track = video.filter(|_| !still).map(|s| VideoTrack {
        codec: match s.codec_name.as_deref() {
            Some("h264") => VideoCodec::H264,
            _ => VideoCodec::Other,
        },
        pixel_format: s.pix_fmt.as_deref().map(|p| match p {
            "yuv420p" => VideoPixelFormat::Yuv420p,
            _ => VideoPixelFormat::Other,
        }),
        b_frames: s.has_b_frames.map(|n| n > 0),
        frame_rate: rate(s.avg_frame_rate.as_deref()).or_else(|| rate(s.r_frame_rate.as_deref())),
        duration: seconds(s.duration.as_deref()).or_else(|| seconds(container.duration.as_deref())),
    });
    Some(MediaInfo {
        format,
        bytes,
        dimensions,
        video: track,
        has_audio,
    })
}

/// `"30000/1001"` → a frame rate; `"0/0"` and garbage → `None`.
fn rate(text: Option<&str>) -> Option<FrameRate> {
    let (num, den) = text?.split_once('/')?;
    FrameRate::new(num.parse().ok()?, den.parse().ok()?)
}

/// `"12.500000"` seconds → a duration.
fn seconds(text: Option<&str>) -> Option<Duration> {
    let value: f64 = text?.parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| Duration::from_secs_f64(value))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bezel_core::domain::job::{CancelToken, Job};
    use bezel_core::domain::media::{MediaTools, StreamSpec};
    use bezel_core::ports::{MediaLocation, MediaTranscoder};

    use super::*;
    use crate::FfmpegTranscoder;
    use crate::mp4::fixtures::Movie;

    /// A file in a fresh temporary folder.
    pub(crate) fn temp_file(name: &str, bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    fn location(path: &Path) -> MediaLocation {
        MediaLocation(path.to_str().unwrap().to_string())
    }

    fn nowhere() -> Lookup {
        Lookup {
            configured: Some(PathBuf::from("/nonexistent/bezel/ffmpeg")),
            search_path: Some(OsString::new()),
        }
    }

    #[test]
    fn missing_ffmpeg_is_reported_not_fatal() {
        let mut media = FfmpegTranscoder::with_lookup(nowhere(), HostSystem::Fedora);
        let tools = media.tools();
        let MediaTools::Missing { install_hints } = tools else {
            panic!("no ffmpeg anywhere must be Missing, got {tools:?}");
        };
        assert_eq!(install_hints.len(), 2);
        assert!(install_hints[0].contains("rpmfusion"));
        assert!(install_hints[1].starts_with("sudo dnf install ffmpeg"));
        // An MP4 and an image are still inspected natively.
        let (_dir, mp4) = temp_file("clip.mp4", &Movie::in_rev_c_profile().bytes());
        let info = media.probe(&location(&mp4)).unwrap();
        assert_eq!(info.dimensions, Some(Size::new(480, 1920)));
        // Anything else, a conversion and host decoding are Unsupported.
        let (_other, avi) = temp_file("clip.avi", b"RIFF\0\0\0\0AVI LIST");
        assert!(matches!(
            media.probe(&location(&avi)),
            Err(BezelError::Unsupported(_))
        ));
        let target = crate::transcode::tests::rev_c_target();
        let token = CancelToken::new();
        let mut sink = |_| {};
        let mut job = Job::new(&token, &mut sink);
        let converted = media.transcode(&location(&mp4), &target, &mut job);
        let Err(BezelError::Unsupported(why)) = converted else {
            panic!("a conversion without ffmpeg must be Unsupported");
        };
        assert!(why.contains("sudo dnf install ffmpeg"), "{why}");
        let spec = StreamSpec {
            size: Size::new(4, 4),
            fps: 10,
        };
        assert!(matches!(
            media.stream(&location(&mp4), spec),
            Err(BezelError::Unsupported(_))
        ));
        // Loading needs no tool either.
        assert_eq!(
            media.load(&location(&avi)).unwrap(),
            b"RIFF\0\0\0\0AVI LIST"
        );
    }

    #[test]
    fn install_hints_follow_the_host_system() {
        assert_eq!(install_hints(HostSystem::Debian), vec![APT.to_string()]);
        assert_eq!(install_hints(HostSystem::Windows), vec![WINGET.to_string()]);
        assert_eq!(install_hints(HostSystem::OtherLinux).len(), 2);
        assert_eq!(install_hints(HostSystem::Other).len(), 3);
        let fedora = "NAME=\"Fedora Linux\"\nID=fedora\nVERSION_ID=44\n";
        assert_eq!(
            HostSystem::from_parts("linux", Some(fedora)),
            HostSystem::Fedora
        );
        let mint = "ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";
        assert_eq!(
            HostSystem::from_parts("linux", Some(mint)),
            HostSystem::Debian
        );
        let nobara = "ID='nobara'\nID_LIKE='rhel centos fedora'\n";
        assert_eq!(
            HostSystem::from_parts("linux", Some(nobara)),
            HostSystem::Fedora
        );
        assert_eq!(
            HostSystem::from_parts("linux", Some("ID=arch\n")),
            HostSystem::OtherLinux
        );
        assert_eq!(
            HostSystem::from_parts("linux", None),
            HostSystem::OtherLinux
        );
        assert_eq!(HostSystem::from_parts("windows", None), HostSystem::Windows);
        assert_eq!(HostSystem::from_parts("macos", None), HostSystem::Other);
        let _ = HostSystem::detect();
    }

    #[test]
    fn versions_and_encoders_are_read_from_the_tools_output() {
        let line =
            "ffmpeg version 8.1.3 Copyright (c) 2000-2026 the FFmpeg developers\nbuilt with gcc";
        assert_eq!(version_of("ffmpeg", line), Some("8.1.3".to_string()));
        assert_eq!(version_of("ffprobe", line), None);
        assert_eq!(version_of("ffmpeg", "ffmpeg"), None);
        assert_eq!(version_of("ffmpeg", ""), None);
        let encoders = " V....D libx264rgb   libx264 RGB\n V....D libopenh264  OpenH264\n";
        assert!(!lists_encoder(encoders, "libx264"));
        assert!(lists_encoder(
            " V....D libx264              libx264 H.264",
            "libx264"
        ));
        assert_eq!(
            Absence::NotFound.to_string(),
            "ffmpeg was not found (settings, then PATH)"
        );
        let no_probe = Absence::NoProbe {
            ffmpeg: PathBuf::from("/x/ffmpeg"),
        };
        assert!(
            no_probe
                .to_string()
                .contains("no ffprobe next to /x/ffmpeg")
        );
    }

    #[test]
    fn stills_are_probed_without_ffmpeg() {
        let cases = [
            (ImageFormat::Png, MediaFormat::Png),
            (ImageFormat::Jpeg, MediaFormat::Jpeg),
            (ImageFormat::Gif, MediaFormat::Gif),
            (ImageFormat::Bmp, MediaFormat::Bmp),
        ];
        let dir = tempfile::tempdir().unwrap();
        for (format, expected) in cases {
            let path = dir
                .path()
                .join(format!("still.{}", format.extensions_str()[0]));
            image::RgbImage::new(3, 2)
                .save_with_format(&path, format)
                .unwrap();
            let info = probe_file(&path, || None).unwrap();
            assert_eq!(info.format, expected, "{format:?}");
            assert_eq!(info.dimensions, Some(Size::new(3, 2)));
            assert_eq!(info.video, None);
            assert_eq!(info.bytes, fs::metadata(&path).unwrap().len());
        }
        // A WebP is an image screens do not show.
        let (_webp, webp) = temp_file("a.webp", b"RIFF\0\0\0\0WEBPVP8 ");
        let info = probe_file(&webp, || None).unwrap();
        assert_eq!((info.format, info.dimensions), (MediaFormat::Other, None));
        // A PNG signature with nothing behind it.
        let (_bad, bad) = temp_file("bad.png", b"\x89PNG\r\n\x1a\n\0\0");
        assert!(matches!(
            probe_file(&bad, || None),
            Err(BezelError::InvalidInput(_))
        ));
    }

    #[test]
    fn unreadable_paths_and_broken_mp4s_are_invalid_input() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            probe_file(dir.path(), || None),
            Err(BezelError::InvalidInput(_))
        ));
        let missing = dir.path().join("none.mp4");
        let err = probe_file(&missing, || None).unwrap_err();
        assert!(err.to_string().contains("cannot read"), "{err}");
        let mut broken = Movie::in_rev_c_profile().bytes();
        broken.truncate(40);
        let (_b, path) = temp_file("broken.mp4", &broken);
        let err = probe_file(&path, || None).unwrap_err();
        assert!(err.to_string().contains("not a readable MP4"), "{err}");
    }

    #[test]
    fn ffprobe_json_is_mapped() {
        let raw = r#"{"streams":[{"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p",
            "width":192,"height":48,"has_b_frames":0,"r_frame_rate":"48/1","avg_frame_rate":"25/1"}],
            "format":{"format_name":"h264","size":"1604"}}"#;
        let info = from_ffprobe(raw, 1604).unwrap();
        assert_eq!(info.format, MediaFormat::H264);
        assert_eq!(info.dimensions, Some(Size::new(192, 48)));
        let track = info.video.unwrap();
        assert_eq!(track.codec, VideoCodec::H264);
        assert_eq!(track.pixel_format, Some(VideoPixelFormat::Yuv420p));
        assert_eq!(track.b_frames, Some(false));
        assert_eq!(track.frame_rate, FrameRate::new(25, 1));
        assert_eq!(track.duration, None);
        let mkv = r#"{"streams":[
            {"codec_type":"video","codec_name":"mjpeg","width":600,"height":600,"disposition":{"attached_pic":1}},
            {"codec_type":"video","codec_name":"vp9","pix_fmt":"yuv420p10le","width":1920,"height":1080,
             "has_b_frames":2,"avg_frame_rate":"0/0","r_frame_rate":"30000/1001","duration":"n/a"},
            {"codec_type":"audio","codec_name":"opus"}],
            "format":{"format_name":"matroska,webm","duration":"12.500000"}}"#;
        let info = from_ffprobe(mkv, 9).unwrap();
        assert_eq!(info.format, MediaFormat::Other);
        assert!(info.has_audio);
        assert_eq!(info.dimensions, Some(Size::new(1920, 1080)));
        let track = info.video.unwrap();
        assert_eq!(track.codec, VideoCodec::Other);
        assert_eq!(track.pixel_format, Some(VideoPixelFormat::Other));
        assert_eq!(track.b_frames, Some(true));
        assert_eq!(track.frame_rate, FrameRate::new(30000, 1001));
        assert_eq!(track.duration, Some(Duration::from_millis(12_500)));
        let webp = r#"{"streams":[{"codec_type":"video","codec_name":"webp","width":8,"height":8}],
            "format":{"format_name":"webp_pipe"}}"#;
        let info = from_ffprobe(webp, 1).unwrap();
        assert_eq!((info.format, info.video), (MediaFormat::Other, None));
        assert_eq!(info.dimensions, Some(Size::new(8, 8)));
        assert!(from_ffprobe("{}", 1).is_none(), "no format section");
        assert!(from_ffprobe("not json", 1).is_none());
        assert_eq!(rate(Some("24")), None);
        assert_eq!(seconds(Some("-1")), None);
    }

    #[cfg(unix)]
    mod with_fake_tools {
        use super::*;
        use crate::fakes;

        fn lookup(configured: Option<&str>, path: &[&str]) -> Lookup {
            let root = fakes::dir();
            Lookup {
                configured: configured.map(|c| root.join(c)),
                search_path: Some(std::env::join_paths(path.iter().map(|p| root.join(p))).unwrap()),
            }
        }

        #[test]
        fn the_configured_path_comes_before_path() {
            let root = fakes::dir();
            let tools = locate(&lookup(Some("ready/ffmpeg"), &["other"])).unwrap();
            assert_eq!(tools.ffmpeg, root.join("ready/ffmpeg"));
            assert_eq!(tools.ffprobe, root.join("ready/ffprobe"));
            assert_eq!(tools.version, "9.9-fake");
            // A folder is accepted; a broken setting falls back to PATH.
            assert_eq!(
                locate(&lookup(Some("ready"), &[])).unwrap().ffmpeg,
                root.join("ready/ffmpeg")
            );
            let fallback = locate(&lookup(Some("nothing/here"), &["", "empty", "ready"])).unwrap();
            assert_eq!(fallback.ffmpeg, root.join("ready/ffmpeg"));
            // ffprobe is also looked up on PATH.
            let split = locate(&lookup(Some("noprobe/ffmpeg"), &["probe-only"])).unwrap();
            assert_eq!(split.ffprobe, root.join("probe-only/ffprobe"));
        }

        #[test]
        fn unusable_tools_are_reported_with_their_reason() {
            let err = locate(&lookup(Some("nox264/ffmpeg"), &[])).unwrap_err();
            assert!(matches!(err, Absence::NoEncoder { .. }), "{err}");
            assert!(err.to_string().ends_with("has no libx264 encoder"), "{err}");
            let err = locate(&lookup(Some("noprobe/ffmpeg"), &[])).unwrap_err();
            assert!(matches!(err, Absence::NoProbe { .. }), "{err}");
            let err = locate(&lookup(Some("impostor/ffmpeg"), &[])).unwrap_err();
            assert!(
                err.to_string().contains("does not answer like ffmpeg"),
                "{err}"
            );
            let err = locate(&lookup(Some("missing/ffmpeg"), &[])).unwrap_err();
            assert!(err.to_string().contains("no such file"), "{err}");
            assert_eq!(
                locate(&lookup(None, &["empty"])).unwrap_err(),
                Absence::NotFound
            );
            let mut media =
                FfmpegTranscoder::with_lookup(lookup(Some("nox264"), &[]), HostSystem::Debian);
            assert_eq!(
                media.tools(),
                MediaTools::Missing {
                    install_hints: vec![APT.to_string()]
                }
            );
        }

        #[test]
        fn other_formats_are_described_by_ffprobe() {
            let root = fakes::dir();
            let (_d, raw) = temp_file("clip.h264", &[0, 0, 0, 1, 0x67, 0x42]);
            let info = probe_file(&raw, || Some(root.join("ready/ffprobe"))).unwrap();
            assert_eq!(info.format, MediaFormat::H264);
            assert_eq!(info.bytes, 6);
            assert_eq!(info.dimensions, Some(Size::new(480, 1920)));
            // A broken MP4 is handed to ffprobe too.
            let (_m, mp4) = temp_file("cut.mp4", &Movie::in_rev_c_profile().bytes()[..40]);
            assert!(probe_file(&mp4, || Some(root.join("ready/ffprobe"))).is_ok());
            let err = probe_file(&raw, || Some(root.join("failing/ffprobe"))).unwrap_err();
            assert!(err.to_string().contains("moov atom not found"), "{err}");
            let err = probe_file(&raw, || Some(root.join("impostor/ffprobe"))).unwrap_err();
            assert!(err.to_string().contains("unreadable description"), "{err}");
            let err = probe_file(&raw, || Some(root.join("missing/ffprobe"))).unwrap_err();
            assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        }
    }
}
