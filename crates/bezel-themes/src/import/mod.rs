//! Importers: convert other apps' themes into Bezel's model (they convert,
//! they do not emulate — D-2026-09-30-render-engine-4).
//!
//! - [`turzx`]: the vendor app's `.turtheme` files (an MS-NRBF object graph,
//!   read by the whitelisting [`nrbf`] parser).
//! - [`python_yaml`]: turing-smart-screen-python `theme.yaml` folders.
//!
//! Every importer returns the theme, the assets it bundles and an
//! [`ImportReport`] listing what could not be mapped exactly.

pub mod colors;
pub mod nrbf;
pub mod python_yaml;
pub mod turzx;
mod warning;
mod yaml;

pub use warning::{ImportWarning, LAYER_NAMES, WarningCode};

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use bezel_core::domain::geometry::Size;
use bezel_core::domain::theme::{AssetRef, BoxF, Element, ElementId, ElementKind, Fit, Theme};
use bezel_core::{BezelError, Result};

/// What an importer produced: the theme, its assets and the report.
pub type Imported = (Theme, BTreeMap<AssetRef, Vec<u8>>, ImportReport);

/// What could not be mapped exactly: codes with arguments, which read as
/// English sentences.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// One entry per distinct problem.
    pub warnings: Vec<ImportWarning>,
}

impl ImportReport {
    /// Records a problem once.
    pub fn warn(&mut self, warning: ImportWarning) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// True when everything was mapped.
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// Largest vendor theme accepted.
const MAX_TURTHEME: u64 = 64 * 1024 * 1024;
/// Largest background video bundled from next to a vendor theme.
const MAX_VIDEO: u64 = 512 * 1024 * 1024;

/// Imports the theme at `path`, choosing the importer by content and name:
/// an NRBF stream (`.turtheme`), a folder with a `theme.yaml`, or a
/// `theme.yaml` file itself.
pub fn import_path(path: &Path) -> Result<Imported> {
    let fail = |e: String| BezelError::ThemeFile(format!("import {}: {e}", path.display()));
    if path.is_dir() {
        return python_yaml::import_dir(path).map_err(fail);
    }
    let is_yaml = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"));
    if is_yaml {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        return python_yaml::import_file(path, dir).map_err(fail);
    }
    let bytes = read_limited(path, MAX_TURTHEME).map_err(fail)?;
    if !turzx::looks_like_turtheme(&bytes) {
        return Err(fail("not a theme Bezel can import".into()));
    }
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut video = |file: &str| find_video(path, file);
    turzx::import(&bytes, &name, &mut video).map_err(fail)
}

/// Extensions of the background videos the vendor app accepts.
const VIDEO_EXTENSIONS: [&str; 4] = ["mp4", "gif", "h264", "264"];

/// Looks for a vendor background video next to the theme file, then in the
/// vendor layout `<app>/video/<resolution>/` for `<app>/theme/<resolution>/`.
/// The name comes from the theme, so only a plain video file name is looked
/// up, and only inside those folders.
fn find_video(theme_file: &Path, file: &str) -> Option<Vec<u8>> {
    let is_video = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.iter().any(|v| v.eq_ignore_ascii_case(e)));
    let rel = relative_inside(file).filter(|r| is_video && r.components().count() == 1)?;
    let dir = theme_file
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut folders = vec![dir.to_path_buf()];
    if let (Some(res), Some(app)) = (dir.file_name(), dir.parent().and_then(Path::parent)) {
        folders.push(app.join("video").join(res));
    }
    folders
        .into_iter()
        .find_map(|folder| read_inside(&folder, &rel, MAX_VIDEO).ok())
}

/// Reads `base/rel` (at most `max` bytes) after checking that it really is
/// inside `base` once symbolic links are resolved.
pub(crate) fn read_inside(
    base: &Path,
    rel: &Path,
    max: u64,
) -> std::result::Result<Vec<u8>, String> {
    let path = base.join(rel);
    let shown = path.display().to_string();
    let base = base
        .canonicalize()
        .map_err(|e| format!("{}: {e}", base.display()))?;
    let real = path.canonicalize().map_err(|e| format!("{shown}: {e}"))?;
    if !real.starts_with(&base) {
        return Err(format!("{shown} points outside {}", base.display()));
    }
    read_limited(&real, max)
}

/// Reads a file of at most `max` bytes.
pub(crate) fn read_limited(path: &Path, max: u64) -> std::result::Result<Vec<u8>, String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if meta.len() > max {
        return Err(format!("{} is larger than {max} bytes", path.display()));
    }
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// A relative path that stays inside its base: no root, no `..`, no `.`.
pub(crate) fn relative_inside(path: &str) -> Option<PathBuf> {
    let p = Path::new(path.trim());
    let ok = !path.trim().is_empty()
        && !path.contains('\\')
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    ok.then(|| p.to_path_buf())
}

/// A file name usable as the last part of an asset path: the part after the
/// last `/` or `\`, without control characters; `None` when nothing is left.
pub(crate) fn file_name(path: &str) -> Option<String> {
    let name: String = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty() && name != "." && name != "..").then_some(name)
}

