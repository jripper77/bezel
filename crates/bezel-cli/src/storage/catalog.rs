//! `bezel storage catalog` and `bezel storage cache`
//! (D-2026-09-30-storage-manager-5, -6, -10, -12): what Bezel sent to the
//! screen, associating a stored file with its original on this computer,
//! forgetting an entry, and the local copies with their limit. None of them
//! changes the screen; associating, forgetting and clearing need `--yes`.

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use bezel_core::app::manager::{self, Inventory};
use bezel_core::app::storage as usecase;
use bezel_core::domain::archive::{ArchiveEntry, Clear};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{Medium, RemotePath};
use bezel_core::ports::{MediaLocation, ScreenLink};
use chrono::{DateTime, Local};
use clap::{Args, Subcommand};

use super::{
    LISTING, StorageKit, files, manager, optional_size, parse_path, screen_error, size_text,
};
use crate::Target;
use crate::messages::Messages;

/// Options of `bezel storage catalog`: `catalog` alone lists.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct CatalogArgs {
    /// What to do with the catalog (default: `ls`).
    #[command(subcommand)]
    pub action: Option<CatalogAction>,
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
}

impl CatalogArgs {
    /// The screen the command names.
    pub fn target(&self) -> &Target {
        match &self.action {
            Some(CatalogAction::Ls { target } | CatalogAction::Forget { target, .. }) => target,
            Some(CatalogAction::Associate(args)) => &args.target,
            None => &self.target,
        }
    }
}

/// The `bezel storage catalog` subcommands.
#[derive(Debug, Subcommand)]
pub enum CatalogAction {
    /// List what Bezel sent to the screen: state (stored, pending, missing,
    /// deleted, on another card), local copy, when it was sent and from
    /// where.
    Ls {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
    },
    /// Associate a stored file with its original on this computer, a file of
    /// exactly its size and kind (or a folder to look in): the original's
    /// bytes become the file's local copy, so it can be moved, renamed and
    /// restored. Needs --yes.
    Associate(AssociateArgs),
    /// Forget an entry of the catalog, and its local copy unless another
    /// entry has the same bytes. The screen is not changed. Needs --yes.
    Forget {
        /// Screen to use.
        #[command(flatten)]
        target: Target,
        /// The file as the catalog names it (`sd/video/intro.mp4`).
        #[arg(value_name = "PATH", value_parser = parse_path)]
        path: RemotePath,
        /// Really forget it.
        #[arg(long)]
        yes: bool,
    },
}

/// Options of `bezel storage catalog associate`.
#[derive(Debug, Args)]
pub struct AssociateArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// The stored file (`sd/video/NVI.mp4`).
    #[arg(value_name = "PATH", value_parser = parse_path)]
    pub path: RemotePath,
    /// Its original on this computer, or a folder whose files are the
    /// candidates (the likeliest is chosen; the others are listed).
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
    /// Really associate them.
    #[arg(long)]
    pub yes: bool,
}

/// Options of `bezel storage cache`: `cache` alone shows the use.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct CacheArgs {
    /// What to do with the local copies (default: `info`).
    #[command(subcommand)]
    pub action: Option<CacheAction>,
    /// Set the size limit of the copies of files deleted through Bezel
    /// (`2GiB`, `500MiB`, bytes); above it the oldest go. Copies of files
    /// still on a screen, or missing from it, never count.
    #[arg(long, value_name = "SIZE", value_parser = parse_size)]
    pub limit: Option<u64>,
}

/// The `bezel storage cache` subcommands.
#[derive(Debug, Subcommand)]
pub enum CacheAction {
    /// How many local copies there are, their size and the limit.
    Info,
    /// Remove the local copies of files deleted through Bezel (every copy
    /// with --all). Their catalog entries and thumbnails stay, without a
    /// local copy. Needs --yes.
    Clear {
        /// Every local copy, also of files still on a screen.
        #[arg(long)]
        all: bool,
        /// Really remove them.
        #[arg(long)]
        yes: bool,
    },
}

/// Parses a size: bytes, or a number with a unit (`KiB`, `MiB`, `GiB`,
/// `TiB`, or `K`, `M`, `G`, `T` for the same; `KB`, `MB`, `GB`, `TB` are
/// powers of 1000), fractions allowed (`1.5GiB`).
pub fn parse_size(text: &str) -> Result<u64, String> {
    let wrong = || format!("{text}: expected a size such as 2GiB, 500MiB or 1048576");
    let trimmed = text.trim();
    let split = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let value: f64 = number.parse().map_err(|_| wrong())?;
    let factor: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kib" => 1024.0,
        "m" | "mib" => 1024.0 * 1024.0,
        "g" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => return Err(wrong()),
    };
    let bytes = (value * factor).round();
    if !bytes.is_finite() || bytes >= u64::MAX as f64 {
        return Err(wrong());
    }
    Ok(bytes as u64)
}

