//! Opening discovered screens with the right driver.

use std::time::{Duration, Instant};

use bezel_core::domain::device::Family;
use bezel_core::domain::discovery::{Endpoint, Screen, UsbLocation, group_screens};
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};
use bezel_core::{BezelError, Result};

use crate::discovery::SystemBus;
use crate::driver::kipye_rev_d::KipyeRevD;
use crate::driver::turing_rev_a::TuringRevA;
use crate::driver::turing_rev_c::{RealTime, TuringRevC};
use crate::driver::weact::WeAct;
use crate::driver::xuanfang_rev_b::XuanFangRevB;
use crate::wire::{Flow, SerialWire};

/// How long a rev C SoC may take to boot after its MCU is poked.
const WAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Delay between wake attempts.
const WAKE_STEP: Duration = Duration::from_secs(1);

/// Connects to real screens through the host's serial ports and USB.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemConnector;

impl ScreenConnector for SystemConnector {
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        let models = &screen.candidates;
        match screen.family {
            Family::TuringRevC => {
                let display = match &screen.display {
                    Some(d) => d.clone(),
                    None => wake_rev_c(screen)?,
                };
                let wire = open_serial(&display, Flow::None)?;
                Ok(Box::new(TuringRevC::connect(wire, &RealTime, models)?))
            }
            Family::TuringRevA => {
                let wire = open_serial(display(screen)?, Flow::Hardware)?;
                Ok(Box::new(TuringRevA::connect(wire, &RealTime, models)?))
            }
            Family::XuanFangRevB => {
                let wire = open_serial(display(screen)?, Flow::Hardware)?;
                Ok(Box::new(XuanFangRevB::connect(wire, &RealTime, models)?))
            }
            Family::KipyeRevD => {
                let wire = open_serial(display(screen)?, Flow::Hardware)?;
                Ok(Box::new(KipyeRevD::connect(wire, &RealTime, models)?))
            }
            Family::WeAct => {
                let wire = open_serial(display(screen)?, Flow::Hardware)?;
                Ok(Box::new(WeAct::connect(wire, &RealTime, models)?))
            }
            other => Err(BezelError::Transport(format!(
                "{} screens are not supported yet",
                other.slug()
            ))),
        }
    }
}

/// The display endpoint of a family without a wake companion.
fn display(screen: &Screen) -> Result<&Endpoint> {
    screen
        .display
        .as_ref()
        .ok_or_else(|| BezelError::ScreenNotFound("screen without a display endpoint".into()))
}

fn open_serial(endpoint: &Endpoint, flow: Flow) -> Result<SerialWire> {
    let address = &endpoint.address.0;
    let holders = crate::busy::holders(address);
    if !holders.is_empty() {
        return Err(BezelError::InUse {
            address: address.clone(),
            holders,
        });
    }
    SerialWire::open(address, flow).map_err(|e| access_error(address, &e))
}

/// Maps an open failure: permission problems get their own variant so the
/// UI can explain the udev rule.
pub fn access_error(address: &str, e: &std::io::Error) -> BezelError {
    let text = e.to_string();
    let lower = text.to_lowercase();
    if e.kind() == std::io::ErrorKind::PermissionDenied
        || lower.contains("permission denied")
        || lower.contains("access is denied")
    {
        BezelError::AccessDenied {
            address: address.to_string(),
            reason: text,
        }
    } else {
        BezelError::Transport(format!("{address}: {text}"))
    }
}

/// Wakes a sleeping rev C screen: opening and closing its MCU port makes it
/// boot the SoC (the only side effect discovery-adjacent code may have; no
/// byte is written). Returns the SoC endpoint once it enumerates.
fn wake_rev_c(screen: &Screen) -> Result<Endpoint> {
    let wake = screen
        .wake
        .as_ref()
        .ok_or_else(|| BezelError::ScreenNotFound("rev C screen without endpoints".into()))?;
    let hub = wake.location.as_ref().and_then(UsbLocation::parent);
    let started = Instant::now();
    while started.elapsed() < WAKE_TIMEOUT {
        if let Ok(port) = SerialWire::open(&wake.address.0, Flow::None) {
            drop(port);
        }
        std::thread::sleep(WAKE_STEP);
        let screens = group_screens(SystemBus.endpoints()?);
        let awake = screens
            .into_iter()
            .filter(|s| s.family == Family::TuringRevC)
            .find_map(|s| {
                let display = s.display?;
                let same_hub =
                    hub.is_none() || display.location.as_ref().and_then(UsbLocation::parent) == hub;
                same_hub.then_some(display)
            });
        if let Some(display) = awake {
            return Ok(display);
        }
    }
    Err(BezelError::Timeout("the screen did not wake up".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::{ModelId, Transport, UsbId};
    use bezel_core::domain::discovery::DeviceAddress;

    fn endpoint(addr: &str) -> Endpoint {
        Endpoint {
            address: DeviceAddress(addr.into()),
            transport: Transport::Serial,
            usb: UsbId::new(0x0525, 0xa4a7),
            serial_number: None,
            manufacturer: None,
            product: None,
            location: None,
        }
    }

    #[test]
    fn every_serial_family_is_routed_and_missing_ports_fail_cleanly() {
        for (family, model) in [
            (Family::TuringRevA, "turing-3.5"),
            (Family::XuanFangRevB, "xuanfang-3.5"),
            (Family::KipyeRevD, "kipye-qiye-3.5"),
            (Family::WeAct, "weact-fs-3.5"),
        ] {
            let mut screen = Screen {
                family,
                candidates: vec![model_by_id(ModelId(model)).unwrap()],
                display: Some(endpoint("/dev/bezel-no-such-port")),
                wake: None,
            };
            let err = SystemConnector.connect(&screen).err().unwrap();
            assert!(matches!(err, BezelError::Transport(_)), "{family:?}: {err}");
            screen.display = None;
            let err = SystemConnector.connect(&screen).err().unwrap();
            assert!(matches!(err, BezelError::ScreenNotFound(_)), "{family:?}");
        }
    }

    #[test]
    fn missing_port_is_a_transport_error_and_permission_is_access_denied() {
        let screen = Screen {
            family: Family::TuringRevC,
            candidates: vec![model_by_id(ModelId("turing-8.8")).unwrap()],
            display: Some(endpoint("/dev/bezel-no-such-port")),
            wake: None,
        };
        assert!(matches!(
            SystemConnector.connect(&screen).err(),
            Some(BezelError::Transport(_))
        ));
        let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        assert!(matches!(
            access_error("x", &denied),
            BezelError::AccessDenied { .. }
        ));
        let other = std::io::Error::other("Access is denied.");
        assert!(matches!(
            access_error("COM3", &other),
            BezelError::AccessDenied { .. }
        ));
    }

    #[test]
    fn a_rev_c_screen_without_endpoints_cannot_wake() {
        let screen = Screen {
            family: Family::TuringRevC,
            candidates: vec![],
            display: None,
            wake: None,
        };
        assert!(matches!(
            SystemConnector.connect(&screen).err(),
            Some(BezelError::ScreenNotFound(_))
        ));
    }
}
