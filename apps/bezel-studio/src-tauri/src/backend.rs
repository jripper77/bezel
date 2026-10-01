//! What each UI command does, without Tauri: the commands module only adds
//! threads, dialogs and IPC around these methods, so they run on fakes in
//! tests. Errors reach the UI as codes with arguments ([`UiError`]).

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bezel_core::app::{
    choose_screen, connect_screen, discover_devices, discover_screens, leave_desktop_mode,
    reopen_screen, restart_screen,
};
use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::Screen;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::sensor::{SensorKey, Wanted};
use bezel_core::domain::theme::{Background, MIN_REFRESH_SECONDS, Theme};
use bezel_core::ports::{
    DesktopModeHid, DeviceBus, ScreenConnector, ScreenLink, SensorSource, ThemeLocation, ThemeStore,
};
use bezel_sensors::SensorOptions;
use bezel_themes::dto::{BackgroundDto, ThemeDto};
use bezel_themes::import::import_path;
use bezel_themes::native::{is_native, native_location};

use crate::dto::{
    AddedDto, AssetDto, DevicesDto, ImportedDto, LiveVideoDto, MonitorModeDto, PreferencesDto,
    ReconnectingDto, RestartedDto, SampleDto, SavedDto, SensorDto, SessionDto, ThemeEntryDto,
    ThemeFilterDto, VideoAutoDto,
};
use crate::library::ThemeLibrary;
use crate::media::{extension_of, is_animated_gif, kind_of, thumbnail_data_url};
pub use crate::messages::UiResult;
use crate::messages::{ErrorCode, UiError};
use crate::settings::{SettingsFile, THEME_AXES, THEME_SCOPES};
use crate::storage::StorageState;
pub use crate::studio::MAX_REFRESH;
use crate::studio::{Delivery, Motion, Studio, TakenPoster, VideoProbe, Wait};
use crate::texts::{Texts, language_slug, parse_language, texts};
use crate::thumbnails::Thumbnails;
use crate::udev_help::UdevHelp;

/// Largest file accepted as an image or theme, bytes.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Builds the machine's sensors with their options (they take effect when
/// the source is built).
pub type SensorFactory = Arc<dyn Fn(SensorOptions) -> Box<dyn SensorSource> + Send + Sync>;

/// The ports and state behind the window.
pub struct Backend {
    /// Where screens are discovered.
    pub bus: Arc<dyn DeviceBus + Send + Sync>,
    /// How screens are opened.
    pub connector: Arc<dyn ScreenConnector + Send + Sync>,
    /// The HID interface of panels in desktop mode.
    pub hid: Arc<dyn DesktopModeHid + Send + Sync>,
    /// Theme files.
    pub store: Arc<dyn ThemeStore + Send + Sync>,
    /// The theme folders.
    pub library: ThemeLibrary,
    /// Remembered choices.
    pub settings: SettingsFile,
    /// The system's language, which the app follows unless the user chose
    /// another in the settings.
    pub system_language: Language,
    /// Builds the sensors again when their options change.
    pub make_sensors: SensorFactory,
    /// On Linux, the udev rule that fixes a denied port (`None` elsewhere).
    pub udev: Option<UdevHelp>,
    /// Font families themes can use.
    pub fonts: Vec<String>,
    /// The editing session.
    pub studio: Session,
    /// The screen's files: the media converter and the running operation.
    pub storage: StorageState,
    /// The library's thumbnails, drawn off the session's lock.
    pub thumbnails: Thumbnails,
}

/// The editing session behind its lock, and the signal that the live
/// screen's link came back from showing a frame: the screen's I/O happens
/// outside the lock, so previews render while the screen works.
pub struct Session {
    studio: Mutex<Studio>,
    link_back: Condvar,
}

/// Longest wait for the live link to come back from showing a frame; after
/// it the link counts as in use.
const LINK_BACK_WAIT: Duration = Duration::from_secs(10);

impl Session {
    /// The session of `studio`.
    pub fn new(studio: Studio) -> Self {
        Self {
            studio: Mutex::new(studio),
            link_back: Condvar::new(),
        }
    }

    /// The session, even after a panic in another command.
    pub fn lock(&self) -> MutexGuard<'_, Studio> {
        self.studio.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The session once the live link is back from showing a frame.
    fn idle(&self) -> MutexGuard<'_, Studio> {
        let waited = self
            .link_back
            .wait_timeout_while(self.lock(), LINK_BACK_WAIT, |s| s.presenting());
        waited.unwrap_or_else(PoisonError::into_inner).0
    }
}

/// Longest sleep of the refresh loop between two looks at the session: an
/// edit or a screen going live is picked up this soon.
pub const LOOK_AGAIN: Duration = Duration::from_millis(250);

/// How long the refresh loop sleeps at `now` for the refresh due at `due`:
/// until then (nothing when it is past), at most [`LOOK_AGAIN`].
pub fn sleep_until(due: Instant, now: Instant) -> Duration {
    due.saturating_duration_since(now).min(LOOK_AGAIN)
}

/// Says that a preview does not change by itself (no animated GIF shows,
/// no video plays).
pub const STILL: u32 = u32::MAX;

/// A preview frame for the UI: a 12-byte header (width, height, and the
/// milliseconds until its next picture is due: its animated GIFs change or
/// its video background shows its next picture, [`STILL`] when nothing
/// moves; u32 little-endian), then the RGBA pixels.
pub fn frame_bytes(frame: &bezel_core::domain::frame::Frame, change: Option<Duration>) -> Vec<u8> {
    let size = frame.size();
    let next = change.map_or(STILL, |d| u32::try_from(d.as_millis()).unwrap_or(STILL - 1));
    let mut out = Vec::with_capacity(12 + frame.as_rgba().len());
    out.extend_from_slice(&size.width.to_le_bytes());
    out.extend_from_slice(&size.height.to_le_bytes());
    out.extend_from_slice(&next.to_le_bytes());
    out.extend_from_slice(frame.as_rgba());
    out
}

fn theme_of(dto: &ThemeDto) -> UiResult<Theme> {
    check_rotation(&dto.background)?;
    Theme::try_from(dto).map_err(|e| UiError::new(ErrorCode::InvalidTheme).arg("detail", e.0))
}

/// A video background's framing turns its video by 0, 90, 180 or 270
/// degrees, or leaves it to Auto (no `rotation`): another rotation is
/// `invalidInput`, not `invalidTheme`
/// (D-2026-10-01-video-background-framing-2); the rotations are the theme
/// format's ([`bezel_themes::dto::FramingDto::quarter_turns`]). Its other
/// numbers are clamped when the theme is read.
fn check_rotation(background: &BackgroundDto) -> UiResult<()> {
    match background {
        BackgroundDto::Video {
            framing: Some(framing),
            ..
        } => framing
            .quarter_turns()
            .map(drop)
            .map_err(|e| UiError::new(ErrorCode::InvalidInput).arg("detail", e.0)),
        _ => Ok(()),
    }
}

/// Pages of the user guide the UI opens in the system's browser.
pub const GUIDE_PAGES: &[&str] = &["ffmpeg"];

/// Where the guide's `page` is in `language` (`en` or `pt-BR`): a fixed
/// address on the project's site, so the UI can open no other.
pub fn guide_url(page: &str, language: &str) -> UiResult<String> {
    let folder = match language {
        "en" => Some(""),
        "pt-BR" => Some("pt-BR/"),
        _ => None,
    };
    match folder {
        Some(folder) if GUIDE_PAGES.contains(&page) => Ok(format!(
            "https://github.com/slipalison/bezel/blob/main/docs/user/{folder}{page}.md"
        )),
        _ => Err(UiError::new(ErrorCode::InvalidInput)
            .arg("detail", format!("guide page \"{page}\" in \"{language}\""))),
    }
}

/// Orientation of a new theme for `model` when none was used with its screen
/// yet: horizontal for bar-shaped panels (the long side at least twice the
/// short one, like the 8.8"), else the model's native orientation.
pub fn default_orientation(model: &DeviceModel) -> Orientation {
    let panel = model.panel.portrait();
    if u64::from(panel.height) >= 2 * u64::from(panel.width) {
        Orientation::Landscape
    } else {
        model.native_orientation
    }
}

