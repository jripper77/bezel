//! Opening discovered screens with the right driver.

use std::time::Duration;

use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::discovery::{Endpoint, Screen, UsbLocation, group_screens};
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};
use bezel_core::{BezelError, Result};

use crate::discovery::SystemBus;
use crate::driver::kipye_rev_d::KipyeRevD;
use crate::driver::turing_rev_a::TuringRevA;
use crate::driver::turing_rev_c::TuringRevC;
use crate::driver::turing_usb::{self, TuringUsb};
use crate::driver::wch::{self, Wch};
use crate::driver::weact::WeAct;
use crate::driver::xuanfang_rev_b::XuanFangRevB;
use crate::driver::{Pause, RealTime};
use crate::usb::{Endpoints, UsbWire};
use crate::wire::{Flow, SerialWire, Wire};

/// Wake attempts, [`WAKE_STEP`] apart: a rev C SoC takes about 11 s to boot
/// after its MCU is poked when it has slept a while, longer right after it
/// shut down; 30 attempts give it about 30 s.
const WAKE_TRIES: usize = 30;
/// Delay between wake attempts.
const WAKE_STEP: Duration = Duration::from_secs(1);
/// Checks that a rev C SoC which is shutting down left the bus, [`LEAVE_STEP`]
/// apart: about 5 s.
const LEAVE_POLLS: usize = 20;
/// Delay between checks that it left.
const LEAVE_STEP: Duration = Duration::from_millis(250);

/// Connects to real screens through the host's serial ports and USB.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemConnector;

