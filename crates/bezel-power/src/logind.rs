//! logind's shutdown delay lock and `PrepareForShutdown` (Linux), and the
//! address of the bus that carries them.

use std::fmt;
use std::time::Duration;

use crate::PowerError;

/// The system bus when nothing moves it: systemd's socket, the one
/// `/var/run/dbus/system_bus_socket` links to.
const SYSTEM_BUS: &str = "unix:path=/run/dbus/system_bus_socket";

/// The variable that moves the system bus (D-Bus specification).
const SYSTEM_BUS_VARIABLE: &str = "DBUS_SYSTEM_BUS_ADDRESS";

/// The D-Bus address [`Logind::connect`] connects to, and the only one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusAddress(String);

impl BusAddress {
    /// The system bus, where logind is: `$DBUS_SYSTEM_BUS_ADDRESS` when it
    /// is set (as libdbus and sd-bus read it), else
    /// `unix:path=/run/dbus/system_bus_socket`.
    pub fn system() -> Self {
        Self::system_from(std::env::var(SYSTEM_BUS_VARIABLE).ok())
    }

    /// The system bus, given the value of `$DBUS_SYSTEM_BUS_ADDRESS`.
    fn system_from(variable: Option<String>) -> Self {
        Self(
            variable
                .filter(|address| !address.is_empty())
                .unwrap_or_else(|| SYSTEM_BUS.to_owned()),
        )
    }

    /// Any D-Bus address, such as a test's private bus.
    pub fn new(address: impl Into<String>) -> Self {
        Self(address.into())
    }

    /// The address as D-Bus writes it (`unix:path=...`).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BusAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What logind's `PrepareForShutdown` announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shutdown {
    /// `true`: the machine is shutting down or restarting. logind waits for
    /// the delay locks, [`Logind::delay_max`] at most.
    Starting,
    /// `false`: a shutdown that had started was cancelled. A lock released
    /// for it must be taken again.
    Cancelled,
}

/// A *delay* inhibitor lock held on logind: a shutdown waits for it, up to
/// [`Logind::delay_max`]. Dropping it, or [`Inhibitor::release`], closes
/// its descriptor, which releases the lock.
pub struct Inhibitor(#[allow(dead_code, reason = "held only to be released on drop")] sys::Lock);

impl Inhibitor {
    /// Releases the lock now: the shutdown goes on.
    pub fn release(self) {
        // Consuming `self` drops the descriptor, which closes it.
    }
}

impl fmt::Debug for Inhibitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Inhibitor")
    }
}

/// A connection to logind on one bus. Besides `Hello` and the `AddMatch` of
/// `PrepareForShutdown`, both to the bus itself when connecting, every
/// message it sends goes to `org.freedesktop.login1`.
pub struct Logind(sys::Connection);

impl Logind {
    /// Connects to the bus at `address`, and to no other, and asks it for
    /// logind's `PrepareForShutdown` broadcasts. Connecting does not need
    /// logind: [`PowerError::NoBus`] when nothing answers at the address,
    /// [`PowerError::Unsupported`] off Linux.
    pub fn connect(address: &BusAddress) -> Result<Self, PowerError> {
        sys::Connection::open(address).map(Self)
    }

    /// Takes a delay lock for `shutdown`:
    /// `Inhibit("shutdown", "Bezel", reason, "delay")`, which logind shows
    /// in `systemd-inhibit --list`. [`PowerError::NoLogind`] when logind is
    /// not on the bus.
    pub fn inhibit(&self, reason: &str) -> Result<Inhibitor, PowerError> {
        self.0.inhibit(reason).map(Inhibitor)
    }

    /// How long logind waits for a delay lock once a shutdown starts: its
    /// `InhibitDelayMaxUSec` (5 s unless configured).
    pub fn delay_max(&self) -> Result<Duration, PowerError> {
        self.0.delay_max()
    }

