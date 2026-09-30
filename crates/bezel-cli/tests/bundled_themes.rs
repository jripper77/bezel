//! The themes that ship in `themes/`: each loads with the native store, fits
//! a known panel, reads only sensors the demo catalog has, uses only the
//! bundled fonts and renders at its canvas size without a single problem.
//!
//! Set `BEZEL_THEME_PREVIEWS=<folder>` to also write a PNG of each theme
//! (English and Brazilian Portuguese dates) after a minute of busy sensors;
//! otherwise two frames are enough to check them.
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bezel_core::app::ThemeRuntime;
use bezel_core::domain::catalog::MODELS;
use bezel_core::domain::clock::Language;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::theme::{AssetRef, ElementKind, Theme};
use bezel_core::ports::{SensorSource, ThemeLocation, ThemeStore};
use bezel_sensors::FakeSensors;
use bezel_themes::FsThemeStore;

/// Samples graphs get before a preview is written.
const PREVIEW_FRAMES: usize = 60;

type Assets = BTreeMap<AssetRef, Vec<u8>>;

fn load(dir: &Path) -> (Theme, Assets) {
    FsThemeStore
        .load(&ThemeLocation(dir.display().to_string()))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
}

fn families(theme: &Theme) -> BTreeSet<String> {
    theme
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Text { style, .. } => Some(style.font.family.clone()),
            _ => None,
        })
        .collect()
}

fn fits_a_panel(theme: &Theme) -> bool {
    MODELS
        .iter()
        .any(|m| m.panel.in_orientation(theme.orientation) == theme.canvas)
}

fn save_png(frame: &Frame, path: &Path) {
    let size = frame.size();
    image::RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())
        .expect("frame bytes")
        .save(path)
        .expect("preview written");
}

fn render(
    theme: &Theme,
    assets: &Assets,
    language: Language,
    frames: usize,
) -> (Frame, Vec<String>) {
    let mut renderer = support::renderer();
    let mut sensors = support::busy_sensors(frames);
    let mut runtime = ThemeRuntime::new(theme.clone(), assets.clone(), language);
    let mut frame = None;
    for _ in 0..frames {
        frame = Some(
            runtime
                .frame(&mut sensors, &mut renderer, support::TIME)
                .expect("frame"),
        );
    }
    (frame.expect("rendered"), renderer.problems())
}

#[test]
fn every_bundled_theme_renders_cleanly() {
    let themes = support::bundled_themes();
    let count = themes.len();
    let offered: BTreeSet<String> = FakeSensors::demo()
        .catalog()
        .expect("catalog")
        .into_iter()
        .map(|s| s.key.as_str().to_string())
        .collect();
    let previews = std::env::var_os("BEZEL_THEME_PREVIEWS").map(PathBuf::from);
    let (frames, languages) = match previews {
        Some(_) => (
            PREVIEW_FRAMES,
            &[(Language::English, ""), (Language::PortugueseBr, ".pt")][..],
        ),
        None => (2, &[(Language::English, "")][..]),
    };
    for dir in themes {
        let name = dir
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let (theme, assets) = load(&dir);
        assert!(fits_a_panel(&theme), "{name}: {:?}", theme.canvas);
        for asset in theme.assets() {
            assert!(assets.contains_key(&asset), "{name}: {} missing", asset.0);
        }
        for key in theme.sensor_keys() {
            assert!(
                offered.contains(key.as_str()),
                "{name}: {key} is not a demo sensor"
            );
        }
        let fonts = families(&theme);
        assert!(
            fonts.iter().all(|f| f == "Inter" || f == "JetBrains Mono"),
            "{name}: {fonts:?}"
        );
        for (language, suffix) in languages {
            let (frame, problems) = render(&theme, &assets, *language, frames);
            assert_eq!(frame.size(), theme.canvas, "{name}");
            assert!(problems.is_empty(), "{name}: {problems:?}");
            if let Some(dir) = &previews {
                std::fs::create_dir_all(dir).expect("preview folder");
                save_png(&frame, &dir.join(format!("{name}{suffix}.png")));
            }
        }
    }
    assert!(count >= 6, "{count} bundled themes");
}
