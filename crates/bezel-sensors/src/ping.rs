//! `net.ping`: the round trip to a host, measured by a thread of its own
//! (D-2026-09-30-release-polish-5) so that a slow or mute host never delays
//! a sample (D-2026-09-30-sensors-4): `sample` only reads the last result.
//!
//! A probe sends an ICMP echo through an unprivileged datagram socket
//! (Linux allows one to the groups in `net.ipv4.ping_group_range`), else
//! times a TCP connect to port 53, then 443 (one SYN, SYN-ACK round trip).
//! No answer within [`TIMEOUT`] reads as unavailable, and so does a result
//! older than the probe period allows (the probe itself is stuck, for
//! example resolving the host name).

use std::io;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};
use socket2::{Domain, Protocol, Socket, Type};

use crate::provider::{Provider, describe, put};

/// Time between the end of one probe and the start of the next.
const EVERY: Duration = Duration::from_secs(1);
/// How long one probe waits for its answer.
const TIMEOUT: Duration = Duration::from_secs(2);
/// Slack over `every + timeout` before a result counts as stale.
const GRACE: Duration = Duration::from_secs(2);
/// TCP ports tried, in order, when ICMP is not allowed.
const TCP_PORTS: [u16; 2] = [53, 443];

/// ICMP message types (RFC 792, RFC 4443).
const ECHO_REQUEST_V4: u8 = 8;
const ECHO_REPLY_V4: u8 = 0;
const ECHO_REQUEST_V6: u8 = 128;
const ECHO_REPLY_V6: u8 = 129;
/// What an echo carries after its 8-byte header, and its answer returns.
const PAYLOAD: [u8; 8] = *b"bezelpng";

/// Measures one round trip.
pub(crate) trait Probe: Send + 'static {
    /// One round trip, or why there is none.
    fn round_trip(&mut self) -> Result<Duration, String>;
}

/// The last probe's result and when it ended.
type Latest = Arc<Mutex<Option<(Instant, Result<Duration, String>)>>>;

/// The `net.ping` provider: shows what its probe thread measured last.
pub(crate) struct Ping {
    host: String,
    latest: Latest,
    /// A result older than this is not shown.
    stale_after: Duration,
    /// Dropped with the provider, which ends the probe thread.
    _stop: Sender<()>,
}

impl Ping {
    /// Pings `host` every second, on a thread of its own.
    pub(crate) fn start(host: &str) -> Self {
        Self::with_probe(host, NetProbe::new(host, TIMEOUT), EVERY, TIMEOUT)
    }

    /// Runs `probe` on a thread, `every` apart; one probe takes at most
    /// `timeout`.
    pub(crate) fn with_probe(
        host: &str,
        probe: impl Probe,
        every: Duration,
        timeout: Duration,
    ) -> Self {
        let latest: Latest = Arc::default();
        let (stop, stopped) = mpsc::channel();
        let shared = Arc::clone(&latest);
        let spawned = thread::Builder::new()
            .name("bezel-ping".to_string())
            .spawn(move || run(probe, every, &shared, &stopped));
        if let Err(e) = spawned {
            store(&latest, Err(format!("cannot start the ping task: {e}")));
        }
        Self {
            host: host.to_string(),
            latest,
            stale_after: every + timeout + GRACE,
            _stop: stop,
        }
    }

    /// What `net.ping` reads at `now`: milliseconds, or why not.
    fn reading(&self, now: Instant) -> Reading {
        let latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        match latest.as_ref() {
            None => Reading::Unavailable(format!("measuring: no answer from {} yet", self.host)),
            Some((at, _)) if now.saturating_duration_since(*at) > self.stale_after => {
                Reading::Unavailable(format!(
                    "no answer from {} for {} s",
                    self.host,
                    now.saturating_duration_since(*at).as_secs()
                ))
            }
            Some((_, Ok(round_trip))) => Reading::Value(milliseconds(*round_trip)),
            Some((_, Err(why))) => Reading::Unavailable(why.clone()),
        }
    }
}

