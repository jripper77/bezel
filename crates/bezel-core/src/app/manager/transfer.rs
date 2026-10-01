//! Moving, copying, renaming and restoring: plans made from what the screen
//! lists now, then run one file at a time (D-2026-09-30-storage-manager-7,
//! -8).

use super::Manager;
use super::ledger::{self, Recorded};
use super::report::{Batch, Halt, ManagerError, Stage, StepProgress, Stopped, TransferReport};
use crate::app::storage::{
    Presence, presence, profile_of, storage_of, stored_on, verify, with_sizes,
};
use crate::domain::archive::{
    self, ArchiveEntry, Catalog, EntryState, PlanRefusal, Room, ScreenKey, ScreenView, Step,
    Transfer, TransferPlan,
};
use crate::domain::job::Job;
use crate::domain::screen::Confirm;
use crate::domain::storage::{
    Confirmed, FileEntry, Medium, Operation, Refusal, RemotePath, StorageInfo, check_size,
};
use crate::ports::{ArchiveStore, ScreenStorage};
use crate::{BezelError, Result};

impl Manager<'_> {
    /// Plans moving the listed `sources` to the other medium `to`
    /// ([`archive::plan_move`]): files whose name is taken there are skipped
    /// unless their target is in `overwrite`. Only queries the screen.
    pub fn plan_move(
        &mut self,
        sources: &[RemotePath],
        to: Medium,
        overwrite: &[RemotePath],
    ) -> std::result::Result<TransferPlan, ManagerError> {
        self.plan(|view, _| archive::plan_move(view, sources, to, overwrite))
    }

    /// Plans copying the listed `sources` to the other medium `to`; the
    /// sources stay ([`archive::plan_copy`]). Only queries the screen.
    pub fn plan_copy(
        &mut self,
        sources: &[RemotePath],
        to: Medium,
        overwrite: &[RemotePath],
    ) -> std::result::Result<TransferPlan, ManagerError> {
        self.plan(|view, _| archive::plan_copy(view, sources, to, overwrite))
    }

    /// Plans renaming the listed `source` to `new_name` on its medium
    /// ([`archive::plan_rename`]). Only queries the screen.
    pub fn plan_rename(
        &mut self,
        source: &RemotePath,
        new_name: &str,
        overwrite: &[RemotePath],
    ) -> std::result::Result<TransferPlan, ManagerError> {
        self.plan(|view, _| archive::plan_rename(view, source, new_name, overwrite))
    }

    /// Plans restoring `selection` onto `to` ([`archive::plan_restore`];
    /// the default selection is [`super::Inventory`]'s
    /// `overview.restorable(to)`): refused unless every file fits the
    /// per-file limit and their total the free space. Only queries the
    /// screen.
    pub fn plan_restore(
        &mut self,
        selection: &[ArchiveEntry],
        to: Medium,
        overwrite: &[RemotePath],
    ) -> std::result::Result<TransferPlan, ManagerError> {
        let cap = profile_of(self.link)?.max_upload_bytes;
        self.plan(|view, info| {
            // Without a card the plan refuses a card target (`noCard`).
            let free = info.capacity(to).map_or(0, |c| c.free);
            let room = Room { free, cap };
            archive::plan_restore(view, selection, to, room, overwrite)
        })
    }

    /// A plan made by `make` from what the screen lists now.
    fn plan(
        &mut self,
        make: impl FnOnce(
            &ScreenView<'_>,
            &StorageInfo,
        ) -> std::result::Result<TransferPlan, PlanRefusal>,
    ) -> std::result::Result<TransferPlan, ManagerError> {
        let inventory = self.inventory()?;
        let protected = self.protected(&inventory.catalog);
        Ok(make(&inventory.view(&protected), &inventory.info)?)
    }

    /// Runs `plan`, one file at a time: preflight on the target (card, the
    /// per-file limit, the source unchanged, the name not taken unless its
    /// replacement was confirmed, free space), upload of the local copy,
    /// check of the stored size, and only then (move, rename) delete of the
    /// source; the catalog follows each step. The batch stops at the first
    /// failure or cancel: that file's source stays, and the report lists what
    /// was done, what stopped it and what was not started.
    ///
    /// Needs `Confirm::Yes`, the user's answer to the one confirmation that
    /// listed every file; with `Confirm::No` the screen is not called. A
    /// restore checks again that the files fit the medium before the first
    /// byte (refused otherwise) and never deletes. `now` is the time sent
    /// (seconds since the Unix epoch).
    pub fn run(
        &mut self,
        plan: &TransferPlan,
        confirm: Confirm,
        now: u64,
        batch: &mut Batch<'_>,
    ) -> std::result::Result<TransferReport, ManagerError> {
        if confirm == Confirm::No {
            return Err(BezelError::NotConfirmed(asked(plan)).into());
        }
        if plan.transfer.deletes_source() {
            self.refuse_without_delete()?;
        }
        let cap = profile_of(self.link)?.max_upload_bytes;
        let storage = storage_of(self.link)?;
        if plan.transfer == Transfer::Restore {
            fits(storage, plan, cap)?;
        }
        let mut runner = Runner {
            storage,
            store: &mut *self.store,
            key: &self.key,
            cap,
            transfer: plan.transfer,
            confirm,
            now,
        };
        Ok(runner.all(plan, batch))
    }
}

