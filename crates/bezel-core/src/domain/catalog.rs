//! The supported-device catalog and the rules that recognise each model's USB
//! endpoints. Every row comes from `docs/reverse-engineering/devices.md`.

use super::device::{Capabilities, DeviceModel, Family, ModelId, UsbId};
use super::geometry::{Orientation, Size};

/// What an endpoint does for its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointRole {
    /// The endpoint that receives frames and commands.
    Display,
    /// A companion micro-controller that only wakes the display SoC up
    /// (rev C screens: `CT21INCH`, `CT88INCH`, `USB7INCH`). Never written to.
    Wake,
}

/// How an endpoint's USB serial-number string must look for a rule to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialRule {
    /// Any serial number (or none).
    Any,
    /// Exactly this string.
    Exact(&'static str),
    /// Starts with this prefix.
    Prefix(&'static str),
}

impl SerialRule {
    fn accepts(self, serial: Option<&str>) -> bool {
        match self {
            SerialRule::Any => true,
            SerialRule::Exact(s) => serial == Some(s),
            SerialRule::Prefix(p) => serial.is_some_and(|s| s.starts_with(p)),
        }
    }
}

/// A recognition rule: an endpoint with this USB id and serial shape plays
/// `role` for one of `models`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointRule {
    /// USB vendor/product id.
    pub usb: UsbId,
    /// Serial-number constraint.
    pub serial: SerialRule,
    /// Role of the endpoint.
    pub role: EndpointRole,
    /// Protocol family of the screen.
    pub family: Family,
    /// Models this endpoint may belong to; the exact one is resolved by the
    /// device handshake when several share an identity.
    pub models: &'static [ModelId],
}

const fn caps(
    brightness: bool,
    device_rotation: bool,
    partial_update: bool,
    backplate_led: bool,
    storage: bool,
    video_playback: bool,
) -> Capabilities {
    Capabilities {
        brightness,
        device_rotation,
        partial_update,
        backplate_led,
        storage,
        video_playback,
    }
}

const fn model(
    id: &'static str,
    name: &'static str,
    diagonal_tenths: u16,
    panel: (u32, u32),
    native_orientation: Orientation,
    family: Family,
    capabilities: Capabilities,
) -> DeviceModel {
    DeviceModel {
        id: ModelId(id),
        name,
        diagonal_tenths,
        panel: Size::new(panel.0, panel.1),
        native_orientation,
        family,
        capabilities,
        hardware_validated: false,
    }
}

use Family::*;
use Orientation::*;

const SERIAL_BASIC: Capabilities = caps(true, true, true, false, false, false);
const REV_C: Capabilities = caps(true, false, true, false, true, true);
const TURING_USB: Capabilities = caps(true, false, false, false, true, true);
const WCH: Capabilities = caps(true, false, false, false, false, false);

