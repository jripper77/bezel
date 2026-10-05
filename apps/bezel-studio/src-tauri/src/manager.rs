//! The storage manager of the Storage tab (D-2026-09-30-storage-manager-2..
//! -13): both media next to the catalog of what Bezel sent, thumbnails of
//! the local copies, moving, copying, renaming and restoring files by
//! re-sending their copies, the cleanup assistant's batch delete,
//! associating a screen file with its original on the PC, and the cache of
//! copies; over the core's `app::manager` use cases.
//!
//! Every command that reaches a screen is the one storage operation of the
//! moment, like the storage tab's ([`StorageState`]): the others answer
//! `busy`, and a live screen lends its link meanwhile. The catalog and the
//! copies live in the store the composition root picks ([`Copies`]); the
//! store is only touched under that claim, except for thumbnails, which are
//! read or made from the copies alone with a converter of their own, so they
//! never wait for a running job.
//!
//! A plan waits here under a ticket for the user's one confirmation, which
//! `run_plan` passes on to the core as its `Confirm`. Any other job that may
//! change the screen meanwhile makes the ticket stale.

mod dto;
mod originals;
mod plans;

#[cfg(test)]
mod tests;

pub use dto::*;
pub use plans::Ask;

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bezel_core::app::manager::{self, Cleared, Inventory, Manager};
use bezel_core::domain::archive::{
    ArchiveEntry, Catalog, Clear, ContentId, Deletes, EntryState, TransferPlan,
};
use bezel_core::domain::cleanup::{Finding, Protected};
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::media::UploadProfile;
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::{Medium, RemotePath};
use bezel_core::domain::theme::{AssetRef, Background, Theme};
use bezel_core::ports::{ArchiveStore, MediaTranscoder, ScreenLink};
use bezel_media::archive::{DiskArchive, MemoryArchive};

use crate::backend::Backend;
use crate::diag::{self, DiagCode};
use crate::messages::{ErrorCode, UiError, UiResult};
use crate::storage::StorageState;
use crate::studio::Resume;

/// Thumbnails of the local copies, made without the store's catalog: from a
/// content id alone (D-2026-09-30-storage-manager-10).
pub trait Pictures: Send + Sync {
    /// The PNG thumbnail of `content` (kept, or made from its copy with
    /// `media`); `None` when there is no copy, or no ffmpeg for a video.
    fn thumbnail(
        &self,
        content: &ContentId,
        media: &mut dyn MediaTranscoder,
    ) -> bezel_core::Result<Option<Vec<u8>>>;
}

impl Pictures for DiskArchive {
    fn thumbnail(
        &self,
        content: &ContentId,
        media: &mut dyn MediaTranscoder,
    ) -> bezel_core::Result<Option<Vec<u8>>> {
        DiskArchive::thumbnail(self, content, media)
    }
}

/// No thumbnails: a store in memory keeps none.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPictures;

impl Pictures for NoPictures {
    fn thumbnail(
        &self,
        _: &ContentId,
        _: &mut dyn MediaTranscoder,
    ) -> bezel_core::Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

/// Where the catalog and the local copies of what the studio sends live,
/// and their thumbnails.
pub struct Copies {
    /// The catalog and the copies.
    pub store: Box<dyn ArchiveStore>,
    /// Their thumbnails.
    pub pictures: Box<dyn Pictures>,
}

impl Copies {
    /// In a folder (`<data>/bezel/storage`), shared with the CLI.
    pub fn on_disk(archive: DiskArchive) -> Self {
        Self {
            pictures: Box::new(archive.clone()),
            store: Box::new(archive),
        }
    }

    /// In memory, for this run only (tests, or when the folder cannot be
    /// made).
    pub fn in_memory(store: MemoryArchive) -> Self {
        Self {
            store: Box::new(store),
            pictures: Box::new(NoPictures),
        }
    }
}

/// A plan waiting for the user's confirmation.
#[derive(Debug)]
pub(crate) struct PendingPlan {
    pub(crate) ticket: u64,
    pub(crate) screen: String,
    pub(crate) plan: TransferPlan,
}

/// A file the last overview listed, as the thumbnails and the originals'
/// search need it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ShownFile {
    size: Option<u64>,
    content: Option<ContentId>,
}

/// What the last overview listed, by path, so that thumbnails and the
/// originals' search do not ask the screen again.
#[derive(Debug, Default)]
pub(crate) struct Shown {
    screen: String,
    files: BTreeMap<String, ShownFile>,
}

impl Shown {
    fn of(screen: &str, inventory: &Inventory) -> Self {
        let files = inventory.overview.listed.iter().map(|listed| {
            let file = ShownFile {
                size: Inventory::size(listed),
                content: listed.entry.as_ref().map(|e| e.content.clone()),
            };
            (listed.file.path.to_string(), file)
        });
        Self {
            screen: screen.to_string(),
            files: files.collect(),
        }
    }