/// Runs `catalog` on the screen behind `link`.
pub(super) fn run(
    link: &mut dyn ScreenLink,
    args: &CatalogArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    match &args.action {
        None | Some(CatalogAction::Ls { .. }) => ls(link, kit),
        Some(CatalogAction::Associate(associate_args)) => associate(link, associate_args, kit),
        Some(CatalogAction::Forget { path, yes, .. }) => forget(link, path, *yes, kit),
    }
}

/// When a file was sent, in local time (`2026-09-30 21:05`).
fn sent_time(seconds: u64) -> String {
    i64::try_from(seconds)
        .ok()
        .and_then(|s| DateTime::from_timestamp(s, 0))
        .map_or_else(
            || "?".to_string(),
            |t| t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string(),
        )
}

/// An entry's state as `catalog ls` shows it.
fn state_label(inventory: &Inventory, entry: &ArchiveEntry) -> &'static str {
    if inventory.overview.other_card.contains(entry) {
        "other card"
    } else {
        entry.state.slug()
    }
}

/// `bezel storage catalog ls`.
fn ls(link: &mut dyn ScreenLink, kit: &mut StorageKit<'_>) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let inventory = manager(link, kit.archive, kit.theme_videos)
        .inventory()
        .map_err(screen_error(LISTING))?;
    let entries = inventory.record().map_or(&[][..], |r| r.entries.as_slice());
    if entries.is_empty() {
        return Ok(format!("Bezel has not sent anything to {screen} yet.\n"));
    }
    let width = entries
        .iter()
        .map(|e| e.path.to_string().len())
        .max()
        .unwrap_or(0);
    let mut out = format!(
        "Bezel's catalog of {screen} ({}), oldest first:\n",
        inventory.key
    );
    for entry in entries {
        let copy = if inventory.has_copy(entry) {
            "local copy"
        } else {
            "no local copy"
        };
        let line = format!(
            "  {:<width$}  {:>10}  {:<10}  {copy:<13}  {}  {}",
            entry.path.to_string(),
            size_text(entry.size),
            state_label(&inventory, entry),
            sent_time(entry.sent_at),
            entry.source.as_deref().unwrap_or_default()
        );
        out.push_str(line.trim_end());
        out.push('\n');
    }
    let copies = entries.iter().filter(|e| inventory.has_copy(e)).count();
    out.push_str(&format!(
        "{}, {copies} with a local copy (`bezel storage cache` shows their use)\n",
        files(entries.len())
    ));
    if let Some(boot) = inventory.boot() {
        out.push_str(&format!("Boot media set by Bezel: {boot}\n"));
    }
    Ok(out)
}

