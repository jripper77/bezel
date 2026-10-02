//! The user's collection of GIFs and stickers, behind the core's
//! [`GifCollection`](bezel_core::ports::GifCollection) port
//! (D-2026-10-01-gif-sticker-search-5), and the fake GIF provider behind its
//! [`GifSource`](bezel_core::ports::GifSource) port.
//!
//! GIFs are addressed by their SHA-256
//! ([`content_id`](crate::archive::content_id)), so the same bytes are kept
//! once. [`DiskCollection`] keeps them in the user's data folder
//! ([`collection_dir`]), with the index and the previews;
//! [`MemoryCollection`] keeps the index, the GIFs and their previews in
//! memory and records every call; [`FakeGifSource`] answers scripted pages
//! and files and records every request: the fakes the core's and the
//! studio's tests run through.

mod disk;
mod fake;
mod memory;

#[cfg(test)]
mod tests;

pub use disk::DiskCollection;
pub use fake::{FakeGifSource, GifCall};
pub use memory::{CollectionCall, MemoryCollection};

use std::path::{Path, PathBuf};

/// The collection's folder, `<data>/bezel/collection`, in the user's data
/// folder `data` that the CLI and the studio both resolve, as for
/// [`storage_dir`](crate::archive::storage_dir).
pub fn collection_dir(data: &Path) -> PathBuf {
    data.join("bezel").join("collection")
}
