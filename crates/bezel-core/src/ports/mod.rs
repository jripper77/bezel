//! Ports: the traits adapters implement (driven) or call (driving).

use crate::Result;
use crate::domain::clock::{Language, LocalTime};
use crate::domain::discovery::{Endpoint, Screen};
use crate::domain::frame::Frame;
use crate::domain::geometry::Orientation;
use crate::domain::history::Histories;
use crate::domain::screen::{Brightness, ScreenIdentity};
use crate::domain::sensor::{SensorInfo, Snapshot};
use crate::domain::theme::{AssetRef, Theme};
use std::collections::BTreeMap;

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

/// Driven port: measures the machine. Adapters time their own samples (rates
/// are per second of real elapsed time between two `sample` calls).
pub trait SensorSource: Send {
    /// The sensors this machine offers right now.
    fn catalog(&mut self) -> Result<Vec<SensorInfo>>;
    /// Current readings of every sensor in the catalog.
    fn sample(&mut self) -> Result<Snapshot>;
}

/// Everything a frame depends on besides the theme.
#[derive(Debug, Clone, Copy)]
pub struct RenderContext<'a> {
    /// Current readings.
    pub snapshot: &'a Snapshot,
    /// Graph histories.
    pub histories: &'a Histories,
    /// Local wall-clock time for clock elements.
    pub time: LocalTime,
    /// Language of day and month names.
    pub language: Language,
}

/// Driven port: draws a theme into a frame of its canvas size.
pub trait FrameRenderer: Send {
    /// Renders `theme` with `assets` in `context`.
    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame>;
}

/// Where a theme lives for a [`ThemeStore`] (a file or folder for disk stores).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThemeLocation(pub String);

/// Driven port: reads and writes themes with their assets.
pub trait ThemeStore {
    /// Loads a theme and its assets.
    fn load(&self, location: &ThemeLocation) -> Result<(Theme, BTreeMap<AssetRef, Vec<u8>>)>;
    /// Saves a theme and its assets.
    fn save(
        &self,
        location: &ThemeLocation,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
    ) -> Result<()>;
}
