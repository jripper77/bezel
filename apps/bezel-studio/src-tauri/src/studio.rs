//! The editing session behind the window: the theme being edited with its
//! assets, the latest sensor readings and graph histories, and the screen
//! showing the theme live. Everything goes through the core's ports, so the
//! whole session runs on fakes in tests.
//!
//! A theme with a video background (D-2026-09-30-storage-video-4): the
//! preview always shows the poster; the live screen loops the stored video
//! and gets overlays on a transparent base when the core's
//! [`ThemeRuntime::start_video`] finds the video on the screen, and the
//! poster otherwise ([`VideoState::VideoMissing`] carries what sending it
//! takes). The session renders its own frames from the editor's readings, so
//! its runtime only carries that decision: it gets the theme without assets
//! (the decision reads the background, never the bytes).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use bezel_core::app::{MissingVideo, ThemeRuntime, VideoState};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::history::Histories;
use bezel_core::domain::screen::Brightness;
use bezel_core::domain::sensor::{Quantities, SensorInfo, Snapshot};
use bezel_core::domain::theme::{AssetRef, Theme};
use bezel_core::ports::{
    Backdrop, FrameRenderer, RenderContext, ScreenLink, SensorSource, ThemeLocation, ThemeStore,
};
use bezel_core::{BezelError, Result};

/// The screen showing the edited theme.
struct Live {
    key: String,
    /// `None` while a storage job borrows it ([`Studio::lend_live_link`]):
    /// frames pause until it comes back.
    link: Option<Box<dyn ScreenLink>>,
    orientation: Option<Orientation>,
    /// How the theme's video background reaches this screen.
    video: ThemeRuntime,
    /// Start the video (again) before the next frame: after going live, after
    /// a theme with another video, after a job that changed what plays.
    restart_video: bool,
}

impl Live {
    /// What a frame shows under the elements on this screen.
    fn backdrop(&self) -> Backdrop<'static> {
        match self.video.video() {
            VideoState::OnDevice(_) => Backdrop::OnDevice,
            _ => Backdrop::Poster,
        }
    }
}

/// What a borrowed live link resumes when it comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    /// Frames only: the job only asked questions.
    Frames,
    /// Frames, and the theme's video started again: the job may have changed
    /// what the screen plays (an upload stops playback first).
    Video,
}

/// One editing session.
pub struct Studio {
    sensors: Box<dyn SensorSource>,
    renderer: Box<dyn FrameRenderer>,
    language: Language,
    catalog: Vec<SensorInfo>,
    quantities: Quantities,
    snapshot: Snapshot,
    sample_millis: f64,
    histories: Histories,
    theme: Theme,
    assets: BTreeMap<AssetRef, Vec<u8>>,
    location: Option<ThemeLocation>,
    live: Option<Live>,
    live_error: Option<String>,
}

impl Studio {
    /// A session editing `theme`.
    pub fn new(
        sensors: Box<dyn SensorSource>,
        renderer: Box<dyn FrameRenderer>,
        language: Language,
        theme: Theme,
    ) -> Self {
        Self {
            sensors,
            renderer,
            language,
            catalog: Vec::new(),
            quantities: Quantities::new(),
            snapshot: Snapshot::default(),
            sample_millis: 0.0,
            histories: Histories::new(&theme.history_lengths()),
            theme,
            assets: BTreeMap::new(),
            location: None,
            live: None,
            live_error: None,
        }
    }

    // ------------------------------------------------------------ sensors --

    /// Re-reads the sensor catalog (sensors come and go with hardware).
    pub fn refresh_catalog(&mut self) -> Result<&[SensorInfo]> {
        self.catalog = self.sensors.catalog()?;
        self.quantities = Quantities::from_catalog(&self.catalog);
        Ok(&self.catalog)
    }

    /// The last catalog read.
    pub fn catalog(&self) -> &[SensorInfo] {
        &self.catalog
    }

    /// What each sensor of the last catalog measures.
    pub fn quantities(&self) -> &Quantities {
        &self.quantities
    }

