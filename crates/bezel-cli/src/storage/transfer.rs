//! `mv`, `rename` and `restore` (D-2026-09-30-storage-manager-7, -8, -12):
//! a plan made from what the screen lists now (queries only), printed as the
//! exact list (source -> target, sizes, what is deleted), then, with
//! `--yes`, run one file at a time from Bezel's local copies: each copy is
//! sent, its stored size checked, and only then (move, rename) its source
//! deleted. The batch stops at the first failure or Ctrl+C.

use anyhow::anyhow;
use bezel_core::app::manager::{
    Batch, Inventory, Manager, ManagerError, Stage, Stopped, TransferReport,
};
use bezel_core::domain::archive::{
    ArchiveEntry, ScreenRecord, Skip, Step, Transfer, TransferPlan, Warning,
};
use bezel_core::domain::job::CancelToken;
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{Medium, RemotePath};
use bezel_core::ports::ScreenLink;
use clap::Args;

use super::{
    NOTHING_CHANGED, ProgressStyle, ProgressView, StorageKit, files, halt_text, joined, manager,
    manager_error, medium_name, optional_size, parse_path, screen_error, size_text,
};
use crate::Target;
use crate::messages::Messages;

/// Options of `bezel storage mv`.
#[derive(Debug, Args)]
pub struct MoveArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// The files, as `bezel storage ls` names them
    /// (`internal/video/intro.mp4`).
    #[arg(value_name = "PATH", required = true, value_parser = parse_path)]
    pub paths: Vec<RemotePath>,
    /// Where they go: `internal` or `sd`. They keep their folder and get
    /// the name an upload would (`NVI.mp4` becomes `nvi.mp4`).
    #[arg(long, value_name = "internal|sd", value_parser = parse_medium)]
    pub to: Medium,
    /// Replace the files of the same name already there (listed first);
    /// without it they are skipped.
    #[arg(long)]
    pub overwrite: bool,
    /// Really move them.
    #[arg(long)]
    pub yes: bool,
}

/// Options of `bezel storage rename`.
#[derive(Debug, Args)]
pub struct RenameArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// The file (`internal/video/intro.mp4`).
    #[arg(value_name = "PATH", value_parser = parse_path)]
    pub path: RemotePath,
    /// Its new name, with the same extension; it follows the upload rule
    /// (lower case letters, digits, `_`, `.`, `-`).
    #[arg(value_name = "NEW_NAME")]
    pub new_name: String,
    /// Replace a file of the new name already there.
    #[arg(long)]
    pub overwrite: bool,
    /// Really rename it.
    #[arg(long)]
    pub yes: bool,
}

/// Options of `bezel storage restore`.
#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// Where the files go: `internal` or `sd`.
    #[arg(value_name = "internal|sd", value_parser = parse_medium)]
    pub medium: Medium,
    /// The files to send again, by name (`intro.mp4`) or path
    /// (`sd/video/intro.mp4`), also ones deleted through Bezel. Default:
    /// the files Bezel sent there that are missing (and, for `sd`, those of
    /// another card).
    #[arg(value_name = "NAME")]
    pub names: Vec<String>,
    /// Replace the files of the same name and another size already there;
    /// without it they are skipped (the same name and size is skipped as
    /// present).
    #[arg(long)]
    pub overwrite: bool,
    /// Really send them.
    #[arg(long)]
    pub yes: bool,
}

/// Parses `internal` or `sd`.
fn parse_medium(text: &str) -> Result<Medium, String> {
    Medium::from_slug(text).ok_or_else(|| format!("{text}: expected internal or sd"))
}

/// How the messages name what a plan does.
struct Verbs {
    /// `move`.
    base: &'static str,
    /// `moved`.
    past: &'static str,
    /// `moving`.
    ing: &'static str,
    /// What a screen error is about (`moving files`).
    what: &'static str,
}

impl Verbs {
    const fn of(transfer: Transfer) -> Self {
        let (base, past, ing, what) = match transfer {
            Transfer::Move => ("move", "moved", "moving", "moving files"),
            Transfer::Copy => ("copy", "copied", "copying", "copying files"),
            Transfer::Rename => ("rename", "renamed", "renaming", "renaming files"),
            Transfer::Restore => ("restore", "restored", "restoring", "restoring files"),
        };
        Self {
            base,
            past,
            ing,
            what,
        }
    }
}

