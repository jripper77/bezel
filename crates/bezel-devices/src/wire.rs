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
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.port.write_all(bytes)?;
        self.port.flush()
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
}
