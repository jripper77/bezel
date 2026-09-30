//! JSON shapes sent to the webview (camelCase, the UI's contract).

use std::collections::BTreeMap;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{Endpoint, Screen, ScreenState};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::sensor::{
    DisplayFormat, Quantities, Quantity, Reading, SensorInfo, Snapshot, format_reading,
};
use bezel_themes::dto::{SizeDto, ThemeDto};
use serde::Serialize;

use crate::library::ThemeEntry;

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

/// A sensor of the catalog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorDto {
    /// Stable key.
    pub key: String,
    /// Category slug.
    pub category: &'static str,
    /// English label.
    pub label: String,
    /// Quantity slug.
    pub quantity: &'static str,
    /// Where the value comes from.
    pub source: String,
}

impl From<&SensorInfo> for SensorDto {
    fn from(s: &SensorInfo) -> Self {
        Self {
            key: s.key.to_string(),
            category: s.category.slug(),
            label: s.label.clone(),
            quantity: s.quantity.slug(),
            source: s.source.clone(),
        }
    }
}

/// One reading: `display` always, `value` for numbers, `unavailable` with
/// the reason when the sensor cannot be read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingDto {
    /// The number, in the sensor's quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Formatted text (`63°C`, `4.72 GHz`, `—`).
    pub display: String,
    /// Why there is no value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

/// The latest sample and the live screen's state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleDto {
    /// Time the sample took, milliseconds.
    pub sample_millis: f64,
    /// Readings by key.
    pub readings: BTreeMap<String, ReadingDto>,
    /// Key of the screen showing the theme.
    pub live: Option<String>,
    /// Why the live screen stopped.
    pub live_error: Option<String>,
}

impl SampleDto {
    /// The readings of `snapshot`, formatted with the catalog's units.
    pub fn readings(snapshot: &Snapshot, quantities: &Quantities) -> BTreeMap<String, ReadingDto> {
        snapshot
            .iter()
            .map(|(key, reading)| {
                let quantity = quantities.get(key).unwrap_or(Quantity::Number);
                let dto = ReadingDto {
                    value: reading.value(),
                    display: format_reading(reading, quantity, DisplayFormat::default()),
                    unavailable: match reading {
                        Reading::Unavailable(why) => Some(why.clone()),
                        _ => None,
                    },
                };
                (key.to_string(), dto)
            })
            .collect()
    }
}

/// The theme being edited.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    /// The theme.
    pub theme: ThemeDto,
    /// Where it lives.
    pub location: Option<String>,
}

/// A theme of the library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeEntryDto {
    /// Display name.
    pub name: String,
    /// Where it lives.
    pub location: String,
    /// Canvas size.
    pub canvas: SizeDto,
    /// `portrait`, `reverse-portrait`, `landscape` or `reverse-landscape`.
    pub orientation: &'static str,
    /// Ships with the app.
    pub bundled: bool,
}

impl From<&ThemeEntry> for ThemeEntryDto {
    fn from(e: &ThemeEntry) -> Self {
        Self {
            name: e.theme.name.clone(),
            location: e.location.0.clone(),
            canvas: SizeDto {
                width: e.theme.canvas.width,
                height: e.theme.canvas.height,
            },
            orientation: orientation_slug(e.theme.orientation),
            bundled: e.bundled,
        }
    }
}

/// An orientation as `theme.json` and the UI spell it.
pub fn orientation_slug(orientation: Orientation) -> &'static str {
    match orientation {
        Orientation::Portrait => "portrait",
        Orientation::ReversePortrait => "reverse-portrait",
        Orientation::Landscape => "landscape",
        Orientation::ReverseLandscape => "reverse-landscape",
    }
}

/// The orientation spelled `slug` (see [`orientation_slug`]).
pub fn parse_orientation(slug: &str) -> Option<Orientation> {
    Orientation::ALL
        .into_iter()
        .find(|o| orientation_slug(*o) == slug)
}

/// An asset of the edited theme.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDto {
    /// Reference used in the theme.
    #[serde(rename = "ref")]
    pub reference: String,
    /// `image`, `font`, `video` or `other`.
    pub kind: &'static str,
    /// Small PNG preview of images.
    pub data_url: Option<String>,
}

/// Where a theme was saved.
#[derive(Debug, Clone, Serialize)]
pub struct SavedDto {
    /// The location.
    pub location: String,
}

/// A theme imported from another app.
#[derive(Debug, Clone, Serialize)]
pub struct ImportedDto {
    /// The converted theme (now the edited one).
    pub theme: ThemeDto,
    /// What had no equivalent.
    pub warnings: Vec<String>,
}

/// An asset added to the theme.
#[derive(Debug, Clone, Serialize)]
pub struct AddedDto {
    /// Its reference.
    #[serde(rename = "ref")]
    pub reference: String,
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
        assert_eq!(json["models"][0]["hardwareValidated"], true);
        assert_eq!(json["models"][0]["capabilities"]["videoPlayback"], true);
        assert_eq!(json["wake"]["serial"], "CT88INCH");
    }

    #[test]
    fn orientations_are_spelled_like_theme_json() {
        use bezel_core::domain::geometry::Size;
        use bezel_core::domain::theme::Theme;
        for o in Orientation::ALL {
            let dto = ThemeDto::from(&Theme::blank("T", Size::new(480, 1920), o));
            assert_eq!(dto.orientation, orientation_slug(o));
            assert_eq!(parse_orientation(orientation_slug(o)), Some(o));
        }
        assert_eq!(parse_orientation("sideways"), None);
    }

    #[test]
    fn readings_use_the_catalog_units() {
        use bezel_core::domain::sensor::{Category, SensorKey};
        let key = SensorKey::new("hwmon.nvme0.composite").unwrap();
        let other = SensorKey::new("x.y").unwrap();
        let catalog = [SensorInfo {
            key: key.clone(),
            category: Category::Disk,
            label: "NVMe".into(),
            quantity: Quantity::Celsius,
            source: "hwmon".into(),
        }];
        let mut snapshot = Snapshot::default();
        snapshot.insert(key, Reading::Value(40.2));
        snapshot.insert(other, Reading::Unavailable("gone".into()));
        let readings = SampleDto::readings(&snapshot, &Quantities::from_catalog(&catalog));
        assert_eq!(readings["hwmon.nvme0.composite"].display, "40°C");
        assert_eq!(readings["x.y"].unavailable.as_deref(), Some("gone"));
        let json = serde_json::to_value(SensorDto::from(&catalog[0])).unwrap();
        assert_eq!(
            (json["category"].as_str(), json["quantity"].as_str()),
            (Some("disk"), Some("celsius"))
        );
        let asset = serde_json::to_value(AssetDto {
            reference: "assets/a.png".into(),
            kind: "image",
            data_url: None,
        })
        .unwrap();
        assert_eq!(asset["ref"], "assets/a.png");
    }
}
