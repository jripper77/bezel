//! The storage manager's use cases (D-2026-09-30-storage-manager-2..-11):
//! what a screen stores next to the catalog of what Bezel sent it, uploads,
//! deletes and boot media recorded in that catalog, moving, copying,
//! renaming and restoring files by re-sending their local copies, the
//! cleanup assistant, associating a screen file with its original on the PC,
//! and the local copies' cache.
//!
//! A [`Manager`] works on one screen ([`ScreenLink`]) and the store of local
//! copies ([`ArchiveStore`]). The catalog is loaded from the store and saved
//! back at every step, so the CLI and the studio can share it. Nothing here
//! changes the screen without `Confirm::Yes`: with `Confirm::No` the screen
//! only answers queries. Every file of a batch runs alone, in the only order
//! that cannot lose it (D-2026-09-30-storage-manager-7): preflight on the
//! target, upload of the local copy, check of the stored size, and only then
//! delete of the source. A batch stops at the first failure or cancel.
//! Restoring never deletes and checks the space and the per-file limit
//! before the first byte (D-2026-09-30-storage-manager-8).

mod cleanup;
mod copies;
mod ledger;
mod report;
mod transfer;

pub use cleanup::Cleanup;
pub use copies::{Cleared, cache_info, clear_cache, forget, set_cache_limit};
pub use report::{
    Batch, DeleteReport, Halt, ManagerError, Stage, StepProgress, Stopped, TransferReport,
    Undeleted,
};

use crate::app::storage::{
    self, PreparedUpload, Sending, Uploaded, presence, profile_of, storage_of,
};
use crate::domain::archive::{
    ArchiveEntry, Catalog, ContentId, Deletes, Listed, Listing, Overview, ScreenKey, ScreenRecord,
    ScreenView,
};
use crate::domain::cleanup::Protected;
use crate::domain::device::Family;
use crate::domain::job::Job;
use crate::domain::screen::{Brightness, Confirm};
use crate::domain::storage::{
    BootMedia, Confirmed, FileEntry, Medium, Operation, RemotePath, StorageInfo, StorageLocation,
};
use crate::domain::theme::AssetRef;
use crate::ports::{ArchiveStore, MediaTranscoder, ScreenLink, ScreenStorage};
use crate::{BezelError, Result};

/// The storage manager on one screen.
pub struct Manager<'a> {
    link: &'a mut dyn ScreenLink,
    store: &'a mut dyn ArchiveStore,
    key: ScreenKey,
    theme_videos: Vec<AssetRef>,
}

impl std::fmt::Debug for Manager<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager")
            .field("key", &self.key)
            .field("theme_videos", &self.theme_videos)
            .finish_non_exhaustive()
    }
}

/// What one screen stores next to its catalog record
/// ([`Manager::inventory`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    /// The screen's catalog key.
    pub key: ScreenKey,
    /// Capacity and use of both media.
    pub info: StorageInfo,
    /// Every file of both media (the card's only when one is inserted), and
    /// the card's capacity.
    pub listing: Listing,
    /// The listing next to the catalog: entries listed, missing, on another
    /// card.
    pub overview: Overview,
    /// The catalog as saved after this listing (with the copies held).
    pub catalog: Catalog,
    /// Whether the screen deletes through Bezel (TUR_USB does not,
    /// D-2026-09-30-storage-manager-11).
    pub deletes: Deletes,
}

impl Inventory {
    /// The size to show for a listed file: the screen's, or the catalog's
    /// when the screen cannot tell (TUR_USB); `None`: unknown.
    pub fn size(listed: &Listed) -> Option<u64> {
        listed.file.size.or(listed.entry.as_ref().map(|e| e.size))
    }

    /// Whether `entry`'s local copy is held (it can be moved, renamed and
    /// restored, and has a thumbnail).
    pub fn has_copy(&self, entry: &ArchiveEntry) -> bool {
        self.catalog.has_copy(&entry.content)
    }

    /// The screen's catalog record.
    pub fn record(&self) -> Option<&ScreenRecord> {
        self.catalog.screen(&self.key)
    }

    /// The boot media Bezel last set on the screen.
    pub fn boot(&self) -> Option<&RemotePath> {
        self.record().and_then(|r| r.boot.as_ref())
    }

    /// The screen as the plans see it.
    pub fn view<'v>(&'v self, protected: &'v Protected) -> ScreenView<'v> {
        ScreenView {
            catalog: &self.catalog,
            key: &self.key,
            listing: &self.listing,
            deletes: self.deletes,
            protected,
        }
    }
}

/// Whether the screen behind `link` deletes through Bezel.
pub fn deletes(link: &dyn ScreenLink) -> Deletes {
    match link.identity().model.family {
        Family::TuringUsb => Deletes::Unsupported,
        _ => Deletes::Supported,
    }
}

impl<'a> Manager<'a> {
    /// The manager of the screen behind `link`, keyed by its model alone,
    /// with the local copies in `store`.
    pub fn new(link: &'a mut dyn ScreenLink, store: &'a mut dyn ArchiveStore) -> Self {
        let key = ScreenKey::new(link.identity().model.id);
        Self {
            link,
            store,
            key,
            theme_videos: Vec::new(),
        }
    }

    /// Keys the screen by its model plus the name the user gave it.
    #[must_use]
    pub fn named(mut self, name: &str) -> Self {
        self.key = ScreenKey::named(self.link.identity().model.id, name);
        self
    }

    /// Protects the videos themes play: the cleanup assistant never suggests
    /// them and renaming one warns (D-2026-09-30-storage-manager-7, -9).
    #[must_use]
    pub fn protecting(mut self, theme_videos: impl IntoIterator<Item = AssetRef>) -> Self {
        self.theme_videos.extend(theme_videos);
        self
    }