/// The candidates for an original: the files of a folder (sorted), or the
/// file itself.
fn originals_in(file: &Path) -> anyhow::Result<Vec<MediaLocation>> {
    let location = |p: &Path| MediaLocation(p.to_string_lossy().into_owned());
    if !file.is_dir() {
        anyhow::ensure!(file.exists(), "no file or folder at {}", file.display());
        return Ok(vec![location(file)]);
    }
    let entries =
        std::fs::read_dir(file).with_context(|| format!("cannot list {}", file.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    Ok(paths.iter().map(|p| location(p)).collect())
}

const ASSOCIATING: &str = "associating files";

/// `bezel storage catalog associate`: the likeliest original of exactly the
/// stored file's size and kind, confirmed with `--yes`.
fn associate(
    link: &mut dyn ScreenLink,
    args: &AssociateArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let path = &args.path;
    let listed = usecase::list(link, path.location).map_err(screen_error(ASSOCIATING))?;
    let stored = listed
        .into_iter()
        .find(|e| &e.path == path)
        .ok_or_else(|| anyhow!("{path} is not stored on {screen}"))?;
    let sources = originals_in(&args.file)?;
    let mut manager = manager(link, kit.archive, kit.theme_videos);
    let ranked = manager
        .originals(kit.media, path, &sources)
        .map_err(screen_error(ASSOCIATING))?;
    let Some((best, others)) = ranked.split_first() else {
        anyhow::bail!(
            "no {} of exactly {} ({} bytes) at {}, the size of {path}; nothing was associated",
            path.location.kind.slug(),
            optional_size(stored.size),
            stored.size.unwrap_or_default(),
            args.file.display()
        );
    };
    let mut log = Messages::new(&mut *kit.log);
    writeln!(
        log,
        "Associate {path} ({}) on {screen} with {}:\n  its bytes become the file's local copy, \
         so it gets a thumbnail and can be moved, renamed and restored. The screen is not \
         changed.",
        optional_size(stored.size),
        best.source
    );
    for other in others {
        writeln!(
            log,
            "  another candidate: {} (pass it as FILE to choose it)",
            other.source
        );
    }
    if !args.yes {
        writeln!(log, "Nothing was associated. Add --yes to associate them.");
        log.check()?;
        anyhow::bail!("associating {path} needs --yes");
    }
    log.check()?;
    let original = MediaLocation(best.source.clone());
    let entry = manager
        .associate(kit.media, path, &original, kit.now)
        .map_err(screen_error(ASSOCIATING))?;
    Ok(format!(
        "associated {} with {} ({})\n",
        entry.path,
        best.source,
        size_text(entry.size)
    ))
}

/// `bezel storage catalog forget`: the entry at `path` (the inserted card's
/// first, then the newest), confirmed with `--yes`.
fn forget(
    link: &mut dyn ScreenLink,
    path: &RemotePath,
    yes: bool,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let inventory = manager(link, kit.archive, kit.theme_videos)
        .inventory()
        .map_err(screen_error(LISTING))?;
    let card = inventory.listing.card;
    let entries = inventory.record().map_or(&[][..], |r| r.entries.as_slice());
    let here = |e: &&ArchiveEntry| e.path.location.medium == Medium::Internal || e.card == card;
    let entry = entries
        .iter()
        .filter(|e| e.path == *path)
        .max_by_key(|e| (here(e), e.sent_at))
        .cloned()
        .ok_or_else(|| anyhow!("{path} is not in Bezel's catalog of {screen}"))?;
    let sharing = inventory
        .catalog
        .screens
        .values()
        .flat_map(|r| &r.entries)
        .filter(|e| e.content == entry.content)
        .count();
    let copy = if !inventory.has_copy(&entry) {
        "it has no local copy".to_string()
    } else if sharing > 1 {
        "its local copy stays: another entry has the same bytes".to_string()
    } else {
        format!("its local copy ({}) goes too", size_text(entry.size))
    };
    let mut log = Messages::new(&mut *kit.log);
    writeln!(
        log,
        "Forget {path} ({}, {}) in Bezel's catalog of {screen}; {copy}. The screen is not \
         changed.",
        size_text(entry.size),
        state_label(&inventory, &entry)
    );
    if !yes {
        writeln!(log, "Nothing was forgotten. Add --yes to forget it.");
        log.check()?;
        anyhow::bail!("forgetting {path} needs --yes");
    }
    log.check()?;
    manager::forget(kit.archive, &inventory.key, path, entry.card).context("Bezel's catalog")?;
    Ok(format!("forgot {path}\n"))
}

/// "1 copy", "3 copies".
fn copies(count: usize) -> String {
    if count == 1 {
        "1 copy".to_string()
    } else {
        format!("{count} copies")
    }
}

/// Runs `cache` (no screen involved).
pub(super) fn cache(args: &CacheArgs, kit: &mut StorageKit<'_>) -> anyhow::Result<String> {
    if let Some(limit) = args.limit {
        let cleared =
            manager::set_cache_limit(kit.archive, limit).context("Bezel's local copies")?;
        let removed = if cleared.copies == 0 {
            "nothing removed".to_string()
        } else {
            format!(
                "removed the oldest {} ({})",
                copies(cleared.copies),
                size_text(cleared.bytes)
            )
        };
        return Ok(format!(
            "limit of the local copies of deleted files: {}; {removed}\n",
            size_text(limit)
        ));
    }
    match &args.action {
        None | Some(CacheAction::Info) => info(kit),
        Some(CacheAction::Clear { all, yes }) => clear(kit, *all, *yes),
    }
}

/// `bezel storage cache info`.
fn info(kit: &mut StorageKit<'_>) -> anyhow::Result<String> {
    let info = manager::cache_info(kit.archive).context("Bezel's local copies")?;
    let place = kit.archive_dir.map_or_else(
        || "in memory (--fake)".to_string(),
        |dir| format!("in {}", dir.display()),
    );
    Ok(format!(
        "Bezel's local copies, {place}:\n  {}, {}\n  of files deleted through Bezel: {}, {} \
         of the {} limit (above it the oldest go)\n",
        copies(info.copies),
        size_text(info.bytes),
        copies(info.deleted_copies),
        size_text(info.deleted_bytes),
        size_text(info.limit)
    ))
}

/// `bezel storage cache clear`.
fn clear(kit: &mut StorageKit<'_>, all: bool, yes: bool) -> anyhow::Result<String> {
    let info = manager::cache_info(kit.archive).context("Bezel's local copies")?;
    let (scope, count, bytes, what) = if all {
        (Clear::All, info.copies, info.bytes, "every local copy")
    } else {
        (
            Clear::Deleted,
            info.deleted_copies,
            info.deleted_bytes,
            "the local copies of files deleted through Bezel",
        )
    };
    if count == 0 {
        return Ok("nothing to clear\n".to_string());
    }
    let mut log = Messages::new(&mut *kit.log);
    writeln!(
        log,
        "Clear {what}: {}, {}. Their catalog entries and thumbnails stay, without a local \
         copy: they cannot be moved, renamed or restored until associated again.",
        copies(count),
        size_text(bytes)
    );
    if !yes {
        writeln!(log, "Nothing was removed. Add --yes to clear them.");
        log.check()?;
        anyhow::bail!("clearing the local copies needs --yes");
    }
    log.check()?;
    let cleared =
        manager::clear_cache(kit.archive, scope, Confirm::Yes).context("Bezel's local copies")?;
    Ok(format!(
        "cleared {} ({})\n",
        copies(cleared.copies),
        size_text(cleared.bytes)
    ))
}
