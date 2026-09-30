//! Screen discovery and selection.

use crate::domain::discovery::{Screen, ScreenState, group_screens};
use crate::ports::{DeviceBus, ScreenConnector, ScreenLink};
use crate::{BezelError, Result};

/// Lists the connected screens, grouping each screen's endpoints.
pub fn discover_screens<B: DeviceBus + ?Sized>(bus: &B) -> Result<Vec<Screen>> {
    Ok(group_screens(bus.endpoints()?))
}

/// Picks a screen: the one whose display or wake endpoint has `address`, or
/// else the first awake screen, or else the first one.
pub fn choose_screen(screens: Vec<Screen>, address: Option<&str>) -> Result<Screen> {
    let matches = |s: &Screen| {
        [&s.display, &s.wake]
            .into_iter()
            .flatten()
            .any(|e| Some(e.address.0.as_str()) == address)
    };
    match address {
        Some(a) => screens
            .into_iter()
            .find(matches)
            .ok_or_else(|| BezelError::ScreenNotFound(a.to_string())),
        None => {
            let awake = screens.iter().position(|s| s.state() == ScreenState::Awake);
            let index = awake.unwrap_or(0);
            screens
                .into_iter()
                .nth(index)
                .ok_or_else(|| BezelError::ScreenNotFound("no smart screen connected".into()))
        }
    }
}

/// Discovers, chooses and connects a screen.
pub fn open_screen<B, C>(
    bus: &B,
    connector: &C,
    address: Option<&str>,
) -> Result<Box<dyn ScreenLink>>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let screen = choose_screen(discover_screens(bus)?, address)?;
    connector.connect(&screen)
}
