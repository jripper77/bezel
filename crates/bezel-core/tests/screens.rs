//! Screen use cases through the device adapter's fakes.

use bezel_core::BezelError;
use bezel_core::app::{choose_screen, discover_screens, open_screen};
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint, ScreenState, group_screens};
use bezel_devices::{FakeBus, FakeConnector};

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
fn discover_groups_the_turing_88() {
    let screens = discover_screens(&FakeBus::turing_88()).expect("discovers");
    assert_eq!(screens.len(), 1);
    assert!(discover_screens(&FakeBus::default()).expect("discovers").is_empty());
}

#[test]
fn choose_prefers_address_then_awake() {
    let asleep = ep("COM3", 0x1a86, 0xca21, Some("CT21INCH"));
    let awake = ep("COM9", 0x1cbe, 0x0088, None);
    let screens = || group_screens(vec![asleep.clone(), awake.clone()]);
    let chosen = choose_screen(screens(), None).expect("awake one");
    assert_eq!(chosen.address().map(|a| a.0.as_str()), Some("COM9"));
    let chosen = choose_screen(screens(), Some("COM3")).expect("by address");
    assert_eq!(chosen.state(), ScreenState::Asleep);
    assert!(matches!(choose_screen(screens(), Some("COM1")), Err(BezelError::ScreenNotFound(_))));
    assert!(matches!(choose_screen(vec![], None), Err(BezelError::ScreenNotFound(_))));
}

#[test]
fn open_screen_goes_through_the_connector() {
    let link = open_screen(&FakeBus::turing_88(), &FakeConnector::default(), None).expect("opens");
    assert_eq!(link.identity().model.id.0, "turing-8.8");
    let ambiguous = FakeBus::new(vec![ep("COM3", 0x1a86, 0xca21, Some("CT21INCH"))]);
    assert!(open_screen(&ambiguous, &FakeConnector::default(), None).is_err());
}
