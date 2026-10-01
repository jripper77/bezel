//! Moving, copying, renaming and restoring (D-2026-09-30-storage-manager-7,
//! -8): a plan made from what the screen lists now, kept for its one
//! confirmation, then run file by file with each file's progress; and the
//! batch delete of a confirmed list (the cleanup assistant, a selection).

use bezel_core::app::manager::{Batch, ManagerError, StepProgress, TransferReport};
use bezel_core::app::storage;
use bezel_core::domain::archive::{ArchiveEntry, EntryState, ScreenKey, TransferPlan};
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::job::{CancelToken, JobPhase, Progress};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{FileEntry, Medium, RemotePath};

use super::{
    DeleteReportDto, Desk, PlanDto, PlanReadyDto, PlanRefusedDto, TransferReportDto, restore_id,
};
use crate::backend::Backend;
use crate::clock::unix_seconds;
use crate::dto::{ProgressDto, ProgressStepDto};
use crate::messages::{ErrorCode, UiError, UiResult};
use crate::storage::{ProgressThrottle, remote};
use crate::studio::Resume;

/// What the user asks a plan for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Move `paths` to the other medium `to`.
    Move {
        /// The files.
        paths: Vec<String>,
        /// `internal` or `sd`.
        to: String,
    },
    /// Copy `paths` to the other medium `to`.
    Copy {
        /// The files.
        paths: Vec<String>,
        /// `internal` or `sd`.
        to: String,
    },
    /// Rename `path` to `new_name` on its medium.
    Rename {
        /// The file.
        path: String,
        /// What the user typed.
        new_name: String,
    },
    /// Restore the entries `ids` (`RestorableDto::id`) onto `to`.
    Restore {
        /// The entries.
        ids: Vec<String>,
        /// `internal` or `sd`.
        to: String,
    },
}

/// `internal` or `sd` as a medium.
fn medium(slug: &str) -> UiResult<Medium> {
    Medium::from_slug(slug)
        .ok_or_else(|| UiError::new(ErrorCode::UnknownMedium).arg("medium", slug))
}

fn remotes(paths: &[String]) -> UiResult<Vec<RemotePath>> {
    paths.iter().map(|p| remote(p)).collect()
}

/// The cataloged entries of the screen behind `desk` that `ids` name (not
/// deleted; unknown ids are left out).
fn chosen_entries(desk: &mut Desk<'_>, ids: &[String]) -> UiResult<Vec<ArchiveEntry>> {
    let key = ScreenKey::new(desk.model().id);
    let catalog = desk.store.load()?;
    let Some(record) = catalog.screen(&key) else {
        return Ok(Vec::new());
    };
    let live = record
        .entries
        .iter()
        .filter(|e| e.state != EntryState::Deleted);
    let named = live.filter(|e| ids.contains(&restore_id(e)));
    Ok(named.cloned().collect())
}

impl Desk<'_> {
    /// The plan `ask` asks for, onto its medium.
    fn plan(
        &mut self,
        ask: &Ask,
        overwrite: &[RemotePath],
    ) -> UiResult<(Medium, Result<TransferPlan, ManagerError>)> {
        Ok(match ask {
            Ask::Move { paths, to } => {
                let (sources, to) = (remotes(paths)?, medium(to)?);
                (to, self.manager().plan_move(&sources, to, overwrite))
            }
            Ask::Copy { paths, to } => {
                let (sources, to) = (remotes(paths)?, medium(to)?);
                (to, self.manager().plan_copy(&sources, to, overwrite))
            }
            Ask::Rename { path, new_name } => {
                let source = remote(path)?;
                let to = source.location.medium;
                (to, self.manager().plan_rename(&source, new_name, overwrite))
            }
            Ask::Restore { ids, to } => {
                let to = medium(to)?;
                let selection = chosen_entries(self, ids)?;
                (to, self.manager().plan_restore(&selection, to, overwrite))
            }
        })
    }
}

/// Reports a run's progress file by file: the core's reports of each step
/// (upload bytes, verify 0 → 1), throttled per file, and the delete of a
/// moved or renamed file's source (0 → 1), which the core does not report:
/// it starts once the copy is verified and is over when the next file
/// starts or the run ends with that file done.
struct Relay<'a> {
    plan: &'a TransferPlan,
    out: &'a mut dyn FnMut(ProgressDto),
    throttle: ProgressThrottle,
    at: Option<usize>,
    deleting: Option<usize>,
}

