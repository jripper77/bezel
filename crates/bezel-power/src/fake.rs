//! Test support (feature `fake`, Linux): a private `dbus-daemon`, a fake
//! logind on it and a monitor of that whole bus, so that the code that uses
//! [`Logind`](crate::Logind) runs against a real bus and real descriptors,
//! never the machine's system bus (D-2026-10-03-power-off-standby-6 (3)).
//!
//! [`PrivateBus::start`] fails, it never skips, when `dbus-daemon` is
//! missing (package `dbus`).

use std::io::{self, BufRead, BufReader, PipeReader, Read};
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dbus::Message;
use dbus::arg::{ArgType, Variant};
use dbus::channel::Channel;
use dbus::message::MessageType;
use dbus::strings::ErrorName;

use crate::logind::sys::{
    BUS, BUS_PATH, DELAY_MAX, LOGIND, LOGIND_PATH, MANAGER, PREPARE_FOR_SHUTDOWN, PROPERTIES, call,
    open,
};
use crate::{BusAddress, PowerError};

/// How long a fake's thread waits on its socket before looking at its
/// orders again.
const TICK: Duration = Duration::from_millis(10);

/// How long [`FakeLogind::prepare_for_shutdown`] waits for the fake to
/// send the signal.
const ORDER_TIMEOUT: Duration = Duration::from_secs(5);

/// `RequestName` flag: fail instead of waiting in the queue.
const DO_NOT_QUEUE: u32 = 4;

/// `RequestName` reply: the name is ours.
const PRIMARY_OWNER: u32 = 1;

/// A `dbus-daemon` of its own, on a socket in a temporary folder, with no
/// service to start; stopped, and its folder removed, on drop.
pub struct PrivateBus {
    daemon: Child,
    folder: PathBuf,
    address: BusAddress,
}

impl PrivateBus {
    /// Starts `dbus-daemon` and waits until it listens. An error, never a
    /// skip, when it cannot run (package `dbus`).
    pub fn start() -> io::Result<Self> {
        Self::start_program("dbus-daemon")
    }

    fn start_program(program: &str) -> io::Result<Self> {
        let folder = fresh_folder()?;
        let started = start_daemon(program, &folder);
        match started {
            Ok((daemon, address)) => Ok(Self {
                daemon,
                folder,
                address: BusAddress::new(address),
            }),
            Err(error) => {
                let _ = std::fs::remove_dir_all(&folder);
                Err(io::Error::new(
                    error.kind(),
                    format!("cannot start {program} (package `dbus`): {error}"),
                ))
            }
        }
    }

    /// Where the bus listens.
    pub fn address(&self) -> BusAddress {
        self.address.clone()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

/// A new, empty folder for one bus.
fn fresh_folder() -> io::Result<PathBuf> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "bezel-power-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if folder.exists() {
        // Left by an earlier process that had the same id.
        std::fs::remove_dir_all(&folder)?;
    }
    std::fs::create_dir(&folder)?;
    Ok(folder)
}

/// Starts `program` as a bus listening in `folder`; the daemon and the
/// address it printed once it listens. What it says goes to `daemon.log`
/// in `folder`, quoted in the error when it does not start.
fn start_daemon(program: &str, folder: &Path) -> io::Result<(Child, String)> {
    let config = folder.join("bus.conf");
    std::fs::write(&config, bus_config(&folder.join("bus")))?;
    let log = folder.join("daemon.log");
    let mut daemon = Command::new(program)
        .args(["--nofork", "--nopidfile", "--nosyslog", "--print-address=1"])
        .arg(format!("--config-file={}", config.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&log)?)
        .spawn()?;
    let mut address = String::new();
    let read = match daemon.stdout.take() {
        Some(out) => BufReader::new(out).read_line(&mut address),
        None => Ok(0),
    };
    let address = address.trim().to_owned();
    if matches!(read, Ok(n) if n > 0) && !address.is_empty() {
        return Ok((daemon, address));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    Err(read.err().unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("it printed no address: {}", said.trim()),
        )
    }))
}

/// The configuration of a private bus on the socket `socket`: anyone may
/// own any name and talk to anyone, and no service folder, so a call to a
/// name nobody owns fails (`ServiceUnknown`) instead of starting anything.
fn bus_config(socket: &Path) -> String {
    format!(
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
        socket.display()
    )
}

