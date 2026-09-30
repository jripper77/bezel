//! Imports every theme of the local third-party corpus (never committed):
//! the vendor app's `.turtheme` files and turing-smart-screen-python's
//! theme folders, then round-trips each result through `FsThemeStore`.
//!
//! Locations default to the developer's checkout and can be overridden with
//! `BEZEL_TURZX_APP` (the vendor app folder) and `BEZEL_PYTHON_THEMES`
//! (`res/themes` of the Python repository).
#![allow(clippy::panic)] // a failing test panics

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use bezel_core::ports::{ThemeLocation, ThemeStore};
use bezel_themes::FsThemeStore;
use bezel_themes::import::import_path;

const TURZX_APP: &str = "/home/slipalison/repos/turx/TURZX-V3.07-88inchENG";
const PYTHON_THEMES: &str = "/home/slipalison/repos/turx/turing-smart-screen-python/res/themes";

fn location(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(|| PathBuf::from(default), PathBuf::from)
}

fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .collect();
    out.sort();
    out
}

fn corpus() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let app = location("BEZEL_TURZX_APP", TURZX_APP);
    let mut turzx = files_with_extension(&app.join("theme/4801920"), "turtheme");
    turzx.extend(files_with_extension(
        &app.join("restore/4801920"),
        "turtheme",
    ));
    turzx.push(app.join("visual/panel_common.turtheme"));
    let themes = location("BEZEL_PYTHON_THEMES", PYTHON_THEMES);
    let mut python: Vec<PathBuf> = fs::read_dir(&themes)
        .unwrap_or_else(|e| panic!("{}: {e}", themes.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("theme.yaml").is_file())
        .collect();
    python.sort();
    (turzx, python)
}

/// Theme-specific details (names, paths, numbers) collapsed so that the
/// summary groups warnings by kind.
fn warning_kind(warning: &str) -> String {
    let kind: String = warning
        .split(':')
        .next()
        .unwrap_or(warning)
        .chars()
        .take(90)
        .collect();
    if kind.starts_with("STATS.") || kind.starts_with("static_") || kind.starts_with("Text: ") {
        warning
            .split_once(": ")
            .map(|(_, rest)| rest)
            .unwrap_or(warning)
            .chars()
            .take(90)
            .collect()
    } else {
        kind
    }
}

#[test]
#[ignore = "needs the local third-party theme corpus"]
fn imports_the_local_corpus() {
    let (turzx, python) = corpus();
    assert!(turzx.len() > 1, "no .turtheme files found");
    assert!(!python.is_empty(), "no Python themes found");
    let scratch = std::env::temp_dir().join(format!("bezel-corpus-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let all = turzx.iter().chain(python.iter());
    for (i, path) in all.enumerate() {
        let (theme, assets, report) = match import_path(path) {
            Ok(imported) => imported,
            Err(e) => {
                failures.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        for w in &report.warnings {
            *kinds.entry(warning_kind(w)).or_default() += 1;
        }
        let referenced = theme.assets();
        for asset in assets.keys() {
            assert!(
                referenced.contains(asset),
                "{}: unreferenced {asset:?}",
                path.display()
            );
        }
        let loc = ThemeLocation(scratch.join(format!("t{i}")).display().to_string());
        FsThemeStore
            .save(&loc, &theme, &assets)
            .unwrap_or_else(|e| panic!("{}: save: {e}", path.display()));
        let (loaded, loaded_assets) = FsThemeStore
            .load(&loc)
            .unwrap_or_else(|e| panic!("{}: load: {e}", path.display()));
        assert_eq!(loaded, theme, "{}", path.display());
        assert_eq!(loaded_assets, assets, "{}", path.display());
        let _ = fs::remove_dir_all(scratch.join(format!("t{i}")));
    }
    let _ = fs::remove_dir_all(&scratch);
    let mut ranked: Vec<(&String, &usize)> = kinds.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!(
        "imported {} TURZX and {} Python themes; {} failed",
        turzx.len(),
        python.len(),
        failures.len()
    );
    for (kind, count) in ranked.iter().take(25) {
        eprintln!("{count:5} × {kind}");
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