    fn file(&self, screen: &str, path: &str) -> Option<&ShownFile> {
        (self.screen == screen)
            .then(|| self.files.get(path))
            .flatten()
    }

    fn set_content(&mut self, screen: &str, path: &RemotePath, content: ContentId) {
        if self.screen == screen
            && let Some(file) = self.files.get_mut(&path.to_string())
        {
            file.content = Some(content);
        }
    }
}

/// The video a theme plays as its background.
fn video_of(theme: &Theme) -> Option<AssetRef> {
    match &theme.background {
        Background::Video { asset, .. } => Some(asset.clone()),
        Background::DeviceVideo { path, .. } => Some(AssetRef(format!("screen://{path}"))),
        Background::Color(_) | Background::Image { .. } => None,
    }
}

/// What the manager protects on a screen of `model`: the boot media Bezel
/// recorded and every name `videos` may have there.
fn protected(boot: Option<RemotePath>, videos: &[AssetRef], model: &DeviceModel) -> Protected {
    let mut protected = Protected::new(boot);
    if let Some(profile) = UploadProfile::for_model(model) {
        for video in videos {
            protected.theme_video(video, &profile);
        }
    }
    protected
}

/// A PNG as a `data:` URL.
fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

/// What a manager command works with on one screen: its link, the store and
/// the videos themes play.
pub(crate) struct Desk<'a> {
    link: &'a mut dyn ScreenLink,
    store: &'a mut dyn ArchiveStore,
    videos: &'a [AssetRef],
}

impl Desk<'_> {
    /// The manager of the screen, protecting the theme videos.
    fn manager(&mut self) -> Manager<'_> {
        Manager::new(&mut *self.link, &mut *self.store).protecting(self.videos.iter().cloned())
    }

    fn model(&self) -> &'static DeviceModel {
        self.link.identity().model
    }

    /// Both media next to the catalog, with the cleanup findings (none on a
    /// screen that cannot delete) and what the manager protects.
    fn overview(&mut self, screen: &str) -> UiResult<(ManagerOverviewDto, Shown)> {
        let model = self.model();
        let deletes = manager::deletes(&*self.link);
        let (inventory, findings) = match deletes {
            Deletes::Supported => {
                let cleanup = self.manager().cleanup()?;
                (cleanup.inventory, cleanup.findings)
            }
            Deletes::Unsupported => (self.manager().inventory()?, Vec::new()),
        };
        let protected = protected(inventory.boot().cloned(), self.videos, model);
        let cap = UploadProfile::for_model(model).map_or(0, |p| p.max_upload_bytes);
        let dto = overview_dto(&inventory, &findings, &protected, cap);
        Ok((dto, Shown::of(screen, &inventory)))
    }
}

fn overview_dto(
    inventory: &Inventory,
    findings: &[Finding],
    protected: &Protected,
    cap: u64,
) -> ManagerOverviewDto {
    let catalog = &inventory.catalog;
    let finding = |path: &RemotePath| findings.iter().find(|f| f.file.path == *path);
    let files = inventory.overview.listed.iter().map(|listed| {
        ManagedFileDto::listed(listed, catalog, finding(&listed.file.path), protected)
    });
    let missing = restorable(catalog, &inventory.overview.missing, false);
    let elsewhere = restorable(catalog, &inventory.overview.other_card, true);
    let deleted = deleted_with_copies(inventory)
        .map(|e| RestorableDto::of(e, catalog, on_another_card(e, inventory)));
    ManagerOverviewDto {
        internal: inventory.info.internal.into(),
        card: inventory.info.card.map(Into::into),
        files: files.collect(),
        folder_errors: Vec::new(),
        restorable: missing.chain(elsewhere).chain(deleted).collect(),
        deletes: inventory.deletes == Deletes::Supported,
        cap,
        cache: catalog.cache().into(),
    }
}

fn restorable<'a>(
    catalog: &'a Catalog,
    entries: &'a [ArchiveEntry],
    other_card: bool,
) -> impl Iterator<Item = RestorableDto> + 'a {
    entries
        .iter()
        .map(move |e| RestorableDto::of(e, catalog, other_card))
}

/// The files deleted through Bezel whose local copies are held: a restore
/// offers them too, never chosen by default (D-2026-09-30-storage-manager-6,
/// -8; the CLI's `restore internal|sd NAME`).
fn deleted_with_copies(inventory: &Inventory) -> impl Iterator<Item = &ArchiveEntry> {
    let entries = inventory.record().map_or(&[][..], |r| r.entries.as_slice());
    let deleted = entries.iter().filter(|e| e.state == EntryState::Deleted);
    deleted.filter(|e| inventory.has_copy(e))
}