/// A value shared with a fake's threads, with a signal on each change.
struct Watched<T> {
    value: Mutex<T>,
    changed: Condvar,
}

impl<T: Default> Default for Watched<T> {
    fn default() -> Self {
        Self {
            value: Mutex::new(T::default()),
            changed: Condvar::new(),
        }
    }
}

impl<T> Watched<T> {
    fn lock(&self) -> MutexGuard<'_, T> {
        self.value.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update(&self, change: impl FnOnce(&mut T)) {
        change(&mut self.lock());
        self.changed.notify_all();
    }

    /// Whether `done` holds within `timeout`.
    fn wait_until(&self, timeout: Duration, done: impl Fn(&T) -> bool) -> bool {
        let (value, _) = self
            .changed
            .wait_timeout_while(self.lock(), timeout, |value| !done(value))
            .unwrap_or_else(PoisonError::into_inner);
        done(&value)
    }
}

/// One method call the fake logind received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogindCall {
    /// `Manager.Inhibit(what, who, why, mode)`, answered with the write end
    /// of a new pipe.
    Inhibit {
        /// The caller's unique name.
        sender: String,
        /// `what` (`shutdown`, `sleep`...).
        what: String,
        /// `who`.
        who: String,
        /// `why`.
        why: String,
        /// `mode` (`delay` or `block`).
        mode: String,
    },
    /// `Properties.Get(interface, property)`.
    Get {
        /// The caller's unique name.
        sender: String,
        /// The interface asked.
        interface: String,
        /// The property asked.
        property: String,
    },
    /// Any other call, answered with an error.
    Other {
        /// The caller's unique name.
        sender: String,
        /// The interface called.
        interface: String,
        /// The method called.
        member: String,
    },
}

/// What the fake logind received and which of its inhibitors were closed.
#[derive(Default)]
struct Record {
    calls: Vec<LogindCall>,
    /// One per `Inhibit`, in order: whether its descriptor reached EOF.
    closed: Vec<bool>,
}

/// An order to the fake's thread.
enum Order {
    Announce(bool, mpsc::Sender<Result<(), PowerError>>),
    Stop,
}

/// A logind on a [`PrivateBus`]: it owns `org.freedesktop.login1`, answers
/// `Inhibit` with the write end of a real pipe (whose read end it watches
/// for EOF) and `InhibitDelayMaxUSec`, records every call and broadcasts
/// `PrepareForShutdown` when told.
pub struct FakeLogind {
    record: Arc<Watched<Record>>,
    orders: mpsc::Sender<Order>,
    server: Option<JoinHandle<()>>,
    name: String,
}

impl FakeLogind {
    /// Takes logind's name on the bus at `address`; `delay_max` is its
    /// `InhibitDelayMaxUSec`.
    pub fn start(address: &BusAddress, delay_max: Duration) -> Result<Self, PowerError> {
        Self::start_slow(address, delay_max, Duration::ZERO)
    }

    /// [`FakeLogind::start`], answering the read of `InhibitDelayMaxUSec`
    /// only `slowness` after it is asked: a logind busy at shutdown.
    pub fn start_slow(
        address: &BusAddress,
        delay_max: Duration,
        slowness: Duration,
    ) -> Result<Self, PowerError> {
        let channel = open(address)?;
        let name = channel.unique_name().unwrap_or_default().to_owned();
        let reply = call(&channel, BUS, BUS_PATH, BUS, "RequestName", |m| {
            m.append2(LOGIND, DO_NOT_QUEUE)
        })?;
        if !matches!(reply.read1::<u32>(), Ok(PRIMARY_OWNER)) {
            return Err(PowerError::Call {
                call: "RequestName",
                reason: format!("{LOGIND} is already owned"),
            });
        }
        let record = Arc::new(Watched::default());
        let (orders, inbox) = mpsc::channel();
        let server = Server {
            channel,
            record: Arc::clone(&record),
            delay_max,
            slowness,
        };
        let server = thread::Builder::new()
            .name("fake-logind".into())
            .spawn(move || server.run(&inbox))
            .map_err(|error| PowerError::Call {
                call: "RequestName",
                reason: error.to_string(),
            })?;
        Ok(Self {
            record,
            orders,
            server: Some(server),
            name,
        })
    }

