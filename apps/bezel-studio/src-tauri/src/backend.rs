//! What each UI command does, without Tauri: the commands module only adds
//! threads, dialogs and IPC around these methods, so they run on fakes in
//! tests. Errors reach the UI as text.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bezel_core::app::{choose_screen, discover_screens};
use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::Screen;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::screen::Brightness;
use bezel_core::domain::theme::Theme;
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink, ThemeLocation, ThemeStore};
use bezel_themes::dto::ThemeDto;
use bezel_themes::import::import_path;

use crate::dto::{
    AddedDto, AssetDto, ImportedDto, LiveVideoDto, SampleDto, SavedDto, ScreenDto, SensorDto,
    SessionDto, ThemeEntryDto,
};
use crate::library::{ThemeLibrary, is_native_theme};
use crate::media::{kind_of, thumbnail_data_url};
use crate::settings::SettingsFile;
use crate::storage::StorageState;
use crate::studio::Studio;

/// Result of a UI command: errors are shown as text.
pub type UiResult<T> = Result<T, String>;

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Largest file accepted as an image or theme, bytes.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// The ports and state behind the window.
pub struct Backend {
    /// Where screens are discovered.
    pub bus: Arc<dyn DeviceBus + Send + Sync>,
    /// How screens are opened.
    pub connector: Arc<dyn ScreenConnector + Send + Sync>,
    /// Theme files.
    pub store: Arc<dyn ThemeStore + Send + Sync>,
    /// The theme folders.
    pub library: ThemeLibrary,
    /// Remembered choices.
    pub settings: SettingsFile,
    /// Font families themes can use.
    pub fonts: Vec<String>,
    /// The editing session.
    pub studio: Mutex<Studio>,
    /// The screen's files: the media converter and the running operation.
    pub storage: StorageState,
}

/// Header of a frame sent to the UI: width and height, u32 little-endian.
pub fn frame_bytes(frame: &bezel_core::domain::frame::Frame) -> Vec<u8> {
    let size = frame.size();
    let mut out = Vec::with_capacity(8 + frame.as_rgba().len());
    out.extend_from_slice(&size.width.to_le_bytes());
    out.extend_from_slice(&size.height.to_le_bytes());
    out.extend_from_slice(frame.as_rgba());
    out
}