    /// Waits up to `timeout` for logind's next `PrepareForShutdown`;
    /// `None` when none came. Only logind's broadcast counts: the bus lets
    /// through only the one sent by the owner of `org.freedesktop.login1`,
    /// and a signal addressed to this connection alone is ignored.
    /// [`PowerError::Disconnected`] when the bus goes away.
    pub fn wait(&self, timeout: Duration) -> Result<Option<Shutdown>, PowerError> {
        self.0.wait(timeout)
    }
}

/// The D-Bus side of [`Logind`], through libdbus.
#[cfg(target_os = "linux")]
pub(crate) mod sys {
    use std::os::fd::OwnedFd;
    use std::time::{Duration, Instant};

    use dbus::Message;
    use dbus::arg::Variant;
    use dbus::channel::Channel;
    use dbus::message::MessageType;

    use super::{BusAddress, Shutdown};
    use crate::PowerError;

    /// logind's name on the bus.
    pub(crate) const LOGIND: &str = "org.freedesktop.login1";
    /// logind's manager object.
    pub(crate) const LOGIND_PATH: &str = "/org/freedesktop/login1";
    /// The interface of `Inhibit`, `PrepareForShutdown` and the delay.
    pub(crate) const MANAGER: &str = "org.freedesktop.login1.Manager";
    /// The standard interface that reads a property.
    pub(crate) const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
    /// The signal logind sends before (`true`) and after a cancelled
    /// (`false`) shutdown.
    pub(crate) const PREPARE_FOR_SHUTDOWN: &str = "PrepareForShutdown";
    /// The property that holds the longest delay, in microseconds.
    pub(crate) const DELAY_MAX: &str = "InhibitDelayMaxUSec";
    /// The bus itself: its name, object and interface.
    pub(crate) const BUS: &str = "org.freedesktop.DBus";
    pub(crate) const BUS_PATH: &str = "/org/freedesktop/DBus";
    /// The rule given to the bus: logind's broadcast of
    /// `PrepareForShutdown`, from whoever owns logind's name, and nothing
    /// else.
    pub(crate) const RULE: &str = "type='signal',sender='org.freedesktop.login1',\
         path='/org/freedesktop/login1',interface='org.freedesktop.login1.Manager',\
         member='PrepareForShutdown'";
    /// How long a call waits for its reply.
    const CALL_TIMEOUT: Duration = Duration::from_secs(2);
    /// The longest single poll of the socket (libdbus takes milliseconds
    /// as a C `int`).
    const LONGEST_POLL: Duration = Duration::from_secs(60);

    /// An open connection, registered on the bus.
    pub(crate) struct Connection(Channel);

