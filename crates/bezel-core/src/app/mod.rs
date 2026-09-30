//! Use cases, generic over the driven ports.

use crate::Result;
use crate::domain::discovery::{Screen, group_screens};
use crate::ports::DeviceBus;

/// Lists the connected screens, grouping each screen's endpoints.
pub fn discover_screens<B: DeviceBus + ?Sized>(bus: &B) -> Result<Vec<Screen>> {
    Ok(group_screens(bus.endpoints()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BezelError;
    use crate::domain::device::{Transport, UsbId};
    use crate::domain::discovery::{DeviceAddress, Endpoint};

    struct StaticBus(Result<Vec<Endpoint>>);

    impl DeviceBus for StaticBus {
        fn endpoints(&self) -> Result<Vec<Endpoint>> {
            self.0.clone()
        }
    }

    #[test]
    fn discovers_and_propagates_errors() {
        let soc = Endpoint {
            address: DeviceAddress("/dev/ttyACM1".into()),
            transport: Transport::Serial,
            usb: UsbId::new(0x0525, 0xa4a7),
            serial_number: None,
            manufacturer: None,
            product: None,
            location: None,
        };
        let screens = discover_screens(&StaticBus(Ok(vec![soc]))).unwrap();
        assert_eq!(screens.len(), 1);
        let err = BezelError::Transport("boom".into());
        assert_eq!(discover_screens(&StaticBus(Err(err.clone()))), Err(err));
    }
}
