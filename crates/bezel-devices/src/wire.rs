//! Byte transports the drivers talk through, and a scripted fake for tests.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

/// Silence after the last received byte that ends a reply.
const QUIET: Duration = Duration::from_millis(30);

/// A bidirectional byte pipe to one device endpoint. Adapter-internal: the
/// core never sees it.
pub trait Wire: Send {
    /// Writes every byte.
    fn send(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// What the device says within `timeout`: returns once some bytes arrived
    /// and the line stayed quiet briefly, once `max` bytes arrived, or empty
    /// on timeout.
    fn receive(&mut self, max: usize, timeout: Duration) -> io::Result<Vec<u8>>;
    /// Drops unread input.
    fn discard_input(&mut self) -> io::Result<()>;
}

/// Serial flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// None (rev C, as the vendor app opens it).
    None,
    /// RTS/CTS (rev A/B/D and WeAct, as the Python reference opens them).
    Hardware,
}

/// How long a write may make no progress before it fails: the SoC reads
/// while it writes to its flash or the memory card, which can stall.
const WRITE_STALL: Duration = Duration::from_secs(10);
/// Pause between two looks at the bytes still queued for the device.
const DRAIN_POLL: Duration = Duration::from_millis(1);

/// Writes every byte of `bytes` through `write`: a write a signal
/// interrupted is tried again, and so is one that timed out while the last
/// progress (on `now`'s clock) is less than `stall` old. A write that takes
/// no bytes, one stalled for `stall` and any other error fail.
fn write_patiently(
    mut bytes: &[u8],
    stall: Duration,
    mut now: impl FnMut() -> Instant,
    mut write: impl FnMut(&[u8]) -> io::Result<usize>,
) -> io::Result<()> {
    let mut progress = now();
    while !bytes.is_empty() {
        match write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "the port took no bytes",
                ));
            }
            Ok(n) => {
                bytes = bytes.get(n..).unwrap_or_default();
                progress = now();
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e)
                if e.kind() == io::ErrorKind::TimedOut
                    && now().saturating_duration_since(progress) < stall => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Waits until the port has sent everything, watching `queued` (bytes
/// still in the host's output buffer) instead of blocking in the kernel's
/// drain: that one waits forever when the device stops reading (seen on
/// the 8.8": a firmware that hung mid-upload kept a sender blocked for good)
/// and gives up early when a signal interrupts it. The queue must shrink
/// at least once every `stall`, else the device stopped reading.
fn drain_watching(
    stall: Duration,
    mut now: impl FnMut() -> Instant,
    mut queued: impl FnMut() -> io::Result<u32>,
    mut pause: impl FnMut(),
) -> io::Result<()> {
    let mut progress = now();
    let mut last = u32::MAX;
    loop {
        let left = queued()?;
        if left == 0 {
            return Ok(());
        }
        if left < last {
            last = left;
            progress = now();
        } else if now().saturating_duration_since(progress) >= stall {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("the screen stopped reading ({left} bytes still queued)"),
            ));
        }
        pause();
    }
}

/// A CDC-ACM serial port.
pub struct SerialWire {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialWire {
    /// Opens `path` at 115200 8N1 with DTR and RTS on (the baud rate is only a
    /// SET_LINE_CODING value on CDC-ACM) and the given flow control.
    pub fn open(path: &str, flow: Flow) -> io::Result<Self> {
        let flow = match flow {
            Flow::None => serialport::FlowControl::None,
            Flow::Hardware => serialport::FlowControl::Hardware,
        };
        let port = serialport::new(path, 115_200)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(flow)
            .dtr_on_open(true)
            .timeout(Duration::from_millis(10))
            .open()
            .map_err(io::Error::other)?;
        let mut wire = Self { port };
        wire.port
            .write_request_to_send(true)
            .map_err(io::Error::other)?;
        Ok(wire)
    }
}

impl Wire for SerialWire {
    /// Writes every byte and waits until they left. The port's 10 ms timeout
    /// only paces reads: a write that makes no progress for [`WRITE_STALL`]
    /// fails, and so does a drain whose queue stops shrinking for as long
    /// ([`drain_watching`]; the kernel's own drain can block forever, and a
    /// signal such as Ctrl+C cuts it short). Seen on
    /// the 8.8" during a memory-card upload.
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        write_patiently(bytes, WRITE_STALL, Instant::now, |rest| {
            self.port.write(rest)
        })?;
        let port = &self.port;
        drain_watching(
            WRITE_STALL,
            Instant::now,
            || port.bytes_to_write().map_err(io::Error::other),
            || std::thread::sleep(DRAIN_POLL),
        )
    }

