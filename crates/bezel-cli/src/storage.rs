//! `bezel storage`: the files a screen stores (internal flash and memory
//! card), uploads with progress and cancellation, deletes, device-side
//! playback and the boot media, over the core's `app::storage` use cases;
//! and the storage manager over `app::manager`
//! (D-2026-09-30-storage-manager-12): moving, renaming and restoring files
//! from Bezel's local copies, the cleanup assistant, the catalog of what
//! Bezel sent and the cache of local copies. `put`, `rm` and `boot` are
//! recorded in that catalog.
//!
//! Deleting, replacing a stored file and changing the boot media need
//! `--yes` (`Confirm::Yes`, D-2026-09-30-storage-video-1 and -5). Every one
//! of them first prints what it is about to do, `--yes` or not; without it
//! the command fails before anything changes the screen: `rm` and `boot` do
//! not even open it, and `put`, `mv`, `rename`, `restore` and `cleanup` only
//! query it to learn what they would do.

mod catalog;
mod cleanup;
pub mod demo;
mod transfer;

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use bezel_core::BezelError;
use bezel_core::app::manager::{Halt, Inventory, Manager, ManagerError, StepProgress};
use bezel_core::app::open_screen;
use bezel_core::app::storage::{self as usecase, PreparedUpload, UploadRequest};
use bezel_core::domain::archive::{Listed, PlanRefusal};
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::{CancelToken, Job, JobPhase, Progress};
use bezel_core::domain::media::{
    ConvertOptions, MediaInfo, MediaKind, MediaTools, TranscodeTarget, UploadProfile,
    fitting_options,
};
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::storage::{
    BootMedia, Capacity, Medium, Refusal, RemotePath, Repeat, Rounding, StorageInfo,
    StorageLocation, UploadAction, mib_text,
};
use bezel_core::domain::theme::AssetRef;
use bezel_core::ports::{
    ArchiveStore, DeviceBus, MediaLocation, MediaTranscoder, ScreenConnector, ScreenLink,
};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::messages::Messages;
use crate::{OrientationArg, Target};

