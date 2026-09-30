//! What each UI command does, without Tauri: the commands module only adds
//! threads, dialogs and IPC around these methods, so they run on fakes in
//! tests. Errors reach the UI as text.

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

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
use crate::studio::{Delivery, Studio};
pub use crate::studio::{MAX_REFRESH, MIN_REFRESH};

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
    pub studio: Session,
    /// The screen's files: the media converter and the running operation.
    pub storage: StorageState,
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

/// Keeps the refresh loop on its cadence: each refresh is due one period
/// after the previous one was due, however long the work took (the screen's
/// I/O does not stretch the period). A loop that fell behind starts again
/// from now instead of catching up in a burst.
#[derive(Debug, Clone, Copy)]
pub struct Pacer {
    due: Instant,
}

impl Pacer {
    /// A cadence whose first refresh was due at `now`.
    pub fn new(now: Instant) -> Self {
        Self { due: now }
    }

    /// How long to wait at `now` for the refresh due `period` after the
    /// last one.
    pub fn wait(&mut self, period: Duration, now: Instant) -> Duration {
        self.due += period;
        if self.due < now {
            self.due = now;
        }
        self.due - now
    }
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
        let unwanted = self.studio().presented(delivery, &outcome);
        self.studio.link_back.notify_all();
        drop(unwanted);
        outcome
    }

    /// Shows the edited theme on the live screen now.
    pub(crate) fn show_now(&self, time: LocalTime) -> bezel_core::Result<()> {
        let delivered = self.idle_studio().frame_for_screen(time);
        self.deliver(delivered)
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
        let previous = self.idle_studio().stop_live();
        drop(previous);
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
            studio.go_live(key.to_string(), link);
            studio.theme().orientation
        };
        self.show_now(time).map_err(text)?;
        self.settings.update(|s| {
            s.live_screen = Some(key.to_string());
            s.remember_orientation(key, orientation);
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
        let screen = discover_screens(self.bus.as_ref())
            .and_then(|screens| choose_screen(screens, None))
            .map_err(text)?;
        let key = screen.address().ok_or("the screen has no address")?;
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

    /// Sets a screen's brightness (through the live link when it is live).
    pub fn set_brightness(&self, screen: &str, percent: u8) -> UiResult<()> {
        let brightness = Brightness::new(percent).ok_or("brightness is 0 to 100")?;
        if self
            .idle_studio()
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
            let mut studio = self.idle_studio();
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
            studio.live_key().map(str::to_string)
        };
        self.show_now(time).map_err(text)?;
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

    /// Opens a theme of the library, or a theme file the user picked in a
    /// dialog during this session.
    pub fn open(&self, location: &str) -> UiResult<ThemeDto> {
        let location = ThemeLocation(location.to_string());
        if !self.library.allows(&location) {
            return Err(format!(
                "{} is not in the theme library; import it instead",
                location.0
            ));
        }
        self.open_at(location)
    }

    /// Opens the theme at `location`, which the app chose itself.
    fn open_at(&self, location: ThemeLocation) -> UiResult<ThemeDto> {
        let mut studio = self.studio();
        studio
            .open(self.store.as_ref(), location.clone())
            .map_err(text)?;
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
            return Err(format!("{} was not picked to save to", target.0));
        }
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
            && let Err(e) = self.open_at(entry.location.clone())
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

    /// One refresh of the session (a sample when due, and a frame on the
    /// live screen, shown outside the session's lock). Returns the time
    /// until the next one.
    pub fn tick(&self, time: LocalTime) -> Duration {
        let delivered = self.studio().tick(time);
        if let Err(e) = self.deliver(delivered) {
            tracing::warn!("live screen stopped: {e}");
        }
        self.studio().period()
    }
}

/// Name of a theme started without one.
pub const UNTITLED: &str = "Untitled";

/// Model a new theme is sized for when no screen is connected.
pub const DEFAULT_MODEL: bezel_core::domain::device::ModelId =
    bezel_core::domain::device::ModelId("turing-8.8");

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
            store: Arc::new(FsThemeStore),
            library: ThemeLibrary::new(root.join("themes"), vec![]),
            settings: SettingsFile::new(root.join("settings.json")),
            fonts: vec!["Inter".into()],
            studio: Session::new(studio),
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
        assert!(f.backend.tick(TIME).as_secs_f32() >= MIN_REFRESH);
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
    fn the_refresh_keeps_its_cadence() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let second = Duration::from_secs(1);
        let mut pacer = Pacer::new(t0);
        // The frame took 260 ms: the wait is the rest of the period.
        assert_eq!(pacer.wait(second, t0 + ms(260)), ms(740));
        assert_eq!(pacer.wait(second, t0 + second + ms(300)), ms(700));
        // Far behind: the next one now, then the cadence from there.
        assert_eq!(pacer.wait(second, t0 + ms(5000)), Duration::ZERO);
        assert_eq!(pacer.wait(second, t0 + ms(5100)), ms(900));
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
            let ticking = scope.spawn(|| backend.tick(TIME));
            frame_arrived.recv().unwrap();
            // The screen is busy with a frame: the session is not.
            assert!(backend.render(&theme, TIME).is_ok());
            assert_eq!(backend.sample().live.as_deref(), Some(KEY));
            // Whoever needs the link waits for it.
            let dimming = scope.spawn(|| backend.set_brightness(KEY, 30));
            std::thread::sleep(Duration::from_millis(50));
            assert!(f.connector.log().brightness.is_empty(), "after the frame");
            let_through.send(()).unwrap();
            assert!(ticking.join().unwrap() >= Duration::from_secs_f32(MIN_REFRESH));
            dimming.join().unwrap().unwrap();
        });
        let log = f.connector.log();
        assert_eq!((log.frames.len(), log.brightness.len()), (1, 1));
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
            assert!(error.contains("not in the theme library"), "{error}");
        }
        let error = f.backend.save(&theme, Some(at.clone())).unwrap_err();
        assert!(error.contains("not picked"), "{error}");
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
