//! JSON shapes sent to the webview (camelCase, the UI's contract).

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{Endpoint, Screen, ScreenState};
use serde::Serialize;

/// One screen as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDto {
    /// Stable key for the UI: the display (or wake) endpoint address.
    pub key: String,
    /// `awake` or `asleep`.
    pub state: &'static str,
    /// Protocol family slug.
    pub family: &'static str,
    /// Candidate models (one when known).
    pub models: Vec<ModelDto>,
    /// Frame endpoint.
    pub display: Option<EndpointDto>,
    /// Wake-only micro-controller.
    pub wake: Option<EndpointDto>,
}

/// A catalog model.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    /// Model id.
    pub id: &'static str,
    /// Marketing name.
    pub name: &'static str,
    /// Diagonal, e.g. `8.8"`.
    pub diagonal: String,
    /// Panel width in portrait form.
    pub width: u32,
    /// Panel height in portrait form.
    pub height: u32,
    /// Capabilities the UI can offer.
    pub capabilities: CapabilitiesDto,
    /// Validated on real hardware by the project.
    pub hardware_validated: bool,
}

/// Capability flags.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesDto {
    /// Brightness control.
    pub brightness: bool,
    /// Device-side rotation.
    pub device_rotation: bool,
    /// Partial updates.
    pub partial_update: bool,
    /// Backplate LEDs.
    pub backplate_led: bool,
    /// On-device storage.
    pub storage: bool,
    /// On-device video playback.
    pub video_playback: bool,
}

/// One USB endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDto {
    /// Port name or USB path.
    pub address: String,
    /// `vid:pid`.
    pub usb: String,
    /// USB serial number.
    pub serial: Option<String>,
    /// USB manufacturer string.
    pub manufacturer: Option<String>,
    /// USB product string.
    pub product: Option<String>,
    /// USB location (`bus-port.port`).
    pub location: Option<String>,
}

impl From<&DeviceModel> for ModelDto {
    fn from(m: &DeviceModel) -> Self {
        let c = m.capabilities;
        Self {
            id: m.id.0,
            name: m.name,
            diagonal: m.diagonal(),
            width: m.panel.width,
            height: m.panel.height,
            capabilities: CapabilitiesDto {
                brightness: c.brightness,
                device_rotation: c.device_rotation,
                partial_update: c.partial_update,
                backplate_led: c.backplate_led,
                storage: c.storage,
                video_playback: c.video_playback,
            },
            hardware_validated: m.hardware_validated,
        }
    }
}

impl From<&Endpoint> for EndpointDto {
    fn from(e: &Endpoint) -> Self {
        Self {
            address: e.address.0.clone(),
            usb: e.usb.to_string(),
            serial: e.serial_number.clone(),
            manufacturer: e.manufacturer.clone(),
            product: e.product.clone(),
            location: e.location.as_ref().map(ToString::to_string),
        }
    }
}

impl From<&Screen> for ScreenDto {
    fn from(s: &Screen) -> Self {
        Self {
            key: s.address().map(|a| a.0.clone()).unwrap_or_default(),
            state: match s.state() {
                ScreenState::Awake => "awake",
                ScreenState::Asleep => "asleep",
            },
            family: s.family.slug(),
            models: s.candidates.iter().map(|m| ModelDto::from(*m)).collect(),
            display: s.display.as_ref().map(EndpointDto::from),
            wake: s.wake.as_ref().map(EndpointDto::from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::app::discover_screens;
    use bezel_devices::FakeBus;

    #[test]
    fn serializes_camel_case() {
        let screens = discover_screens(&FakeBus::turing_88()).unwrap();
        let json = serde_json::to_value(ScreenDto::from(&screens[0])).unwrap();
        assert_eq!(json["key"], "/dev/ttyACM1");
        assert_eq!(json["models"][0]["hardwareValidated"], false);
        assert_eq!(json["models"][0]["capabilities"]["videoPlayback"], true);
        assert_eq!(json["wake"]["serial"], "CT88INCH");
    }
}