pub use catalog::{AssociateArgs, CacheAction, CacheArgs, CatalogAction, CatalogArgs, parse_size};
pub use cleanup::CleanupArgs;
pub use transfer::{MoveArgs, RenameArgs, RestoreArgs};

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

    /// True for the commands Ctrl+C cancels cleanly: a running upload, a
    /// batch of moves, renames or restores, the deletes of a cleanup.
    pub fn cancellable(&self) -> bool {
        self.cancel_note().is_some()
    }

    /// What the first Ctrl+C does, said on stderr when it is pressed; `None`
    /// for the commands that are not cancelled that way.
    pub fn cancel_note(&self) -> Option<&'static str> {
        match &self.action {
            StorageAction::Put(_) => Some("cancelling the upload..."),
            StorageAction::Mv(_) | StorageAction::Rename(_) | StorageAction::Restore(_) => Some(
                "cancelling: the file being sent stops at its next block and its source stays...",
            ),
            StorageAction::Cleanup(args) if !args.dry_run => {
                Some("cancelling: nothing after the file being deleted is deleted...")
            }
            _ => None,
        }
    }

    /// True for the commands that read or write Bezel's catalog of local
    /// copies (all but `info`, `play` and `stop`).
    pub fn uses_catalog(&self) -> bool {
        !matches!(
            self.action,
            StorageAction::Info { .. } | StorageAction::Play { .. } | StorageAction::Stop { .. }
        )
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
    /// internal/video, sd/image and sd/video. Files Bezel sent show their
    /// catalog state (stored, or pending: an upload that did not finish) and
    /// whether Bezel keeps a local copy of them.
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
    /// Move files to the other medium (internal flash or memory card): each
    /// one is sent again from Bezel's local copy, its stored size checked,
    /// and only then deleted from where it was. Prints the list first; needs
    /// --yes. Ctrl+C cancels: the file being sent stays at its source.
    Mv(MoveArgs),
    /// Rename a stored file: sent again under the new name from Bezel's
    /// local copy, checked, and only then the old one deleted. Prints what it
    /// does first; needs --yes.
    Rename(RenameArgs),
    /// Send files Bezel sent before back to a medium from their local
    /// copies (after formatting it, or onto a new card): by default the ones
    /// missing from it. The space is checked before the first byte; nothing
    /// is deleted. Prints the list first; needs --yes.
    Restore(RestoreArgs),
    /// The cleanup assistant: lists likely leftovers (the vendor app's
    /// duplicate copies, interrupted uploads, files no theme plays) and
    /// deletes only the pre-checked ones it printed, with --yes. Never
    /// suggests the boot media Bezel set nor a video your themes play.
    Cleanup(CleanupArgs),
    /// Bezel's catalog of what it sent to this screen: list it, associate a
    /// stored file with its original on this computer, or forget an entry.
    Catalog(CatalogArgs),
    /// The local copies Bezel keeps of what it sends: their use, clearing
    /// them, and the size limit of the copies of deleted files.
    Cache(CacheArgs),
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
    /// Inspects and converts local media files (`put`, `catalog associate`).
    pub media: &'a mut dyn MediaTranscoder,
    /// Cancels a running upload or batch (the Ctrl+C handler holds a clone).
    pub cancel: &'a CancelToken,
    /// How upload progress is drawn.
    pub progress: ProgressStyle,
    /// Confirmation summaries, warnings and progress (stderr).
    pub log: &'a mut dyn Write,
    /// Bezel's catalog and local copies (D-2026-09-30-storage-manager-5).
    pub archive: &'a mut dyn ArchiveStore,
    /// Where the local copies are kept, as `cache` names it; `None`: in
    /// memory (`--fake`).
    pub archive_dir: Option<&'a Path>,
    /// The videos your themes play: the cleanup assistant never suggests
    /// them and renaming one warns.
    pub theme_videos: &'a [AssetRef],
    /// The time recorded for what is sent now, in seconds since the Unix
    /// epoch.
    pub now: u64,
}

/// The storage manager on the screen behind `link`, protecting the videos
/// of `kit`'s themes.
fn manager<'m>(
    link: &'m mut dyn ScreenLink,
    archive: &'m mut dyn ArchiveStore,
    theme_videos: &[AssetRef],
) -> Manager<'m> {
    Manager::new(link, archive).protecting(theme_videos.iter().cloned())
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
        } => ls(open(target)?.as_mut(), kit, *folder, *json),
        StorageAction::Put(put_args) => put(open(&put_args.target)?.as_mut(), put_args, kit),
        StorageAction::Rm { target, paths, yes } => {
            if !yes {
                return refuse_delete(paths, &mut Messages::new(&mut *kit.log));
            }
            rm(open(target)?.as_mut(), paths, kit)
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
            boot(open(&boot_args.target)?.as_mut(), boot_args, kit)
        }
        StorageAction::Mv(mv) => transfer::mv(open(&mv.target)?.as_mut(), mv, kit),
        StorageAction::Rename(r) => transfer::rename(open(&r.target)?.as_mut(), r, kit),
        StorageAction::Restore(r) => transfer::restore(open(&r.target)?.as_mut(), r, kit),
        StorageAction::Cleanup(c) => cleanup::run(open(&c.target)?.as_mut(), c, kit),
        StorageAction::Catalog(c) => catalog::run(open(c.target())?.as_mut(), c, kit),
        StorageAction::Cache(c) => catalog::cache(c, kit),
    }
}

pub(crate) const NOTHING_SENT: &str = "Nothing was sent to the screen.";

/// Said when a command that only queried the screen stops for `--yes`.
const NOTHING_CHANGED: &str = "Nothing on the screen was changed.";