/// The plan `make` returns for no overwrite; with `overwrite`, made again
/// replacing the files whose names were taken (a name another file of the
/// list takes stays skipped).
fn with_overwrite(
    overwrite: bool,
    mut make: impl FnMut(&[RemotePath]) -> Result<TransferPlan, ManagerError>,
) -> Result<TransferPlan, ManagerError> {
    let plan = make(&[])?;
    let taken: Vec<RemotePath> = plan
        .skipped
        .iter()
        .filter(|s| matches!(s.skip, Skip::Conflict(_)))
        .map(|s| s.target.clone())
        .collect();
    if !overwrite || taken.is_empty() {
        return Ok(plan);
    }
    make(&taken)
}

/// The text of a file a plan leaves out.
fn skip_text(plan: &TransferPlan, skip: &Skip) -> String {
    match skip {
        Skip::Conflict(file) if plan.steps.iter().any(|s| s.target == file.path) => {
            format!("another file of this list goes to {}", file.path)
        }
        Skip::Conflict(file) => format!(
            "{} is there ({}); --overwrite replaces it",
            file.path,
            optional_size(file.size)
        ),
        Skip::NoLocalCopy => "Bezel has no local copy of it (it did not send it, or the copy \
             was cleared); `bezel storage catalog associate` links it to its original on this \
             computer"
            .to_string(),
        Skip::DeleteUnsupported => {
            "this screen cannot delete files through Bezel, so nothing can be moved or renamed \
             on it"
                .to_string()
        }
        Skip::Present => "already there with the same size".to_string(),
    }
}

fn warning_text(warning: &Warning) -> String {
    match warning {
        Warning::BootMedia(path) => format!(
            "{path} is the boot media Bezel set; the screen boots the last file played, so \
             set it again afterwards with `bezel storage boot`"
        ),
        Warning::ThemeVideo(path) => format!(
            "a theme plays {path} by its name; after the rename the theme no longer finds it"
        ),
    }
}

/// The exact list a plan's confirmation shows: `header`, every file as
/// source -> target with its size (and `note`), what is skipped and why, the
/// warnings, then the total and `footer`.
fn summary(
    plan: &TransferPlan,
    header: &str,
    footer: &str,
    note: &dyn Fn(&Step) -> String,
) -> String {
    let mut out = String::new();
    let pairs: Vec<String> = plan
        .steps
        .iter()
        .map(|s| format!("{} -> {}", s.source, s.target))
        .collect();
    let width = pairs.iter().map(String::len).max().unwrap_or(0);
    if !plan.steps.is_empty() {
        out.push_str(header);
        out.push('\n');
    }
    for (step, pair) in plan.steps.iter().zip(&pairs) {
        let size = size_text(step.size);
        out.push_str(&format!("  {pair:<width$}  {size:>10}{}", note(step)));
        if let Some(old) = &step.replaces {
            out.push_str(&format!(
                "  replaces {} ({})",
                old.path,
                optional_size(old.size)
            ));
        }
        out.push('\n');
    }
    if !plan.skipped.is_empty() {
        out.push_str("Skipped:\n");
        for skipped in &plan.skipped {
            out.push_str(&format!(
                "  {} -> {}: {}\n",
                skipped.source,
                skipped.target,
                skip_text(plan, &skipped.skip)
            ));
        }
    }
    for warning in &plan.warnings {
        out.push_str(&format!("Warning: {}\n", warning_text(warning)));
    }
    if !plan.steps.is_empty() {
        let one_by_one = if plan.steps.len() > 1 {
            ", one at a time"
        } else {
            ""
        };
        out.push_str(&format!(
            "{}, {} to send{one_by_one}; {footer}\n",
            files(plan.steps.len()),
            size_text(plan.bytes())
        ));
    }
    out
}

/// What running a plan needs besides the manager.
struct Batching<'r, 'l> {
    log: Messages<'l>,
    cancel: &'r CancelToken,
    progress: ProgressStyle,
    now: u64,
}