impl<'a> Relay<'a> {
    fn new(plan: &'a TransferPlan, out: &'a mut dyn FnMut(ProgressDto)) -> Self {
        Self {
            plan,
            out,
            throttle: ProgressThrottle::default(),
            at: None,
            deleting: None,
        }
    }

    fn emit(&mut self, index: usize, progress: Progress, phase: &'static str) {
        let Some(step) = self.plan.steps.get(index) else {
            return;
        };
        (self.out)(ProgressDto {
            phase,
            done: progress.done,
            total: progress.total,
            step: Some(ProgressStepDto {
                index,
                count: self.plan.steps.len(),
                source: step.source.to_string(),
                target: Some(step.target.to_string()),
            }),
        });
    }

    fn deleted(&mut self, index: usize) {
        self.emit(index, Progress::new(JobPhase::Verify, 1, 1), "delete");
    }

    fn report(&mut self, report: StepProgress) {
        if let Some(index) = self.deleting.take_if(|i| *i != report.step) {
            self.deleted(index);
        }
        if self.at != Some(report.step) {
            self.at = Some(report.step);
            self.throttle = ProgressThrottle::default();
        }
        let progress = report.progress;
        if self.throttle.pass(progress) {
            self.emit(report.step, progress, progress.phase.slug());
        }
        let verified = progress.phase == JobPhase::Verify && progress.done == progress.total;
        if verified && self.plan.transfer.deletes_source() {
            let starting = Progress::new(JobPhase::Verify, 0, 1);
            self.emit(report.step, starting, "delete");
            self.deleting = Some(report.step);
        }
    }

    fn finish(&mut self, report: &TransferReport) {
        if let Some(index) = self.deleting.take_if(|i| report.done.len() > *i) {
            self.deleted(index);
        }
    }
}

impl Backend {
    /// The plan `ask` asks for on `screen`, kept for its confirmation, or why
    /// none can be made. Files whose target name is taken are skipped unless
    /// that target is in `overwrite` (the user confirmed replacing it). Only
    /// queries the screen.
    pub fn plan_transfer(
        &self,
        screen: &str,
        ask: &Ask,
        overwrite: &[String],
        time: LocalTime,
    ) -> UiResult<PlanDto> {
        let overwrite = remotes(overwrite)?;
        let planned = self.with_desk(screen, Resume::Frames, time, |desk| {
            let (to, plan) = desk.plan(ask, &overwrite)?;
            let plan = match plan {
                Ok(plan) => plan,
                Err(ManagerError::Refused(refusal)) => return Ok(Err(refusal)),
                Err(ManagerError::Failed(e)) => return Err(e.into()),
            };
            let info = storage::info(&mut *desk.link)?;
            let free = info.capacity(to).map_or(0, |c| c.free);
            Ok(Ok((plan, to, free)))
        })?;
        match planned {
            Ok((plan, to, free)) => {
                let ticket = self.storage.keep_plan(screen, plan.clone());
                Ok(PlanDto::Ready(PlanReadyDto::of(ticket, &plan, to, free)))
            }
            Err(refusal) => Ok(PlanDto::Refused(PlanRefusedDto::from(&refusal))),
        }
    }

