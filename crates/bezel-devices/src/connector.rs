//! Opening discovered screens with the right driver.

use std::time::Duration;

use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::discovery::{Endpoint, Screen, UsbLocation, group_screens};
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};
use bezel_core::{BezelError, Result};

use crate::busy::Holders;
use crate::discovery::SystemBus;
use crate::driver::kipye_rev_d::KipyeRevD;
use crate::driver::turing_rev_a::TuringRevA;
use crate::driver::turing_rev_c::TuringRevC;
use crate::driver::turing_usb::{self, TuringUsb};
use crate::driver::wch::{self, Wch};
use crate::driver::weact::WeAct;
use crate::driver::xuanfang_rev_b::XuanFangRevB;
use crate::driver::{Pause, RealTime, io_err};
use crate::protocol::turing_rev_c as proto;
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
/// How long the MCU port stays open after the restart command: the
/// vendor's 8000 ms (spec § 15; D-2026-09-30-release-polish-13).
const RESTART_HOLD: Duration = Duration::from_secs(8);
/// Longest wait, after the hold, for the SoC to be back on the bus: the
/// 8.8"'s returns about 10 s after the command (2 s after the hold); 30 s
/// leaves room for slower boots.
const RESTART_RETURN: Duration = Duration::from_secs(30);
/// Delay between looks at the bus while the SoC comes back.
const RESTART_POLL: Duration = Duration::from_millis(500);
/// Looks at the bus within [`RESTART_RETURN`].
const RESTART_POLLS: u128 = RESTART_RETURN.as_millis() / RESTART_POLL.as_millis();

/// Connects to real screens through the host's serial ports and USB.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemConnector;

impl ScreenConnector for SystemConnector {
    /// A rev C screen with its MCU listed restarts through it
    /// ([`RevCHost::restart_rev_c`]); any other screen is `Unsupported`.
    fn restart(&self, screen: &Screen) -> Result<()> {
        match (&screen.wake, screen.family) {
            (Some(wake), Family::TuringRevC) => RevCHost::SYSTEM
                .restart_rev_c(wake, screen.display.as_ref())
                .map(drop),
            _ => Err(BezelError::Unsupported(format!(
                "restarting a {} screen without its wake chip (MCU)",
                screen.family.slug()
            ))),
        }
    }

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

/// The serial ports a screen is opened through: every serial family's
/// display, and the rev C MCU that wakes and restarts it. The host's are
/// [`SystemPorts`]; tests script their own.
trait SerialPorts {
    /// What an opened port talks through.
    type Wire: Wire + 'static;
    /// One try at opening `endpoint`, nothing looked at first: the
    /// system's refusal when it does not open.
    fn try_open(&self, endpoint: &Endpoint, flow: Flow) -> Result<Self::Wire>;
    /// Opens and closes `endpoint` without writing a byte: the rev C MCU
    /// wake. A port that does not open is skipped (the next attempt retries).
    fn poke(&self, endpoint: &Endpoint);
    /// The processes holding `endpoint` open, in one look: this one (as
    /// `"<command> (PID <pid>)"`, like the others) told apart from the
    /// other programs.
    fn holders(&self, endpoint: &Endpoint) -> Holders;

    /// Opens `endpoint` to talk to it ([`Self::try_open`]), after one look
    /// at who holds it ([`Self::holders`]). While other programs hold it,
    /// `InUse` before the port is touched (D-2026-09-30-device-protocols-3).
    /// Ports open exclusively, so one this process already holds is refused
    /// as busy: that refusal is `InUse` naming this process, at once, never
    /// a display shutting down, so no wait, wake or restart follows
    /// (D-2026-10-01-live-screen-controls-4). Other refusals stand.
    fn open(&self, endpoint: &Endpoint, flow: Flow) -> Result<Self::Wire> {
        let Holders { this, others } = self.holders(endpoint);
        let in_use = |holders| BezelError::InUse {
            address: endpoint.address.0.clone(),
            holders,
        };
        if !others.is_empty() {
            return Err(in_use(others));
        }
        match (self.try_open(endpoint, flow), this) {
            (Err(BezelError::Transport(_)), Some(this)) => Err(in_use(vec![this])),
            (opened, _) => opened,
        }
    }
}

/// The host's serial ports.
#[derive(Debug, Clone, Copy, Default)]
struct SystemPorts;

impl SerialPorts for SystemPorts {
    type Wire = SerialWire;

