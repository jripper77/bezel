//! End-to-end tests of `bezel storage` against the simulated 8.8" of
//! `--fake` (files Bezel sent on its flash and the user's 8 GiB card the
//! vendor app filled, fresh in every process, with Bezel's catalog in
//! memory). Nothing here opens a real screen.
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::theme::{AssetRef, Background, Theme};
use bezel_core::ports::{ThemeLocation, ThemeStore};
use bezel_themes::FsThemeStore;

/// Runs `bezel --fake <args>`.
fn bezel(args: &[&str]) -> Output {
    Command::cargo_bin("bezel")
        .expect("binary built")
        .arg("--fake")
        .args(args)
        .output()
        .expect("bezel ran")
}

/// Runs `bezel --fake <args>` with `data` as the user's data folder (both
/// the XDG and the Windows variables point at it).
fn bezel_with_data(data: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("bezel")
        .expect("binary built")
        .env("XDG_DATA_HOME", data)
        .env("APPDATA", data)
        .arg("--fake")
        .args(args)
        .output()
        .expect("bezel ran")
}

/// A fresh data folder for one test.
fn data_folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bezel-data-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("data folder");
    dir
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
    assert_eq!(json.as_array().map(Vec::len), Some(2));
    assert_eq!(json[0]["name"], "bezel_cut.mp4");
    assert_eq!(json[0]["state"], "pending");
    assert_eq!(json[1]["name"], "bezel_demo.mp4");
    assert_eq!(json[1]["localCopy"], true);
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

#[test]
fn mv_and_rename_print_the_exact_list_and_need_yes() {
    let out = bezel(&[
        "storage",
        "mv",
        "internal/video/bezel_demo.mp4",
        "internal/image/bezel_demo.png",
        "--to",
        "sd",
    ]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let err = text(&out.stderr);
    assert!(
        err.starts_with(
            "Move 2 files to the memory card of Turing Smart Screen 8.8\", each sent from \
             Bezel's local copy:\n  internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4     \
             2.3 MiB\n  internal/image/bezel_demo.png -> sd/image/bezel_demo.png    48.0 KiB\n"
        ),
        "{err}"
    );
    assert!(
        err.contains("each source is deleted only after its copy is verified"),
        "{err}"
    );
    assert!(
        err.ends_with(
            "Nothing on the screen was changed. Add --yes to move them.\nbezel: moving 2 files \
             needs --yes\n"
        ),
        "{err}"
    );

    let out = bezel(&[
        "storage",
        "mv",
        "internal/video/bezel_demo.mp4",
        "--to",
        "sd",
        "--yes",
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "moved internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4 (2.3 MiB)\n"
    );
    // Not a terminal: plain numbered progress lines.
    let log = text(&out.stderr);
    assert!(
        log.contains("[1/1] verify  [########################] 100%  stored size checked"),
        "{log}"
    );
    assert!(!log.contains('\r'), "{log:?}");

    let out = bezel(&[
        "storage",
        "rename",
        "internal/video/bezel_demo.mp4",
        "Intro.mp4",
    ]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(
        err.contains("  internal/video/bezel_demo.mp4 -> internal/video/intro.mp4     2.3 MiB\n"),
        "{err}"
    );
    assert!(err.contains("bezel: renaming 1 file needs --yes"), "{err}");
}

#[test]
fn cleanup_dry_run_lists_the_vendor_duplicates_but_never_a_theme_video() {
    let data = data_folder("cleanup");
    let mut theme = Theme::blank("AMD loop", Size::new(480, 1920), Orientation::Portrait);
    theme.background = Background::Video {
        asset: AssetRef("assets/AMD.mp4".into()),
        poster: None,
    };
    let folder = data.join("bezel").join("themes").join("amd-loop");
    FsThemeStore
        .save(
            &ThemeLocation(folder.to_string_lossy().into_owned()),
            &theme,
            &BTreeMap::new(),
        )
        .expect("theme saved");

    let out = bezel_with_data(&data, &["storage", "cleanup", "--dry-run"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let found = text(&out.stdout);
    for artifact in [
        "demon_open.mp4.mp4.mp4",
        "demon.mp401115025.mp4",
        "NVI.mp427034822.mp4",
        "Rani.mp417075004.mp4",
        "m04.mp424045157.mp4",
    ] {
        let line = found
            .lines()
            .find(|l| l.starts_with(&format!("  sd/video/{artifact} ")))
            .unwrap_or_else(|| panic!("{artifact} listed: {found}"));
        assert!(
            line.contains("variant: a vendor copy of sd/video/"),
            "{line}"
        );
    }
    assert!(
        !found.lines().any(|l| l.starts_with("  sd/video/AMD.mp4 ")),
        "a theme plays it: {found}"
    );
    assert!(
        found.contains("  sd/video/8.8APEX_2.mp4              2.2 MiB  unused"),
        "{found}"
    );
    assert!(
        found.ends_with("Dry run: nothing was deleted.\n"),
        "{found}"
    );

    // Without --yes nothing goes either; the list goes to stderr.
    let out = bezel_with_data(&data, &["storage", "cleanup"]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("internal/video/bezel_cut.mp4"), "{err}");
    assert!(err.contains("bezel: deleting 1 file needs --yes"), "{err}");
    let _ = std::fs::remove_dir_all(data);
}

#[test]
fn restore_catalog_and_cache_show_what_bezel_keeps() {
    let out = bezel(&["storage", "restore", "sd"]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(
        err.contains(
            "  sd/video/bezel_intro.mp4 -> sd/video/bezel_intro.mp4     1.2 MiB  (missing)"
        ),
        "{err}"
    );
    assert!(err.contains("bezel: restoring 1 file needs --yes"), "{err}");
    let out = bezel(&["storage", "restore", "sd", "--yes"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "restored sd/video/bezel_intro.mp4 -> sd/video/bezel_intro.mp4 (1.2 MiB)\n"
    );

    let out = bezel(&["storage", "catalog"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let listed = text(&out.stdout);
    assert!(
        listed.starts_with(
            "Bezel's catalog of Turing Smart Screen 8.8\" (turing-8.8), oldest first:\n"
        ),
        "{listed}"
    );
    assert!(listed.contains("sd/video/bezel_intro.mp4"), "{listed}");

    let out = bezel(&["storage", "cache"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "Bezel's local copies, in memory (--fake):\n  5 copies, 6.1 MiB\n  of files deleted \
         through Bezel: 0 copies, 0 B of the 2.0 GiB limit (above it the oldest go)\n"
    );
    let out = bezel(&["storage", "cache", "clear", "--all"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("clearing the local copies needs --yes"));
}
