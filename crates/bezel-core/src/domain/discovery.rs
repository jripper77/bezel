//! Turning the raw USB endpoints an adapter sees into screens.
//!
//! A screen can expose more than one endpoint: rev C Turing screens show a
//! wake-only micro-controller and, once awake, a Linux/Android SoC gadget,
//! both behind the same internal USB hub. They are grouped by that hub.
//!
//! A Turing USB panel in the vendor's desktop mode is no screen: it is
//! listed apart ([`DesktopModePanel`]) with its HID interface.

use super::catalog::{self, EndpointRole};
use super::device::{DeviceModel, Family, ModelId, Transport, UsbId};
use super::screen::Confirm;
use crate::BezelError;
use std::fmt;

/// Opaque address of an endpoint: a serial port name (`/dev/ttyACM1`, `COM5`)
/// or a USB path. Only the adapter that produced it can interpret it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceAddress(pub String);

impl fmt::Display for DeviceAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

/// Physical USB location: bus plus the port numbers from the root hub.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsbLocation {
    /// Bus identifier as reported by the OS.
    pub bus: String,
    /// Port chain from the root hub (e.g. `[1, 2]` for `3-1.2`).
    pub ports: Vec<u8>,
}

impl UsbLocation {
    /// The location of the hub this device is plugged into.
    pub fn parent(&self) -> Option<UsbLocation> {
        let (_, rest) = self.ports.split_last()?;
        Some(UsbLocation {
            bus: self.bus.clone(),
            ports: rest.to_vec(),
        })
    }
}

impl fmt::Display for UsbLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chain: Vec<String> = self.ports.iter().map(u8::to_string).collect();
        write!(f, "{}-{}", self.bus, chain.join("."))
    }
}

/// One USB interface the host can reach, as reported by an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Where to reach it.
    pub address: DeviceAddress,
    /// How it is reached.
    pub transport: Transport,
    /// USB identity.
    pub usb: UsbId,
    /// USB serial-number string, when the device has one.
    pub serial_number: Option<String>,
    /// USB manufacturer string.
    pub manufacturer: Option<String>,
    /// USB product string.
    pub product: Option<String>,
    /// Physical location, when the OS reports it.
    pub location: Option<UsbLocation>,
}

/// Whether a screen can take frames right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    /// The display endpoint is present.
    Awake,
    /// Only the wake micro-controller is present: the SoC must be woken first.
    Asleep,
}

/// A physical screen assembled from its endpoints.
#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    /// Protocol family.
    pub family: Family,
    /// Candidate models; exactly one once the handshake narrowed it down.
    pub candidates: Vec<&'static DeviceModel>,
    /// Endpoint that takes frames, when present.
    pub display: Option<Endpoint>,
    /// Wake-only micro-controller, when present.
    pub wake: Option<Endpoint>,
}

impl Screen {
    /// Awake when the display endpoint is present.
    pub fn state(&self) -> ScreenState {
        if self.display.is_some() {
            ScreenState::Awake
        } else {
            ScreenState::Asleep
        }
    }

    /// The single model, when discovery alone is enough to know it.
    pub fn model(&self) -> Option<&'static DeviceModel> {
        match self.candidates.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// Whether Bezel can restart the screen without a USB replug: a Turing
    /// rev C screen whose wake MCU is listed, which restarts its SoC on
    /// command (D-2026-09-30-release-polish-13).
    pub fn restartable(&self) -> bool {
        self.family == Family::TuringRevC && self.wake.is_some()
    }

    /// A stable, human-readable identity: the display endpoint address, or the
    /// wake endpoint's when asleep.
    pub fn address(&self) -> Option<&DeviceAddress> {
        self.display
            .as_ref()
            .or(self.wake.as_ref())
            .map(|e| &e.address)
    }

    /// Whether `address` reaches this screen: its display or its wake
    /// endpoint is there. A rev C screen answers to its SoC's port and to its
    /// MCU's (D-2026-10-01-live-screen-controls-3).
    pub fn answers_to(&self, address: &str) -> bool {
        [&self.display, &self.wake]
            .into_iter()
            .flatten()
            .any(|e| e.address.0 == address)
    }
}

