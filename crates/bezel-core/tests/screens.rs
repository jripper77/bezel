//! Screen use cases through the device adapter's fakes.

use std::cell::RefCell;

use bezel_core::app::{choose_screen, discover_screens, open_screen, restart_screen};
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{
    DeviceAddress, Endpoint, ScreenState, UsbLocation, group_screens,
};
use bezel_core::ports::DeviceBus;
use bezel_core::{BezelError, Result};
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
    assert!(
        discover_screens(&FakeBus::default())
            .expect("discovers")
            .is_empty()
    );
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
    let link = open_screen(&FakeBus::turing_88(), &FakeConnector::default(), None).expect("opens");
    assert_eq!(link.identity().model.id.0, "turing-8.8");
    let ambiguous = FakeBus::new(vec![ep("COM3", 0x1a86, 0xca21, Some("CT21INCH"))]);
    assert!(open_screen(&ambiguous, &FakeConnector::default(), None).is_err());
}

/// An endpoint behind hub 3-1, at port `port`.
fn behind_hub(addr: &str, vid: u16, pid: u16, serial: Option<&str>, port: u8) -> Endpoint {
    Endpoint {
        location: Some(UsbLocation {
            bus: "3".into(),
            ports: vec![1, port],
        }),
        ..ep(addr, vid, pid, serial)
    }
}

/// A bus answering from a script, then repeating its last answer.
struct ScriptedBus(RefCell<Vec<Vec<Endpoint>>>);

impl DeviceBus for ScriptedBus {
    fn endpoints(&self) -> Result<Vec<Endpoint>> {
        let mut answers = self.0.borrow_mut();
        if answers.len() > 1 {
            return Ok(answers.remove(0));
        }
        Ok(answers.first().cloned().unwrap_or_default())
    }
}

/// D-2026-09-30-release-polish-13: a rev C screen is restarted through the
/// connector and found again under the display's new address.
#[test]
fn a_rev_c_screen_restarts_and_is_found_under_its_new_address() {
    let mcu = behind_hub("/dev/ttyACM0", 0x1a86, 0xca88, Some("CT88INCH"), 1);
    let old = behind_hub("/dev/ttyACM1", 0x0525, 0xa4a7, None, 2);
    let new = behind_hub("/dev/ttyACM2", 0x0525, 0xa4a7, None, 2);
    let bus = ScriptedBus(RefCell::new(vec![
        vec![mcu.clone(), old],
        vec![mcu.clone(), new],
    ]));
    let connector = FakeConnector::default();
    let back = restart_screen(&bus, &connector, Some("/dev/ttyACM1")).expect("restarts");
    assert_eq!(connector.log().restarts, ["/dev/ttyACM1"]);
    assert_eq!(back.address().map(|a| a.0.as_str()), Some("/dev/ttyACM2"));
    assert_eq!(back.wake, Some(mcu));
    // The first awake screen by default; an unknown address is not found.
    let back = restart_screen(&FakeBus::turing_88(), &connector, None).expect("restarts");
    assert!(back.restartable());
    let missing = restart_screen(&FakeBus::turing_88(), &connector, Some("COM9"));
    assert!(matches!(missing, Err(BezelError::ScreenNotFound(_))));
    assert_eq!(connector.log().restarts.len(), 2);
}

#[test]
fn screens_without_an_mcu_are_not_restarted() {
    let connector = FakeConnector::default();
    // A WeAct 0.96" restarts with a replug only.
    let weact = FakeBus::new(vec![ep("/dev/ttyACM0", 0x1a86, 0xfe0c, Some("AD0001"))]);
    let err = restart_screen(&weact, &connector, None).unwrap_err();
    assert_eq!(
        err.to_string(),
        "not supported: restarting WeAct Studio Display FS 0.96\": only Turing rev C screens \
         restart, through their wake chip (MCU); unplug the screen and plug it back in"
    );
    // A rev C display whose MCU is not listed.
    let lone = FakeBus::new(vec![ep("/dev/ttyACM1", 0x0525, 0xa4a7, None)]);
    let err = restart_screen(&lone, &connector, None).unwrap_err();
    assert!(
        err.to_string()
            .contains("its wake chip (MCU) is not listed; unplug the screen"),
        "{err}"
    );
    assert!(connector.log().restarts.is_empty(), "nothing was sent");
}