/// Writes the summary; without `yes` stops there (the screen was only
/// queried); with it runs the plan once the summary reached the user and
/// reports what became of every file.
fn confirm_and_run(
    manager: &mut Manager<'_>,
    plan: &TransferPlan,
    summary: &str,
    yes: bool,
    run: &mut Batching<'_, '_>,
) -> anyhow::Result<String> {
    let verbs = Verbs::of(plan.transfer);
    write!(run.log, "{summary}");
    let count = plan.steps.len();
    if count == 0 {
        run.log.check()?;
        return Ok(format!("nothing to {}\n", verbs.base));
    }
    if !yes {
        let them = if count == 1 { "it" } else { "them" };
        writeln!(
            run.log,
            "{NOTHING_CHANGED} Add --yes to {} {them}.",
            verbs.base
        );
        run.log.check()?;
        anyhow::bail!("{} {} needs --yes", verbs.ing, files(count));
    }
    run.log.check()?;
    let mut view = ProgressView::new(run.progress, &mut run.log);
    let result = {
        let mut sink = |step| view.report_step(step);
        let mut batch = Batch::new(run.cancel, &mut sink);
        manager.run(plan, Confirm::Yes, run.now, &mut batch)
    };
    view.finish();
    let report = result.map_err(manager_error(verbs.what))?;
    // Progress lines that could not be drawn still end the command with
    // that error, after the batch, saying how far it came.
    run.log.check().map_err(|e| {
        e.context(format!(
            "{} {} of {count}",
            verbs.past,
            files(report.done.len())
        ))
    })?;
    report_text(&report, &verbs)
}

/// How far a stopped step had come.
const fn stage_text(stage: Stage) -> &'static str {
    match stage {
        Stage::Preflight => "before sending",
        Stage::Upload => "while sending",
        Stage::Verify => "while checking the stored size",
        Stage::Delete => "while deleting the source, its copy verified",
        Stage::Catalog => "after deleting the source, while updating Bezel's catalog",
    }
}

/// Where a moved or renamed file stands once its step stopped: both copies
/// (the copy verified, the source not deleted), the target alone (the
/// source deleted, the catalog not updated) or the source alone.
fn whereabouts(stopped: &Stopped) -> String {
    let step = &stopped.step;
    let (source, target) = (&step.source, &step.target);
    match stopped.stage {
        Stage::Delete => format!(
            "its copy at {target} is verified and {source} is still there too: delete it with \
             `bezel storage rm {source} --yes`"
        ),
        Stage::Catalog => format!(
            "its copy at {target} is verified and {source} was deleted, but Bezel's catalog \
             still names it: `bezel storage catalog forget {source} --yes` drops it"
        ),
        Stage::Preflight | Stage::Upload | Stage::Verify => format!("{source} stays where it was"),
    }
}

/// What a batch did: every file done on stdout; when it stopped, the error
/// says where and why, what stays where, what was left on the screen (with
/// the command that deletes it) and what was not started.
fn report_text(report: &TransferReport, verbs: &Verbs) -> anyhow::Result<String> {
    let done: Vec<String> = report
        .done
        .iter()
        .map(|s| {
            let size = size_text(s.size);
            format!("{} {} -> {} ({size})", verbs.past, s.source, s.target)
        })
        .collect();
    let Some(stopped) = &report.stopped else {
        return Ok(done.iter().map(|line| format!("{line}\n")).collect());
    };
    let step = &stopped.step;
    let mut text = format!(
        "stopped at {} -> {} ({}): {}",
        step.source,
        step.target,
        stage_text(stopped.stage),
        halt_text(&stopped.halt, verbs.what)
    );
    if report.transfer.deletes_source() {
        text.push_str(&format!("\n  {}", whereabouts(stopped)));
    }
    if let Some(left) = stopped.leftover() {
        text.push_str(&format!(
            "\n  an incomplete file of {} remains at {}: delete it with `bezel storage rm {} \
             --yes`",
            optional_size(left.size),
            left.path,
            left.path
        ));
    }
    if !done.is_empty() {
        text.push_str("\n  done before it:");
        for line in &done {
            text.push_str(&format!("\n    {line}"));
        }
    }
    if !report.not_started.is_empty() {
        let sources: Vec<RemotePath> = report
            .not_started
            .iter()
            .map(|s| s.source.clone())
            .collect();
        text.push_str(&format!("\n  not started: {}", joined(&sources)));
    }
    Err(anyhow!(text))
}

fn no_note(_: &Step) -> String {
    String::new()
}

/// `bezel storage mv`.
pub(super) fn mv(
    link: &mut dyn ScreenLink,
    args: &MoveArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let mut manager = manager(link, kit.archive, kit.theme_videos);
    let plan = with_overwrite(args.overwrite, |taken| {
        manager.plan_move(&args.paths, args.to, taken)
    })
    .map_err(manager_error("moving files"))?;
    let header = format!(
        "Move {} to the {} of {screen}, each sent from Bezel's local copy:",
        files(plan.steps.len()),
        medium_name(args.to)
    );
    let footer = "each source is deleted only after its copy is verified.";
    let text = summary(&plan, &header, footer, &no_note);
    let mut run = Batching {
        log: Messages::new(&mut *kit.log),
        cancel: kit.cancel,
        progress: kit.progress,
        now: kit.now,
    };
    confirm_and_run(&mut manager, &plan, &text, args.yes, &mut run)
}

