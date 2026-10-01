//! The storage manager's JSON shapes (camelCase, the contract of
//! `src/bridge.js`): reasons travel as the core's stable codes with their
//! arguments, never as text to parse (D-2026-09-30-release-polish-6).

use bezel_core::BezelError;
use bezel_core::app::manager::{DeleteReport, Halt, Inventory, Stopped, TransferReport};
use bezel_core::domain::archive::{
    ArchiveEntry, CacheInfo, Candidate, Catalog, Listed, PlanRefusal, Skip, Skipped, Step,
    TransferPlan, Warning,
};
use bezel_core::domain::cleanup::{Finding, Protected, Reason, artifact_base};
use bezel_core::domain::storage::{Medium, Operation, RemotePath};
use bezel_themes::dto::SizeDto;
use serde::Serialize;
use serde_json::{Value, json};

use crate::dto::{CapacityDto, RefusalDto, StoredFileDto, name_error_char};
use crate::messages::{ErrorCode, UiError};

/// A file Bezel sent (or associated) as its catalog entry tells it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntryDto {
    /// `pending`, `stored`, `missing` or `deleted`.
    pub state: &'static str,
    /// The store holds its exact bytes.
    pub local_copy: bool,
    /// When it was sent, seconds since the Unix epoch.
    pub sent_at: u64,
    /// The file on the PC it came from.
    pub source: Option<String>,
    /// Play time, when probed.
    pub duration_ms: Option<u64>,
    /// Picture size, when probed.
    pub resolution: Option<SizeDto>,
}

impl CatalogEntryDto {
    /// The DTO of `entry`; `local_copy` says whether its copy is held.
    pub fn of(entry: &ArchiveEntry, local_copy: bool) -> Self {
        Self {
            state: entry.state.slug(),
            local_copy,
            sent_at: entry.sent_at,
            source: entry.source.clone(),
            duration_ms: entry.duration.map(millis),
            resolution: entry.resolution.map(|r| SizeDto {
                width: r.width,
                height: r.height,
            }),
        }
    }
}

fn millis(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// A cleanup finding (core `cleanup::Finding`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingDto {
    /// The reason's code (`duplicate`, `hangPartial`, ...).
    pub code: &'static str,
    /// Whether the assistant checks it at first.
    pub prechecked: bool,
    /// The file that stays (`duplicate`, `variant`, `sameSize`).
    pub kept: Option<String>,
    /// The size Bezel stored there (`sizeDiffers`).
    pub cataloged: Option<u64>,
}

impl From<&Finding> for FindingDto {
    fn from(finding: &Finding) -> Self {
        let (kept, cataloged) = match &finding.reason {
            Reason::Duplicate { kept } | Reason::Variant { kept } | Reason::SameSize { kept } => {
                (Some(kept.to_string()), None)
            }
            Reason::SizeDiffers { cataloged } => (None, Some(*cataloged)),
            Reason::HangPartial | Reason::Pending | Reason::Unused => (None, None),
        };
        Self {
            code: finding.code().slug(),
            prechecked: finding.prechecked(),
            kept,
            cataloged,
        }
    }
}

/// A listed file with its catalog entry, its cleanup finding and whether it
/// is protected.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFileDto {
    /// Where it is and its size (the catalog's when the screen cannot tell).
    #[serde(flatten)]
    pub file: StoredFileDto,
    /// Its entry at that place; `None` when Bezel did not send it.
    pub entry: Option<CatalogEntryDto>,
    /// Why the cleanup assistant suggests it, if it does.
    pub finding: Option<FindingDto>,
    /// `boot` (the boot media Bezel set) or `themeVideo` (a video a theme
    /// plays); `None` otherwise.
    pub protected: Option<&'static str>,
}

/// How `protected` covers `path`.
pub fn protection(protected: &Protected, path: &RemotePath) -> Option<&'static str> {
    if protected.is_boot(path) {
        Some("boot")
    } else if protected.is_theme_video(path) {
        Some("themeVideo")
    } else {
        None
    }
}

