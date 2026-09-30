//! Raw USB bulk transport (nusb) behind the [`crate::wire::Wire`] trait.
//!
//! The Turing USB (VID 0x1CBE) and WCH (VID 0x43A8) families are not serial
//! ports: the host claims interface 0 and exchanges bulk transfers on two
//! fixed endpoints (D-2026-09-30-device-protocols-4). Discovery names such an
//! endpoint `usb:<bus>-<port>.<port>…` (see `crate::discovery`), which is
//! stable across re-enumeration, unlike the device address.

use std::io;
use std::time::Duration;

use bezel_core::domain::discovery::UsbLocation;
use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Bulk, Completion, In, Interrupt, Out, TransferError};
use nusb::{Endpoint, Interface, MaybeFuture};

use crate::wire::Wire;

/// Prefix of the discovery address of a raw USB endpoint.
pub const ADDRESS_PREFIX: &str = "usb:";

/// Timeout of a write, as the vendor app uses per transfer.
const WRITE_TIMEOUT: Duration = Duration::from_millis(2000);
/// Reads made by [`Wire::discard_input`] (the Python reference flushes up to 5 times).
const DRAIN_READS: usize = 5;
/// How long one drain read waits for stale data.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(10);

/// Which interface and endpoints a family talks through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoints {
    /// Interface number to claim.
    pub interface: u8,
    /// Bulk OUT endpoint address (e.g. `0x01`).
    pub out: u8,
    /// IN endpoint address (e.g. `0x81`); bulk or interrupt, as the descriptor says.
    pub input: u8,
}

/// Parses a discovery address `usb:<bus>-<p1>.<p2>…` into its location.
///
/// The bus identifier is everything up to the last `-` (it is a number on
/// Linux but an opaque string elsewhere); the port chain is one or more
/// dot-separated port numbers.
pub fn parse_address(address: &str) -> Option<UsbLocation> {
    let rest = address.strip_prefix(ADDRESS_PREFIX)?;
    let (bus, chain) = rest.rsplit_once('-')?;
    if bus.is_empty() {
        return None;
    }
    let ports = chain
        .split('.')
        .map(|p| p.parse::<u8>().ok())
        .collect::<Option<Vec<u8>>>()?;
    Some(UsbLocation {
        bus: bus.to_string(),
        ports,
    })
}

/// Length to request for an IN transfer that should return up to `max`
/// bytes: a non-zero multiple of the endpoint's maximum packet size, as the
/// host controller requires.
pub fn in_request_len(max: usize, packet: usize) -> usize {
    let packet = packet.max(1);
    max.max(1).div_ceil(packet) * packet
}

/// Timeout of a write of `len` bytes: the vendor's 2 s, plus 1 ms per KiB so a
/// 1 MiB frame still fits on a full-speed bus.
pub fn write_timeout(len: usize) -> Duration {
    WRITE_TIMEOUT + Duration::from_millis((len / 1024) as u64)
}

/// The IN endpoint: bulk on the known devices, interrupt when the descriptor
/// says so (the WCH vendor app opens it as "interrupt").
enum InEndpoint {
    Bulk(Endpoint<Bulk, In>),
    Interrupt(Endpoint<Interrupt, In>),
}

impl InEndpoint {
    fn max_packet_size(&self) -> usize {
        match self {
            InEndpoint::Bulk(ep) => ep.max_packet_size(),
            InEndpoint::Interrupt(ep) => ep.max_packet_size(),
        }
    }

    fn transfer(&mut self, len: usize, timeout: Duration) -> Completion {
        match self {
            InEndpoint::Bulk(ep) => ep.transfer_blocking(Buffer::new(len), timeout),
            InEndpoint::Interrupt(ep) => ep.transfer_blocking(Buffer::new(len), timeout),
        }
    }
}

/// A claimed USB interface with one OUT and one IN endpoint.
pub struct UsbWire {
    address: String,
    // Kept so the claim lives as long as the wire.
    _interface: Interface,
    out: Endpoint<Bulk, Out>,
    input: InEndpoint,
}

impl std::fmt::Debug for UsbWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsbWire")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl UsbWire {
    /// Opens the device at the discovery `address`, claims `endpoints.interface`
    /// (detaching a bound kernel driver on Linux) and opens both endpoints.
    pub fn open(address: &str, endpoints: Endpoints) -> io::Result<Self> {
        let location = parse_address(address).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("not a USB address: {address}"),
            )
        })?;
        let info = nusb::list_devices()
            .wait()?
            .find(|d| d.bus_id() == location.bus && d.port_chain() == location.ports)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no USB device at {address}"),
                )
            })?;
        let device = info.open().wait()?;
        let interface = device
            .detach_and_claim_interface(endpoints.interface)
            .wait()?;
        let out = interface.endpoint::<Bulk, Out>(endpoints.out)?;
        let in_type = interface.descriptor().and_then(|d| {
            d.endpoints()
                .find(|e| e.address() == endpoints.input)
                .map(|e| e.transfer_type())
        });
        let input = match in_type {
            Some(TransferType::Interrupt) => {
                InEndpoint::Interrupt(interface.endpoint::<Interrupt, In>(endpoints.input)?)
            }
            _ => InEndpoint::Bulk(interface.endpoint::<Bulk, In>(endpoints.input)?),
        };
        Ok(Self {
            address: address.to_string(),
            _interface: interface,
            out,
            input,
        })
    }

    /// The discovery address this wire was opened with.
    pub fn address(&self) -> &str {
        &self.address
    }
}