/// Whether `a` and `b` are on the bus through the same wake chip: a rev C
/// MCU keeps its port while its SoC leaves and comes back. A display
/// grouped with it from another hub (a lone one, while the SoC is away)
/// is not the screen.
fn same_wake(a: &Screen, b: &Screen) -> bool {
    let (Some(x), Some(y)) = (&a.wake, &b.wake) else {
        return false;
    };
    let hub = |e: &Endpoint| e.location.as_ref().and_then(UsbLocation::parent);
    let behind = match (hub(x), a.display.as_ref().and_then(hub)) {
        (Some(w), Some(d)) => w == d,
        _ => true,
    };
    x.address == y.address && behind
}

/// Whether the displays of `a` and `b` sit in the same USB port.
fn same_port(a: &Screen, b: &Screen) -> bool {
    let port = |s: &Screen| s.display.as_ref().and_then(|d| d.location.clone());
    port(a).is_some() && port(a) == port(b)
}

/// Whether the displays of `a` and `b` are the same USB device by serial.
fn same_serial(a: &Screen, b: &Screen) -> bool {
    let id = |s: &Screen| {
        let d = s.display.as_ref()?;
        Some((d.usb, d.serial_number.clone()?))
    };
    id(a).is_some() && id(a) == id(b)
}

/// Whether `a` and `b` are reached at the same address.
fn same_address(a: &Screen, b: &Screen) -> bool {
    a.address().is_some() && a.address() == b.address()
}

/// The screen of `screens` that is `known` again after it left the bus and
/// came back (a rev C SoC returns under a new device name after a restart;
/// devices.md § 5.4): the one behind the same wake chip, else in the same
/// USB port, else with the same serial number, else at the same address.
/// `None` while it is not back.
pub fn find_again(screens: Vec<Screen>, known: &Screen) -> Option<Screen> {
    let mut same_family: Vec<Screen> = screens
        .into_iter()
        .filter(|s| s.family == known.family)
        .collect();
    let rules: [fn(&Screen, &Screen) -> bool; 4] =
        [same_wake, same_port, same_serial, same_address];
    let found = rules
        .iter()
        .find_map(|rule| same_family.iter().position(|s| rule(s, known)))?;
    Some(same_family.swap_remove(found))
}

struct Classified {
    endpoint: Endpoint,
    role: EndpointRole,
    family: Family,
    models: &'static [ModelId],
}

/// Groups endpoints into screens. Unknown endpoints and HID interfaces are
/// ignored.
///
/// Wake endpoints join the display endpoint of the same family that shares
/// their parent hub; without location data, a lone wake endpoint joins a lone
/// display endpoint of the same family. Unmatched wake endpoints become
/// asleep screens.
pub fn group_screens(endpoints: Vec<Endpoint>) -> Vec<Screen> {
    let classified: Vec<Classified> = endpoints.into_iter().filter_map(classify).collect();
    let (wakes, displays): (Vec<Classified>, Vec<Classified>) = classified
        .into_iter()
        .partition(|c| c.role == EndpointRole::Wake);

    let mut screens: Vec<Screen> = displays.into_iter().map(screen_from_display).collect();
    for wake in wakes {
        attach_wake(&mut screens, wake);
    }
    screens.sort_by(|a, b| a.address().cmp(&b.address()));
    screens
}

/// A screen endpoint; HID interfaces never are (only desktop-mode panels
/// are reached through HID, see [`desktop_mode_panels`]).
fn classify(endpoint: Endpoint) -> Option<Classified> {
    if endpoint.transport == Transport::Hid {
        return None;
    }
    let rule = catalog::classify(endpoint.usb, endpoint.serial_number.as_deref())?;
    Some(Classified {
        endpoint,
        role: rule.role,
        family: rule.family,
        models: rule.models,
    })
}

fn models_of(ids: &[ModelId]) -> Vec<&'static DeviceModel> {
    ids.iter()
        .filter_map(|id| catalog::model_by_id(*id))
        .collect()
}