/// Every model Bezel knows about. Rows from the Python reference and the vendor
/// app's device table (`docs/reverse-engineering/devices.md`).
#[rustfmt::skip]
pub const MODELS: &[DeviceModel] = &[
    // Rev A / B / D / WeAct (serial, rectangle updates).
    model("turing-3.5", "Turing Smart Screen 3.5\"", 35, (320, 480), Portrait, TuringRevA, SERIAL_BASIC),
    model("usbpcmonitor-3.5", "UsbPCMonitor 3.5\"", 35, (320, 480), Portrait, TuringRevA, SERIAL_BASIC),
    model("usbpcmonitor-5", "UsbPCMonitor 5\"", 50, (480, 800), Portrait, TuringRevA, SERIAL_BASIC),
    model("usbpcmonitor-7", "UsbPCMonitor 7\"", 70, (600, 1024), Portrait, TuringRevA, SERIAL_BASIC),
    model("xuanfang-3.5", "XuanFang 3.5\"", 35, (320, 480), Portrait, XuanFangRevB, caps(true, true, true, false, false, false)),
    model("xuanfang-3.5-flagship", "XuanFang 3.5\" Flagship", 35, (320, 480), Portrait, XuanFangRevB, caps(true, true, true, true, false, false)),
    model("kipye-qiye-3.5", "Kipye Qiye Smart Display 3.5\"", 35, (320, 480), Portrait, KipyeRevD, caps(true, true, true, false, false, false)),
    model("weact-fs-3.5", "WeAct Studio Display FS 3.5\"", 35, (320, 480), Portrait, WeAct, SERIAL_BASIC),
    model("weact-fs-0.96", "WeAct Studio Display FS 0.96\"", 9, (80, 160), Portrait, WeAct, SERIAL_BASIC),
    // Rev C: Linux/Android SoC gadgets behind a wake MCU (serial, run-list updates).
    model("turing-2.1", "Turing Smart Screen 2.1\"", 21, (480, 480), Landscape, TuringRevC, REV_C),
    model("turing-2.4", "Turing Smart Screen 2.4\"", 24, (240, 320), Portrait, TuringRevC, REV_C),
    model("turing-2.8", "Turing Smart Screen 2.8\"", 28, (480, 480), Landscape, TuringRevC, REV_C),
    model("turing-2.8-square", "Turing Smart Screen 2.8\" Square", 28, (320, 320), Landscape, TuringRevC, REV_C),
    model("turing-3.4", "Turing Smart Screen 3.4\" Square", 34, (480, 480), Landscape, TuringRevC, REV_C),
    model("turing-4", "Turing Smart Screen 4\" Square", 40, (720, 720), Landscape, TuringRevC, REV_C),
    model("turing-5", "Turing Smart Screen 5\"", 50, (480, 800), Landscape, TuringRevC, REV_C),
    model("turing-6.5", "Turing Smart Screen 6.5\"", 65, (720, 1568), Portrait, TuringRevC, REV_C),
    model("turing-6.8", "Turing Smart Screen 6.8\"", 68, (1080, 2320), Portrait, TuringRevC, REV_C),
    model("turing-8", "Turing Smart Screen 8\"", 80, (800, 1280), Portrait, TuringRevC, REV_C),
    model("turing-8.8", "Turing Smart Screen 8.8\"", 88, (480, 1920), ReversePortrait, TuringRevC, REV_C),
    // Turing/TURZX USB generation (VID 0x1CBE, full PNG/JPEG frames).
    model("turing-usb-1.6", "Turing 1.6\" Square (USB)", 16, (400, 400), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-2.1-round", "Turing 2.1\" Round (USB)", 21, (480, 480), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-2.8-round", "Turing 2.8\" Round (USB)", 28, (480, 480), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-2.8-square", "Turing 2.8\" Square (USB)", 28, (400, 400), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-2.88-round", "Turing 2.88\" Round (USB)", 29, (480, 480), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-3.4", "Turing 3.4\" Square (USB)", 34, (480, 480), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-3.5", "Turing 3.5\" (USB)", 35, (480, 640), Landscape, TuringUsb, TURING_USB),
    model("turing-usb-4", "Turing 4\" Square (USB)", 40, (720, 720), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-4.6", "Turing 4.6\" (USB)", 46, (320, 960), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-5.2", "Turing 5.2\" (USB)", 52, (720, 1280), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-6.2", "Turing 6.2\" (USB)", 62, (368, 960), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-6.2-v2", "Turing 6.2\" V2 (USB)", 62, (448, 1280), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-6.5", "Turing 6.5\" (USB)", 65, (720, 1472), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-6.5-b", "Turing 6.5\" B (USB)", 65, (720, 1568), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-6.8", "Turing 6.8\" (USB)", 68, (1080, 2320), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-6.8-b", "Turing 6.8\" B (USB)", 68, (1080, 2224), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-8", "Turing 8\" (USB)", 80, (800, 1280), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-8.8", "Turing 8.8\" V1.x (USB)", 88, (480, 1920), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-9.2", "Turing 9.2\" (USB)", 92, (462, 1920), ReversePortrait, TuringUsb, TURING_USB),
    model("turing-usb-12.3", "Turing 12.3\" (USB)", 123, (720, 1920), ReversePortrait, TuringUsb, TURING_USB),
    // WCH-based panels (VID 0x43A8, raw BGR888).
    model("wch-2.4-2.8-rect", "2.4\"/2.8\" (WCH)", 28, (240, 320), Portrait, Wch, WCH),
    model("wch-2.8-square", "2.8\" Square / G-GEAR aio (WCH)", 28, (320, 320), Portrait, Wch, WCH),
    model("wch-3.38", "3.38\" Bar (WCH)", 34, (180, 640), Portrait, Wch, WCH),
    model("wch-4.3", "4.3\" (WCH)", 43, (272, 480), Landscape, Wch, WCH),
];