    /// Takes a sample and records it in the graph histories.
    pub fn sample(&mut self) -> Result<()> {
        let started = Instant::now();
        self.snapshot = self.sensors.sample()?;
        self.sample_millis = started.elapsed().as_secs_f64() * 1000.0;
        self.histories.push(&self.snapshot);
        Ok(())
    }

    /// The latest readings and how long taking them took.
    pub fn readings(&self) -> (&Snapshot, f64) {
        (&self.snapshot, self.sample_millis)
    }

    // ----------------------------------------------------------- document --

    /// The theme being edited.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Where the theme was loaded from or saved to.
    pub fn location(&self) -> Option<&ThemeLocation> {
        self.location.as_ref()
    }

    /// The session's assets (used by the theme or added for it).
    pub fn assets(&self) -> &BTreeMap<AssetRef, Vec<u8>> {
        &self.assets
    }

    /// Replaces the edited theme, keeping the history of sensors still graphed.
    pub fn set_theme(&mut self, theme: Theme) {
        if theme == self.theme {
            return;
        }
        let mut histories = Histories::new(&theme.history_lengths());
        histories.adopt(&self.histories);
        self.histories = histories;
        if let Some(live) = self.live.as_mut() {
            let before = live.video.video().clone();
            live.video.replace(theme.clone(), BTreeMap::new());
            live.restart_video |= *live.video.video() != before;
        }
        self.theme = theme;
    }

    /// Starts a new document.
    pub fn start(
        &mut self,
        theme: Theme,
        assets: BTreeMap<AssetRef, Vec<u8>>,
        location: Option<ThemeLocation>,
    ) {
        self.set_theme(theme);
        self.assets = assets;
        self.location = location;
    }

    /// Opens the theme at `location`.
    pub fn open(&mut self, store: &dyn ThemeStore, location: ThemeLocation) -> Result<()> {
        let (theme, assets) = store.load(&location)?;
        self.start(theme, assets, Some(location));
        Ok(())
    }

