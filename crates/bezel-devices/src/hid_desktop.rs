//! The HID interface of a Turing USB panel in the vendor's desktop mode
//! (`docs/reverse-engineering/protocol-turing-usb.md` section 10), not
//! validated on hardware (D-2026-09-30-release-polish-8).
//!
//! Such a panel enumerates as 1a86:ad10-ad13. Listing it is read-only:
//! [`endpoints`] only enumerates (sysfs on Linux, the HID class on Windows)
//! and opens nothing. Reports go out only through [`SystemHid`], whose
//! methods the core calls with the proof of a confirmed switch. Each report
//! is 64 bytes behind report id 0, so 65 bytes reach the HID stack.

use std::ffi::CString;
use std::io;
use std::time::Duration;

use bezel_core::domain::catalog;
use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{
    DesktopModePanel, DeviceAddress, Endpoint, MonitorModeConfirmed,
};
use bezel_core::ports::DesktopModeHid;
use bezel_core::{BezelError, Result};
use hidapi::{HidApi, HidDevice, HidError};

use crate::connector::access_error;
use crate::wire::Wire;

/// Prefix of the discovery address of a HID interface; the rest is the
/// HID stack's path (`/dev/hidraw3`, `\\?\HID#VID_1A86&PID_AD11...`).
pub const ADDRESS_PREFIX: &str = "hid:";

/// Payload bytes of every report, without the report id.
pub const REPORT_LEN: usize = 64;

/// The report id: the panel uses unnumbered reports.
pub const REPORT_ID: u8 = 0;

/// How long the vendor app waits for the answer to the model query.
pub const MODEL_REPLY_TIMEOUT: Duration = Duration::from_millis(1000);

/// Payload of the model query.
const MODEL_QUERY: [u8; 3] = [0xaa, 0x55, 0x33];

/// The ASCII `5f3759df` both back-to-monitor-mode reports carry.
const MONITOR_MODE_MAGIC: &[u8; 8] = b"5f3759df";

/// Where the model byte sits in the answer, as the HID stack returns it
/// (without the report id).
const MODEL_BYTE_INDEX: usize = 2;

/// Stale input reports dropped at most before the model query.
const DRAIN_READS: usize = 8;

/// A report as it goes to the HID stack: the report id, then `payload`
/// zero-padded to [`REPORT_LEN`] bytes (a longer payload is cut there).
pub fn report(payload: &[u8]) -> Vec<u8> {
    let mut wire = vec![0; REPORT_LEN + 1];
    wire[0] = REPORT_ID;
    let len = payload.len().min(REPORT_LEN);
    wire[1..=len].copy_from_slice(&payload[..len]);
    wire
}

/// The model query: `aa 55 33` and 61 zeros.
pub fn model_query() -> Vec<u8> {
    report(&MODEL_QUERY)
}

/// The two back-to-monitor-mode reports, in order: `5f3759df` in ASCII,
/// then a zero followed by `5f3759df` (the spec keeps both exactly as the
/// vendor sends them; why there are two is an open question).
pub fn back_to_monitor_reports() -> [Vec<u8>; 2] {
    let mut second = vec![0];
    second.extend_from_slice(MONITOR_MODE_MAGIC);
    [report(MONITOR_MODE_MAGIC), report(&second)]
}

/// The model byte of an answer to the model query (`80` 8", `88` 8.8", `50`
/// 5.2"); `None` for no answer or one too short to carry it.
pub fn model_byte(answer: &[u8]) -> Option<u8> {
    answer.get(MODEL_BYTE_INDEX).copied()
}

/// An answer that names `model_byte`, for simulated panels: only the model
/// byte of a real answer is known, the rest is zeros here.
pub fn simulated_answer(model_byte: u8) -> Vec<u8> {
    let mut answer = vec![0; REPORT_LEN];
    answer[MODEL_BYTE_INDEX] = model_byte;
    answer
}

