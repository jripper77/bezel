//! JSON shapes sent to the webview (camelCase, the UI's contract).

use std::collections::BTreeMap;

use bezel_core::app::VideoState;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{
    DesktopModePanel, Discovery, Endpoint, MonitorModeSwitch, Screen, ScreenState,
};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::gifs::{CollectedGif, GifItem, GifPage, GifQuery};
use bezel_core::domain::job::Progress;
use bezel_core::domain::media::{MediaInfo, MediaTools, Mismatch};
use bezel_core::domain::sensor::{
    DisplayFormat, Quantities, Reading, SensorInfo, Snapshot, format_reading,
};
use bezel_core::domain::standby::{Offer, SleepMinutes, Standby, StandbyOption, Unavailable};
use bezel_core::domain::storage::{Capacity, FileEntry, NameError, Refusal, RemotePath};
use bezel_themes::dto::{SizeDto, ThemeDto};
use serde::Serialize;

use crate::library::{ThemeEntry, fitting_models, made_for};
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
    /// Bezel can restart it without a replug (a rev C screen with its MCU,
    /// D-2026-09-30-release-polish-13).
    pub restartable: bool,
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
            restartable: s.restartable(),
        }
    }
}

/// A Turing USB panel the vendor app left in desktop mode: listed, and
/// switched back to USB monitor mode on request (not validated on hardware).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPanelDto {
    /// Its HID interface's address: what `leave_desktop_mode` takes.
    pub key: String,
    /// `vid:pid` of the HID interface.
    pub usb: String,
    /// The family it belongs to once back in USB monitor mode.
    pub family: &'static str,
    /// The models it may be.
    pub models: Vec<ModelDto>,
    /// Always `false`: the switch was not validated on hardware
    /// (D-2026-09-30-release-polish-8).
    pub hardware_validated: bool,
}

impl From<&DesktopModePanel> for DesktopPanelDto {
    fn from(p: &DesktopModePanel) -> Self {
        Self {
            key: p.address().0.clone(),
            usb: p.hid.usb.to_string(),
            family: DesktopModePanel::FAMILY.slug(),
            models: p.candidates.iter().map(|m| ModelDto::from(*m)).collect(),
            hardware_validated: DesktopModePanel::HARDWARE_VALIDATED,
        }
    }
}

/// What one enumeration found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesDto {
    /// The screens.
    pub screens: Vec<ScreenDto>,
    /// The panels in desktop mode.
    pub desktop_mode: Vec<DesktopPanelDto>,
}

impl From<&Discovery> for DevicesDto {
    fn from(d: &Discovery) -> Self {
        Self {
            screens: d.screens.iter().map(ScreenDto::from).collect(),
            desktop_mode: d.desktop_mode.iter().map(DesktopPanelDto::from).collect(),
        }
    }
}

/// A screen restarted without a replug (D-2026-09-30-release-polish-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartedDto {
    /// Its key now: the display comes back under a new address.
    pub key: String,
    /// It shows the theme live again (it was live before the restart).
    pub live: bool,
}

/// What a confirmed switch back to USB monitor mode did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorModeDto {
    /// The model the panel named, when it named a known one.
    pub model: Option<&'static str>,
}