impl ManagedFileDto {
    /// The DTO of a listed file.
    pub fn listed(
        listed: &Listed,
        catalog: &Catalog,
        finding: Option<&Finding>,
        protected: &Protected,
    ) -> Self {
        let path = &listed.file.path;
        Self {
            file: StoredFileDto::at(path, Inventory::size(listed)),
            entry: listed
                .entry
                .as_ref()
                .map(|e| CatalogEntryDto::of(e, catalog.has_copy(&e.content))),
            finding: finding.map(FindingDto::from),
            protected: protection(protected, path),
        }
    }
}

/// A cataloged file the screen does not store now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorableDto {
    /// Where it was, and its size.
    #[serde(flatten)]
    pub file: StoredFileDto,
    /// What `plan_restore` takes: its path and card ([`restore_id`]).
    pub id: String,
    /// When it was sent, seconds since the Unix epoch.
    pub sent_at: u64,
    /// The store holds its exact bytes.
    pub local_copy: bool,
    /// It was on a card that is not the inserted one.
    pub other_card: bool,
    /// `pending`, `stored` or `missing`.
    pub state: &'static str,
}

/// How a restorable entry is named to `plan_restore`: its path and, on a
/// card, the card's capacity (`sd/video/a.mp4@31914983424`,
/// `internal/video/a.mp4@`).
pub fn restore_id(entry: &ArchiveEntry) -> String {
    let card = entry.card.map(|c| c.to_string()).unwrap_or_default();
    format!("{}@{card}", entry.path)
}

impl RestorableDto {
    /// The DTO of `entry`.
    pub fn of(entry: &ArchiveEntry, catalog: &Catalog, other_card: bool) -> Self {
        Self {
            file: StoredFileDto::at(&entry.path, Some(entry.size)),
            id: restore_id(entry),
            sent_at: entry.sent_at,
            local_copy: catalog.has_copy(&entry.content),
            other_card,
            state: entry.state.slug(),
        }
    }
}

/// The local copies in numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheDto {
    /// Copies held.
    pub copies: usize,
    /// Their bytes.
    pub bytes: u64,
    /// Copies of files deleted through Bezel (what the limit counts).
    pub deleted_copies: usize,
    /// Their bytes.
    pub deleted_bytes: u64,
    /// The limit of the copies of deleted files, bytes.
    pub limit: u64,
}

impl From<CacheInfo> for CacheDto {
    fn from(info: CacheInfo) -> Self {
        Self {
            copies: info.copies,
            bytes: info.bytes,
            deleted_copies: info.deleted_copies,
            deleted_bytes: info.deleted_bytes,
            limit: info.limit,
        }
    }
}

/// What "Clear cache" removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearedDto {
    /// Copies removed.
    pub removed: usize,
    /// Their bytes.
    pub bytes: u64,
}

/// A folder that could not be listed (the others still are).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderErrorDto {
    /// `internal` or `sd`.
    pub medium: &'static str,
    /// `image` or `video`.
    pub kind: &'static str,
    /// Why.
    pub error: UiError,
}

/// Both media, listed and reconciled with the catalog.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerOverviewDto {
    /// Internal flash.
    pub internal: CapacityDto,
    /// The inserted card; `None` without one.
    pub card: Option<CapacityDto>,
    /// Every listed file, in the listing's order.
    pub files: Vec<ManagedFileDto>,
    /// Folders that could not be listed. The core lists both media as one
    /// inventory, so a folder that fails fails the whole overview (the
    /// command answers its error): this stays empty.
    pub folder_errors: Vec<FolderErrorDto>,
    /// Cataloged files the screen does not store now.
    pub restorable: Vec<RestorableDto>,
    /// Whether the screen deletes through Bezel (TUR_USB does not).
    pub deletes: bool,
    /// The screen's per-file limit, bytes.
    pub cap: u64,
    /// The local copies.
    pub cache: CacheDto,
}

