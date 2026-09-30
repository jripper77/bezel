//! `bezel storage`: the files a screen stores (internal flash and memory
//! card), uploads with progress and cancellation, deletes, device-side
//! playback and the boot media, over the core's `app::storage` use cases.
//!
//! Deleting, replacing a stored file and changing the boot media need
//! `--yes` (`Confirm::Yes`, D-2026-09-30-storage-video-1 and -5). Every one
//! of them first prints what it is about to do, `--yes` or not; without it
//! the command fails before anything changes the screen: `rm` and `boot` do
//! not even open it, and `put` only queries it to learn what it would replace.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use bezel_core::BezelError;
use bezel_core::app::open_screen;
use bezel_core::app::storage::{self as usecase, PreparedUpload, UploadRequest};
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::{CancelToken, Job, JobPhase, Progress};
use bezel_core::domain::media::{
    ConvertOptions, MediaInfo, MediaKind, MediaTools, TranscodeTarget, UploadProfile,
    fitting_options,
};
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::storage::{
    BootMedia, Capacity, FileEntry, Medium, Refusal, RemotePath, Repeat, StorageInfo,
    StorageLocation, UploadAction,
};
use bezel_core::ports::{DeviceBus, MediaLocation, MediaTranscoder, ScreenConnector, ScreenLink};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::messages::Messages;
use crate::{OrientationArg, Target};

/// Options of `bezel storage`.
#[derive(Debug, Args)]
pub struct StorageArgs {
    /// What to do with the stored files.
    #[command(subcommand)]
    pub action: StorageAction,
}

impl StorageArgs {
    /// The ffmpeg `put --ffmpeg` names, if any.
    pub fn ffmpeg(&self) -> Option<&Path> {
        match &self.action {
            StorageAction::Put(put) => put.ffmpeg.as_deref(),
            _ => None,
        }
    }

    /// True for the commands Ctrl+C cancels cleanly (a running upload).
    pub fn cancellable(&self) -> bool {
        matches!(self.action, StorageAction::Put(_))
    }
}

/// The `bezel storage` subcommands.
#[derive(Debug, Subcommand)]
pub enum StorageAction {
    /// Capacity and use of the screen's internal flash and memory card.
    Info {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// List the stored files: every folder, or one of internal/image,
    /// internal/video, sd/image and sd/video.
    Ls {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// Only this folder.
        #[arg(value_name = "FOLDER", value_parser = parse_location)]
        folder: Option<StorageLocation>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Send a picture (JPEG, PNG, BMP, GIF) or a video to the screen. A
    /// video not already in the screen's format is converted with ffmpeg
    /// (cropped to the panel's shape, never stretched). Ctrl+C cancels.
    Put(PutArgs),
    /// Delete stored files (needs --yes).
    Rm {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// The files, as `bezel storage ls` names them
        /// (`internal/video/intro.mp4`).
        #[arg(value_name = "PATH", required = true, value_parser = parse_path)]
        paths: Vec<RemotePath>,
        /// Really delete them.
        #[arg(long)]
        yes: bool,
    },
    /// Have the screen itself play a stored video (looping) or show a stored
    /// picture, until `bezel storage stop` or the next theme.
    Play {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// The file (`internal/video/intro.mp4`).
        #[arg(value_name = "PATH", value_parser = parse_path)]
        path: RemotePath,
        /// Play a video once instead of looping it.
        #[arg(long)]
        once: bool,
    },
    /// Stop what the screen plays on its own.
    Stop {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
    },
    /// Choose what the screen shows on its own after power-up: a stored file
    /// (it starts playing now) or `default` for its built-in start screen.
    /// Persistent: needs --yes.
    Boot(BootArgs),
}

/// Options of `bezel storage put`.
#[derive(Debug, Args)]
pub struct PutArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// The picture or video on this computer.
    pub file: PathBuf,
    /// Where it goes: `internal` or `sd`, a folder (`internal/video`) or a
    /// full path (`sd/video/intro.mp4`). Default: the internal folder of the
    /// file's kind, under a name made from the file's.
    #[arg(value_name = "DEST", value_parser = parse_destination)]
    pub dest: Option<Destination>,
    /// How the screen stands while the video plays. Default: the video's own
    /// shape (horizontal when wider than tall); a video already at the
    /// panel's native size and format is sent as it is.
    #[arg(long, value_enum)]
    pub orientation: Option<OrientationArg>,
    /// Convert the video to this many frames per second (the vendor app
    /// offers 24).
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=60))]
    pub fps: Option<u32>,
    /// Replace a stored file of the same name.
    #[arg(long)]
    pub yes: bool,
    /// The ffmpeg program (or its folder) that converts videos; default:
    /// ffmpeg on the PATH.
    #[arg(long, value_name = "PATH")]
    pub ffmpeg: Option<PathBuf>,
}

/// Options of `bezel storage boot`.
#[derive(Debug, Args)]
pub struct BootArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// A stored file (`internal/video/intro.mp4`) or `default`.
    #[arg(value_name = "PATH|default", value_parser = parse_boot)]
    pub media: BootMedia,
    /// Backlight level in percent the screen boots with (rev C screens store
    /// it with the boot media; default: the vendor's, about 67%).
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub brightness: Option<u8>,
    /// Really change the boot media.
    #[arg(long)]
    pub yes: bool,
}

/// Where `put` stores a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// A medium; the folder follows the file's kind.
    Medium(Medium),
    /// A folder.
    Folder(StorageLocation),
    /// A folder and a file name (normalized by the preflight).
    File(StorageLocation, String),
}

const PLACES: &str = "expected internal or sd, then /image or /video, then a name";

/// Parses `internal`, `sd/video` or `internal/image/logo.png`.
fn parse_destination(text: &str) -> Result<Destination, String> {
    let mut parts = text.splitn(3, '/');
    let medium = parts
        .next()
        .and_then(Medium::from_slug)
        .ok_or_else(|| format!("{text}: {PLACES}"))?;
    let Some(kind) = parts.next().filter(|k| !k.is_empty()) else {
        return Ok(Destination::Medium(medium));
    };
    let kind = MediaKind::from_slug(kind).ok_or_else(|| format!("{text}: {PLACES}"))?;
    let location = StorageLocation::new(medium, kind);
    match parts.next().filter(|name| !name.is_empty()) {
        None => Ok(Destination::Folder(location)),
        Some(name) => Ok(Destination::File(location, name.to_string())),
    }
}

/// Parses a folder: `<internal|sd>/<image|video>`.
fn parse_location(text: &str) -> Result<StorageLocation, String> {
    match parse_destination(text)? {
        Destination::Folder(location) => Ok(location),
        _ => Err(format!("{text}: expected <internal|sd>/<image|video>")),
    }
}

/// Parses a stored file: `<internal|sd>/<image|video>/<name>`.
fn parse_path(text: &str) -> Result<RemotePath, String> {
    RemotePath::parse(text).map_err(|e| e.to_string())
}

/// Parses the boot media: a stored file or `default`.
fn parse_boot(text: &str) -> Result<BootMedia, String> {
    if text == "default" {
        return Ok(BootMedia::Default);
    }
    parse_path(text).map(BootMedia::File)
}

/// How `put` draws its progress on stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressStyle {
    /// One line redrawn in place (a terminal).
    Bar,
    /// A plain line when a phase starts and every tenth of it (logs, pipes).
    Lines,
}

/// What `bezel storage` works with besides the screen, built by the
/// composition root.
pub struct StorageKit<'a> {
    /// Inspects and converts local media files (`put`).
    pub media: &'a mut dyn MediaTranscoder,
    /// Cancels a running upload (the Ctrl+C handler holds a clone).
    pub cancel: &'a CancelToken,
    /// How upload progress is drawn.
    pub progress: ProgressStyle,
    /// Confirmation summaries, warnings and progress (stderr).
    pub log: &'a mut dyn Write,
}