impl From<&MonitorModeSwitch> for MonitorModeDto {
    fn from(s: &MonitorModeSwitch) -> Self {
        Self {
            model: s.model().map(|m| m.name),
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
    /// The live screen's link failed and Bezel connects it again (T-7.11);
    /// `None` otherwise.
    pub reconnecting: Option<ReconnectingDto>,
}

/// Where a live screen whose link failed stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectingDto {
    /// The attempt under way or next (1 for the first).
    pub attempt: usize,
    /// Attempts in all.
    pub attempts: usize,
}

impl From<crate::studio::Reconnecting> for ReconnectingDto {
    fn from(r: crate::studio::Reconnecting) -> Self {
        Self {
            attempt: r.attempt,
            attempts: r.attempts,
        }
    }
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
    pub screen: Option<String>,
    /// The theme.
    pub theme: ThemeDto,
    /// Where it lives.
    pub location: Option<String>,
    /// The fastest refresh a theme may ask for, seconds (the core's
    /// `MIN_REFRESH_SECONDS`).
    pub min_refresh_seconds: f32,
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
    /// Ids of the catalog models whose panel it fits (the core's rule): the
    /// screens the Themes tab lists it for.
    pub models: Vec<&'static str>,
    /// The diagonal of the screen it was made for, hundredths of an inch,
    /// when every model it fits has the same one.
    pub diagonal_hundredths: Option<u16>,
    /// Changes whenever its files do: the UI asks for its thumbnail again.
    pub revision: String,
}

impl From<&ThemeEntry> for ThemeEntryDto {
    fn from(e: &ThemeEntry) -> Self {
        let models = fitting_models(&e.theme);
        Self {
            name: e.theme.name.clone(),
            location: e.location.0.clone(),
            canvas: SizeDto {
                width: e.theme.canvas.width,
                height: e.theme.canvas.height,
            },
            orientation: orientation_slug(e.theme.orientation),
            bundled: e.bundled,
            diagonal_hundredths: made_for(&models),
            models: models.iter().map(|m| m.id.0).collect(),
            revision: format!("{:016x}", e.revision),
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
    /// Original catalog SVG with editable paint and effects, when applicable.
    pub icon_svg: Option<String>,
    /// Size of the file, bytes.
    pub bytes: u64,
    /// A GIF of several pictures: it can be a video background.
    pub animated: bool,
    /// The poster of a video (or animated GIF): the one taken when it was
    /// added, or the one the theme's background names.
    pub poster: Option<String>,
    /// How long a video plays, ms, when known (learnt when it was added).
    pub duration_ms: Option<u64>,
}

/// Where a theme was saved.
#[derive(Debug, Clone, Serialize)]
pub struct SavedDto {
    /// The location.
    pub location: String,
}

/// What the preferences show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesDto {
    pub light_on_close: bool,
    pub light_runtime: bool,
    /// The language the user chose (`pt-BR` or `en`); `None` follows the
    /// system.
    pub language: Option<&'static str>,
    /// The system's language.
    pub system_language: &'static str,
    /// The host `net.ping` measures.
    pub ping_host: String,
    /// The host `net.ping` measures unless the user picks another.
    pub default_ping_host: &'static str,
    /// The folder of MangoHud's logs; `None`: MangoHud's own.
    pub mangohud_dir: Option<String>,
    /// Whether `gpu.fps` reads MangoHud's logs on this system (Linux).
    pub mangohud: bool,
    /// Which themes the Themes tab lists.
    pub theme_filter: ThemeFilterDto,
}

/// Which themes the Themes tab lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFilterDto {
    /// `screen` (those that fit the screen in use) or `all`; `None` until
    /// the user chooses: the screen's when one is known.
    pub scope: Option<&'static str>,
    /// `all`, `vertical` or `horizontal`.
    pub axis: &'static str,
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

/// A file added from the Media panel or dropped on the window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedMediaDto {
    /// Its reference.
    #[serde(rename = "ref")]
    pub reference: String,
    /// `video` (a video or an animated GIF: a video background) or `image`.
    pub kind: &'static str,
    /// The poster taken from a video; `None` for an image, or when it could
    /// not be taken ([`Self::poster_error`]).
    pub poster: Option<String>,
    /// Size of the file, bytes.
    pub bytes: u64,
    /// How long a video plays, ms, when known.
    pub duration_ms: Option<u64>,
    /// Why a video has no poster (`unsupported` without ffmpeg).
    pub poster_error: Option<UiError>,
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
            VideoState::StoredMissing(path) => ("storedMissing", Some(path.to_string())),
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

/// What Auto turns a theme's video background and the video's own size
/// (`video_auto`, D-2026-10-01-video-background-framing-2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoAutoDto {
    /// Clockwise degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// The video's size as probed without ffmpeg; `None` when unknown.
    pub size: Option<SizeDto>,
}

impl VideoAutoDto {
    /// The DTO of the session's answer ([`crate::studio::Studio::video_auto`]):
    /// clockwise quarter turns and the size; `None` (no video background)
    /// turns nothing and knows no size.
    pub fn of(auto: Option<(u8, Option<Size>)>) -> Self {
        let (turns, size) = auto.unwrap_or((0, None));
        Self {
            rotation: u16::from(turns % 4) * 90,
            size: size.map(|s| SizeDto {
                width: s.width,
                height: s.height,
            }),
        }
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

/// What a screen does when the computer shuts down, as the section "When
/// the computer shuts down" shows it (D-2026-10-03-power-off-standby-2, -6):
/// the choice recorded in the catalog the CLI shares, the four options and
/// why any is not offered, the videos stored on the screen, its card, and
/// how it stands (the shape the album's photos are framed in).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandbyDto {
    /// `keep`, `off`, `video` or `album`.
    pub choice: &'static str,
    /// The sleep timer of `off`, 1 to 10 minutes; `None` for the others.
    pub sleep_minutes: Option<u8>,
    /// The video of `video` (`<internal|sd>/video/<name>`); `None` for the
    /// others.
    pub file: Option<String>,
    /// The four options, in the order `keep`, `off`, `video`, `album`.
    pub options: Vec<StandbyOptionDto>,
    /// The videos stored on the screen with their sizes, internal first
    /// (empty when it is not awake).
    pub videos: Vec<StoredFileDto>,
    /// Whether the screen has a memory card (false when it is not awake).
    pub card: bool,
    /// How the screen stands (`portrait`, `landscape`, ...): the orientation
    /// last used with it, else its model's.
    pub orientation: &'static str,
}

impl StandbyDto {
    /// The DTO of `standby` as recorded, `options` as offered, what the
    /// screen offers (`offer`: its card and videos) and `orientation`.
    pub fn of(
        standby: &Standby,
        options: &[StandbyOption],
        offer: &Offer,
        orientation: Orientation,
    ) -> Self {
        Self {
            choice: standby.choice().slug(),
            sleep_minutes: standby.sleep_minutes().map(SleepMinutes::get),
            file: standby.file().map(ToString::to_string),
            options: options.iter().map(StandbyOptionDto::from).collect(),
            videos: offer.videos.iter().map(StoredFileDto::from).collect(),
            card: offer.card,
            orientation: orientation_slug(orientation),
        }
    }
}

/// One of the four options for a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StandbyOptionDto {
    /// `keep`, `off`, `video` or `album`.
    pub choice: &'static str,
    /// Whether it can be chosen now.
    pub enabled: bool,
    /// Why not (`notConnected`, `unsupported`, `noCard`, `noVideo`);
    /// `None` exactly when it is enabled.
    pub reason: Option<&'static str>,
}

impl From<&StandbyOption> for StandbyOptionDto {
    fn from(option: &StandbyOption) -> Self {
        Self {
            choice: option.choice.slug(),
            enabled: option.unavailable.is_none(),
            reason: option.unavailable.map(Unavailable::code),
        }
    }
}

/// A photo added to the card's album.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumAddedDto {
    /// Where it is stored: `sd/image/<name>`.
    pub path: String,
    /// The bytes of the PNG stored.
    pub bytes: u64,
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
    /// `needsConverter`, `emptyFile`, `tooLarge`, `convertedTooLarge`,
    /// `noCard` or `noSpace`.
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
            Refusal::ConvertedTooLarge { bytes, limit } => {
                (dto.bytes, dto.limit) = (Some(*bytes), Some(*limit));
                "convertedTooLarge"
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
pub(crate) fn name_error_char(e: &NameError) -> Option<String> {
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
    /// Refused before a byte was sent: the converted video is over the
    /// screen's per-file limit (D-2026-09-30-release-polish-12).
    Refused(RefusalDto),
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    /// `convert`, `upload`, `verify` or `delete`.
    pub phase: &'static str,
    /// Units done (ms of video, bytes, checks, or files deleted).
    pub done: u64,
    /// Units in the phase; 0 when unknown.
    pub total: u64,
    /// The file a storage manager job is at; absent for an upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<ProgressStepDto>,
}

/// Which file of a storage manager job a progress report is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressStepDto {
    /// Its place in the job, from 0.
    pub index: usize,
    /// Files in the job.
    pub count: usize,
    /// Its screen path.
    pub source: String,
    /// Where its copy goes; `None` for a delete.
    pub target: Option<String>,
}

impl From<Progress> for ProgressDto {
    fn from(p: Progress) -> Self {
        Self {
            phase: p.phase.slug(),
            done: p.done,
            total: p.total,
            step: None,
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

// ---------------------------------------------------- GIFs and stickers --

/// Whether a KLIPY key is saved, as the window sees it: never the key
/// (D-2026-10-01-gif-sticker-search-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDto {
    /// A key is saved.
    pub configured: bool,
    /// Its last 4 characters, to tell keys apart; `None` without a key, or
    /// for a key of 8 characters or fewer (most of it would show).
    pub last4: Option<String>,
}

/// A page of search results (`search_gifs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifPageDto {
    /// `gif` or `sticker`.
    pub kind: &'static str,
    /// What was searched, trimmed; empty for the trending items.
    pub text: String,
    /// The page, from 1.
    pub page: u32,
    /// Whether "Load more" has another page.
    pub has_next: bool,
    /// The results, in the provider's order.
    pub items: Vec<GifItemDto>,
}

impl GifPageDto {
    /// The page `page` answered for `query`.
    pub fn of(query: &GifQuery, page: &GifPage) -> Self {
        Self {
            kind: query.kind.slug(),
            text: query.text.clone(),
            page: query.page,
            has_next: page.has_next,
            items: page.items.iter().map(GifItemDto::from).collect(),
        }
    }
}

/// One search result, named by its id: the window never gets an address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifItemDto {
    /// The provider's id of the item.
    pub id: String,
    /// Its title (may be empty).
    pub title: String,
    /// Width of the GIF collected from it, pixels (0 when unknown).
    pub width: u32,
    /// Height of that GIF, pixels.
    pub height: u32,
}

impl From<&GifItem> for GifItemDto {
    fn from(item: &GifItem) -> Self {
        let (width, height) = item.download().map_or((0, 0), |r| (r.width, r.height));
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            width,
            height,
        }
    }
}