    /// The fake's unique name on the bus (`:1.N`).
    pub fn unique_name(&self) -> &str {
        &self.name
    }

    /// Every call received, in order.
    pub fn calls(&self) -> Vec<LogindCall> {
        self.record.lock().calls.clone()
    }

    /// How many inhibitors were handed out and are still open.
    pub fn held(&self) -> usize {
        self.record
            .lock()
            .closed
            .iter()
            .filter(|closed| !**closed)
            .count()
    }

    /// Whether at least `count` inhibitors were handed out within
    /// `timeout`.
    pub fn wait_for_inhibitors(&self, count: usize, timeout: Duration) -> bool {
        self.record
            .wait_until(timeout, |record| record.closed.len() >= count)
    }

    /// Whether the descriptor of inhibitor `index` (0 = the first handed
    /// out) was closed by everyone who held it within `timeout`: its pipe
    /// reached EOF.
    pub fn wait_for_release(&self, index: usize, timeout: Duration) -> bool {
        self.record.wait_until(timeout, |record| {
            record.closed.get(index).copied().unwrap_or(false)
        })
    }

    /// Broadcasts `PrepareForShutdown(starting)`, as logind does when a
    /// shutdown starts (`true`) or is cancelled (`false`).
    pub fn prepare_for_shutdown(&self, starting: bool) -> Result<(), PowerError> {
        let (done, outcome) = mpsc::channel();
        self.orders
            .send(Order::Announce(starting, done))
            .map_err(|_| PowerError::Disconnected)?;
        outcome
            .recv_timeout(ORDER_TIMEOUT)
            .unwrap_or(Err(PowerError::Disconnected))
    }
}

