//! The studio's backend on a real screen: live mode with a bundled theme,
//! a turn from horizontal to vertical while live, then the screen handed
//! back. Opens the first connected screen, so it only runs on request:
//!
//! ```bash
//! BEZEL_HW_TESTS=1 cargo test -p bezel-studio --test hardware -- --ignored --nocapture
//! ```
//!
//! Stop any other program that drives the screen first.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bezel_core::domain::geometry::Orientation;
use bezel_devices::{SystemBus, SystemConnector};
use bezel_media::FfmpegTranscoder;
use bezel_render::{SkiaRenderer, SystemFonts, font_files};
use bezel_sensors::SystemSensors;
use bezel_studio::backend::{Backend, Session, sleep_until};
use bezel_studio::clock::{language, now};
use bezel_studio::library::ThemeLibrary;
use bezel_studio::settings::SettingsFile;
use bezel_studio::storage::StorageState;
use bezel_studio::studio::Studio;
use bezel_themes::FsThemeStore;

/// How long each orientation stays live.
const LIVE_FOR: Duration = Duration::from_secs(8);

fn backend(scratch: &Path) -> Backend {
    let themes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../themes");
    let renderer = SkiaRenderer::with_fonts(font_files(&themes.join("fonts")), SystemFonts::Load);
    let fonts = renderer.font_families();
    let blank = bezel_core::domain::theme::Theme::blank(
        "hardware",
        bezel_core::domain::geometry::Size::new(480, 1920),
        Orientation::Landscape,
    );
    Backend {
        bus: Arc::new(SystemBus),
        connector: Arc::new(SystemConnector),
        hid: Arc::new(bezel_devices::SystemHid),
        store: Arc::new(FsThemeStore),
        library: ThemeLibrary::new(scratch.join("themes"), vec![themes]),
        settings: SettingsFile::new(scratch.join("settings.json")),
        system_language: language(),
        make_sensors: Arc::new(|options| Box::new(SystemSensors::with_options(options))),
        udev: None,
        fonts,
        studio: Session::new(Studio::new(
            Box::new(SystemSensors::new()),
            Box::new(renderer),
            language(),
            blank,
        )),
        storage: StorageState::new(Box::new(FfmpegTranscoder::new(None)), scratch.join("media")),
    }
}

fn stay_live(b: &Backend, label: &str) {
    let started = Instant::now();
    let mut ticks = 0;
    while started.elapsed() < LIVE_FOR {
        let due = b.tick(now(), Instant::now());
        ticks += 1;
        std::thread::sleep(sleep_until(due, Instant::now()));
    }
    let sample = b.sample();
    assert_eq!(sample.live_error, None, "{label}: the live screen stopped");
    assert!(sample.live.is_some(), "{label}: not live any more");
    println!(
        "{label}: {ticks} refreshes in {:.1} s",
        started.elapsed().as_secs_f64()
    );
}

#[test]
#[ignore = "drives a real screen; run with BEZEL_HW_TESTS=1"]
fn live_mode_turns_and_releases_on_the_real_screen() {
    if std::env::var_os("BEZEL_HW_TESTS").is_none() {
        eprintln!("BEZEL_HW_TESTS is not set: skipped");
        return;
    }
    let scratch = std::env::temp_dir().join(format!("bezel-hw-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let b = backend(&scratch);
    let screens = b.devices().expect("screens").screens;
    let key = screens.first().expect("a connected screen").key.clone();
    println!("screen {key}");

    // First run: the bundled theme made for this screen and its default
    // orientation (horizontal on the 8.8").
    b.restore_theme();
    let horizontal = b.session().theme;
    println!("theme {:?} {}", horizontal.name, horizontal.orientation);
    b.set_live(true, Some(&key), now()).expect("live on");
    stay_live(&b, "horizontal");

    // Turned to vertical while live: the screen follows at once.
    let vertical = b
        .new_theme(Some(&key), "hardware vertical", Some(Orientation::Portrait))
        .expect("vertical theme");
    b.push(&vertical, now()).expect("push the vertical theme");
    stay_live(&b, "vertical");

    b.release(&key).expect("release");
    assert_eq!(b.sample().live, None);
    let _ = std::fs::remove_dir_all(&scratch);
}
