//! `bezel storage cleanup` (D-2026-09-30-storage-manager-9, -12): the
//! cleanup assistant's findings for what the screen stores (queries only),
//! pre-checked ones first. `--dry-run` only lists; `--yes` deletes exactly
//! the pre-checked files it printed, one at a time; without either it stops
//! before changing the screen.

use anyhow::anyhow;
use bezel_core::app::manager::DeleteReport;
use bezel_core::domain::cleanup::{Finding, HANG_PARTIAL_BYTES, Reason};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{FileEntry, RemotePath};
use bezel_core::ports::ScreenLink;
use clap::Args;

use super::{
    DELETING, NOTHING_CHANGED, StorageKit, files, halt_text, joined, manager, optional_size,
    screen_error, size_text,
};
use crate::Target;
use crate::messages::Messages;

/// Options of `bezel storage cleanup`.
#[derive(Debug, Args)]
pub struct CleanupArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// Only list the suggestions: nothing is deleted, not even what is
    /// pre-checked.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
    /// Delete the pre-checked files it lists (and nothing else).
    #[arg(long)]
    pub yes: bool,
}

/// Why a file is suggested, as the user reads it: its stable code, then
/// the reason.
fn reason_text(reason: &Reason) -> String {
    let code = reason.code().slug();
    let why = match reason {
        Reason::Duplicate { kept } => format!("a vendor copy of {kept}, same size"),
        Reason::HangPartial => format!(
            "exactly {HANG_PARTIAL_BYTES} bytes, what an upload that hung the screen leaves"
        ),
        Reason::Pending => {
            "an upload by Bezel that did not finish or failed its size check".to_string()
        }
        Reason::Variant { kept } => format!("a vendor copy of {kept} with another size"),
        Reason::SizeDiffers { cataloged } => {
            format!("Bezel stored {} here", size_text(*cataloged))
        }
        Reason::SameSize { kept } => format!("exactly the size of {kept}"),
        Reason::Unused => "no theme plays it".to_string(),
    };
    format!("{code}: {why}")
}

/// The findings, pre-checked ones first, with the bytes deleting them
/// frees.
fn suggestions(screen: &str, findings: &[Finding]) -> String {
    if findings.is_empty() {
        return format!("No cleanup suggestions for {screen}.\n");
    }
    let width = findings
        .iter()
        .map(|f| f.file.path.to_string().len())
        .max()
        .unwrap_or(0);
    let line = |f: &Finding| {
        let size = optional_size(f.file.size);
        let path = f.file.path.to_string();
        format!("  {path:<width$}  {size:>10}  {}\n", reason_text(&f.reason))
    };
    let (checked, listed): (Vec<&Finding>, Vec<&Finding>) =
        findings.iter().partition(|f| f.prechecked());
    let mut out = format!(
        "Cleanup suggestions for {screen} (never the boot media Bezel set nor a video your \
         themes play):\n"
    );
    if !checked.is_empty() {
        out.push_str("Pre-checked, deleted by `bezel storage cleanup --yes`:\n");
        out.extend(checked.iter().map(|f| line(f)));
    }
    if !listed.is_empty() {
        out.push_str(
            "Only listed (`bezel storage rm PATH --yes` deletes one you no longer need):\n",
        );
        out.extend(listed.iter().map(|f| line(f)));
    }
    let freed: u64 = checked.iter().filter_map(|f| f.file.size).sum();
    out.push_str(&format!(
        "{} pre-checked ({} to free), {} only listed.\n",
        files(checked.len()),
        size_text(freed),
        files(listed.len())
    ));
    out
}

/// What the confirmed deletes did: every file deleted on stdout; when one
/// stopped them, the error says which, why, and what was not touched.
fn deleted_text(report: &DeleteReport) -> anyhow::Result<String> {
    let done: Vec<String> = report
        .deleted
        .iter()
        .map(|f| format!("deleted {} ({})", f.path, optional_size(f.size)))
        .collect();
    let Some(stopped) = &report.stopped else {
        let mut out: String = done.iter().map(|line| format!("{line}\n")).collect();
        out.push_str(&format!("freed {}\n", size_text(report.freed())));
        return Ok(out);
    };
    let mut text = format!(
        "stopped at {}: {}; it was not deleted",
        stopped.file.path,
        halt_text(&stopped.halt, DELETING)
    );
    if !done.is_empty() {
        text.push_str("\n  done before it:");
        for line in &done {
            text.push_str(&format!("\n    {line}"));
        }
    }
    if !report.not_started.is_empty() {
        let paths: Vec<RemotePath> = report
            .not_started
            .iter()
            .map(|f: &FileEntry| f.path.clone())
            .collect();
        text.push_str(&format!("\n  not touched: {}", joined(&paths)));
    }
    Err(anyhow!(text))
}

/// `bezel storage cleanup`.
pub(super) fn run(
    link: &mut dyn ScreenLink,
    args: &CleanupArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let mut manager = manager(link, kit.archive, kit.theme_videos);
    let found = manager
        .cleanup()
        .map_err(screen_error("the cleanup assistant"))?;
    let text = suggestions(screen, &found.findings);
    if args.dry_run {
        return Ok(text + "Dry run: nothing was deleted.\n");
    }
    let checked = found.prechecked();
    let mut log = Messages::new(&mut *kit.log);
    write!(log, "{text}");
    if checked.is_empty() {
        log.check()?;
        return Ok("nothing pre-checked; nothing deleted\n".to_string());
    }
    if !args.yes {
        let them = files(checked.len());
        writeln!(
            log,
            "{NOTHING_CHANGED} Add --yes to delete what is pre-checked ({them})."
        );
        log.check()?;
        anyhow::bail!("deleting {them} needs --yes");
    }
    // Nothing is deleted unless the list reached the user.
    log.check()?;
    let report = manager
        .delete_files(&checked, Confirm::Yes, kit.cancel)
        .map_err(screen_error(DELETING))?;
    deleted_text(&report)
}
