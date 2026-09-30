//! Shared by the integration tests: the repository's bundled themes, a
//! renderer with only the bundled fonts (reproducible pixels) and a demo
//! sensor source whose values move like a busy desktop.

#![allow(dead_code)]

use std::f64::consts::TAU;
use std::path::{Path, PathBuf};

use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::sensor::{Reading, SensorKey, Snapshot};
use bezel_core::ports::SensorSource;
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::FakeSensors;

/// Wednesday 2026-09-30 21:05:42.
pub const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 42,
    weekday: 2,
};

/// The repository's `themes/` folder.
pub fn themes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes")
}

/// Every bundled theme folder, sorted.
pub fn bundled_themes() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(themes_dir())
        .expect("themes/ exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("theme.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

/// A renderer that knows only the bundled fonts.
pub fn renderer() -> SkiaRenderer {
    SkiaRenderer::with_fonts(font_files(&themes_dir().join("fonts")), SystemFonts::Skip)
}

/// A value swinging smoothly over `frames` samples.
fn wave(i: usize, base: f64, swing: f64, period: f64, phase: f64) -> f64 {
    let t = i as f64 / period;
    base + swing * (TAU * t + phase).sin() + swing * 0.35 * (TAU * t * 2.7 + phase).cos()
}

/// The demo catalog with `frames` snapshots whose usages, temperatures and
/// rates move, ending on the demo's values for the rest.
pub fn busy_sensors(frames: usize) -> FakeSensors {
    let mut demo = FakeSensors::demo();
    let catalog = demo.catalog().expect("catalog");
    demo.sample().expect("warm-up sample");
    let base = demo.sample().expect("demo sample");
    let mib = 1024.0 * 1024.0;
    let script = (0..frames)
        .map(|i| {
            let mut s: Snapshot = base.clone();
            let mut set = |k: &str, v: f64| {
                s.insert(SensorKey::new(k).expect("key"), Reading::Value(v));
            };
            set(
                "cpu.usage",
                wave(i, 48.0, 22.0, 23.0, 0.0).clamp(0.0, 100.0),
            );
            set("cpu.temperature", wave(i, 58.0, 6.0, 31.0, 1.0));
            set(
                "gpu.usage",
                wave(i, 70.0, 20.0, 17.0, 2.0).clamp(0.0, 100.0),
            );
            set("gpu.temperature", wave(i, 64.0, 5.0, 29.0, 0.5));
            set("gpu.power", wave(i, 240.0, 80.0, 17.0, 2.0));
            let memory = wave(i, 42.0, 4.0, 41.0, 0.3);
            set("memory.percent", memory);
            set("memory.used", memory / 100.0 * 64.0 * 1024.0 * mib);
            set("net.down", (wave(i, 4.0, 3.0, 29.0, 0.7) * mib).max(0.0));
            set("net.up", (wave(i, 0.6, 0.45, 37.0, 2.2) * mib).max(0.0));
            set("disk.read", (wave(i, 30.0, 26.0, 19.0, 1.4) * mib).max(0.0));
            set(
                "disk.write",
                (wave(i, 12.0, 10.0, 15.0, 0.1) * mib).max(0.0),
            );
            s
        })
        .collect();
    FakeSensors::new(catalog, script)
}