/// What the confirmation `plan` needs is about.
fn asked(plan: &TransferPlan) -> String {
    let verb = match plan.transfer {
        Transfer::Move => "moving",
        Transfer::Copy => "copying",
        Transfer::Rename => "renaming",
        Transfer::Restore => "restoring",
    };
    let files = plan.steps.len();
    let plural = if files == 1 { "" } else { "s" };
    format!("{verb} {files} file{plural}")
}

/// A restore must fit before its first byte (D-2026-09-30-storage-manager-8):
/// every file within the per-file limit, their total within the free space.
fn fits(
    storage: &mut dyn ScreenStorage,
    plan: &TransferPlan,
    cap: u64,
) -> std::result::Result<(), ManagerError> {
    let Some(first) = plan.steps.first() else {
        return Ok(());
    };
    let info = storage.info()?;
    let medium = first.target.location.medium;
    let free = info.capacity(medium).ok_or(PlanRefusal::NoCard)?.free;
    for step in &plan.steps {
        check_size(step.size, cap).map_err(|refusal| PlanRefusal::Unsendable {
            path: step.target.clone(),
            refusal,
        })?;
    }
    let needed = plan.bytes();
    if needed >= free {
        return Err(PlanRefusal::NoSpace { needed, free }.into());
    }
    Ok(())
}

/// What a step needs once its preflight passed.
struct Ready {
    /// The inserted card's capacity (keys card entries).
    card: Option<u64>,
    /// The local copy's bytes.
    bytes: Vec<u8>,
}

/// Runs the steps of one plan.
struct Runner<'r> {
    storage: &'r mut dyn ScreenStorage,
    store: &'r mut dyn ArchiveStore,
    key: &'r ScreenKey,
    cap: u64,
    transfer: Transfer,
    confirm: Confirm,
    now: u64,
}