impl Provider for Ping {
    fn catalog(&self) -> Vec<SensorInfo> {
        describe(
            keys::NET_PING,
            Category::Network,
            "Ping",
            Quantity::Number,
            format!(
                "round trip to {}, ms: ICMP echo, else TCP connect to port 53 or 443",
                self.host
            ),
        )
        .into_iter()
        .collect()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        put(out, keys::NET_PING, self.reading(now));
    }
}

/// `duration` in milliseconds, to the microsecond (no binary noise such
/// as `3.6089450000000003`).
fn milliseconds(duration: Duration) -> f64 {
    f64::from(u32::try_from(duration.as_micros()).unwrap_or(u32::MAX)) / 1000.0
}

/// The probe thread: probes, stores, waits; ends when the provider drops.
fn run(mut probe: impl Probe, every: Duration, latest: &Latest, stop: &Receiver<()>) {
    loop {
        let result = probe.round_trip();
        store(latest, result);
        if !matches!(stop.recv_timeout(every), Err(RecvTimeoutError::Timeout)) {
            return;
        }
    }
}

fn store(latest: &Latest, result: Result<Duration, String>) {
    *latest.lock().unwrap_or_else(PoisonError::into_inner) = Some((Instant::now(), result));
}

/// Pings a host over the network.
pub(crate) struct NetProbe {
    host: String,
    timeout: Duration,
    /// False once an ICMP socket could not be opened: TCP from then on.
    icmp: bool,
    sequence: u16,
}

impl NetProbe {
    /// A probe of `host` (a name or an address) that waits `timeout`.
    pub(crate) fn new(host: &str, timeout: Duration) -> Self {
        Self {
            host: host.to_string(),
            timeout,
            icmp: true,
            sequence: 0,
        }
    }

    /// Why a probe got no round trip.
    fn failure(&self, error: &io::Error, how: &str) -> String {
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            format!(
                "no answer from {} within {} ms ({how})",
                self.host,
                self.timeout.as_millis()
            )
        } else {
            format!("{} did not answer ({how}): {error}", self.host)
        }
    }
}

impl Probe for NetProbe {
    fn round_trip(&mut self) -> Result<Duration, String> {
        let ip = resolve(&self.host)?;
        if self.icmp {
            self.sequence = self.sequence.wrapping_add(1);
            match echo(ip, self.sequence, self.timeout) {
                Ok(round_trip) => return Ok(round_trip),
                Err(Echo::NoSocket) => self.icmp = false,
                Err(Echo::Failed(e)) => return Err(self.failure(&e, "ICMP echo")),
            }
        }
        connect(ip, &TCP_PORTS, self.timeout)
            .map_err(|e| self.failure(&e, "TCP connect to port 53 or 443"))
    }
}

/// The first address of `host`.
fn resolve(host: &str) -> Result<IpAddr, String> {
    (host, 0)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}: {e}"))?
        .next()
        .map(|address| address.ip())
        .ok_or_else(|| format!("{host} has no address"))
}

/// Why an ICMP echo gave no round trip.
#[derive(Debug)]
enum Echo {
    /// No unprivileged ICMP socket here (not in `ping_group_range`, or
    /// not supported by the system).
    NoSocket,
    /// The echo went out and no answer came, or sending failed.
    Failed(io::Error),
}