fn screen_from_display(c: Classified) -> Screen {
    Screen {
        family: c.family,
        candidates: models_of(c.models),
        display: Some(c.endpoint),
        wake: None,
    }
}

fn attach_wake(screens: &mut Vec<Screen>, wake: Classified) {
    match partner_index(screens, &wake) {
        Some(i) => {
            let screen = &mut screens[i];
            screen.candidates.retain(|m| wake.models.contains(&m.id));
            screen.wake = Some(wake.endpoint);
        }
        None => screens.push(Screen {
            family: wake.family,
            candidates: models_of(wake.models),
            display: None,
            wake: Some(wake.endpoint),
        }),
    }
}

fn partner_index(screens: &[Screen], wake: &Classified) -> Option<usize> {
    let free: Vec<usize> = screens
        .iter()
        .enumerate()
        .filter(|(_, s)| s.family == wake.family && s.wake.is_none() && s.display.is_some())
        .filter(|(_, s)| s.candidates.iter().any(|m| wake.models.contains(&m.id)))
        .map(|(i, _)| i)
        .collect();

    let wake_hub = wake
        .endpoint
        .location
        .as_ref()
        .and_then(UsbLocation::parent);
    if let Some(hub) = wake_hub {
        let same_hub = free.iter().copied().find(|&i| {
            screens[i]
                .display
                .as_ref()
                .and_then(|d| d.location.as_ref())
                .and_then(UsbLocation::parent)
                .is_some_and(|p| p == hub)
        });
        if same_hub.is_some() {
            return same_hub;
        }
    }
    match free.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// A Turing USB panel the vendor app switched into its Windows "desktop
/// mode" (`docs/reverse-engineering/protocol-turing-usb.md` section 10): it
/// enumerates as 1a86:ad10-ad13 and is reached only through its HID
/// interface. Bezel cannot draw on it; it lists it and, behind a
/// confirmation, switches it back to USB monitor mode.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopModePanel {
    /// The HID interface.
    pub hid: Endpoint,
    /// The models it may be; the panel names one only when asked (the model
    /// query is sent only as part of a confirmed switch).
    pub candidates: Vec<&'static DeviceModel>,
}

impl DesktopModePanel {
    /// Desktop mode has not been validated on real hardware by the project:
    /// every surface labels it "not validated on hardware"
    /// (D-2026-09-30-release-polish-8).
    pub const HARDWARE_VALIDATED: bool = false;

    /// The family the panel belongs to once back in USB monitor mode.
    pub const FAMILY: Family = Family::TuringUsb;

    /// The HID interface's address, which picks the panel.
    pub fn address(&self) -> &DeviceAddress {
        &self.hid.address
    }
}

/// The panels in desktop mode among `endpoints`: HID interfaces with one of
/// the [`catalog::DESKTOP_MODE_IDS`], sorted by address.
pub fn desktop_mode_panels(endpoints: &[Endpoint]) -> Vec<DesktopModePanel> {
    let mut panels: Vec<DesktopModePanel> = endpoints
        .iter()
        .filter(|e| e.transport == Transport::Hid && catalog::is_desktop_mode(e.usb))
        .map(|e| DesktopModePanel {
            hid: e.clone(),
            candidates: catalog::desktop_mode_candidates(),
        })
        .collect();
    panels.sort_by(|a, b| a.address().cmp(b.address()));
    panels
}

/// Everything one enumeration found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Discovery {
    /// The screens, as [`group_screens`] assembles them.
    pub screens: Vec<Screen>,
    /// The panels in desktop mode.
    pub desktop_mode: Vec<DesktopModePanel>,
}

/// Sorts `endpoints` into screens and panels in desktop mode; unknown
/// endpoints are ignored.
pub fn group_devices(endpoints: Vec<Endpoint>) -> Discovery {
    let desktop_mode = desktop_mode_panels(&endpoints);
    Discovery {
        screens: group_screens(endpoints),
        desktop_mode,
    }
}

