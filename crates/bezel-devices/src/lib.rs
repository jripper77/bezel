//! Driven adapters that reach smart screens: USB/serial discovery today;
//! transports and wire protocols as the roadmap adds them.
//!
//! Every byte this crate sends to a screen is specified in
//! `docs/reverse-engineering/`.
#![forbid(unsafe_code)]

pub mod discovery;
pub mod fake;

pub use discovery::SystemBus;
pub use fake::FakeBus;