/// One ICMP echo to `ip` through an unprivileged datagram socket.
fn echo(ip: IpAddr, sequence: u16, timeout: Duration) -> Result<Duration, Echo> {
    let (domain, protocol, request, reply) = match ip {
        IpAddr::V4(_) => (
            Domain::IPV4,
            Protocol::ICMPV4,
            ECHO_REQUEST_V4,
            ECHO_REPLY_V4,
        ),
        IpAddr::V6(_) => (
            Domain::IPV6,
            Protocol::ICMPV6,
            ECHO_REQUEST_V6,
            ECHO_REPLY_V6,
        ),
    };
    let socket: UdpSocket = Socket::new(domain, Type::DGRAM, Some(protocol))
        .map_err(|_| Echo::NoSocket)?
        .into();
    let start = Instant::now();
    socket
        .send_to(&echo_request(request, sequence), SocketAddr::new(ip, 0))
        .map_err(Echo::Failed)?;
    let mut buffer = [0u8; 1500];
    loop {
        let left = timeout
            .checked_sub(start.elapsed())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| Echo::Failed(io::ErrorKind::TimedOut.into()))?;
        socket.set_read_timeout(Some(left)).map_err(Echo::Failed)?;
        let (len, from) = socket.recv_from(&mut buffer).map_err(Echo::Failed)?;
        if from.ip() == ip && is_reply(&buffer[..len], reply, sequence) {
            return Ok(start.elapsed());
        }
    }
}

/// An echo request: type, code 0, checksum, identifier 0 (the kernel sets
/// its own on a datagram socket), `sequence`, [`PAYLOAD`]. The ICMPv6
/// checksum covers a pseudo-header, so the kernel fills that one in.
fn echo_request(kind: u8, sequence: u16) -> [u8; 16] {
    let mut packet = [0u8; 16];
    packet[0] = kind;
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());
    packet[8..].copy_from_slice(&PAYLOAD);
    if kind == ECHO_REQUEST_V4 {
        let sum = checksum(&packet);
        packet[2..4].copy_from_slice(&sum.to_be_bytes());
    }
    packet
}

/// The Internet checksum (RFC 1071).
fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = bytes
        .chunks(2)
        .map(|pair| {
            u32::from(u16::from_be_bytes([
                pair[0],
                pair.get(1).copied().unwrap_or(0),
            ]))
        })
        .sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// True for the answer to the echo `sequence`. Linux hands a datagram
/// socket the ICMP message alone; some systems keep the IPv4 header in
/// front (first nibble 4, where no ICMP echo type starts), which is skipped.
fn is_reply(data: &[u8], kind: u8, sequence: u16) -> bool {
    let message = match data.first() {
        Some(first) if first >> 4 == 4 => data.get(usize::from(first & 0x0f) * 4..),
        _ => Some(data),
    };
    let Some(message) = message else {
        return false;
    };
    message.first() == Some(&kind)
        && message.get(6..8) == Some(&sequence.to_be_bytes()[..])
        && message.get(8..16) == Some(&PAYLOAD[..])
}