/// "1 file", "3 files".
fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// A manager error as the user reads it: screen errors as [`screen_error`]
/// says them, refused plans with what to do.
fn manager_error(what: &'static str) -> impl Fn(ManagerError) -> anyhow::Error {
    move |error| match error {
        ManagerError::Failed(error) => screen_error(what)(error),
        ManagerError::Refused(refusal) => refused_plan(refusal),
    }
}

/// Why a file stopped a batch, as the user reads it; screen errors as
/// [`screen_error`] says them about `what`.
fn halt_text(halt: &Halt, what: &'static str) -> String {
    match halt {
        Halt::Cancelled { .. } => "cancelled".to_string(),
        Halt::Conflict(file) => format!(
            "{} is there now ({}) and replacing it was not confirmed (--overwrite)",
            file.path,
            optional_size(file.size)
        ),
        Halt::Refused(refusal) => explain(BezelError::Refused(refusal.clone())).to_string(),
        Halt::Failed(error) => screen_error(what)(error.clone()).to_string(),
        other => other.to_string(),
    }
}

/// A plan that could not be made (nothing was sent or deleted).
fn refused_plan(refusal: PlanRefusal) -> anyhow::Error {
    match refusal {
        PlanRefusal::NoSpace { needed, free } => anyhow!(
            "refused: the files need {} and {} is free ({} short); nothing was sent or deleted",
            size_text(needed),
            size_text(free),
            size_text(needed.saturating_add(1).saturating_sub(free))
        ),
        PlanRefusal::Unsendable { path, refusal } => {
            anyhow!("{path}: {}", explain(BezelError::Refused(refusal)))
        }
        other => anyhow!("refused: {other}; nothing was sent or deleted"),
    }
}

fn connect<B, C>(bus: &B, connector: &C, target: &Target) -> anyhow::Result<Box<dyn ScreenLink>>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    open_screen(bus, connector, target.screen.as_deref()).context("could not open the screen")
}

/// A size as people read it: bytes, then KiB, MiB, GiB with one decimal.
pub(crate) fn size_text(bytes: u64) -> String {
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
    /// The catalog state (`stored`, `pending`) of a file Bezel sent; `null`
    /// for any other file.
    state: Option<&'static str>,
    /// Whether Bezel keeps a local copy of it (it can be moved, renamed and
    /// restored).
    local_copy: bool,
}

