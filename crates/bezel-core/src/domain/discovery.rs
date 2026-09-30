//! Turning the raw USB endpoints an adapter sees into screens.
//!
//! A screen can expose more than one endpoint: rev C Turing screens show a
//! wake-only micro-controller and, once awake, a Linux/Android SoC gadget,
//! both behind the same internal USB hub. They are grouped by that hub.

use super::catalog::{self, EndpointRole};
use super::device::{DeviceModel, Family, ModelId, Transport, UsbId};
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

    /// A stable, human-readable identity: the display endpoint address, or the
    /// wake endpoint's when asleep.
    pub fn address(&self) -> Option<&DeviceAddress> {
        self.display
            .as_ref()
            .or(self.wake.as_ref())
            .map(|e| &e.address)
    }
}

struct Classified {
    endpoint: Endpoint,
    role: EndpointRole,
    family: Family,
    models: &'static [ModelId],
}

/// Groups endpoints into screens. Unknown endpoints are ignored.
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

fn classify(endpoint: Endpoint) -> Option<Classified> {
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