/// Runs a `bezel storage` command and returns what should be printed on
/// stdout; summaries, warnings and progress go to `kit.log`. A summary that
/// cannot be written there stops the command before the screen changes.
pub fn run<B, C>(
    args: &StorageArgs,
    bus: &B,
    connector: &C,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let open = |target: &Target| connect(bus, connector, target);
    match &args.action {
        StorageAction::Info { target, json } => info(open(target)?.as_mut(), *json),
        StorageAction::Ls {
            target,
            folder,
            json,
        } => ls(open(target)?.as_mut(), *folder, *json),
        StorageAction::Put(put_args) => put(open(&put_args.target)?.as_mut(), put_args, kit),
        StorageAction::Rm { target, paths, yes } => {
            let mut log = Messages::new(&mut *kit.log);
            if !yes {
                return refuse_delete(paths, &mut log);
            }
            rm(open(target)?.as_mut(), paths, &mut log)
        }
        StorageAction::Play { target, path, once } => {
            let repeat = if *once { Repeat::Once } else { Repeat::Loop };
            play(open(target)?.as_mut(), path, repeat)
        }
        StorageAction::Stop { target } => {
            let mut link = open(target)?;
            usecase::stop(link.as_mut()).map_err(screen_error("stopping playback"))?;
            Ok(format!("{}: stopped\n", link.identity().model.name))
        }
        StorageAction::Boot(boot_args) => {
            let mut log = Messages::new(&mut *kit.log);
            write!(log, "{}", boot_summary(boot_args));
            if !boot_args.yes {
                writeln!(log, "{NOTHING_SENT} Add --yes to change the boot media.");
                log.check()?;
                anyhow::bail!("changing the boot media needs --yes");
            }
            log.check()?;
            boot(open(&boot_args.target)?.as_mut(), boot_args)
        }
    }
}

const NOTHING_SENT: &str = "Nothing was sent to the screen.";

fn connect<B, C>(bus: &B, connector: &C, target: &Target) -> anyhow::Result<Box<dyn ScreenLink>>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    open_screen(bus, connector, target.screen.as_deref()).context("could not open the screen")
}

/// A size as people read it: bytes, then KiB, MiB, GiB with one decimal.
fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn optional_size(size: Option<u64>) -> String {
    size.map_or_else(|| "size unknown".to_string(), size_text)
}

const fn medium_name(medium: Medium) -> &'static str {
    match medium {
        Medium::Internal => "internal flash",
        Medium::Card => "memory card",
    }
}

const BAR_WIDTH: usize = 24;

/// `[#######-----]` for a fraction in `0.0..=1.0`.
fn bar(fraction: f64) -> String {
    let filled = ((fraction.clamp(0.0, 1.0) * BAR_WIDTH as f64).round() as usize).min(BAR_WIDTH);
    format!("[{}{}]", "#".repeat(filled), "-".repeat(BAR_WIDTH - filled))
}

fn percent(fraction: f64) -> u64 {
    (fraction.clamp(0.0, 1.0) * 100.0).floor() as u64
}

// ---------------------------------------------------------------- info, ls

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CapacityDto {
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
}

impl From<Capacity> for CapacityDto {
    fn from(c: Capacity) -> Self {
        Self {
            total_bytes: c.total,
            used_bytes: c.used,
            free_bytes: c.free,
        }
    }
}

/// JSON shape of `bezel storage info`. Field names are a public contract.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoDto {
    screen: &'static str,
    internal: CapacityDto,
    card: Option<CapacityDto>,
}

/// JSON shape of one file of `bezel storage ls`. Field names are a public
/// contract.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileDto {
    path: String,
    medium: &'static str,
    folder: &'static str,
    name: String,
    size_bytes: Option<u64>,
}

impl From<&FileEntry> for FileDto {
    fn from(e: &FileEntry) -> Self {
        Self {
            path: e.path.to_string(),
            medium: e.path.location.medium.slug(),
            folder: e.path.location.kind.slug(),
            name: e.path.name.to_string(),
            size_bytes: e.size,
        }
    }
}

fn capacity_line(out: &mut String, label: &str, capacity: Option<Capacity>) {
    let Some(c) = capacity else {
        out.push_str(&format!("{label:<9} no memory card\n"));
        return;
    };
    let used = if c.total == 0 {
        0.0
    } else {
        c.used as f64 / c.total as f64
    };
    out.push_str(&format!(
        "{label:<9} {} {:>3}%  {} used of {}, {} free\n",
        bar(used),
        percent(used),
        size_text(c.used),
        size_text(c.total),
        size_text(c.free)
    ));
}

/// `bezel storage info`.
fn info(link: &mut dyn ScreenLink, json: bool) -> anyhow::Result<String> {
    let report: StorageInfo = usecase::info(link).map_err(screen_error("reading its storage"))?;
    let screen = link.identity().model.name;
    if json {
        let dto = InfoDto {
            screen,
            internal: report.internal.into(),
            card: report.card.map(Into::into),
        };
        return Ok(serde_json::to_string_pretty(&dto)? + "\n");
    }
    let mut out = format!("{screen}\n");
    capacity_line(&mut out, "internal", Some(report.internal));
    capacity_line(&mut out, "sd", report.card);
    Ok(out)
}

const LISTING: &str = "listing its files";

/// `bezel storage ls`: one folder, or every folder of the media present.
fn ls(
    link: &mut dyn ScreenLink,
    folder: Option<StorageLocation>,
    json: bool,
) -> anyhow::Result<String> {
    let (folders, card) = match folder {
        Some(folder) => (vec![folder], true),
        None => {
            let report = usecase::info(link).map_err(screen_error(LISTING))?;
            let card = report.card.is_some();
            let present = StorageLocation::ALL.into_iter();
            (
                present
                    .filter(|l| card || l.medium == Medium::Internal)
                    .collect(),
                card,
            )
        }
    };
    let mut entries = Vec::new();
    for folder in &folders {
        entries.extend(usecase::list(link, *folder).map_err(screen_error(LISTING))?);
    }
    if json {
        let dtos: Vec<FileDto> = entries.iter().map(FileDto::from).collect();
        return Ok(serde_json::to_string_pretty(&dtos)? + "\n");
    }
    Ok(listing(&folders, &entries, card))
}

fn listing(folders: &[StorageLocation], entries: &[FileEntry], card: bool) -> String {
    let no_card = if card { "" } else { " (no memory card)" };
    if entries.is_empty() {
        let names: Vec<String> = folders.iter().map(ToString::to_string).collect();
        return format!("No files in {}{no_card}.\n", names.join(", "));
    }
    let width = entries
        .iter()
        .map(|e| e.path.to_string().len())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for e in entries {
        let size = e.size.map_or_else(|| "?".to_string(), size_text);
        out.push_str(&format!("{:<width$}  {size:>10}\n", e.path.to_string()));
    }
    let total: u64 = entries.iter().filter_map(|e| e.size).sum();
    let files = if entries.len() == 1 { "file" } else { "files" };
    out.push_str(&format!(
        "{} {files}, {}{no_card}\n",
        entries.len(),
        size_text(total)
    ));
    out
}

// ---------------------------------------------------------------- put

/// Draws the progress of an upload job on stderr.
struct ProgressView<'a, 'b> {
    style: ProgressStyle,
    out: &'a mut Messages<'b>,
    /// The phase and step last drawn (percent for a bar, tenths for lines).
    shown: Option<(JobPhase, u64)>,
    /// Length of the bar line on screen; 0 when no line is open.
    open: usize,
}

impl<'a, 'b> ProgressView<'a, 'b> {
    fn new(style: ProgressStyle, out: &'a mut Messages<'b>) -> Self {
        Self {
            style,
            out,
            shown: None,
            open: 0,
        }
    }

