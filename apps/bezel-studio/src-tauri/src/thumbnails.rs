//! Thumbnails of the library's themes for the Themes tab: each theme drawn
//! by the real renderer with demo sensor values (a video background shows
//! its poster), reduced to at most [`THUMBNAIL_SIDE`] pixels and kept as a
//! PNG in the app's cache folder.
//!
//! A thumbnail is found again by where its theme lives and the
//! [`revision`] of the theme's files, so an edited theme is drawn again;
//! saving a theme through the studio also forgets its thumbnails
//! ([`Thumbnails::forget`]). Drawing uses a renderer of its own, never the
//! editing session's: the editor does not wait for the gallery.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bezel_core::app::ThemeRuntime;
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::theme::{AssetRef, Theme};
use bezel_core::ports::{FrameRenderer, SensorSource, ThemeLocation, ThemeStore};
use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::diag::{self, DiagCode};
use crate::library::{Fnv, revision};
use crate::texts::language_slug;

/// Longest side of a theme's thumbnail, pixels.
pub const THUMBNAIL_SIDE: u32 = 360;

/// Changes whenever thumbnails are drawn differently: the older ones in the
/// cache are then drawn again.
const FORMAT: &[u8] = b"bezel-thumbnail-1";

/// Most samples taken to fill a theme's graphs before it is drawn.
const MAX_SAMPLES: usize = 600;

/// Builds the renderer thumbnails are drawn with (once, on first use: it
/// loads fonts).
pub type MakeRenderer = Box<dyn Fn() -> Box<dyn FrameRenderer> + Send + Sync>;

/// Builds the sensors a thumbnail shows (demo values: the gallery measures
/// nothing).
pub type MakeSensors = Box<dyn Fn() -> Box<dyn SensorSource> + Send + Sync>;

/// The thumbnails of the theme library, cached in a folder.
pub struct Thumbnails {
    dir: PathBuf,
    make_renderer: MakeRenderer,
    make_sensors: MakeSensors,
    renderer: Mutex<Option<Box<dyn FrameRenderer>>>,
}

impl std::fmt::Debug for Thumbnails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbnails")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

/// `png` as a `data:` URL the webview shows.
fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

/// The hash of `text`.
fn hash_of(text: &str) -> u64 {
    Fnv::new().field(text.as_bytes()).finish()
}

/// `frame` reduced to fit [`THUMBNAIL_SIDE`] (never enlarged), as a PNG.
fn reduce(frame: &Frame) -> Option<Vec<u8>> {
    let size = frame.size();
    let image = RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())?;
    let image = DynamicImage::ImageRgba8(image);
    let small = if size.width.max(size.height) > THUMBNAIL_SIDE {
        image.thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE)
    } else {
        image
    };
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(small.to_rgb8())
        .write_to(&mut png, ImageFormat::Png)
        .ok()?;
    Some(png.into_inner())
}

impl Thumbnails {
    /// Thumbnails kept in `dir`, drawn by the renderer `make_renderer`
    /// builds with the readings of the sensors `make_sensors` builds.
    pub fn new(dir: PathBuf, make_renderer: MakeRenderer, make_sensors: MakeSensors) -> Self {
        Self {
            dir,
            make_renderer,
            make_sensors,
            renderer: Mutex::new(None),
        }
    }

    /// The folder thumbnails are kept in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The beginning of the cached file names of `location`'s thumbnails.
    fn prefix(location: &ThemeLocation) -> String {
        format!("{:016x}-", hash_of(&location.0))
    }

    /// Where the thumbnail of `location` at `revision`, drawn in `language`
    /// (dates and units), is kept.
    fn file_of(&self, location: &ThemeLocation, revision: u64, language: Language) -> PathBuf {
        let key = Fnv::new()
            .field(FORMAT)
            .field(&revision.to_le_bytes())
            .field(language_slug(language).as_bytes())
            .finish();
        self.dir
            .join(format!("{}{key:016x}.png", Self::prefix(location)))
    }

    /// The thumbnail of the theme at `location` as a PNG `data:` URL: the
    /// one kept for its files as they are now, else drawn at `time` and
    /// kept. `None` when the theme cannot be read or drawn (logged): the
    /// gallery shows its placeholder.
    pub fn get(
        &self,
        store: &dyn ThemeStore,
        location: &ThemeLocation,
        language: Language,
        time: LocalTime,
    ) -> Option<String> {
        let file = self.file_of(location, revision(location)?, language);
        if let Ok(png) = std::fs::read(&file) {
            return Some(data_url(&png));
        }
        let png = self.draw(store, location, language, time)?;
        self.keep(location, &file, &png);
        Some(data_url(&png))
    }

