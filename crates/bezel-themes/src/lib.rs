//! Theme files: the native `.bezeltheme` format (a zip, or the same layout as
//! a folder) and, later, importers for other apps' themes.
//!
//! Serialization types (DTOs) live here, never in the core: the core model can
//! evolve while `schema` versions keep old files readable.
#![forbid(unsafe_code)]

pub mod color;
pub mod dto;
pub mod native;

pub use native::FsThemeStore;