const fn rule(
    vid: u16,
    pid: u16,
    serial: SerialRule,
    role: EndpointRole,
    family: Family,
    models: &'static [ModelId],
) -> EndpointRule {
    EndpointRule {
        usb: UsbId::new(vid, pid),
        serial,
        role,
        family,
        models,
    }
}

use EndpointRole::{Display, Wake};
use SerialRule::{Any, Exact, Prefix};

/// Recognition rules, most specific first: the first matching rule wins.
#[rustfmt::skip]
pub const RULES: &[EndpointRule] = &[
    // Rev C wake MCUs. 1a86:ca21/ca88 and the 5" `USB7INCH` are documented;
    // the other PIDs are the vendor table's CH552 codes (CA24, CA27, ...),
    // inferred to be the MCU's USB PID the same way CA88 is.
    rule(0x1a86, 0xca88, Any, Wake, TuringRevC, &[ModelId("turing-8.8")]),
    rule(0x1a86, 0xca21, Any, Wake, TuringRevC, &[ModelId("turing-2.1"), ModelId("turing-2.8"), ModelId("turing-8.8")]),
    rule(0x1a86, 0x5722, Exact("USB7INCH"), Wake, TuringRevC, &[ModelId("turing-5")]),
    rule(0x1a86, 0xca24, Any, Wake, TuringRevC, &[ModelId("turing-2.4")]),
    rule(0x1a86, 0xca27, Any, Wake, TuringRevC, &[ModelId("turing-2.8-square")]),
    rule(0x1a86, 0xca34, Any, Wake, TuringRevC, &[ModelId("turing-3.4")]),
    rule(0x1a86, 0xca40, Any, Wake, TuringRevC, &[ModelId("turing-4")]),
    rule(0x1a86, 0xca50, Any, Wake, TuringRevC, &[ModelId("turing-5")]),
    rule(0x1a86, 0xca65, Any, Wake, TuringRevC, &[ModelId("turing-6.5")]),
    rule(0x1a86, 0xca68, Any, Wake, TuringRevC, &[ModelId("turing-6.8")]),
    // Rev C SoC gadgets.
    rule(0x0525, 0xa4a7, Any, Display, TuringRevC, &[ModelId("turing-8.8")]),
    rule(0x1d6b, 0x0121, Any, Display, TuringRevC, &[ModelId("turing-2.1"), ModelId("turing-2.8")]),
    rule(0x1d6b, 0x0106, Any, Display, TuringRevC, &[ModelId("turing-5")]),
    rule(0x1d6b, 0x0124, Any, Display, TuringRevC, &[ModelId("turing-2.4")]),
    rule(0x125f, 0x7903, Any, Display, TuringRevC, &[ModelId("turing-2.4")]),
    rule(0x1d6b, 0x0127, Any, Display, TuringRevC, &[ModelId("turing-2.8-square")]),
    rule(0x1d6b, 0x0134, Any, Display, TuringRevC, &[ModelId("turing-3.4")]),
    rule(0x1d6b, 0xa040, Any, Display, TuringRevC, &[ModelId("turing-4")]),
    rule(0x1d6b, 0xa065, Any, Display, TuringRevC, &[ModelId("turing-6.5")]),
    rule(0x1d6b, 0xa068, Any, Display, TuringRevC, &[ModelId("turing-6.8")]),
    rule(0x1d6b, 0xa080, Any, Display, TuringRevC, &[ModelId("turing-8")]),
    // 1a86:5722 is shared by rev A, rev B and the sleeping rev C 5": serial decides.
    rule(0x1a86, 0x5722, Exact("2017-2-25"), Display, XuanFangRevB, &[ModelId("xuanfang-3.5"), ModelId("xuanfang-3.5-flagship")]),
    rule(0x1a86, 0x5722, Any, Display, TuringRevA, &[ModelId("turing-3.5"), ModelId("usbpcmonitor-3.5"), ModelId("usbpcmonitor-5"), ModelId("usbpcmonitor-7")]),
    rule(0x454d, 0x4e41, Any, Display, KipyeRevD, &[ModelId("kipye-qiye-3.5")]),
    rule(0x1a86, 0xfe0c, Prefix("AD"), Display, WeAct, &[ModelId("weact-fs-0.96")]),
    rule(0x1a86, 0xfe0c, Any, Display, WeAct, &[ModelId("weact-fs-3.5")]),
    // Turing/TURZX USB generation.
    rule(0x1cbe, 0x0005, Any, Display, TuringUsb, &[ModelId("turing-usb-1.6")]),
    rule(0x1cbe, 0x0016, Any, Display, TuringUsb, &[ModelId("turing-usb-2.8-square")]),
    rule(0x1cbe, 0x0021, Any, Display, TuringUsb, &[ModelId("turing-usb-2.1-round")]),
    rule(0x1cbe, 0x0028, Any, Display, TuringUsb, &[ModelId("turing-usb-2.8-round")]),
    rule(0x1cbe, 0x0034, Any, Display, TuringUsb, &[ModelId("turing-usb-3.4")]),
    rule(0x1cbe, 0x0035, Any, Display, TuringUsb, &[ModelId("turing-usb-3.5")]),
    rule(0x1cbe, 0x0040, Any, Display, TuringUsb, &[ModelId("turing-usb-4")]),
    rule(0x1cbe, 0x0046, Any, Display, TuringUsb, &[ModelId("turing-usb-4.6")]),
    rule(0x1cbe, 0x0050, Any, Display, TuringUsb, &[ModelId("turing-usb-5.2")]),
    rule(0x1cbe, 0x0068, Any, Display, TuringUsb, &[ModelId("turing-usb-6.8")]),
    rule(0x1cbe, 0x0080, Any, Display, TuringUsb, &[ModelId("turing-usb-8")]),
    rule(0x1cbe, 0x0088, Any, Display, TuringUsb, &[ModelId("turing-usb-8.8")]),
    rule(0x1cbe, 0x0092, Any, Display, TuringUsb, &[ModelId("turing-usb-9.2")]),
    rule(0x1cbe, 0x0123, Any, Display, TuringUsb, &[ModelId("turing-usb-12.3")]),
    rule(0x1cbe, 0x0288, Any, Display, TuringUsb, &[ModelId("turing-usb-2.88-round")]),
    rule(0x1cbe, 0xa062, Any, Display, TuringUsb, &[ModelId("turing-usb-6.2")]),
    rule(0x1cbe, 0xa065, Any, Display, TuringUsb, &[ModelId("turing-usb-6.5")]),
    rule(0x1cbe, 0xa068, Any, Display, TuringUsb, &[ModelId("turing-usb-6.8-b")]),
    rule(0x1cbe, 0xb062, Any, Display, TuringUsb, &[ModelId("turing-usb-6.2-v2")]),
    rule(0x1cbe, 0xb065, Any, Display, TuringUsb, &[ModelId("turing-usb-6.5-b")]),
    // WCH-based panels.
    rule(0x43a8, 0x0e5e, Any, Display, Wch, &[ModelId("wch-2.4-2.8-rect")]),
    rule(0x43a8, 0x0e61, Any, Display, Wch, &[ModelId("wch-3.38")]),
    rule(0x43a8, 0x0e64, Any, Display, Wch, &[ModelId("wch-2.8-square")]),
    rule(0x43a8, 0x0e6d, Any, Display, Wch, &[ModelId("wch-4.3")]),
];