    /// Draws the theme at `location` and reduces it.
    fn draw(
        &self,
        store: &dyn ThemeStore,
        location: &ThemeLocation,
        language: Language,
        time: LocalTime,
    ) -> Option<Vec<u8>> {
        let drawn = store
            .load(location)
            .and_then(|(theme, assets)| self.render(theme, assets, language, time));
        match drawn {
            Ok(frame) => reduce(&frame),
            Err(_) => {
                diag::report(DiagCode::NoThumbnail);
                None
            }
        }
    }

    /// One frame of `theme` after enough demo samples to fill its graphs,
    /// a video background showing its poster.
    fn render(
        &self,
        theme: Theme,
        assets: std::collections::BTreeMap<AssetRef, Vec<u8>>,
        language: Language,
        time: LocalTime,
    ) -> bezel_core::Result<Frame> {
        let samples = theme
            .history_lengths()
            .iter()
            .map(|(_, length)| *length)
            .max()
            .unwrap_or(0)
            .clamp(2, MAX_SAMPLES);
        let mut runtime = ThemeRuntime::new(theme, assets, language);
        let mut sensors = (self.make_sensors)();
        for _ in 0..samples {
            runtime.sample(sensors.as_mut())?;
        }
        let mut renderer = self.renderer.lock().unwrap_or_else(PoisonError::into_inner);
        let renderer = renderer.get_or_insert_with(|| (self.make_renderer)());
        runtime.render(renderer.as_mut(), time, Duration::ZERO)
    }