/// What switching a panel in desktop mode back to USB monitor mode is called
/// in confirmations and errors.
pub const MONITOR_MODE_SWITCH: &str =
    "switching a panel in desktop mode back to USB monitor mode (not validated on hardware)";

/// Proof that the user confirmed switching a panel in desktop mode back to
/// USB monitor mode. Only [`MonitorModeConfirmed::require`] makes one, and
/// only from [`Confirm::Yes`]; the HID port's switch takes it.
#[derive(Debug)]
pub struct MonitorModeConfirmed {
    _proof: (),
}

impl MonitorModeConfirmed {
    /// The proof, or `NotConfirmed` for [`Confirm::No`].
    pub fn require(confirm: Confirm) -> crate::Result<Self> {
        match confirm {
            Confirm::Yes => Ok(Self { _proof: () }),
            Confirm::No => Err(BezelError::NotConfirmed(MONITOR_MODE_SWITCH.to_string())),
        }
    }
}

/// What a confirmed switch back to USB monitor mode did.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorModeSwitch {
    /// The panel that was switched.
    pub panel: DesktopModePanel,
    /// Its answer to the model query; `None` when it did not answer in time.
    pub model_byte: Option<u8>,
}

impl MonitorModeSwitch {
    /// The catalog model the panel named, when it named a known one.
    pub fn model(&self) -> Option<&'static DeviceModel> {
        self.model_byte.and_then(catalog::desktop_mode_model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(
        addr: &str,
        vid: u16,
        pid: u16,
        serial: Option<&str>,
        loc: Option<(&str, &[u8])>,
    ) -> Endpoint {
        Endpoint {
            address: DeviceAddress(addr.to_string()),
            transport: Transport::Serial,
            usb: UsbId::new(vid, pid),
            serial_number: serial.map(str::to_string),
            manufacturer: None,
            product: None,
            location: loc.map(|(bus, ports)| UsbLocation {
                bus: bus.to_string(),
                ports: ports.to_vec(),
            }),
        }
    }

    #[test]
    fn turing_88_pair_behind_one_hub_is_one_screen() {
        let screens = group_screens(vec![
            ep(
                "/dev/ttyACM0",
                0x1a86,
                0xca88,
                Some("CT88INCH"),
                Some(("3", &[1, 1])),
            ),
            ep("/dev/ttyACM1", 0x0525, 0xa4a7, None, Some(("3", &[1, 2]))),
            ep("/dev/ttyUSB0", 0x067b, 0x23a3, None, Some(("11", &[1, 3]))),
        ]);
        assert_eq!(screens.len(), 1);
        let s = &screens[0];
        assert_eq!(s.state(), ScreenState::Awake);
        assert_eq!(s.model().map(|m| m.id), Some(ModelId("turing-8.8")));
        assert_eq!(s.address(), Some(&DeviceAddress("/dev/ttyACM1".into())));
        assert_eq!(
            s.wake.as_ref().map(|w| w.address.0.as_str()),
            Some("/dev/ttyACM0")
        );
    }

    #[test]
    fn two_88_screens_pair_by_hub() {
        let screens = group_screens(vec![
            ep(
                "a-mcu",
                0x1a86,
                0xca88,
                Some("CT88INCH"),
                Some(("3", &[1, 1])),
            ),
            ep(
                "b-mcu",
                0x1a86,
                0xca88,
                Some("CT88INCH"),
                Some(("3", &[4, 1])),
            ),
            ep("b-soc", 0x0525, 0xa4a7, None, Some(("3", &[4, 2]))),
            ep("a-soc", 0x0525, 0xa4a7, None, Some(("3", &[1, 2]))),
        ]);
        assert_eq!(screens.len(), 2);
        for s in &screens {
            let d = &s.display.as_ref().unwrap().address.0;
            let w = &s.wake.as_ref().unwrap().address.0;
            assert_eq!(d.chars().next(), w.chars().next(), "{d} paired with {w}");
        }
    }

    #[test]
    fn lone_mcu_is_an_asleep_screen() {
        let screens = group_screens(vec![ep("COM3", 0x1a86, 0xca21, Some("CT21INCH"), None)]);
        assert_eq!(screens.len(), 1);
        assert_eq!(screens[0].state(), ScreenState::Asleep);
        assert_eq!(screens[0].candidates.len(), 3);
        assert_eq!(screens[0].address(), Some(&DeviceAddress("COM3".into())));
    }

    #[test]
    fn ca21_wake_narrows_candidates_without_location() {
        let screens = group_screens(vec![
            ep("COM3", 0x1a86, 0xca21, Some("CT21INCH"), None),
            ep("COM4", 0x1d6b, 0x0121, Some("20080411"), None),
        ]);
        assert_eq!(screens.len(), 1);
        let ids: Vec<_> = screens[0].candidates.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![ModelId("turing-2.1"), ModelId("turing-2.8")]);
        assert!(screens[0].model().is_none());
    }