/// Whether `text` can name a host to ping: a name or an IPv4 or IPv6
/// address (letters, digits, `.`, `-` and `:`), not an option.
fn is_host(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 253
        && !text.starts_with(['-', '.'])
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

fn read_file(path: &Path) -> UiResult<Vec<u8>> {
    read_limited(path, MAX_FILE_BYTES)
}

/// The content of the file at `path`, refused over `limit` bytes.
pub(crate) fn read_limited(path: &Path, limit: u64) -> UiResult<Vec<u8>> {
    let unreadable = |e| UiError::file(path.display(), e);
    let size = std::fs::metadata(path).map_err(unreadable)?.len();
    if size > limit {
        return Err(UiError::new(ErrorCode::FileTooLarge)
            .arg("file", path.display())
            .arg("size", size / (1024 * 1024))
            .arg("limit", limit / (1024 * 1024)));
    }
    std::fs::read(path).map_err(unreadable)
}

impl Backend {
    /// The editing session, even after a panic in another command.
    pub fn studio(&self) -> MutexGuard<'_, Studio> {
        self.studio.lock()
    }

    /// The editing session once the live link is back from showing a frame
    /// (for whatever needs the link).
    pub(crate) fn idle_studio(&self) -> MutexGuard<'_, Studio> {
        self.studio.idle()
    }

    /// Shows the frame the session prepared, outside its lock, then gives
    /// the link back to the session.
    fn deliver(&self, delivered: bezel_core::Result<Option<Delivery>>) -> bezel_core::Result<()> {
        let Some(mut delivery) = delivered? else {
            return Ok(());
        };
        let outcome = delivery.present();
        let unwanted = self.studio().presented(delivery, &outcome, Instant::now());
        self.studio.link_back.notify_all();
        drop(unwanted);
        outcome
    }

    /// Shows the edited theme on the live screen now (its video probed
    /// first when the screen is to start it).
    pub(crate) fn show_now(&self, time: LocalTime) -> bezel_core::Result<()> {
        let probe = self.studio().live_video_to_probe();
        self.learn_video(probe, Wait::No);
        let delivered = self.idle_studio().frame_for_screen(time, Instant::now());
        self.deliver(delivered)
    }

    /// Runs `probe` of the theme's video outside the session's lock (ffprobe
    /// reads a container that is not MP4 or GIF) and hands what it said to
    /// the session; with [`Wait::No`], nothing while a storage job holds the
    /// converter.
    fn learn_video(&self, probe: Option<VideoProbe>, wait: Wait) {
        if let Some(probed) = probe.and_then(|probe| probe.run(wait)) {
            self.studio().probed(probed);
        }
    }

    fn find_screen(&self, key: &str) -> UiResult<Screen> {
        Ok(choose_screen(
            discover_screens(self.bus.as_ref())?,
            Some(key),
        )?)
    }

    pub(crate) fn connect(&self, key: &str) -> UiResult<Box<dyn ScreenLink>> {
        let screen = self.find_screen(key)?;
        Ok(self.connector.connect(&screen)?)
    }

    /// `error` with what fixes it: the udev rule's install command for a
    /// port the system denied (Linux).
    pub fn explain(&self, error: UiError) -> UiError {
        if error.code() != "accessDenied" {
            return error;
        }
        match self.udev.as_ref().and_then(UdevHelp::command) {
            Some(command) => error.with_udev_command(command),
            None => error,
        }
    }

    // -------------------------------------------------------- preferences --

    /// The app's language: the one chosen in the settings, else the system's.
    pub fn language(&self) -> Language {
        self.settings
            .load()
            .language()
            .unwrap_or(self.system_language)
    }

    /// The backend's own texts, in the app's language.
    pub fn texts(&self) -> Texts {
        texts(self.language())
    }

    /// What the preferences show.
    pub fn preferences(&self) -> PreferencesDto {
        let settings = self.settings.load();
        PreferencesDto {
            theme_filter: ThemeFilterDto {
                scope: settings.themes_shown(),
                axis: settings.themes_axis(),
            },
            language: settings.language().map(language_slug),
            system_language: language_slug(self.system_language),
            ping_host: settings.sensor_options().ping_host,
            default_ping_host: SensorOptions::DEFAULT_PING_HOST,
            mangohud_dir: settings.mangohud_dir,
            mangohud: cfg!(target_os = "linux"),
        }
    }

    /// Measures the round trip to `ping_host` (empty: the default) and reads
    /// MangoHud's logs from `mangohud_dir` (`None`: MangoHud's own folder)
    /// from now on, and remembers both. The sensors are built again with
    /// them, outside the session's lock.
    pub fn set_sensor_options(&self, ping_host: &str, mangohud_dir: Option<&str>) -> UiResult<()> {
        let host = match ping_host.trim() {
            "" | SensorOptions::DEFAULT_PING_HOST => None,
            host if is_host(host) => Some(host.to_string()),
            host => return Err(UiError::new(ErrorCode::InvalidHost).arg("host", host)),
        };
        let folder = mangohud_dir
            .map(|dir| match Path::new(dir) {
                path if path.is_absolute() && path.is_dir() => Ok(dir.to_string()),
                _ => Err(UiError::new(ErrorCode::InvalidFolder).arg("folder", dir)),
            })
            .transpose()?;
        self.settings.update(|s| {
            s.ping_host = host;
            s.mangohud_dir = folder;
        });
        let sensors = (self.make_sensors)(self.settings.load().sensor_options());
        self.studio().replace_sensors(sensors)?;
        Ok(())
    }

    /// Remembers which themes the Themes tab lists: `scope` (`screen`,
    /// `all`, or `None` for the screen's when one is known) and `axis`
    /// (`all`, `vertical` or `horizontal`).
    pub fn set_theme_filter(&self, scope: Option<&str>, axis: &str) -> UiResult<()> {
        let unknown = |text: &str| {
            UiError::new(ErrorCode::InvalidInput).arg("detail", format!("theme filter \"{text}\""))
        };
        if let Some(scope) = scope.filter(|s| !THEME_SCOPES.contains(s)) {
            return Err(unknown(scope));
        }
        if !THEME_AXES.contains(&axis) {
            return Err(unknown(axis));
        }
        self.settings.update(|s| {
            s.themes_shown = scope.map(str::to_string);
            s.themes_axis = Some(axis.to_string());
        });
        Ok(())
    }

    /// Uses `language` (`pt-BR` or `en`) from now on, or the system's for
    /// `None`, and remembers the choice. Returns the language in use.
    pub fn set_language(&self, language: Option<&str>) -> UiResult<Language> {
        let chosen = language
            .map(|slug| {
                parse_language(slug)
                    .ok_or_else(|| UiError::new(ErrorCode::UnknownLanguage).arg("language", slug))
            })
            .transpose()?;
        self.settings
            .update(|s| s.language = chosen.map(|l| language_slug(l).to_string()));
        let language = chosen.unwrap_or(self.system_language);
        self.studio().set_language(language);
        Ok(language)
    }

    // ------------------------------------------------------------ screens --

    /// The connected screens and the panels in desktop mode (read-only).
    pub fn devices(&self) -> UiResult<DevicesDto> {
        Ok(DevicesDto::from(&discover_devices(self.bus.as_ref())?))
    }

    /// Switches the panel in desktop mode at `key` back to USB monitor mode
    /// (D-2026-09-30-release-polish-8, not validated on hardware). Without
    /// `Confirm::Yes` nothing is sent.
    pub fn leave_desktop_mode(&self, key: &str, confirm: Confirm) -> UiResult<MonitorModeDto> {
        let switched =
            leave_desktop_mode(self.bus.as_ref(), self.hid.as_ref(), Some(key), confirm)?;
        Ok(MonitorModeDto::from(&switched))
    }

    /// Restarts the hung screen `key` without a USB replug
    /// (D-2026-09-30-release-polish-13): live mode on it (by either of its
    /// ports) stops first (its port closes), its MCU restarts it and Bezel
    /// waits until it is back (about 10 s); a screen that was live shows the
    /// theme live again, under its new key. Storage operations, live mode
    /// and brightness answer `busy` meanwhile.
    pub fn restart_screen(&self, key: &str, time: LocalTime) -> UiResult<RestartedDto> {
        let restarted = {
            let _claim = self.storage.claim()?;
            let was_live = {
                let mut studio = self.idle_studio();
                let live = studio.is_live(key);
                if live {
                    drop(studio.stop_live());
                }
                live
            };
            let screen = restart_screen(self.bus.as_ref(), self.connector.as_ref(), Some(key))?;
            (key_of(&screen, key), was_live)
        };
        let (key, was_live) = restarted;
        let live = was_live
            && self
                .set_live(true, Some(&key), time)
                .inspect_err(|e| tracing::warn!(screen = key, "live mode not resumed: {e}"))
                .is_ok();
        Ok(RestartedDto { key, live })
    }

    /// Starts (`screen` given, by either of its ports) or stops showing the
    /// edited theme live. The screen goes live under its one key, the
    /// address it is listed by once connected (its display's;
    /// D-2026-10-01-live-screen-controls-2), which is remembered with the
    /// orientation; `screen` only when it cannot be listed again.
    pub fn set_live(&self, on: bool, screen: Option<&str>, time: LocalTime) -> UiResult<()> {
        // Stop first: a screen can only be opened once.
        let previous = self.idle_studio().stop_live();
        drop(previous);
        if !on {
            self.settings.update(|s| s.live_screen = None);
            return Ok(());
        }
        let asked = screen.ok_or_else(|| UiError::new(ErrorCode::NoScreenChosen))?;
        self.storage.ensure_idle()?;
        // Opening wakes the screen (seconds); the session stays usable meanwhile.
        let (found, link) =
            connect_screen(self.bus.as_ref(), self.connector.as_ref(), Some(asked))?;
        let key = key_of(&found, asked);
        let orientation = {
            let mut studio = self.studio();
            studio.go_live_on(key.clone(), found, link);
            studio.theme().orientation
        };
        self.show_now(time)?;
        self.settings.update(|s| {
            s.live_screen = Some(key.clone());
            s.remember_orientation(&key, orientation);
        });
        Ok(())
    }

    /// The tray's live switch: off when a screen is live, else on for the
    /// first connected screen (an awake one first). Whether a screen is live
    /// after.
    pub fn toggle_live(&self, time: LocalTime) -> UiResult<bool> {
        if self.studio().live_key().is_some() {
            self.set_live(false, None, time)?;
            return Ok(false);
        }
        let screen =
            discover_screens(self.bus.as_ref()).and_then(|screens| choose_screen(screens, None))?;
        let key = screen
            .address()
            .ok_or_else(|| UiError::new(ErrorCode::NoScreenAddress))?;
        self.set_live(true, Some(&key.0), time)?;
        Ok(true)
    }

    /// Remembers `orientation` as the last one used with `screen` (the file
    /// is written only when it changes).
    fn remember_orientation(&self, screen: &str, orientation: Orientation) {
        if self.settings.load().orientation_for(screen) != Some(orientation) {
            self.settings
                .update(|s| s.remember_orientation(screen, orientation));
        }
    }

    /// Sets a screen's brightness (through the live link when it is live,
    /// named by either of its ports).
    pub fn set_brightness(&self, screen: &str, percent: u8) -> UiResult<()> {
        let brightness =
            Brightness::new(percent).ok_or_else(|| UiError::new(ErrorCode::BrightnessRange))?;
        if self.idle_studio().live_brightness(screen, brightness)? {
            return Ok(());
        }
        self.storage.ensure_idle()?;
        Ok(self.connect(screen)?.set_brightness(brightness)?)
    }

    /// Hands a screen back to its own mode; a live one (named by either of
    /// its ports) stops live mode and is released through its live link.
    pub fn release(&self, screen: &str) -> UiResult<()> {
        self.storage.ensure_idle()?;
        let live = {
            let mut studio = self.idle_studio();
            if studio.is_live(screen) {
                studio.stop_live()
            } else {
                None
            }
        };
        let mut link = match live {
            Some(link) => {
                self.settings.update(|s| s.live_screen = None);
                link
            }
            None => self.connect(screen)?,
        };
        Ok(link.release()?)
    }

    // ------------------------------------------------------------ sensors --

    /// The sensor catalog, re-read.
    pub fn catalog(&self) -> UiResult<Vec<SensorDto>> {
        let mut studio = self.studio();
        Ok(studio
            .refresh_catalog()?
            .iter()
            .map(SensorDto::from)
            .collect())
    }

    /// The sensors the library's list shows now (none while it is hidden):
    /// the refresh loop measures them with the theme's
    /// (D-2026-09-30-release-polish-11). Keys that are not sensor keys are
    /// left out.
    pub fn show_sensors(&self, keys: &[String]) {
        let listed: Wanted = keys
            .iter()
            .filter_map(|k| SensorKey::new(k.as_str()))
            .collect();
        self.studio().show_sensors(listed);
    }

    /// The latest readings (sampled by the refresh loop).
    pub fn sample(&self) -> SampleDto {
        let studio = self.studio();
        let (snapshot, millis) = studio.readings();
        SampleDto {
            sample_millis: millis,
            readings: SampleDto::readings(snapshot, studio.quantities()),
            live: studio.live_key().map(str::to_string),
            live_error: studio.live_error().cloned(),
            video: studio.live_video().and_then(LiveVideoDto::of),
            reconnecting: studio.reconnecting().map(ReconnectingDto::from),
        }
    }

    // ------------------------------------------------------------- themes --

    /// The edited theme and its location.
    pub fn session(&self) -> SessionDto {
        let studio = self.studio();
        SessionDto {
            theme: ThemeDto::from(studio.theme()),
            location: studio.location().map(|l| l.0.clone()),
            min_refresh_seconds: MIN_REFRESH_SECONDS,
        }
    }

    /// Takes the UI's theme and renders it at `now` ([`frame_bytes`]: its
    /// size, when its next picture is due, then RGBA). A video background
    /// plays with `motion` ([`Studio::preview`]), else shows its poster; a
    /// video not probed yet is probed first, outside the session's lock
    /// (while a storage job holds the converter, the poster shows and the UI
    /// asks again soon).
    pub fn render(
        &self,
        theme: &ThemeDto,
        time: LocalTime,
        now: Instant,
        motion: Motion,
    ) -> UiResult<Vec<u8>> {
        let theme = theme_of(theme)?;
        let mut studio = self.studio();
        studio.set_theme(theme);
        if let Some(probe) = studio.video_to_probe() {
            drop(studio);
            self.learn_video(Some(probe), Wait::No);
            // The theme the UI sent last, which may be a newer one by now.
            studio = self.studio();
        }
        let (frame, change) = studio.preview(time, now, motion)?;
        Ok(frame_bytes(&frame, change))
    }

    /// Takes the UI's theme and says what Auto turns its video background
    /// and the video's own size (D-2026-10-01-video-background-framing-2):
    /// probed once (an MP4's header needs no ffmpeg), outside the session's
    /// lock, waiting for a storage job that holds the converter.
    pub fn video_auto(&self, theme: &ThemeDto) -> UiResult<VideoAutoDto> {
        let theme = theme_of(theme)?;
        let probe = {
            let mut studio = self.studio();
            studio.set_theme(theme);
            studio.video_to_probe()
        };
        self.learn_video(probe, Wait::Yes);
        Ok(VideoAutoDto::of(self.studio().video_auto()))
    }

    /// Takes the UI's theme and shows it on the live screen now, in the
    /// theme's orientation (remembered for that screen).
    pub fn push(&self, theme: &ThemeDto, time: LocalTime) -> UiResult<()> {
        let theme = theme_of(theme)?;
        let orientation = theme.orientation;
        let live = {
            let mut studio = self.studio();
            studio.set_theme(theme);
            studio.live_key().map(str::to_string)
        };
        self.show_now(time)?;
        if let Some(key) = live {
            self.remember_orientation(&key, orientation);
        }
        Ok(())
    }

    /// The library's themes.
    pub fn themes(&self) -> Vec<ThemeEntryDto> {
        self.library
            .list()
            .iter()
            .map(ThemeEntryDto::from)
            .collect()
    }

    /// The thumbnail of a library theme as a PNG `data:` URL, drawn at
    /// `time` when it is not kept yet, outside the session's lock. `None`
    /// when the theme cannot be drawn: the gallery shows its placeholder.
    pub fn thumbnail(&self, location: &str, time: LocalTime) -> UiResult<Option<String>> {
        let location = ThemeLocation(location.to_string());
        if !self.library.allows(&location) {
            return Err(UiError::new(ErrorCode::NotInLibrary).arg("location", location.0));
        }
        let language = self.language();
        Ok(self
            .thumbnails
            .get(self.store.as_ref(), &location, language, time))
    }

    /// Opens a theme of the library, or a theme file the user picked in a
    /// dialog during this session.
    pub fn open(&self, location: &str) -> UiResult<ThemeDto> {
        let location = ThemeLocation(location.to_string());
        if !self.library.allows(&location) {
            return Err(UiError::new(ErrorCode::NotInLibrary).arg("location", location.0));
        }
        self.open_at(location)
    }

    /// Opens the theme at `location`, which the app chose itself.
    fn open_at(&self, location: ThemeLocation) -> UiResult<ThemeDto> {
        let mut studio = self.studio();
        studio.open(self.store.as_ref(), location.clone())?;
        self.settings
            .update(|s| s.last_theme = Some(location.0.clone()));
        Ok(ThemeDto::from(studio.theme()))
    }

    /// Saves the UI's theme: to `target` when given (a file picked in the
    /// save dialog, [`ThemeLibrary::grant`]ed first), else where it was
    /// opened from when the library allows it (otherwise, and for a bundled
    /// theme, a copy in the user folder).
    pub fn save(&self, theme: &ThemeDto, target: Option<ThemeLocation>) -> UiResult<SavedDto> {
        let theme = theme_of(theme)?;
        if let Some(target) = target.as_ref().filter(|t| !self.library.allows(t)) {
            return Err(UiError::new(ErrorCode::NotPicked).arg("location", &target.0));
        }
        let poster = self.retake_poster(&theme);
        let mut studio = self.studio();
        let location =
            target.unwrap_or_else(|| self.library.save_location(studio.location(), &theme.name));
        studio.set_theme(theme);
        if let Some(poster) = poster {
            studio.poster_taken(poster);
        }
        studio.save(self.store.as_ref(), location.clone())?;
        drop(studio);
        // Its gallery card shows it as saved.
        self.thumbnails.forget(&location);
        self.settings
            .update(|s| s.last_theme = Some(location.0.clone()));
        Ok(SavedDto {
            location: location.0,
        })
    }

    /// The poster of `theme`'s video background taken again when the theme
    /// frames the video otherwise than the poster shows
    /// (D-2026-10-01-video-background-framing-3), the video probed first if
    /// it is not yet: both outside the session's lock, so the refresh, the
    /// live screen and the preview go on meanwhile. `None` when no new
    /// poster is needed or none could be taken (without ffmpeg, or while a
    /// storage job holds the converter, the poster stays as it is).
    fn retake_poster(&self, theme: &Theme) -> Option<TakenPoster> {
        let probe = {
            let mut studio = self.studio();
            studio.set_theme(theme.clone());
            studio.video_to_probe()
        };
        self.learn_video(probe, Wait::No);
        let retake = {
            let mut studio = self.studio();
            studio.set_theme(theme.clone());
            studio.poster_to_take()
        };
        retake?.run()
    }

    /// A blank theme sized for `screen` (or the 8.8" when none is known) in
    /// `orientation`, which is then remembered for that screen. Without one:
    /// the orientation last used with the screen, else
    /// [`default_orientation`].
    pub fn new_theme(
        &self,
        screen: Option<&str>,
        name: &str,
        orientation: Option<Orientation>,
    ) -> UiResult<ThemeDto> {
        let model = screen
            .and_then(|key| self.find_screen(key).ok())
            .and_then(|s| s.candidates.first().copied())
            .or_else(|| model_by_id(DEFAULT_MODEL))
            .ok_or_else(|| UiError::new(ErrorCode::NoModel))?;
        let orientation = match (orientation, screen) {
            (Some(chosen), Some(key)) => {
                self.remember_orientation(key, chosen);
                chosen
            }
            (Some(chosen), None) => chosen,
            (None, key) => key
                .and_then(|k| self.settings.load().orientation_for(k))
                .unwrap_or_else(|| default_orientation(model)),
        };
        let theme = Theme::blank(name, model.panel, orientation);
        let mut studio = self.studio();
        studio.start(theme, Default::default(), None);
        Ok(ThemeDto::from(studio.theme()))
    }

    /// Imports a theme: Bezel's own (`.bezeltheme`, a folder with a
    /// `theme.json`, or that `theme.json`) as it is; another app's (a TURZX `.turtheme`, a
    /// turing-smart-screen-python `theme.yaml` or its folder) converted, with
    /// what had no exact equivalent as warnings.
    pub fn import(&self, path: &Path) -> UiResult<ImportedDto> {
        let (theme, assets, warnings) = if is_native(path) {
            let (theme, assets) = self.store.load(&native_location(path))?;
            (theme, assets, Vec::new())
        } else {
            let (theme, assets, report) = import_path(path)?;
            (
                theme,
                assets,
                report.warnings.iter().map(Into::into).collect(),
            )
        };
        let mut studio = self.studio();
        // A copy: saving writes to the user folder, not over the imported file.
        studio.start(theme, assets, None);
        Ok(ImportedDto {
            theme: ThemeDto::from(studio.theme()),
            warnings,
        })
    }

    // -------------------------------------------------------------- media --

    /// Adds the image at `path` to the theme. An animated GIF also gets its
    /// poster, ready to be a video background.
    pub fn add_image(&self, path: &Path) -> UiResult<AddedDto> {
        let bytes = read_file(path)?;
        if is_animated_gif(&bytes) {
            let added = self.add_moving(path, bytes)?;
            return Ok(AddedDto {
                reference: added.reference,
            });
        }
        self.add_image_bytes(path, bytes)
    }

    /// Adds the image file at `path`, whose content is `bytes`.
    pub(crate) fn add_image_bytes(&self, path: &Path, bytes: Vec<u8>) -> UiResult<AddedDto> {
        if image::guess_format(&bytes).is_err() {
            return Err(UiError::new(ErrorCode::NotAnImage).arg("file", path.display()));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let asset = self.studio().add_asset(&name, bytes);
        Ok(AddedDto { reference: asset.0 })
    }

    /// The theme's assets with previews (images only: videos are not
    /// decoded nor copied for the list), the poster of each video and what
    /// the session learnt of it.
    pub fn assets(&self) -> Vec<AssetDto> {
        let listed: Vec<(AssetDto, Option<Vec<u8>>)> = {
            let studio = self.studio();
            let background = match &studio.theme().background {
                Background::Video { asset, poster, .. } => Some((asset.clone(), poster.clone())),
                _ => None,
            };
            studio
                .assets()
                .iter()
                .map(|(asset, bytes)| {
                    let kind = kind_of(asset);
                    let known = studio.added_video(asset);
                    let named = background
                        .as_ref()
                        .filter(|(video, _)| video == asset)
                        .and_then(|(_, poster)| poster.clone());
                    let dto = AssetDto {
                        reference: asset.0.clone(),
                        kind,
                        data_url: None,
                        bytes: bytes.len() as u64,
                        animated: false,
                        poster: known.and_then(|k| k.poster.clone()).or(named).map(|p| p.0),
                        duration_ms: known
                            .and_then(|k| k.duration)
                            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
                    };
                    (dto, (kind == "image").then(|| bytes.clone()))
                })
                .collect()
        };
        listed
            .into_iter()
            .map(|(dto, image)| match image {
                Some(bytes) => AssetDto {
                    data_url: thumbnail_data_url(&bytes),
                    animated: extension_of(&dto.reference) == "gif" && is_animated_gif(&bytes),
                    ..dto
                },
                None => dto,
            })
            .collect()
    }

    // -------------------------------------------------------------- start --

    /// The theme the window starts with: the last one when it still opens,
    /// else a blank one for the first connected screen (in the orientation
    /// [`Self::new_theme`] picks for it).
    pub fn restore_theme(&self) {
        if let Some(last) = self.settings.load().last_theme {
            // The app's own record of a theme the window was allowed to open
            // or save last time: saving writes back to it again.
            let location = ThemeLocation(last.clone());
            self.library.grant(&location);
            match self.open_at(location) {
                Ok(_) => return,
                Err(e) => tracing::warn!(theme = last, "last theme not reopened: {e}"),
            }
        }
        let screen = discover_screens(self.bus.as_ref())
            .and_then(|screens| choose_screen(screens, None))
            .ok();
        let key = screen
            .as_ref()
            .and_then(Screen::address)
            .map(|a| a.0.clone());
        let blank = match self.new_theme(key.as_deref(), self.texts().untitled, None) {
            Ok(theme) => theme,
            Err(e) => {
                tracing::warn!("no starting theme: {e}");
                return;
            }
        };
        // First run: a bundled theme made for this screen and orientation is a
        // better start than an empty canvas (saving it writes a copy).
        let fitting = self.library.list().into_iter().find(|e| {
            e.bundled
                && e.theme.canvas.width == blank.canvas.width
                && e.theme.canvas.height == blank.canvas.height
                && crate::dto::orientation_slug(e.theme.orientation) == blank.orientation
        });
        if let Some(entry) = fitting
            && let Err(e) = self.open_at(entry.location.clone())
        {
            tracing::warn!(theme = entry.location.0, "bundled theme not opened: {e}");
        }
    }

    /// Shows the theme live again on the screen that was live when the app
    /// last ran, when it is connected. A failure leaves live mode off. A key
    /// saved by an older version as the MCU's port is replaced by the
    /// display's ([`Self::set_live`]; D-2026-10-01-live-screen-controls-2).
    pub fn restore_live(&self, time: LocalTime) {
        if let Some(key) = self.settings.load().live_screen
            && let Err(e) = self.set_live(true, Some(&key), time)
        {
            tracing::warn!(screen = key, "live mode not restored: {e}");
        }
    }

    /// One refresh of the session at `now`: an attempt to connect a live
    /// screen that failed when one is due, the probe of the theme's video
    /// when the live screen is to start it, a sample when one is due, and
    /// the live screen's frame when it is due (the screen's I/O and the
    /// probe outside the session's lock). Returns when the next refresh is
    /// due.
    pub fn tick(&self, time: LocalTime, now: Instant) -> Instant {
        self.reconnect(now);
        let probe = self.studio().live_video_to_probe();
        self.learn_video(probe, Wait::No);
        let delivered = self.studio().tick(time, now);
        if let Err(e) = self.deliver(delivered) {
            tracing::warn!("live screen frame failed: {e}");
        }
        self.studio().next_due()
    }

    /// Connects a live screen whose link failed again, when an attempt is
    /// due (T-7.11): found by identity (a rev C SoC comes back under a new
    /// device name), restarted through its MCU by the connection when it
    /// hung, outside the session's lock. Back, it shows the theme again
    /// under its new key, which is remembered.
    fn reconnect(&self, now: Instant) {
        let Some(attempt) = self.studio().reconnect_due(now) else {
            return;
        };
        let outcome = reopen_screen(self.bus.as_ref(), self.connector.as_ref(), attempt.screen());
        let unwanted = self.studio().reconnected(attempt, outcome, Instant::now());
        drop(unwanted);
        let back = {
            let studio = self.studio();
            studio
                .reconnecting()
                .is_none()
                .then(|| studio.live_key().map(str::to_string))
                .flatten()
        };
        if let Some(key) = back
            && self.settings.load().live_screen.as_deref() != Some(key.as_str())
        {
            self.settings.update(|s| s.live_screen = Some(key));
        }
    }
}

