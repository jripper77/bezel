//! What the manager's batches report: their progress, why one stopped and
//! what became of every file, with stable codes the studio and the CLI
//! translate (D-2026-09-30-storage-manager-7, -8, -9).

use std::fmt;

use thiserror::Error;

use crate::BezelError;
use crate::domain::archive::{PlanRefusal, Skipped, Step, Transfer, TransferPlan};
use crate::domain::job::{CancelToken, Progress};
use crate::domain::storage::{FileEntry, Refusal};

/// Why a manager call made no plan, or ran nothing of one.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManagerError {
    /// The screen, the store or a confirmation failed.
    #[error(transparent)]
    Failed(#[from] BezelError),
    /// No plan could be made, or a restore no longer fits the medium (its
    /// space and the per-file limit are checked again before the first
    /// byte): nothing was sent or deleted.
    #[error("{0}")]
    Refused(PlanRefusal),
}

impl From<PlanRefusal> for ManagerError {
    fn from(refusal: PlanRefusal) -> Self {
        ManagerError::Refused(refusal)
    }
}

/// Where a batch is: the step that runs (counted from 0), how many there
/// are, and that step's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepProgress {
    /// The step, an index into the plan's steps.
    pub step: usize,
    /// Steps in the plan.
    pub steps: usize,
    /// The step's own progress (upload, verify).
    pub progress: Progress,
}

/// What a batch receives from its caller: where to report its progress and
/// the token to poll, between files and inside each upload.
pub struct Batch<'a> {
    pub(super) cancel: &'a CancelToken,
    pub(super) progress: &'a mut dyn FnMut(StepProgress),
}

impl fmt::Debug for Batch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Batch")
            .field("cancelled", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl<'a> Batch<'a> {
    /// A batch reporting to `progress` and cancelled through `cancel`.
    pub fn new(cancel: &'a CancelToken, progress: &'a mut dyn FnMut(StepProgress)) -> Self {
        Self { cancel, progress }
    }
}

/// How far a step had come when it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Checking the target, the source and the local copy: nothing was sent.
    Preflight,
    /// Sending the copy.
    Upload,
    /// Checking the stored size.
    Verify,
    /// Deleting the source, once its copy was verified: the copy is there
    /// and the source too.
    Delete,
    /// Recording the source's delete in the catalog: the copy is there and
    /// the source is gone; only the catalog still names the source.
    Catalog,
}

impl Stage {
    /// Every stage, in the order a step goes through them.
    pub const ALL: [Stage; 5] = [
        Stage::Preflight,
        Stage::Upload,
        Stage::Verify,
        Stage::Delete,
        Stage::Catalog,
    ];

    /// Stable machine name (`preflight`, `upload`, `verify`, `delete`,
    /// `catalog`).
    pub const fn slug(self) -> &'static str {
        match self {
            Stage::Preflight => "preflight",
            Stage::Upload => "upload",
            Stage::Verify => "verify",
            Stage::Delete => "delete",
            Stage::Catalog => "catalog",
        }
    }
}

/// Why a file stopped its batch. The source of a move or rename is never
/// deleted after any of these, except at [`Stage::Catalog`]: the source was
/// deleted and the catalog could not record it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Halt {
    /// The user cancelled. `partial`: the bytes an interrupted upload left
    /// at the target (offer a confirmed delete), `None` when it left none.
    Cancelled {
        /// Bytes left at the target.
        partial: Option<u64>,
    },
    /// The source is gone or no longer has the planned size.
    SourceChanged,
    /// A file of the target's name is there now and replacing it was not
    /// confirmed.
    Conflict(FileEntry),
    /// The local copy is gone (cleared, evicted) or not the planned bytes.
    NoLocalCopy,
    /// The target refused the file before a byte was sent: no card, over the
    /// per-file limit, or not enough space (with the files of that medium as
    /// candidates; nothing is deleted to make room).
    Refused(Refusal),
    /// The screen or the store failed (a stored size that differs from the
    /// copy's is `BezelError::SizeMismatch`).
    Failed(BezelError),
}

