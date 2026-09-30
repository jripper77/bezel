//! JSON shapes sent to the webview (camelCase, the UI's contract).

use std::collections::BTreeMap;

use bezel_core::app::VideoState;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{Endpoint, Screen, ScreenState};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::Progress;
use bezel_core::domain::media::{MediaInfo, MediaTools, Mismatch};
use bezel_core::domain::sensor::{
    DisplayFormat, Quantities, Reading, SensorInfo, Snapshot, format_reading,
};
use bezel_core::domain::storage::{Capacity, FileEntry, NameError, Refusal, RemotePath};
use bezel_themes::dto::{SizeDto, ThemeDto};
use serde::Serialize;

use crate::library::ThemeEntry;
use crate::messages::{UiError, WarningDto};

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
    pub live_error: Option<UiError>,
    /// How the theme's video background reaches the live screen; `None`
    /// when nothing is live or the theme has no video.
    pub video: Option<LiveVideoDto>,
}

impl SampleDto {
    /// The readings of `snapshot`, formatted with the catalog's units.
    pub fn readings(snapshot: &Snapshot, quantities: &Quantities) -> BTreeMap<String, ReadingDto> {
        snapshot
            .iter()
            .map(|(key, reading)| {
                let quantity = quantities.quantity(key);
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
    pub warnings: Vec<WarningDto>,
}

/// An asset added to the theme.
#[derive(Debug, Clone, Serialize)]
pub struct AddedDto {
    /// Its reference.
    #[serde(rename = "ref")]
    pub reference: String,
}

/// How the theme's video background reaches the live screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveVideoDto {
    /// `notStarted`, `onDevice`, `missing` (the "Send to screen" call to
    /// action), `host`, `noConverter` or `noPlayback`.
    pub state: &'static str,
    /// The stored file it plays, or where it belongs when missing.
    pub path: Option<String>,
}

impl LiveVideoDto {
    /// The DTO of `state`; `None` for a theme without a video.
    pub fn of(state: &VideoState) -> Option<Self> {
        let (state, path) = match state {
            VideoState::NoVideo => return None,
            VideoState::NotStarted => ("notStarted", None),
            VideoState::OnDevice(path) => ("onDevice", Some(path.to_string())),
            VideoState::VideoMissing(missing) => ("missing", Some(missing.path.to_string())),
            VideoState::Host => ("host", None),
            VideoState::NoConverter { .. } => ("noConverter", None),
            VideoState::NoPlayback => ("noPlayback", None),
        };
        Some(Self { state, path })
    }
}

/// Size and use of one storage medium, bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapacityDto {
    /// Usable size.
    pub total: u64,
    /// In use.
    pub used: u64,
    /// Available for uploads.
    pub free: u64,
}

impl From<Capacity> for CapacityDto {
    fn from(c: Capacity) -> Self {
        Self {
            total: c.total,
            used: c.used,
            free: c.free,
        }
    }
}

/// A file stored on a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredFileDto {
    /// `internal/video/clip.mp4`: what the storage commands take.
    pub path: String,
    /// `internal` or `sd`.
    pub medium: &'static str,
    /// `image` or `video`.
    pub kind: &'static str,
    /// The file name.
    pub name: String,
    /// Bytes, when the screen reports them.
    pub size: Option<u64>,
}

impl StoredFileDto {
    /// A file at `path`.
    pub fn at(path: &RemotePath, size: Option<u64>) -> Self {
        Self {
            path: path.to_string(),
            medium: path.location.medium.slug(),
            kind: path.location.kind.slug(),
            name: path.name.to_string(),
            size,
        }
    }
}

impl From<&FileEntry> for StoredFileDto {
    fn from(e: &FileEntry) -> Self {
        Self::at(&e.path, e.size)
    }
}

/// One of the four folders of a screen and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderDto {
    /// `internal` or `sd`.
    pub medium: &'static str,
    /// `image` or `video`.
    pub kind: &'static str,
    /// The files, in the order the screen lists them.
    pub files: Vec<StoredFileDto>,
    /// Why the folder could not be listed (the other folders still are).
    pub error: Option<UiError>,
}