    /// Saves the theme (with only the assets it uses) at `location`.
    pub fn save(&mut self, store: &dyn ThemeStore, location: ThemeLocation) -> Result<()> {
        let used: BTreeSet<AssetRef> = self.theme.assets().into_iter().collect();
        let assets: BTreeMap<AssetRef, Vec<u8>> = self
            .assets
            .iter()
            .filter(|(k, _)| used.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        store.save(&location, &self.theme, &assets)?;
        self.location = Some(location);
        Ok(())
    }

    /// Adds a file to the theme under `assets/`, named after `file_name`.
    /// The same bytes added twice give the same reference.
    pub fn add_asset(&mut self, file_name: &str, bytes: Vec<u8>) -> AssetRef {
        if let Some((existing, _)) = self.assets.iter().find(|(_, b)| **b == bytes) {
            return existing.clone();
        }
        let (stem, extension) = split_name(file_name);
        let mut n = 1;
        let asset = loop {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            let candidate = AssetRef(format!("assets/{stem}{suffix}{extension}"));
            if !self.assets.contains_key(&candidate) {
                break candidate;
            }
            n += 1;
        };
        self.assets.insert(asset.clone(), bytes);
        asset
    }

    // ------------------------------------------------------------- frames --

    /// Renders the edited theme with the latest readings, as the preview
    /// shows it (a video background shows its poster).
    pub fn render(&mut self, time: LocalTime) -> Result<Frame> {
        self.render_with(time, Backdrop::Poster)
    }

    fn render_with(&mut self, time: LocalTime, backdrop: Backdrop<'_>) -> Result<Frame> {
        let context = RenderContext {
            snapshot: &self.snapshot,
            histories: &self.histories,
            quantities: &self.quantities,
            time,
            language: self.language,
            backdrop,
        };
        self.renderer.render(&self.theme, &self.assets, context)
    }

    /// Key of the screen showing the theme, if any.
    pub fn live_key(&self) -> Option<&str> {
        self.live.as_ref().map(|l| l.key.as_str())
    }

    /// Why the live screen stopped, until the next `go_live`.
    pub fn live_error(&self) -> Option<&str> {
        self.live_error.as_deref()
    }

    /// How the theme's video background reaches the live screen (`None`
    /// when no screen is live).
    pub fn live_video(&self) -> Option<&VideoState> {
        self.live.as_ref().map(|l| l.video.video())
    }

    /// The theme video the live screen `key` could play but does not store.
    pub fn missing_video(&self, key: &str) -> Option<MissingVideo> {
        match self.live.as_ref().filter(|l| l.key == key)?.video.video() {
            VideoState::VideoMissing(missing) => Some(missing.clone()),
            _ => None,
        }
    }

    /// Lends the live link of `key` to a storage job: frames pause and the
    /// session stays usable (previews keep rendering) while the job talks to
    /// the screen. `None` when `key` is not live (or its link is lent).
    pub fn lend_live_link(&mut self, key: &str) -> Option<Box<dyn ScreenLink>> {
        self.live
            .as_mut()
            .filter(|l| l.key == key)
            .and_then(|l| l.link.take())
    }

    /// Takes back a link lent by [`Self::lend_live_link`] and shows a frame
    /// now (after `resume`). Hands the link back when `key` stopped being
    /// live meanwhile: the caller closes it.
    pub fn return_live_link(
        &mut self,
        key: &str,
        link: Box<dyn ScreenLink>,
        resume: Resume,
        time: LocalTime,
    ) -> Option<Box<dyn ScreenLink>> {
        let Some(live) = self
            .live
            .as_mut()
            .filter(|l| l.key == key && l.link.is_none())
        else {
            return Some(link);
        };
        live.link = Some(link);
        live.restart_video |= resume == Resume::Video;
        if let Err(e) = self.present(time) {
            tracing::warn!(screen = key, "live screen stopped after a storage job: {e}");
        }
        None
    }

    /// Shows the edited theme on `link` from now on, starting now.
    pub fn go_live(
        &mut self,
        key: String,
        link: Box<dyn ScreenLink>,
        time: LocalTime,
    ) -> Result<()> {
        let video = ThemeRuntime::new(self.theme.clone(), BTreeMap::new(), self.language);
        self.live = Some(Live {
            key,
            link: Some(link),
            orientation: None,
            video,
            restart_video: true,
        });
        self.live_error = None;
        self.present(time)
    }

    /// Stops showing the theme and hands back the screen's link (`None`
    /// while a storage job borrows it: the job closes it when done).
    pub fn stop_live(&mut self) -> Option<Box<dyn ScreenLink>> {
        self.live.take().and_then(|l| l.link)
    }

    /// Sets the brightness of the live screen when it is `key`; `false` when
    /// that screen is not live. `InUse` while a storage job borrows its link.
    pub fn live_brightness(&mut self, key: &str, brightness: Brightness) -> Result<bool> {
        let Some(live) = self.live.as_mut().filter(|l| l.key == key) else {
            return Ok(false);
        };
        let Some(link) = live.link.as_mut() else {
            return Err(BezelError::InUse {
                address: key.to_string(),
                holders: vec![STORAGE_JOB.to_string()],
            });
        };
        link.set_brightness(brightness).map(|()| true)
    }

    /// Starts the theme's video on the live screen when it has to (a failure
    /// keeps the poster).
    fn start_live_video(&mut self) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(link) = live.link.as_mut() else {
            return;
        };
        if std::mem::take(&mut live.restart_video)
            && let Err(e) = live.video.start_video(link.as_mut(), None)
        {
            tracing::warn!(screen = live.key, "video background not started: {e}");
        }
    }

