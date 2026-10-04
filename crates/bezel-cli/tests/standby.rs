//! End-to-end tests of `bezel standby` (D-2026-10-03-power-off-standby-6):
//! the real binary against the simulated 8.8" of `--fake` (fresh in every
//! process: 1 GiB of flash with the files Bezel sent, an 8 GiB card with the
//! vendor app's videos), with Bezel's catalog on disk in a data folder of
//! the test's own, read back here through the `DiskArchive` the studio reads
//! at shutdown. Nothing here opens a real screen or the user's data.
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use bezel_core::domain::archive::{Catalog, ScreenKey};
use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::device::ModelId;
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::standby::{SleepMinutes, Standby};
use bezel_core::domain::storage::{BootMedia, RemotePath};
use bezel_core::ports::ArchiveStore;
use bezel_media::archive::{DiskArchive, storage_dir};
use bezel_media::photo;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, RgbImage};

/// A data and a config folder for one test, empty.
struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("bezel-standby-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["data", "config", "photos"] {
            std::fs::create_dir_all(root.join(dir)).expect("folder");
        }
        Self { root }
    }

    fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    /// `<data>/bezel/storage`: Bezel's catalog and local copies.
    fn catalog_dir(&self) -> PathBuf {
        storage_dir(&self.data())
    }

    /// Runs `bezel --fake <args>` with this home as the user's data and
    /// config folders (the XDG and the Windows variables).
    fn bezel(&self, args: &[&str]) -> Output {
        Command::cargo_bin("bezel")
            .expect("binary built")
            .env("XDG_DATA_HOME", self.data())
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("APPDATA", self.data())
            .arg("--fake")
            .args(args)
            .output()
            .expect("bezel ran")
    }

    /// The catalog as the studio reads it.
    fn catalog(&self) -> Catalog {
        DiskArchive::open(self.catalog_dir())
            .expect("archive")
            .load()
            .expect("catalog")
    }

    /// The choice recorded for the 8.8" (`keep` without a record).
    fn recorded(&self) -> Standby {
        self.catalog()
            .screen(&key())
            .map(|r| r.standby.clone())
            .unwrap_or_default()
    }

    /// The bytes of Bezel's local copy of `path`.
    fn local_copy(&self, path: &str) -> Vec<u8> {
        let catalog = self.catalog();
        let entry = catalog
            .screen(&key())
            .and_then(|r| r.entries.iter().find(|e| e.path == remote(path)))
            .unwrap_or_else(|| panic!("{path} cataloged"));
        let archive = DiskArchive::open(self.catalog_dir()).expect("archive");
        let copy = archive
            .copy_path(&entry.content)
            .expect("copy listed")
            .expect("copy kept");
        std::fs::read(copy).expect("copy read")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn key() -> ScreenKey {
    ScreenKey::new(ModelId("turing-8.8"))
}

fn remote(text: &str) -> RemotePath {
    RemotePath::parse(text).expect("path")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Asserts the command succeeded and returns its stdout.
fn ok(out: &Output) -> String {
    assert!(out.status.success(), "{}", text(&out.stderr));
    text(&out.stdout)
}

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// A JPEG as a phone writes it held sideways: the sensor's picture is wide
/// (left half red, right half blue) and its APP1 Exif segment says
/// orientation 6 (turn 90° clockwise to see it: tall, red on top). The
/// segment is built here: big-endian TIFF, one IFD entry, the orientation
/// tag (0x0112, SHORT).
fn phone_photo(width: u32, height: u32) -> Vec<u8> {
    let sensor = RgbImage::from_fn(width, height, |x, _| {
        image::Rgb(if x < width / 2 { RED } else { BLUE })
    });
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 95)
        .write_image(sensor.as_raw(), width, height, ExtendedColorType::Rgb8)
        .expect("JPEG encoded");
    let mut tiff = b"MM\0\x2A".to_vec();
    tiff.extend_from_slice(&8u32.to_be_bytes()); // IFD0 right after
    tiff.extend_from_slice(&1u16.to_be_bytes()); // one entry
    tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // orientation
    tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_be_bytes()); // one value
    tiff.extend_from_slice(&6u16.to_be_bytes()); // 6: turn 90° clockwise
    tiff.extend_from_slice(&[0, 0]); // the value's padding
    tiff.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let length = u16::try_from(app1.len() + 2).expect("short segment");
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "SOI");
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// Writes `bytes` as `name` in the home's photo folder.
fn photo_file(home: &Home, name: &str, bytes: &[u8]) -> String {
    let file = home.root.join("photos").join(name);
    std::fs::write(&file, bytes).expect("photo written");
    file.to_string_lossy().into_owned()
}

/// The album PNG `png` (panel-native) as the user sees the 8.8" standing in
/// `orientation`.
fn seen(png: &[u8], orientation: Orientation) -> Frame {
    assert_eq!(
        image::guess_format(png).expect("format"),
        image::ImageFormat::Png
    );
    let native = photo::decode(png).expect("PNG decoded");
    assert_eq!(native.size(), Size::new(480, 1920), "panel-native size");
    let model = model_by_id(ModelId("turing-8.8")).expect("8.8");
    let turns = orientation.quarter_turns_to(model.native_orientation);
    native.rotated((4 - turns) % 4)
}