impl FileDto {
    fn new(listed: &Listed, local_copy: bool) -> Self {
        let path = &listed.file.path;
        Self {
            path: path.to_string(),
            medium: path.location.medium.slug(),
            folder: path.location.kind.slug(),
            name: path.name.to_string(),
            size_bytes: Inventory::size(listed),
            state: listed.entry.as_ref().map(|e| e.state.slug()),
            local_copy,
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

/// `bezel storage ls`: one folder, or every folder of the media present,
/// next to Bezel's catalog (which the listing updates: stored or missing).
fn ls(
    link: &mut dyn ScreenLink,
    kit: &mut StorageKit<'_>,
    folder: Option<StorageLocation>,
    json: bool,
) -> anyhow::Result<String> {
    let inventory = manager(link, kit.archive, kit.theme_videos)
        .inventory()
        .map_err(screen_error(LISTING))?;
    let card = inventory.listing.card.is_some();
    if folder.is_some_and(|f| f.medium == Medium::Card) && !card {
        return Err(screen_error(LISTING)(BezelError::Refused(Refusal::NoCard)));
    }
    let folders: Vec<StorageLocation> = match folder {
        Some(folder) => vec![folder],
        None => StorageLocation::ALL
            .into_iter()
            .filter(|l| card || l.medium == Medium::Internal)
            .collect(),
    };
    let shown = |path: &RemotePath| folders.contains(&path.location);
    let listed: Vec<&Listed> = inventory
        .overview
        .listed
        .iter()
        .filter(|l| shown(&l.file.path))
        .collect();
    let copy = |l: &Listed| l.entry.as_ref().is_some_and(|e| inventory.has_copy(e));
    if json {
        let dtos: Vec<FileDto> = listed.iter().map(|l| FileDto::new(l, copy(l))).collect();
        return Ok(serde_json::to_string_pretty(&dtos)? + "\n");
    }
    let missing: Vec<&RemotePath> = inventory
        .overview
        .missing
        .iter()
        .map(|e| &e.path)
        .filter(|p| shown(p))
        .collect();
    let rows: Vec<Row> = listed
        .iter()
        .map(|l| Row {
            path: l.file.path.to_string(),
            size: Inventory::size(l),
            sent: l.entry.is_some(),
            catalog: catalog_column(l, copy(l)),
        })
        .collect();
    Ok(listing(&folders, &rows, card, &missing))
}

/// What `ls` says of a listed file next to the catalog: its state and
/// whether a local copy is kept; `-` for a file Bezel did not send.
fn catalog_column(listed: &Listed, copy: bool) -> String {
    match &listed.entry {
        None => "-".to_string(),
        Some(entry) => {
            let copy = if copy { "local copy" } else { "no local copy" };
            format!("{:<7}  {copy}", entry.state.slug())
        }
    }
}

/// One line of `ls`.
struct Row {
    path: String,
    size: Option<u64>,
    /// Whether Bezel sent the file (its catalog has it).
    sent: bool,
    catalog: String,
}

fn listing(
    folders: &[StorageLocation],
    rows: &[Row],
    card: bool,
    missing: &[&RemotePath],
) -> String {
    let no_card = if card { "" } else { " (no memory card)" };
    let mut out = String::new();
    if rows.is_empty() {
        let names: Vec<String> = folders.iter().map(ToString::to_string).collect();
        out.push_str(&format!("No files in {}{no_card}.\n", names.join(", ")));
    } else {
        let width = rows.iter().map(|r| r.path.len()).max().unwrap_or(0);
        for row in rows {
            let size = row.size.map_or_else(|| "?".to_string(), size_text);
            out.push_str(&format!(
                "{:<width$}  {size:>10}  {}\n",
                row.path, row.catalog
            ));
        }
        let total: u64 = rows.iter().filter_map(|r| r.size).sum();
        let sent = rows.iter().filter(|r| r.sent).count();
        let sent = if sent == 0 {
            String::new()
        } else {
            format!("; {sent} sent by Bezel")
        };
        out.push_str(&format!(
            "{}, {}{sent}{no_card}\n",
            files(rows.len()),
            size_text(total)
        ));
    }
    if !missing.is_empty() {
        let names: Vec<String> = missing.iter().map(ToString::to_string).collect();
        out.push_str(&format!(
            "Missing from the screen, sent by Bezel: {} (`bezel storage restore` sends them again)\n",
            names.join(", ")
        ));
    }
    out
}

// ---------------------------------------------------------------- put

/// Draws the progress of an upload job, or of a batch of them, on stderr.
struct ProgressView<'a, 'b> {
    style: ProgressStyle,
    out: &'a mut Messages<'b>,
    /// The phase and step last drawn (percent for a bar, tenths for lines).
    shown: Option<(JobPhase, u64)>,
    /// Length of the bar line on screen; 0 when no line is open.
    open: usize,
    /// `[2/5] ` before every line of a batch's second of five files.
    prefix: String,
}

impl<'a, 'b> ProgressView<'a, 'b> {
    fn new(style: ProgressStyle, out: &'a mut Messages<'b>) -> Self {
        Self {
            style,
            out,
            shown: None,
            open: 0,
            prefix: String::new(),
        }
    }

    /// The progress of one file of a batch, its lines numbered.
    fn report_step(&mut self, step: StepProgress) {
        let prefix = format!("[{}/{}] ", step.step + 1, step.steps);
        if prefix != self.prefix {
            self.finish();
            self.prefix = prefix;
            self.shown = None;
        }
        self.report(step.progress);
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
        let line = format!("{}{}", self.prefix, progress_line(progress));
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

/// How a video gets under a screen's per-file limit (rev C: 25 MiB,
/// D-2026-09-30-release-polish-12): `--fps` converts it, with its bitrate
/// capped to fit.
const SMALLER_VIDEO: &str = "For a video: send a shorter clip, or a lower frame rate with --fps \
                             (for example --fps 24)";

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
            "refused: the file is {} MiB and this screen takes files up to {} MiB each; \
             nothing was sent. {SMALLER_VIDEO}",
            mib_text(bytes, Rounding::Up),
            mib_text(limit, Rounding::Down)
        ),
        BezelError::Refused(Refusal::ConvertedTooLarge { bytes, limit }) => anyhow!(
            "refused: converted, the video is {} MiB and this screen takes files up to {} MiB \
             each; nothing was sent. {SMALLER_VIDEO}",
            mib_text(bytes, Rounding::Up),
            mib_text(limit, Rounding::Down)
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
pub(crate) fn screen_error(what: &'static str) -> impl Fn(BezelError) -> anyhow::Error {
    move |error| match error {
        BezelError::Unsupported(reason) => {
            anyhow!("this screen does not support {what} ({reason})")
        }
        other => explain(other),
    }
}

/// The error of an interrupted upload, with what to do about a partial file.
pub(crate) fn cancelled(path: &RemotePath, partial: Option<u64>) -> anyhow::Error {
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
    let screen = link.identity().model.name;
    let mut view = ProgressView::new(kit.progress, &mut log);
    // Recorded in the catalog: the exact bytes sent are kept as the local
    // copy before the first byte, the entry stored once its size is checked.
    let result = {
        let mut sink = |p: Progress| view.report(p);
        let mut job = Job::new(kit.cancel, &mut sink);
        let mut manager = manager(link, kit.archive, kit.theme_videos);
        manager.upload(kit.media, &prepared, confirm, kit.now, &mut job)
    };
    view.finish();
    let uploaded = match result {
        Ok(uploaded) => uploaded,
        Err(BezelError::Cancelled { partial }) => return Err(cancelled(path, partial)),
        Err(e) => return Err(screen_error(UPLOADING)(e)),
    };
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
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let log = &mut Messages::new(&mut *kit.log);
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
    // Recorded: each entry is marked deleted, its copy kept for a restore
    // within the cache limit.
    let mut manager = manager(link, kit.archive, kit.theme_videos);
    for entry in &stored {
        manager
            .delete(&entry.path, Confirm::Yes)
            .map_err(screen_error(DELETING))?;
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
/// with, and its own sleep timer, which stays as the shutdown choice set
/// it: the minutes of a recorded `off`, else none;
/// D-2026-10-03-power-off-standby-2 (4)).
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
         and with the sleep timer of the shutdown choice: it goes to sleep on its own\n  \
         only when `bezel standby` chose off, after the minutes chosen there\n"
    ));
    out
}

/// `bezel storage boot --yes`. The boot media is recorded in the catalog:
/// the cleanup assistant never suggests it and moving it warns.
fn boot(
    link: &mut dyn ScreenLink,
    args: &BootArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let brightness = args
        .brightness
        .map(|level| Brightness::new(level).context("brightness is 0-100"))
        .transpose()?;
    let screen = link.identity().model.name;
    manager(link, kit.archive, kit.theme_videos)
        .set_boot_media(&args.media, brightness, Confirm::Yes)
        .map_err(screen_error("changing the boot media"))?;
    let what = match &args.media {
        BootMedia::Default => "its built-in start screen".to_string(),
        BootMedia::File(path) => path.to_string(),
    };
    Ok(format!("{screen}: boots with {what}\n"))
}

#[cfg(test)]
pub(crate) mod doubles;

#[cfg(test)]
mod tests;
