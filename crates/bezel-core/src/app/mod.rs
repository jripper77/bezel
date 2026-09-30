//! Use cases, generic over the driven ports.

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::{Transport, UsbId};
    use crate::domain::discovery::{DeviceAddress, Endpoint};

    struct StaticBus(Result<Vec<Endpoint>>);

    impl DeviceBus for StaticBus {
        fn endpoints(&self) -> Result<Vec<Endpoint>> {
            self.0.clone()
        }
    }

    struct Refuse;

    impl ScreenConnector for Refuse {
        fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
            Err(BezelError::Transport(format!(
                "refused {:?}",
                screen.address()
            )))
        }
    }

    fn ep(addr: &str, vid: u16, pid: u16, serial: Option<&str>) -> Endpoint {
        Endpoint {
            address: DeviceAddress(addr.into()),
            transport: Transport::Serial,
            usb: UsbId::new(vid, pid),
            serial_number: serial.map(str::to_string),
            manufacturer: None,
            product: None,
            location: None,
        }
    }

    #[test]
    fn discovers_and_propagates_errors() {
        let screens = discover_screens(&StaticBus(Ok(vec![ep(
            "/dev/ttyACM1",
            0x0525,
            0xa4a7,
            None,
        )])))
        .unwrap();
        assert_eq!(screens.len(), 1);
        let err = BezelError::Transport("boom".into());
        assert_eq!(discover_screens(&StaticBus(Err(err.clone()))), Err(err));
    }

    #[test]
    fn choose_prefers_address_then_awake() {
        let asleep = ep("COM3", 0x1a86, 0xca21, Some("CT21INCH"));
        let awake = ep("COM9", 0x1cbe, 0x0088, None);
        let screens = || group_screens(vec![asleep.clone(), awake.clone()]);
        let chosen = choose_screen(screens(), None).unwrap();
        assert_eq!(chosen.address().map(|a| a.0.as_str()), Some("COM9"));
        let chosen = choose_screen(screens(), Some("COM3")).unwrap();
        assert_eq!(chosen.state(), ScreenState::Asleep);
        assert!(matches!(
            choose_screen(screens(), Some("COM1")),
            Err(BezelError::ScreenNotFound(_))
        ));
        assert!(matches!(
            choose_screen(vec![], None),
            Err(BezelError::ScreenNotFound(_))
        ));
    }

    #[test]
    fn open_screen_goes_through_the_connector() {
        let bus = StaticBus(Ok(vec![ep("/dev/ttyACM1", 0x0525, 0xa4a7, None)]));
        let err = open_screen(&bus, &Refuse, None).err().unwrap();
        assert!(err.to_string().contains("ttyACM1"));
    }
}