    fn hid(addr: &str, pid: u16) -> Endpoint {
        Endpoint {
            transport: Transport::Hid,
            ..ep(addr, 0x1a86, pid, None, None)
        }
    }

    #[test]
    fn desktop_mode_panels_are_listed_apart_from_screens() {
        let found = group_devices(vec![
            hid("hid:/dev/hidraw9", 0xad13),
            ep("/dev/ttyACM1", 0x0525, 0xa4a7, None, Some(("3", &[1, 2]))),
            hid("hid:/dev/hidraw3", 0xad11),
            // Same USB id, but not a HID interface: not a panel Bezel can reach.
            ep("/dev/ttyACM7", 0x1a86, 0xad11, None, None),
            // A HID interface with a screen's id: neither a screen nor a panel.
            hid("hid:/dev/hidraw0", 0x5722),
        ]);
        assert_eq!(found.screens.len(), 1);
        assert_eq!(found.screens[0].family, Family::TuringRevC);
        let addresses: Vec<&str> = found
            .desktop_mode
            .iter()
            .map(|p| p.address().0.as_str())
            .collect();
        assert_eq!(addresses, ["hid:/dev/hidraw3", "hid:/dev/hidraw9"]);
        let panel = &found.desktop_mode[0];
        assert_eq!(panel.hid.usb, UsbId::new(0x1a86, 0xad11));
        let ids: Vec<_> = panel.candidates.iter().map(|m| m.id.0).collect();
        assert_eq!(ids, ["turing-usb-8.8", "turing-usb-8", "turing-usb-5.2"]);
        const { assert!(!DesktopModePanel::HARDWARE_VALIDATED) };
        assert!(
            panel
                .candidates
                .iter()
                .all(|m| m.family == DesktopModePanel::FAMILY)
        );
        assert!(group_screens(vec![hid("hid:/dev/hidraw3", 0xad11)]).is_empty());
        assert_eq!(group_devices(Vec::new()), Discovery::default());
    }

    #[test]
    fn the_switch_back_needs_confirmation_and_names_the_model() {
        let err = MonitorModeConfirmed::require(Confirm::No).unwrap_err();
        assert_eq!(
            err.to_string(),
            "switching a panel in desktop mode back to USB monitor mode \
             (not validated on hardware) needs confirmation"
        );
        assert!(MonitorModeConfirmed::require(Confirm::Yes).is_ok());

        let panel = desktop_mode_panels(&[hid("hid:/dev/hidraw3", 0xad11)]).remove(0);
        let mut switch = MonitorModeSwitch {
            panel,
            model_byte: Some(0x80),
        };
        assert_eq!(switch.model().map(|m| m.id.0), Some("turing-usb-8"));
        switch.model_byte = Some(0x42);
        assert!(switch.model().is_none());
        switch.model_byte = None;
        assert!(switch.model().is_none());
    }

