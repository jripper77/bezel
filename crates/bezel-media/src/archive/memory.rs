//! An [`ArchiveStore`] in memory: the fake behind the tests of the core's
//! use cases, the CLI's `--fake` mode and the studio backend.

use std::collections::BTreeMap;

use bezel_core::Result;
use bezel_core::domain::archive::{Catalog, ContentId};
use bezel_core::ports::ArchiveStore;

use super::content_id;

/// Keeps the catalog and the copies in memory, as the disk store keeps
/// them in files: the last saved catalog, one copy per content.
#[derive(Debug, Clone, Default)]
pub struct MemoryArchive {
    saved: Option<Catalog>,
    copies: BTreeMap<ContentId, Vec<u8>>,
    saves: usize,
}

impl MemoryArchive {
    /// An empty store: nothing saved, no copy.
    pub fn new() -> Self {
        Self::default()
    }

    /// A store holding `catalog` as if it had been saved before.
    pub fn with_catalog(catalog: Catalog) -> Self {
        Self {
            saved: Some(catalog),
            ..Self::default()
        }
    }

    /// The catalog last saved.
    pub fn saved(&self) -> Option<&Catalog> {
        self.saved.as_ref()
    }

    /// How many times the catalog was saved.
    pub fn saves(&self) -> usize {
        self.saves
    }

    /// How many copies are kept.
    pub fn copies(&self) -> usize {
        self.copies.len()
    }

    /// Whether a copy of `content` is kept.
    pub fn holds(&self, content: &ContentId) -> bool {
        self.copies.contains_key(content)
    }
}

impl ArchiveStore for MemoryArchive {
    fn load(&mut self) -> Result<Catalog> {
        Ok(self.saved.clone().unwrap_or_default())
    }

    fn save(&mut self, catalog: &Catalog) -> Result<()> {
        self.saved = Some(catalog.clone());
        self.saves += 1;
        Ok(())
    }

    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId> {
        let id = content_id(bytes);
        self.copies
            .entry(id.clone())
            .or_insert_with(|| bytes.to_vec());
        Ok(id)
    }

    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        Ok(self.copies.get(content).cloned())
    }

    fn discard(&mut self, content: &ContentId) -> Result<()> {
        self.copies.remove(content);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::archive::{DEFAULT_CACHE_LIMIT, ScreenKey};
    use bezel_core::domain::device::ModelId;

    #[test]
    fn same_bytes_are_kept_once() {
        let mut store = MemoryArchive::new();
        let clip = b"hello world".to_vec();
        let first = store.keep(&clip).unwrap();
        assert_eq!(
            first.as_str(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            "the SHA-256 of the bytes"
        );
        // Sent to the internal flash and to the card: one copy.
        assert_eq!(store.keep(&clip).unwrap(), first);
        assert_eq!(store.copies(), 1);
        let other = store.keep(b"hello world!").unwrap();
        assert_ne!(other, first);
        assert_eq!(store.copies(), 2);
        assert_eq!(store.read(&first).unwrap(), Some(clip));
        assert_eq!(content_id(b"hello world"), first);

        // Discarding drops the copy, once; an absent copy reads as none.
        store.discard(&first).unwrap();
        store.discard(&first).unwrap();
        assert!(!store.holds(&first) && store.holds(&other));
        assert_eq!(store.read(&first).unwrap(), None);

        // The catalog: the default until saved, then what was saved.
        assert_eq!(store.load().unwrap().limit, DEFAULT_CACHE_LIMIT);
        assert!(store.saved().is_none());
        let mut catalog = Catalog::default();
        catalog.copies.insert(other.clone());
        catalog.screen_mut(&ScreenKey::new(ModelId("turing-8.8")));
        store.save(&catalog).unwrap();
        assert_eq!(store.load().unwrap(), catalog);
        assert_eq!(store.saves(), 1);
        let reopened = MemoryArchive::with_catalog(catalog.clone());
        assert_eq!(reopened.saved(), Some(&catalog));
        assert_eq!(reopened.copies(), 0);
    }
}
