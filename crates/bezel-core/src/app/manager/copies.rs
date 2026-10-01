//! The local copies: associating a screen file with its original on the PC
//! (D-2026-09-30-storage-manager-10), forgetting an entry, and the cache of
//! copies with its limit (D-2026-09-30-storage-manager-6).

use super::{Manager, ledger};
use crate::app::storage::{Presence, presence, profile_of, storage_of};
use crate::domain::archive::{
    ArchiveEntry, CacheInfo, Candidate, Clear, EntryState, ScreenKey, Sought, rank_candidates,
};
use crate::domain::media::MediaKind;
use crate::domain::screen::Confirm;
use crate::domain::storage::RemotePath;
use crate::ports::{ArchiveStore, MediaLocation, MediaTranscoder};
use crate::{BezelError, Result};

impl Manager<'_> {
    /// The files among `sources` that can be the original of the screen
    /// file at `path`, likeliest first ([`rank_candidates`]: exactly its
    /// size and kind, then name, resolution and duration). Sources that
    /// cannot be probed are left out. Only queries the screen.
    pub fn originals(
        &mut self,
        media: &mut dyn MediaTranscoder,
        path: &RemotePath,
        sources: &[MediaLocation],
    ) -> Result<Vec<Candidate>> {
        let profile = profile_of(self.link)?;
        let card = self.card_for(path.location.medium)?;
        let size = self.stored_size(path)?;
        let catalog = self.store.load()?;
        let entry = catalog.screen(&self.key).and_then(|r| r.entry(path, card));
        let sought = Sought {
            path,
            size,
            resolution: (path.location.kind == MediaKind::Video).then_some(profile.video_size),
            duration: entry.and_then(|e| e.duration),
        };
        let probed = sources.iter().filter_map(|source| {
            let media = media.probe(source).ok()?;
            let source = source.0.clone();
            Some(Candidate { source, media })
        });
        Ok(rank_candidates(&sought, probed.collect()))
    }

    /// Associates the screen file at `path` with its `original` on the PC,
    /// which the user confirmed: the original's bytes are kept as the file's
    /// local copy and it is cataloged as stored (movable, restorable, with a
    /// thumbnail). The original must have exactly the file's size and kind.
    /// `now` is the time recorded (seconds since the Unix epoch). Only
    /// queries the screen.
    pub fn associate(
        &mut self,
        media: &mut dyn MediaTranscoder,
        path: &RemotePath,
        original: &MediaLocation,
        now: u64,
    ) -> Result<ArchiveEntry> {
        let card = self.card_for(path.location.medium)?;
        let size = self.stored_size(path)?;
        let probed = media.probe(original)?;
        let kind = path.location.kind;
        if probed.bytes != size || probed.kind() != Some(kind) {
            return Err(BezelError::InvalidInput(format!(
                "{} is not a {} of {size} bytes like {path}",
                original.0,
                kind.slug()
            )));
        }
        let bytes = media.load(original)?;
        if bytes.len() as u64 != size {
            return Err(BezelError::InvalidInput(format!(
                "{} changed while it was associated",
                original.0
            )));
        }
        let content = self.store.keep(&bytes)?;
        let mut entry = ArchiveEntry::pending(path.clone(), card, size, content, now);
        entry.state = EntryState::Stored;
        entry.source = Some(original.0.clone());
        entry.duration = probed.video.and_then(|v| v.duration);
        entry.resolution = probed.dimensions;
        let key = &self.key;
        ledger::change(self.store, |edit| {
            let pending = ledger::record(edit, key, entry.clone());
            if let Some(old) = pending.replaced {
                edit.release(&old.content);
            }
        })?;
        Ok(entry)
    }

    /// The size of the file stored at `path`: refused when it is not there
    /// or the screen cannot tell its size.
    fn stored_size(&mut self, path: &RemotePath) -> Result<u64> {
        match presence(storage_of(self.link)?, path)? {
            Presence::Stored(Some(size)) => Ok(size),
            Presence::Stored(None) => Err(BezelError::Unsupported(format!(
                "the screen cannot tell the size of {path}"
            ))),
            Presence::Absent => Err(BezelError::InvalidInput(format!(
                "{path} is not stored on the screen"
            ))),
        }
    }
}

/// Forgets the entry at `path` of `key`'s record (on the card of capacity
/// `card`, ignored on the internal flash), deleted or not; its local copy
/// goes when no other entry names it. Nothing is sent to a screen.
pub fn forget(
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
    path: &RemotePath,
    card: Option<u64>,
) -> Result<Option<ArchiveEntry>> {
    ledger::forget(store, key, path, card)
}

/// The local copies in numbers, for "Clear cache" to list before it asks.
pub fn cache_info(store: &mut dyn ArchiveStore) -> Result<CacheInfo> {
    Ok(store.load()?.cache())
}

/// What clearing or limiting the cache removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cleared {
    /// Copies removed.
    pub copies: usize,
    /// Their bytes, as cataloged.
    pub bytes: u64,
}

/// "Clear cache": removes the copies `scope` names (the copies of files
/// deleted through Bezel, or all of them); the entries and thumbnails stay,
/// with no local copy. Needs `Confirm::Yes`; with `Confirm::No` nothing
/// changes.
pub fn clear_cache(
    store: &mut dyn ArchiveStore,
    scope: Clear,
    confirm: Confirm,
) -> Result<Cleared> {
    if confirm == Confirm::No {
        return Err(BezelError::NotConfirmed(
            "clearing the local copies".to_string(),
        ));
    }
    ledger::change(store, |edit| {
        let before = edit.catalog.cache().bytes;
        let gone = edit.catalog.clear(scope);
        let cleared = Cleared {
            copies: gone.len(),
            bytes: before.saturating_sub(edit.catalog.cache().bytes),
        };
        edit.dropped(gone);
        cleared
    })
}

/// Sets the size limit of the copies of deleted files, in bytes, and
/// evicts the oldest of them until they fit; copies of files still stored
/// or missing never go.
pub fn set_cache_limit(store: &mut dyn ArchiveStore, limit: u64) -> Result<Cleared> {
    ledger::change(store, |edit| {
        let before = edit.catalog.cache();
        edit.catalog.limit = limit;
        let gone = edit.catalog.evict();
        let cleared = Cleared {
            copies: gone.len(),
            bytes: before.bytes.saturating_sub(edit.catalog.cache().bytes),
        };
        edit.dropped(gone);
        cleared
    })
}