impl Halt {
    /// Stable reason code (`cancelled`, `sourceChanged`, `conflict`,
    /// `noLocalCopy`, `refused`, `failed`).
    pub const fn code(&self) -> &'static str {
        match self {
            Halt::Cancelled { .. } => "cancelled",
            Halt::SourceChanged => "sourceChanged",
            Halt::Conflict(_) => "conflict",
            Halt::NoLocalCopy => "noLocalCopy",
            Halt::Refused(_) => "refused",
            Halt::Failed(_) => "failed",
        }
    }
}

impl From<BezelError> for Halt {
    fn from(error: BezelError) -> Self {
        match error {
            BezelError::Cancelled { partial } => Halt::Cancelled { partial },
            other => Halt::Failed(other),
        }
    }
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Halt::Cancelled { partial } => {
                write!(f, "{}", BezelError::Cancelled { partial: *partial })
            }
            Halt::SourceChanged => {
                f.write_str("the file is gone or changed since the list was made")
            }
            Halt::Conflict(file) => {
                write!(
                    f,
                    "{} is there and replacing it was not confirmed",
                    file.path
                )
            }
            Halt::NoLocalCopy => f.write_str("its local copy is gone"),
            Halt::Refused(refusal) => write!(f, "refused: {refusal}"),
            Halt::Failed(error) => write!(f, "{error}"),
        }
    }
}

/// The step that stopped a batch, how far it came and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    /// The step.
    pub step: Step,
    /// How far it came.
    pub stage: Stage,
    /// Why it stopped.
    pub halt: Halt,
}

impl Stopped {
    /// Whether its copy reached the target and was verified (a move or
    /// rename stopped before deleting its source: both are there).
    pub fn copied(&self) -> bool {
        self.stage == Stage::Delete
    }

    /// Whether its source was deleted after the copy was verified, and only
    /// the catalog could not record it: the file is at the target alone.
    pub fn source_deleted(&self) -> bool {
        self.stage == Stage::Catalog
    }

    /// What the interrupted upload left at the target, for a confirmed
    /// delete (D-2026-09-30-release-polish-10): a cancelled transfer's
    /// partial file, or a file whose stored size differs from the copy's.
    pub fn leftover(&self) -> Option<FileEntry> {
        let size = match &self.halt {
            Halt::Cancelled { partial } => *partial,
            Halt::Failed(BezelError::SizeMismatch { stored, .. }) => {
                (*stored > 0).then_some(*stored)
            }
            _ => None,
        }?;
        Some(FileEntry {
            path: self.step.target.clone(),
            size: Some(size),
        })
    }
}

/// What a move, copy, rename or restore did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReport {
    /// What the plan did.
    pub transfer: Transfer,
    /// Files sent and verified (and, for a move or rename, whose source was
    /// then deleted), in order.
    pub done: Vec<Step>,
    /// The step that stopped the batch, if one did.
    pub stopped: Option<Stopped>,
    /// The steps after it, not started.
    pub not_started: Vec<Step>,
    /// The files the plan left out.
    pub skipped: Vec<Skipped>,
}

impl TransferReport {
    pub(super) fn new(plan: &TransferPlan) -> Self {
        Self {
            transfer: plan.transfer,
            done: Vec::new(),
            stopped: None,
            not_started: Vec::new(),
            skipped: plan.skipped.clone(),
        }
    }

    /// Whether every step ran.
    pub fn completed(&self) -> bool {
        self.stopped.is_none()
    }
}

/// A file a confirmed delete did not delete, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undeleted {
    /// The file as confirmed.
    pub file: FileEntry,
    /// Why not.
    pub halt: Halt,
}

/// What deleting a confirmed list did (the cleanup assistant).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeleteReport {
    /// Files deleted, in order.
    pub deleted: Vec<FileEntry>,
    /// The file that stopped the batch, if one did.
    pub stopped: Option<Undeleted>,
    /// The files after it, not touched.
    pub not_started: Vec<FileEntry>,
}

impl DeleteReport {
    /// Bytes the deleted files held (as confirmed).
    pub fn freed(&self) -> u64 {
        self.deleted.iter().filter_map(|f| f.size).sum()
    }
}
