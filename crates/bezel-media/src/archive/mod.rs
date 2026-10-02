//! The local copies of what Bezel sends to screens, behind the core's
//! [`ArchiveStore`](bezel_core::ports::ArchiveStore) port
//! (D-2026-09-30-storage-manager-2, -5, -10).
//!
//! Copies are addressed by their SHA-256 ([`content_id`]), so the same bytes
//! sent to the internal flash and to the card are kept once.
//! [`DiskArchive`] keeps them in the user's data folder ([`storage_dir`]),
//! with the catalog and the thumbnails; [`MemoryArchive`] keeps everything in
//! memory: the fake the core, CLI and studio tests run through.

mod disk;
mod memory;
mod thumbs;

#[cfg(test)]
mod tests;

pub use disk::DiskArchive;
// The atomic replace, the removal and the I/O error the GIF collection reuses.
pub(crate) use disk::{failed, remove, write_atomically};
pub use memory::MemoryArchive;
pub use thumbs::THUMBNAIL_EDGE;

use std::path::{Path, PathBuf};

use bezel_core::domain::archive::ContentId;
use sha2::{Digest, Sha256};

/// The content id of `bytes`: their SHA-256.
pub fn content_id(bytes: &[u8]) -> ContentId {
    ContentId::from_digest(Sha256::digest(bytes).into())
}

/// The store's folder, `<data>/bezel/storage`, in the user's data folder
/// `data` that the CLI and the studio both resolve, as for `bezel/themes`:
/// `$XDG_DATA_HOME` or `~/.local/share` on Linux, `%APPDATA%` on Windows
/// (the CLI's `data_home`, Tauri's `data_dir`).
pub fn storage_dir(data: &Path) -> PathBuf {
    data.join("bezel").join("storage")
}
