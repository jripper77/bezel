//! Driven adapters that reach smart screens: USB/serial discovery today;
//! transports and wire protocols as the roadmap adds them.
//!
//! Every byte this crate sends to a screen is specified in
//! `docs/reverse-engineering/`.
#![forbid(unsafe_code)]

pub mod busy;
pub mod connector;
pub mod discovery;
pub mod driver;
pub mod fake;
pub mod protocol;
pub mod usb;
pub mod wire;

pub use connector::SystemConnector;
pub use discovery::SystemBus;
pub use fake::{FakeBus, FakeConnector};