/// The pixel size and file extension of a PNG, GIF or JPEG, read from its
/// header only (the renderer decodes the pixels).
pub(crate) fn image_size(bytes: &[u8]) -> Option<(u32, u32, &'static str)> {
    let (w, h, ext) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let be = |i: usize| Some(u32::from_be_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
        (bytes.get(12..16) == Some(b"IHDR")).then_some(())?;
        (be(16)?, be(20)?, "png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let le = |i: usize| {
            Some(u32::from(u16::from_le_bytes(
                bytes.get(i..i + 2)?.try_into().ok()?,
            )))
        };
        (le(6)?, le(8)?, "gif")
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let (w, h) = jpeg_size(bytes)?;
        (w, h, "jpg")
    } else {
        return None;
    };
    (w > 0 && h > 0 && w <= 1 << 15 && h <= 1 << 15).then_some((w, h, ext))
}

/// Walks the JPEG markers up to the first start-of-frame.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    loop {
        if *bytes.get(i)? != 0xff {
            return None;
        }
        let marker = *bytes.get(i + 1)?;
        let len = usize::from(u16::from_be_bytes(
            bytes.get(i + 2..i + 4)?.try_into().ok()?,
        ));
        let is_frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if is_frame {
            let be16 = |j: usize| {
                Some(u32::from(u16::from_be_bytes(
                    bytes.get(j..j + 2)?.try_into().ok()?,
                )))
            };
            return Some((be16(i + 7)?, be16(i + 5)?));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
}

/// How a picture drawn 1:1 with its top-left corner at `(x, y)` fills the
/// canvas when it covers it: `Fill` when it matches the canvas exactly,
/// `None` (original size, clipped) when it is larger; `None` of the option
/// when it leaves part of the canvas uncovered.
pub(crate) fn covering_fit(x: f32, y: f32, (w, h): (u32, u32), canvas: Size) -> Option<Fit> {
    if (x, y) != (0.0, 0.0) || w < canvas.width || h < canvas.height {
        return None;
    }
    Some(if (w, h) == (canvas.width, canvas.height) {
        Fit::Fill
    } else {
        Fit::None
    })
}

/// Width of a box that fits `chars` characters of a `px` font, generously
/// (the exact advance is only known to the renderer).
pub(crate) fn text_width(chars: usize, px: f32) -> f32 {
    (chars.max(1) as f32 * px * 0.62 + px * 0.5).ceil()
}

/// Height of one line of a `px` font.
pub(crate) fn line_height(px: f32) -> f32 {
    (px * 1.25).ceil()
}

/// Collects elements and assets while an importer walks its source.
#[derive(Debug, Default)]
pub(crate) struct Builder {
    pub elements: Vec<Element>,
    pub assets: BTreeMap<AssetRef, Vec<u8>>,
    pub report: ImportReport,
}

impl Builder {
    /// Appends an element on top of the others.
    pub fn push(&mut self, name: impl Into<String>, frame: BoxF, visible: bool, kind: ElementKind) {
        let id = ElementId(u32::try_from(self.elements.len()).unwrap_or(u32::MAX - 1) + 1);
        self.elements.push(Element {
            card: None,
            card_member: None,
            id,
            name: name.into(),
            frame,
            opacity: 1.0,
            visible,
            locked: false,
            kind,
        });
    }

    /// Stores an asset under `path` (kept when already present).
    pub fn asset(&mut self, path: String, bytes: impl FnOnce() -> Vec<u8>) -> AssetRef {
        let asset = AssetRef(path);
        if !self.assets.contains_key(&asset) {
            self.assets.insert(asset.clone(), bytes());
        }
        asset
    }
}