// ----------------------------------------------------------------- plans --

/// One file of a plan.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStepDto {
    /// The file moved, copied or renamed (a restore: where its entry was).
    pub source: String,
    /// Where its copy goes.
    pub target: String,
    /// Bytes sent.
    pub size: u64,
    /// The file there whose replacement was confirmed.
    pub replaces: Option<StoredFileDto>,
}

impl From<&Step> for PlanStepDto {
    fn from(step: &Step) -> Self {
        Self {
            source: step.source.to_string(),
            target: step.target.to_string(),
            size: step.size,
            replaces: step.replaces.as_ref().map(StoredFileDto::from),
        }
    }
}

/// A file a plan leaves out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDto {
    /// The file.
    pub source: String,
    /// Where it would have gone.
    pub target: String,
    /// `conflict`, `noLocalCopy`, `deleteUnsupported` or `present`.
    pub code: &'static str,
    /// The file of the target's name (`conflict`).
    pub conflict: Option<StoredFileDto>,
}

impl From<&Skipped> for SkippedDto {
    fn from(skipped: &Skipped) -> Self {
        let conflict = match &skipped.skip {
            Skip::Conflict(file) => Some(StoredFileDto::from(file)),
            Skip::NoLocalCopy | Skip::DeleteUnsupported | Skip::Present => None,
        };
        Self {
            source: skipped.source.to_string(),
            target: skipped.target.to_string(),
            code: skipped.skip.code(),
            conflict,
        }
    }
}

/// What a plan's confirmation warns about.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanWarningDto {
    /// `bootMedia` or `themeVideo`.
    pub code: &'static str,
    /// The file.
    pub path: String,
}

impl From<&Warning> for PlanWarningDto {
    fn from(warning: &Warning) -> Self {
        let (Warning::BootMedia(path) | Warning::ThemeVideo(path)) = warning;
        Self {
            code: warning.code(),
            path: path.to_string(),
        }
    }
}

/// A plan ready to confirm.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReadyDto {
    /// What `run_plan` takes.
    pub ticket: u64,
    /// `move`, `copy`, `rename` or `restore`.
    pub transfer: &'static str,
    /// The target medium.
    pub to: &'static str,
    /// The files sent, in order.
    pub steps: Vec<PlanStepDto>,
    /// The files left out.
    pub skipped: Vec<SkippedDto>,
    /// What the confirmation warns about.
    pub warnings: Vec<PlanWarningDto>,
    /// Bytes the steps send.
    pub bytes: u64,
    /// Free bytes of the target medium.
    pub free: u64,
}

impl PlanReadyDto {
    /// The DTO of `plan`, kept as `ticket`, onto `to` with `free` bytes.
    pub fn of(ticket: u64, plan: &TransferPlan, to: Medium, free: u64) -> Self {
        Self {
            ticket,
            transfer: plan.transfer.slug(),
            to: to.slug(),
            steps: plan.steps.iter().map(PlanStepDto::from).collect(),
            skipped: plan.skipped.iter().map(SkippedDto::from).collect(),
            warnings: plan.warnings.iter().map(PlanWarningDto::from).collect(),
            bytes: plan.bytes(),
            free,
        }
    }
}

/// Why no plan could be made; nothing was sent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRefusedDto {
    /// The core's `PlanRefusal` code.
    pub code: &'static str,
    /// Its arguments: `notListed`/`sameMedium` `{path}`, `invalidName`
    /// `{char?}`, `extensionChanged` `{expected}`, `unsendable`
    /// `{path, refusal}`, `noSpace` `{needed, free}`.
    pub args: Value,
    /// The core's sentence, in English.
    pub message: String,
}