    fn receive(&mut self, max: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        let mut last_byte = Instant::now();
        let mut buf = [0u8; 1024];
        while out.len() < max {
            let now = Instant::now();
            if now >= deadline || (!out.is_empty() && now.duration_since(last_byte) >= QUIET) {
                break;
            }
            match self.port.read(&mut buf[..(max - out.len()).min(1024)]) {
                Ok(0) => {}
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    last_byte = Instant::now();
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    fn discard_input(&mut self) -> io::Result<()> {
        self.port
            .clear(serialport::ClearBuffer::Input)
            .map_err(io::Error::other)
    }
}

/// A fake wire that records what is sent and answers from a script.
#[derive(Debug, Default)]
pub struct ScriptedWire {
    /// Every `send`, in order.
    pub sent: Vec<Vec<u8>>,
    replies: VecDeque<Vec<u8>>,
    /// Number of `discard_input` calls.
    pub discards: usize,
}

impl ScriptedWire {
    /// A wire answering each `receive` with the next scripted reply (then silence).
    pub fn with_replies<I: IntoIterator<Item = Vec<u8>>>(replies: I) -> Self {
        Self {
            replies: replies.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Queues one more reply.
    pub fn reply(&mut self, bytes: &[u8]) {
        self.replies.push_back(bytes.to_vec());
    }

    /// Sent packets whose first byte is `opcode`.
    pub fn sent_with_opcode(&self, opcode: u8) -> Vec<&Vec<u8>> {
        self.sent
            .iter()
            .filter(|p| p.first() == Some(&opcode))
            .collect()
    }
}

impl Wire for ScriptedWire {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.sent.push(bytes.to_vec());
        Ok(())
    }

    fn receive(&mut self, max: usize, _timeout: Duration) -> io::Result<Vec<u8>> {
        let mut reply = self.replies.pop_front().unwrap_or_default();
        reply.truncate(max);
        Ok(reply)
    }

    fn discard_input(&mut self) -> io::Result<()> {
        self.discards += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_wire_records_and_replies() {
        let mut w = ScriptedWire::with_replies([b"abc".to_vec()]);
        w.reply(b"xyz");
        w.send(&[1, 2]).unwrap();
        w.send(&[3]).unwrap();
        assert_eq!(w.receive(2, Duration::ZERO).unwrap(), b"ab");
        assert_eq!(w.receive(10, Duration::ZERO).unwrap(), b"xyz");
        assert!(w.receive(10, Duration::ZERO).unwrap().is_empty());
        w.discard_input().unwrap();
        assert_eq!(w.discards, 1);
        assert_eq!(w.sent_with_opcode(3).len(), 1);
    }

    #[test]
    fn opening_a_missing_port_fails_cleanly() {
        assert!(SerialWire::open("/dev/bezel-no-such-port", Flow::None).is_err());
        assert!(SerialWire::open("/dev/bezel-no-such-port", Flow::Hardware).is_err());
    }

    /// A clock that moves one second on every look.
    fn ticking() -> impl FnMut() -> Instant {
        let start = Instant::now();
        let mut ticks = 0;
        move || {
            ticks += 1;
            start + Duration::from_secs(ticks)
        }
    }

    fn failure(kind: io::ErrorKind) -> io::Error {
        io::Error::new(kind, "scripted")
    }

    fn timeouts(n: usize) -> impl Iterator<Item = io::Result<usize>> {
        (0..n).map(|_| Err(failure(io::ErrorKind::TimedOut)))
    }

    /// Writes `bytes` through a port whose writes answer `script` in order
    /// (then take everything); returns the result, the bytes the port took
    /// and the number of writes.
    fn write_with(
        bytes: &[u8],
        stall: Duration,
        script: Vec<io::Result<usize>>,
    ) -> (io::Result<()>, Vec<u8>, usize) {
        let mut script = VecDeque::from(script);
        let mut taken = Vec::new();
        let mut writes = 0;
        let result = write_patiently(bytes, stall, ticking(), |rest| {
            writes += 1;
            let n = script.pop_front().unwrap_or(Ok(rest.len()))?;
            taken.extend_from_slice(&rest[..n.min(rest.len())]);
            Ok(n)
        });
        (result, taken, writes)
    }

    #[test]
    fn writes_survive_signals_and_short_stalls() {
        let script = vec![
            Err(failure(io::ErrorKind::Interrupted)),
            Ok(2),
            Err(failure(io::ErrorKind::Interrupted)),
            Err(failure(io::ErrorKind::TimedOut)),
            Ok(1),
        ];
        let (result, taken, writes) = write_with(b"abcde", WRITE_STALL, script);
        result.unwrap();
        assert_eq!(taken, b"abcde");
        assert_eq!(writes, 6);

        // Progress restarts the stall: 8 s of timeouts, a byte, 8 s again,
        // each under the 10 s limit.
        let mut script: Vec<io::Result<usize>> = timeouts(8).collect();
        script.push(Ok(1));
        script.extend(timeouts(8));
        let (result, taken, writes) = write_with(b"xy", WRITE_STALL, script);
        result.unwrap();
        assert_eq!((taken.as_slice(), writes), (&b"xy"[..], 18));
    }

    #[test]
    fn a_stalled_write_fails_at_the_limit() {
        let script = timeouts(100).collect();
        let (result, taken, writes) = write_with(b"abc", Duration::from_secs(3), script);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(taken.is_empty());
        // The clock moves 1 s per look: the third timeout is 3 s after the
        // start, the limit.
        assert_eq!(writes, 3);
    }

    #[test]
    fn writes_that_take_nothing_or_fail_otherwise_end_at_once() {
        let (result, _, writes) = write_with(b"abc", WRITE_STALL, vec![Ok(0)]);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WriteZero);
        assert_eq!(writes, 1);
        let script = vec![Ok(1), Err(failure(io::ErrorKind::BrokenPipe))];
        let (result, taken, writes) = write_with(b"abc", WRITE_STALL, script);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
        assert_eq!((taken.as_slice(), writes), (&b"a"[..], 2));
        let (result, _, writes) = write_with(b"", WRITE_STALL, vec![]);
        result.unwrap();
        assert_eq!(writes, 0, "nothing to write");
    }

    /// Drains watching a queue that reports `script` in order (then 0),
    /// with a clock moving 1 s per look; returns the result and the looks.
    fn drain_with(script: Vec<io::Result<u32>>) -> (io::Result<()>, usize) {
        let mut script = VecDeque::from(script);
        let start = Instant::now();
        let mut ticks = 0u64;
        let mut looks = 0;
        let result = drain_watching(
            Duration::from_secs(3),
            || {
                ticks += 1;
                start + Duration::from_secs(ticks)
            },
            || {
                looks += 1;
                script.pop_front().unwrap_or(Ok(0))
            },
            || {},
        );
        (result, looks)
    }

    #[test]
    fn a_drain_waits_while_the_queue_shrinks_and_fails_when_it_stalls() {
        let (result, looks) = drain_with(vec![Ok(0)]);
        result.unwrap();
        assert_eq!(looks, 1, "nothing queued: done at once");

        // Shrinking, however slowly, is progress.
        let (result, looks) = drain_with((1..=20).rev().map(Ok).collect());
        result.unwrap();
        assert_eq!(looks, 21);

        // A device that stopped reading: the same count for 3 s fails
        // instead of blocking forever.
        let (result, looks) = drain_with((0..100).map(|_| Ok(10_240)).collect());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            err.to_string().contains("10240 bytes still queued"),
            "{err}"
        );
        assert!(looks < 10, "{looks}");

        // The port's own error stands.
        let (result, _) = drain_with(vec![Ok(5), Err(failure(io::ErrorKind::BrokenPipe))]);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }
}
