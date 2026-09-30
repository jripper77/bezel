//! Opening discovered screens with the right driver.

use std::time::{Duration, Instant};

use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::discovery::{Endpoint, Screen, UsbLocation, group_screens};
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};
use bezel_core::{BezelError, Result};

use crate::discovery::SystemBus;
use crate::driver::RealTime;
use crate::driver::kipye_rev_d::KipyeRevD;
use crate::driver::turing_rev_a::TuringRevA;
use crate::driver::turing_rev_c::TuringRevC;
use crate::driver::turing_usb::{self, TuringUsb};
use crate::driver::wch::{self, Wch};
use crate::driver::weact::WeAct;
use crate::driver::xuanfang_rev_b::XuanFangRevB;
use crate::usb::{Endpoints, UsbWire};
use crate::wire::{Flow, SerialWire};

/// How long a rev C SoC may take to boot after its MCU is poked: about 11 s
/// when it has slept a while, longer right after it shut down.
const WAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// Delay between wake attempts.
const WAKE_STEP: Duration = Duration::from_secs(1);
/// How long a rev C SoC that is shutting down may take to leave the bus.
const LEAVE_TIMEOUT: Duration = Duration::from_secs(5);
/// Delay between checks that it left.
const LEAVE_STEP: Duration = Duration::from_millis(250);

/// Connects to real screens through the host's serial ports and USB.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemConnector;

impl ScreenConnector for SystemConnector {
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        let models = &screen.candidates;
        match screen.family {
            Family::TuringRevC => connect_rev_c(screen, models),
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
            Family::TuringUsb => {
                let wire = open_usb(display(screen)?, turing_usb::ENDPOINTS)?;
                Ok(Box::new(TuringUsb::connect(wire, &RealTime, models)?))
            }
            Family::Wch => {
                let wire = open_usb(display(screen)?, wch::ENDPOINTS)?;
                Ok(Box::new(Wch::connect(wire, &RealTime, models)?))
            }
        }
    }
}

fn open_rev_c(display: &Endpoint, models: &[&'static DeviceModel]) -> Result<Box<dyn ScreenLink>> {
    let wire = open_serial(display, Flow::None)?;
    Ok(Box::new(TuringRevC::connect(wire, &RealTime, models)?))
}

/// Opens a rev C screen, waking it when it sleeps. A display that fails its
/// handshake with a transport error or a timeout is shutting down: the
/// vendor app and turing-smart-screen-python send TURNOFF when they exit,
/// and the SoC then leaves the bus. Wait for it to go, wake it, try again.
fn connect_rev_c(screen: &Screen, models: &[&'static DeviceModel]) -> Result<Box<dyn ScreenLink>> {
    let Some(display) = &screen.display else {
        return open_rev_c(&wake_rev_c(screen)?, models);
    };
    match open_rev_c(display, models) {
        Err(BezelError::Transport(reason) | BezelError::Timeout(reason))
            if screen.wake.is_some() =>
        {
            tracing::debug!(%reason, "rev C handshake failed; waking the screen and retrying");
            wait_until_gone(display);
            open_rev_c(&wake_rev_c(screen)?, models)
        }
        other => other,
    }
}

/// Waits (bounded) until `endpoint` is no longer connected.
fn wait_until_gone(endpoint: &Endpoint) {
    let started = Instant::now();
    while started.elapsed() < LEAVE_TIMEOUT {
        let present = SystemBus
            .endpoints()
            .is_ok_and(|all| all.iter().any(|e| e.address == endpoint.address));
        if !present {
            return;
        }
        std::thread::sleep(LEAVE_STEP);
    }
}

fn open_usb(endpoint: &Endpoint, endpoints: Endpoints) -> Result<UsbWire> {
    let address = &endpoint.address.0;
    UsbWire::open(address, endpoints).map_err(|e| access_error(address, &e))
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
    fn every_family_is_routed_and_missing_devices_fail_cleanly() {
        for (family, model) in [
            (Family::TuringRevA, "turing-3.5"),
            (Family::XuanFangRevB, "xuanfang-3.5"),
            (Family::KipyeRevD, "kipye-qiye-3.5"),
            (Family::WeAct, "weact-fs-3.5"),
            (Family::TuringUsb, "turing-usb-8.8"),
            (Family::Wch, "wch-3.38"),
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
