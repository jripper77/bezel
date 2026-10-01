//! Screen discovery and selection, and the switch of a panel in desktop
//! mode back to USB monitor mode.

use crate::domain::device::Family;
use crate::domain::discovery::{
    DesktopModePanel, Discovery, MonitorModeConfirmed, MonitorModeSwitch, Screen, ScreenState,
    desktop_mode_panels, find_again, group_devices, group_screens,
};
use crate::domain::screen::Confirm;
use crate::ports::{DesktopModeHid, DeviceBus, ScreenConnector, ScreenLink};
use crate::{BezelError, Result};

/// Lists the connected screens, grouping each screen's endpoints.
pub fn discover_screens<B: DeviceBus + ?Sized>(bus: &B) -> Result<Vec<Screen>> {
    Ok(group_screens(bus.endpoints()?))
}

/// Lists the connected screens and the panels in desktop mode, from one
/// enumeration. Read-only: nothing is sent to any of them.
pub fn discover_devices<B: DeviceBus + ?Sized>(bus: &B) -> Result<Discovery> {
    Ok(group_devices(bus.endpoints()?))
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

/// Opens `known` again after its live link failed (T-7.11): finds it on
/// the bus by identity ([`find_again`]: a rev C SoC comes back under a new
/// device name) and connects it, which restarts a hung rev C screen through
/// its MCU (D-2026-09-30-release-polish-13). Returns the screen as the bus
/// lists it now, with the link. `ScreenNotFound` while it is not back.
pub fn reopen_screen<B, C>(
    bus: &B,
    connector: &C,
    known: &Screen,
) -> Result<(Screen, Box<dyn ScreenLink>)>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let screen = find_again(discover_screens(bus)?, known).ok_or_else(|| {
        let address = known.address().map_or_else(|| "?".into(), |a| a.0.clone());
        BezelError::ScreenNotFound(format!("{address} is not back on the bus"))
    })?;
    let link = connector.connect(&screen)?;
    // Connecting may have woken it, or restarted it under a new device
    // name: the bus lists its display now (on the 8.8" the live log named
    // the MCU's port when the screen was found while its SoC was away).
    let now = discover_screens(bus)
        .ok()
        .and_then(|screens| find_again(screens, &screen))
        .unwrap_or(screen);
    Ok((now, link))
}

/// Restarts a hung screen without a USB replug
/// (D-2026-09-30-release-polish-13): picks the screen like [`open_screen`]
/// (`address` is its display or wake address), has `connector` restart it
/// and returns it as the bus lists it afterwards: a rev C display comes back
/// under a new address, its MCU keeps its own. Disruptive, never
/// destructive: what the screen plays stops, its stored files stay. A
/// screen that cannot be restarted ([`Screen::restartable`]) is refused
/// with `Unsupported` before anything is sent.
pub fn restart_screen<B, C>(bus: &B, connector: &C, address: Option<&str>) -> Result<Screen>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let screen = choose_screen(discover_screens(bus)?, address)?;
    if !screen.restartable() {
        return Err(cannot_restart(&screen));
    }
    connector.restart(&screen)?;
    let anchor = screen.wake.as_ref().map(|e| e.address.0.clone());
    let back = discover_screens(bus).and_then(|all| choose_screen(all, anchor.as_deref()));
    Ok(back.unwrap_or(screen))
}

/// Why `screen` cannot be restarted, and what to do instead.
fn cannot_restart(screen: &Screen) -> BezelError {
    let name = screen.model().map_or_else(
        || {
            let address = screen.address().map(|a| a.0.as_str()).unwrap_or("?");
            format!("the screen at {address}")
        },
        |m| m.name.to_string(),
    );
    let why = if screen.family == Family::TuringRevC {
        "its wake chip (MCU) is not listed"
    } else {
        "only Turing rev C screens restart, through their wake chip (MCU)"
    };
    BezelError::Unsupported(format!(
        "restarting {name}: {why}; unplug the screen and plug it back in"
    ))
}

/// Switches a panel in desktop mode back to USB monitor mode
/// (D-2026-09-30-release-polish-8, not validated on hardware).
///
/// Without [`Confirm::Yes`] it fails with `NotConfirmed` before touching
/// either port. Confirmed, it picks the panel whose HID address is
/// `address` (or the only panel; several need an address), asks its model
/// and sends the two switch reports. A panel that does not name its model
/// is still switched: the reports are the same for every model.
pub fn leave_desktop_mode<B, H>(
    bus: &B,
    hid: &H,
    address: Option<&str>,
    confirm: Confirm,
) -> Result<MonitorModeSwitch>
where
    B: DeviceBus + ?Sized,
    H: DesktopModeHid + ?Sized,
{
    let confirmed = MonitorModeConfirmed::require(confirm)?;
    let panel = choose_desktop_panel(desktop_mode_panels(&bus.endpoints()?), address)?;
    let model_byte = hid.query_model(&panel, &confirmed)?;
    hid.back_to_monitor(&panel, confirmed)?;
    Ok(MonitorModeSwitch { panel, model_byte })
}

/// The panel at `address`, or the only one. Never a guess: several panels
/// without an address is an error that lists them.
fn choose_desktop_panel(
    mut panels: Vec<DesktopModePanel>,
    address: Option<&str>,
) -> Result<DesktopModePanel> {
    if let Some(a) = address {
        return panels
            .into_iter()
            .find(|p| p.address().0 == a)
            .ok_or_else(|| BezelError::ScreenNotFound(format!("no panel in desktop mode at {a}")));
    }
    match panels.len() {
        0 => Err(BezelError::ScreenNotFound(
            "no panel in desktop mode connected".into(),
        )),
        1 => Ok(panels.remove(0)),
        _ => {
            let names: Vec<String> = panels.iter().map(|p| p.address().to_string()).collect();
            Err(BezelError::InvalidInput(format!(
                "{} panels are in desktop mode ({}): pick one by its address",
                panels.len(),
                names.join(", ")
            )))
        }
    }
}