impl Runner<'_> {
    /// Every step in order, until one stops the batch.
    fn all(&mut self, plan: &TransferPlan, batch: &mut Batch<'_>) -> TransferReport {
        let mut report = TransferReport::new(plan);
        let steps = plan.steps.len();
        for (index, step) in plan.steps.iter().enumerate() {
            let mut sink = |progress| {
                (batch.progress)(StepProgress {
                    step: index,
                    steps,
                    progress,
                });
            };
            let mut job = Job::new(batch.cancel, &mut sink);
            if let Err((stage, halt)) = self.step(step, &mut job) {
                let step = step.clone();
                report.stopped = Some(Stopped { step, stage, halt });
                report.not_started = plan.steps[index + 1..].to_vec();
                break;
            }
            report.done.push(step.clone());
        }
        report
    }

    /// One file: preflight, upload, verify, then (move, rename) delete and
    /// the catalog forgets the source.
    fn step(&mut self, step: &Step, job: &mut Job<'_>) -> std::result::Result<(), (Stage, Halt)> {
        let ready = self.preflight(step, job).map_err(at(Stage::Preflight))?;
        let pending = self
            .record_target(step, ready.card)
            .map_err(|e| (Stage::Preflight, Halt::from(e)))?;
        let sent = self.send(step, &ready.bytes, job);
        let settled = ledger::settle(
            self.store,
            self.key,
            &pending,
            sent.as_ref().err().map(|(_, e)| e),
        );
        sent.map_err(|(stage, error)| (stage, Halt::from(error)))?;
        settled.map_err(|e| (Stage::Verify, Halt::from(e)))?;
        if self.transfer.deletes_source() {
            self.delete_source(step, job).map_err(at(Stage::Delete))?;
            ledger::forget(self.store, self.key, &step.source, ready.card)
                .map_err(|e| (Stage::Catalog, Halt::from(e)))?;
        }
        Ok(())
    }

    /// Nothing is sent unless the target takes the file, the source is the
    /// one planned and the local copy is there.
    fn preflight(&mut self, step: &Step, job: &Job<'_>) -> std::result::Result<Ready, Halt> {
        job.checkpoint()?;
        let info = self.storage.info()?;
        let medium = step.target.location.medium;
        let capacity = info
            .capacity(medium)
            .ok_or(Halt::Refused(Refusal::NoCard))?;
        check_size(step.size, self.cap).map_err(Halt::Refused)?;
        if self.transfer.deletes_source() {
            self.source_unchanged(step)?;
        }
        self.target_free(step)?;
        if step.size >= capacity.free {
            let names = stored_on(self.storage, &info, medium)?;
            let candidates = with_sizes(self.storage, names)?;
            let refusal = Refusal::no_space(step.size, capacity.free, candidates);
            return Err(Halt::Refused(refusal));
        }
        let bytes = self.store.read(&step.content)?;
        let bytes = bytes.filter(|b| b.len() as u64 == step.size);
        let bytes = bytes.ok_or(Halt::NoLocalCopy)?;
        let card = info.card.map(|c| c.total);
        Ok(Ready { card, bytes })
    }

    /// The source of a move or rename must still be the file planned.
    fn source_unchanged(&mut self, step: &Step) -> std::result::Result<(), Halt> {
        match presence(self.storage, &step.source)? {
            Presence::Stored(None) => Ok(()),
            Presence::Stored(Some(size)) if size == step.size => Ok(()),
            _ => Err(Halt::SourceChanged),
        }
    }

    /// The target's name must be free, letter case aside, unless the plan
    /// replaces that file with the user's confirmation.
    fn target_free(&mut self, step: &Step) -> std::result::Result<(), Halt> {
        let names = self.storage.list(step.target.location)?;
        let wanted = step.target.name.as_str();
        let Some(name) = names
            .into_iter()
            .find(|n| n.as_str().eq_ignore_ascii_case(wanted))
        else {
            return Ok(());
        };
        let path = RemotePath::new(step.target.location, name);
        if step.replaces.as_ref().is_some_and(|r| r.path == path) {
            return Ok(());
        }
        let size = presence(self.storage, &path)?.size();
        Err(Halt::Conflict(FileEntry { path, size }))
    }

    /// Catalogs the target as pending, with what is known of the file.
    fn record_target(&mut self, step: &Step, card: Option<u64>) -> Result<Recorded> {
        let (key, now) = (self.key, self.now);
        ledger::change(self.store, |edit| {
            let mut entry = ArchiveEntry::pending(
                step.target.clone(),
                card,
                step.size,
                step.content.clone(),
                now,
            );
            if let Some(origin) = origin(&edit.catalog, key, step) {
                entry.source = origin.source.clone();
                entry.duration = origin.duration;
                entry.resolution = origin.resolution;
            }
            ledger::record(edit, key, entry)
        })
    }

    /// Uploads the copy and checks the stored size.
    fn send(
        &mut self,
        step: &Step,
        bytes: &[u8],
        job: &mut Job<'_>,
    ) -> std::result::Result<(), (Stage, BezelError)> {
        let upload = |e| (Stage::Upload, e);
        job.checkpoint().map_err(upload)?;
        self.storage
            .upload(&step.target, bytes, job)
            .map_err(upload)?;
        verify(self.storage, &step.target, step.size, job).map_err(|e| (Stage::Verify, e))
    }

    /// Deletes the source of a verified copy, unless the user cancelled
    /// meanwhile; the caller then forgets its entry: the file lives at the
    /// target now.
    fn delete_source(&mut self, step: &Step, job: &Job<'_>) -> std::result::Result<(), Halt> {
        job.checkpoint()?;
        let confirmed = Confirmed::require(self.confirm, &Operation::Delete(step.source.clone()))?;
        self.storage.delete(&step.source, confirmed)?;
        Ok(())
    }
}

/// The entry `step` sends again: the one at its source with its copy (live
/// first: a restore's may be missing or of another card).
fn origin<'c>(catalog: &'c Catalog, key: &ScreenKey, step: &Step) -> Option<&'c ArchiveEntry> {
    let record = catalog.screen(key)?;
    let same = |e: &&ArchiveEntry| e.path == step.source && e.content == step.content;
    let mut found = record.entries.iter().filter(same);
    let live = found.clone().find(|e| e.state != EntryState::Deleted);
    live.or_else(|| found.next())
}

/// Tags a halt with the stage it happened at.
fn at(stage: Stage) -> impl Fn(Halt) -> (Stage, Halt) {
    move |halt| (stage, halt)
}
