//! Theme files: the native `.bezeltheme` format (a zip, or the same layout as
//! a folder) and importers for other apps' themes (turing-smart-screen-python
//! YAML folders and the vendor app's `.turtheme` files).
//!
//! Serialization types (DTOs) live here, never in the core: the core model can
//! evolve while `schema` versions keep old files readable.
#![forbid(unsafe_code)]

pub mod color;
pub mod dto;
pub mod import;
pub mod native;

pub use native::FsThemeStore;