    fn try_open(&self, endpoint: &Endpoint, flow: Flow) -> Result<SerialWire> {
        let address = &endpoint.address.0;
        SerialWire::open(address, flow).map_err(|e| access_error(address, &e))
    }

    fn poke(&self, endpoint: &Endpoint) {
        if let Ok(port) = SerialWire::open(&endpoint.address.0, Flow::None) {
            drop(port);
        }
    }

    /// One scan of `/proc` (nobody where it does not exist).
    fn holders(&self, endpoint: &Endpoint) -> Holders {
        crate::busy::on_this_machine(&endpoint.address.0)
    }
}

/// What the first try of a rev C connection found.
enum FirstTry {
    /// Connected.
    Open(Box<dyn ScreenLink>),
    /// A display is on the bus but its handshake timed out or stalled: the
    /// SoC hung. It is restarted through the MCU.
    Silent(Endpoint, BezelError),
}

/// A handshake that got no answer: HELLO timed out, or the SoC stopped
/// reading what was sent.
fn silent(error: &BezelError) -> bool {
    matches!(error, BezelError::Timeout(_) | BezelError::Hung(_))
}

/// The connection after a restart failed: a SoC still silent needs a replug.
fn after_restart(error: BezelError) -> BezelError {
    if silent(&error) {
        return BezelError::Timeout(
            "the screen: it answered no HELLO even after a restart through its MCU; \
             unplug it and plug it back in"
                .into(),
        );
    }
    error
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
    /// Opens a rev C screen, waking it when it sleeps ([`Self::open_or_wake`]).
    /// A SoC that stays on the bus but answers no HELLO, or stopped reading,
    /// has hung: it is restarted through the MCU once and the connection is
    /// tried once more (D-2026-09-30-release-polish-13). A screen that
    /// answers is never restarted.
    fn connect_rev_c(
        &self,
        screen: &Screen,
        models: &[&'static DeviceModel],
    ) -> Result<Box<dyn ScreenLink>> {
        let Some(wake) = &screen.wake else {
            let display = screen.display.as_ref().ok_or_else(|| {
                BezelError::ScreenNotFound("rev C screen without endpoints".into())
            })?;
            return self.open_rev_c(display, models);
        };
        let (soc, why) = match self.open_or_wake(screen, models)? {
            FirstTry::Open(link) => return Ok(link),
            FirstTry::Silent(soc, why) => (soc, why),
        };
        tracing::warn!(
            soc = %soc.address,
            %why,
            "the screen is on the bus but answers no HELLO; restarting it through its MCU"
        );
        let back = self.restart_rev_c(wake, Some(&soc))?;
        self.open_rev_c(&back, models).map_err(after_restart)
    }

    /// The first try: the listed display, or the one waking the screen
    /// brings up. A display that fails its handshake and leaves the bus was
    /// shutting down (the vendor app and turing-smart-screen-python send
    /// TURNOFF when they exit): it is woken and tried again; so is one that
    /// stays after its port failed to open. One that stays and answers
    /// nothing is [`FirstTry::Silent`].
    fn open_or_wake(&self, screen: &Screen, models: &[&'static DeviceModel]) -> Result<FirstTry> {
        let (mut display, mut woken) = match &screen.display {
            Some(display) => (display.clone(), false),
            None => (self.wake_rev_c(screen)?, true),
        };
        loop {
            let error = match self.open_rev_c(&display, models) {
                Ok(link) => return Ok(FirstTry::Open(link)),
                Err(e) => e,
            };
            let quiet = silent(&error);
            let shutting_down = quiet || matches!(error, BezelError::Transport(_));
            if woken || !shutting_down {
                return if quiet {
                    Ok(FirstTry::Silent(display, error))
                } else {
                    Err(error)
                };
            }
            tracing::debug!(%error, "rev C handshake failed; waiting for the display to leave");
            if !self.wait_until_gone(&display) && quiet {
                return Ok(FirstTry::Silent(display, error));
            }
            display = self.wake_rev_c(screen)?;
            woken = true;
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
    /// connected; `true` once it left. A bus that cannot be read counts as
    /// gone.
    fn wait_until_gone(&self, endpoint: &Endpoint) -> bool {
        for _ in 0..LEAVE_POLLS {
            let present = self
                .bus
                .endpoints()
                .is_ok_and(|all| all.iter().any(|e| e.address == endpoint.address));
            if !present {
                return true;
            }
            self.pause.pause(LEAVE_STEP);
        }
        false
    }

    /// Restarts the SoC through the MCU `wake` (spec § 15, § 16;
    /// D-2026-09-30-release-polish-13): the port opened like the vendor does
    /// (115200 8N1, DTR and RTS on), exactly [`proto::MCU_RESTART`] written,
    /// held [`RESTART_HOLD`], closed. The SoC leaves the bus at once and is
    /// back about 10 s later under a new device number, also from a hung
    /// firmware: once `display` (the SoC as it was listed) left, the display
    /// back behind the MCU's hub is returned, whichever endpoint it is.
    /// Refused with `InUse`, before anything is sent, while another program
    /// holds the SoC's port (D-2026-09-30-device-protocols-3): restarting
    /// would pull the screen from under it.
    fn restart_rev_c(&self, wake: &Endpoint, display: Option<&Endpoint>) -> Result<Endpoint> {
        if let Some(display) = display {
            let holders = self.ports.holders(display).others;
            if !holders.is_empty() {
                return Err(BezelError::InUse {
                    address: display.address.0.clone(),
                    holders,
                });
            }
        }
        tracing::info!(mcu = %wake.address, "restarting the rev C screen through its MCU");
        let mut mcu = self.ports.open(wake, Flow::None)?;
        mcu.send(&proto::MCU_RESTART).map_err(io_err)?;
        self.pause.pause(RESTART_HOLD);
        drop(mcu);
        if let Some(display) = display {
            self.wait_until_gone(display);
        }
        self.wait_for_display(wake)
    }

    /// The SoC display back behind the hub of `wake`, looking every
    /// [`RESTART_POLL`] for [`RESTART_RETURN`]; a bus that cannot be read
    /// is looked at again. A clear timeout when it does not come back.
    fn wait_for_display(&self, wake: &Endpoint) -> Result<Endpoint> {
        let hub = wake.location.as_ref().and_then(UsbLocation::parent);
        for _ in 0..RESTART_POLLS {
            let back = self
                .bus
                .endpoints()
                .ok()
                .and_then(|all| awake_display(all, hub.as_ref()));
            if let Some(soc) = back {
                tracing::info!(soc = %soc.address, "the rev C screen is back");
                return Ok(soc);
            }
            self.pause.pause(RESTART_POLL);
        }
        Err(BezelError::Timeout(format!(
            "the screen after its restart: it did not come back within {} s; \
             unplug it and plug it back in",
            RESTART_RETURN.as_secs()
        )))
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

/// Opens the display port of a serial family on this host
/// ([`SerialPorts::open`]).
fn open_serial(endpoint: &Endpoint, flow: Flow) -> Result<SerialWire> {
    SystemPorts.open(endpoint, flow)
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

    /// Everything the fake host saw, in order: opens, bytes sent, closes and
    /// pauses (the restart tests read it).
    type Journal = Arc<Mutex<Vec<String>>>;

    fn note(journal: &Journal, entry: String) {
        journal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    }

    fn entries(journal: &Journal) -> Vec<String> {
        journal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What an open of the script gives: a scripted wire, or one whose
    /// device stopped reading (every send stalls).
    struct Opened {
        wire: ScriptedWire,
        stalls: bool,
    }

    /// A port opened by [`ScriptedPorts`]: notes what is sent and its close.
    struct Noted {
        opened: Opened,
        address: String,
        journal: Journal,
    }

    impl Wire for Noted {
        fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            note(&self.journal, format!("send {} {hex}", self.address));
            if self.opened.stalls {
                return Err(crate::wire::Stalled { queued: Some(250) }.error());
            }
            self.opened.wire.send(bytes)
        }

        fn receive(&mut self, max: usize, timeout: Duration) -> std::io::Result<Vec<u8>> {
            self.opened.wire.receive(max, timeout)
        }

        fn discard_input(&mut self) -> std::io::Result<()> {
            self.opened.wire.discard_input()
        }
    }

    impl Drop for Noted {
        fn drop(&mut self) {
            note(&self.journal, format!("close {}", self.address));
        }
    }

    /// Serial ports whose opens answer from a script (then fail as a missing
    /// port), recording every open and poke in order.
    #[derive(Default)]
    struct ScriptedPorts {
        opens: RefCell<VecDeque<Result<Opened>>>,
        log: RefCell<Vec<String>>,
        journal: Journal,
        /// Other programs holding every port.
        held_by: Vec<String>,
        /// This process, holding every port itself.
        held_here: Option<String>,
    }

    impl ScriptedPorts {
        fn new(opens: impl IntoIterator<Item = Result<Opened>>) -> Self {
            Self {
                opens: RefCell::new(opens.into_iter().collect()),
                ..Self::default()
            }
        }

        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl SerialPorts for ScriptedPorts {
        type Wire = Noted;

        fn try_open(&self, endpoint: &Endpoint, flow: Flow) -> Result<Noted> {
            assert_eq!(flow, Flow::None, "rev C ports open without flow control");
            let address = &endpoint.address.0;
            self.log.borrow_mut().push(format!("open {address}"));
            note(&self.journal, format!("open {address}"));
            let next = self.opens.borrow_mut().pop_front();
            let opened = next.unwrap_or_else(|| {
                Err(BezelError::Transport(format!("{address}: no such port")))
            })?;
            Ok(Noted {
                opened,
                address: address.clone(),
                journal: Arc::clone(&self.journal),
            })
        }

        fn poke(&self, endpoint: &Endpoint) {
            let address = &endpoint.address.0;
            self.log.borrow_mut().push(format!("poke {address}"));
        }

        fn holders(&self, _endpoint: &Endpoint) -> Holders {
            Holders {
                this: self.held_here.clone(),
                others: self.held_by.clone(),
            }
        }
    }

    /// Records every pause instead of sleeping (and notes it in the
    /// journal, when it has one).
    #[derive(Clone, Default)]
    struct Pauses(Arc<Mutex<Vec<Duration>>>, Option<Journal>);

    impl Pause for Pauses {
        fn pause(&self, d: Duration) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(d);
            if let Some(journal) = &self.1 {
                note(journal, format!("pause {d:?}"));
            }
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
    fn answering() -> Result<Opened> {
        Ok(Opened {
            wire: ScriptedWire::with_replies([
                HELLO_88.as_bytes().to_vec(),
                b"media_stop".to_vec(),
            ]),
            stalls: false,
        })
    }

    /// A display that answers nothing: its HELLO times out. Also the MCU,
    /// which answers nothing either.
    fn mute() -> Result<Opened> {
        Ok(Opened {
            wire: ScriptedWire::default(),
            stalls: false,
        })
    }

    /// A display whose firmware hung: it stopped reading, so HELLO stalls.
    fn stalled() -> Result<Opened> {
        Ok(Opened {
            wire: ScriptedWire::default(),
            stalls: true,
        })
    }

    /// A display port that vanished between discovery and the open.
    fn vanished() -> Result<Opened> {
        Err(BezelError::Transport(format!("{SOC}: no such port")))
    }

    type Host = RevCHost<ScriptedBus, ScriptedPorts, Pauses>;

    fn fake_host(bus: ScriptedBus, ports: ScriptedPorts) -> Host {
        let journal = Arc::clone(&ports.journal);
        RevCHost {
            bus,
            ports,
            pause: Pauses(Arc::default(), Some(journal)),
        }
    }

    /// The MCU's part of the journal: its opens, sends and closes, and the
    /// pauses while its port is open.
    fn mcu_journal(host: &Host) -> Vec<String> {
        let mut open = false;
        entries(&host.ports.journal)
            .into_iter()
            .filter(|e| {
                if e.ends_with(MCU) {
                    open = e.starts_with("open");
                    return true;
                }
                e.contains(&format!("{MCU} ")) || (open && e.starts_with("pause"))
            })
            .collect()
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

    /// Every MCU command the journal saw.
    fn mcu_sends(host: &Host) -> usize {
        let send = format!("send {MCU} ");
        entries(&host.ports.journal)
            .iter()
            .filter(|e| e.starts_with(&send))
            .count()
    }

    #[test]
    fn the_mcu_restart_sends_six_bytes_holds_8_s_and_waits_for_the_soc() {
        // D-2026-09-30-release-polish-13: after the hold the SoC is listed
        // once more, then gone for two looks, then back under a new name
        // behind the same hub.
        let old = soc(SOC, &[1, 2]);
        let new = soc("/dev/ttyACM2", &[1, 2]);
        let bus = ScriptedBus::new([
            Ok(vec![mcu(), old.clone()]),
            Ok(vec![mcu()]),
            Ok(vec![mcu()]),
            Ok(vec![mcu()]),
            Ok(vec![mcu(), new.clone()]),
        ]);
        let host = fake_host(bus, ScriptedPorts::new([mute()]));
        let back = host.restart_rev_c(&mcu(), Some(&old)).expect("back");
        assert_eq!(back, new, "whichever endpoint came back");
        assert_eq!(
            mcu_journal(&host),
            [
                "open /dev/ttyACM0",
                "send /dev/ttyACM0 0000000000c9",
                "pause 8s",
                "close /dev/ttyACM0"
            ],
            "exactly the six bytes, unpadded, held 8 s"
        );
        assert_eq!(
            host.pause.taken(),
            [RESTART_HOLD, LEAVE_STEP, RESTART_POLL, RESTART_POLL]
        );
        assert_eq!(host.ports.log(), ["open /dev/ttyACM0"], "only the MCU");
    }

    #[test]
    fn a_restart_is_refused_while_another_program_holds_the_soc() {
        // Review W1: `bezel restart` must not pull the screen from under the
        // vendor app or another service; nothing reaches the MCU.
        let old = soc(SOC, &[1, 2]);
        let ports = ScriptedPorts {
            held_by: vec!["turing-smart-screen (pid 42)".into()],
            ..ScriptedPorts::new([mute()])
        };
        let host = fake_host(ScriptedBus::new([Ok(vec![mcu(), old.clone()])]), ports);
        let err = host.restart_rev_c(&mcu(), Some(&old)).unwrap_err();
        assert_eq!(
            err,
            BezelError::InUse {
                address: SOC.into(),
                holders: vec!["turing-smart-screen (pid 42)".into()],
            }
        );
        assert!(host.ports.log().is_empty(), "the MCU is not even opened");
        assert!(host.pause.taken().is_empty());
    }

    #[test]
    fn a_soc_that_does_not_come_back_after_its_restart_times_out() {
        // A display on another hub is someone else's screen.
        let elsewhere = soc("/dev/ttyACM5", &[2, 2]);
        let bus = ScriptedBus::new([Ok(vec![mcu(), elsewhere])]);
        let host = fake_host(bus, ScriptedPorts::new([mute()]));
        let err = host
            .restart_rev_c(&mcu(), Some(&soc(SOC, &[1, 2])))
            .unwrap_err();
        assert_eq!(
            err,
            BezelError::Timeout(
                "the screen after its restart: it did not come back within 30 s; \
                 unplug it and plug it back in"
                    .into()
            )
        );
        let polls = host
            .pause
            .taken()
            .into_iter()
            .filter(|d| *d == RESTART_POLL);
        assert_eq!(polls.count(), 60, "30 s of looks, 500 ms apart");
        assert_eq!(RESTART_POLLS, 60);

        // An MCU port that does not open fails before anything is sent.
        let host = fake_host(
            ScriptedBus::new([Ok(vec![mcu()])]),
            ScriptedPorts::default(),
        );
        let err = host.restart_rev_c(&mcu(), None).unwrap_err();
        assert!(matches!(err, BezelError::Transport(_)), "{err}");
        assert_eq!((mcu_sends(&host), host.pause.taken().len()), (0, 0));
    }

    #[test]
    fn a_soc_on_the_bus_without_hello_is_restarted_once_then_connected() {
        // The SoC hung: listed, silent, it never leaves on its own.
        let old = soc(SOC, &[1, 2]);
        let new = soc("/dev/ttyACM2", &[1, 2]);
        let mut answers = vec![Ok(vec![mcu(), old.clone()]); LEAVE_POLLS];
        answers.push(Ok(vec![mcu()]));
        answers.push(Ok(vec![mcu(), new]));
        let ports = ScriptedPorts::new([mute(), mute(), answering()]);
        let host = fake_host(ScriptedBus::new(answers), ports);
        let screen = rev_c(Some(old), Some(mcu()));
        let link = host
            .connect_rev_c(&screen, &screen.candidates)
            .expect("restarted and connected");
        assert_eq!(link.identity().firmware.as_deref(), Some(HELLO_88));
        assert_eq!(
            host.ports.log(),
            [
                "open /dev/ttyACM1",
                "open /dev/ttyACM0",
                "open /dev/ttyACM2"
            ]
        );
        assert_eq!(mcu_sends(&host), 1);
        let hold = host
            .pause
            .taken()
            .iter()
            .filter(|d| **d == RESTART_HOLD)
            .count();
        assert_eq!(hold, 1);
    }

    #[test]
    fn a_woken_soc_that_stopped_reading_is_restarted_once() {
        // Asleep; the wake brings a SoC whose HELLO stalls (hung firmware).
        let first = soc(SOC, &[1, 2]);
        let back = soc("/dev/ttyACM2", &[1, 2]);
        let bus = ScriptedBus::new([
            Ok(vec![mcu(), first]),
            Ok(vec![mcu()]),
            Ok(vec![mcu(), back]),
        ]);
        let host = fake_host(bus, ScriptedPorts::new([stalled(), mute(), answering()]));
        let screen = rev_c(None, Some(mcu()));
        assert!(host.connect_rev_c(&screen, &screen.candidates).is_ok());
        assert_eq!(
            host.ports.log(),
            [
                "poke /dev/ttyACM0",
                "open /dev/ttyACM1",
                "open /dev/ttyACM0",
                "open /dev/ttyACM2"
            ]
        );
        assert_eq!(mcu_sends(&host), 1);
    }

    #[test]
    fn a_soc_still_silent_after_its_restart_is_not_restarted_again() {
        let old = soc(SOC, &[1, 2]);
        let mut answers = vec![Ok(vec![mcu(), old.clone()]); LEAVE_POLLS];
        answers.push(Ok(vec![mcu()]));
        answers.push(Ok(vec![mcu(), soc("/dev/ttyACM2", &[1, 2])]));
        let ports = ScriptedPorts::new([mute(), mute(), mute()]);
        let host = fake_host(ScriptedBus::new(answers), ports);
        let screen = rev_c(Some(old), Some(mcu()));
        let err = host
            .connect_rev_c(&screen, &screen.candidates)
            .err()
            .unwrap();
        assert_eq!(
            err.to_string(),
            "timeout talking to the screen: it answered no HELLO even after a restart through \
             its MCU; unplug it and plug it back in"
        );
        assert_eq!(mcu_sends(&host), 1, "once per connect");
        assert_eq!(host.ports.log().len(), 3);
    }

    #[test]
    fn a_screen_that_answers_is_never_restarted() {
        let host = fake_host(
            ScriptedBus::new([Ok(vec![mcu(), soc(SOC, &[1, 2])])]),
            ScriptedPorts::new([answering()]),
        );
        let screen = rev_c(Some(soc(SOC, &[1, 2])), Some(mcu()));
        assert!(host.connect_rev_c(&screen, &screen.candidates).is_ok());
        assert_eq!(host.ports.log(), ["open /dev/ttyACM1"]);
        assert_eq!(mcu_sends(&host), 0);
        assert!(!host.pause.taken().contains(&RESTART_HOLD));
    }

    #[test]
    fn only_rev_c_screens_with_their_mcu_restart() {
        let weact = Screen {
            family: Family::WeAct,
            candidates: vec![model_by_id(ModelId("weact-fs-3.5")).unwrap()],
            display: Some(endpoint("/dev/bezel-no-such-port")),
            wake: None,
        };
        let err = SystemConnector.restart(&weact).unwrap_err();
        assert!(matches!(err, BezelError::Unsupported(_)), "{err}");
        let lone = rev_c(Some(soc("/dev/bezel-no-such-port", &[1, 2])), None);
        assert!(matches!(
            SystemConnector.restart(&lone),
            Err(BezelError::Unsupported(_))
        ));
        // The real MCU port of a screen that is not there does not open.
        let missing_mcu = Endpoint {
            address: DeviceAddress("/dev/bezel-no-such-mcu".into()),
            ..mcu()
        };
        let gone = rev_c(None, Some(missing_mcu));
        assert!(matches!(
            SystemConnector.restart(&gone),
            Err(BezelError::Transport(_))
        ));
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
    fn a_port_this_app_holds_fails_at_once_without_a_wake() {
        // D-2026-10-01-live-screen-controls-4: the studio holds the SoC's
        // port (live) and a reopen fails as busy. That is this app, not a
        // display shutting down: no wait for it to leave, no poke, no MCU,
        // no restart, no pause.
        let display = soc(SOC, &[1, 2]);
        let busy = BezelError::Transport(format!("{SOC}: Device or resource busy"));
        let ports = ScriptedPorts {
            held_here: Some("bezel-studio (PID 7)".into()),
            ..ScriptedPorts::new([Err(busy.clone()), answering()])
        };
        let bus = ScriptedBus::new([Ok(vec![mcu(), display.clone()])]);
        let host = fake_host(bus, ports);
        let screen = rev_c(Some(display.clone()), Some(mcu()));
        let err = host
            .connect_rev_c(&screen, &screen.candidates)
            .err()
            .unwrap();
        assert_eq!(
            err,
            BezelError::InUse {
                address: SOC.into(),
                holders: vec!["bezel-studio (PID 7)".into()],
            }
        );
        assert_eq!(entries(&host.ports.journal), ["open /dev/ttyACM1"]);
        assert_eq!(host.ports.log(), ["open /dev/ttyACM1"], "no poke");
        assert!(host.pause.taken().is_empty(), "no pause");

        // The same refusal of a port nobody here holds is still a display
        // shutting down: it is waited for and woken (device-protocols-3).
        let gone = ScriptedBus::new([Ok(vec![mcu()]), Ok(vec![mcu(), display.clone()])]);
        let host = fake_host(gone, ScriptedPorts::new([Err(busy), answering()]));
        assert!(host.connect_rev_c(&screen, &screen.candidates).is_ok());
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
    fn a_port_other_programs_hold_is_refused_before_it_is_opened() {
        // D-2026-09-30-device-protocols-3, decided by the one look `open`
        // takes: other programs win over this one, and the port is not
        // touched.
        let ports = ScriptedPorts {
            held_by: vec!["turing-smart-screen (pid 42)".into()],
            held_here: Some("bezel-studio (PID 7)".into()),
            ..ScriptedPorts::new([answering()])
        };
        let err = ports.open(&soc(SOC, &[1, 2]), Flow::None).err().unwrap();
        assert_eq!(
            err,
            BezelError::InUse {
                address: SOC.into(),
                holders: vec!["turing-smart-screen (pid 42)".into()],
            }
        );
        assert!(ports.log().is_empty(), "the port is not opened");
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

    /// A rev C connection through the host's own serial ports, from a bus
    /// that lists `display` and `mcu` behind one hub and pauses recorded
    /// instead of slept: what it ends with, and the pauses it took.
    #[cfg(unix)]
    fn through_the_host_ports(display: &str, mcu: &str) -> (Option<BezelError>, Vec<Duration>) {
        let mcu = at(mcu, UsbId::new(0x1a86, 0xca88), Some("CT88INCH"), &[1, 1]);
        let display = soc(display, &[1, 2]);
        let host = RevCHost {
            bus: ScriptedBus::new([Ok(vec![mcu.clone(), display.clone()])]),
            ports: SystemPorts,
            pause: Pauses::default(),
        };
        let screen = rev_c(Some(display), Some(mcu));
        let ended = host.connect_rev_c(&screen, &screen.candidates).err();
        (ended, host.pause.taken())
    }

    #[cfg(unix)]
    #[test]
    fn the_host_ports_name_this_process_for_a_port_it_holds() {
        // The host's ports look at the real `/proc`: a path this process
        // keeps open is held by this process and nobody else; closed, or
        // never opened, nobody holds it.
        let path = std::env::temp_dir().join(format!("bezel-ports-held-{}", std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let address = path.to_str().unwrap();
        let held = endpoint(address);
        let me = format!("(PID {})", std::process::id());
        let Holders { this, others } = SystemPorts.holders(&held);
        let this = this.expect("this process holds it");
        assert!(this.ends_with(&me), "{this} is not {me}");
        assert!(others.is_empty());
        // D-2026-10-01-live-screen-controls-4 through the host's `open`, the
        // entry of `open_serial` and `open_rev_c`: the open fails as busy (a
        // file is no tty, so serialport's exclusive lock refuses it) and is
        // `InUse` naming this process. The rev C connection stops there, with
        // no pause: no wait for the display to leave and no wake of its MCU
        // (a path that does not exist; each poke is followed by a pause).
        let in_use = BezelError::InUse {
            address: address.into(),
            holders: vec![this],
        };
        assert_eq!(
            SystemPorts.open(&held, Flow::None).err(),
            Some(in_use.clone())
        );
        let no_mcu =
            std::env::temp_dir().join(format!("bezel-ports-no-mcu-{}", std::process::id()));
        let (ended, pauses) = through_the_host_ports(address, no_mcu.to_str().unwrap());
        assert_eq!(ended, Some(in_use));
        assert!(pauses.is_empty(), "{pauses:?}");
        drop(file);
        // Closed, the same refusal is the system's own again.
        assert!(matches!(
            SystemPorts.open(&held, Flow::None).err(),
            Some(BezelError::Transport(_))
        ));
        assert_eq!(SystemPorts.holders(&held), Holders::default());
        std::fs::remove_file(&path).unwrap();
        let nobody = endpoint("/dev/bezel-no-such-port");
        assert_eq!(SystemPorts.holders(&nobody), Holders::default());
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