/// `bezel storage rename`.
pub(super) fn rename(
    link: &mut dyn ScreenLink,
    args: &RenameArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let mut manager = manager(link, kit.archive, kit.theme_videos);
    let plan = with_overwrite(args.overwrite, |taken| {
        manager.plan_rename(&args.path, &args.new_name, taken)
    })
    .map_err(manager_error("renaming files"))?;
    let header = format!(
        "Rename on the {} of {screen}, sent again from Bezel's local copy:",
        medium_name(args.path.location.medium)
    );
    let footer = "the old name is deleted only after the new file is verified.";
    let text = summary(&plan, &header, footer, &no_note);
    let mut run = Batching {
        log: Messages::new(&mut *kit.log),
        cancel: kit.cancel,
        progress: kit.progress,
        now: kit.now,
    };
    confirm_and_run(&mut manager, &plan, &text, args.yes, &mut run)
}

/// The entries `names` choose (a name, letter case aside, or a full path):
/// for each, the one on `to` first, then the newest.
fn chosen(
    record: Option<&ScreenRecord>,
    names: &[String],
    to: Medium,
) -> anyhow::Result<Vec<ArchiveEntry>> {
    let entries = record.map_or(&[][..], |r| r.entries.as_slice());
    names
        .iter()
        .map(|name| {
            let named = |e: &&ArchiveEntry| {
                if name.contains('/') {
                    e.path.to_string() == *name
                } else {
                    e.path.name.as_str().eq_ignore_ascii_case(name)
                }
            };
            let best = entries
                .iter()
                .filter(named)
                .max_by_key(|e| (e.path.location.medium == to, e.sent_at));
            best.cloned().ok_or_else(|| {
                anyhow!(
                    "{name} is not in Bezel's catalog of this screen; `bezel storage catalog` \
                     lists what Bezel sent it"
                )
            })
        })
        .collect()
}

/// Why a restored file is not on the screen: what its entry says.
fn restore_note(inventory: &Inventory, step: &Step) -> String {
    let same = |e: &&ArchiveEntry| e.path == step.source && e.content == step.content;
    if inventory.overview.other_card.iter().any(|e| same(&e)) {
        return "  (on another card)".to_string();
    }
    let entries = inventory.record().map_or(&[][..], |r| r.entries.as_slice());
    match entries.iter().find(same) {
        Some(entry) => format!("  ({})", entry.state.slug()),
        None => String::new(),
    }
}

/// `bezel storage restore`.
pub(super) fn restore(
    link: &mut dyn ScreenLink,
    args: &RestoreArgs,
    kit: &mut StorageKit<'_>,
) -> anyhow::Result<String> {
    let screen = link.identity().model.name;
    let mut manager = manager(link, kit.archive, kit.theme_videos);
    let inventory = manager
        .inventory()
        .map_err(screen_error("restoring files"))?;
    let selection = if args.names.is_empty() {
        inventory.overview.restorable(args.medium)
    } else {
        chosen(inventory.record(), &args.names, args.medium)?
    };
    let medium = medium_name(args.medium);
    if selection.is_empty() {
        return Ok(format!(
            "nothing to restore: no file Bezel sent to the {medium} is missing from it\n"
        ));
    }
    let plan = with_overwrite(args.overwrite, |taken| {
        manager.plan_restore(&selection, args.medium, taken)
    })
    .map_err(manager_error("restoring files"))?;
    let header = format!(
        "Restore {} to the {medium} of {screen} from Bezel's local copies:",
        files(plan.steps.len())
    );
    let free = inventory.info.capacity(args.medium).map_or(0, |c| c.free);
    let footer = format!("{} free there; nothing is deleted.", size_text(free));
    let note = |step: &Step| restore_note(&inventory, step);
    let text = summary(&plan, &header, &footer, &note);
    let mut run = Batching {
        log: Messages::new(&mut *kit.log),
        cancel: kit.cancel,
        progress: kit.progress,
        now: kit.now,
    };
    confirm_and_run(&mut manager, &plan, &text, args.yes, &mut run)
}
