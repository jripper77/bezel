//! What a smart-screen model is: its USB identity, protocol family, panel
//! geometry and capabilities.

use super::geometry::{Orientation, Size};
use std::fmt;

/// A USB vendor/product id pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsbId {
    /// Vendor id.
    pub vid: u16,
    /// Product id.
    pub pid: u16,
}

impl UsbId {
    /// Builds a USB id.
    pub const fn new(vid: u16, pid: u16) -> Self {
        Self { vid, pid }
    }
}

impl fmt::Display for UsbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)
    }
}

/// The wire protocol a model speaks. Each family has its own specification in
/// `docs/reverse-engineering/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// Turing 3.5" and UsbPCMonitor 3.5"/5"/7": 6-byte packed-coordinate commands, RGB565 LE.
    TuringRevA,
    /// XuanFang 3.5" (rev B and flagship): 10-byte framed commands, RGB565 BE.
    XuanFangRevB,
    /// Turing 2.1"/2.8"/5"/8.8" UART generation: 250-byte `xx ef 69` blocks, BGR(A).
    TuringRevC,
    /// Kipye Qiye 3.5": ASCII-ish 4-byte commands, 64-byte data packets.
    KipyeRevD,
    /// WeAct Studio Display FS V1 (3.5" and 0.96"): LE16 commands ending in 0x0A.
    WeAct,
    /// Turing/TURZX USB generation (VID 0x1CBE): DES-CBC headers, PNG/JPEG frames.
    TuringUsb,
    /// WCH-based panels (VID 0x43A8): DES-ECB commands, raw BGR888 in 512-byte blocks.
    Wch,
}

impl Family {
    /// How the family is reached from the host.
    pub const fn transport(self) -> Transport {
        match self {
            Family::TuringUsb | Family::Wch => Transport::UsbBulk,
            _ => Transport::Serial,
        }
    }

    /// Stable machine name.
    pub const fn slug(self) -> &'static str {
        match self {
            Family::TuringRevA => "turing-rev-a",
            Family::XuanFangRevB => "xuanfang-rev-b",
            Family::TuringRevC => "turing-rev-c",
            Family::KipyeRevD => "kipye-rev-d",
            Family::WeAct => "weact",
            Family::TuringUsb => "turing-usb",
            Family::Wch => "wch",
        }
    }
}

/// How the host talks to a device endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    /// A CDC-ACM serial port (`/dev/ttyACM*`, `COMn`).
    Serial,
    /// Raw USB bulk endpoints (libusb/WinUSB).
    UsbBulk,
}

/// Stable identifier of a catalog model, e.g. `turing-8.8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(pub &'static str);

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.0)
    }
}

/// What a model can do. Unknown or unsupported features are `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// Backlight brightness can be set.
    pub brightness: bool,
    /// The device rotates frames itself (otherwise the host rotates pixels).
    pub device_rotation: bool,
    /// Sub-rectangles can be updated without resending the whole frame.
    pub partial_update: bool,
    /// Backplate RGB LEDs.
    pub backplate_led: bool,
    /// Files can be stored on the device (internal flash or SD card).
    pub storage: bool,
    /// Videos stored on the device can be played by the device itself.
    pub video_playback: bool,
}

/// One entry of the supported-device catalog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceModel {
    /// Stable identifier.
    pub id: ModelId,
    /// Marketing name.
    pub name: &'static str,
    /// Diagonal in hundredths of an inch (880 = 8.8", 96 = 0.96").
    pub diagonal_hundredths: u16,
    /// Panel size in portrait form (`width <= height`).
    pub panel: Size,
    /// Orientation in which the panel's framebuffer is laid out.
    pub native_orientation: Orientation,
    /// Wire protocol.
    pub family: Family,
    /// Feature set.
    pub capabilities: Capabilities,
    /// True when the model has been validated on real hardware by the project.
    pub hardware_validated: bool,
}

impl DeviceModel {
    /// Diagonal formatted like `8.8"`, `0.96"` or `5"`.
    pub fn diagonal(&self) -> String {
        let whole = self.diagonal_hundredths / 100;
        let fraction = self.diagonal_hundredths % 100;
        match fraction {
            0 => format!("{whole}\""),
            f if f % 10 == 0 => format!("{whole}.{}\"", f / 10),
            f => format!("{whole}.{f:02}\""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_id_displays_as_lsusb() {
        assert_eq!(UsbId::new(0x0525, 0xa4a7).to_string(), "0525:a4a7");
    }

    #[test]
    fn family_transport_and_slug() {
        assert_eq!(Family::TuringUsb.transport(), Transport::UsbBulk);
        assert_eq!(Family::Wch.transport(), Transport::UsbBulk);
        assert_eq!(Family::TuringRevC.transport(), Transport::Serial);
        assert_eq!(Family::TuringRevC.slug(), "turing-rev-c");
        assert_eq!(ModelId("turing-8.8").to_string(), "turing-8.8");
    }
}