    /// Renders and shows one frame on the live screen (nothing while a
    /// storage job borrows its link). A failure stops the live mode (the link
    /// is dropped) and is kept for [`Self::live_error`].
    pub fn present(&mut self, time: LocalTime) -> Result<()> {
        self.start_live_video();
        let Some(backdrop) = self
            .live
            .as_ref()
            .filter(|l| l.link.is_some())
            .map(Live::backdrop)
        else {
            return Ok(());
        };
        let result = self
            .render_with(time, backdrop)
            .and_then(|frame| self.present_frame(&frame));
        if let Err(e) = &result {
            self.live = None;
            self.live_error = Some(e.to_string());
        }
        result
    }

    fn present_frame(&mut self, frame: &Frame) -> Result<()> {
        let orientation = self.theme.orientation;
        let Some(live) = self.live.as_mut() else {
            return Ok(());
        };
        let Some(link) = live.link.as_mut() else {
            return Ok(());
        };
        let expected = link.identity().model.panel.in_orientation(orientation);
        if frame.size() != expected {
            return Err(BezelError::Transport(format!(
                "this theme is {}x{} but the screen is {}x{} in this orientation",
                frame.size().width,
                frame.size().height,
                expected.width,
                expected.height
            )));
        }
        if live.orientation != Some(orientation) {
            link.set_orientation(orientation)?;
            live.orientation = Some(orientation);
        }
        link.present(frame)
    }

    /// One refresh: a sample, then a frame on the live screen.
    pub fn tick(&mut self, time: LocalTime) -> Result<()> {
        if let Err(e) = self.sample() {
            tracing::warn!("sensor sample failed: {e}");
        }
        self.present(time)
    }
}

/// Who holds a live screen while a storage job borrows its link.
pub const STORAGE_JOB: &str = "a storage job of Bezel";

/// `"My Photo.PNG"` → (`"my-photo"`, `".png"`): safe, lowercase asset names.
fn split_name(file_name: &str) -> (String, String) {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or_default();
    let (stem, extension) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() => (s, format!(".{}", clean(e))),
        _ => (base, String::new()),
    };
    let stem = clean(stem);
    (
        if stem.is_empty() { "file".into() } else { stem },
        extension,
    )
}