    /// The descriptor of a delay lock.
    pub(crate) struct Lock(#[allow(dead_code, reason = "held only to be closed on drop")] OwnedFd);

    impl Connection {
        pub(crate) fn open(address: &BusAddress) -> Result<Self, PowerError> {
            let connection = Self(open(address)?);
            call(&connection.0, BUS, BUS_PATH, BUS, "AddMatch", |m| {
                m.append1(RULE)
            })?;
            Ok(connection)
        }

        pub(crate) fn inhibit(&self, reason: &str) -> Result<Lock, PowerError> {
            let reply = call(&self.0, LOGIND, LOGIND_PATH, MANAGER, "Inhibit", |m| {
                m.append3("shutdown", "Bezel", reason).append1("delay")
            })?;
            reply
                .read1::<OwnedFd>()
                .map(Lock)
                .map_err(|e| PowerError::Call {
                    call: "Inhibit",
                    reason: e.to_string(),
                })
        }

        pub(crate) fn delay_max(&self) -> Result<Duration, PowerError> {
            let reply = call(&self.0, LOGIND, LOGIND_PATH, PROPERTIES, "Get", |m| {
                m.append2(MANAGER, DELAY_MAX)
            })?;
            let micros = reply
                .read1::<Variant<u64>>()
                .map_err(|e| PowerError::Call {
                    call: "Get",
                    reason: e.to_string(),
                })?;
            Ok(Duration::from_micros(micros.0))
        }

        pub(crate) fn wait(&self, timeout: Duration) -> Result<Option<Shutdown>, PowerError> {
            let deadline = Instant::now().checked_add(timeout);
            loop {
                if let Some(news) = self.next_news()? {
                    return Ok(Some(news));
                }
                let left = deadline.map_or(LONGEST_POLL, |deadline| {
                    deadline.saturating_duration_since(Instant::now())
                });
                self.0
                    .read_write(Some(left.min(LONGEST_POLL)))
                    .map_err(|()| PowerError::Disconnected)?;
                if left.is_zero() {
                    return self.next_news();
                }
            }
        }

        /// The first announcement among the messages already read.
        fn next_news(&self) -> Result<Option<Shutdown>, PowerError> {
            while let Some(message) = self.0.pop_message() {
                if let Some(news) = news_in(&message)? {
                    return Ok(Some(news));
                }
            }
            Ok(None)
        }
    }

    /// What `message` announces: logind's broadcast `PrepareForShutdown`,
    /// or the loss of the bus (libdbus's local `Disconnected`).
    fn news_in(message: &Message) -> Result<Option<Shutdown>, PowerError> {
        if message.msg_type() != MessageType::Signal {
            return Ok(None);
        }
        let interface = message.interface();
        let member = message.member();
        if interface.as_deref() == Some("org.freedesktop.DBus.Local")
            && member.as_deref() == Some("Disconnected")
        {
            return Err(PowerError::Disconnected);
        }
        let from_logind = message.destination().is_none()
            && message.path().as_deref() == Some(LOGIND_PATH)
            && interface.as_deref() == Some(MANAGER)
            && member.as_deref() == Some(PREPARE_FOR_SHUTDOWN);
        if !from_logind {
            return Ok(None);
        }
        Ok(message.read1::<bool>().ok().map(announced))
    }

    /// The announcement of `PrepareForShutdown(starting)`.
    pub(crate) fn announced(starting: bool) -> Shutdown {
        if starting {
            Shutdown::Starting
        } else {
            Shutdown::Cancelled
        }
    }

    /// A private connection to the bus at `address`, registered (`Hello`).
    pub(crate) fn open(address: &BusAddress) -> Result<Channel, PowerError> {
        let no_bus = |error: dbus::Error| PowerError::NoBus {
            address: address.to_string(),
            reason: describe(&error),
        };
        let mut channel = Channel::open_private(address.as_str()).map_err(no_bus)?;
        channel.register().map_err(no_bus)?;
        Ok(channel)
    }

    /// Calls `member` and waits for its reply; `args` writes the arguments.
    pub(crate) fn call(
        channel: &Channel,
        destination: &str,
        path: &str,
        interface: &str,
        member: &'static str,
        args: impl FnOnce(Message) -> Message,
    ) -> Result<Message, PowerError> {
        let message =
            Message::new_method_call(destination, path, interface, member).map_err(|reason| {
                PowerError::Call {
                    call: member,
                    reason,
                }
            })?;
        channel
            .send_with_reply_and_block(args(message), CALL_TIMEOUT)
            .map_err(|error| call_error(member, &error))
    }

    /// The typed error of a failed call.
    pub(crate) fn call_error(call: &'static str, error: &dbus::Error) -> PowerError {
        match error.name() {
            Some(
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner",
            ) => PowerError::NoLogind,
            Some("org.freedesktop.DBus.Error.Disconnected") => PowerError::Disconnected,
            _ => PowerError::Call {
                call,
                reason: describe(error),
            },
        }
    }

    /// An error's name and message.
    fn describe(error: &dbus::Error) -> String {
        format!(
            "{}: {}",
            error.name().unwrap_or("unnamed error"),
            error.message().unwrap_or_default()
        )
    }
}

/// No logind off Linux: nothing connects, so no lock or connection exists.
#[cfg(not(target_os = "linux"))]
mod sys {
    use std::convert::Infallible;
    use std::time::Duration;