/// Sends the model query over `wire` and reads one answer: the model byte,
/// or `None` when nothing came within [`MODEL_REPLY_TIMEOUT`].
pub fn ask_model(wire: &mut dyn Wire) -> io::Result<Option<u8>> {
    wire.discard_input()?;
    wire.send(&model_query())?;
    let answer = wire.receive(REPORT_LEN, MODEL_REPLY_TIMEOUT)?;
    Ok(model_byte(&answer))
}

/// Sends the two back-to-monitor-mode reports over `wire`.
pub fn switch_back(wire: &mut dyn Wire) -> io::Result<()> {
    for report in back_to_monitor_reports() {
        wire.send(&report)?;
    }
    Ok(())
}

/// Refuses to talk to `usb` unless it is a panel in desktop mode: a HID
/// path can lead to another device once the panel is unplugged.
pub fn ensure_desktop_mode(address: &str, usb: UsbId) -> Result<()> {
    if catalog::is_desktop_mode(usb) {
        return Ok(());
    }
    Err(BezelError::InvalidInput(format!(
        "{address} is {usb}, not a panel in desktop mode; nothing was sent"
    )))
}

/// The fields of one enumerated HID interface Bezel reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidInterface {
    /// The HID stack's path.
    pub path: String,
    /// USB identity.
    pub usb: UsbId,
    /// USB serial-number string.
    pub serial_number: Option<String>,
    /// USB manufacturer string.
    pub manufacturer: Option<String>,
    /// USB product string.
    pub product: Option<String>,
}

/// The endpoint of `interface` when it belongs to a panel in desktop mode.
/// The HID stack reports no USB location.
pub fn desktop_mode_endpoint(interface: HidInterface) -> Option<Endpoint> {
    if !catalog::is_desktop_mode(interface.usb) {
        return None;
    }
    Some(Endpoint {
        address: DeviceAddress(format!("{ADDRESS_PREFIX}{}", interface.path)),
        transport: Transport::Hid,
        usb: interface.usb,
        serial_number: non_empty(interface.serial_number),
        manufacturer: non_empty(interface.manufacturer),
        product: non_empty(interface.product),
        location: None,
    })
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

fn interface_of(device: &hidapi::DeviceInfo) -> Option<HidInterface> {
    let path = device.path().to_str().ok()?.to_string();
    Some(HidInterface {
        path,
        usb: UsbId::new(device.vendor_id(), device.product_id()),
        serial_number: device.serial_number().map(str::to_string),
        manufacturer: device.manufacturer_string().map(str::to_string),
        product: device.product_string().map(str::to_string),
    })
}

fn hid_io(e: HidError) -> io::Error {
    match e {
        HidError::IoError { error } => error,
        other => io::Error::other(other.to_string()),
    }
}

/// The HID interfaces of the panels in desktop mode. Read-only: the HID
/// stack's enumeration opens no device for writing and sends nothing.
pub fn endpoints() -> io::Result<Vec<Endpoint>> {
    let api = HidApi::new().map_err(hid_io)?;
    Ok(api
        .device_list()
        .filter_map(interface_of)
        .filter_map(desktop_mode_endpoint)
        .collect())
}

/// One opened HID interface behind the [`Wire`] trait.
struct HidWire {
    device: HidDevice,
}

impl Wire for HidWire {
    /// Writes one report. The count the HID stack returns is not checked:
    /// Windows reports 0 for a write that completed at once.
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let written = self.device.write(bytes).map_err(hid_io)?;
        tracing::debug!(bytes = bytes.len(), written, "HID report written");
        Ok(())
    }

    fn receive(&mut self, max: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        let mut buffer = vec![0; max.max(1)];
        let millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        let read = self
            .device
            .read_timeout(&mut buffer, millis)
            .map_err(hid_io)?;
        buffer.truncate(read.min(max));
        Ok(buffer)
    }

    fn discard_input(&mut self) -> io::Result<()> {
        let mut buffer = [0; REPORT_LEN];
        for _ in 0..DRAIN_READS {
            if self.device.read_timeout(&mut buffer, 0).map_err(hid_io)? == 0 {
                break;
            }
        }
        Ok(())
    }
}