impl ScreenConnector for SystemConnector {
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>> {
        let models = &screen.candidates;
        match screen.family {
            Family::TuringRevC => RevCHost::SYSTEM.connect_rev_c(screen, models),
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

/// The serial ports the rev C path opens: the display to talk to it, the
/// MCU to wake it. The host's are [`SystemPorts`]; tests script their own.
trait SerialPorts {
    /// What an opened display port talks through.
    type Wire: Wire + 'static;
    /// Opens `endpoint` to talk to it; `InUse` when another program holds it.
    fn open(&self, endpoint: &Endpoint, flow: Flow) -> Result<Self::Wire>;
    /// Opens and closes `endpoint` without writing a byte: the rev C MCU
    /// wake. A port that does not open is skipped (the next attempt retries).
    fn poke(&self, endpoint: &Endpoint);
}

/// The host's serial ports.
#[derive(Debug, Clone, Copy, Default)]
struct SystemPorts;

impl SerialPorts for SystemPorts {
    type Wire = SerialWire;

    fn open(&self, endpoint: &Endpoint, flow: Flow) -> Result<SerialWire> {
        open_serial(endpoint, flow)
    }

    fn poke(&self, endpoint: &Endpoint) {
        if let Ok(port) = SerialWire::open(&endpoint.address.0, Flow::None) {
            drop(port);
        }
    }
}

/// What opening a rev C screen needs from the host: the bus it enumerates,
/// the serial ports it opens and the pauses between attempts. Every wait
/// is a bounded count of pauses, so a fake pause makes the path instant.
struct RevCHost<B, S, P> {
    bus: B,
    ports: S,
    pause: P,
}

impl RevCHost<SystemBus, SystemPorts, RealTime> {
    /// The real host.
    const SYSTEM: Self = Self {
        bus: SystemBus,
        ports: SystemPorts,
        pause: RealTime,
    };
}

impl<B, S, P> RevCHost<B, S, P>
where
    B: DeviceBus,
    S: SerialPorts,
    P: Pause + Clone + 'static,
{
    /// Opens a rev C screen, waking it when it sleeps. A display that fails
    /// its handshake with a transport error or a timeout is shutting down:
    /// the vendor app and turing-smart-screen-python send TURNOFF when they
    /// exit, and the SoC then leaves the bus. Wait for it to go, wake it,
    /// try again.
    fn connect_rev_c(
        &self,
        screen: &Screen,
        models: &[&'static DeviceModel],
    ) -> Result<Box<dyn ScreenLink>> {
        let Some(display) = &screen.display else {
            return self.open_rev_c(&self.wake_rev_c(screen)?, models);
        };
        match self.open_rev_c(display, models) {
            Err(BezelError::Transport(reason) | BezelError::Timeout(reason))
                if screen.wake.is_some() =>
            {
                tracing::debug!(%reason, "rev C handshake failed; waking the screen and retrying");
                self.wait_until_gone(display);
                self.open_rev_c(&self.wake_rev_c(screen)?, models)
            }
            other => other,
        }
    }

    fn open_rev_c(
        &self,
        display: &Endpoint,
        models: &[&'static DeviceModel],
    ) -> Result<Box<dyn ScreenLink>> {
        let wire = self.ports.open(display, Flow::None)?;
        Ok(Box::new(TuringRevC::connect(wire, &self.pause, models)?))
    }

    /// Waits (at most [`LEAVE_POLLS`] checks) until `endpoint` is no longer
    /// connected. A bus that cannot be read counts as gone.
    fn wait_until_gone(&self, endpoint: &Endpoint) {
        for _ in 0..LEAVE_POLLS {
            let present = self
                .bus
                .endpoints()
                .is_ok_and(|all| all.iter().any(|e| e.address == endpoint.address));
            if !present {
                return;
            }
            self.pause.pause(LEAVE_STEP);
        }
    }

    /// Wakes a sleeping rev C screen: opening and closing its MCU port makes
    /// it boot the SoC (the only side effect discovery-adjacent code may
    /// have; no byte is written). Returns the SoC endpoint once it
    /// enumerates behind the MCU's hub, after at most [`WAKE_TRIES`] pokes.
    fn wake_rev_c(&self, screen: &Screen) -> Result<Endpoint> {
        let wake = screen
            .wake
            .as_ref()
            .ok_or_else(|| BezelError::ScreenNotFound("rev C screen without endpoints".into()))?;
        let hub = wake.location.as_ref().and_then(UsbLocation::parent);
        for _ in 0..WAKE_TRIES {
            self.ports.poke(wake);
            self.pause.pause(WAKE_STEP);
            if let Some(display) = awake_display(self.bus.endpoints()?, hub.as_ref()) {
                return Ok(display);
            }
        }
        Err(BezelError::Timeout("the screen did not wake up".into()))
    }
}

/// The display endpoint of an awake rev C screen behind `hub` (any hub when
/// the MCU's location is unknown).
fn awake_display(endpoints: Vec<Endpoint>, hub: Option<&UsbLocation>) -> Option<Endpoint> {
    group_screens(endpoints)
        .into_iter()
        .filter(|s| s.family == Family::TuringRevC)
        .find_map(|s| {
            let display = s.display?;
            let same_hub = hub.is_none()
                || display
                    .location
                    .as_ref()
                    .and_then(UsbLocation::parent)
                    .as_ref()
                    == hub;
            same_hub.then_some(display)
        })
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex, PoisonError};

    use super::*;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::{ModelId, Transport, UsbId};
    use bezel_core::domain::discovery::DeviceAddress;

    const MCU: &str = "/dev/ttyACM0";
    const SOC: &str = "/dev/ttyACM1";
    const HELLO_88: &str = "chs_88inch.dev1_rom1.90";

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

    /// A USB serial endpoint on bus 3 at hub port chain `ports`.
    fn at(addr: &str, usb: UsbId, serial: Option<&str>, ports: &[u8]) -> Endpoint {
        Endpoint {
            usb,
            serial_number: serial.map(str::to_string),
            location: Some(UsbLocation {
                bus: "3".into(),
                ports: ports.to_vec(),
            }),
            ..endpoint(addr)
        }
    }

    /// The 8.8"'s MCU, behind hub 3-1.
    fn mcu() -> Endpoint {
        at(MCU, UsbId::new(0x1a86, 0xca88), Some("CT88INCH"), &[1, 1])
    }

    /// An 8.8" SoC at `addr` and hub port chain `ports`.
    fn soc(addr: &str, ports: &[u8]) -> Endpoint {
        at(addr, UsbId::new(0x0525, 0xa4a7), None, ports)
    }

    fn rev_c(display: Option<Endpoint>, wake: Option<Endpoint>) -> Screen {
        Screen {
            family: Family::TuringRevC,
            candidates: vec![model_by_id(ModelId("turing-8.8")).unwrap()],
            display,
            wake,
        }
    }

    /// A bus answering from a script, then repeating its last answer.
    struct ScriptedBus(RefCell<VecDeque<Result<Vec<Endpoint>>>>);

    impl ScriptedBus {
        fn new(answers: impl IntoIterator<Item = Result<Vec<Endpoint>>>) -> Self {
            Self(RefCell::new(answers.into_iter().collect()))
        }
    }

    impl DeviceBus for ScriptedBus {
        fn endpoints(&self) -> Result<Vec<Endpoint>> {
            let mut answers = self.0.borrow_mut();
            if answers.len() > 1 {
                return answers.pop_front().unwrap();
            }
            answers.front().cloned().unwrap_or(Ok(Vec::new()))
        }
    }

    /// Serial ports whose opens answer from a script (then fail as a missing
    /// port), recording every open and poke in order.
    #[derive(Default)]
    struct ScriptedPorts {
        opens: RefCell<VecDeque<Result<ScriptedWire>>>,
        log: RefCell<Vec<String>>,
    }

    impl ScriptedPorts {
        fn new(opens: impl IntoIterator<Item = Result<ScriptedWire>>) -> Self {
            Self {
                opens: RefCell::new(opens.into_iter().collect()),
                log: RefCell::default(),
            }
        }

        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl SerialPorts for ScriptedPorts {
        type Wire = ScriptedWire;

        fn open(&self, endpoint: &Endpoint, flow: Flow) -> Result<ScriptedWire> {
            assert_eq!(flow, Flow::None, "rev C ports open without flow control");
            let address = &endpoint.address.0;
            self.log.borrow_mut().push(format!("open {address}"));
            let next = self.opens.borrow_mut().pop_front();
            next.unwrap_or_else(|| Err(BezelError::Transport(format!("{address}: no such port"))))
        }

        fn poke(&self, endpoint: &Endpoint) {
            let address = &endpoint.address.0;
            self.log.borrow_mut().push(format!("poke {address}"));
        }
    }

    /// Records every pause instead of sleeping.
    #[derive(Clone, Default)]
    struct Pauses(Arc<Mutex<Vec<Duration>>>);

    impl Pause for Pauses {
        fn pause(&self, d: Duration) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(d);
        }
    }

    impl Pauses {
        fn taken(&self) -> Vec<Duration> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    /// A display that answers HELLO and STOP_MEDIA.
    fn answering() -> Result<ScriptedWire> {
        Ok(ScriptedWire::with_replies([
            HELLO_88.as_bytes().to_vec(),
            b"media_stop".to_vec(),
        ]))
    }

    /// A display that answers nothing: its HELLO times out.
    fn mute() -> Result<ScriptedWire> {
        Ok(ScriptedWire::default())
    }

    /// A display port that vanished between discovery and the open.
    fn vanished() -> Result<ScriptedWire> {
        Err(BezelError::Transport(format!("{SOC}: no such port")))
    }

    type Host = RevCHost<ScriptedBus, ScriptedPorts, Pauses>;

    fn fake_host(bus: ScriptedBus, ports: ScriptedPorts) -> Host {
        RevCHost {
            bus,
            ports,
            pause: Pauses::default(),
        }
    }

    /// The leave checks' and wake attempts' pauses, the driver's 200 ms
    /// STOP_VIDEO settle left out. Only for scripts where HELLO answers at
    /// once: its 1 s retry pause would pass for a wake step.
    fn waits(host: &Host) -> Vec<Duration> {
        host.pause
            .taken()
            .into_iter()
            .filter(|d| *d == LEAVE_STEP || *d == WAKE_STEP)
            .collect()
    }

    #[test]
    fn rev_c_wakes_and_retries_with_injected_bus() {
        // The display fails its handshake (it is shutting down): it stays on
        // the bus for two checks, then only the MCU is left until the third
        // poke brings the SoC back under a new name, behind the same hub.
        let old = soc(SOC, &[1, 2]);
        let new = soc("/dev/ttyACM2", &[1, 3]);
        let bus = ScriptedBus::new([
            Ok(vec![mcu(), old.clone()]),
            Ok(vec![mcu(), old.clone()]),
            Ok(vec![mcu()]),
            Ok(vec![mcu()]),
            Ok(vec![mcu()]),
            Ok(vec![mcu(), new]),
        ]);
        let host = fake_host(bus, ScriptedPorts::new([mute(), answering()]));
        let screen = rev_c(Some(old), Some(mcu()));
        let link = host
            .connect_rev_c(&screen, &screen.candidates)
            .expect("woken and connected");
        assert_eq!(link.identity().firmware.as_deref(), Some(HELLO_88));
        assert_eq!(
            host.ports.log(),
            [
                "open /dev/ttyACM1",
                "poke /dev/ttyACM0",
                "poke /dev/ttyACM0",
                "poke /dev/ttyACM0",
                "open /dev/ttyACM2"
            ]
        );
        // The three HELLO retries (1 s each) come first, then the leave
        // checks, then one wake step after each poke.
        let all = host.pause.taken();
        assert_eq!(
            all[3..8],
            [LEAVE_STEP, LEAVE_STEP, WAKE_STEP, WAKE_STEP, WAKE_STEP]
        );
    }

    #[test]
    fn a_sleeping_rev_c_screen_is_woken_without_waiting_for_it_to_leave() {
        let bus = ScriptedBus::new([Ok(vec![mcu()]), Ok(vec![mcu(), soc(SOC, &[1, 2])])]);
        let host = fake_host(bus, ScriptedPorts::new([answering()]));
        let screen = rev_c(None, Some(mcu()));
        assert!(host.connect_rev_c(&screen, &screen.candidates).is_ok());
        assert_eq!(
            host.ports.log(),
            [
                "poke /dev/ttyACM0",
                "poke /dev/ttyACM0",
                "open /dev/ttyACM1"
            ]
        );
        assert_eq!(waits(&host), [WAKE_STEP, WAKE_STEP]);
    }

    #[test]
    fn a_rev_c_screen_that_never_wakes_times_out_after_the_last_poke() {
        // A rev C display on another hub is someone else's screen.
        let elsewhere = soc(SOC, &[2, 2]);
        let bus = ScriptedBus::new([Ok(vec![mcu(), elsewhere])]);
        let host = fake_host(bus, ScriptedPorts::default());
        let screen = rev_c(None, Some(mcu()));
        let err = host
            .connect_rev_c(&screen, &screen.candidates)
            .err()
            .unwrap();
        assert_eq!(
            err,
            BezelError::Timeout("the screen did not wake up".into())
        );
        assert_eq!(host.ports.log().len(), WAKE_TRIES, "pokes only");
        assert_eq!(waits(&host), [WAKE_STEP; WAKE_TRIES]);
    }

    #[test]
    fn a_display_that_never_leaves_is_waited_for_then_reopened() {
        // Still listed after every check: the wait gives up and the wake
        // finds it at once. An MCU without a location takes any hub's SoC.
        let display = soc(SOC, &[1, 2]);
        let loose_mcu = Endpoint {
            location: None,
            ..mcu()
        };
        let bus = ScriptedBus::new([Ok(vec![loose_mcu.clone(), display.clone()])]);
        let host = fake_host(bus, ScriptedPorts::new([vanished(), answering()]));
        let screen = rev_c(Some(display), Some(loose_mcu));
        assert!(host.connect_rev_c(&screen, &screen.candidates).is_ok());
        let mut expected = vec![LEAVE_STEP; LEAVE_POLLS];
        expected.push(WAKE_STEP);
        assert_eq!(waits(&host), expected);
        assert_eq!(
            host.ports.log(),
            [
                "open /dev/ttyACM1",
                "poke /dev/ttyACM0",
                "open /dev/ttyACM1"
            ]
        );
    }

    #[test]
    fn an_unreadable_bus_counts_as_gone_and_fails_the_wake() {
        let broken = || Err(BezelError::Transport("serial port enumeration".into()));
        let host = fake_host(
            ScriptedBus::new([broken()]),
            ScriptedPorts::new([vanished()]),
        );
        let display = soc(SOC, &[1, 2]);
        let screen = rev_c(Some(display), Some(mcu()));
        let err = host
            .connect_rev_c(&screen, &screen.candidates)
            .err()
            .unwrap();
        assert_eq!(err, broken().unwrap_err());
        assert_eq!(waits(&host), [WAKE_STEP], "no leave check waited");
    }

    #[test]
    fn rev_c_failures_without_a_wake_or_of_another_kind_are_returned() {
        // No MCU to poke: the handshake timeout stands.
        let host = fake_host(ScriptedBus::new([]), ScriptedPorts::new([mute()]));
        let screen = rev_c(Some(soc(SOC, &[1, 2])), None);
        let err = host
            .connect_rev_c(&screen, &screen.candidates)
            .err()
            .unwrap();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert_eq!(host.ports.log(), ["open /dev/ttyACM1"]);

        // A port another program holds is not a sleeping screen.
        let in_use = BezelError::InUse {
            address: SOC.into(),
            holders: vec!["turing-smart-screen (pid 42)".into()],
        };
        let host = fake_host(
            ScriptedBus::new([]),
            ScriptedPorts::new([Err(in_use.clone())]),
        );
        let screen = rev_c(Some(soc(SOC, &[1, 2])), Some(mcu()));
        let err = host
            .connect_rev_c(&screen, &screen.candidates)
            .err()
            .unwrap();
        assert_eq!(err, in_use);
        assert_eq!(host.ports.log(), ["open /dev/ttyACM1"]);
        assert!(host.pause.taken().is_empty());
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
        // The real wake poke of a missing port does nothing.
        SystemPorts.poke(&endpoint("/dev/bezel-no-such-port"));
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
