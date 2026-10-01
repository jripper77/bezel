//! The local copies of what Bezel sends to screens, behind the core's
//! [`ArchiveStore`](bezel_core::ports::ArchiveStore) port
//! (D-2026-09-30-storage-manager-2, -5).
//!
//! Copies are addressed by their SHA-256 ([`content_id`]), so the same bytes
//! sent to the internal flash and to the card are kept once.
//! [`MemoryArchive`] keeps everything in memory: the fake the core, CLI and
//! studio tests run through.

mod memory;

pub use memory::MemoryArchive;

use bezel_core::domain::archive::ContentId;
use sha2::{Digest, Sha256};

/// The content id of `bytes`: their SHA-256.
pub fn content_id(bytes: &[u8]) -> ContentId {
    ContentId::from_digest(Sha256::digest(bytes).into())
}