impl Drop for FakeLogind {
    fn drop(&mut self) {
        let _ = self.orders.send(Order::Stop);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// The fake logind's thread: its connection and what it records.
struct Server {
    channel: Channel,
    record: Arc<Watched<Record>>,
    delay_max: Duration,
    /// How long the read of the delay waits before it is answered.
    slowness: Duration,
}

/// A D-Bus error to answer with.
type Refusal = (&'static str, &'static std::ffi::CStr);

const UNKNOWN_METHOD: Refusal = (
    "org.freedesktop.DBus.Error.UnknownMethod",
    c"the fake logind knows Inhibit and InhibitDelayMaxUSec only",
);
const UNKNOWN_PROPERTY: Refusal = (
    "org.freedesktop.DBus.Error.UnknownProperty",
    c"the fake logind has InhibitDelayMaxUSec only",
);
const INVALID_ARGS: Refusal = (
    "org.freedesktop.DBus.Error.InvalidArgs",
    c"unexpected arguments",
);
const FAILED: Refusal = ("org.freedesktop.DBus.Error.Failed", c"no pipe");

impl Server {
    fn run(self, inbox: &mpsc::Receiver<Order>) {
        loop {
            match inbox.try_recv() {
                Ok(Order::Announce(starting, done)) => {
                    let _ = done.send(self.announce(starting));
                }
                Ok(Order::Stop) | Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if self.channel.read_write(Some(TICK)).is_err() {
                return;
            }
            while let Some(message) = self.channel.pop_message() {
                self.answer(&message);
            }
        }
    }

    fn announce(&self, starting: bool) -> Result<(), PowerError> {
        let signal = Message::new_signal(LOGIND_PATH, MANAGER, PREPARE_FOR_SHUTDOWN)
            .map_err(|reason| PowerError::Call {
                call: PREPARE_FOR_SHUTDOWN,
                reason,
            })?
            .append1(starting);
        self.channel
            .send(signal)
            .map_err(|()| PowerError::Disconnected)?;
        self.channel.flush();
        Ok(())
    }

    fn answer(&self, message: &Message) {
        if message.msg_type() != MessageType::MethodCall {
            return;
        }
        let sender = text(message.sender().as_deref());
        let interface = text(message.interface().as_deref());
        let member = text(message.member().as_deref());
        let at_manager = message.path().as_deref() == Some(LOGIND_PATH);
        let reply = match (interface.as_str(), member.as_str()) {
            (MANAGER, "Inhibit") if at_manager => self.inhibit(message, sender),
            (PROPERTIES, "Get") if at_manager => self.get(message, sender),
            _ => {
                self.record.update(|record| {
                    record.calls.push(LogindCall::Other {
                        sender,
                        interface,
                        member,
                    });
                });
                Err(UNKNOWN_METHOD)
            }
        };
        let reply = match reply {
            Ok(reply) => Some(reply),
            Err((name, why)) => ErrorName::new(name)
                .ok()
                .map(|name| message.error(&name, why)),
        };
        if let Some(reply) = reply {
            let _ = self.channel.send(reply);
            self.channel.flush();
        }
    }

    fn inhibit(&self, message: &Message, sender: String) -> Result<Message, Refusal> {
        let (what, who, why, mode) = message
            .read4::<&str, &str, &str, &str>()
            .map_err(|_| INVALID_ARGS)?;
        let (reader, writer) = io::pipe().map_err(|_| FAILED)?;
        let mut index = 0;
        self.record.update(|record| {
            record.calls.push(LogindCall::Inhibit {
                sender,
                what: what.to_owned(),
                who: who.to_owned(),
                why: why.to_owned(),
                mode: mode.to_owned(),
            });
            index = record.closed.len();
            record.closed.push(false);
        });
        let record = Arc::clone(&self.record);
        thread::Builder::new()
            .name("fake-logind-inhibitor".into())
            .spawn(move || watch_until_closed(reader, &record, index))
            .map_err(|_| FAILED)?;
        // libdbus sends a duplicate; ours closes when the argument drops.
        Ok(message.method_return().append1(OwnedFd::from(writer)))
    }

    fn get(&self, message: &Message, sender: String) -> Result<Message, Refusal> {
        let (interface, property) = message.read2::<&str, &str>().map_err(|_| INVALID_ARGS)?;
        let asked_delay = interface == MANAGER && property == DELAY_MAX;
        self.record.update(|record| {
            record.calls.push(LogindCall::Get {
                sender,
                interface: interface.to_owned(),
                property: property.to_owned(),
            });
        });
        if !asked_delay {
            return Err(UNKNOWN_PROPERTY);
        }
        thread::sleep(self.slowness);
        let micros = u64::try_from(self.delay_max.as_micros()).unwrap_or(u64::MAX);
        Ok(message.method_return().append1(Variant(micros)))
    }
}

/// Reads the inhibitor's pipe until EOF (every copy of its write end is
/// closed), then records it closed.
fn watch_until_closed(mut reader: PipeReader, record: &Watched<Record>, index: usize) {
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => break,
            Err(error) if error.kind() != io::ErrorKind::Interrupted => break,
            _ => {}
        }
    }
    record.update(|record| {
        if let Some(closed) = record.closed.get_mut(index) {
            *closed = true;
        }
    });
}

/// A header field as text; empty when absent.
fn text(field: Option<&str>) -> String {
    field.unwrap_or_default().to_owned()
}

/// The kind of a message on the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// A method call.
    MethodCall,
    /// A method's reply.
    MethodReturn,
    /// An error reply.
    Error,
    /// A signal.
    Signal,
}

/// A message a [`BusMonitor`] saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusMessage {
    /// Call, reply, error or signal.
    pub kind: MessageKind,
    /// The sender's unique name (the bus's own messages:
    /// `org.freedesktop.DBus`).
    pub sender: Option<String>,
    /// The destination as the sender wrote it; `None` for a broadcast.
    pub destination: Option<String>,
    /// The object path.
    pub path: Option<String>,
    /// The interface.
    pub interface: Option<String>,
    /// The method or signal.
    pub member: Option<String>,
    /// Strings, booleans and `u32`s as text; any other argument
    /// as its type in angle brackets (`<UnixFd>`, `<Variant>`...).
    pub args: Vec<String>,
}