/// Whether `pixel` is close to `color` (JPEG is lossy).
fn near(pixel: Option<Rgba>, color: [u8; 3]) -> bool {
    let pixel = pixel.expect("inside");
    let close = |a: u8, b: u8| a.abs_diff(b) < 48;
    close(pixel.r, color[0]) && close(pixel.g, color[1]) && close(pixel.b, color[2])
}

#[test]
fn set_without_yes_sends_and_records_nothing_and_says_so() {
    let home = Home::new("no-yes");
    for args in [
        &["standby", "set", "off", "--sleep", "3"][..],
        &["standby", "set", "album"][..],
        &["standby", "set", "keep"][..],
        &[
            "standby",
            "set",
            "video",
            "--file",
            "sd/video/bezel_loop.mp4",
        ][..],
    ] {
        let out = home.bezel(args);
        assert!(!out.status.success(), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let err = text(&out.stderr);
        assert!(err.starts_with("When the computer shuts down: "), "{err}");
        assert!(err.contains("  plan B      "), "{err}");
        assert!(
            err.contains(
                "Bezel Studio carries out the choice when the computer shuts down, while it \
                 runs; this command only records it and stores the plan B on the screen."
            ),
            "{err}"
        );
        assert!(
            err.ends_with(
                "Nothing was sent to the screen and nothing was recorded. Add --yes to make it \
                 the choice.\nbezel: changing what the screen does when the computer shuts down \
                 needs --yes\n"
            ),
            "{err}"
        );
    }
    // Not even the catalog's folder was made.
    assert!(!home.data().join("bezel").exists());
}

#[test]
fn set_with_yes_stores_the_plan_b_and_records_the_choice_the_studio_reads() {
    let home = Home::new("yes");
    // Another writer (the studio, `bezel storage boot`) recorded a boot
    // video: off keeps its start mode next to the timer, and it stays.
    let mut archive = DiskArchive::open(home.catalog_dir()).expect("archive");
    let mut catalog = archive.load().expect("catalog");
    let boot = BootMedia::File(remote("internal/video/bezel_demo.mp4"));
    catalog.screen_mut(&key()).set_boot(&boot);
    archive.save(&catalog).expect("saved");

    let out = ok(&home.bezel(&["standby", "set", "off", "--sleep", "3", "--yes"]));
    let dir = home.catalog_dir();
    assert_eq!(
        out,
        format!(
            "Turing Smart Screen 8.8\": turns off when the computer shuts down\n  plan B stored \
             on the screen: start mode 2, sleep timer 3 min\n  recorded in Bezel's catalog \
             ({}), which Bezel Studio reads to carry out the choice when the computer shuts \
             down\n",
            dir.display()
        )
    );
    assert_eq!(
        home.recorded(),
        Standby::Off(SleepMinutes::new(3).expect("3 min"))
    );
    let catalog = home.catalog();
    let record = catalog.screen(&key()).expect("record");
    assert_eq!(record.boot_media(), boot, "the boot media stays");

    let out = ok(&home.bezel(&[
        "standby",
        "set",
        "video",
        "--file",
        "sd/video/bezel_loop.mp4",
        "--yes",
    ]));
    assert!(out.contains("start mode 2, sleep timer off"), "{out}");
    assert_eq!(
        home.recorded(),
        Standby::Video(remote("sd/video/bezel_loop.mp4"))
    );
    let out = ok(&home.bezel(&["standby", "set", "album", "--brightness", "50", "--yes"]));
    assert!(out.contains("start mode 1, sleep timer off"), "{out}");
    assert_eq!(home.recorded(), Standby::Album);

    // keep undoes the plan B: the boot video's start mode, no timer.
    let out = ok(&home.bezel(&["standby", "set", "keep", "--yes"]));
    assert!(
        out.starts_with(
            "Turing Smart Screen 8.8\": left as it is when the computer shuts down\n  plan B \
             stored on the screen: start mode 2, sleep timer off\n"
        ),
        "{out}"
    );
    assert_eq!(home.recorded(), Standby::Keep);
}

#[test]
fn show_reads_the_choice_from_the_catalog_on_disk() {
    let home = Home::new("show");
    let out = ok(&home.bezel(&["standby", "show"]));
    assert_eq!(
        out,
        "Turing Smart Screen 8.8\" at /dev/ttyACM1\n  when the computer shuts down: leave the \
         screen as it is (nothing is sent)\n  plan B:                       start mode 0, sleep \
         timer off\n  choices:\n    keep   leave it as it is\n    off    turn it off\n    \
         video  loop a stored video (15 on the screen)\n    album  the photo album of the card \
         (sd/image)\n\nBezel Studio carries out the choice when the computer shuts down, while \
         it runs; `bezel standby set` only records it and stores the plan B on the screen.\n"
    );

    // A choice another writer recorded (the studio) is the one shown.
    let mut archive = DiskArchive::open(home.catalog_dir()).expect("archive");
    let mut catalog = archive.load().expect("catalog");
    catalog.screen_mut(&key()).standby = Standby::Album;
    archive.save(&catalog).expect("saved");
    let out = ok(&home.bezel(&["standby", "show", "-s", "/dev/ttyACM0"]));
    assert!(
        out.contains(
            "  when the computer shuts down: restart the screen into the photo album of its \
             card (sd/image)\n  plan B:                       start mode 1, sleep timer off\n"
        ),
        "{out}"
    );
    let out = home.bezel(&["standby", "show", "-s", "/dev/nope"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("/dev/nope"));
}

#[test]
fn set_refuses_what_the_screen_cannot_honour_and_records_nothing() {
    let home = Home::new("refused");
    let out = home.bezel(&[
        "standby",
        "set",
        "video",
        "--file",
        "internal/video/gone.mp4",
        "--yes",
    ]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).ends_with(
            "bezel: internal/video/gone.mp4 is not stored on the screen; nothing was sent or \
             recorded\n"
        ),
        "{}",
        text(&out.stderr)
    );
    for (args, why) in [
        (
            &["standby", "set", "keep", "--sleep", "3", "--yes"][..],
            "bezel: the sleep timer goes only with the choice off\n",
        ),
        (
            &[
                "standby",
                "set",
                "album",
                "--file",
                "sd/video/x.mp4",
                "--yes",
            ][..],
            "bezel: a file goes only with the choice video\n",
        ),
    ] {
        let out = home.bezel(args);
        assert!(!out.status.success(), "{args:?}");
        assert_eq!(text(&out.stderr), why);
    }
    let out = home.bezel(&["standby", "set", "off", "--sleep", "11", "--yes"]);
    assert!(!out.status.success(), "1 to 10 minutes");
    assert_eq!(home.recorded(), Standby::Keep);
    assert_eq!(home.catalog(), Catalog::default());
}

#[test]
fn album_add_stands_a_phone_photo_up_on_a_vertical_screen() {
    let home = Home::new("album-vertical");
    let file = photo_file(&home, "IMG 0042.JPG", &phone_photo(200, 100));
    let out = home.bezel(&[
        "standby",
        "album",
        "add",
        &file,
        "--orientation",
        "vertical",
        "--fit",
        "contain",
    ]);
    let stdout = ok(&out);
    assert!(
        stdout.starts_with(
            "Turing Smart Screen 8.8\": added sd/image/img_0042.png to the photo album ("
        ),
        "{stdout}"
    );
    let log = text(&out.stderr);
    assert!(log.contains("(100x200) to the photo album of"), "{log}");
    assert!(
        log.contains("  framed   vertical (--orientation), fitted: the whole photo"),
        "{log}"
    );
    assert!(
        log.contains("  stored   as a 480x1920 PNG turned for the panel"),
        "{log}"
    );

    // The local copy is the PNG the album shows: standing up, red on top,
    // the whole photo (480x960) between black bands.
    let view = seen(
        &home.local_copy("sd/image/img_0042.png"),
        Orientation::Portrait,
    );
    assert_eq!(view.size(), Size::new(480, 1920));
    assert_eq!(view.pixel(240, 100), Some(Rgba::BLACK));
    assert_eq!(view.pixel(240, 1820), Some(Rgba::BLACK));
    assert!(near(view.pixel(240, 600), RED));
    assert!(near(view.pixel(240, 1300), BLUE));
}

#[test]
fn album_add_frames_a_phone_photo_for_a_horizontal_screen_by_default() {
    let home = Home::new("album-horizontal");
    let file = photo_file(&home, "IMG 0042.JPG", &phone_photo(200, 100));
    let out = home.bezel(&["standby", "album", "add", &file, "--name", "Sunset"]);
    let stdout = ok(&out);
    assert!(
        stdout.contains("added sd/image/sunset.png to the photo album"),
        "{stdout}"
    );
    let log = text(&out.stderr);
    assert!(
        log.contains("  framed   horizontal (the model's; --orientation chooses), filled"),
        "{log}"
    );

    // Filled across the 1920x480 screen: the upright photo's middle band,
    // its red top half above its blue bottom half.
    let view = seen(
        &home.local_copy("sd/image/sunset.png"),
        Orientation::Landscape,
    );
    assert_eq!(view.size(), Size::new(1920, 480));
    assert!(near(view.pixel(960, 100), RED));
    assert!(near(view.pixel(960, 380), BLUE));
    assert!(near(view.pixel(10, 100), RED), "no black band: filled");
    let out = home.bezel(&["standby", "album", "add", "/no/such/photo.jpg"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).starts_with("bezel: cannot read /no/such/photo.jpg"),
        "{}",
        text(&out.stderr)
    );
}
