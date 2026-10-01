//! The user's collection of GIFs and stickers, behind the core's
//! [`GifCollection`](bezel_core::ports::GifCollection) port
//! (D-2026-10-01-gif-sticker-search-5), and the fake GIF provider behind its
//! [`GifSource`](bezel_core::ports::GifSource) port.
//!
//! GIFs are addressed by their SHA-256
//! ([`content_id`](crate::archive::content_id)), so the same bytes are kept
//! once. [`MemoryCollection`] keeps the index, the GIFs and their previews
//! in memory and records every call; [`FakeGifSource`] answers scripted
//! pages and files and records every request: the fakes the core's and the
//! studio's tests run through.

mod fake;
mod memory;

pub use fake::{FakeGifSource, GifCall};
pub use memory::{CollectionCall, MemoryCollection};