/// What the storage tab shows: capacity and the files of every folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDto {
    /// Internal flash.
    pub internal: CapacityDto,
    /// The memory card; `None` without one.
    pub card: Option<CapacityDto>,
    /// Internal folders, then the card's when a card is present.
    pub folders: Vec<FolderDto>,
}

/// Whether ffmpeg can convert videos, and how to install it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaToolsDto {
    /// Conversions can run.
    pub ready: bool,
    /// The version ffmpeg reported.
    pub version: Option<String>,
    /// Install commands for this system, most likely first.
    pub install_hints: Vec<String>,
    /// The ffmpeg chosen with Locate (else the one on `PATH` is used).
    pub configured: Option<String>,
    /// A file chosen with Locate that is not a usable ffmpeg (nothing was
    /// changed).
    pub rejected: Option<String>,
}

impl MediaToolsDto {
    /// The DTO of `tools`.
    pub fn of(tools: &MediaTools, configured: Option<String>) -> Self {
        let (ready, version, install_hints) = match tools {
            MediaTools::Ready { version } => (true, Some(version.clone()), Vec::new()),
            MediaTools::Missing { install_hints } => (false, None, install_hints.clone()),
        };
        Self {
            ready,
            version,
            install_hints,
            configured,
            rejected: None,
        }
    }
}

/// The conversion an upload runs first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionDto {
    /// Output width (the panel in its native orientation).
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// Clockwise quarter turns applied to the video first.
    pub quarter_turns: u8,
    /// Whether part of the picture is cut to fill the panel.
    pub cropped: bool,
}

/// An upload that passed its preflight: what the confirmation shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedDto {
    /// What `run_upload` takes.
    pub ticket: u64,
    /// The local file's name.
    pub source: String,
    /// Where it goes.
    pub target: StoredFileDto,
    /// Size of the local file.
    pub bytes: u64,
    /// Format of the local file (`MP4`, `PNG`…).
    pub format: String,
    /// Picture size of the local file.
    pub dimensions: Option<SizeDto>,
    /// The conversion, when one runs first.
    pub convert: Option<ConversionDto>,
    /// The stored file it replaces (needs the overwrite confirmation).
    pub replaces: Option<StoredFileDto>,
}

/// A way the file differs from what the screen accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MismatchDto {
    /// `format`, `codec`, `pixelFormat`, `bFrames`, `audio` or `resolution`.
    pub code: &'static str,
    /// What the file has (`GIF`, `1920x1080`).
    pub found: Option<String>,
    /// What the screen takes.
    pub expected: Option<String>,
}

fn size_text(size: bezel_core::domain::geometry::Size) -> String {
    format!("{}x{}", size.width, size.height)
}

impl From<&Mismatch> for MismatchDto {
    fn from(m: &Mismatch) -> Self {
        let (code, found, expected) = match m {
            Mismatch::Format { found, accepted } => {
                let names: Vec<String> = accepted.iter().map(ToString::to_string).collect();
                ("format", Some(found.to_string()), Some(names.join(", ")))
            }
            Mismatch::Codec(_) => ("codec", None, None),
            Mismatch::PixelFormat(_) => ("pixelFormat", None, None),
            Mismatch::BFrames => ("bFrames", None, None),
            Mismatch::Audio => ("audio", None, None),
            Mismatch::Resolution { expected, found } => (
                "resolution",
                found.map(size_text),
                Some(size_text(*expected)),
            ),
        };
        Self {
            code,
            found,
            expected,
        }
    }
}