/// One item of the collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectedDto {
    /// Its bytes' SHA-256, in hex.
    pub id: String,
    /// Its name.
    pub name: String,
    /// `gif` or `sticker`.
    pub kind: &'static str,
    /// Width, pixels.
    pub width: u32,
    /// Height, pixels.
    pub height: u32,
    /// Size of the GIF, bytes.
    pub bytes: u64,
    /// When it was added, seconds since the Unix epoch.
    pub added_at: u64,
    /// Where it came from.
    pub source: GifSourceDto,
    /// Its preview as a `data:` URL; `None` when it has none.
    pub preview: Option<String>,
}

impl CollectedDto {
    /// `item` with its `preview`.
    pub fn of(item: &CollectedGif, preview: Option<String>) -> Self {
        Self {
            id: item.content.to_string(),
            name: item.name.clone(),
            kind: item.kind.slug(),
            width: item.width,
            height: item.height,
            bytes: item.bytes,
            added_at: item.added_at,
            source: GifSourceDto {
                provider: item.origin.provider.clone(),
                id: item.origin.id.clone(),
                url: item.origin.page_url.clone(),
            },
            preview,
        }
    }
}

/// Where a collected item came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifSourceDto {
    /// The provider (`klipy`).
    pub provider: String,
    /// The provider's id of the item.
    pub id: String,
    /// The provider's page about it.
    pub url: Option<String>,
}