    /// Percent done (seconds of video when the length is unknown); lines
    /// only change every ten of them.
    fn step(&self, progress: Progress) -> u64 {
        let step = progress.fraction().map_or(progress.done / 1000, percent);
        match self.style {
            ProgressStyle::Bar => step,
            ProgressStyle::Lines => step / 10,
        }
    }

    fn report(&mut self, progress: Progress) {
        let step = self.step(progress);
        if self.shown == Some((progress.phase, step)) {
            return;
        }
        let new_phase = self.shown.map(|(phase, _)| phase) != Some(progress.phase);
        self.shown = Some((progress.phase, step));
        let line = progress_line(progress);
        match self.style {
            ProgressStyle::Lines => writeln!(self.out, "{line}"),
            ProgressStyle::Bar => {
                if new_phase {
                    self.finish();
                }
                let width = self.open.max(line.len());
                write!(self.out, "\r{line:<width$}");
                self.out.flush();
                self.open = line.len();
            }
        }
    }

    /// Ends the line a bar is drawn on.
    fn finish(&mut self) {
        if self.open > 0 {
            writeln!(self.out);
            self.open = 0;
        }
    }
}

fn seconds(ms: u64) -> String {
    format!("{:.1} s", ms as f64 / 1000.0)
}

fn progress_line(p: Progress) -> String {
    let detail = match p.phase {
        JobPhase::Convert if p.total > 0 => {
            format!("{} of {} of video", seconds(p.done), seconds(p.total))
        }
        JobPhase::Convert => format!("{} of video converted", seconds(p.done)),
        JobPhase::Upload => format!("{} / {}", size_text(p.done), size_text(p.total)),
        JobPhase::Verify if p.done >= p.total => "stored size checked".to_string(),
        JobPhase::Verify => "checking the stored size".to_string(),
    };
    let label = p.phase.slug();
    match p.fraction() {
        Some(f) => format!("{label:<7} {} {:>3}%  {detail}", bar(f), percent(f)),
        None => format!("{label:<7} {detail}"),
    }
}

/// The screen's upload profile, or `Unsupported`.
fn profile_of(model: &DeviceModel) -> anyhow::Result<UploadProfile> {
    UploadProfile::for_model(model).ok_or_else(|| {
        screen_error(UPLOADING)(BezelError::Unsupported(format!(
            "{} stores no media",
            model.name
        )))
    })
}

/// The conversion that fits a video to the panel at `fps`: turned for the
/// way the screen stands and cropped to the panel's shape, never stretched
/// (the core's `fitting_options`). The way it stands is `orientation`, else
/// the video's own shape; a video already at the panel's native size, with
/// no orientation asked for, is left as it is.
fn convert_options(
    model: &DeviceModel,
    profile: &UploadProfile,
    media: &MediaInfo,
    orientation: Option<OrientationArg>,
    fps: Option<u32>,
) -> ConvertOptions {
    let unchanged = ConvertOptions {
        frame_rate: fps,
        ..ConvertOptions::default()
    };
    if media.kind() != Some(MediaKind::Video) {
        return ConvertOptions::default();
    }
    let orientation = match (orientation, media.dimensions) {
        (Some(o), _) => Orientation::from(o),
        (None, Some(size)) if size == profile.video_size => return unchanged,
        (None, Some(size)) if size.width > size.height => Orientation::Landscape,
        (None, _) => Orientation::Portrait,
    };
    ConvertOptions {
        frame_rate: fps,
        ..fitting_options(model, orientation, media)
    }
}

/// The upload `put` asks for: the destination, a name made from the file's
/// when none is given, and the conversion options.
fn upload_request(
    link: &dyn ScreenLink,
    args: &PutArgs,
    source: MediaLocation,
    media: &MediaInfo,
) -> anyhow::Result<UploadRequest> {
    let model = link.identity().model;
    let profile = profile_of(model)?;
    let unknown = || {
        anyhow!(
            "{} is not a picture (JPEG, PNG, BMP, GIF) or a video the screen can store",
            args.file.display()
        )
    };
    let in_folder_of_kind = |medium| {
        let kind = media.kind().ok_or_else(unknown)?;
        anyhow::Ok(StorageLocation::new(medium, kind))
    };
    let (location, name) = match &args.dest {
        None => (in_folder_of_kind(Medium::Internal)?, None),
        Some(Destination::Medium(medium)) => (in_folder_of_kind(*medium)?, None),
        Some(Destination::Folder(location)) => (*location, None),
        Some(Destination::File(location, name)) => (*location, Some(name.clone())),
    };
    let name = match name {
        Some(name) => name,
        None => {
            let host = args.file.file_name().unwrap_or_default().to_string_lossy();
            let suggested =
                usecase::suggest_name(link, &host, media).map_err(screen_error(UPLOADING))?;
            suggested.ok_or_else(unknown)?.to_string()
        }
    };
    Ok(UploadRequest {
        source,
        name,
        location,
        options: convert_options(model, &profile, media, args.orientation, args.fps),
    })
}

fn conversion_text(target: &TranscodeTarget) -> String {
    let mut out = format!(
        "to {}x{} {} (H.264, no audio) with ffmpeg",
        target.size.width, target.size.height, target.format
    );
    if !target.quarter_turns.is_multiple_of(4) {
        out.push_str(&format!(
            ", turned {}°",
            u32::from(target.quarter_turns % 4) * 90
        ));
    }
    if let Some(crop) = target.crop {
        // The crop applies after the turn; the user thinks of the clip as it is.
        let (width, height) = if target.quarter_turns % 2 == 1 {
            (crop.height, crop.width)
        } else {
            (crop.width, crop.height)
        };
        out.push_str(&format!(
            ", keeping the middle {width}x{height} of the clip (the panel's shape)"
        ));
    }
    if let Some(fps) = target.frame_rate {
        out.push_str(&format!(", {fps} fps"));
    }
    out
}

/// What `put` is about to do: the file, where it goes, the conversion and
/// the file it replaces.
fn put_summary(file: &Path, screen: &str, prepared: &PreparedUpload) -> String {
    let media = &prepared.media;
    let plan = &prepared.plan;
    let shape = media
        .dimensions
        .map(|d| format!(" {}x{}", d.width, d.height))
        .unwrap_or_default();
    let mut out = format!(
        "Upload {} ({}, {}{shape})\n",
        file.display(),
        size_text(media.bytes),
        media.format
    );
    let medium = medium_name(plan.path.location.medium);
    out.push_str(&format!(
        "  to       {} on {screen} ({medium})\n",
        plan.path
    ));
    match &plan.action {
        UploadAction::Convert(target) => {
            out.push_str(&format!("  convert  {}\n", conversion_text(target)));
        }
        UploadAction::AsIs { .. } if media.kind() == Some(MediaKind::Video) => {
            out.push_str(
                "  as is    already in the screen's format; it plays in the panel's native \
                 orientation (--orientation turns it)\n",
            );
        }
        UploadAction::AsIs { .. } => {}
    }
    if let Some(old) = &plan.replaces {
        out.push_str(&format!(
            "  replaces {} ({})\n",
            old.path,
            optional_size(old.size)
        ));
    }
    out
}

/// A refused preflight as the user should read it.
fn explain(error: BezelError) -> anyhow::Error {
    match error {
        BezelError::Refused(Refusal::NoSpace {
            needed,
            free,
            candidates,
        }) => {
            let mut text = format!(
                "refused: {} does not fit in the {} free; nothing was deleted",
                size_text(needed),
                size_text(free)
            );
            if !candidates.is_empty() {
                text.push_str(".\nStored on that medium, largest first:");
                for c in &candidates {
                    text.push_str(&format!("\n  {}  {}", c.path, optional_size(c.size)));
                }
                text.push_str(
                    "\nDelete what you no longer need with `bezel storage rm <PATH> --yes`, \
                     then try again",
                );
            }
            anyhow!(text)
        }
        BezelError::Refused(Refusal::TooLarge { bytes, limit }) => anyhow!(
            "refused: the file is {} and a screen takes files up to {} ({limit} bytes)",
            size_text(bytes),
            size_text(limit)
        ),
        BezelError::Refused(refusal @ Refusal::NeedsConverter(_)) => {
            anyhow!("refused: {refusal}; install ffmpeg (see above) or pass --ffmpeg PATH")
        }
        other => other.into(),
    }
}

