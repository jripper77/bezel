//! Read-only discovery of smart-screen endpoints.
//!
//! Serial (CDC-ACM) endpoints come from the OS serial-port list; raw USB
//! endpoints (Turing USB, WCH) from the USB device list. Nothing is opened:
//! enumeration only reads descriptors the OS already cached.

use bezel_core::domain::catalog;
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint, UsbLocation};
use bezel_core::ports::DeviceBus;
use bezel_core::{BezelError, Result};
use nusb::MaybeFuture;
use serialport::{SerialPortInfo, SerialPortType, UsbPortInfo};

/// The host's real USB/serial bus.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBus;

impl DeviceBus for SystemBus {
    fn endpoints(&self) -> Result<Vec<Endpoint>> {
        let ports = serialport::available_ports()
            .map_err(|e| BezelError::Transport(format!("serial port enumeration: {e}")))?;
        let mut endpoints: Vec<Endpoint> = ports.into_iter().filter_map(serial_endpoint).collect();
        match usb_endpoints() {
            Ok(usb) => endpoints.extend(usb),
            Err(e) => tracing::warn!("USB enumeration failed, serial screens only: {e}"),
        }
        Ok(endpoints)
    }
}

/// Maps one OS serial port to an endpoint; ports that are not USB are skipped.
pub fn serial_endpoint(port: SerialPortInfo) -> Option<Endpoint> {
    match port.port_type {
        SerialPortType::UsbPort(info) => Some(usb_serial_endpoint(port.port_name, info)),
        _ => None,
    }
}

fn usb_serial_endpoint(port_name: String, info: UsbPortInfo) -> Endpoint {
    Endpoint {
        address: DeviceAddress(port_name),
        transport: Transport::Serial,
        usb: UsbId::new(info.vid, info.pid),
        serial_number: non_empty(info.serial_number),
        manufacturer: non_empty(info.manufacturer),
        product: non_empty(info.product),
        location: info.location.map(|l| UsbLocation {
            bus: l.bus_id().to_string(),
            ports: l.port_chain().to_vec(),
        }),
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

/// True when the catalog says this USB id is reached over raw bulk transfers.
fn is_bulk_screen(usb: UsbId) -> bool {
    catalog::classify(usb, None).is_some_and(|r| r.family.transport() == Transport::UsbBulk)
}

fn usb_endpoints() -> std::result::Result<Vec<Endpoint>, nusb::Error> {
    let devices = nusb::list_devices().wait()?;
    Ok(devices
        .filter(|d| is_bulk_screen(UsbId::new(d.vendor_id(), d.product_id())))
        .map(|d| {
            let location = UsbLocation {
                bus: d.bus_id().to_string(),
                ports: d.port_chain().to_vec(),
            };
            Endpoint {
                address: DeviceAddress(format!("usb:{location}")),
                transport: Transport::UsbBulk,
                usb: UsbId::new(d.vendor_id(), d.product_id()),
                serial_number: non_empty(d.serial_number().map(str::to_string)),
                manufacturer: non_empty(d.manufacturer_string().map(str::to_string)),
                product: non_empty(d.product_string().map(str::to_string)),
                location: Some(location),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::discovery::{ScreenState, group_screens};
    use serialport::Location;

    fn usb_port(
        name: &str,
        vid: u16,
        pid: u16,
        serial: Option<&str>,
        chain: &[u8],
    ) -> SerialPortInfo {
        SerialPortInfo {
            port_name: name.to_string(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                serial_number: serial.map(str::to_string),
                manufacturer: Some("Turing".to_string()),
                product: Some(String::new()),
                location: Some(Location::new("3".to_string(), chain.to_vec())),
            }),
        }
    }

    #[test]
    fn groups_turing_88_mcu_and_soc() {
        let ports = vec![
            usb_port("/dev/ttyACM0", 0x1a86, 0xca88, Some("CT88INCH"), &[1, 1]),
            usb_port("/dev/ttyACM1", 0x0525, 0xa4a7, None, &[1, 2]),
            SerialPortInfo {
                port_name: "/dev/ttyS0".to_string(),
                port_type: SerialPortType::Unknown,
            },
        ];
        let endpoints: Vec<Endpoint> = ports.into_iter().filter_map(serial_endpoint).collect();
        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].manufacturer.as_deref(), Some("Turing"));
        assert_eq!(endpoints[0].product, None, "blank strings are dropped");

        let screens = group_screens(endpoints);
        assert_eq!(screens.len(), 1);
        let screen = &screens[0];
        assert_eq!(screen.state(), ScreenState::Awake);
        assert_eq!(screen.model().map(|m| m.id.0), Some("turing-8.8"));
        assert_eq!(
            screen.display.as_ref().map(|d| d.address.0.as_str()),
            Some("/dev/ttyACM1")
        );
        assert_eq!(
            screen.wake.as_ref().map(|w| w.address.0.as_str()),
            Some("/dev/ttyACM0")
        );
    }

    #[test]
    fn only_bulk_families_are_scanned_over_usb() {
        assert!(is_bulk_screen(UsbId::new(0x1cbe, 0x0088)));
        assert!(is_bulk_screen(UsbId::new(0x43a8, 0x0e5e)));
        assert!(!is_bulk_screen(UsbId::new(0x0525, 0xa4a7)));
        assert!(!is_bulk_screen(UsbId::new(0x046d, 0x082d)));
    }
}