impl From<&PlanRefusal> for PlanRefusedDto {
    fn from(refusal: &PlanRefusal) -> Self {
        let args = match refusal {
            PlanRefusal::NoCard | PlanRefusal::SameName => json!({}),
            PlanRefusal::NotListed(path) | PlanRefusal::SameMedium(path) => {
                json!({ "path": path.to_string() })
            }
            PlanRefusal::InvalidName(e) => match name_error_char(e) {
                Some(c) => json!({ "char": c }),
                None => json!({}),
            },
            PlanRefusal::ExtensionChanged { expected } => json!({ "expected": expected }),
            PlanRefusal::Unsendable { path, refusal } => json!({
                "path": path.to_string(),
                "refusal": RefusalDto::from(refusal),
            }),
            PlanRefusal::NoSpace { needed, free } => json!({ "needed": needed, "free": free }),
        };
        Self {
            code: refusal.code(),
            args,
            message: refusal.to_string(),
        }
    }
}

/// The answer to a plan request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PlanDto {
    /// Show the one confirmation, then run it.
    Ready(PlanReadyDto),
    /// Explain why not.
    Refused(PlanRefusedDto),
}

// --------------------------------------------------------------- reports --

/// The file that stopped a run because it failed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedStepDto {
    /// The file.
    pub step: PlanStepDto,
    /// What failed, as an error code; `None` for a refusal.
    pub error: Option<UiError>,
    /// The target's preflight refusal (no card, too large, no space).
    pub refusal: Option<RefusalDto>,
}

/// The file whose upload the user cancelled.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledStepDto {
    /// The file.
    pub step: PlanStepDto,
    /// Bytes the interrupted upload left at the target.
    pub partial: Option<u64>,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferReportDto {
    /// `move`, `copy`, `rename` or `restore`.
    pub transfer: &'static str,
    /// Files sent and verified (moved, renamed: their sources deleted).
    pub done: Vec<PlanStepDto>,
    /// The file that failed, if one did.
    pub failed: Option<FailedStepDto>,
    /// The file the user cancelled, if any.
    pub cancelled: Option<CancelledStepDto>,
    /// The files after it, not started.
    pub not_started: Vec<PlanStepDto>,
}

/// Why a file stopped its batch, as the UI's error codes: the core's error
/// for a failure; a target taken since the plan as the confirmation its
/// replacement still needs; a source changed or a copy gone as unusable
/// input, with the core's sentence.
pub fn halt_error(halt: &Halt) -> UiError {
    match halt {
        Halt::Failed(error) => UiError::from(error.clone()),
        Halt::Conflict(file) => UiError::from(BezelError::NotConfirmed(
            Operation::Overwrite(file.path.clone()).to_string(),
        )),
        Halt::Cancelled { partial } => UiError::from(BezelError::Cancelled { partial: *partial }),
        Halt::SourceChanged | Halt::NoLocalCopy | Halt::Refused(_) => {
            UiError::new(ErrorCode::InvalidInput).arg("detail", halt)
        }
    }
}

impl From<&TransferReport> for TransferReportDto {
    fn from(report: &TransferReport) -> Self {
        let steps = |steps: &[Step]| steps.iter().map(PlanStepDto::from).collect();
        let mut dto = Self {
            transfer: report.transfer.slug(),
            done: steps(&report.done),
            failed: None,
            cancelled: None,
            not_started: steps(&report.not_started),
        };
        if let Some(Stopped { step, halt, .. }) = &report.stopped {
            let step = PlanStepDto::from(step);
            match halt {
                Halt::Cancelled { partial } => {
                    dto.cancelled = Some(CancelledStepDto {
                        step,
                        partial: *partial,
                    });
                }
                Halt::Refused(refusal) => {
                    dto.failed = Some(FailedStepDto {
                        step,
                        error: None,
                        refusal: Some(RefusalDto::from(refusal)),
                    });
                }
                other => {
                    dto.failed = Some(FailedStepDto {
                        step,
                        error: Some(halt_error(other)),
                        refusal: None,
                    });
                }
            }
        }
        dto
    }
}

