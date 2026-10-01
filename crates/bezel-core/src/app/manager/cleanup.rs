//! The cleanup assistant (D-2026-09-30-storage-manager-3, -9): findings for
//! what the screen lists, and the delete of exactly the list the user
//! confirmed, one file at a time.

use super::report::{DeleteReport, Halt, Undeleted};
use super::{Inventory, Manager, ledger};
use crate::app::storage::{Presence, presence, storage_of};
use crate::domain::cleanup::{Finding, findings};
use crate::domain::job::CancelToken;
use crate::domain::screen::Confirm;
use crate::domain::storage::{Confirmed, FileEntry, Medium, Operation};
use crate::{BezelError, Result};

/// What the cleanup assistant found on one screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cleanup {
    /// What the screen stores next to its catalog.
    pub inventory: Inventory,
    /// The findings, in the listing's order (never the boot media nor a
    /// theme's video).
    pub findings: Vec<Finding>,
}

impl Cleanup {
    /// The files pre-checked: only exact signals (a vendor duplicate of
    /// equal size, the rev C hang partial, an interrupted Bezel upload).
    pub fn prechecked(&self) -> Vec<FileEntry> {
        let checked = self.findings.iter().filter(|f| f.prechecked());
        checked.map(|f| f.file.clone()).collect()
    }
}

impl Manager<'_> {
    /// The cleanup findings for the screen: lists both media (queries only)
    /// and suggests; deletes nothing. Screens that cannot delete through
    /// Bezel are refused (D-2026-09-30-storage-manager-11).
    pub fn cleanup(&mut self) -> Result<Cleanup> {
        self.refuse_without_delete()?;
        let inventory = self.inventory()?;
        let protected = self.protected(&inventory.catalog);
        let findings = findings(&inventory.listing, inventory.record(), &protected);
        Ok(Cleanup {
            inventory,
            findings,
        })
    }

    /// Deletes exactly `files`, the list the user confirmed with its sizes,
    /// one by one, each entry marked deleted. A file that is gone or changed
    /// size since stops the batch (`sourceChanged`), as do a failure and a
    /// cancel (checked between files). Needs `Confirm::Yes`; with
    /// `Confirm::No` the screen is not called.
    pub fn delete_files(
        &mut self,
        files: &[FileEntry],
        confirm: Confirm,
        cancel: &CancelToken,
    ) -> Result<DeleteReport> {
        if confirm == Confirm::No {
            let plural = if files.len() == 1 { "" } else { "s" };
            let asked = format!("deleting {} file{plural}", files.len());
            return Err(BezelError::NotConfirmed(asked));
        }
        self.refuse_without_delete()?;
        let on_card = files.iter().any(|f| f.path.location.medium == Medium::Card);
        let card = if on_card {
            self.card_for(Medium::Card)?
        } else {
            None
        };
        let mut report = DeleteReport::default();
        for (index, file) in files.iter().enumerate() {
            if let Err(halt) = self.delete_one(file, card, confirm, cancel) {
                let file = file.clone();
                report.stopped = Some(Undeleted { file, halt });
                report.not_started = files[index + 1..].to_vec();
                break;
            }
            report.deleted.push(file.clone());
        }
        Ok(report)
    }

    fn delete_one(
        &mut self,
        file: &FileEntry,
        card: Option<u64>,
        confirm: Confirm,
        cancel: &CancelToken,
    ) -> std::result::Result<(), Halt> {
        if cancel.is_cancelled() {
            return Err(Halt::Cancelled { partial: None });
        }
        let storage = storage_of(self.link)?;
        match presence(storage, &file.path)? {
            Presence::Stored(size) if size == file.size => {}
            _ => return Err(Halt::SourceChanged),
        }
        let confirmed = Confirmed::require(confirm, &Operation::Delete(file.path.clone()))?;
        storage.delete(&file.path, confirmed)?;
        ledger::deleted(self.store, &self.key, &file.path, card)?;
        Ok(())
    }
}