/// The key `screen` goes by: the address it is listed by (its display's
/// when awake), else `asked`, the key it was asked for.
fn key_of(screen: &Screen, asked: &str) -> String {
    screen
        .address()
        .map_or_else(|| asked.to_string(), |a| a.0.clone())
}

/// Model a new theme is sized for when no screen is connected.
pub const DEFAULT_MODEL: bezel_core::domain::device::ModelId =
    bezel_core::domain::device::ModelId("turing-8.8");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::Copies;
    use crate::storage::tests::{Call, FakeMedia, fixture_on};
    use crate::studio::CONVERTER_BUSY;
    use bezel_core::domain::frame::{Frame, Rgba};
    use bezel_core::domain::framing::{VideoFit, VideoFraming, Zoom};
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_core::domain::theme::AssetRef;
    use bezel_devices::fake::FakeStorage;
    use bezel_devices::{FakeBus, FakeConnector, FakeHid};
    use bezel_media::archive::MemoryArchive;
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;
    use bezel_themes::FsThemeStore;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::mpsc;

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 21,
        minute: 5,
        second: 0,
        weekday: 2,
    };
    const KEY: &str = "/dev/ttyACM1";

    struct Fixture {
        backend: Backend,
        connector: FakeConnector,
        root: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn fixture(name: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("bezel-backend-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let connector = FakeConnector::default();
        let theme = Theme::blank("Start", Size::new(480, 1920), Orientation::ReversePortrait);
        let studio = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip)),
            Language::English,
            theme,
        );
        let backend = Backend {
            bus: Arc::new(FakeBus::turing_88()),
            connector: Arc::new(connector.clone()),
            hid: Arc::new(FakeHid::answering(0x88)),
            store: Arc::new(FsThemeStore),
            library: ThemeLibrary::new(root.join("themes"), vec![]),
            settings: SettingsFile::new(root.join("settings.json")),
            system_language: Language::English,
            make_sensors: Arc::new(|_| Box::new(FakeSensors::demo())),
            udev: Some(UdevHelp::new(root.join("cache").join("60-bezel.rules"))),
            fonts: vec!["Inter".into()],
            studio: Session::new(studio),
            storage: StorageState::new(
                Box::new(crate::storage::tests::FakeMedia::ready()),
                crate::manager::Copies::in_memory(bezel_media::archive::MemoryArchive::new()),
                root.join("scratch"),
            ),
            thumbnails: crate::thumbnails::tests::thumbnails(root.join("thumbnails")),
        };
        Fixture {
            backend,
            connector,
            root,
        }
    }

    #[test]
    fn the_first_run_opens_the_bundled_theme_for_the_screen() {
        let mut f = fixture("first-run");
        let bundled = f.root.join("bundled");
        for (name, size, orientation) in [
            ("Wide", Size::new(480, 1920), Orientation::Landscape),
            ("Tall", Size::new(480, 1920), Orientation::Portrait),
            ("Small", Size::new(320, 480), Orientation::Landscape),
        ] {
            let at = ThemeLocation(
                bundled
                    .join(format!("{name}.bezeltheme"))
                    .display()
                    .to_string(),
            );
            FsThemeStore
                .save(
                    &at,
                    &Theme::blank(name, size, orientation),
                    &Default::default(),
                )
                .unwrap();
        }
        f.backend.library = ThemeLibrary::new(f.root.join("themes"), vec![bundled]);
        f.backend.restore_theme();
        let session = f.backend.session();
        assert_eq!(session.theme.name, "Wide", "the 8.8\" starts horizontal");
        assert!(session.location.unwrap().ends_with("Wide.bezeltheme"));
        // Saving it writes a copy in the user folder, never over the bundled file.
        let saved = f.backend.save(&session.theme, None).unwrap();
        assert!(
            Path::new(&saved.location).starts_with(f.root.join("themes")),
            "{}",
            saved.location
        );
    }

    #[test]
    fn the_language_follows_the_system_until_one_is_chosen() {
        let f = fixture("language");
        assert_eq!(f.backend.language(), Language::English);
        let prefs = f.backend.preferences();
        assert_eq!((prefs.language, prefs.system_language), (None, "en"));
        assert_eq!(f.backend.texts().untitled, "Untitled");

        let chosen = f.backend.set_language(Some("pt-BR")).unwrap();
        assert_eq!(chosen, Language::PortugueseBr);
        assert_eq!(f.backend.preferences().language, Some("pt-BR"));
        assert_eq!(f.backend.texts().untitled, "Sem título");
        assert_eq!(f.backend.studio().language(), Language::PortugueseBr);
        let json = std::fs::read_to_string(f.backend.settings.path()).unwrap();
        assert!(json.contains("\"language\": \"pt-BR\""), "{json}");

        let error = f.backend.set_language(Some("klingon")).unwrap_err();
        assert_eq!(error.code(), "unknownLanguage");
        assert_eq!(f.backend.language(), Language::PortugueseBr, "unchanged");

        assert_eq!(f.backend.set_language(None).unwrap(), Language::English);
        assert_eq!(f.backend.preferences().language, None);
        assert_eq!(f.backend.studio().language(), Language::English);
    }

    #[test]
    fn the_gallery_filter_is_checked_and_remembered() {
        let f = fixture("theme-filter");
        let prefs = f.backend.preferences();
        assert_eq!(
            prefs.theme_filter,
            ThemeFilterDto {
                scope: None,
                axis: "all"
            }
        );
        f.backend
            .set_theme_filter(Some("all"), "horizontal")
            .unwrap();
        let filter = f.backend.preferences().theme_filter;
        assert_eq!((filter.scope, filter.axis), (Some("all"), "horizontal"));
        for (scope, axis) in [(Some("mine"), "all"), (None, "diagonal")] {
            let error = f.backend.set_theme_filter(scope, axis).unwrap_err();
            assert_eq!(error.code(), "invalidInput", "{scope:?} {axis}");
        }
        assert_eq!(f.backend.preferences().theme_filter, filter, "unchanged");
        f.backend.set_theme_filter(None, "vertical").unwrap();
        let filter = f.backend.preferences().theme_filter;
        assert_eq!((filter.scope, filter.axis), (None, "vertical"));
    }

    #[test]
    fn library_themes_get_thumbnails_and_name_their_screens() {
        let f = fixture("thumbnails");
        let theme = Theme::blank("Mine", Size::new(480, 1920), Orientation::Landscape);
        let saved = f.backend.save(&ThemeDto::from(&theme), None).unwrap();
        let listed = f.backend.themes();
        let entry = listed
            .iter()
            .find(|e| e.location == saved.location)
            .unwrap();
        assert_eq!(entry.models, vec!["turing-8.8", "turing-usb-8.8"]);
        assert_eq!(entry.diagonal_hundredths, Some(880));
        let first = f.backend.thumbnail(&saved.location, TIME).unwrap();
        assert!(first.unwrap().starts_with("data:image/png;base64,"));
        let kept = || std::fs::read_dir(f.backend.thumbnails.dir()).map_or(0, Iterator::count);
        assert_eq!(kept(), 1);
        // Saving it again through the studio forgets the old thumbnail, and
        // the list says it changed.
        let mut renamed = theme.clone();
        renamed.name = "Mine again".into();
        f.backend.save(&ThemeDto::from(&renamed), None).unwrap();
        assert_eq!(kept(), 0, "forgotten on save");
        let again = f.backend.themes();
        let after = again.iter().find(|e| e.location == saved.location).unwrap();
        assert_ne!(after.revision, entry.revision);
        assert!(
            f.backend
                .thumbnail(&saved.location, TIME)
                .unwrap()
                .is_some()
        );
        assert_eq!(kept(), 1);
        // Only the library's themes.
        let error = f.backend.thumbnail("/etc/passwd", TIME).unwrap_err();
        assert_eq!(error.code(), "notInLibrary");
        // A broken file of the library: no thumbnail, no error.
        let broken = f.root.join("themes").join("broken.bezeltheme");
        std::fs::write(&broken, b"not a zip").unwrap();
        let none = f.backend.thumbnail(&broken.display().to_string(), TIME);
        assert_eq!(none.unwrap(), None);
    }

    #[test]
    fn ping_host_and_mangohud_folder_are_checked_remembered_and_used() {
        let f = fixture("sensor-options");
        let prefs = f.backend.preferences();
        assert_eq!(
            (prefs.ping_host.as_str(), prefs.default_ping_host),
            ("8.8.8.8", "8.8.8.8")
        );
        assert_eq!(prefs.mangohud_dir, None);
        assert_eq!(prefs.mangohud, cfg!(target_os = "linux"));

        let logs = f.root.join("mangohud");
        std::fs::create_dir_all(&logs).unwrap();
        let logs = logs.display().to_string();
        f.backend
            .set_sensor_options(" 1.1.1.1 ", Some(&logs))
            .unwrap();
        let prefs = f.backend.preferences();
        assert_eq!(prefs.ping_host, "1.1.1.1");
        assert_eq!(prefs.mangohud_dir.as_deref(), Some(logs.as_str()));
        let options = f.backend.settings.load().sensor_options();
        assert_eq!(options.mangohud_dir.as_deref(), Some(Path::new(&logs)));
        assert!(
            !f.backend.studio().catalog().is_empty(),
            "the new sensors' catalog"
        );

        for (host, dir, code) in [
            ("-c 5 x", None, "invalidHost"),
            ("a b", None, "invalidHost"),
            ("8.8.8.8", Some("relative/dir"), "invalidFolder"),
            ("8.8.8.8", Some("/no/such/folder"), "invalidFolder"),
        ] {
            let error = f.backend.set_sensor_options(host, dir).unwrap_err();
            assert_eq!(error.code(), code, "{host} {dir:?}");
        }
        assert_eq!(f.backend.preferences().ping_host, "1.1.1.1", "unchanged");
        for host in ["dns.google", "2001:4860:4860::8888", "192.168.0.1"] {
            assert!(is_host(host), "{host}");
        }

        f.backend.set_sensor_options("", None).unwrap();
        let settings = f.backend.settings.load();
        assert_eq!((settings.ping_host, settings.mangohud_dir), (None, None));
        assert_eq!(f.backend.preferences().ping_host, "8.8.8.8");
    }

    #[test]
    fn a_denied_port_comes_with_the_udev_command() {
        let f = fixture("denied");
        let denied = UiError::from(bezel_core::BezelError::AccessDenied {
            address: KEY.into(),
            reason: "Permission denied (os error 13)".into(),
        });
        let explained = f.backend.explain(denied.clone());
        let command = explained.udev_command().unwrap();
        let rule = f.root.join("cache").join("60-bezel.rules");
        let quoted = bezel_devices::udev::shell_quote(&rule.to_string_lossy());
        assert!(
            command.contains(&format!("{quoted} /etc/udev/rules.d/")),
            "{command}"
        );
        assert!(rule.is_file());
        let other = f.backend.explain(UiError::new(ErrorCode::Busy));
        assert_eq!(other.udev_command(), None, "only a denied port");
        let mut elsewhere = fixture("denied-elsewhere");
        elsewhere.backend.udev = None;
        assert_eq!(elsewhere.backend.explain(denied).udev_command(), None);
    }

    #[test]
    fn a_panel_in_desktop_mode_switches_back_only_when_confirmed() {
        let mut f = fixture("desktop-mode");
        let hid = FakeHid::answering(0x88);
        f.backend.hid = Arc::new(hid.clone());
        f.backend.bus = Arc::new(FakeBus::turing_88().and(FakeBus::desktop_mode()));
        let found = f.backend.devices().unwrap();
        assert_eq!((found.screens.len(), found.desktop_mode.len()), (1, 1));
        let key = found.desktop_mode[0].key.clone();

        let refused = f.backend.leave_desktop_mode(&key, Confirm::No).unwrap_err();
        assert_eq!(refused.code(), "notConfirmed");
        assert!(hid.calls().is_empty(), "nothing without Confirm::Yes");

        let done = f.backend.leave_desktop_mode(&key, Confirm::Yes).unwrap();
        assert_eq!(done.model, Some("Turing 8.8\" V1.x (USB)"));
        assert_eq!(hid.calls().len(), 2, "the model query, then the switch");
        let gone = f
            .backend
            .leave_desktop_mode("hid:/dev/hidraw9", Confirm::Yes);
        assert_eq!(gone.unwrap_err().code(), "screenNotFound");
    }

    /// The guide's pages open at their fixed addresses, pages of this
    /// repository; nothing else does.
    #[test]
    fn the_guide_opens_only_at_its_fixed_addresses() {
        let site = "https://github.com/slipalison/bezel/blob/main/";
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for (language, page) in [
            ("en", "docs/user/ffmpeg.md"),
            ("pt-BR", "docs/user/pt-BR/ffmpeg.md"),
        ] {
            assert_eq!(
                guide_url("ffmpeg", language).unwrap(),
                format!("{site}{page}")
            );
            assert!(repo.join(page).is_file(), "{page}");
        }
        for (page, language) in [
            ("ffmpeg", "fr"),
            ("ffmpeg", "pt-br"),
            ("../../etc/passwd", "en"),
            ("https://example.com", "en"),
            ("", "pt-BR"),
        ] {
            let error = guide_url(page, language).unwrap_err();
            assert_eq!(error.code(), "invalidInput", "{page} {language}");
            assert!(error.to_string().contains(page), "{error}");
        }
    }

    #[test]
    fn render_preview_returns_the_canvas_size() {
        let f = fixture("render");
        let theme = f.backend.session().theme;
        let bytes = f
            .backend
            .render(&theme, TIME, Instant::now(), Motion::Allowed)
            .unwrap();
        assert_eq!(&bytes[..8], &[224, 1, 0, 0, 128, 7, 0, 0]);
        assert_eq!(&bytes[8..12], &STILL.to_le_bytes(), "nothing animates");
        let json = serde_json::to_value(f.backend.session()).unwrap();
        assert_eq!(json["minRefreshSeconds"], MIN_REFRESH_SECONDS);
        assert_eq!(bytes.len(), 12 + 480 * 1920 * 4);
        let mut bad = theme.clone();
        bad.orientation = "sideways".into();
        assert!(
            f.backend
                .render(&bad, TIME, Instant::now(), Motion::Allowed)
                .is_err()
        );
    }

    #[test]
    fn live_mode_shows_edits_and_is_remembered() {
        let f = fixture("live");
        assert_eq!(f.backend.devices().unwrap().screens.len(), 1);
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        let theme = f.backend.session().theme;
        f.backend.push(&theme, TIME).unwrap();
        let now = Instant::now();
        let due = f.backend.tick(TIME, now);
        assert!(due >= now + Duration::from_secs_f32(MIN_REFRESH_SECONDS));
        assert_eq!(f.connector.log().frames.len(), 3);
        f.backend.set_brightness(KEY, 40).unwrap();
        assert!(f.backend.set_brightness(KEY, 101).is_err());
        assert_eq!(f.backend.settings.load().live_screen.as_deref(), Some(KEY));

        f.backend.release(KEY).unwrap();
        assert_eq!(f.connector.log().releases, 1);
        assert_eq!(f.backend.sample().live, None);
        assert_eq!(f.backend.settings.load().live_screen, None);
        f.backend.set_live(false, None, TIME).unwrap();
        assert!(f.backend.set_live(true, None, TIME).is_err());
        assert!(f.backend.set_live(true, Some("COM9"), TIME).is_err());
    }

    #[test]
    fn a_hung_screen_restarts_and_shows_the_theme_live_again() {
        // D-2026-09-30-release-polish-13: live mode stops (the port closes),
        // the MCU restarts the screen, live mode comes back on it.
        let f = fixture("restart-live");
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        let frames = f.connector.log().frames.len();
        let done = f.backend.restart_screen(KEY, TIME).unwrap();
        assert_eq!(
            done,
            RestartedDto {
                key: KEY.into(),
                live: true
            }
        );
        assert_eq!(f.connector.log().restarts, [KEY]);
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        assert!(f.connector.log().frames.len() > frames, "a frame at once");
        assert_eq!(f.backend.settings.load().live_screen.as_deref(), Some(KEY));

        // Not live: it only restarts.
        f.backend.set_live(false, None, TIME).unwrap();
        let done = f.backend.restart_screen(KEY, TIME).unwrap();
        assert!(!done.live);
        assert_eq!(f.backend.sample().live, None);
        assert_eq!(f.connector.log().restarts.len(), 2);
        let err = f.backend.restart_screen("COM9", TIME).unwrap_err();
        assert_eq!(err.code(), "screenNotFound");
    }

    #[test]
    fn a_restart_waits_for_storage_and_needs_a_screen_with_an_mcu() {
        let f = fixture("restart-busy");
        let claim = f.backend.storage.claim().unwrap();
        let err = f.backend.restart_screen(KEY, TIME).unwrap_err();
        assert_eq!(err.code(), "busy");
        drop(claim);
        assert!(f.connector.log().restarts.is_empty());

        let mut f = fixture("restart-weact");
        f.backend.bus = Arc::new(FakeBus::new(vec![
            bezel_core::domain::discovery::Endpoint {
                address: bezel_core::domain::discovery::DeviceAddress("/dev/ttyACM0".into()),
                transport: bezel_core::domain::device::Transport::Serial,
                usb: bezel_core::domain::device::UsbId::new(0x1a86, 0xfe0c),
                serial_number: Some("AD0001".into()),
                manufacturer: None,
                product: None,
                location: None,
            },
        ]));
        let err = f.backend.restart_screen("/dev/ttyACM0", TIME).unwrap_err();
        assert_eq!(err.code(), "unsupported");
        assert!(err.to_string().contains("unplug the screen"), "{err}");
        assert!(f.connector.log().restarts.is_empty());
        let screens = f.backend.devices().unwrap().screens;
        assert!(!screens[0].restartable);
    }

    #[test]
    fn the_refresh_loop_sleeps_until_the_next_refresh_is_due() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        assert_eq!(sleep_until(t0 + ms(40), t0), ms(40), "a GIF's next frame");
        assert_eq!(sleep_until(t0, t0 + ms(5)), Duration::ZERO, "already due");
        assert_eq!(sleep_until(t0 + ms(2000), t0), LOOK_AGAIN, "edits are seen");
    }

    /// The fixture's frame header for a preview whose GIF changes in
    /// `ms`: `None` when nothing animates.
    fn next_change(bytes: &[u8]) -> Option<u32> {
        let next = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        (next != STILL).then_some(next)
    }

    /// A GIF of two 100 ms frames.
    fn blinking_gif() -> Vec<u8> {
        use image::codecs::gif::{GifEncoder, Repeat};
        let mut out = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut out);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for red in [0, 255] {
                let pixels = image::RgbaImage::from_pixel(4, 4, image::Rgba([red, 0, 0, 255]));
                let delay = image::Delay::from_numer_denom_ms(100, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(pixels, 0, 0, delay))
                    .unwrap();
            }
        }
        out
    }

    /// T-7.11: the preview says when its GIF changes next, so the UI draws
    /// it at the GIF's pace; a hidden GIF says nothing.
    #[test]
    fn the_preview_says_when_its_gif_changes() {
        let f = fixture("preview-gif");
        std::fs::create_dir_all(&f.root).unwrap();
        let file = f.root.join("blink.gif");
        std::fs::write(&file, blinking_gif()).unwrap();
        let asset = f.backend.add_image(&file).unwrap().reference;
        let mut theme = f.backend.session().theme;
        let element = serde_json::json!({
            "id": 1, "name": "blink", "frame": {"x": 10, "y": 10, "width": 40, "height": 40},
            "opacity": 1, "visible": true, "locked": false,
            "kind": {"type": "image", "asset": asset, "fit": "fill"}
        });
        theme
            .elements
            .push(serde_json::from_value(element).unwrap());
        let bytes = f
            .backend
            .render(&theme, TIME, Instant::now(), Motion::Allowed)
            .unwrap();
        let next = next_change(&bytes).expect("animates");
        assert!((1..=100).contains(&next), "{next} ms");
        theme.elements[0].visible = false;
        let bytes = f
            .backend
            .render(&theme, TIME, Instant::now(), Motion::Allowed)
            .unwrap();
        assert_eq!(next_change(&bytes), None);
    }

    /// T-7.11: a live screen whose link failed (it hung) is connected again
    /// after 2 s by the refresh loop and shows the theme again, a whole frame
    /// on the new link; the UI hears about it meanwhile.
    #[test]
    fn a_live_screen_that_hangs_comes_back_by_itself() {
        let mut f = fixture("reconnect");
        let connector = FakeConnector::default()
            .breaking_after(1, bezel_core::BezelError::Hung("stalled".into()));
        f.backend.connector = Arc::new(connector.clone());
        f.connector = connector;
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        let later = |ms| Instant::now() + Duration::from_millis(ms);
        f.backend.tick(TIME, later(1_000));
        let sample = f.backend.sample();
        assert_eq!(sample.live.as_deref(), Some(KEY), "still live");
        assert_eq!(sample.live_error, None);
        let waiting = sample.reconnecting.expect("reconnecting");
        assert_eq!((waiting.attempt, waiting.attempts), (1, 3));
        let json = serde_json::to_value(&sample).unwrap();
        assert_eq!(json["reconnecting"]["attempt"], 1);
        // The link is gone meanwhile: whoever needs it hears why.
        let busy = f.backend.set_brightness(KEY, 30).unwrap_err();
        assert!(
            busy.to_string().contains(crate::studio::RECONNECTING),
            "{busy}"
        );

        f.backend.tick(TIME, later(500));
        assert_eq!(f.connector.log().connects, 1, "not before 2 s");
        f.backend.tick(TIME, later(3_000));
        let sample = f.backend.sample();
        assert_eq!(sample.reconnecting, None);
        assert_eq!(sample.live.as_deref(), Some(KEY));
        let log = f.connector.log();
        assert_eq!((log.connects, log.frames.len()), (2, 2), "a frame at once");
        assert_eq!(log.orientations.len(), 2, "turned again");
        assert_eq!(f.backend.settings.load().live_screen.as_deref(), Some(KEY));
    }

    /// After three attempts live mode stops with the error that stopped the
    /// link (a hung screen's card offers the restart); turning live mode off
    /// while the screen is away ends the attempts at once.
    #[test]
    fn reconnecting_gives_up_after_three_attempts_or_when_stopped() {
        let hung = || bezel_core::BezelError::Hung("stalled".into());
        let mut f = fixture("reconnect-gone");
        let connector = FakeConnector::default().breaking_after(1, hung());
        f.backend.connector = Arc::new(connector.clone());
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        let later = |ms| Instant::now() + Duration::from_millis(ms);
        f.backend.bus = Arc::new(FakeBus::new(Vec::new()));
        f.backend.tick(TIME, later(0));
        for (wait, attempt) in [(3_000, 2), (6_000, 3)] {
            f.backend.tick(TIME, later(wait));
            let sample = f.backend.sample();
            assert_eq!(sample.reconnecting.map(|r| r.attempt), Some(attempt));
        }
        f.backend.tick(TIME, later(11_000));
        let sample = f.backend.sample();
        assert_eq!((sample.live, sample.reconnecting), (None, None));
        assert_eq!(sample.live_error.unwrap().code(), "hung");
        assert_eq!(connector.log().connects, 1, "the screen never came back");

        let mut f = fixture("reconnect-stop");
        let connector = FakeConnector::default().breaking_after(1, hung());
        f.backend.connector = Arc::new(connector.clone());
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        f.backend.tick(TIME, later(0));
        assert!(f.backend.sample().reconnecting.is_some());
        f.backend.set_live(false, None, TIME).unwrap();
        f.backend.tick(TIME, later(3_000));
        let sample = f.backend.sample();
        assert_eq!((sample.live, sample.reconnecting), (None, None));
        assert_eq!(sample.live_error, None);
        assert_eq!(connector.log().connects, 1, "no attempt after the stop");
    }

    /// A link whose frames wait until the test lets them through, telling
    /// when one arrived: the screen's I/O in slow motion.
    struct Held {
        inner: Box<dyn ScreenLink>,
        arrived: mpsc::Sender<()>,
        through: mpsc::Receiver<()>,
    }

    impl ScreenLink for Held {
        fn identity(&self) -> &bezel_core::domain::screen::ScreenIdentity {
            self.inner.identity()
        }
        fn set_brightness(&mut self, brightness: Brightness) -> bezel_core::Result<()> {
            self.inner.set_brightness(brightness)
        }
        fn set_orientation(&mut self, orientation: Orientation) -> bezel_core::Result<()> {
            self.inner.set_orientation(orientation)
        }
        fn present(&mut self, frame: &bezel_core::domain::frame::Frame) -> bezel_core::Result<()> {
            self.arrived.send(()).unwrap();
            self.through.recv().unwrap();
            self.inner.present(frame)
        }
        fn screen_off(&mut self) -> bezel_core::Result<()> {
            self.inner.screen_off()
        }
        fn release(&mut self) -> bezel_core::Result<()> {
            self.inner.release()
        }
    }

    #[test]
    fn previews_render_while_the_screen_shows_a_frame() {
        let f = fixture("held");
        let (arrived, frame_arrived) = mpsc::channel();
        let (let_through, through) = mpsc::channel();
        let inner = f.backend.connect(KEY).unwrap();
        let held = Held {
            inner,
            arrived,
            through,
        };
        f.backend.studio().go_live(KEY.into(), Box::new(held));
        let theme = f.backend.session().theme;
        let backend = &f.backend;
        std::thread::scope(|scope| {
            let started = Instant::now();
            let ticking = scope.spawn(|| backend.tick(TIME, Instant::now()));
            frame_arrived.recv().unwrap();
            // The screen is busy with a frame: the session is not.
            assert!(
                backend
                    .render(&theme, TIME, Instant::now(), Motion::Allowed)
                    .is_ok()
            );
            assert_eq!(backend.sample().live.as_deref(), Some(KEY));
            // Whoever needs the link waits for it.
            let dimming = scope.spawn(|| backend.set_brightness(KEY, 30));
            std::thread::sleep(Duration::from_millis(50));
            assert!(f.connector.log().brightness.is_empty(), "after the frame");
            let_through.send(()).unwrap();
            assert!(ticking.join().unwrap() > started);
            dimming.join().unwrap().unwrap();
        });
        let log = f.connector.log();
        assert_eq!((log.frames.len(), log.brightness.len()), (1, 1));
    }

    const DRAGON: &str = "assets/dragon.mp4";
    const DRAGON_POSTER: &str = "assets/poster-195.png";
    /// Longest wait for a held call of the converter to arrive (it arrives
    /// at once; this only keeps a broken test from hanging).
    const ARRIVAL: Duration = Duration::from_secs(30);

    /// Live on the fake 8.8" (a blank theme first), then editing the Dragon
    /// Ball theme: 1920x480, its video the vendor's pre-turned 480x1920
    /// `dragon.mp4` (not probed yet) with its poster. The session probes and
    /// takes posters with `media`, the storage tab's converter, as in the
    /// app.
    fn dragon_live(name: &str, media: FakeMedia) -> (crate::storage::tests::Fixture, Theme) {
        let copies = Copies::in_memory(MemoryArchive::new());
        let f = fixture_on(
            name,
            FakeBus::turing_88(),
            FakeStorage::default(),
            media,
            copies,
        );
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        let mut theme = Theme::blank("Dragon Ball", Size::new(480, 1920), Orientation::Landscape);
        theme.background = Background::Video {
            asset: AssetRef(DRAGON.into()),
            poster: Some(AssetRef(DRAGON_POSTER.into())),
            framing: None,
        };
        let poster = Frame::filled(Size::new(1920, 480), Rgba::opaque(9, 9, 9));
        let assets = BTreeMap::from([
            (AssetRef(DRAGON.into()), vec![9; 4096]),
            (
                AssetRef(DRAGON_POSTER.into()),
                crate::media::png_of(&poster).unwrap(),
            ),
        ]);
        f.backend.studio().start(theme.clone(), assets, None);
        (f, theme)
    }

    /// When a preview ([`frame_bytes`]) changes next, milliseconds.
    fn next_ms(preview: &[u8]) -> u32 {
        u32::from_le_bytes(preview[8..12].try_into().unwrap())
    }

    /// While a call holds the converter, the session goes on: its lock is
    /// free, the refresh `ms` from now draws the live screen's frame, a push
    /// shows one at once, and the preview renders `theme` (its poster, asked
    /// again soon).
    fn the_session_goes_on(f: &crate::storage::tests::Fixture, theme: &ThemeDto, ms: u64) {
        assert!(
            f.backend.studio.studio.try_lock().is_ok(),
            "the lock is free"
        );
        let frames = || f.connector.log().frames.len();
        let before = frames();
        f.backend
            .tick(TIME, Instant::now() + Duration::from_millis(ms));
        assert_eq!(frames(), before + 1, "the refresh drew a frame");
        f.backend.push(theme, TIME).unwrap();
        assert_eq!(frames(), before + 2, "a push shows one at once");
        let preview = f
            .backend
            .render(theme, TIME, Instant::now(), Motion::Allowed)
            .unwrap();
        let busy = u32::try_from(CONVERTER_BUSY.as_millis()).unwrap();
        assert_eq!(next_ms(&preview), busy, "the poster, asked again soon");
    }

    /// Review W2: probing the theme's video (ffprobe, for a container that
    /// is not MP4 or GIF) runs outside the session's lock; the live screen
    /// starts the video once it is probed.
    #[test]
    fn the_session_goes_on_while_the_video_is_probed() {
        let (media, arrivals, go) = FakeMedia::ready().holding(Call::Probe);
        let (f, theme) = dragon_live("probing", media);
        let dto = ThemeDto::from(&theme);
        let backend = &f.backend;
        std::thread::scope(|scope| {
            // Dropped if the test fails: the held probe goes on.
            let go = go;
            let rendering =
                scope.spawn(|| backend.render(&dto, TIME, Instant::now(), Motion::Allowed));
            assert_eq!(arrivals.recv_timeout(ARRIVAL), Ok(Call::Probe));
            the_session_goes_on(&f, &dto, 60_000);
            let video = backend.sample().video.unwrap();
            assert_eq!(video.state, "notStarted", "not before the probe");
            go.send(()).unwrap();
            let preview = rendering.join().unwrap().unwrap();
            assert_eq!(next_ms(&preview), 67, "the video plays");
        });
        let auto = f.backend.video_auto(&dto).unwrap();
        let size = auto.size.map(|s| (s.width, s.height));
        assert_eq!((auto.rotation, size), (270, Some((480, 1920))));
        f.backend
            .tick(TIME, Instant::now() + Duration::from_secs(120));
        let video = f.backend.sample().video.unwrap();
        assert_eq!(
            (video.state, video.path.as_deref()),
            ("missing", Some("internal/video/dragon.mp4"))
        );
    }

    /// Review W2: the poster taken again on save (ffmpeg, up to 30 s) runs
    /// outside the session's lock; it reaches the saved theme.
    #[test]
    fn the_session_goes_on_while_the_poster_is_taken() {
        let (media, arrivals, go) = FakeMedia::ready().holding(Call::Poster);
        let (f, mut theme) = dragon_live("posters", media);
        // The refresh probes the video for the live screen.
        f.backend
            .tick(TIME, Instant::now() + Duration::from_secs(60));
        assert_eq!(f.backend.sample().video.unwrap().state, "missing");
        let poster = AssetRef(DRAGON_POSTER.into());
        let before = f.backend.studio().assets()[&poster].clone();
        if let Background::Video { framing, .. } = &mut theme.background {
            *framing = Some(VideoFraming {
                fit: VideoFit::Contain,
                zoom: Zoom::from_percent(125),
                ..VideoFraming::default()
            });
        }
        let dto = ThemeDto::from(&theme);
        let backend = &f.backend;
        let saved = std::thread::scope(|scope| {
            // Dropped if the test fails: the held poster goes on.
            let go = go;
            let saving = scope.spawn(|| backend.save(&dto, None));
            assert_eq!(arrivals.recv_timeout(ARRIVAL), Ok(Call::Poster));
            the_session_goes_on(&f, &dto, 120_000);
            go.send(()).unwrap();
            saving.join().unwrap().unwrap()
        });
        let taken = f.backend.studio().assets()[&poster].clone();
        assert_ne!(taken, before, "taken again");
        let (_, assets) = FsThemeStore.load(&ThemeLocation(saved.location)).unwrap();
        assert_eq!(assets[&poster], taken, "saved with the theme");
        let picture = image::load_from_memory(&taken).unwrap().to_rgba8();
        assert_eq!(picture.dimensions(), (1920, 480));
    }

    #[test]
    fn the_tray_switches_live_mode_on_the_connected_screen() {
        let f = fixture("tray");
        assert!(f.backend.toggle_live(TIME).unwrap());
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        assert_eq!(f.connector.log().frames.len(), 1);
        assert!(!f.backend.toggle_live(TIME).unwrap());
        assert_eq!(f.backend.sample().live, None);
        assert_eq!(f.backend.settings.load().live_screen, None);

        let mut empty = fixture("tray-empty");
        empty.backend.bus = Arc::new(FakeBus::new(Vec::new()));
        assert!(empty.backend.toggle_live(TIME).is_err());
        assert_eq!(empty.backend.sample().live, None);
    }

    #[test]
    fn brightness_and_release_open_a_screen_that_is_not_live() {
        let f = fixture("offline");
        f.backend.set_brightness(KEY, 10).unwrap();
        f.backend.release(KEY).unwrap();
        let log = f.connector.log();
        assert_eq!((log.brightness.len(), log.releases), (1, 1));
    }

    /// The fake 8.8"'s MCU (its wake chip): the screen's other port.
    const MCU: &str = "/dev/ttyACM0";

    /// What reopening a port this app already holds answers on Linux.
    fn busy() -> bezel_core::BezelError {
        bezel_core::BezelError::Transport(format!("{KEY}: Device or resource busy"))
    }

    /// The fake 8.8" asleep: its MCU listed, its display not.
    fn asleep() -> Vec<bezel_core::domain::discovery::Endpoint> {
        let all = FakeBus::turing_88().endpoints().unwrap();
        all.into_iter().filter(|e| e.address.0 == MCU).collect()
    }

    /// The fake 8.8" asleep until the connector's first connection wakes
    /// its display.
    struct Waking(FakeConnector);

    impl DeviceBus for Waking {
        fn endpoints(&self) -> bezel_core::Result<Vec<bezel_core::domain::discovery::Endpoint>> {
            if self.0.log().connects == 0 {
                return Ok(asleep());
            }
            FakeBus::turing_88().endpoints()
        }
    }

    /// D-2026-10-01-live-screen-controls-2: put live by its MCU's port (the
    /// tray finds a sleeping 8.8" by it), the screen goes live under its
    /// display's key, remembered with its orientation; the MCU's port only
    /// while the display cannot be listed.
    #[test]
    fn a_screen_put_live_by_its_mcu_port_is_keyed_by_its_display() {
        let f = fixture("live-by-mcu");
        f.backend.set_live(true, Some(MCU), TIME).unwrap();
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        let settings = f.backend.settings.load();
        assert_eq!(settings.live_screen.as_deref(), Some(KEY));
        let turned = Some(Orientation::ReversePortrait);
        assert_eq!(settings.orientation_for(KEY), turned);
        assert_eq!(settings.orientation_for(MCU), None);
        assert_eq!(f.connector.log().connects, 1);

        // The tray on a sleeping 8.8": the connection wakes its display.
        let mut tray = fixture("live-by-mcu-tray");
        tray.backend.bus = Arc::new(Waking(tray.connector.clone()));
        assert!(tray.backend.toggle_live(TIME).unwrap());
        assert_eq!(tray.backend.sample().live.as_deref(), Some(KEY));
        let saved = tray.backend.settings.load().live_screen;
        assert_eq!(saved.as_deref(), Some(KEY));

        // Still asleep once connected: its MCU's port is all there is.
        let mut sleepy = fixture("live-by-mcu-asleep");
        sleepy.backend.bus = Arc::new(FakeBus::new(asleep()));
        sleepy.backend.set_live(true, Some(MCU), TIME).unwrap();
        assert_eq!(sleepy.backend.sample().live.as_deref(), Some(MCU));
        let saved = sleepy.backend.settings.load().live_screen;
        assert_eq!(saved.as_deref(), Some(MCU));
    }

    /// Writes down, in order, what a screen's ports see: the links `inner`
    /// opens and that close, and the restarts.
    #[derive(Clone, Default)]
    struct Tracked {
        inner: FakeConnector,
        seen: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Tracked {
        fn note(&self, what: &'static str) {
            self.seen.lock().unwrap().push(what);
        }
    }

    /// A link of [`Tracked`], noting when it closes.
    struct TrackedLink(Box<dyn ScreenLink>, Tracked);

    impl Drop for TrackedLink {
        fn drop(&mut self) {
            self.1.note("close");
        }
    }

    impl ScreenLink for TrackedLink {
        fn identity(&self) -> &bezel_core::domain::screen::ScreenIdentity {
            self.0.identity()
        }
        fn set_brightness(&mut self, brightness: Brightness) -> bezel_core::Result<()> {
            self.0.set_brightness(brightness)
        }
        fn set_orientation(&mut self, orientation: Orientation) -> bezel_core::Result<()> {
            self.0.set_orientation(orientation)
        }
        fn present(&mut self, frame: &bezel_core::domain::frame::Frame) -> bezel_core::Result<()> {
            self.0.present(frame)
        }
        fn screen_off(&mut self) -> bezel_core::Result<()> {
            self.0.screen_off()
        }
        fn release(&mut self) -> bezel_core::Result<()> {
            self.0.release()
        }
    }

    impl ScreenConnector for Tracked {
        fn connect(&self, screen: &Screen) -> bezel_core::Result<Box<dyn ScreenLink>> {
            let link = self.inner.connect(screen)?;
            self.note("open");
            Ok(Box::new(TrackedLink(link, self.clone())))
        }
        fn restart(&self, screen: &Screen) -> bezel_core::Result<()> {
            self.inner.restart(screen)?;
            self.note("restart");
            Ok(())
        }
    }

    /// D-2026-10-01-live-screen-controls-3: live on the 8.8" (the UI names
    /// it by its display), either of its ports reaches the open live link:
    /// brightness, the storage tab and release go through it and never open
    /// the screen again (which would answer busy); a restart by the MCU's
    /// port stops live mode before the screen restarts and resumes it under
    /// the display's key.
    #[test]
    fn either_port_of_the_live_screen_reaches_its_live_link() {
        let f = fixture("either-port");
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        let _held = f.connector.clone().refusing_after(1, vec![busy()]);
        for port in [MCU, KEY] {
            f.backend.set_brightness(port, 40).unwrap();
            let overview = f.backend.storage_overview(port, TIME).unwrap();
            assert!(overview.internal.total > 0, "{port}");
        }
        f.backend.release(MCU).unwrap();
        let log = f.connector.log();
        assert_eq!(
            (log.connects, log.brightness.len(), log.releases),
            (1, 2, 1)
        );
        assert_eq!(f.backend.sample().live, None);
        assert_eq!(f.backend.settings.load().live_screen, None);

        let mut f = fixture("either-port-restart");
        let tracked = Tracked {
            inner: f.connector.clone(),
            ..Tracked::default()
        };
        f.backend.connector = Arc::new(tracked.clone());
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        let done = f.backend.restart_screen(MCU, TIME).unwrap();
        assert_eq!(
            done,
            RestartedDto {
                key: KEY.into(),
                live: true
            }
        );
        let seen = tracked.seen.lock().unwrap().clone();
        assert_eq!(seen, ["open", "close", "restart", "open"]);
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        assert_eq!(f.backend.settings.load().live_screen.as_deref(), Some(KEY));
    }

    /// D-2026-10-01-live-screen-controls-2: a settings file that kept the
    /// live screen by its MCU's port (0.1.0-dev.287) brings it back live
    /// under its display's key and is rewritten with it: the UI's
    /// brightness, by the display, then goes through the live link.
    #[test]
    fn a_saved_mcu_key_is_restored_under_the_display_key() {
        let f = fixture("restore-mcu");
        f.backend
            .settings
            .update(|s| s.live_screen = Some(MCU.into()));
        f.backend.restore_live(TIME);
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        let settings = f.backend.settings.load();
        assert_eq!(settings.live_screen.as_deref(), Some(KEY));
        assert_eq!(settings.orientation_for(MCU), None);
        let _held = f.connector.clone().refusing_after(1, vec![busy()]);
        f.backend.set_brightness(KEY, 60).unwrap();
        let log = f.connector.log();
        assert_eq!((log.connects, log.brightness.len()), (1, 1));
    }

    #[test]
    fn sensors_reach_the_ui() {
        let f = fixture("sensors");
        let catalog = f.backend.catalog().unwrap();
        assert!(catalog.iter().any(|s| s.key == "cpu.usage"));
        f.backend.tick(TIME, Instant::now());
        let sample = f.backend.sample();
        assert!(sample.readings.contains_key("cpu.usage"));
        assert_eq!(sample.live_error, None);
    }

    #[test]
    fn the_sensor_list_says_what_it_shows() {
        let f = fixture("sensor-list");
        f.backend.restore_theme();
        let theme = f.backend.studio().wanted().clone();
        assert!(!theme.contains("net.ping"), "nothing shows the ping");
        f.backend
            .show_sensors(&["net.ping".to_string(), "not a key".to_string()]);
        let listed = f.backend.studio().wanted().clone();
        let ping: Wanted = SensorKey::new("net.ping").into_iter().collect();
        assert_eq!(listed, theme.union(&ping));
        f.backend.show_sensors(&[]);
        assert_eq!(f.backend.studio().wanted(), &theme, "the list is hidden");
    }

    #[test]
    fn themes_save_list_open_and_restore() {
        let f = fixture("themes");
        let mut theme = f
            .backend
            .new_theme(Some(KEY), "Mine", Some(Orientation::ReversePortrait))
            .unwrap();
        assert_eq!((theme.canvas.width, theme.canvas.height), (480, 1920));
        theme.refresh_seconds = 2.0;
        let saved = f.backend.save(&theme, None).unwrap();
        assert!(
            saved.location.ends_with("Mine.bezeltheme"),
            "{}",
            saved.location
        );
        let again = f.backend.save(&theme, None).unwrap();
        assert_eq!(again.location, saved.location, "saving again overwrites");
        let listed = f.backend.themes();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Mine");
        assert_eq!(listed[0].orientation, "reverse-portrait");

        f.backend.new_theme(None, "Other", None).unwrap();
        assert_eq!(
            f.backend.open(&saved.location).unwrap().refresh_seconds,
            2.0
        );
        assert!(f.backend.open("/nope.bezeltheme").is_err());

        f.backend.new_theme(None, "Scratch", None).unwrap();
        f.backend
            .settings
            .update(|s| s.live_screen = Some(KEY.into()));
        f.backend.restore_theme();
        f.backend.restore_live(TIME);
        assert_eq!(f.backend.session().theme.name, "Mine");
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
    }

    #[test]
    fn the_window_opens_and_saves_only_where_allowed() {
        let f = fixture("allowed");
        let theme = f.backend.session().theme;
        let outside = f.root.join("Desktop").join("Mine.bezeltheme");
        let at = ThemeLocation(outside.display().to_string());
        FsThemeStore
            .save(&at, &theme_of(&theme).unwrap(), &Default::default())
            .unwrap();
        let sneaky = f.root.join("themes").join("..").join("Desktop");
        for refused in [
            outside.display().to_string(),
            sneaky.join("Mine.bezeltheme").display().to_string(),
            "/etc/passwd".into(),
        ] {
            let error = f.backend.open(&refused).unwrap_err();
            assert_eq!(error.code(), "notInLibrary", "{error}");
            assert_eq!(error.value("location"), Some(refused.as_str()));
        }
        let error = f.backend.save(&theme, Some(at.clone())).unwrap_err();
        assert_eq!(error.code(), "notPicked", "{error}");
        assert!(error.to_string().contains("not picked"), "{error}");
        assert!(!outside.with_file_name("theme.json").exists());

        // A theme from elsewhere (an import keeps no location; say it had
        // one) is saved as a copy in the user folder.
        f.backend.studio().start(
            theme_of(&theme).unwrap(),
            Default::default(),
            Some(at.clone()),
        );
        let copy = f.backend.save(&theme, None).unwrap().location;
        assert!(
            Path::new(&copy).starts_with(f.root.join("themes")),
            "{copy}"
        );

        // Picked in a dialog: open and save reach it for the session, and
        // the next start reopens it and saves back to it.
        f.backend.library.grant(&at);
        f.backend.open(&at.0).unwrap();
        assert_eq!(f.backend.save(&theme, None).unwrap().location, at.0);
        let next = fixture("allowed-next");
        next.backend
            .settings
            .update(|s| s.last_theme = Some(at.0.clone()));
        next.backend.restore_theme();
        assert_eq!(
            next.backend.session().location.as_deref(),
            Some(at.0.as_str())
        );
        assert_eq!(next.backend.save(&theme, None).unwrap().location, at.0);
    }

    #[test]
    fn new_themes_follow_the_screen_shape_then_the_last_orientation_used() {
        let f = fixture("orientation");
        let model = model_by_id(DEFAULT_MODEL).unwrap();
        assert_eq!(
            default_orientation(model),
            Orientation::Landscape,
            "8.8\" bar"
        );
        let square = model_by_id(bezel_core::domain::device::ModelId("turing-2.1")).unwrap();
        assert_eq!(default_orientation(square), square.native_orientation);
        let five = model_by_id(bezel_core::domain::device::ModelId("usbpcmonitor-5")).unwrap();
        assert_eq!(
            default_orientation(five),
            Orientation::Portrait,
            "5:3 is no bar"
        );

        let first = f.backend.new_theme(Some(KEY), "A", None).unwrap();
        assert_eq!(first.orientation, "landscape");
        assert_eq!((first.canvas.width, first.canvas.height), (1920, 480));
        let chosen = f
            .backend
            .new_theme(Some(KEY), "B", Some(Orientation::ReversePortrait))
            .unwrap();
        assert_eq!((chosen.canvas.width, chosen.canvas.height), (480, 1920));
        let next = f.backend.new_theme(Some(KEY), "C", None).unwrap();
        assert_eq!(
            next.orientation, "reverse-portrait",
            "remembered for the screen"
        );
        let elsewhere = f.backend.new_theme(None, "D", None).unwrap();
        assert_eq!(
            elsewhere.orientation, "landscape",
            "no screen: the 8.8\" rule"
        );
        let unplugged = f
            .backend
            .new_theme(Some("COM9"), "E", Some(Orientation::Portrait))
            .unwrap();
        assert_eq!(
            (unplugged.canvas.width, unplugged.canvas.height),
            (480, 1920)
        );
        assert_eq!(
            f.backend.settings.load().orientation_for("COM9"),
            Some(Orientation::Portrait)
        );

        // With no last theme the app starts with a blank theme for the first
        // screen, in the orientation last used with it.
        f.backend.restore_theme();
        let start = f.backend.session();
        assert_eq!(
            (start.theme.name.as_str(), start.theme.orientation.as_str()),
            ("Untitled", "reverse-portrait")
        );
        assert_eq!(start.location, None);
        f.backend
            .settings
            .update(|s| s.last_theme = Some("/gone.bezeltheme".into()));
        f.backend.restore_theme();
        assert_eq!(
            f.backend.session().theme.name,
            "Untitled",
            "an unreadable last theme"
        );
    }

    #[test]
    fn live_mode_follows_a_turn_to_the_other_orientation() {
        let f = fixture("turn");
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        assert_eq!(
            f.backend.settings.load().orientation_for(KEY),
            Some(Orientation::ReversePortrait)
        );
        let mut wide = f.backend.session().theme;
        wide.orientation = "landscape".into();
        wide.canvas = bezel_themes::dto::SizeDto {
            width: 1920,
            height: 480,
        };
        f.backend.push(&wide, TIME).unwrap();
        let log = f.connector.log();
        assert_eq!(
            log.orientations,
            vec![Orientation::ReversePortrait, Orientation::Landscape]
        );
        assert_eq!(log.frames.len(), 2);
        assert_eq!(log.frames[0].size(), Size::new(480, 1920));
        assert_eq!(log.frames[1].size(), Size::new(1920, 480));
        assert_eq!(
            f.backend.settings.load().orientation_for(KEY),
            Some(Orientation::Landscape)
        );
        assert_eq!(
            f.backend
                .new_theme(Some(KEY), "Next", None)
                .unwrap()
                .orientation,
            "landscape"
        );
        // Not live: a push shows nothing and remembers nothing.
        f.backend.set_live(false, None, TIME).unwrap();
        let mut tall = wide.clone();
        tall.orientation = "portrait".into();
        tall.canvas = bezel_themes::dto::SizeDto {
            width: 480,
            height: 1920,
        };
        f.backend.push(&tall, TIME).unwrap();
        assert_eq!(f.connector.log().frames.len(), 2);
        assert_eq!(
            f.backend.settings.load().orientation_for(KEY),
            Some(Orientation::Landscape)
        );
    }

    /// A small turing-smart-screen-python theme; the LED color has no
    /// equivalent in a Bezel theme.
    const TINY_PYTHON_THEME: &str = r#"---
display:
  DISPLAY_SIZE: 3.5"
  DISPLAY_ORIENTATION: landscape
  DISPLAY_RGB_LED: 0, 120, 255
static_text:
  LABEL:
    TEXT: "CPU"
    X: 20
    Y: 18
    FONT_SIZE: 18
    FONT_COLOR: 255, 255, 255
    BACKGROUND_COLOR: 0, 0, 0
"#;

    /// The theme laid out like the Python repository (`res/themes/<name>`).
    fn python_theme(root: &Path) -> PathBuf {
        let dir = root.join("res/themes/Tiny");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("theme.yaml"), TINY_PYTHON_THEME).unwrap();
        dir
    }

    #[test]
    fn a_theme_json_opens_its_folder_like_the_cli() {
        let f = fixture("import-manifest");
        let folder = f.root.join("Manifest theme");
        let target = ThemeLocation(folder.display().to_string());
        f.backend.library.grant(&target);
        f.backend
            .save(&f.backend.session().theme, Some(target))
            .unwrap();
        let imported = f.backend.import(&folder.join("theme.json")).unwrap();
        assert_eq!(imported.theme.name, "Start");
        assert!(imported.warnings.is_empty());
    }

    #[test]
    fn imports_native_and_other_apps_themes() {
        let f = fixture("import");
        let native = f
            .backend
            .save(&f.backend.session().theme, None)
            .unwrap()
            .location;
        let imported = f.backend.import(Path::new(&native)).unwrap();
        assert_eq!(imported.theme.name, "Start");
        assert!(imported.warnings.is_empty());
        assert_eq!(f.backend.session().location, None, "a copy, not the file");
        let folder = f.root.join("Folder theme");
        let target = ThemeLocation(folder.display().to_string());
        // Picked in the save dialog.
        f.backend.library.grant(&target);
        f.backend
            .save(&f.backend.session().theme, Some(target))
            .unwrap();
        assert!(folder.join("theme.json").is_file());
        let imported = f.backend.import(&folder).unwrap();
        assert_eq!(imported.theme.name, "Start");
        assert!(imported.warnings.is_empty());

        let dir = python_theme(&f.root);
        for path in [dir.clone(), dir.join("theme.yaml")] {
            let imported = f.backend.import(&path).unwrap();
            assert_eq!(imported.theme.name, "Tiny", "{}", path.display());
            assert_eq!(imported.theme.orientation, "landscape");
            assert_eq!(
                (imported.theme.canvas.width, imported.theme.canvas.height),
                (480, 320)
            );
            assert!(
                imported
                    .warnings
                    .iter()
                    .any(|w| w.code == "backplateLed" && w.message.contains("LED")),
                "{:?}",
                imported.warnings
            );
            assert_eq!(f.backend.session().theme.name, "Tiny");
        }
        let json = serde_json::to_value(f.backend.import(&dir).unwrap()).unwrap();
        assert!(json["warnings"].as_array().is_some_and(|w| !w.is_empty()));
        assert_eq!(json["warnings"][0]["code"], "backplateLed");

        let junk = f.root.join("notes.turtheme");
        std::fs::write(&junk, b"not a theme").unwrap();
        assert!(f.backend.import(&junk).is_err());
        assert!(f.backend.import(&f.root.join("missing.yaml")).is_err());
        assert_eq!(
            f.backend.session().theme.name,
            "Tiny",
            "a failed import keeps the theme"
        );
    }

    #[test]
    fn images_are_checked_and_previewed() {
        let f = fixture("media");
        std::fs::create_dir_all(&f.root).unwrap();
        let png = f.root.join("Logo Final.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .save(&png)
            .unwrap();
        let text = f.root.join("notes.txt");
        std::fs::write(&text, b"hello").unwrap();
        assert_eq!(
            f.backend.add_image(&png).unwrap().reference,
            "assets/logo-final.png"
        );
        assert!(f.backend.add_image(&text).is_err());
        assert!(f.backend.add_image(&f.root.join("missing.png")).is_err());
        let assets = f.backend.assets();
        assert_eq!(assets.len(), 1);
        assert!(
            assets[0]
                .data_url
                .as_deref()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(f.backend.fonts, vec!["Inter".to_string()]);
        // An animated GIF is an image that can be a video background: it
        // comes with its poster.
        let gif = f.root.join("Ondas.gif");
        std::fs::write(&gif, crate::media::tests::gif(3)).unwrap();
        let added = f.backend.add_image(&gif).unwrap();
        assert_eq!(added.reference, "assets/ondas.gif");
        let listed = f.backend.assets();
        let ondas = listed.iter().find(|a| a.reference == "assets/ondas.gif");
        let ondas = ondas.unwrap();
        assert!(ondas.animated && ondas.data_url.is_some());
        assert_eq!(ondas.poster.as_deref(), Some("assets/ondas-poster.png"));
    }
}