/// The host's HID stack (hidraw on Linux, the HID class driver on Windows).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHid;

impl SystemHid {
    /// Opens the panel's HID interface after checking that its address
    /// still leads to a panel in desktop mode.
    fn open(panel: &DesktopModePanel) -> Result<HidWire> {
        let address = &panel.address().0;
        let path = address
            .strip_prefix(ADDRESS_PREFIX)
            .and_then(|p| CString::new(p).ok())
            .ok_or_else(|| BezelError::InvalidInput(format!("not a HID address: {address}")))?;
        let failed = |e: HidError| access_error(address, &hid_io(e));
        let api = HidApi::new().map_err(failed)?;
        let device = api.open_path(&path).map_err(failed)?;
        let info = device.get_device_info().map_err(failed)?;
        ensure_desktop_mode(address, UsbId::new(info.vendor_id(), info.product_id()))?;
        Ok(HidWire { device })
    }
}

impl DesktopModeHid for SystemHid {
    fn query_model(
        &self,
        panel: &DesktopModePanel,
        _confirmed: &MonitorModeConfirmed,
    ) -> Result<Option<u8>> {
        let mut wire = Self::open(panel)?;
        ask_model(&mut wire).map_err(|e| access_error(&panel.address().0, &e))
    }

    fn back_to_monitor(
        &self,
        panel: &DesktopModePanel,
        _confirmed: MonitorModeConfirmed,
    ) -> Result<()> {
        let mut wire = Self::open(panel)?;
        switch_back(&mut wire).map_err(|e| access_error(&panel.address().0, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::discovery::desktop_mode_panels;
    use bezel_core::domain::screen::Confirm;

    /// Hex of `bytes`, space-separated, as the spec writes vectors.
    fn hex(bytes: &[u8]) -> String {
        let text: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
        text.join(" ")
    }

    /// `head` then zeros up to 65 bytes: a golden report on the wire.
    fn golden(head: &str) -> String {
        let mut bytes: Vec<u8> = head
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect();
        bytes.resize(REPORT_LEN + 1, 0);
        hex(&bytes)
    }

    fn interface(path: &str, pid: u16) -> HidInterface {
        HidInterface {
            path: path.to_string(),
            usb: UsbId::new(0x1a86, pid),
            serial_number: Some(String::new()),
            manufacturer: Some("Turing".to_string()),
            product: None,
        }
    }

    fn panel() -> DesktopModePanel {
        let hid = desktop_mode_endpoint(interface("/dev/hidraw7", 0xad11)).unwrap();
        desktop_mode_panels(&[hid]).remove(0)
    }

    #[test]
    fn model_query_matches_the_spec() {
        let query = model_query();
        assert_eq!(query.len(), 65, "report id 0 plus 64 bytes");
        assert_eq!(hex(&query), golden("00 aa 55 33"));
    }

    #[test]
    fn back_to_monitor_reports_match_the_spec() {
        let [first, second] = back_to_monitor_reports();
        assert_eq!(hex(&first), golden("00 35 66 33 37 35 39 64 66"));
        assert_eq!(hex(&second), golden("00 00 35 66 33 37 35 39 64 66"));
        assert_eq!(&first[1..9], b"5f3759df");
        assert_eq!(&second[2..10], b"5f3759df");
    }

    #[test]
    fn reports_are_padded_and_cut_to_64_bytes() {
        assert_eq!(report(&[]), vec![0; 65]);
        let long = report(&[0xff; 80]);
        assert_eq!(long.len(), 65);
        assert_eq!(long[0], REPORT_ID);
        assert!(long[1..].iter().all(|&b| b == 0xff));
    }

    #[test]
    fn the_model_byte_is_the_third_byte_of_the_answer() {
        assert_eq!(model_byte(&[0xaa, 0x55, 0x88, 0, 0]), Some(0x88));
        assert_eq!(model_byte(&simulated_answer(0x50)), Some(0x50));
        assert_eq!(simulated_answer(0x80).len(), REPORT_LEN);
        assert_eq!(model_byte(&[0xaa, 0x55]), None);
        assert_eq!(model_byte(&[]), None);
    }

    #[test]
    fn asking_the_model_sends_one_query_and_reads_one_answer() {
        let mut wire = ScriptedWire::with_replies([simulated_answer(0x88)]);
        assert_eq!(ask_model(&mut wire).unwrap(), Some(0x88));
        assert_eq!(wire.sent, vec![model_query()]);
        assert_eq!(wire.discards, 1, "stale reports are dropped first");

        let mut silent = ScriptedWire::default();
        assert_eq!(ask_model(&mut silent).unwrap(), None, "no answer, no guess");
    }

    #[test]
    fn switching_back_sends_exactly_the_two_reports() {
        let mut wire = ScriptedWire::default();
        switch_back(&mut wire).unwrap();
        assert_eq!(wire.sent, back_to_monitor_reports().to_vec());
        assert_eq!(wire.discards, 0);
    }

    #[test]
    fn only_desktop_mode_interfaces_become_endpoints() {
        let e = desktop_mode_endpoint(interface("/dev/hidraw7", 0xad11)).unwrap();
        assert_eq!(e.address.0, "hid:/dev/hidraw7");
        assert_eq!(e.transport, Transport::Hid);
        assert_eq!(e.serial_number, None, "blank strings are dropped");
        assert_eq!(e.manufacturer.as_deref(), Some("Turing"));
        assert_eq!(e.location, None);
        for pid in [0xad10, 0xad12, 0xad13] {
            assert!(desktop_mode_endpoint(interface("/dev/hidraw1", pid)).is_some());
        }
        assert!(desktop_mode_endpoint(interface("/dev/hidraw0", 0x5722)).is_none());
        let keyboard = HidInterface {
            usb: UsbId::new(0x046d, 0xc52b),
            ..interface("/dev/hidraw2", 0)
        };
        assert!(desktop_mode_endpoint(keyboard).is_none());
    }

    #[test]
    fn a_path_leading_elsewhere_is_refused() {
        assert!(ensure_desktop_mode("hid:/dev/hidraw7", UsbId::new(0x1a86, 0xad11)).is_ok());
        let err = ensure_desktop_mode("hid:/dev/hidraw7", UsbId::new(0x046d, 0xc52b)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid input: hid:/dev/hidraw7 is 046d:c52b, not a panel in desktop mode; \
             nothing was sent"
        );
    }

    #[test]
    fn hid_errors_keep_their_kind() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(
            hid_io(HidError::IoError { error: denied }).kind(),
            io::ErrorKind::PermissionDenied
        );
        let other = hid_io(HidError::HidApiError {
            message: "failed to open device with path /dev/hidraw7: Permission denied".into(),
        });
        assert!(matches!(
            access_error("hid:/dev/hidraw7", &other),
            BezelError::AccessDenied { .. }
        ));
    }

    /// Only enumerates (sysfs on Linux): no HID device is opened.
    #[test]
    fn enumeration_is_read_only_and_finds_no_stray_devices() {
        let found = endpoints().expect("the HID stack enumerates");
        for e in &found {
            assert!(catalog::is_desktop_mode(e.usb), "{}", e.usb);
            assert!(e.address.0.starts_with(ADDRESS_PREFIX));
        }
    }

    /// A bad address fails before the HID stack is asked for anything.
    #[test]
    fn a_malformed_address_is_refused_without_opening() {
        let confirmed = MonitorModeConfirmed::require(Confirm::Yes).unwrap();
        let mut bad = panel();
        bad.hid.address = DeviceAddress("/dev/hidraw7".into());
        let err = SystemHid.query_model(&bad, &confirmed).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid input: not a HID address: /dev/hidraw7"
        );
        bad.hid.address = DeviceAddress("hid:/dev/hid\0raw7".into());
        let err = SystemHid.back_to_monitor(&bad, confirmed).unwrap_err();
        assert!(matches!(err, BezelError::InvalidInput(_)), "{err}");
    }
}
