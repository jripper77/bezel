//! Bezel core: the domain of USB "smart screens", the ports the rest of the
//! application plugs into, and the use cases built on them.
//!
//! This crate is the hexagon: it performs no I/O, spawns no thread, reads no
//! clock and has no platform-specific code. Serial ports, USB, sensors,
//! rendering and files are adapters that implement [`ports`].
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod app;
pub mod domain;
pub mod ports;

pub use domain::error::BezelError;

/// Result alias used across the core.
pub type Result<T> = std::result::Result<T, BezelError>;