/// The file that stopped a batch delete.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFailedDto {
    /// The file.
    pub path: String,
    /// Why.
    pub error: UiError,
}

/// What deleting a confirmed list did.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReportDto {
    /// Files deleted, in order.
    pub deleted: Vec<String>,
    /// The file that failed, if one did.
    pub failed: Option<DeleteFailedDto>,
    /// Whether the user cancelled.
    pub cancelled: bool,
    /// The files not touched.
    pub not_started: Vec<String>,
    /// Bytes the deleted files held.
    pub freed: u64,
}

impl DeleteReportDto {
    /// The batch stopped at `path` by `error` before the core could start
    /// on it (the card's capacity could not be read); `rest` not started.
    pub fn failed_at(mut self, path: &RemotePath, error: UiError, rest: &[RemotePath]) -> Self {
        self.failed = Some(DeleteFailedDto {
            path: path.to_string(),
            error,
        });
        self.not_started = rest.iter().map(ToString::to_string).collect();
        self
    }

    /// Adds what one call of the core's batch delete did; `false` when it
    /// stopped the batch (`rest`: the files after it, not started).
    pub fn add(&mut self, report: &DeleteReport, rest: &[RemotePath]) -> bool {
        self.deleted
            .extend(report.deleted.iter().map(|f| f.path.to_string()));
        self.freed += report.freed();
        let Some(stopped) = &report.stopped else {
            return true;
        };
        let path = stopped.file.path.to_string();
        let mut not_started: Vec<String> = report
            .not_started
            .iter()
            .map(|f| f.path.to_string())
            .collect();
        not_started.extend(rest.iter().map(ToString::to_string));
        if let Halt::Cancelled { .. } = stopped.halt {
            self.cancelled = true;
            not_started.insert(0, path);
        } else {
            self.failed = Some(DeleteFailedDto {
                path,
                error: halt_error(&stopped.halt),
            });
        }
        self.not_started = not_started;
        false
    }
}

// ------------------------------------------------------------- originals --

/// A file on the PC of exactly the screen file's size and kind.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    /// Where it is on the PC.
    pub source: String,
    /// Its file name.
    pub name: String,
    /// Its size, bytes.
    pub size: u64,
    /// `image` or `video`.
    pub kind: &'static str,
    /// Play time, when probed.
    pub duration_ms: Option<u64>,
    /// Picture size, when probed.
    pub resolution: Option<SizeDto>,
    /// It has the screen file's name (vendor name artifacts aside).
    pub same_name: bool,
}

/// A name's comparable part, as the core ranks candidates: the stem of the
/// name a vendor artifact stands for, letters and digits only, lower-case
/// (`NVI.mp427034822.mp4` and `nvi.mp4`: `nvi`).
fn name_key(name: &str) -> String {
    let base = artifact_base(name).unwrap_or_else(|| name.to_ascii_lowercase());
    let stem = base
        .rsplit_once('.')
        .map_or(base.as_str(), |(stem, _)| stem);
    stem.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The last part of a path on the PC (either separator).
pub fn file_name_of(source: &str) -> &str {
    source.rsplit(['/', '\\']).next().unwrap_or(source)
}

impl CandidateDto {
    /// The DTO of `candidate` for the screen file at `path`.
    pub fn of(candidate: &Candidate, path: &RemotePath) -> Self {
        let name = file_name_of(&candidate.source).to_string();
        let media = &candidate.media;
        Self {
            same_name: name_key(&name) == name_key(path.name.as_str()),
            source: candidate.source.clone(),
            name,
            size: media.bytes,
            kind: media.kind().unwrap_or(path.location.kind).slug(),
            duration_ms: media.video.and_then(|v| v.duration).map(millis),
            resolution: media.dimensions.map(|d| SizeDto {
                width: d.width,
                height: d.height,
            }),
        }
    }
}

/// The originals found for a screen file, likeliest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidatesDto {
    /// The candidates.
    pub candidates: Vec<CandidateDto>,
}