/// Why an upload was refused before anything was converted, sent or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefusalDto {
    /// `invalidName`, `wrongExtension`, `wrongKind`, `wrongProfile`,
    /// `needsConverter`, `emptyFile`, `tooLarge`, `noCard` or `noSpace`.
    pub code: &'static str,
    /// The core's explanation, in English.
    pub message: String,
    /// The name the file would get.
    pub name: Option<String>,
    /// Extensions that fit.
    pub accepted: Vec<&'static str>,
    /// How the file differs from what the screen takes.
    pub mismatches: Vec<MismatchDto>,
    /// Bytes to store.
    pub bytes: Option<u64>,
    /// The size limit, or the free bytes when it does not fit.
    pub limit: Option<u64>,
    /// Files the user may choose to delete to make room (largest first).
    pub candidates: Vec<StoredFileDto>,
}

impl From<&Refusal> for RefusalDto {
    fn from(r: &Refusal) -> Self {
        let mut dto = Self {
            code: "",
            message: r.to_string(),
            name: None,
            accepted: Vec::new(),
            mismatches: Vec::new(),
            bytes: None,
            limit: None,
            candidates: Vec::new(),
        };
        let mismatches = |m: &[Mismatch]| m.iter().map(MismatchDto::from).collect();
        dto.code = match r {
            Refusal::InvalidName(e) => {
                dto.name = name_error_char(e);
                "invalidName"
            }
            Refusal::WrongExtension { name, accepted } => {
                dto.name = Some(name.to_string());
                dto.accepted = accepted.to_vec();
                "wrongExtension"
            }
            Refusal::WrongKind { .. } => "wrongKind",
            Refusal::WrongProfile(m) => {
                dto.mismatches = mismatches(m);
                "wrongProfile"
            }
            Refusal::NeedsConverter(m) => {
                dto.mismatches = mismatches(m);
                "needsConverter"
            }
            Refusal::EmptyFile => "emptyFile",
            Refusal::TooLarge { bytes, limit } => {
                (dto.bytes, dto.limit) = (Some(*bytes), Some(*limit));
                "tooLarge"
            }
            Refusal::NoCard => "noCard",
            Refusal::NoSpace {
                needed,
                free,
                candidates,
            } => {
                (dto.bytes, dto.limit) = (Some(*needed), Some(*free));
                dto.candidates = candidates.iter().map(StoredFileDto::from).collect();
                "noSpace"
            }
        };
        dto
    }
}

/// The character that made a name invalid, when one did.
fn name_error_char(e: &NameError) -> Option<String> {
    match e {
        NameError::Forbidden(c) => Some(c.to_string()),
        _ => None,
    }
}

/// The preflight's answer: ready to confirm, or refused with the reason.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PrepareDto {
    /// Passed: show the summary and ask.
    Ready(PreparedDto),
    /// Refused: explain it inline.
    Refused(RefusalDto),
}

/// How an upload ended (errors other than a cancel reject the command).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum JobDto {
    /// Stored and verified.
    #[serde(rename_all = "camelCase")]
    Done {
        /// The stored file.
        file: StoredFileDto,
        /// Converted first.
        converted: bool,
    },
    /// Cancelled by the user.
    #[serde(rename_all = "camelCase")]
    Cancelled {
        /// Where the file was going.
        path: String,
        /// Bytes of an incomplete file left on the screen (offer a
        /// confirmed delete), `None` when nothing is left.
        partial: Option<u64>,
    },
}

/// One progress report of a running job (the `storage-progress` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    /// `convert`, `upload` or `verify`.
    pub phase: &'static str,
    /// Units done (ms of video, bytes or checks).
    pub done: u64,
    /// Units in the phase; 0 when unknown.
    pub total: u64,
}

impl From<Progress> for ProgressDto {
    fn from(p: Progress) -> Self {
        Self {
            phase: p.phase.slug(),
            done: p.done,
            total: p.total,
        }
    }
}

/// The format and picture size of a probed file, for the summary.
pub fn media_summary(media: &MediaInfo) -> (String, Option<SizeDto>) {
    (
        media.format.to_string(),
        media.dimensions.map(|d| SizeDto {
            width: d.width,
            height: d.height,
        }),
    )
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
        use bezel_core::domain::sensor::{Category, Quantity, SensorKey};
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
