//! Driven adapters that reach smart screens: USB/serial/HID discovery,
//! transports and wire protocols, and the Linux udev rule they need.
//!
//! Every byte this crate sends to a screen is specified in
//! `docs/reverse-engineering/`.
#![forbid(unsafe_code)]

pub mod busy;
pub mod connector;
pub mod discovery;
pub mod driver;
pub mod fake;
pub mod hid_desktop;
pub mod protocol;
pub mod udev;
pub mod usb;
pub mod wire;

pub use connector::SystemConnector;
pub use discovery::SystemBus;
pub use fake::{FakeBus, FakeConnector, FakeHid};
pub use hid_desktop::SystemHid;