    use super::{BusAddress, Shutdown};
    use crate::PowerError;

    pub(crate) struct Connection(Infallible);

    pub(crate) struct Lock(#[allow(dead_code, reason = "never built")] Infallible);

    impl Connection {
        pub(crate) fn open(_address: &BusAddress) -> Result<Self, PowerError> {
            Err(PowerError::Unsupported)
        }

        pub(crate) fn inhibit(&self, _reason: &str) -> Result<Lock, PowerError> {
            match self.0 {}
        }

        pub(crate) fn delay_max(&self) -> Result<Duration, PowerError> {
            match self.0 {}
        }

        pub(crate) fn wait(&self, _timeout: Duration) -> Result<Option<Shutdown>, PowerError> {
            match self.0 {}
        }
    }
}

#[cfg(test)]
mod address_tests {
    use super::BusAddress;

    #[test]
    fn the_system_bus_is_systemds_socket_unless_moved() {
        assert_eq!(
            BusAddress::system_from(None).as_str(),
            "unix:path=/run/dbus/system_bus_socket"
        );
        assert_eq!(
            BusAddress::system_from(Some(String::new())),
            BusAddress::system_from(None)
        );
        let moved = BusAddress::system_from(Some("unix:path=/x/bus".into()));
        assert_eq!(moved, BusAddress::new("unix:path=/x/bus"));
        assert_eq!(moved.to_string(), "unix:path=/x/bus");
        assert_eq!(
            BusAddress::system(),
            BusAddress::system_from(std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok())
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::time::Duration;

    use dbus::Message;

    use crate::fake::{BusMessage, BusMonitor, FakeLogind, LogindCall, MessageKind, PrivateBus};
    use crate::logind::sys::{
        BUS, LOGIND, LOGIND_PATH, MANAGER, PREPARE_FOR_SHUTDOWN, PROPERTIES, RULE, announced,
        call_error, open,
    };
    use crate::{BusAddress, Logind, PowerError, Shutdown};

    const PATIENCE: Duration = Duration::from_secs(5);
    const DELAY_MAX: Duration = Duration::from_millis(1_500);
    const REASON: &str = "Bezel applies each screen's choice for when the computer shuts down";

    /// A private bus; a failure, never a skip, without `dbus-daemon`.
    #[allow(clippy::expect_used, reason = "the helper of a failing test panics")]
    fn bus() -> PrivateBus {
        PrivateBus::start()
            .expect("dbus-daemon (package `dbus`) runs these tests; they fail without it")
    }

    /// (destination, interface, member, arguments) of `message`.
    fn call_of(message: &BusMessage) -> (String, String, String, Vec<String>) {
        let text = |field: &Option<String>| field.clone().unwrap_or_default();
        (
            text(&message.destination),
            text(&message.interface),
            text(&message.member),
            message.args.clone(),
        )
    }

    fn owned(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn speaks_only_to_logind_on_its_bus() {
        let bus = bus();
        let logind = FakeLogind::start(&bus.address(), DELAY_MAX).unwrap();
        let monitor = BusMonitor::start(&bus.address()).unwrap();

        let client = Logind::connect(&bus.address()).unwrap();
        let inhibitor = client.inhibit(REASON).unwrap();
        assert_eq!(client.delay_max().unwrap(), DELAY_MAX);
        logind.prepare_for_shutdown(true).unwrap();
        assert_eq!(client.wait(PATIENCE).unwrap(), Some(Shutdown::Starting));
        logind.prepare_for_shutdown(false).unwrap();
        assert_eq!(client.wait(PATIENCE).unwrap(), Some(Shutdown::Cancelled));
        inhibitor.release();
        assert!(logind.wait_for_release(0, PATIENCE));
        drop(client);

        let Some(LogindCall::Inhibit { sender: me, .. }) = logind.calls().first().cloned() else {
            panic!("no Inhibit: {:?}", logind.calls());
        };
        assert!(monitor.wait_for(PATIENCE, |seen| {
            seen.iter()
                .filter(|m| m.member.as_deref() == Some(PREPARE_FOR_SHUTDOWN))
                .count()
                == 2
        }));
        let seen = monitor.messages();
        let sent: Vec<_> = seen
            .iter()
            .filter(|m| m.sender.as_deref() == Some(me.as_str()))
            .map(call_of)
            .collect();
        assert_eq!(
            sent,
            [
                (BUS.into(), BUS.into(), "Hello".into(), Vec::new()),
                (BUS.into(), BUS.into(), "AddMatch".into(), owned(&[RULE])),
                (
                    LOGIND.into(),
                    MANAGER.into(),
                    "Inhibit".into(),
                    owned(&["shutdown", "Bezel", REASON, "delay"]),
                ),
                (
                    LOGIND.into(),
                    PROPERTIES.into(),
                    "Get".into(),
                    owned(&[MANAGER, "InhibitDelayMaxUSec"]),
                ),
            ],
            "{seen:#?}"
        );
        let announced: Vec<_> = seen
            .iter()
            .filter(|m| m.member.as_deref() == Some(PREPARE_FOR_SHUTDOWN))
            .map(|m| {
                (
                    m.kind,
                    m.sender.clone(),
                    m.destination.clone(),
                    m.args.clone(),
                )
            })
            .collect();
        let from_logind = Some(logind.unique_name().to_owned());
        assert_eq!(
            announced,
            [
                (
                    MessageKind::Signal,
                    from_logind.clone(),
                    None,
                    owned(&["true"])
                ),
                (MessageKind::Signal, from_logind, None, owned(&["false"])),
            ]
        );
    }

    #[test]
    fn the_lock_is_a_shutdown_delay_and_dropping_it_closes_the_fd() {
        let bus = bus();
        let logind = FakeLogind::start(&bus.address(), DELAY_MAX).unwrap();
        let client = Logind::connect(&bus.address()).unwrap();
        let inhibitor = client.inhibit(REASON).unwrap();
        let LogindCall::Inhibit {
            what,
            who,
            why,
            mode,
            ..
        } = logind.calls()[0].clone()
        else {
            panic!("{:?}", logind.calls());
        };
        assert_eq!(
            (what.as_str(), who.as_str(), why.as_str(), mode.as_str()),
            ("shutdown", "Bezel", REASON, "delay")
        );
        assert!(logind.wait_for_inhibitors(1, PATIENCE));
        assert_eq!(logind.held(), 1);
        assert!(!logind.wait_for_release(0, Duration::from_millis(200)));
        assert_eq!(format!("{inhibitor:?}"), "Inhibitor");
        drop(inhibitor);
        assert!(logind.wait_for_release(0, PATIENCE));
        assert_eq!(logind.held(), 0);
        // The connection stays: a cancelled shutdown takes a new lock.
        let again = client.inhibit(REASON).unwrap();
        assert!(logind.wait_for_inhibitors(2, PATIENCE));
        assert_eq!(logind.held(), 1);
        drop(again);
        assert!(logind.wait_for_release(1, PATIENCE));
    }

    #[test]
    fn without_a_bus_connecting_is_a_typed_error() {
        for address in [
            "unix:path=/nonexistent/bezel-power/bus",
            "not a D-Bus address",
        ] {
            let error = Logind::connect(&BusAddress::new(address)).err();
            assert!(
                matches!(&error, Some(PowerError::NoBus { address: a, .. }) if a == address),
                "{error:?}"
            );
        }
    }

    #[test]
    fn without_logind_every_call_is_a_typed_error() {
        let bus = bus();
        let client = Logind::connect(&bus.address()).unwrap();
        assert_eq!(client.inhibit(REASON).err(), Some(PowerError::NoLogind));
        assert_eq!(client.delay_max().err(), Some(PowerError::NoLogind));
        assert_eq!(client.wait(Duration::from_millis(50)), Ok(None));
    }

    #[test]
    fn waiting_without_an_announcement_times_out() {
        let bus = bus();
        let _logind = FakeLogind::start(&bus.address(), DELAY_MAX).unwrap();
        let client = Logind::connect(&bus.address()).unwrap();
        assert_eq!(client.wait(Duration::ZERO), Ok(None));
        assert_eq!(client.wait(Duration::from_millis(100)), Ok(None));
    }

    #[test]
    fn only_loginds_broadcast_counts() {
        let bus = bus();
        let logind = FakeLogind::start(&bus.address(), DELAY_MAX).unwrap();
        let monitor = BusMonitor::start(&bus.address()).unwrap();
        let client = Logind::connect(&bus.address()).unwrap();
        let _inhibitor = client.inhibit(REASON).unwrap();
        let Some(LogindCall::Inhibit { sender: me, .. }) = logind.calls().first().cloned() else {
            panic!("{:?}", logind.calls());
        };
        let rogue = open(&bus.address()).unwrap();
        let forged = || {
            Message::new_signal(LOGIND_PATH, MANAGER, PREPARE_FOR_SHUTDOWN)
                .unwrap()
                .append1(true)
        };
        let mut to_bezel_only = forged();
        to_bezel_only.set_destination(Some(me.as_str().into()));
        rogue.send(to_bezel_only).unwrap();
        rogue.send(forged()).unwrap();
        rogue.flush();
        assert!(monitor.wait_for(PATIENCE, |seen| {
            seen.iter()
                .filter(|m| m.member.as_deref() == Some(PREPARE_FOR_SHUTDOWN))
                .count()
                == 2
        }));
        assert_eq!(client.wait(Duration::from_millis(300)), Ok(None));
        logind.prepare_for_shutdown(true).unwrap();
        assert_eq!(client.wait(PATIENCE), Ok(Some(Shutdown::Starting)));
    }

    #[test]
    fn a_lost_bus_is_a_typed_error() {
        let bus = bus();
        let client = Logind::connect(&bus.address()).unwrap();
        drop(bus);
        assert_eq!(client.wait(PATIENCE), Err(PowerError::Disconnected));
        assert_eq!(client.wait(PATIENCE), Err(PowerError::Disconnected));
        assert_eq!(client.inhibit(REASON).err(), Some(PowerError::Disconnected));
        assert_eq!(client.delay_max().err(), Some(PowerError::Disconnected));
    }

    #[test]
    fn prepare_for_shutdown_true_starts_and_false_cancels() {
        assert_eq!(announced(true), Shutdown::Starting);
        assert_eq!(announced(false), Shutdown::Cancelled);
    }

    #[test]
    fn call_errors_are_typed_by_name() {
        let named = |name: &str| dbus::Error::new_custom(name, "why");
        assert_eq!(
            call_error(
                "Inhibit",
                &named("org.freedesktop.DBus.Error.ServiceUnknown")
            ),
            PowerError::NoLogind
        );
        assert_eq!(
            call_error(
                "Inhibit",
                &named("org.freedesktop.DBus.Error.NameHasNoOwner")
            ),
            PowerError::NoLogind
        );
        assert_eq!(
            call_error("Get", &named("org.freedesktop.DBus.Error.Disconnected")),
            PowerError::Disconnected
        );
        let refused = call_error("Inhibit", &named("org.freedesktop.DBus.Error.AccessDenied"));
        assert_eq!(
            refused,
            PowerError::Call {
                call: "Inhibit",
                reason: "org.freedesktop.DBus.Error.AccessDenied: why".into(),
            }
        );
        assert_eq!(
            refused.to_string(),
            "Inhibit failed: org.freedesktop.DBus.Error.AccessDenied: why"
        );
    }
}