/// Whether `entry` was on a card that is not the inserted one.
fn on_another_card(entry: &ArchiveEntry, inventory: &Inventory) -> bool {
    entry.path.location.medium == Medium::Card && entry.card != inventory.listing.card
}

impl StorageState {
    /// Remembers what an overview of `screen` listed.
    fn show(&self, shown: Shown) {
        *self.shown() = shown;
    }
}

impl Backend {
    /// Every video a theme plays: the library's and the edited (or live)
    /// theme's, protected from the cleanup assistant and warned about when
    /// renamed (D-2026-09-30-storage-manager-7, -9).
    pub(crate) fn theme_videos(&self) -> Vec<AssetRef> {
        let mut videos = self.library.videos();
        videos.extend(video_of(self.studio().theme()));
        videos
    }

    /// Runs `work` on `screen` as the one storage operation, with the store.
    pub(crate) fn with_desk<T>(
        &self,
        screen: &str,
        resume: Resume,
        time: LocalTime,
        work: impl FnOnce(&mut Desk<'_>) -> UiResult<T>,
    ) -> UiResult<T> {
        let videos = self.theme_videos();
        let _claim = self.storage.claim()?;
        self.on_screen(screen, resume, time, |link| {
            let mut store = self.storage.archive();
            let mut desk = Desk {
                link,
                store: store.as_mut(),
                videos: &videos,
            };
            work(&mut desk)
        })?
    }

    /// Both media of `screen` next to the catalog: files with their entry,
    /// finding and protection, the restorable entries, whether the screen
    /// deletes, its per-file limit and the local copies. Only queries the
    /// screen; the catalog is saved reconciled with what it lists.
    pub fn manager_overview(&self, screen: &str, time: LocalTime) -> UiResult<ManagerOverviewDto> {
        let (dto, shown) =
            self.with_desk(screen, Resume::Frames, time, |desk| desk.overview(screen))?;
        self.storage.show(shown);
        Ok(dto)
    }

    /// The thumbnail of a file the last overview of `screen` listed with a
    /// catalog entry, as a PNG `data:` URL; `None` without one (no entry,
    /// no copy, no ffmpeg for a video, or a copy that cannot be read).
    /// Neither the screen nor the claim is needed: it runs during jobs.
    pub fn manager_thumbnail(&self, screen: &str, path: &str) -> Option<String> {
        let content = self.storage.shown().file(screen, path)?.content.clone()?;
        let mut media = self.storage.picture_media();
        match self.storage.pictures().thumbnail(&content, media.as_mut()) {
            Ok(png) => png.map(|png| data_url(&png)),
            Err(_) => {
                diag::report(DiagCode::NoFileThumbnail);
                None
            }
        }
    }

    // ------------------------------------------------------------- cache --

    /// The local copies in numbers.
    pub fn cache_info(&self) -> UiResult<CacheDto> {
        let _claim = self.storage.claim()?;
        let mut store = self.storage.archive();
        Ok(manager::cache_info(store.as_mut())?.into())
    }

    /// "Clear cache" (D-2026-09-30-storage-manager-6): the copies of files
    /// deleted through Bezel (`deleted`) or every copy (`all`); entries and
    /// thumbnails stay. `confirm` is the answer to the dialog that listed
    /// the number of copies and their size.
    pub fn clear_cache(&self, scope: &str, confirm: Confirm) -> UiResult<ClearedDto> {
        let scope = match scope {
            "deleted" => Clear::Deleted,
            "all" => Clear::All,
            other => {
                let detail = format!("cache scope \"{other}\" (deleted or all)");
                return Err(UiError::new(ErrorCode::InvalidInput).arg("detail", detail));
            }
        };
        let _claim = self.storage.claim()?;
        let mut store = self.storage.archive();
        let Cleared { copies, bytes } = manager::clear_cache(store.as_mut(), scope, confirm)?;
        Ok(ClearedDto {
            removed: copies,
            bytes,
        })
    }

    /// Sets the limit of the copies of deleted files (bytes, more than 0);
    /// the oldest of them over it go.
    pub fn set_cache_limit(&self, bytes: u64) -> UiResult<CacheDto> {
        if bytes == 0 {
            let detail = "a cache limit of 0 bytes";
            return Err(UiError::new(ErrorCode::InvalidInput).arg("detail", detail));
        }
        let _claim = self.storage.claim()?;
        let mut store = self.storage.archive();
        manager::set_cache_limit(store.as_mut(), bytes)?;
        Ok(manager::cache_info(store.as_mut())?.into())
    }
}