/// Times a TCP connect to the first of `ports` that accepts one.
fn connect(ip: IpAddr, ports: &[u16], timeout: Duration) -> io::Result<Duration> {
    let deadline = Instant::now() + timeout;
    let mut last = io::Error::from(io::ErrorKind::TimedOut);
    for port in ports {
        let Some(left) = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
        else {
            break;
        };
        let start = Instant::now();
        match TcpStream::connect_timeout(&SocketAddr::new(ip, *port), left) {
            Ok(_) => return Ok(start.elapsed()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::sensor::SensorKey;
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Answers `result` every time and counts the calls.
    struct Scripted {
        result: Result<Duration, String>,
        calls: Arc<AtomicUsize>,
    }

    impl Probe for Scripted {
        fn round_trip(&mut self) -> Result<Duration, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    fn scripted(result: Result<Duration, String>) -> (Scripted, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let probe = Scripted {
            result,
            calls: Arc::clone(&calls),
        };
        (probe, calls)
    }

    /// A host that never answers: the probe blocks until the test ends.
    struct Mute(Receiver<()>);

    impl Probe for Mute {
        fn round_trip(&mut self) -> Result<Duration, String> {
            let _ = self.0.recv();
            Err("released".to_string())
        }
    }

    fn sample(ping: &mut Ping) -> Reading {
        let mut out = Snapshot::default();
        ping.sample(Instant::now(), &mut out);
        assert_eq!(out.len(), 1);
        out.get(&SensorKey::new(keys::NET_PING).unwrap())
    }

    /// Samples until the probe thread stored a result (at most 3 s).
    fn first_result(ping: &mut Ping) -> Reading {
        let start = Instant::now();
        loop {
            let reading = sample(ping);
            let measuring =
                matches!(&reading, Reading::Unavailable(why) if why.starts_with("measuring"));
            if !measuring || start.elapsed() > Duration::from_secs(3) {
                return reading;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn ping_is_listed_in_network() {
        let (probe, _) = scripted(Ok(Duration::ZERO));
        let ping = Ping::with_probe("8.8.8.8", probe, EVERY, TIMEOUT);
        let catalog = ping.catalog();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].key.as_str(), keys::NET_PING);
        assert_eq!(catalog[0].category, Category::Network);
        assert_eq!(catalog[0].quantity, Quantity::Number);
        assert!(catalog[0].source.starts_with("round trip to 8.8.8.8, ms"));
    }

    #[test]
    fn a_mute_host_never_slows_sampling() {
        let (release, blocked) = mpsc::channel();
        let mut ping = Ping::with_probe("192.0.2.1", Mute(blocked), EVERY, TIMEOUT);
        for _ in 0..20 {
            let start = Instant::now();
            let reading = sample(&mut ping);
            assert!(start.elapsed() < Duration::from_millis(50));
            assert_eq!(
                reading,
                Reading::Unavailable("measuring: no answer from 192.0.2.1 yet".into())
            );
        }
        drop(release);
    }

    #[test]
    fn a_reply_reads_in_milliseconds() {
        let (probe, _) = scripted(Ok(Duration::from_micros(12_345)));
        let mut ping = Ping::with_probe("h", probe, EVERY, TIMEOUT);
        let reading = first_result(&mut ping);
        assert_eq!(reading, Reading::Value(12.345));
        assert_eq!(milliseconds(Duration::from_nanos(3_608_945)), 3.608);
    }

    #[test]
    fn a_timeout_reads_unavailable() {
        let why = "no answer from h within 2000 ms (ICMP echo)".to_string();
        let (probe, _) = scripted(Err(why.clone()));
        let mut ping = Ping::with_probe("h", probe, EVERY, TIMEOUT);
        assert_eq!(first_result(&mut ping), Reading::Unavailable(why));
    }

    #[test]
    fn an_old_result_is_not_shown() {
        let (probe, _) = scripted(Ok(Duration::from_millis(9)));
        let mut ping = Ping::with_probe("h", probe, EVERY, TIMEOUT);
        assert_eq!(first_result(&mut ping), Reading::Value(9.0));
        let later = Instant::now() + Duration::from_secs(8);
        let Reading::Unavailable(why) = ping.reading(later) else {
            unreachable!("a result 8 s old is stale");
        };
        assert!(why.starts_with("no answer from h for "), "{why}");
    }

    #[test]
    fn the_probe_thread_ends_with_the_sensor() {
        let (probe, calls) = scripted(Ok(Duration::ZERO));
        let ping = Ping::with_probe("h", probe, Duration::from_millis(5), TIMEOUT);
        let start = Instant::now();
        while calls.load(Ordering::SeqCst) < 2 && start.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(5));
        }
        drop(ping);
        thread::sleep(Duration::from_millis(100));
        let after_drop = calls.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(100));
        assert!(after_drop >= 2);
        assert_eq!(calls.load(Ordering::SeqCst), after_drop);
    }

    #[test]
    fn echo_requests_carry_their_sequence_and_checksum() {
        let v4 = echo_request(ECHO_REQUEST_V4, 0x0102);
        assert_eq!(v4[0..2], [8, 0]);
        assert_eq!(v4[6..8], [1, 2]);
        assert_eq!(v4[8..], PAYLOAD);
        assert_eq!(checksum(&v4), 0, "a valid checksum sums to zero");
        let v6 = echo_request(ECHO_REQUEST_V6, 7);
        assert_eq!(v6[0..4], [128, 0, 0, 0]);
        // RFC 1071's example: 00 01 f2 03 f4 f5 f6 f7 sums to ddf2.
        assert_eq!(
            checksum(&[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]),
            !0xddf2
        );
        assert_eq!(checksum(&[0xff]), !0xff00);
    }

    #[test]
    fn replies_match_type_sequence_and_payload() {
        let mut reply = echo_request(ECHO_REQUEST_V4, 42);
        reply[0] = ECHO_REPLY_V4;
        assert!(is_reply(&reply, ECHO_REPLY_V4, 42));
        assert!(!is_reply(&reply, ECHO_REPLY_V4, 43));
        assert!(!is_reply(&reply, ECHO_REPLY_V6, 42));
        assert!(!is_reply(&reply[..12], ECHO_REPLY_V4, 42));
        let mut with_ip_header = vec![0x45; 20];
        with_ip_header.extend_from_slice(&reply);
        assert!(is_reply(&with_ip_header, ECHO_REPLY_V4, 42));
        assert!(!is_reply(&[0x4f, 0, 0], ECHO_REPLY_V4, 42));
        assert!(!is_reply(&[], ECHO_REPLY_V4, 42));
    }

    #[test]
    fn tcp_times_a_connect() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let open = listener.local_addr().unwrap().port();
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let timeout = Duration::from_secs(1);
        assert!(connect(local, &[open], timeout).unwrap() < timeout);
        let none = connect(local, &[], timeout).unwrap_err();
        assert_eq!(none.kind(), io::ErrorKind::TimedOut);
    }

    /// Unix refuses a closed port at once (Windows retries its SYN for
    /// about a second first).
    #[cfg(unix)]
    #[test]
    fn tcp_moves_on_from_a_refused_port() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let open = listener.local_addr().unwrap().port();
        let closed = {
            let spare = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            spare.local_addr().unwrap().port()
        };
        let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let timeout = Duration::from_secs(1);
        assert!(connect(local, &[closed, open], timeout).unwrap() < timeout);
        let refused = connect(local, &[closed], timeout).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::ConnectionRefused);
    }

    #[test]
    fn failures_say_what_was_tried() {
        let probe = NetProbe::new("h", Duration::from_millis(300));
        let timed_out = io::Error::from(io::ErrorKind::TimedOut);
        assert_eq!(
            probe.failure(&timed_out, "ICMP echo"),
            "no answer from h within 300 ms (ICMP echo)"
        );
        let refused = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert!(
            probe
                .failure(&refused, "TCP")
                .starts_with("h did not answer (TCP): ")
        );
        assert_eq!(
            resolve("8.8.8.8"),
            Ok(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)))
        );
        assert!(resolve("localhost").is_ok());
    }

    /// This machine, read-only: loopback answers an ICMP echo, or this
    /// user may not open ICMP sockets (outside `ping_group_range`) and TCP
    /// takes over.
    #[cfg(target_os = "linux")]
    #[test]
    fn loopback_answers_an_echo_or_icmp_is_not_allowed() {
        let answer = echo(IpAddr::V4(Ipv4Addr::LOCALHOST), 1, Duration::from_secs(1));
        assert!(matches!(answer, Ok(_) | Err(Echo::NoSocket)), "{answer:?}");
    }

    /// This machine's network: TEST-NET-1 (RFC 5737) never answers, and
    /// sampling does not wait for it.
    #[test]
    fn a_mute_address_is_unavailable_without_blocking_samples() {
        let host = "192.0.2.1";
        let probe = NetProbe::new(host, Duration::from_millis(300));
        let mut ping = Ping::with_probe(host, probe, Duration::from_millis(50), TIMEOUT);
        let start = Instant::now();
        let first = sample(&mut ping);
        assert!(start.elapsed() < Duration::from_millis(50));
        assert!(matches!(first, Reading::Unavailable(_)), "{first:?}");
        let reading = first_result(&mut ping);
        assert!(
            matches!(&reading, Reading::Unavailable(why) if why.contains(host)),
            "{reading:?}"
        );
    }
}