/// Maps a transfer error; a cancelled transfer is how nusb reports a timeout.
fn transfer_error(e: TransferError) -> io::Error {
    match e {
        TransferError::Cancelled => io::Error::new(io::ErrorKind::TimedOut, e),
        other => other.into(),
    }
}

/// The bytes an IN completion carries, up to `max`; a timeout (cancellation)
/// keeps what arrived before it, as a reply that did not come is not an error.
fn received(completion: Completion, max: usize) -> io::Result<Vec<u8>> {
    match completion.status {
        Ok(()) | Err(TransferError::Cancelled) => {
            let len = completion.actual_len.min(max).min(completion.buffer.len());
            Ok(completion.buffer[..len].to_vec())
        }
        Err(e) => Err(transfer_error(e)),
    }
}

impl Wire for UsbWire {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let completion = self
            .out
            .transfer_blocking(Buffer::from(bytes.to_vec()), write_timeout(bytes.len()));
        completion.status.map_err(transfer_error)?;
        if completion.actual_len != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                format!(
                    "short USB write: {} of {} bytes",
                    completion.actual_len,
                    bytes.len()
                ),
            ));
        }
        Ok(())
    }

    fn receive(&mut self, max: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let len = in_request_len(max, self.input.max_packet_size());
        received(self.input.transfer(len, timeout), max)
    }

    fn discard_input(&mut self) -> io::Result<()> {
        let packet = in_request_len(1, self.input.max_packet_size());
        for _ in 0..DRAIN_READS {
            let stale = received(self.input.transfer(packet, DRAIN_TIMEOUT), packet)?;
            if stale.is_empty() {
                break;
            }
            tracing::trace!(bytes = stale.len(), "discarded stale USB input");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: Endpoints = Endpoints {
        interface: 0,
        out: 0x01,
        input: 0x81,
    };

    #[test]
    fn addresses_round_trip_with_discovery() {
        let loc = parse_address("usb:3-1.2").unwrap();
        assert_eq!(loc.bus, "3");
        assert_eq!(loc.ports, vec![1, 2]);
        assert_eq!(format!("{ADDRESS_PREFIX}{loc}"), "usb:3-1.2");
        let deep = parse_address("usb:1-4.3.2.1").unwrap();
        assert_eq!(deep.ports, vec![4, 3, 2, 1]);
        // Non-Linux bus ids are opaque strings: split at the last dash.
        let odd = parse_address("usb:pci-0000-2").unwrap();
        assert_eq!(odd.bus, "pci-0000");
        assert_eq!(odd.ports, vec![2]);
    }

    #[test]
    fn malformed_addresses_are_rejected() {
        for bad in [
            "",
            "/dev/ttyACM0",
            "usb:",
            "usb:3",
            "usb:-1",
            "usb:3-",
            "usb:3-1.",
            "usb:3-1.x",
            "usb:3-256",
            "USB:3-1",
        ] {
            assert!(parse_address(bad).is_none(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn in_requests_are_whole_packets() {
        assert_eq!(in_request_len(32, 64), 64);
        assert_eq!(in_request_len(32, 512), 512);
        assert_eq!(in_request_len(512, 512), 512);
        assert_eq!(in_request_len(513, 512), 1024);
        assert_eq!(in_request_len(512, 64), 512);
        assert_eq!(in_request_len(0, 64), 64);
        assert_eq!(in_request_len(10, 0), 10);
    }

    #[test]
    fn write_timeout_grows_with_the_payload() {
        assert_eq!(write_timeout(512), Duration::from_millis(2000));
        assert_eq!(write_timeout(4096), Duration::from_millis(2004));
        assert_eq!(
            write_timeout(512 + 1024 * 1024),
            Duration::from_millis(3024)
        );
    }

    #[test]
    fn cancelled_transfers_are_timeouts() {
        assert_eq!(
            transfer_error(TransferError::Cancelled).kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            transfer_error(TransferError::Disconnected).kind(),
            io::ErrorKind::ConnectionAborted
        );
    }

    #[test]
    fn completions_keep_what_arrived() {
        let mut buffer = Buffer::new(64);
        buffer.extend_from_slice(&[1, 2, 3, 4]);
        let done = Completion {
            buffer,
            actual_len: 4,
            status: Ok(()),
        };
        assert_eq!(received(done, 2).unwrap(), vec![1, 2]);
        let timed_out = Completion {
            buffer: Buffer::new(64),
            actual_len: 0,
            status: Err(TransferError::Cancelled),
        };
        assert!(received(timed_out, 32).unwrap().is_empty());
        let stalled = Completion {
            buffer: Buffer::new(64),
            actual_len: 0,
            status: Err(TransferError::Stall),
        };
        assert!(received(stalled, 32).is_err());
    }

    /// Only enumerates (sysfs on Linux): nothing is opened, because no bus
    /// can be called `bezel-none`.
    #[test]
    fn opening_an_absent_device_fails_cleanly() {
        let err = UsbWire::open("usb:bezel-none-1.2", EPS).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        let err = UsbWire::open("/dev/ttyACM0", EPS).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