    #[test]
    fn a_screen_back_on_the_bus_is_found_again_by_identity() {
        let mcu = || {
            ep(
                "/dev/ttyACM0",
                0x1a86,
                0xca88,
                Some("CT88INCH"),
                Some(("3", &[1, 1])),
            )
        };
        let soc = |addr: &str, port: u8| ep(addr, 0x0525, 0xa4a7, None, Some(("3", &[1, port])));
        let known = group_screens(vec![mcu(), soc("/dev/ttyACM1", 2)]).remove(0);
        // The SoC restarted and came back as another tty, beside another 8.8".
        let other = ep("/dev/ttyACM9", 0x0525, 0xa4a7, None, Some(("3", &[4, 2])));
        let back = group_screens(vec![other.clone(), soc("/dev/ttyACM2", 2), mcu()]);
        let found = find_again(back, &known).expect("back");
        assert_eq!(found.address(), Some(&DeviceAddress("/dev/ttyACM2".into())));
        // Still away: only the MCU is listed (asleep), which is the screen
        // too; a lone display from another hub grouped with it is not.
        let away = find_again(group_screens(vec![mcu()]), &known).expect("asleep");
        assert_eq!(away.display, None);
        assert!(find_again(group_screens(vec![mcu(), other.clone()]), &known).is_none());

        // Without a wake chip: the same USB port, else the same serial, else
        // the same address.
        let no_wake = |e: Endpoint| group_screens(vec![e]).remove(0);
        let lone = no_wake(soc("/dev/ttyACM1", 2));
        let moved = group_screens(vec![other.clone(), soc("/dev/ttyACM5", 2)]);
        let found = find_again(moved, &lone).expect("same port");
        assert_eq!(found.address(), Some(&DeviceAddress("/dev/ttyACM5".into())));
        let weact = |addr: &str| ep(addr, 0x1a86, 0xfe0c, Some("AD0001"), None);
        let found = find_again(group_screens(vec![weact("COM7")]), &no_wake(weact("COM3")));
        assert_eq!(
            found.and_then(|s| s.address().cloned()),
            Some(DeviceAddress("COM7".into()))
        );
        let plain = |addr: &str| ep(addr, 0x1a86, 0x5722, None, None);
        let at = |addr| group_screens(vec![plain(addr)]);
        assert!(find_again(at("/dev/ttyUSB0"), &no_wake(plain("/dev/ttyUSB0"))).is_some());
        assert!(find_again(at("/dev/ttyUSB1"), &no_wake(plain("/dev/ttyUSB0"))).is_none());
        assert!(find_again(Vec::new(), &known).is_none());
    }

    #[test]
    fn a_screen_answers_to_its_display_and_its_mcu_port() {
        let mcu = || {
            ep(
                "/dev/ttyACM0",
                0x1a86,
                0xca88,
                Some("CT88INCH"),
                Some(("3", &[1, 1])),
            )
        };
        let soc = || ep("/dev/ttyACM1", 0x0525, 0xa4a7, None, Some(("3", &[1, 2])));
        let awake = group_screens(vec![mcu(), soc()]).remove(0);
        assert!(awake.answers_to("/dev/ttyACM1"), "its display");
        assert!(awake.answers_to("/dev/ttyACM0"), "its MCU");
        assert!(!awake.answers_to("/dev/ttyACM2"));
        assert!(!awake.answers_to(""));
        // Asleep, only the MCU is listed: the screen answers to it alone.
        let asleep = group_screens(vec![mcu()]).remove(0);
        assert_eq!(asleep.state(), ScreenState::Asleep);
        assert!(asleep.answers_to("/dev/ttyACM0"));
        assert!(!asleep.answers_to("/dev/ttyACM1"));
        // A display without its MCU answers to the display alone.
        let lone = group_screens(vec![soc()]).remove(0);
        assert!(lone.answers_to("/dev/ttyACM1"));
        assert!(!lone.answers_to("/dev/ttyACM0"));
    }

    #[test]
    fn location_formats_like_sysfs() {
        let loc = UsbLocation {
            bus: "3".into(),
            ports: vec![1, 2],
        };
        assert_eq!(loc.to_string(), "3-1.2");
        assert_eq!(loc.parent().unwrap().to_string(), "3-1");
        assert!(
            UsbLocation {
                bus: "3".into(),
                ports: vec![]
            }
            .parent()
            .is_none()
        );
        assert_eq!(DeviceAddress("x".into()).to_string(), "x");
    }
}