/// A use-case error as the user reads it. What this screen cannot do is a
/// plain "does not support" line (screens without storage; TUR_USB answers
/// `Unsupported` for delete, boot and playing once,
/// D-2026-09-30-storage-video-7; files whose size it cannot report are
/// listed and played as present); refusals are explained.
fn screen_error(what: &'static str) -> impl Fn(BezelError) -> anyhow::Error {
    move |error| match error {
        BezelError::Unsupported(reason) => {
            anyhow!("this screen does not support {what} ({reason})")
        }
        other => explain(other),
    }
}

/// The error of an interrupted upload, with what to do about a partial file.
fn cancelled(path: &RemotePath, partial: Option<u64>) -> anyhow::Error {
    match partial {
        Some(bytes) => anyhow!(
            "cancelled; an incomplete file of {} remains at {path}: delete it with \
             `bezel storage rm {path} --yes`",
            size_text(bytes)
        ),
        None => anyhow!("cancelled; nothing was stored"),
    }
}

const UPLOADING: &str = "storing files";

/// Probes the file, runs the preflight (queries only) and writes the
/// summary of what `put` is about to do.
fn prepare_put(
    link: &mut dyn ScreenLink,
    args: &PutArgs,
    media: &mut dyn MediaTranscoder,
    log: &mut Messages<'_>,
) -> anyhow::Result<PreparedUpload> {
    let source = MediaLocation(args.file.to_string_lossy().into_owned());
    let probed = media
        .probe(&source)
        .with_context(|| format!("cannot read {}", args.file.display()))?;
    if probed.kind() == Some(MediaKind::Video)
        && let MediaTools::Missing { install_hints } = media.tools()
    {
        writeln!(
            log,
            "warning: ffmpeg was not found, so only a video already in the screen's format \
             can be sent. Install it with: {} (or pass --ffmpeg PATH)",
            install_hints.join(" ; ")
        );
    }
    let request = upload_request(link, args, source, &probed)?;
    let prepared =
        usecase::prepare_upload(link, media, &request).map_err(screen_error(UPLOADING))?;
    let screen = link.identity().model.name;
    write!(log, "{}", put_summary(&args.file, screen, &prepared));
    Ok(prepared)
}

/// `bezel storage put`.
fn put(
    link: &mut dyn ScreenLink,
    args: &PutArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let mut log = Messages::new(&mut *kit.log);
    let prepared = prepare_put(link, args, kit.media, &mut log)?;
    let path = &prepared.plan.path;
    if prepared.plan.replaces.is_some() && !args.yes {
        writeln!(log, "{NOTHING_SENT} Add --yes to replace the stored file.");
        log.check()?;
        anyhow::bail!("replacing {path} needs --yes");
    }
    // Nothing is sent unless the summary reached the user.
    log.check()?;
    let confirm = if args.yes { Confirm::Yes } else { Confirm::No };
    let mut view = ProgressView::new(kit.progress, &mut log);
    let result = {
        let mut sink = |p: Progress| view.report(p);
        let mut job = Job::new(kit.cancel, &mut sink);
        usecase::upload(link, kit.media, &prepared, confirm, &mut job)
    };
    view.finish();
    let uploaded = match result {
        Ok(uploaded) => uploaded,
        Err(BezelError::Cancelled { partial }) => return Err(cancelled(path, partial)),
        Err(e) => return Err(screen_error(UPLOADING)(e)),
    };
    let screen = link.identity().model.name;
    // The upload is not stopped for its progress bar; a line that could not
    // be drawn still ends the command with that error, naming what was stored.
    log.check()
        .with_context(|| format!("{screen} stored {}", uploaded.path))?;
    let converted = if uploaded.converted {
        ", converted"
    } else {
        ""
    };
    Ok(format!(
        "{screen}: stored {} ({}{converted})\n",
        uploaded.path,
        size_text(uploaded.bytes)
    ))
}

// ---------------------------------------------------------------- rm, play, boot

fn joined(paths: &[RemotePath]) -> String {
    let names: Vec<String> = paths.iter().map(ToString::to_string).collect();
    names.join(", ")
}

/// `bezel storage rm` without `--yes`: says what would go and refuses
/// without opening the screen.
fn refuse_delete(paths: &[RemotePath], log: &mut Messages<'_>) -> anyhow::Result<String> {
    for path in paths {
        let medium = medium_name(path.location.medium);
        writeln!(log, "Delete {path} from the screen's {medium}");
    }
    let them = if paths.len() == 1 { "it" } else { "them" };
    writeln!(log, "{NOTHING_SENT} Add --yes to delete {them}.");
    log.check()?;
    anyhow::bail!("deleting {} needs --yes", joined(paths))
}

const DELETING: &str = "deleting files";

/// `bezel storage rm --yes`: lists what goes (with sizes), then deletes it
/// once that list reached the user.
fn rm(
    link: &mut dyn ScreenLink,
    paths: &[RemotePath],
    log: &mut Messages<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let mut stored = Vec::new();
    for path in paths {
        let listed = usecase::list(link, path.location).map_err(screen_error(DELETING))?;
        match listed.into_iter().find(|e| &e.path == path) {
            Some(entry) => {
                let medium = medium_name(path.location.medium);
                let size = optional_size(entry.size);
                writeln!(log, "Delete {path} ({size}) from {screen} ({medium})");
                stored.push(entry);
            }
            None => {
                writeln!(log, "{path} is not stored on {screen}; nothing to delete");
            }
        }
    }
    log.check()?;
    let mut out = String::new();
    for entry in &stored {
        usecase::delete(link, &entry.path, Confirm::Yes).map_err(screen_error(DELETING))?;
        out.push_str(&format!(
            "deleted {} ({})\n",
            entry.path,
            optional_size(entry.size)
        ));
    }
    if stored.is_empty() {
        out.push_str("nothing deleted\n");
    }
    Ok(out)
}

/// `bezel storage play`.
fn play(link: &mut dyn ScreenLink, path: &RemotePath, repeat: Repeat) -> anyhow::Result<String> {
    let what = match repeat {
        Repeat::Once => "playing a video once",
        Repeat::Loop => "playing stored files",
    };
    usecase::play(link, path, repeat).map_err(screen_error(what))?;
    let screen = link.identity().model.name;
    let how = match (path.location.kind, repeat) {
        (MediaKind::Image, _) => "showing",
        (MediaKind::Video, Repeat::Loop) => "looping",
        (MediaKind::Video, Repeat::Once) => "playing once",
    };
    Ok(format!(
        "{screen}: {how} {path} (`bezel storage stop` stops it)\n"
    ))
}

/// What `boot` is about to do, printed with or without `--yes`: the file,
/// and what the screen keeps with it (OPTIONS: the brightness it boots
/// with, and its own sleep timer, which Bezel leaves off).
fn boot_summary(args: &BootArgs) -> String {
    let mut out = match &args.media {
        BootMedia::Default => "Boot media: the screen's built-in start screen\n".to_string(),
        BootMedia::File(path) => {
            let what = match path.location.kind {
                MediaKind::Image => "picture",
                MediaKind::Video => "video, looping",
            };
            format!(
                "Boot media: {path} ({what}), shown by the screen on its own after power-up; \
                 it starts playing now\n"
            )
        }
    };
    let brightness = match args.brightness {
        Some(level) => format!("{level}% (--brightness)"),
        None => "the vendor default, about 67% (170 of 255; --brightness chooses)".to_string(),
    };
    out.push_str(&format!(
        "  The screen keeps this choice with the brightness it boots with: {brightness}\n  \
         and with its sleep timer off: it does not go to sleep on its own\n"
    ));
    out
}