fn theme_of(dto: &ThemeDto) -> UiResult<Theme> {
    Theme::try_from(dto).map_err(|e| e.0)
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

fn read_file(path: &Path) -> UiResult<Vec<u8>> {
    let size = std::fs::metadata(path).map_err(text)?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{} is {} MiB; the limit is {} MiB",
            path.display(),
            size / (1024 * 1024),
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    std::fs::read(path).map_err(text)
}

impl Backend {
    /// The editing session, even after a panic in another command.
    pub fn studio(&self) -> MutexGuard<'_, Studio> {
        self.studio.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn find_screen(&self, key: &str) -> UiResult<Screen> {
        choose_screen(
            discover_screens(self.bus.as_ref()).map_err(text)?,
            Some(key),
        )
        .map_err(text)
    }

    pub(crate) fn connect(&self, key: &str) -> UiResult<Box<dyn ScreenLink>> {
        let screen = self.find_screen(key)?;
        self.connector.connect(&screen).map_err(text)
    }

    // ------------------------------------------------------------ screens --

    /// The connected screens.
    pub fn screens(&self) -> UiResult<Vec<ScreenDto>> {
        let screens = discover_screens(self.bus.as_ref()).map_err(text)?;
        Ok(screens.iter().map(ScreenDto::from).collect())
    }

    /// Starts (`screen` given) or stops showing the edited theme live.
    pub fn set_live(&self, on: bool, screen: Option<&str>, time: LocalTime) -> UiResult<()> {
        // Stop first: a screen can only be opened once.
        drop(self.studio().stop_live());
        if !on {
            self.settings.update(|s| s.live_screen = None);
            return Ok(());
        }
        let key = screen.ok_or("no screen chosen")?;
        self.storage.ensure_idle()?;
        // Opening wakes the screen (seconds); the session stays usable meanwhile.
        let link = self.connect(key)?;
        let orientation = {
            let mut studio = self.studio();
            studio.go_live(key.to_string(), link, time).map_err(text)?;
            studio.theme().orientation
        };
        self.settings.update(|s| {
            s.live_screen = Some(key.to_string());
            s.remember_orientation(key, orientation);
        });
        Ok(())
    }

    /// Remembers `orientation` as the last one used with `screen` (the file
    /// is written only when it changes).
    fn remember_orientation(&self, screen: &str, orientation: Orientation) {
        if self.settings.load().orientation_for(screen) != Some(orientation) {
            self.settings
                .update(|s| s.remember_orientation(screen, orientation));
        }
    }

    /// Sets a screen's brightness (through the live link when it is live).
    pub fn set_brightness(&self, screen: &str, percent: u8) -> UiResult<()> {
        let brightness = Brightness::new(percent).ok_or("brightness is 0 to 100")?;
        if self
            .studio()
            .live_brightness(screen, brightness)
            .map_err(text)?
        {
            return Ok(());
        }
        self.storage.ensure_idle()?;
        self.connect(screen)?
            .set_brightness(brightness)
            .map_err(text)
    }

    /// Hands a screen back to its own mode (stopping live mode on it).
    pub fn release(&self, screen: &str) -> UiResult<()> {
        self.storage.ensure_idle()?;
        let live = {
            let mut studio = self.studio();
            if studio.live_key() == Some(screen) {
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
        link.release().map_err(text)
    }

    // ------------------------------------------------------------ sensors --

    /// The sensor catalog, re-read.
    pub fn catalog(&self) -> UiResult<Vec<SensorDto>> {
        let mut studio = self.studio();
        Ok(studio
            .refresh_catalog()
            .map_err(text)?
            .iter()
            .map(SensorDto::from)
            .collect())
    }

    /// The latest readings (sampled by the refresh loop).
    pub fn sample(&self) -> SampleDto {
        let studio = self.studio();
        let (snapshot, millis) = studio.readings();
        SampleDto {
            sample_millis: millis,
            readings: SampleDto::readings(snapshot, studio.quantities()),
            live: studio.live_key().map(str::to_string),
            live_error: studio.live_error().map(str::to_string),
            video: studio.live_video().and_then(LiveVideoDto::of),
        }
    }

    // ------------------------------------------------------------- themes --

    /// The edited theme and its location.
    pub fn session(&self) -> SessionDto {
        let studio = self.studio();
        SessionDto {
            theme: ThemeDto::from(studio.theme()),
            location: studio.location().map(|l| l.0.clone()),
        }
    }

    /// Takes the UI's theme and renders it: 8-byte size header then RGBA.
    pub fn render(&self, theme: &ThemeDto, time: LocalTime) -> UiResult<Vec<u8>> {
        let theme = theme_of(theme)?;
        let mut studio = self.studio();
        studio.set_theme(theme);
        studio.render(time).map(|f| frame_bytes(&f)).map_err(text)
    }

    /// Takes the UI's theme and shows it on the live screen now, in the
    /// theme's orientation (remembered for that screen).
    pub fn push(&self, theme: &ThemeDto, time: LocalTime) -> UiResult<()> {
        let theme = theme_of(theme)?;
        let orientation = theme.orientation;
        let live = {
            let mut studio = self.studio();
            studio.set_theme(theme);
            studio.present(time).map_err(text)?;
            studio.live_key().map(str::to_string)
        };
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

    /// Opens a theme of the library (or any theme file).
    pub fn open(&self, location: &str) -> UiResult<ThemeDto> {
        let location = ThemeLocation(location.to_string());
        let mut studio = self.studio();
        studio
            .open(self.store.as_ref(), location.clone())
            .map_err(text)?;
        self.settings
            .update(|s| s.last_theme = Some(location.0.clone()));
        Ok(ThemeDto::from(studio.theme()))
    }

    /// Saves the UI's theme: to `target` when given, else where it was
    /// opened from (a copy in the user folder for a bundled theme).
    pub fn save(&self, theme: &ThemeDto, target: Option<ThemeLocation>) -> UiResult<SavedDto> {
        let theme = theme_of(theme)?;
        let mut studio = self.studio();
        let location =
            target.unwrap_or_else(|| self.library.save_location(studio.location(), &theme.name));
        studio.set_theme(theme);
        studio
            .save(self.store.as_ref(), location.clone())
            .map_err(text)?;
        self.settings
            .update(|s| s.last_theme = Some(location.0.clone()));
        Ok(SavedDto {
            location: location.0,
        })
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
            .ok_or("no model to size the theme")?;
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

    /// Imports a theme: Bezel's own (`.bezeltheme`, or a folder with a
    /// `theme.json`) as it is; another app's (a TURZX `.turtheme`, a
    /// turing-smart-screen-python `theme.yaml` or its folder) converted, with
    /// what had no exact equivalent as warnings.
    pub fn import(&self, path: &Path) -> UiResult<ImportedDto> {
        let (theme, assets, warnings) = if is_native_theme(path) {
            let location = ThemeLocation(path.display().to_string());
            let (theme, assets) = self.store.load(&location).map_err(text)?;
            (theme, assets, Vec::new())
        } else {
            let (theme, assets, report) = import_path(path).map_err(text)?;
            (theme, assets, report.warnings)
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

    /// Adds the image at `path` to the theme.
    pub fn add_image(&self, path: &Path) -> UiResult<AddedDto> {
        let bytes = read_file(path)?;
        if image::guess_format(&bytes).is_err() {
            return Err(format!("{} is not an image", path.display()));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let asset = self.studio().add_asset(&name, bytes);
        Ok(AddedDto { reference: asset.0 })
    }

    /// The theme's assets with previews.
    pub fn assets(&self) -> Vec<AssetDto> {
        let assets: Vec<(String, &'static str, Vec<u8>)> = {
            let studio = self.studio();
            studio
                .assets()
                .iter()
                .map(|(k, v)| (k.0.clone(), kind_of(k), v.clone()))
                .collect()
        };
        assets
            .into_iter()
            .map(|(reference, kind, bytes)| AssetDto {
                data_url: (kind == "image")
                    .then(|| thumbnail_data_url(&bytes))
                    .flatten(),
                reference,
                kind,
            })
            .collect()
    }

    // -------------------------------------------------------------- start --

    /// The theme the window starts with: the last one when it still opens,
    /// else a blank one for the first connected screen (in the orientation
    /// [`Self::new_theme`] picks for it).
    pub fn restore_theme(&self) {
        if let Some(last) = self.settings.load().last_theme {
            match self.open(&last) {
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
        let blank = match self.new_theme(key.as_deref(), UNTITLED, None) {
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
            && let Err(e) = self.open(&entry.location.0)
        {
            tracing::warn!(theme = entry.location.0, "bundled theme not opened: {e}");
        }
    }

    /// Shows the theme live again on the screen that was live when the app
    /// last ran, when it is connected. A failure leaves live mode off.
    pub fn restore_live(&self, time: LocalTime) {
        if let Some(key) = self.settings.load().live_screen
            && let Err(e) = self.set_live(true, Some(&key), time)
        {
            tracing::warn!(screen = key, "live mode not restored: {e}");
        }
    }

    /// One refresh of the session (sample, and a frame on the live screen).
    /// Returns the seconds until the next one.
    pub fn tick(&self, time: LocalTime) -> f32 {
        let mut studio = self.studio();
        if let Err(e) = studio.tick(time) {
            tracing::warn!("live screen stopped: {e}");
        }
        studio
            .theme()
            .refresh_seconds
            .clamp(MIN_REFRESH, MAX_REFRESH)
    }
}

/// Name of a theme started without one.
pub const UNTITLED: &str = "Untitled";

/// Model a new theme is sized for when no screen is connected.
pub const DEFAULT_MODEL: bezel_core::domain::device::ModelId =
    bezel_core::domain::device::ModelId("turing-8.8");

/// Fastest refresh, seconds.
pub const MIN_REFRESH: f32 = 0.25;
/// Slowest refresh, seconds (sensors still update the UI this often).
pub const MAX_REFRESH: f32 = 2.0;

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::clock::Language;
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;
    use bezel_themes::FsThemeStore;
    use std::path::PathBuf;

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
            store: Arc::new(FsThemeStore),
            library: ThemeLibrary::new(root.join("themes"), vec![]),
            settings: SettingsFile::new(root.join("settings.json")),
            fonts: vec!["Inter".into()],
            studio: Mutex::new(studio),
            storage: StorageState::new(
                Box::new(crate::storage::tests::FakeMedia::ready()),
                root.join("scratch"),
            ),
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
    fn render_preview_returns_the_canvas_size() {
        let f = fixture("render");
        let theme = f.backend.session().theme;
        let bytes = f.backend.render(&theme, TIME).unwrap();
        assert_eq!(&bytes[..8], &[224, 1, 0, 0, 128, 7, 0, 0]);
        assert_eq!(bytes.len(), 8 + 480 * 1920 * 4);
        let mut bad = theme.clone();
        bad.orientation = "sideways".into();
        assert!(f.backend.render(&bad, TIME).is_err());
    }

    #[test]
    fn live_mode_shows_edits_and_is_remembered() {
        let f = fixture("live");
        assert_eq!(f.backend.screens().unwrap().len(), 1);
        f.backend.set_live(true, Some(KEY), TIME).unwrap();
        assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
        let theme = f.backend.session().theme;
        f.backend.push(&theme, TIME).unwrap();
        assert!(f.backend.tick(TIME) >= MIN_REFRESH);
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
    fn brightness_and_release_open_a_screen_that_is_not_live() {
        let f = fixture("offline");
        f.backend.set_brightness(KEY, 10).unwrap();
        f.backend.release(KEY).unwrap();
        let log = f.connector.log();
        assert_eq!((log.brightness.len(), log.releases), (1, 1));
    }

    #[test]
    fn sensors_reach_the_ui() {
        let f = fixture("sensors");
        let catalog = f.backend.catalog().unwrap();
        assert!(catalog.iter().any(|s| s.key == "cpu.usage"));
        f.backend.tick(TIME);
        let sample = f.backend.sample();
        assert!(sample.readings.contains_key("cpu.usage"));
        assert_eq!(sample.live_error, None);
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
            (UNTITLED, "reverse-portrait")
        );
        assert_eq!(start.location, None);
        f.backend
            .settings
            .update(|s| s.last_theme = Some("/gone.bezeltheme".into()));
        f.backend.restore_theme();
        assert_eq!(
            f.backend.session().theme.name,
            UNTITLED,
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
                imported.warnings.iter().any(|w| w.contains("LED")),
                "{:?}",
                imported.warnings
            );
            assert_eq!(f.backend.session().theme.name, "Tiny");
        }
        let json = serde_json::to_value(f.backend.import(&dir).unwrap()).unwrap();
        assert!(json["warnings"].as_array().is_some_and(|w| !w.is_empty()));

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
    }
}
