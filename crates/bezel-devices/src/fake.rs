//! In-memory bus for tests and demos: returns a fixed list of endpoints.

use std::sync::{Arc, Mutex, PoisonError};

use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint, Screen, UsbLocation};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};
use bezel_core::{BezelError, Result};

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

/// What a [`FakeScreen`] was asked to do, shared with the test that built it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FakeLog {
    /// Frames presented, in order.
    pub frames: Vec<Frame>,
    /// Brightness levels set.
    pub brightness: Vec<Brightness>,
    /// Orientations set.
    pub orientations: Vec<Orientation>,
    /// `screen_off` calls.
    pub offs: usize,
    /// `release` calls.
    pub releases: usize,
}

/// Connects to an in-memory screen that records everything.
#[derive(Debug, Clone, Default)]
pub struct FakeConnector {
    log: Arc<Mutex<FakeLog>>,
}

impl FakeConnector {
    /// A snapshot of what the screens were asked to do.
    pub fn log(&self) -> FakeLog {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ScreenConnector for FakeConnector {
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        let model = screen
            .model()
            .ok_or_else(|| BezelError::Transport("simulated screen needs a single model".into()))?;
        Ok(Box::new(FakeScreen {
            identity: ScreenIdentity {
                model,
                firmware: Some("simulated".into()),
            },
            orientation: Orientation::Portrait,
            log: Arc::clone(&self.log),
        }))
    }
}

/// An in-memory screen that checks frame sizes like a real one.
#[derive(Debug)]
pub struct FakeScreen {
    identity: ScreenIdentity,
    orientation: Orientation,
    log: Arc<Mutex<FakeLog>>,
}

impl FakeScreen {
    fn record(&self, f: impl FnOnce(&mut FakeLog)) {
        f(&mut self.log.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl ScreenLink for FakeScreen {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        self.record(|l| l.brightness.push(brightness));
        Ok(())
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        self.orientation = orientation;
        self.record(|l| l.orientations.push(orientation));
        Ok(())
    }

    fn present(&mut self, frame: &Frame) -> Result<()> {
        let expected = self.identity.model.panel.in_orientation(self.orientation);
        if frame.size() != expected {
            return Err(BezelError::Transport(
                "frame size does not match the panel".into(),
            ));
        }
        self.record(|l| l.frames.push(frame.clone()));
        Ok(())
    }

    fn screen_off(&mut self) -> Result<()> {
        self.record(|l| l.offs += 1);
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        self.record(|l| l.releases += 1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::app::{discover_screens, open_screen};
    use bezel_core::domain::frame::Rgba;

    #[test]
    fn fake_screen_records_and_checks_sizes() {
        let connector = FakeConnector::default();
        let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        assert_eq!(link.identity().model.id.0, "turing-8.8");
        link.set_orientation(Orientation::Landscape).unwrap();
        let wide = Frame::filled(link.identity().model.panel.transposed(), Rgba::BLACK);
        link.present(&wide).unwrap();
        assert!(
            link.present(&Frame::filled(link.identity().model.panel, Rgba::BLACK))
                .is_err()
        );
        link.set_brightness(Brightness::MAX).unwrap();
        link.screen_off().unwrap();
        link.release().unwrap();
        let log = connector.log();
        assert_eq!(log.frames.len(), 1);
        assert_eq!(log.orientations, vec![Orientation::Landscape]);
        assert_eq!((log.offs, log.releases, log.brightness.len()), (1, 1, 1));
    }

    #[test]
    fn ambiguous_screens_cannot_be_simulated() {
        let bus = FakeBus::new(vec![serial_endpoint(
            "COM3",
            UsbId::new(0x1a86, 0xca21),
            Some("CT21INCH"),
            &[1],
        )]);
        assert!(open_screen(&bus, &FakeConnector::default(), None).is_err());
    }

    #[test]
    fn turing_88_preset_is_one_awake_screen() {
        let screens = discover_screens(&FakeBus::turing_88()).unwrap();
        assert_eq!(screens.len(), 1);
        assert!(screens[0].display.is_some() && screens[0].wake.is_some());
        assert!(discover_screens(&FakeBus::default()).unwrap().is_empty());
    }
}