/// `bezel storage boot --yes`.
fn boot(link: &mut dyn ScreenLink, args: &BootArgs) -> anyhow::Result<String> {
    let brightness = args
        .brightness
        .map(|level| Brightness::new(level).context("brightness is 0-100"))
        .transpose()?;
    usecase::set_boot_media(link, &args.media, brightness, Confirm::Yes)
        .map_err(screen_error("changing the boot media"))?;
    let screen = link.identity().model.name;
    let what = match &args.media {
        BootMedia::Default => "its built-in start screen".to_string(),
        BootMedia::File(path) => path.to_string(),
    };
    Ok(format!("{screen}: boots with {what}\n"))
}

#[cfg(test)]
pub(crate) mod doubles {
    //! A media transcoder that keeps its files in memory, for the CLI's
    //! tests (the real one runs ffmpeg).

    use std::collections::BTreeMap;
    use std::time::Duration;

    use bezel_core::domain::device::{Transport, UsbId};
    use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
    use bezel_core::domain::frame::{Frame, Rgba};
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::job::{Job, JobPhase, Progress};
    use bezel_core::domain::media::{
        FrameRate, MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget, VideoCodec,
        VideoPixelFormat, VideoTrack,
    };
    use bezel_core::ports::{MediaLocation, MediaTranscoder, VideoFrames};
    use bezel_core::{BezelError, Result};
    use bezel_devices::FakeBus;

    /// A bus with a WeAct 0.96" (80x160): no storage, no device playback.
    pub(crate) fn weact_bus() -> FakeBus {
        FakeBus::new(vec![Endpoint {
            address: DeviceAddress("/dev/ttyACM0".into()),
            transport: Transport::Serial,
            usb: UsbId::new(0x1a86, 0xfe0c),
            serial_number: Some("AD0001".into()),
            manufacturer: None,
            product: None,
            location: None,
        }])
    }

    /// The colour of every picture a stub stream decodes.
    pub(crate) const STREAMED: Rgba = Rgba::opaque(200, 10, 10);

    /// An H.264 yuv420p MP4 of `size`, 10 s long.
    pub(crate) fn video(size: Size, bytes: u64, audio: bool) -> MediaInfo {
        MediaInfo {
            format: MediaFormat::Mp4,
            bytes,
            dimensions: Some(size),
            video: Some(VideoTrack {
                codec: VideoCodec::H264,
                pixel_format: Some(VideoPixelFormat::Yuv420p),
                b_frames: Some(false),
                frame_rate: FrameRate::new(24, 1),
                duration: Some(Duration::from_secs(10)),
            }),
            has_audio: audio,
        }
    }

    /// A still picture.
    pub(crate) fn picture(format: MediaFormat, bytes: u64) -> MediaInfo {
        MediaInfo {
            format,
            bytes,
            dimensions: Some(Size::new(64, 64)),
            video: None,
            has_audio: false,
        }
    }

    /// Local files held in memory; conversions produce an MP4 of the target
    /// size, streams a solid [`STREAMED`] picture.
    pub(crate) struct StubMedia {
        pub(crate) tools: MediaTools,
        pub(crate) files: BTreeMap<String, MediaInfo>,
        pub(crate) targets: Vec<TranscodeTarget>,
        pub(crate) streamed: Vec<(MediaLocation, StreamSpec)>,
    }

    impl StubMedia {
        pub(crate) fn ready() -> Self {
            Self::with_tools(MediaTools::Ready {
                version: "stub".into(),
            })
        }

        pub(crate) fn missing() -> Self {
            Self::with_tools(MediaTools::Missing {
                install_hints: vec!["sudo dnf install ffmpeg".into()],
            })
        }

        fn with_tools(tools: MediaTools) -> Self {
            Self {
                tools,
                files: BTreeMap::new(),
                targets: Vec::new(),
                streamed: Vec::new(),
            }
        }

        pub(crate) fn with(mut self, location: &str, info: MediaInfo) -> Self {
            self.files.insert(location.to_string(), info);
            self
        }
    }

    struct Solid(Frame);

    impl VideoFrames for Solid {
        fn frame_at(&mut self, _: Duration) -> Result<&Frame> {
            Ok(&self.0)
        }
    }

    impl MediaTranscoder for StubMedia {
        fn tools(&mut self) -> MediaTools {
            self.tools.clone()
        }

        fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
            self.files
                .get(&source.0)
                .cloned()
                .ok_or_else(|| BezelError::InvalidInput(format!("no file {}", source.0)))
        }

        fn transcode(
            &mut self,
            source: &MediaLocation,
            target: &TranscodeTarget,
            job: &mut Job<'_>,
        ) -> Result<MediaLocation> {
            self.targets.push(*target);
            job.report(Progress::new(JobPhase::Convert, 0, 10_000));
            job.checkpoint()?;
            job.report(Progress::new(JobPhase::Convert, 10_000, 10_000));
            let output = format!("{}.converted.mp4", source.0);
            self.files
                .insert(output.clone(), video(target.size, 300_000, false));
            Ok(MediaLocation(output))
        }

        fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
            let info = self.probe(source)?;
            Ok(vec![0x42; usize::try_from(info.bytes).unwrap_or(0)])
        }

        fn stream(
            &mut self,
            source: &MediaLocation,
            spec: StreamSpec,
        ) -> Result<Box<dyn VideoFrames>> {
            self.streamed.push((source.clone(), spec));
            Ok(Box::new(Solid(Frame::filled(spec.size, STREAMED))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::doubles::{StubMedia, picture, video, weact_bus};
    use super::*;
    use crate::messages::tests::Closing;
    use crate::{Cli, Command};
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::media::MediaFormat;
    use bezel_core::domain::storage::StartMode;
    use bezel_devices::fake::{FakeStorage, Playback, StorageCall};
    use bezel_devices::{FakeBus, FakeConnector};
    use clap::Parser;

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).unwrap()
    }

    fn with_files(files: &[(&str, usize)]) -> FakeConnector {
        let mut storage = FakeStorage::default();
        for (p, bytes) in files {
            storage = storage.with_file(path(p), vec![7; *bytes]);
        }
        FakeConnector::with_storage(storage)
    }

    /// Runs `bezel storage …` on the simulated 8.8": stdout (or the error)
    /// and what went to stderr.
    fn storage_with(
        args: &[&str],
        connector: &FakeConnector,
        media: &mut StubMedia,
        log: &mut dyn Write,
    ) -> anyhow::Result<String> {
        let cli = Cli::try_parse_from(args)?;
        let Command::Storage(storage) = &cli.command else {
            anyhow::bail!("not a storage command")
        };
        let cancel = CancelToken::new();
        let mut kit = StorageKit {
            media,
            cancel: &cancel,
            progress: ProgressStyle::Lines,
            log,
        };
        run(storage, &FakeBus::turing_88(), connector, &mut kit)
    }

    fn storage(
        args: &[&str],
        connector: &FakeConnector,
        media: &mut StubMedia,
    ) -> (anyhow::Result<String>, String) {
        let mut log = Vec::new();
        let out = storage_with(args, connector, media, &mut log);
        (out, String::from_utf8(log).unwrap())
    }

    #[test]
    fn rm_without_yes_is_refused() {
        let connector = with_files(&[("internal/video/intro.mp4", 3000)]);
        let (out, log) = storage(
            &["bezel", "storage", "rm", "internal/video/intro.mp4"],
            &connector,
            &mut StubMedia::ready(),
        );
        let err = out.unwrap_err().to_string();
        assert!(err.contains("needs --yes"), "{err}");
        assert!(
            log.contains("Delete internal/video/intro.mp4 from the screen's internal flash"),
            "{log}"
        );
        assert!(log.contains("Nothing was sent to the screen"), "{log}");
        let screen = connector.log().storage;
        assert!(screen.calls.is_empty(), "the screen was not even opened");
        assert!(screen.files.contains_key(&path("internal/video/intro.mp4")));
    }

    #[test]
    fn rm_with_yes_summarizes_then_deletes() {
        let connector = with_files(&[
            ("internal/video/intro.mp4", 3000),
            ("internal/image/a.png", 10),
        ]);
        let (out, log) = storage(
            &[
                "bezel",
                "storage",
                "rm",
                "internal/video/intro.mp4",
                "internal/video/gone.mp4",
                "--yes",
            ],
            &connector,
            &mut StubMedia::ready(),
        );
        assert_eq!(out.unwrap(), "deleted internal/video/intro.mp4 (2.9 KiB)\n");
        assert!(
            log.contains(
                "Delete internal/video/intro.mp4 (2.9 KiB) from Turing Smart Screen 8.8\" \
                 (internal flash)"
            ),
            "{log}"
        );
        assert!(
            log.contains("internal/video/gone.mp4 is not stored on Turing Smart Screen 8.8\""),
            "{log}"
        );
        let screen = connector.log().storage;
        assert_eq!(screen.files.len(), 1);
        let changes: Vec<_> = screen
            .calls
            .iter()
            .filter(|c| c.changes_the_screen())
            .collect();
        assert_eq!(
            changes,
            [&StorageCall::Delete(path("internal/video/intro.mp4"))]
        );

        let (out, _) = storage(
            &["bezel", "storage", "rm", "internal/image/nope.png", "--yes"],
            &connector,
            &mut StubMedia::ready(),
        );
        assert_eq!(out.unwrap(), "nothing deleted\n");
        // A card file without a card: refused, nothing deleted.
        let (out, _) = storage(
            &[
                "bezel",
                "storage",
                "rm",
                "sd/image/a.png",
                "internal/image/a.png",
                "--yes",
            ],
            &connector,
            &mut StubMedia::ready(),
        );
        assert!(out.unwrap_err().to_string().contains("no memory card"));
        assert_eq!(connector.log().storage.files.len(), 1);
    }

    #[test]
    fn info_and_ls_as_text_and_json() {
        let storage_with_card = FakeStorage::default()
            .with_card(8 << 30)
            .with_file(path("internal/video/intro.mp4"), vec![1; 3 << 20])
            .with_file(path("sd/image/logo.png"), vec![1; 512]);
        let connector = FakeConnector::with_storage(storage_with_card);
        let mut media = StubMedia::ready();
        let (out, _) = storage(&["bezel", "storage", "info"], &connector, &mut media);
        let out = out.unwrap();
        assert!(out.starts_with("Turing Smart Screen 8.8\"\n"), "{out}");
        assert!(out.contains("internal  ["), "{out}");
        assert!(out.contains("3.0 MiB used of 1.0 GiB"), "{out}");
        assert!(out.contains("512 B used of 8.0 GiB"), "{out}");

        let (out, _) = storage(
            &["bezel", "storage", "info", "--json"],
            &connector,
            &mut media,
        );
        let json: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(json["internal"]["usedBytes"], 3 << 20);
        assert_eq!(json["card"]["totalBytes"], 8_u64 << 30);

        let (out, _) = storage(&["bezel", "storage", "ls"], &connector, &mut media);
        let out = out.unwrap();
        assert!(
            out.contains("internal/video/intro.mp4     3.0 MiB"),
            "{out}"
        );
        assert!(out.contains("sd/image/logo.png"), "{out}");
        assert!(out.ends_with("2 files, 3.0 MiB\n"), "{out}");

        let (out, _) = storage(
            &["bezel", "storage", "ls", "sd/image", "--json"],
            &connector,
            &mut media,
        );
        let json: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
        assert_eq!(json[0]["path"], "sd/image/logo.png");
        assert_eq!(json[0]["medium"], "sd");
        assert_eq!(json[0]["folder"], "image");
        assert_eq!(json[0]["sizeBytes"], 512);

        // Without a card: only the internal folders, and the card says so.
        let bare = FakeConnector::default();
        let (out, _) = storage(&["bezel", "storage", "ls"], &bare, &mut media);
        assert_eq!(
            out.unwrap(),
            "No files in internal/image, internal/video (no memory card).\n"
        );
        let (out, _) = storage(&["bezel", "storage", "info"], &bare, &mut media);
        assert!(out.unwrap().contains("sd        no memory card"));
        let (out, _) = storage(&["bezel", "storage", "ls", "sd/video"], &bare, &mut media);
        assert!(out.unwrap_err().to_string().contains("no memory card"));
        assert!(Cli::try_parse_from(["bezel", "storage", "ls", "internal"]).is_err());
    }

    #[test]
    fn put_sends_a_picture_with_progress_and_a_summary() {
        let connector = FakeConnector::default();
        let mut media =
            StubMedia::ready().with("/pics/My Logo.PNG", picture(MediaFormat::Png, 200_000));
        let (out, log) = storage(
            &["bezel", "storage", "put", "/pics/My Logo.PNG"],
            &connector,
            &mut media,
        );
        assert_eq!(
            out.unwrap(),
            "Turing Smart Screen 8.8\": stored internal/image/my_logo.png (195.3 KiB)\n"
        );
        assert!(
            log.starts_with("Upload /pics/My Logo.PNG (195.3 KiB, PNG 64x64)\n"),
            "{log}"
        );
        assert!(log.contains("  to       internal/image/my_logo.png on Turing Smart Screen 8.8\" (internal flash)"), "{log}");
        assert!(
            log.contains("upload  [------------------------]   0%  0 B / 195.3 KiB"),
            "{log}"
        );
        assert!(
            log.contains("upload  [########################] 100%"),
            "{log}"
        );
        assert!(
            log.contains("verify  [########################] 100%  stored size checked"),
            "{log}"
        );
        assert!(!log.contains("ffmpeg"), "pictures need no converter: {log}");
        let stored = connector.log().storage;
        assert_eq!(
            stored.size(&path("internal/image/my_logo.png")),
            Some(200_000)
        );
    }

    #[test]
    fn put_over_a_stored_file_needs_yes() {
        let stored = FakeStorage::default()
            .with_card(1 << 30)
            .with_file(path("sd/image/logo.png"), vec![7; 100]);
        let connector = FakeConnector::with_storage(stored);
        let mut media = StubMedia::ready().with("logo.png", picture(MediaFormat::Png, 5000));
        let mut args = vec!["bezel", "storage", "put", "logo.png", "sd/image/logo.png"];
        let (out, log) = storage(&args, &connector, &mut media);
        let err = out.unwrap_err().to_string();
        assert!(
            err.contains("replacing sd/image/logo.png needs --yes"),
            "{err}"
        );
        assert!(
            log.contains("  replaces sd/image/logo.png (100 B)"),
            "{log}"
        );
        assert!(log.contains("Add --yes to replace"), "{log}");
        let screen = connector.log().storage;
        assert!(
            !screen.calls.iter().any(StorageCall::changes_the_screen),
            "{:?}",
            screen.calls
        );

        args.push("--yes");
        let (out, _) = storage(&args, &connector, &mut media);
        assert!(out.unwrap().contains("stored sd/image/logo.png (4.9 KiB)"));
        assert_eq!(
            connector.log().storage.size(&path("sd/image/logo.png")),
            Some(5000)
        );
    }

    #[test]
    fn put_converts_a_video_of_another_shape_by_cropping_it() {
        let connector = FakeConnector::default();
        let clip = video(Size::new(1920, 1080), 9_000_000, true);
        let mut media = StubMedia::ready().with("clip.mov", clip);
        let (out, log) = storage(
            &[
                "bezel",
                "storage",
                "put",
                "clip.mov",
                "internal/video",
                "--fps",
                "24",
            ],
            &connector,
            &mut media,
        );
        assert!(
            out.unwrap()
                .contains("stored internal/video/clip.mp4 (293.0 KiB, converted)")
        );
        // Landscape on a reverse-portrait panel: one quarter turn, then the
        // middle of the 1080x1920 picture with the panel's 1:4 shape.
        let target = media.targets[0];
        assert_eq!(target.size, Size::new(480, 1920));
        assert_eq!(target.quarter_turns, 1);
        assert_eq!(
            target.crop,
            Some(Rect {
                x: 300,
                y: 0,
                width: 480,
                height: 1920
            })
        );
        assert_eq!(target.frame_rate, Some(24));
        assert!(log.contains("  convert  to 480x1920 MP4 (H.264, no audio) with ffmpeg, turned 90°, keeping the middle 1920x480 of the clip"), "{log}");
        assert!(
            log.contains("convert [########################] 100%  10.0 s of 10.0 s of video"),
            "{log}"
        );
        assert!(!log.contains("warning"), "{log}");

        // A vertical screen: half a turn, and a clip already of the panel's
        // size is sent as it is unless an orientation is asked for.
        let native = video(Size::new(480, 1920), 4000, false);
        let mut media = StubMedia::ready().with("tall.mp4", native);
        let (out, log) = storage(
            &["bezel", "storage", "put", "tall.mp4"],
            &connector,
            &mut media,
        );
        assert!(
            out.unwrap()
                .contains("stored internal/video/tall.mp4 (3.9 KiB)\n")
        );
        assert!(
            log.contains("  as is    already in the screen's format"),
            "{log}"
        );
        assert!(media.targets.is_empty());
        let (out, _) = storage(
            &[
                "bezel",
                "storage",
                "put",
                "tall.mp4",
                "internal/video/tall_180.mp4",
                "--orientation",
                "vertical",
            ],
            &connector,
            &mut media,
        );
        out.unwrap();
        assert_eq!(media.targets[0].quarter_turns, 2);
        assert_eq!(media.targets[0].crop, None);
    }

    #[test]
    fn without_ffmpeg_only_videos_in_the_screen_format_go() {
        let connector = FakeConnector::default();
        let mut media = StubMedia::missing()
            .with("ready.mp4", video(Size::new(480, 1920), 4000, false))
            .with("raw.mp4", video(Size::new(1920, 1080), 4000, true));
        let (out, log) = storage(
            &["bezel", "storage", "put", "ready.mp4"],
            &connector,
            &mut media,
        );
        out.unwrap();
        assert!(log.contains("warning: ffmpeg was not found"), "{log}");
        assert!(log.contains("sudo dnf install ffmpeg"), "{log}");

        let (out, _) = storage(
            &["bezel", "storage", "put", "raw.mp4"],
            &connector,
            &mut media,
        );
        let err = out.unwrap_err().to_string();
        assert!(err.contains("must be converted"), "{err}");
        assert!(err.contains("--ffmpeg PATH"), "{err}");
        let stored = connector.log().storage;
        assert_eq!(stored.files.len(), 1, "only the ready video");
    }

    #[test]
    fn a_full_screen_lists_what_could_be_deleted() {
        let mut full = FakeStorage::default()
            .with_file(path("internal/video/big.mp4"), vec![1; 6000])
            .with_file(path("internal/image/small.png"), vec![1; 1000]);
        full.internal_total = 8000;
        let connector = FakeConnector::with_storage(full);
        let mut media = StubMedia::ready().with("new.png", picture(MediaFormat::Png, 2000));
        let (out, _) = storage(
            &["bezel", "storage", "put", "new.png"],
            &connector,
            &mut media,
        );
        let err = out.unwrap_err().to_string();
        assert!(
            err.contains("2.0 KiB does not fit in the 1000 B free"),
            "{err}"
        );
        let big = err.find("internal/video/big.mp4  5.9 KiB").unwrap();
        let small = err.find("internal/image/small.png  1000 B").unwrap();
        assert!(big < small, "largest first: {err}");
        assert!(err.contains("bezel storage rm <PATH> --yes"), "{err}");
        assert_eq!(connector.log().storage.files.len(), 2, "nothing deleted");

        let mut media = StubMedia::ready().with("huge.png", picture(MediaFormat::Png, 130_000_000));
        let (out, _) = storage(
            &["bezel", "storage", "put", "huge.png"],
            &connector,
            &mut media,
        );
        assert!(
            out.unwrap_err()
                .to_string()
                .contains("files up to 114.4 MiB")
        );
        let mut media = StubMedia::ready().with(
            "notes.txt",
            MediaInfo {
                format: MediaFormat::Other,
                bytes: 10,
                dimensions: None,
                video: None,
                has_audio: false,
            },
        );
        let (out, _) = storage(
            &["bezel", "storage", "put", "notes.txt"],
            &connector,
            &mut media,
        );
        assert!(out.unwrap_err().to_string().contains("is not a picture"));
        let (out, _) = storage(
            &["bezel", "storage", "put", "missing.png"],
            &connector,
            &mut media,
        );
        assert!(format!("{:#}", out.unwrap_err()).contains("cannot read missing.png"));
    }

    /// Stderr that cancels the upload once a line shows the first 64 KiB
    /// arrived (what Ctrl+C does from the handler's thread).
    struct CancelOnWrite {
        token: CancelToken,
        seen: Vec<u8>,
    }

    impl Write for CancelOnWrite {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.seen.extend_from_slice(buf);
            if String::from_utf8_lossy(&self.seen).contains("64.0 KiB / 256.0 KiB") {
                self.token.cancel();
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_cancelled_upload_says_how_to_delete_what_is_left() {
        let connector = FakeConnector::default();
        let mut media = StubMedia::ready().with("big.png", picture(MediaFormat::Png, 256 * 1024));
        let cancel = CancelToken::new();
        let mut log = CancelOnWrite {
            token: cancel.clone(),
            seen: Vec::new(),
        };
        let cli = Cli::try_parse_from(["bezel", "storage", "put", "big.png"]).unwrap();
        let Command::Storage(args) = &cli.command else {
            unreachable!("parsed as storage")
        };
        let mut kit = StorageKit {
            media: &mut media,
            cancel: &cancel,
            progress: ProgressStyle::Bar,
            log: &mut log,
        };
        let err = run(args, &FakeBus::turing_88(), &connector, &mut kit)
            .unwrap_err()
            .to_string();
        assert_eq!(
            err,
            "cancelled; an incomplete file of 64.0 KiB remains at internal/image/big.png: \
             delete it with `bezel storage rm internal/image/big.png --yes`"
        );
        let drawn = String::from_utf8(log.seen).unwrap();
        assert!(
            drawn.contains("\rupload  ["),
            "a bar redrawn in place: {drawn:?}"
        );
        assert!(drawn.ends_with('\n'), "the bar line is ended: {drawn:?}");
        assert_eq!(
            cancelled(&path("internal/image/x.png"), None).to_string(),
            "cancelled; nothing was stored"
        );
    }

    #[test]
    fn play_stop_and_boot() {
        let connector = with_files(&[
            ("internal/video/intro.mp4", 3000),
            ("internal/image/logo.png", 30),
        ]);
        let mut media = StubMedia::ready();
        let (out, _) = storage(
            &["bezel", "storage", "play", "internal/video/intro.mp4"],
            &connector,
            &mut media,
        );
        assert!(out.unwrap().contains("looping internal/video/intro.mp4"));
        let (out, _) = storage(
            &[
                "bezel",
                "storage",
                "play",
                "internal/image/logo.png",
                "--once",
            ],
            &connector,
            &mut media,
        );
        assert!(out.unwrap().contains("showing internal/image/logo.png"));
        let (out, _) = storage(
            &[
                "bezel",
                "storage",
                "play",
                "internal/video/none.mp4",
                "--once",
            ],
            &connector,
            &mut media,
        );
        assert!(out.unwrap_err().to_string().contains("not stored"));
        let (out, _) = storage(&["bezel", "storage", "stop"], &connector, &mut media);
        assert_eq!(out.unwrap(), "Turing Smart Screen 8.8\": stopped\n");
        assert_eq!(connector.log().storage.playback, Playback::Idle);

        let calls_before = connector.log().storage.calls.len();
        let (out, log) = storage(
            &["bezel", "storage", "boot", "internal/video/intro.mp4"],
            &connector,
            &mut media,
        );
        assert!(out.unwrap_err().to_string().contains("needs --yes"));
        assert!(
            log.contains("Boot media: internal/video/intro.mp4 (video, looping)"),
            "{log}"
        );
        assert!(log.contains("the vendor default, about 67%"), "{log}");
        assert!(
            log.contains("with its sleep timer off: it does not go to sleep on its own"),
            "{log}"
        );
        assert_eq!(
            connector.log().storage.calls.len(),
            calls_before,
            "the screen was not opened"
        );

        let (out, log) = storage(
            &[
                "bezel",
                "storage",
                "boot",
                "internal/video/intro.mp4",
                "--brightness",
                "40",
                "--yes",
            ],
            &connector,
            &mut media,
        );
        assert_eq!(
            out.unwrap(),
            "Turing Smart Screen 8.8\": boots with internal/video/intro.mp4\n"
        );
        assert!(log.contains("boots with: 40% (--brightness)"), "{log}");
        let screen = connector.log();
        assert_eq!(screen.brightness, [Brightness::new(40).unwrap()]);
        assert_eq!(screen.storage.start_mode, Some(StartMode::Video));
        assert_eq!(
            screen.storage.playback,
            Playback::Video(path("internal/video/intro.mp4"), Repeat::Loop)
        );

        let (out, log) = storage(
            &["bezel", "storage", "boot", "default", "--yes"],
            &connector,
            &mut media,
        );
        assert!(
            out.unwrap()
                .ends_with("boots with its built-in start screen\n")
        );
        assert!(
            log.starts_with("Boot media: the screen's built-in start screen"),
            "{log}"
        );
        assert_eq!(connector.log().storage.start_mode, Some(StartMode::Default));
    }

    #[test]
    fn what_a_screen_cannot_do_is_said_plainly() {
        let connector = FakeConnector::default();
        let mut link = open_screen(&weact_bus(), &connector, None).unwrap();
        let err = info(link.as_mut(), false).unwrap_err().to_string();
        assert_eq!(
            err,
            "this screen does not support reading its storage \
             (WeAct Studio Display FS 0.96\" has no storage)"
        );
        let err = play(link.as_mut(), &path("internal/video/a.mp4"), Repeat::Once)
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("this screen does not support playing a video once"),
            "{err}"
        );
        let unsupported = BezelError::Unsupported("no size query for this file".into());
        assert_eq!(
            screen_error(DELETING)(unsupported).to_string(),
            "this screen does not support deleting files (no size query for this file)"
        );
        let timeout = BezelError::Timeout("the screen; reconnect it".into());
        assert_eq!(
            screen_error(UPLOADING)(timeout).to_string(),
            "timeout talking to the screen; reconnect it",
            "other errors are printed as they are"
        );
    }

    #[test]
    fn destinations_and_paths_parse() {
        assert_eq!(
            parse_destination("sd"),
            Ok(Destination::Medium(Medium::Card))
        );
        assert_eq!(
            parse_destination("internal/video/"),
            Ok(Destination::Folder(StorageLocation::new(
                Medium::Internal,
                MediaKind::Video
            )))
        );
        assert_eq!(
            parse_destination("internal/image/Logo.png"),
            Ok(Destination::File(
                StorageLocation::new(Medium::Internal, MediaKind::Image),
                "Logo.png".into()
            ))
        );
        assert!(parse_destination("usb").is_err());
        assert!(parse_destination("sd/music").is_err());
        assert!(parse_path("internal/video").is_err());
        assert_eq!(parse_boot("default"), Ok(BootMedia::Default));
        assert!(
            Cli::try_parse_from(["bezel", "storage", "rm"]).is_err(),
            "a path is required"
        );
        let cli = Cli::try_parse_from(["bezel", "storage", "put", "a.mp4", "--ffmpeg", "/opt/ff"])
            .unwrap();
        let Command::Storage(args) = &cli.command else {
            unreachable!("parsed as storage")
        };
        assert_eq!(args.ffmpeg(), Some(Path::new("/opt/ff")));
        assert!(args.cancellable());
        let cli = Cli::try_parse_from(["bezel", "storage", "stop"]).unwrap();
        let Command::Storage(args) = &cli.command else {
            unreachable!("parsed as storage")
        };
        assert_eq!(args.ffmpeg(), None);
        assert!(!args.cancellable());
    }

    #[test]
    fn sizes_and_progress_read_well() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(1023), "1023 B");
        assert_eq!(size_text(1536), "1.5 KiB");
        assert_eq!(size_text(120_000_000), "114.4 MiB");
        assert_eq!(size_text(1 << 30), "1.0 GiB");
        assert_eq!(
            progress_line(Progress::new(JobPhase::Convert, 2500, 0)),
            "convert 2.5 s of video converted"
        );
        assert_eq!(
            progress_line(Progress::new(JobPhase::Verify, 0, 1)),
            "verify  [------------------------]   0%  checking the stored size"
        );
        let mut out = Vec::new();
        let mut log = Messages::new(&mut out);
        let mut view = ProgressView::new(ProgressStyle::Lines, &mut log);
        for done in [0, 10, 50, 90, 95, 100] {
            view.report(Progress::new(JobPhase::Upload, done, 100));
        }
        view.finish();
        log.check().unwrap();
        let lines = String::from_utf8(out).unwrap();
        assert_eq!(
            lines.lines().count(),
            5,
            "0%, 10%, 50%, 90% and 100%: {lines}"
        );
    }

    #[test]
    fn a_summary_that_cannot_be_written_stops_before_the_screen_changes() {
        let clip = "internal/video/intro.mp4";
        let connector = with_files(&[(clip, 3000)]);
        let mut media = StubMedia::ready().with("new.png", picture(MediaFormat::Png, 100));
        let stderr_error = "could not write to the terminal (stderr): broken pipe";
        for args in [
            vec!["bezel", "storage", "rm", clip],
            vec!["bezel", "storage", "rm", clip, "--yes"],
            vec!["bezel", "storage", "boot", clip],
            vec!["bezel", "storage", "boot", clip, "--yes"],
            vec!["bezel", "storage", "put", "new.png"],
        ] {
            let mut closed = Closing::after(0);
            let out = storage_with(&args, &connector, &mut media, &mut closed);
            let err = format!("{:#}", out.unwrap_err());
            assert_eq!(err, stderr_error, "{args:?}");
        }
        let screen = connector.log().storage;
        let changed: Vec<&StorageCall> = screen
            .calls
            .iter()
            .filter(|c| c.changes_the_screen())
            .collect();
        assert!(changed.is_empty(), "{changed:?}");
        assert!(screen.files.contains_key(&path(clip)));
    }

    #[test]
    fn a_progress_line_that_cannot_be_written_ends_the_finished_upload_with_its_error() {
        let connector = FakeConnector::default();
        let mut media = StubMedia::ready().with("new.png", picture(MediaFormat::Png, 100));
        // Room for the summary, not for the progress lines.
        let mut closing = Closing::after(200);
        let out = storage_with(
            &["bezel", "storage", "put", "new.png"],
            &connector,
            &mut media,
            &mut closing,
        );
        let err = format!("{:#}", out.unwrap_err());
        assert_eq!(
            err,
            "Turing Smart Screen 8.8\" stored internal/image/new.png: \
             could not write to the terminal (stderr): broken pipe"
        );
        let taken = String::from_utf8(closing.taken).unwrap();
        assert!(taken.starts_with("Upload new.png"), "{taken}");
        let stored = path("internal/image/new.png");
        assert_eq!(connector.log().storage.size(&stored), Some(100));
    }
}
