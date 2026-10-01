//! The catalog's bookkeeping. Every change loads the catalog from the store,
//! applies itself and saves it at once (the CLI and the studio share it);
//! the copies the store holds follow [`Catalog::copies`]: a copy is kept
//! before the catalog names it and discarded only after the catalog that no
//! longer names it was saved, so a failure leaves at worst a stray file,
//! never a catalog naming a copy that is gone.

use crate::domain::archive::{ArchiveEntry, Catalog, ContentId, EntryState, ScreenKey};
use crate::domain::storage::RemotePath;
use crate::ports::ArchiveStore;
use crate::{BezelError, Result};

/// A catalog being changed, and the copies the change lets go.
pub(super) struct Edit {
    /// The catalog as loaded.
    pub catalog: Catalog,
    dropped: Vec<ContentId>,
}

impl Edit {
    /// Lets the copy of `content` go when no entry names it any more.
    pub fn release(&mut self, content: &ContentId) {
        if !self.catalog.is_referenced(content) && self.catalog.copies.remove(content) {
            self.dropped.push(content.clone());
        }
    }

    /// Drops the copies of deleted files over the limit, oldest first.
    pub fn evict(&mut self) {
        let gone = self.catalog.evict();
        self.dropped.extend(gone);
    }

    /// Lets go of copies the catalog already dropped.
    pub fn dropped(&mut self, gone: Vec<ContentId>) {
        self.dropped.extend(gone);
    }
}

/// Loads the catalog, applies `change`, saves it, then discards the copies
/// the change let go.
pub(super) fn change<T>(
    store: &mut dyn ArchiveStore,
    change: impl FnOnce(&mut Edit) -> T,
) -> Result<T> {
    let mut edit = Edit {
        catalog: store.load()?,
        dropped: Vec::new(),
    };
    let out = change(&mut edit);
    store.save(&edit.catalog)?;
    for content in &edit.dropped {
        store.discard(content)?;
    }
    Ok(out)
}

/// An entry just recorded, and the one it replaced: what settling an upload
/// needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Recorded {
    /// The entry as recorded.
    pub entry: ArchiveEntry,
    /// The entry it replaced at its place, if any.
    pub replaced: Option<ArchiveEntry>,
}

/// Records `entry` as the newest of `key`'s: its copy is held (the store
/// keeps it already) and the copies of deleted files over the limit go.
pub(super) fn record(edit: &mut Edit, key: &ScreenKey, entry: ArchiveEntry) -> Recorded {
    edit.catalog.copies.insert(entry.content.clone());
    let replaced = edit.catalog.screen_mut(key).record(entry.clone());
    edit.evict();
    Recorded { entry, replaced }
}

/// Records `entry` before its first byte is sent (D-2026-09-30-storage-manager-5).
pub(super) fn begin(
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
    entry: ArchiveEntry,
) -> Result<Recorded> {
    change(store, |edit| record(edit, key, entry))
}

/// Settles a pending upload by its outcome: verified, it is stored and the
/// copy of the file it replaced goes when nothing else names it. Cancelled
/// with nothing left on the screen, it goes and the entry it replaced comes
/// back. Otherwise it stays pending: what the screen holds there is a
/// leftover the cleanup assistant pre-checks (D-2026-09-30-storage-manager-9).
pub(super) fn settle(
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
    pending: &Recorded,
    error: Option<&BezelError>,
) -> Result<()> {
    let (path, card) = (&pending.entry.path, pending.entry.card);
    change(store, |edit| {
        let record = edit.catalog.screen_mut(key);
        match error {
            None => {
                if let Some(entry) = record.entry_mut(path, card) {
                    entry.state = EntryState::Stored;
                }
            }
            Some(BezelError::Cancelled { partial: None }) => {
                record.forget(path, card);
                if let Some(old) = &pending.replaced {
                    record.record(old.clone());
                }
                edit.release(&pending.entry.content);
                return;
            }
            Some(_) => {}
        }
        if let Some(old) = &pending.replaced {
            edit.release(&old.content);
        }
    })
}

/// Marks the entry at `path` deleted through Bezel: from now on its copy
/// counts against the limit (D-2026-09-30-storage-manager-6).
pub(super) fn deleted(
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
    path: &RemotePath,
    card: Option<u64>,
) -> Result<()> {
    change(store, |edit| {
        let record = edit.catalog.screens.get_mut(key);
        if let Some(entry) = record.and_then(|r| r.entry_mut(path, card)) {
            entry.state = EntryState::Deleted;
        }
        edit.evict();
    })
}

/// Forgets the entry at `path` (a moved or renamed file's source): its copy
/// goes when nothing else names it.
pub(super) fn forget(
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
    path: &RemotePath,
    card: Option<u64>,
) -> Result<Option<ArchiveEntry>> {
    change(store, |edit| {
        let record = edit.catalog.screens.get_mut(key);
        let gone = record.and_then(|r| r.forget(path, card));
        if let Some(entry) = &gone {
            edit.release(&entry.content);
        }
        gone
    })
}
