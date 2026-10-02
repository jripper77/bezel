//! A [`GifCollection`] in memory: the fake behind the tests of the core's
//! GIF use cases and of the studio's commands.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bezel_core::Result;
use bezel_core::domain::archive::ContentId;
use bezel_core::domain::gifs::Collection;
use bezel_core::ports::GifCollection;

use crate::archive::content_id;

/// A call the collection received, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionCall {
    /// [`GifCollection::load`].
    Load,
    /// [`GifCollection::save`].
    Save,
    /// [`GifCollection::keep`] of bytes whose id is this.
    Keep(ContentId),
    /// [`GifCollection::read`].
    Read(ContentId),
    /// [`GifCollection::keep_preview`].
    KeepPreview(ContentId),
    /// [`GifCollection::read_preview`].
    ReadPreview(ContentId),
    /// [`GifCollection::discard`].
    Discard(ContentId),
}

#[derive(Debug, Default)]
struct State {
    saved: Option<Collection>,
    files: BTreeMap<ContentId, Vec<u8>>,
    previews: BTreeMap<ContentId, Vec<u8>>,
    calls: Vec<CollectionCall>,
}

/// Keeps the index, the GIFs and their previews in memory, as the disk
/// collection keeps them in files: the last saved index, one GIF per
/// content. Clones share the same collection, so a test keeps one to look
/// inside what it handed over.
#[derive(Debug, Clone, Default)]
pub struct MemoryCollection {
    state: Arc<Mutex<State>>,
}

impl MemoryCollection {
    /// An empty collection: no index saved, nothing kept.
    pub fn new() -> Self {
        Self::default()
    }

    /// A collection holding `index` as if it had been saved before (its
    /// GIFs are not kept).
    pub fn with_index(index: Collection) -> Self {
        let collection = Self::new();
        collection.state().saved = Some(index);
        collection
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record(&self, call: CollectionCall) -> MutexGuard<'_, State> {
        let mut state = self.state();
        state.calls.push(call);
        state
    }

    /// The index last saved.
    pub fn saved(&self) -> Option<Collection> {
        self.state().saved.clone()
    }

    /// How many times the index was saved.
    pub fn saves(&self) -> usize {
        let state = self.state();
        state
            .calls
            .iter()
            .filter(|c| **c == CollectionCall::Save)
            .count()
    }

    /// How many GIFs are kept.
    pub fn files(&self) -> usize {
        self.state().files.len()
    }

    /// Whether a GIF is kept as `content`.
    pub fn holds(&self, content: &ContentId) -> bool {
        self.state().files.contains_key(content)
    }

    /// The preview kept for `content`.
    pub fn preview_of(&self, content: &ContentId) -> Option<Vec<u8>> {
        self.state().previews.get(content).cloned()
    }

    /// Every call received, in order.
    pub fn calls(&self) -> Vec<CollectionCall> {
        self.state().calls.clone()
    }
}

impl GifCollection for MemoryCollection {
    fn load(&mut self) -> Result<Collection> {
        let state = self.record(CollectionCall::Load);
        Ok(state.saved.clone().unwrap_or_default())
    }

    fn save(&mut self, index: &Collection) -> Result<()> {
        self.record(CollectionCall::Save).saved = Some(index.clone());
        Ok(())
    }

    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId> {
        let id = content_id(bytes);
        let mut state = self.record(CollectionCall::Keep(id.clone()));
        state
            .files
            .entry(id.clone())
            .or_insert_with(|| bytes.to_vec());
        Ok(id)
    }

    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        let state = self.record(CollectionCall::Read(content.clone()));
        Ok(state.files.get(content).cloned())
    }

    fn keep_preview(&mut self, content: &ContentId, bytes: &[u8]) -> Result<()> {
        let mut state = self.record(CollectionCall::KeepPreview(content.clone()));
        state.previews.insert(content.clone(), bytes.to_vec());
        Ok(())
    }

    fn read_preview(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        let state = self.record(CollectionCall::ReadPreview(content.clone()));
        Ok(state.previews.get(content).cloned())
    }

    fn discard(&mut self, content: &ContentId) -> Result<()> {
        let mut state = self.record(CollectionCall::Discard(content.clone()));
        state.files.remove(content);
        state.previews.remove(content);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::gifs::{CollectedGif, GifKind, GifOrigin};

    #[test]
    fn keeps_gifs_once_with_their_previews_and_records_calls() {
        let mut collection = MemoryCollection::new();
        let watcher = collection.clone();
        assert_eq!(collection.load().unwrap(), Collection::default());
        let id = collection.keep(b"GIF89a one").unwrap();
        assert_eq!(collection.keep(b"GIF89a one").unwrap(), id);
        assert_eq!(watcher.files(), 1, "a clone sees the same collection");
        collection.keep_preview(&id, b"GIF89a small").unwrap();
        assert_eq!(collection.read(&id).unwrap(), Some(b"GIF89a one".to_vec()));
        assert_eq!(
            collection.read_preview(&id).unwrap(),
            Some(b"GIF89a small".to_vec())
        );

        let item = CollectedGif {
            content: id.clone(),
            name: "one".into(),
            kind: GifKind::Gif,
            width: 1,
            height: 1,
            bytes: 10,
            added_at: 0,
            origin: GifOrigin {
                provider: "fake".into(),
                id: "1".into(),
                page_url: None,
            },
        };
        let index = Collection::new(vec![item]);
        collection.save(&index).unwrap();
        assert_eq!(watcher.saved(), Some(index.clone()));
        assert_eq!(watcher.saves(), 1);
        assert_eq!(collection.load().unwrap(), index);

        // Discarding takes the GIF and its preview, once.
        collection.discard(&id).unwrap();
        collection.discard(&id).unwrap();
        assert!(!watcher.holds(&id));
        assert_eq!(watcher.preview_of(&id), None);
        assert_eq!(collection.read(&id).unwrap(), None);
        assert_eq!(collection.read_preview(&id).unwrap(), None);
        assert_eq!(watcher.calls().first(), Some(&CollectionCall::Load));
        assert!(
            watcher
                .calls()
                .contains(&CollectionCall::Discard(id.clone()))
        );

        let reopened = MemoryCollection::with_index(index.clone());
        assert_eq!(reopened.saved(), Some(index));
        assert_eq!((reopened.files(), reopened.saves()), (0, 0));
    }
}