    /// The screen's catalog key.
    pub fn key(&self) -> &ScreenKey {
        &self.key
    }

    /// What the screen stores next to its catalog record: lists both media
    /// (queries only) and saves the catalog with each entry marked stored or
    /// missing by whether it is listed.
    pub fn inventory(&mut self) -> Result<Inventory> {
        let deletes = deletes(self.link);
        let storage = storage_of(self.link)?;
        let info = storage.info()?;
        let listing = listing(storage, &info)?;
        let key = &self.key;
        let (overview, catalog) = ledger::change(self.store, |edit| {
            let overview = edit.catalog.reconcile(key, &listing);
            (overview, edit.catalog.clone())
        })?;
        Ok(Inventory {
            key: self.key.clone(),
            info,
            listing,
            overview,
            catalog,
            deletes,
        })
    }

    /// Runs a prepared upload as [`storage::upload`] does, recorded: right
    /// before the first byte, the exact bytes sent are kept in the store and
    /// the file is cataloged as pending; once its size is verified it is
    /// stored. A cancel that left nothing drops the entry; any other failure
    /// leaves it pending (a leftover for the cleanup assistant). When the
    /// upload fails and the catalog then cannot be saved, the upload's error
    /// is the one returned.
    pub fn upload(
        &mut self,
        media: &mut dyn MediaTranscoder,
        prepared: &PreparedUpload,
        confirm: Confirm,
        now: u64,
        job: &mut Job<'_>,
    ) -> Result<Uploaded> {
        let path = &prepared.plan.path;
        if prepared.plan.replaces.is_some() {
            Confirmed::require(confirm, &Operation::Overwrite(path.clone()))?;
        }
        let card = self.card_for(path.location.medium)?;
        let (store, key) = (&mut *self.store, &self.key);
        let mut pending = None;
        let mut record = |sending: Sending<'_>| {
            let content = store.keep(sending.data)?;
            let entry = sent_entry(&sending, card, content, &prepared.source.0, now);
            pending = Some(ledger::begin(store, key, entry)?);
            Ok(())
        };
        let result = storage::upload_with(self.link, media, prepared, confirm, job, &mut record);
        let Some(pending) = pending else {
            return result;
        };
        let settled = ledger::settle(self.store, &self.key, &pending, result.as_ref().err());
        let uploaded = result?;
        settled?;
        Ok(uploaded)
    }

    /// Deletes a stored file (`Confirm::Yes`; with `Confirm::No` the screen
    /// is not called) and marks its entry deleted: from then on its copy
    /// counts against the cache limit. Screens that cannot delete through
    /// Bezel are refused before anything is sent.
    pub fn delete(&mut self, path: &RemotePath, confirm: Confirm) -> Result<()> {
        let confirmed = Confirmed::require(confirm, &Operation::Delete(path.clone()))?;
        self.refuse_without_delete()?;
        let card = self.card_for(path.location.medium)?;
        storage_of(self.link)?.delete(path, confirmed)?;
        ledger::deleted(self.store, &self.key, path, card)
    }

    /// Sets the boot media as [`storage::set_boot_media`] does and records
    /// it.
    pub fn set_boot_media(
        &mut self,
        boot: &BootMedia,
        brightness: Option<Brightness>,
        confirm: Confirm,
    ) -> Result<()> {
        storage::set_boot_media(self.link, boot, brightness, confirm)?;
        let key = &self.key;
        ledger::change(self.store, |edit| {
            edit.catalog.screen_mut(key).set_boot(boot);
        })
    }

    /// The boot media and the theme videos the manager protects.
    fn protected(&self, catalog: &Catalog) -> Protected {
        let boot = catalog.screen(&self.key).and_then(|r| r.boot.clone());
        let mut protected = Protected::new(boot);
        if let Ok(profile) = profile_of(self.link) {
            for video in &self.theme_videos {
                protected.theme_video(video, &profile);
            }
        }
        protected
    }

    /// The inserted card's capacity, which keys card entries, when `medium`
    /// is the card (an info query); `None` on the internal flash.
    fn card_for(&mut self, medium: Medium) -> Result<Option<u64>> {
        if medium == Medium::Internal {
            return Ok(None);
        }
        Ok(storage::info(self.link)?.card.map(|c| c.total))
    }

    fn refuse_without_delete(&self) -> Result<()> {
        match deletes(self.link) {
            Deletes::Supported => Ok(()),
            Deletes::Unsupported => Err(BezelError::Unsupported(format!(
                "{} cannot delete files through Bezel",
                self.link.identity().model.name
            ))),
        }
    }
}

/// Every file of both media with its size (a card's only when one is
/// inserted: listing creates its folders), and the card's capacity.
fn listing(storage: &mut dyn ScreenStorage, info: &StorageInfo) -> Result<Listing> {
    let mut files = Vec::new();
    for location in StorageLocation::ALL {
        if info.capacity(location.medium).is_none() {
            continue;
        }
        for name in storage.list(location)? {
            let path = RemotePath::new(location, name);
            let size = presence(storage, &path)?.size();
            files.push(FileEntry { path, size });
        }
    }
    Ok(Listing {
        files,
        card: info.card.map(|c| c.total),
    })
}

/// The pending entry of what an upload is about to send.
fn sent_entry(
    sending: &Sending<'_>,
    card: Option<u64>,
    content: ContentId,
    source: &str,
    now: u64,
) -> ArchiveEntry {
    let size = sending.data.len() as u64;
    let mut entry = ArchiveEntry::pending(sending.path.clone(), card, size, content, now);
    entry.source = Some(source.to_string());
    entry.duration = sending.media.video.and_then(|v| v.duration);
    entry.resolution = sending.media.dimensions;
    entry
}
