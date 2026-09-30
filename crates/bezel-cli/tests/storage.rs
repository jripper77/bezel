//! End-to-end tests of `bezel storage` against the simulated 8.8" of
//! `--fake` (demo files on its flash and an 8 GiB card, fresh in every
//! process). Nothing here opens a real screen.
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;

/// Runs `bezel --fake <args>`.
fn bezel(args: &[&str]) -> Output {
    Command::cargo_bin("bezel")
        .expect("binary built")
        .arg("--fake")
        .args(args)
        .output()
        .expect("bezel ran")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A small PNG on disk.
fn picture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bezel-storage-{}-{name}", std::process::id()));
    image::RgbaImage::from_pixel(40, 30, image::Rgba([10, 200, 30, 255]))
        .save(&path)
        .expect("PNG written");
    path
}

#[test]
fn info_and_ls_show_the_demo_storage() {
    let out = bezel(&["storage", "info", "--json"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(json["screen"], "Turing Smart Screen 8.8\"");
    assert_eq!(json["internal"]["totalBytes"], 1_u64 << 30);
    assert_eq!(json["card"]["totalBytes"], 8_u64 << 30);

    let out = bezel(&["storage", "ls"]);
    assert!(out.status.success());
    let listing = text(&out.stdout);
    assert!(
        listing.contains("internal/image/bezel_demo.png"),
        "{listing}"
    );
    assert!(
        listing.contains("internal/video/bezel_demo.mp4"),
        "{listing}"
    );
    assert!(listing.contains("sd/video/bezel_loop.mp4"), "{listing}");

    let out = bezel(&["storage", "ls", "internal/video", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(json.as_array().map(Vec::len), Some(1));
    assert_eq!(json[0]["name"], "bezel_demo.mp4");
}

#[test]
fn rm_needs_yes() {
    let out = bezel(&["storage", "rm", "internal/video/bezel_demo.mp4"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = text(&out.stderr);
    assert!(
        err.contains("Delete internal/video/bezel_demo.mp4 from the screen's internal flash"),
        "{err}"
    );
    assert!(err.contains("Nothing was sent to the screen"), "{err}");
    assert!(
        err.contains("bezel: deleting internal/video/bezel_demo.mp4 needs --yes"),
        "{err}"
    );

    let out = bezel(&["storage", "rm", "internal/video/bezel_demo.mp4", "--yes"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "deleted internal/video/bezel_demo.mp4 (2.3 MiB)\n"
    );
    assert!(text(&out.stderr).contains("Delete internal/video/bezel_demo.mp4 (2.3 MiB)"));
}

#[test]
fn put_sends_a_picture_and_refuses_to_replace_without_yes() {
    let png = picture("Logo.png");
    let file = png.to_string_lossy().into_owned();
    let out = bezel(&["storage", "put", &file, "sd"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let name = format!("bezel-storage-{}-logo.png", std::process::id());
    assert!(
        text(&out.stdout).contains(&format!("stored sd/image/{name}")),
        "{}",
        text(&out.stdout)
    );
    let log = text(&out.stderr);
    assert!(
        log.contains(&format!("  to       sd/image/{name}")),
        "{log}"
    );
    // Not a terminal: plain progress lines, no carriage returns.
    assert!(
        log.contains("upload  [########################] 100%"),
        "{log}"
    );
    assert!(
        log.contains("verify  [########################] 100%"),
        "{log}"
    );
    assert!(!log.contains('\r'), "{log:?}");

    let out = bezel(&["storage", "put", &file, "internal/image/bezel_demo.png"]);
    assert!(!out.status.success());
    let log = text(&out.stderr);
    assert!(
        log.contains("  replaces internal/image/bezel_demo.png (48.0 KiB)"),
        "{log}"
    );
    assert!(
        log.contains("replacing internal/image/bezel_demo.png needs --yes"),
        "{log}"
    );
    let out = bezel(&[
        "storage",
        "put",
        &file,
        "internal/image/bezel_demo.png",
        "--yes",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));

    let out = bezel(&["storage", "put", "/no/such/picture.png"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("cannot read /no/such/picture.png"));
    let _ = std::fs::remove_file(png);
}

#[test]
fn play_stop_and_boot() {
    let out = bezel(&["storage", "play", "sd/video/bezel_loop.mp4"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("looping sd/video/bezel_loop.mp4"));
    let out = bezel(&["storage", "stop"]);
    assert!(out.status.success());

    let out = bezel(&["storage", "boot", "internal/image/bezel_demo.png"]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(
        err.contains("Boot media: internal/image/bezel_demo.png (picture)"),
        "{err}"
    );
    assert!(err.contains("changing the boot media needs --yes"), "{err}");

    let out = bezel(&[
        "storage",
        "boot",
        "internal/image/bezel_demo.png",
        "--brightness",
        "50",
        "--yes",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).ends_with("boots with internal/image/bezel_demo.png\n"));
    let out = bezel(&["storage", "boot", "internal/image/missing.png", "--yes"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("not stored"));
}
