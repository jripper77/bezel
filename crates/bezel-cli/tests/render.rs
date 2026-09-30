//! End-to-end tests of `bezel render`, `bezel run` and `bezel import` against
//! the simulated screen and the demo sensors.
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

mod support;

use std::path::{Path, PathBuf};

use assert_cmd::Command;

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bezel-e2e-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn bezel(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bezel")
        .expect("binary built")
        // Bundled themes come from the repository, whatever is installed.
        .env("BEZEL_THEMES_DIR", support::themes_dir())
        .args(args)
        .output()
        .expect("bezel ran")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn theme(name: &str) -> String {
    support::themes_dir().join(name).display().to_string()
}

fn png_size(path: &Path) -> (u32, u32) {
    let image = image::open(path).expect("a PNG");
    (image.width(), image.height())
}

#[test]
fn render_writes_a_png_of_the_canvas_size() {
    let dir = scratch("render");
    let png = dir.join("horizontal.png");
    let out = bezel(&[
        "--fake",
        "render",
        &theme("turing-8.8-horizontal"),
        "-o",
        &png.display().to_string(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("(1920x480 horizontal)"),
        "{}",
        text(&out.stdout)
    );
    assert_eq!(png_size(&png), (1920, 480));

    // A bundled theme by name.
    let round = dir.join("round.png");
    let out = bezel(&[
        "--fake",
        "render",
        "turing-2.1-round",
        "-o",
        &round.display().to_string(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(png_size(&round), (480, 480));
}

#[test]
fn render_names_the_bundled_themes_when_one_is_unknown() {
    let out = bezel(&["--fake", "render", "no-such-theme", "-o", "unused.png"]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("turing-8.8-horizontal"), "{err}");
}

#[test]
fn run_shows_a_theme_then_releases_the_screen() {
    let out = bezel(&[
        "--fake",
        "run",
        &theme("turing-8.8-vertical"),
        "--frames",
        "2",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("2 frames of Midnight 8.8\" vertical"),
        "{stdout}"
    );
    assert!(stdout.ends_with("released\n"), "{stdout}");
    assert!(text(&out.stderr).contains("Ctrl+C to stop"));
}

#[test]
fn run_refuses_a_theme_made_for_another_screen() {
    let out = bezel(&[
        "--fake",
        "run",
        &theme("turing-3.5-vertical"),
        "--frames",
        "1",
    ]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("made for a 320x480 canvas"), "{err}");
}

#[test]
fn import_converts_a_python_theme_and_reports_what_it_dropped() {
    let dir = scratch("import");
    let python = dir.join("python");
    std::fs::create_dir_all(&python).expect("theme folder");
    std::fs::write(
        python.join("theme.yaml"),
        "---\ndisplay:\n  DISPLAY_SIZE: 3.5\"\n  DISPLAY_ORIENTATION: landscape\nstatic_text:\n  LABEL:\n    TEXT: \"CPU\"\n    X: 20\n    Y: 18\n",
    )
    .expect("theme.yaml");
    let native = dir.join("converted.bezeltheme");
    let out = bezel(&[
        "import",
        &python.display().to_string(),
        "-o",
        &native.display().to_string(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.starts_with("warning: "), "{stdout}");
    assert!(stdout.contains("(480x320 horizontal)"), "{stdout}");

    let png = dir.join("converted.png");
    let out = bezel(&[
        "--fake",
        "render",
        &native.display().to_string(),
        "-o",
        &png.display().to_string(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(png_size(&png), (480, 320));

    let again = bezel(&[
        "import",
        &python.display().to_string(),
        "-o",
        &native.display().to_string(),
    ]);
    assert!(!again.status.success());
    assert!(text(&again.stderr).contains("already exists"));
}