    /// Keeps `png` as `file`, the only thumbnail of `location`.
    fn keep(&self, location: &ThemeLocation, file: &Path, png: &[u8]) {
        self.forget(location);
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.dir)?;
            let partial = file.with_extension("part");
            std::fs::write(&partial, png)?;
            std::fs::rename(&partial, file)
        };
        if write().is_err() {
            diag::report(DiagCode::ThumbnailNotKept);
        }
    }

    /// Forgets every thumbnail kept for `location` (its theme was saved).
    pub fn forget(&self, location: &ThemeLocation) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let prefix = Self::prefix(location);
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix)
                && std::fs::remove_file(entry.path()).is_err()
            {
                diag::report(DiagCode::OldThumbnailNotRemoved);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_core::domain::theme::{Background, BoxF, ElementKind};
    use bezel_render::{SkiaRenderer, SystemFonts, font_files};
    use bezel_sensors::FakeSensors;
    use bezel_themes::FsThemeStore;
    use std::collections::BTreeMap;

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 21,
        minute: 5,
        second: 42,
        weekday: 2,
    };

    /// The repository's `themes/` folder (no `..` in it: the library only
    /// takes plain paths).
    fn bundled_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("the repository")
            .join("themes")
    }

    /// Thumbnails in `dir` drawn with the bundled fonts only and the demo
    /// sensors, as the app draws them.
    pub(crate) fn thumbnails(dir: PathBuf) -> Thumbnails {
        Thumbnails::new(
            dir,
            Box::new(|| {
                Box::new(SkiaRenderer::with_fonts(
                    font_files(&bundled_dir().join("fonts")),
                    SystemFonts::Skip,
                ))
            }),
            Box::new(|| Box::new(FakeSensors::demo())),
        )
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bezel-thumbnails-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn at(path: &Path) -> ThemeLocation {
        ThemeLocation(path.display().to_string())
    }

    fn decode(url: &str) -> image::RgbImage {
        let png = STANDARD
            .decode(url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        image::load_from_memory(&png).unwrap().to_rgb8()
    }

    /// Pixels of `image` inside `frame` (canvas pixels, scaled by `scale`)
    /// that differ clearly from the `background` color.
    fn drawn_in(image: &image::RgbImage, frame: BoxF, scale: f32, background: [u8; 3]) -> usize {
        let (x0, y0) = ((frame.x * scale) as u32, (frame.y * scale) as u32);
        let (x1, y1) = (
            ((frame.x + frame.width) * scale).ceil() as u32,
            ((frame.y + frame.height) * scale).ceil() as u32,
        );
        let mut count = 0;
        for y in y0..y1.min(image.height()) {
            for x in x0..x1.min(image.width()) {
                let p = image.get_pixel(x, y).0;
                let far = (0..3).any(|c| p[c].abs_diff(background[c]) > 40);
                count += usize::from(far);
            }
        }
        count
    }

    #[test]
    fn a_dark_bundled_theme_shows_its_text_and_graphs() {
        let root = scratch("bundled");
        let location = at(&bundled_dir().join("turing-8.8-horizontal"));
        let (theme, _) = FsThemeStore.load(&location).unwrap();
        let Background::Color(color) = theme.background else {
            panic!("the Midnight themes have a color background");
        };
        let background = [color.r, color.g, color.b];
        assert!(
            background.iter().all(|c| *c < 16),
            "near black: {background:?}"
        );
        let url = thumbnails(root.join("cache"))
            .get(&FsThemeStore, &location, Language::English, TIME)
            .unwrap();
        let image = decode(&url);
        assert_eq!((image.width(), image.height()), (THUMBNAIL_SIDE, 90));
        let scale = THUMBNAIL_SIDE as f32 / theme.canvas.width as f32;
        let (mut texts, mut graphs) = (0, 0);
        for element in theme.elements.iter().filter(|e| e.visible) {
            let drawn = drawn_in(&image, element.frame, scale, background);
            match &element.kind {
                ElementKind::Text { .. } if element.frame.height >= 24.0 => {
                    assert!(drawn > 0, "{} is not drawn", element.name);
                    texts += 1;
                }
                ElementKind::Graph { .. } => {
                    assert!(drawn > 0, "{} is not drawn", element.name);
                    graphs += 1;
                }
                _ => {}
            }
        }
        assert!(texts >= 4 && graphs >= 1, "{texts} texts, {graphs} graphs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_thumbnail_is_kept_until_its_theme_changes() {
        let root = scratch("cache");
        let location = at(&root.join("mine.bezeltheme"));
        let mut theme = Theme::blank("Mine", Size::new(480, 1920), Orientation::Portrait);
        theme.background = Background::Color(bezel_core::domain::frame::Rgba::opaque(200, 0, 0));
        FsThemeStore
            .save(&location, &theme, &BTreeMap::new())
            .unwrap();
        let cache = thumbnails(root.join("cache"));
        let first = cache
            .get(&FsThemeStore, &location, Language::English, TIME)
            .unwrap();
        assert_eq!(decode(&first).get_pixel(10, 10).0, [200, 0, 0]);
        let kept: Vec<PathBuf> = std::fs::read_dir(cache.dir())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        // A hit reads the kept file: drawn again, it would not be this one.
        let marker = reduce(&Frame::filled(
            Size::new(4, 4),
            bezel_core::domain::frame::Rgba::opaque(0, 0, 255),
        ))
        .unwrap();
        std::fs::write(&kept[0], &marker).unwrap();
        let hit = cache
            .get(&FsThemeStore, &location, Language::English, TIME)
            .unwrap();
        assert_eq!(hit, data_url(&marker));
        // Another language is another thumbnail (dates and units differ).
        let other = cache
            .get(&FsThemeStore, &location, Language::PortugueseBr, TIME)
            .unwrap();
        assert_ne!(other, data_url(&marker));
        // Saved again: drawn again from the new files, the old one gone.
        theme.background = Background::Color(bezel_core::domain::frame::Rgba::opaque(0, 160, 0));
        FsThemeStore
            .save(&location, &theme, &BTreeMap::new())
            .unwrap();
        let after = cache
            .get(&FsThemeStore, &location, Language::English, TIME)
            .unwrap();
        assert_eq!(decode(&after).get_pixel(10, 10).0, [0, 160, 0]);
        assert_eq!(std::fs::read_dir(cache.dir()).unwrap().count(), 1);
        cache.forget(&location);
        assert_eq!(std::fs::read_dir(cache.dir()).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_theme_that_cannot_be_drawn_has_no_thumbnail() {
        let root = scratch("broken");
        let cache = thumbnails(root.join("cache"));
        let broken = root.join("broken.bezeltheme");
        std::fs::write(&broken, b"not a zip").unwrap();
        let missing = root.join("missing.bezeltheme");
        let huge = at(&root.join("huge.bezeltheme"));
        let theme = Theme::blank("Huge", Size::new(20_000, 20_000), Orientation::Portrait);
        FsThemeStore.save(&huge, &theme, &BTreeMap::new()).unwrap();
        for location in [at(&broken), at(&missing), huge] {
            let got = cache.get(&FsThemeStore, &location, Language::English, TIME);
            assert_eq!(got, None, "{}", location.0);
        }
        assert_eq!(
            std::fs::read_dir(cache.dir()).map_or(0, Iterator::count),
            0,
            "nothing kept"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