/// What holds a collected item's bytes: the delete dialog names them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectedUsersDto {
    /// Names of the user's themes holding the same bytes.
    pub themes: Vec<String>,
    /// The open theme holds them.
    pub open_theme: bool,
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
        assert_eq!(json["restartable"], true);
    }

    #[test]
    fn panels_in_desktop_mode_are_listed_as_not_validated() {
        use bezel_core::app::discover_devices;
        let bus = FakeBus::turing_88().and(FakeBus::desktop_mode());
        let json =
            serde_json::to_value(DevicesDto::from(&discover_devices(&bus).unwrap())).unwrap();
        assert_eq!(json["screens"].as_array().unwrap().len(), 1);
        let panel = &json["desktopMode"][0];
        assert_eq!(panel["key"], "hid:/dev/hidraw7");
        assert_eq!(panel["usb"], "1a86:ad11");
        assert_eq!(panel["family"], "turing-usb");
        assert_eq!(panel["hardwareValidated"], false);
        assert!(!panel["models"].as_array().unwrap().is_empty());
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
            reference: "assets/a.mp4".into(),
            kind: "video",
            data_url: None,
            icon_svg: None,
            bytes: 2048,
            animated: false,
            poster: Some("assets/a-poster.png".into()),
            duration_ms: Some(1500),
        })
        .unwrap();
        assert_eq!(asset["ref"], "assets/a.mp4");
        assert_eq!(
            (&asset["poster"], &asset["durationMs"], &asset["bytes"]),
            (
                &serde_json::json!("assets/a-poster.png"),
                &serde_json::json!(1500),
                &serde_json::json!(2048)
            )
        );
    }
}
