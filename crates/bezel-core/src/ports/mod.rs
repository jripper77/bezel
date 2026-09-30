//! Ports: the traits adapters implement (driven) or call (driving).

use crate::Result;
use crate::domain::discovery::{Endpoint, Screen};
use crate::domain::frame::Frame;
use crate::domain::geometry::Orientation;
use crate::domain::screen::{Brightness, ScreenIdentity};

/// Driven port: enumerates the USB endpoints the host can see, without
/// opening or writing to any of them.
pub trait DeviceBus {
    /// Every candidate endpoint currently connected. Adapters may pre-filter to
    /// the catalog's USB ids; unknown endpoints are ignored by the core anyway.
    fn endpoints(&self) -> Result<Vec<Endpoint>>;
}

/// Driven port: opens a discovered screen (waking it when needed) and
/// performs the handshake.
pub trait ScreenConnector {
    /// A live link to `screen`.
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>>;
}

/// Driven port: one connected screen. Frames go in the orientation the user
/// looks at; the adapter rotates and encodes them for the panel and decides
/// between a full frame and a partial update.
pub trait ScreenLink: Send {
    /// Who answered the handshake.
    fn identity(&self) -> &ScreenIdentity;
    /// Backlight level.
    fn set_brightness(&mut self, brightness: Brightness) -> Result<()>;
    /// Orientation of the frames that follow.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()>;
    /// Shows `frame`, whose size must be the panel size in the current orientation.
    fn present(&mut self, frame: &Frame) -> Result<()>;
    /// Turns the panel off until the next frame.
    fn screen_off(&mut self) -> Result<()>;
    /// Hands the screen back to its standalone mode (clock, stored media).
    fn release(&mut self) -> Result<()>;
}