fn clean(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    mapped
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::app::open_screen;
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::theme::{Background, Fit};
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_sensors::FakeSensors;
    use std::sync::{Arc, Mutex};

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 12,
        minute: 0,
        second: 0,
        weekday: 2,
    };

    /// Fills the canvas and counts renders.
    #[derive(Default, Clone)]
    struct Flat(Arc<Mutex<usize>>);

    impl FrameRenderer for Flat {
        fn render(
            &mut self,
            theme: &Theme,
            _: &BTreeMap<AssetRef, Vec<u8>>,
            _: RenderContext<'_>,
        ) -> Result<Frame> {
            *self.0.lock().unwrap() += 1;
            Ok(Frame::filled(theme.canvas, Rgba::BLACK))
        }
    }

    fn theme_88() -> Theme {
        Theme::blank("T", Size::new(480, 1920), Orientation::ReversePortrait)
    }

    fn studio() -> Studio {
        Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(Flat::default()),
            Language::English,
            theme_88(),
        )
    }

    #[test]
    fn samples_feed_the_readings() {
        let mut s = studio();
        assert!(!s.refresh_catalog().unwrap().is_empty());
        s.sample().unwrap();
        s.sample().unwrap();
        let (snapshot, millis) = s.readings();
        assert!(!snapshot.is_empty());
        assert!(millis >= 0.0);
        assert_eq!(s.catalog().len(), s.refresh_catalog().unwrap().len());
    }

    #[test]
    fn live_mode_presents_in_the_theme_orientation_and_stops_on_errors() {
        let connector = FakeConnector::default();
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        s.go_live("k".into(), link, TIME).unwrap();
        assert_eq!(s.live_key(), Some("k"));
        s.tick(TIME).unwrap();
        let log = connector.log();
        assert_eq!(log.frames.len(), 2);
        assert_eq!(log.orientations, vec![Orientation::ReversePortrait]);

        assert!(s.live_brightness("k", Brightness::MAX).unwrap());
        assert!(!s.live_brightness("other", Brightness::MAX).unwrap());

        // A theme that does not fit the screen stops the live mode.
        s.set_theme(Theme::blank(
            "Small",
            Size::new(320, 480),
            Orientation::Portrait,
        ));
        let error = s.present(TIME).unwrap_err().to_string();
        assert!(error.contains("320x480"), "{error}");
        assert_eq!(s.live_key(), None);
        assert!(s.live_error().unwrap().contains("480x1920"));
        assert!(s.stop_live().is_none());
        s.present(TIME).unwrap();
    }

    #[test]
    fn stop_live_hands_back_the_link() {
        let connector = FakeConnector::default();
        let link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
        let mut s = studio();
        s.go_live("k".into(), link, TIME).unwrap();
        s.stop_live().unwrap().release().unwrap();
        assert_eq!(connector.log().releases, 1);
        s.tick(TIME).unwrap();
        assert_eq!(connector.log().frames.len(), 1);
    }

    #[test]
    fn assets_get_safe_unique_names() {
        let mut s = studio();
        assert_eq!(
            s.add_asset("C:\\Photos\\My Photo.PNG", vec![1]).0,
            "assets/my-photo.png"
        );
        assert_eq!(
            s.add_asset("my photo.png", vec![2]).0,
            "assets/my-photo-2.png"
        );
        assert_eq!(s.add_asset("again.png", vec![1]).0, "assets/my-photo.png");
        assert_eq!(s.add_asset("../../.hidden", vec![3]).0, "assets/hidden");
        assert_eq!(s.add_asset("noext", vec![4]).0, "assets/noext");
        assert_eq!(s.assets().len(), 4);
    }

    type Stored = (Theme, BTreeMap<AssetRef, Vec<u8>>);

    #[derive(Default)]
    struct MemoryStore(Mutex<BTreeMap<ThemeLocation, Stored>>);

    impl ThemeStore for MemoryStore {
        fn load(&self, location: &ThemeLocation) -> Result<Stored> {
            self.0
                .lock()
                .unwrap()
                .get(location)
                .cloned()
                .ok_or_else(|| BezelError::ScreenNotFound(location.0.clone()))
        }

        fn save(
            &self,
            location: &ThemeLocation,
            theme: &Theme,
            assets: &BTreeMap<AssetRef, Vec<u8>>,
        ) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert(location.clone(), (theme.clone(), assets.clone()));
            Ok(())
        }
    }

    #[test]
    fn save_keeps_only_used_assets_and_open_restores_them() {
        let store = MemoryStore::default();
        let mut s = studio();
        let used = s.add_asset("bg.png", vec![1]);
        s.add_asset("unused.png", vec![2]);
        let mut theme = s.theme().clone();
        theme.background = Background::Image {
            asset: used.clone(),
            fit: Fit::Cover,
        };
        s.set_theme(theme.clone());
        let at = ThemeLocation("mem://a".into());
        s.save(&store, at.clone()).unwrap();
        assert_eq!(s.location(), Some(&at));

        let mut other = studio();
        other.open(&store, at.clone()).unwrap();
        assert_eq!(other.theme(), &theme);
        assert_eq!(other.assets().keys().collect::<Vec<_>>(), vec![&used]);
        assert!(
            other
                .open(&store, ThemeLocation("mem://nope".into()))
                .is_err()
        );
    }

    #[test]
    fn render_uses_the_edited_theme() {
        let renders = Flat::default();
        let mut s = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(renders.clone()),
            Language::PortugueseBr,
            theme_88(),
        );
        s.set_theme(s.theme().clone());
        let frame = s.render(TIME).unwrap();
        assert_eq!(frame.size(), Size::new(480, 1920));
        s.start(
            Theme::blank("Wide", Size::new(480, 1920), Orientation::Landscape),
            BTreeMap::new(),
            None,
        );
        assert_eq!(s.render(TIME).unwrap().size(), Size::new(1920, 480));
        assert_eq!(*renders.0.lock().unwrap(), 2);
        assert_eq!(s.location(), None);
    }
}