    /// Runs the plan `ticket` the user confirmed, one file at a time
    /// (preflight, upload of the copy, size check, then the source's delete
    /// for a move or rename), reporting each file's progress; Cancel is
    /// [`Self::cancel_job`]. `confirm` is the answer to the dialog that
    /// listed every file: with `Confirm::No` the core refuses before it
    /// calls the screen. The report says what was done, what stopped the
    /// run and why, and what never started. `stale` when the plan was run,
    /// replaced, or another job changed the screen since.
    pub fn run_plan(
        &self,
        ticket: u64,
        confirm: Confirm,
        time: LocalTime,
        progress: &mut dyn FnMut(ProgressDto),
    ) -> UiResult<TransferReportDto> {
        let videos = self.theme_videos();
        let _claim = self.storage.claim()?;
        let pending = self.storage.take_plan(ticket)?;
        let token = self.storage.start_job();
        let sent_at = unix_seconds();
        let ran = self.on_screen(&pending.screen, Resume::Video, time, |link| {
            let mut store = self.storage.archive();
            let mut desk = Desk {
                link,
                store: store.as_mut(),
                videos: &videos,
            };
            let mut relay = Relay::new(&pending.plan, progress);
            let report = {
                let mut sink = |step: StepProgress| relay.report(step);
                let mut batch = Batch::new(&token, &mut sink);
                desk.manager()
                    .run(&pending.plan, confirm, sent_at, &mut batch)
            };
            if let Ok(report) = &report {
                relay.finish(report);
            }
            report
        });
        self.storage.end_job();
        match ran? {
            Ok(report) => Ok(TransferReportDto::from(&report)),
            Err(ManagerError::Failed(e)) => Err(e.into()),
            // A restore that no longer fits the medium: nothing was sent.
            Err(ManagerError::Refused(refusal)) => {
                Err(UiError::new(ErrorCode::Refused).arg("detail", refusal))
            }
        }
    }

    /// Deletes exactly the files `paths` the user confirmed, one by one
    /// (`delete` progress counts files), each entry marked deleted. A file
    /// gone or changed since stops the batch, as do a failure and Cancel.
    /// With `Confirm::No` nothing reaches the screen.
    pub fn delete_files(
        &self,
        screen: &str,
        paths: &[String],
        confirm: Confirm,
        time: LocalTime,
        progress: &mut dyn FnMut(ProgressDto),
    ) -> UiResult<DeleteReportDto> {
        let paths = remotes(paths)?;
        let _claim = self.storage.claim()?;
        self.storage.forget_plan();
        let token = self.storage.start_job();
        let deleted = self.on_screen(screen, Resume::Video, time, |link| {
            let mut store = self.storage.archive();
            let mut desk = Desk {
                link,
                store: store.as_mut(),
                videos: &[],
            };
            desk.delete_each(&paths, confirm, &token, progress)
        });
        self.storage.end_job();
        deleted?
    }
}

impl Desk<'_> {
    /// The core's batch delete, one file per call so that each one's
    /// progress is reported.
    fn delete_each(
        &mut self,
        paths: &[RemotePath],
        confirm: Confirm,
        cancel: &CancelToken,
        progress: &mut dyn FnMut(ProgressDto),
    ) -> UiResult<DeleteReportDto> {
        if confirm == Confirm::No {
            // The core refuses before it calls the screen.
            let unsized_files: Vec<FileEntry> = paths.iter().map(size_unknown).collect();
            let report = self
                .manager()
                .delete_files(&unsized_files, confirm, cancel)?;
            let mut dto = DeleteReportDto::default();
            dto.add(&report, &[]);
            return Ok(dto);
        }
        let listing = self.manager().inventory()?.listing;
        let mut dto = DeleteReportDto::default();
        for (index, path) in paths.iter().enumerate() {
            progress(deleting(paths, index, index));
            // As confirmed: the size listed now (none when it is gone: the
            // core then stops the batch).
            let size = listing.file(path).and_then(|f| f.size);
            let file = FileEntry {
                path: path.clone(),
                size,
            };
            let rest = &paths[index + 1..];
            let report = match self.manager().delete_files(&[file], confirm, cancel) {
                Ok(report) => report,
                Err(e) if index == 0 => return Err(e.into()),
                Err(e) => return Ok(dto.failed_at(path, e.into(), rest)),
            };
            if !dto.add(&report, rest) {
                return Ok(dto);
            }
        }
        if let Some(last) = paths.len().checked_sub(1) {
            progress(deleting(paths, last, paths.len()));
        }
        Ok(dto)
    }
}

/// `path` as a file of unknown size.
fn size_unknown(path: &RemotePath) -> FileEntry {
    FileEntry {
        path: path.clone(),
        size: None,
    }
}

/// A batch delete's progress: `done` files of `paths`, at the file `index`.
fn deleting(paths: &[RemotePath], index: usize, done: usize) -> ProgressDto {
    ProgressDto {
        phase: "delete",
        done: done as u64,
        total: paths.len() as u64,
        step: Some(ProgressStepDto {
            index,
            count: paths.len(),
            source: paths[index].to_string(),
            target: None,
        }),
    }
}