/// A monitor of a whole bus (`BecomeMonitor`): every message any connection
/// sends on it, in order, from the moment it starts.
pub struct BusMonitor {
    seen: Arc<Watched<Vec<BusMessage>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl BusMonitor {
    /// Starts monitoring the bus at `address`.
    pub fn start(address: &BusAddress) -> Result<Self, PowerError> {
        let channel = open(address)?;
        let own = channel.unique_name().map(str::to_owned);
        call(
            &channel,
            BUS,
            BUS_PATH,
            "org.freedesktop.DBus.Monitoring",
            "BecomeMonitor",
            |m| m.append2(Vec::<&str>::new(), 0u32),
        )?;
        let seen = Arc::new(Watched::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (into, stopped) = (Arc::clone(&seen), Arc::clone(&stop));
        let thread = thread::Builder::new()
            .name("bus-monitor".into())
            .spawn(move || monitor(&channel, own.as_deref(), &into, &stopped))
            .map_err(|error| PowerError::Call {
                call: "BecomeMonitor",
                reason: error.to_string(),
            })?;
        Ok(Self {
            seen,
            stop,
            thread: Some(thread),
        })
    }

    /// Every message seen so far.
    pub fn messages(&self) -> Vec<BusMessage> {
        self.seen.lock().clone()
    }

    /// Whether `done` holds for the messages seen within `timeout`.
    pub fn wait_for(&self, timeout: Duration, done: impl Fn(&[BusMessage]) -> bool) -> bool {
        self.seen.wait_until(timeout, |seen| done(seen))
    }
}

impl Drop for BusMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Records what the monitor connection receives, except what the bus
/// sends to the monitor itself, until stopped or disconnected.
fn monitor(
    channel: &Channel,
    own: Option<&str>,
    seen: &Watched<Vec<BusMessage>>,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Relaxed) {
        if channel.read_write(Some(TICK)).is_err() {
            return;
        }
        while let Some(message) = channel.pop_message() {
            if own.is_some() && message.destination().as_deref() == own {
                continue;
            }
            let seen_message = observed(&message);
            seen.update(|seen| seen.push(seen_message));
        }
    }
}

/// `message` as a [`BusMessage`].
fn observed(message: &Message) -> BusMessage {
    let kind = match message.msg_type() {
        MessageType::MethodCall => MessageKind::MethodCall,
        MessageType::MethodReturn => MessageKind::MethodReturn,
        MessageType::Error => MessageKind::Error,
        MessageType::Signal => MessageKind::Signal,
    };
    let field = |value: Option<&str>| value.map(str::to_owned);
    BusMessage {
        kind,
        sender: field(message.sender().as_deref()),
        destination: field(message.destination().as_deref()),
        path: field(message.path().as_deref()),
        interface: field(message.interface().as_deref()),
        member: field(message.member().as_deref()),
        args: arguments(message),
    }
}

/// The arguments of `message`, as [`BusMessage::args`] writes them. A
/// descriptor is never read (reading one would duplicate it).
fn arguments(message: &Message) -> Vec<String> {
    let mut iter = message.iter_init();
    let mut args = Vec::new();
    loop {
        let arg = match iter.arg_type() {
            ArgType::Invalid => return args,
            ArgType::String => iter.get::<&str>().map(str::to_owned),
            ArgType::Boolean => iter.get::<bool>().map(|b| b.to_string()),
            ArgType::UInt32 => iter.get::<u32>().map(|n| n.to_string()),
            other => Some(format!("<{other:?}>")),
        };
        args.push(arg.unwrap_or_default());
        if !iter.next() {
            return args;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{BusMonitor, FakeLogind, LogindCall, MessageKind, PrivateBus};
    use crate::PowerError;
    use crate::logind::sys::{BUS, BUS_PATH, LOGIND, LOGIND_PATH, MANAGER, PROPERTIES, call, open};

    const PATIENCE: Duration = Duration::from_secs(5);

    #[test]
    fn a_missing_daemon_is_an_error_not_a_skip() {
        let error = PrivateBus::start_program("bezel-no-such-dbus-daemon")
            .err()
            .expect("no such program");
        assert!(error.to_string().contains("package `dbus`"), "{error}");
    }

    #[test]
    fn a_stopped_bus_takes_its_folder_and_its_address_with_it() {
        let bus = PrivateBus::start().unwrap();
        let folder = bus.folder.clone();
        let address = bus.address();
        assert!(folder.join("bus").exists());
        drop(bus);
        assert!(!folder.exists());
        assert!(matches!(open(&address), Err(PowerError::NoBus { .. })));
    }

    #[test]
    fn the_fake_refuses_what_logind_would_not_be_asked() {
        let bus = PrivateBus::start().unwrap();
        let logind = FakeLogind::start(&bus.address(), PATIENCE).unwrap();
        let monitor = BusMonitor::start(&bus.address()).unwrap();
        let caller = open(&bus.address()).unwrap();
        let power_off = call(&caller, LOGIND, LOGIND_PATH, MANAGER, "PowerOff", |m| {
            m.append1(false)
        });
        assert!(matches!(
            power_off,
            Err(PowerError::Call {
                call: "PowerOff",
                ..
            })
        ));
        let other = call(&caller, LOGIND, LOGIND_PATH, PROPERTIES, "Get", |m| {
            m.append2(MANAGER, "Docked")
        });
        assert!(matches!(other, Err(PowerError::Call { call: "Get", .. })));
        let garbled = call(&caller, LOGIND, LOGIND_PATH, MANAGER, "Inhibit", |m| {
            m.append1(7u32)
        });
        assert!(matches!(
            garbled,
            Err(PowerError::Call {
                call: "Inhibit",
                ..
            })
        ));
        let me = caller.unique_name().unwrap().to_owned();
        assert_eq!(
            logind.calls(),
            [
                LogindCall::Other {
                    sender: me.clone(),
                    interface: MANAGER.into(),
                    member: "PowerOff".into(),
                },
                LogindCall::Get {
                    sender: me,
                    interface: MANAGER.into(),
                    property: "Docked".into(),
                },
            ]
        );
        assert_eq!(logind.held(), 0);
        assert!(!logind.wait_for_inhibitors(1, Duration::from_millis(50)));
        assert!(monitor.wait_for(PATIENCE, |seen| {
            seen.iter().filter(|m| m.kind == MessageKind::Error).count() == 3
        }));
        let power_off = monitor
            .messages()
            .into_iter()
            .find(|m| m.member.as_deref() == Some("PowerOff"))
            .unwrap();
        assert_eq!(power_off.args, ["false"]);
    }

    #[test]
    fn a_second_fake_cannot_take_the_name() {
        let bus = PrivateBus::start().unwrap();
        let _first = FakeLogind::start(&bus.address(), PATIENCE).unwrap();
        let second = FakeLogind::start(&bus.address(), PATIENCE).err();
        assert!(matches!(
            second,
            Some(PowerError::Call {
                call: "RequestName",
                ..
            })
        ));
    }

    #[test]
    fn announcing_on_a_lost_bus_is_an_error() {
        let bus = PrivateBus::start().unwrap();
        let logind = FakeLogind::start(&bus.address(), PATIENCE).unwrap();
        drop(bus);
        let mut announced = Ok(());
        for _ in 0..50 {
            announced = logind.prepare_for_shutdown(true);
            if announced.is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(announced, Err(PowerError::Disconnected));
    }

    #[test]
    fn the_monitor_reports_arguments_by_type() {
        let bus = PrivateBus::start().unwrap();
        let monitor = BusMonitor::start(&bus.address()).unwrap();
        let caller = open(&bus.address()).unwrap();
        let _ = call(&caller, BUS, BUS_PATH, BUS, "RequestName", |m| {
            m.append2("org.example.Bezel", 4u32)
        });
        let me = caller.unique_name().unwrap().to_owned();
        // The reply to RequestName: PRIMARY_OWNER (1).
        assert!(monitor.wait_for(PATIENCE, |seen| {
            seen.iter().any(|m| {
                m.kind == MessageKind::MethodReturn
                    && m.destination.as_deref() == Some(&me)
                    && m.args == ["1"]
            })
        }));
        let seen = monitor.messages();
        let request = seen
            .iter()
            .find(|m| m.member.as_deref() == Some("RequestName"))
            .unwrap();
        assert_eq!(request.kind, MessageKind::MethodCall);
        assert_eq!(request.args, ["org.example.Bezel", "4"]);
    }
}