/// Looks a model up by id.
pub fn model_by_id(id: ModelId) -> Option<&'static DeviceModel> {
    MODELS.iter().find(|m| m.id == id)
}

/// The first rule that recognises an endpoint with this USB id and serial.
pub fn classify(usb: UsbId, serial: Option<&str>) -> Option<&'static EndpointRule> {
    RULES
        .iter()
        .find(|r| r.usb == usb && r.serial.accepts(serial))
}

/// Every USB id the catalog recognises (used for udev rules and USB scans).
pub fn known_usb_ids() -> Vec<UsbId> {
    let mut ids: Vec<UsbId> = RULES.iter().map(|r| r.usb).collect();
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_points_at_catalog_models_of_its_family() {
        for r in RULES {
            assert!(!r.models.is_empty(), "rule {} has no model", r.usb);
            for id in r.models {
                let m = model_by_id(*id).expect("rule points at an unknown model");
                assert_eq!(m.family, r.family, "{id} family");
            }
        }
    }

    #[test]
    fn model_ids_are_unique_and_panels_portrait() {
        for (i, a) in MODELS.iter().enumerate() {
            assert_eq!(
                a.panel,
                a.panel.portrait(),
                "{} panel must be portrait",
                a.id
            );
            for b in &MODELS[i + 1..] {
                assert_ne!(a.id, b.id);
            }
        }
    }

    #[test]
    fn turing_88_endpoints() {
        let mcu = classify(UsbId::new(0x1a86, 0xca88), Some("CT88INCH")).unwrap();
        assert_eq!(mcu.role, EndpointRole::Wake);
        let soc = classify(UsbId::new(0x0525, 0xa4a7), None).unwrap();
        assert_eq!(soc.role, EndpointRole::Display);
        assert_eq!(soc.models, &[ModelId("turing-8.8")]);
        let m = model_by_id(ModelId("turing-8.8")).unwrap();
        assert_eq!(m.diagonal(), "8.8\"");
        assert_eq!(m.panel, Size::new(480, 1920));
    }

    #[test]
    fn shared_5722_is_split_by_serial() {
        let id = UsbId::new(0x1a86, 0x5722);
        assert_eq!(
            classify(id, Some("USB7INCH")).unwrap().role,
            EndpointRole::Wake
        );
        assert_eq!(
            classify(id, Some("2017-2-25")).unwrap().family,
            Family::XuanFangRevB
        );
        assert_eq!(
            classify(id, Some("USB35INCHIPSV2")).unwrap().family,
            Family::TuringRevA
        );
        assert_eq!(classify(id, None).unwrap().family, Family::TuringRevA);
    }

    #[test]
    fn weact_prefix_and_unknown_ids() {
        let id = UsbId::new(0x1a86, 0xfe0c);
        assert_eq!(
            classify(id, Some("AD123")).unwrap().models,
            &[ModelId("weact-fs-0.96")]
        );
        assert_eq!(
            classify(id, Some("AB123")).unwrap().models,
            &[ModelId("weact-fs-3.5")]
        );
        assert!(classify(UsbId::new(0x046d, 0x082d), None).is_none());
        let ids = known_usb_ids();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        assert!(ids.contains(&UsbId::new(0x0525, 0xa4a7)));
        assert_eq!(
            model_by_id(ModelId("turing-3.5")).unwrap().diagonal(),
            "3.5\""
        );
        assert_eq!(
            model_by_id(ModelId("usbpcmonitor-5")).unwrap().diagonal(),
            "5\""
        );
    }
}
