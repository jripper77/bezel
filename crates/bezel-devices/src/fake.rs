//! In-memory bus for tests and demos: returns a fixed list of endpoints.

use bezel_core::Result;
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint, UsbLocation};
use bezel_core::ports::DeviceBus;

/// A [`DeviceBus`] that reports a fixed set of endpoints.
#[derive(Debug, Clone, Default)]
pub struct FakeBus {
    endpoints: Vec<Endpoint>,
}

impl FakeBus {
    /// A bus reporting exactly `endpoints`.
    pub fn new(endpoints: Vec<Endpoint>) -> Self {
        Self { endpoints }
    }

    /// A Turing 8.8" rev C: the CT88INCH MCU and the sunxi SoC behind one hub,
    /// exactly as the reference hardware enumerates on Linux.
    pub fn turing_88() -> Self {
        Self::new(vec![
            serial_endpoint(
                "/dev/ttyACM0",
                UsbId::new(0x1a86, 0xca88),
                Some("CT88INCH"),
                &[1, 1],
            ),
            serial_endpoint("/dev/ttyACM1", UsbId::new(0x0525, 0xa4a7), None, &[1, 2]),
        ])
    }
}

fn serial_endpoint(port: &str, usb: UsbId, serial: Option<&str>, ports: &[u8]) -> Endpoint {
    Endpoint {
        address: DeviceAddress(port.to_string()),
        transport: Transport::Serial,
        usb,
        serial_number: serial.map(str::to_string),
        manufacturer: None,
        product: None,
        location: Some(UsbLocation {
            bus: "3".to_string(),
            ports: ports.to_vec(),
        }),
    }
}

impl DeviceBus for FakeBus {
    fn endpoints(&self) -> Result<Vec<Endpoint>> {
        Ok(self.endpoints.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::app::discover_screens;

    #[test]
    fn turing_88_preset_is_one_awake_screen() {
        let screens = discover_screens(&FakeBus::turing_88()).unwrap();
        assert_eq!(screens.len(), 1);
        assert!(screens[0].display.is_some() && screens[0].wake.is_some());
        assert!(discover_screens(&FakeBus::default()).unwrap().is_empty());
    }
}