/// The header of a PNG of `w` x `h` (enough for [`image_size`]).
#[cfg(test)]
pub(crate) fn test_png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        test_png(w, h)
    }

    #[test]
    fn image_headers() {
        assert_eq!(image_size(&png(480, 1920)), Some((480, 1920, "png")));
        assert_eq!(image_size(&png(0, 10)), None);
        assert_eq!(image_size(&png(1 << 16, 10)), None);
        assert_eq!(image_size(b"\x89PNG\r\n\x1a\n\0\0\0\rIDAT"), None);
        assert_eq!(image_size(b"GIF89a\x10\x00\x20\x00"), Some((16, 32, "gif")));
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00,
            0x40, 0x00, 0x80,
        ];
        assert_eq!(image_size(&jpeg), Some((128, 64, "jpg")));
        assert_eq!(image_size(&jpeg[..10]), None);
        assert_eq!(image_size(&[0xff, 0xd8, 0x00]), None);
        assert_eq!(image_size(&[0xff, 0xd8, 0xff, 0xe0, 0x00, 0x01]), None);
        assert_eq!(image_size(b"BM"), None);
    }

    #[test]
    fn covering_pictures() {
        let canvas = Size::new(480, 1920);
        assert_eq!(covering_fit(0.0, 0.0, (480, 1920), canvas), Some(Fit::Fill));
        assert_eq!(covering_fit(0.0, 0.0, (481, 1921), canvas), Some(Fit::None));
        assert_eq!(covering_fit(0.0, 0.0, (470, 1920), canvas), None);
        assert_eq!(covering_fit(1.0, 0.0, (480, 1920), canvas), None);
    }

    #[test]
    fn paths_and_names() {
        assert_eq!(relative_inside("a/b.png"), Some(PathBuf::from("a/b.png")));
        for bad in ["", "/etc/x", "../x", "a/../b", "./a", "a\\b"] {
            assert_eq!(relative_inside(bad), None, "{bad}");
        }
        assert_eq!(
            file_name("D:\\8.8\\video\\AMD.mp4").as_deref(),
            Some("AMD.mp4")
        );
        assert_eq!(file_name("/a/b/c.mp4").as_deref(), Some("c.mp4"));
        assert_eq!(file_name("x\u{7}.mp4").as_deref(), Some("x.mp4"));
        for bad in ["", "a/", "..", "a\\."] {
            assert_eq!(file_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn report_dedupes() {
        let mut r = ImportReport::default();
        assert!(r.is_clean());
        let a = ImportWarning::new(WarningCode::NoLayers);
        let b = ImportWarning::new(WarningCode::UnusedTopKey).arg("key", "extra");
        r.warn(a.clone());
        r.warn(a.clone());
        r.warn(b.clone());
        assert_eq!(r.warnings, vec![a, b]);
        assert!(!r.is_clean());
    }

    #[test]
    fn builder_numbers_elements_and_keeps_first_asset() {
        let mut b = Builder::default();
        let a = b.asset("assets/x.png".into(), || vec![1]);
        let again = b.asset("assets/x.png".into(), || vec![2]);
        assert_eq!(a, again);
        assert_eq!(b.assets[&a], vec![1]);
        for _ in 0..2 {
            b.push(
                "e",
                BoxF::default(),
                true,
                ElementKind::Image {
                    asset: a.clone(),
                    fit: Default::default(),
                },
            );
        }
        let ids: Vec<u32> = b.elements.iter().map(|e| e.id.0).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(text_width(0, 10.0), 12.0);
        assert_eq!(line_height(10.0), 13.0);
    }

    #[test]
    fn import_failures_are_theme_file_errors() {
        let missing = std::env::temp_dir().join("bezel-import-mod-missing.turtheme");
        let e = import_path(&missing).expect_err("refused");
        assert!(matches!(e, BezelError::ThemeFile(_)), "{e:?}");
    }

    #[test]
    fn import_path_refuses_what_it_cannot_read() {
        let dir = std::env::temp_dir().join(format!("bezel-import-mod-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let junk = dir.join("junk.turtheme");
        fs::write(&junk, b"not nrbf").expect("write");
        let e = import_path(&junk).expect_err("refused");
        assert!(e.to_string().contains("not a theme"), "{e}");
        assert!(import_path(&dir.join("missing.turtheme")).is_err());
        assert!(read_limited(&dir, 10).is_err(), "a folder is not a file");
        fs::write(dir.join("big.bin"), [0u8; 32]).expect("write");
        assert!(read_limited(&dir.join("big.bin"), 8).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn reads_stay_inside_their_folder() {
        let root = std::env::temp_dir().join(format!("bezel-import-inside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let inner = root.join("theme");
        fs::create_dir_all(&inner).expect("dir");
        fs::write(root.join("secret.txt"), b"S").expect("write");
        fs::write(inner.join("ok.png"), b"P").expect("write");
        std::os::unix::fs::symlink(root.join("secret.txt"), inner.join("link.png")).expect("link");
        assert_eq!(
            read_inside(&inner, Path::new("ok.png"), 10),
            Ok(b"P".to_vec())
        );
        let e = read_inside(&inner, Path::new("link.png"), 10).expect_err("refused");
        assert!(e.contains("points outside"), "{e}");
        assert!(read_inside(&inner, Path::new("none.png"), 10).is_err());
        assert!(read_inside(&root.join("nope"), Path::new("ok.png"), 10).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn videos_are_found_next_to_the_theme_or_in_the_vendor_layout() {
        let root = std::env::temp_dir().join(format!("bezel-import-video-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let themes = root.join("theme").join("4801920");
        let videos = root.join("video").join("4801920");
        fs::create_dir_all(&themes).expect("dir");
        fs::create_dir_all(&videos).expect("dir");
        fs::write(videos.join("a.mp4"), b"A").expect("write");
        fs::write(themes.join("b.mp4"), b"B").expect("write");
        let theme = themes.join("x.turtheme");
        assert_eq!(find_video(&theme, "a.mp4"), Some(b"A".to_vec()));
        assert_eq!(find_video(&theme, "b.mp4"), Some(b"B".to_vec()));
        assert_eq!(find_video(&theme, "c.mp4"), None);
        fs::write(themes.join("notes.txt"), b"T").expect("write");
        assert_eq!(find_video(&theme, "notes.txt"), None, "not a video name");
        assert_eq!(find_video(&theme, "../theme/4801920/b.mp4"), None);
        assert_eq!(find_video(Path::new("x.turtheme"), "missing.mp4"), None);
        let _ = fs::remove_dir_all(&root);
    }
}
