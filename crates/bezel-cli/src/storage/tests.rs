use super::doubles::{StubMedia, picture, video, weact_bus};
use super::*;
use crate::messages::tests::Closing;
use crate::{Cli, Command};
use bezel_core::domain::archive::{ArchiveEntry, Catalog, EntryState, ScreenKey};
use bezel_core::domain::device::ModelId;
use bezel_core::domain::frame::Rect;
use bezel_core::domain::geometry::Size;
use bezel_core::domain::media::MediaFormat;
use bezel_core::domain::storage::StartMode;
use bezel_devices::fake::{FakeStorage, Playback, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_media::archive::MemoryArchive;
use clap::Parser;
use std::collections::BTreeMap;

/// The time the tests record as "sent now".
const NOW: u64 = 1_790_500_000;

fn path(text: &str) -> RemotePath {
    RemotePath::parse(text).unwrap()
}

fn with_files(files: &[(&str, usize)]) -> FakeConnector {
    let mut storage = FakeStorage::default();
    for (p, bytes) in files {
        storage = storage.with_file(path(p), vec![7; *bytes]);
    }
    FakeConnector::with_storage(storage)
}

/// Runs `bezel storage …` on the simulated 8.8": stdout (or the error)
/// and what went to stderr.
fn storage_with(
    args: &[&str],
    connector: &FakeConnector,
    media: &mut StubMedia,
    log: &mut dyn Write,
) -> anyhow::Result<String> {
    let cli = Cli::try_parse_from(args)?;
    let Command::Storage(storage) = &cli.command else {
        anyhow::bail!("not a storage command")
    };
    let cancel = CancelToken::new();
    let mut archive = MemoryArchive::new();
    let mut kit = StorageKit {
        media,
        cancel: &cancel,
        progress: ProgressStyle::Lines,
        log,
        archive: &mut archive,
        archive_dir: None,
        theme_videos: &[],
        now: NOW,
    };
    run(storage, &FakeBus::turing_88(), connector, &mut kit)
}

fn storage(
    args: &[&str],
    connector: &FakeConnector,
    media: &mut StubMedia,
) -> (anyhow::Result<String>, String) {
    let mut log = Vec::new();
    let out = storage_with(args, connector, media, &mut log);
    (out, String::from_utf8(log).unwrap())
}

#[test]
fn rm_without_yes_is_refused() {
    let connector = with_files(&[("internal/video/intro.mp4", 3000)]);
    let (out, log) = storage(
        &["bezel", "storage", "rm", "internal/video/intro.mp4"],
        &connector,
        &mut StubMedia::ready(),
    );
    let err = out.unwrap_err().to_string();
    assert!(err.contains("needs --yes"), "{err}");
    assert!(
        log.contains("Delete internal/video/intro.mp4 from the screen's internal flash"),
        "{log}"
    );
    assert!(log.contains("Nothing was sent to the screen"), "{log}");
    let screen = connector.log().storage;
    assert!(screen.calls.is_empty(), "the screen was not even opened");
    assert!(screen.files.contains_key(&path("internal/video/intro.mp4")));
}

#[test]
fn rm_with_yes_summarizes_then_deletes() {
    let connector = with_files(&[
        ("internal/video/intro.mp4", 3000),
        ("internal/image/a.png", 10),
    ]);
    let (out, log) = storage(
        &[
            "bezel",
            "storage",
            "rm",
            "internal/video/intro.mp4",
            "internal/video/gone.mp4",
            "--yes",
        ],
        &connector,
        &mut StubMedia::ready(),
    );
    assert_eq!(out.unwrap(), "deleted internal/video/intro.mp4 (2.9 KiB)\n");
    assert!(
        log.contains(
            "Delete internal/video/intro.mp4 (2.9 KiB) from Turing Smart Screen 8.8\" \
             (internal flash)"
        ),
        "{log}"
    );
    assert!(
        log.contains("internal/video/gone.mp4 is not stored on Turing Smart Screen 8.8\""),
        "{log}"
    );
    let screen = connector.log().storage;
    assert_eq!(screen.files.len(), 1);
    let changes: Vec<_> = screen
        .calls
        .iter()
        .filter(|c| c.changes_the_screen())
        .collect();
    assert_eq!(
        changes,
        [&StorageCall::Delete(path("internal/video/intro.mp4"))]
    );

    let (out, _) = storage(
        &["bezel", "storage", "rm", "internal/image/nope.png", "--yes"],
        &connector,
        &mut StubMedia::ready(),
    );
    assert_eq!(out.unwrap(), "nothing deleted\n");
    // A card file without a card: refused, nothing deleted.
    let (out, _) = storage(
        &[
            "bezel",
            "storage",
            "rm",
            "sd/image/a.png",
            "internal/image/a.png",
            "--yes",
        ],
        &connector,
        &mut StubMedia::ready(),
    );
    assert!(out.unwrap_err().to_string().contains("no memory card"));
    assert_eq!(connector.log().storage.files.len(), 1);
}

#[test]
fn info_and_ls_as_text_and_json() {
    let storage_with_card = FakeStorage::default()
        .with_card(8 << 30)
        .with_file(path("internal/video/intro.mp4"), vec![1; 3 << 20])
        .with_file(path("sd/image/logo.png"), vec![1; 512]);
    let connector = FakeConnector::with_storage(storage_with_card);
    let mut media = StubMedia::ready();
    let (out, _) = storage(&["bezel", "storage", "info"], &connector, &mut media);
    let out = out.unwrap();
    assert!(out.starts_with("Turing Smart Screen 8.8\"\n"), "{out}");
    assert!(out.contains("internal  ["), "{out}");
    assert!(out.contains("3.0 MiB used of 1.0 GiB"), "{out}");
    assert!(out.contains("512 B used of 8.0 GiB"), "{out}");

    let (out, _) = storage(
        &["bezel", "storage", "info", "--json"],
        &connector,
        &mut media,
    );
    let json: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
    assert_eq!(json["internal"]["usedBytes"], 3 << 20);
    assert_eq!(json["card"]["totalBytes"], 8_u64 << 30);

    let (out, _) = storage(&["bezel", "storage", "ls"], &connector, &mut media);
    let out = out.unwrap();
    assert!(
        out.contains("internal/video/intro.mp4     3.0 MiB"),
        "{out}"
    );
    assert!(out.contains("sd/image/logo.png"), "{out}");
    assert!(out.ends_with("2 files, 3.0 MiB\n"), "{out}");

    let (out, _) = storage(
        &["bezel", "storage", "ls", "sd/image", "--json"],
        &connector,
        &mut media,
    );
    let json: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
    assert_eq!(json[0]["path"], "sd/image/logo.png");
    assert_eq!(json[0]["medium"], "sd");
    assert_eq!(json[0]["folder"], "image");
    assert_eq!(json[0]["sizeBytes"], 512);

    // Without a card: only the internal folders, and the card says so.
    let bare = FakeConnector::default();
    let (out, _) = storage(&["bezel", "storage", "ls"], &bare, &mut media);
    assert_eq!(
        out.unwrap(),
        "No files in internal/image, internal/video (no memory card).\n"
    );
    let (out, _) = storage(&["bezel", "storage", "info"], &bare, &mut media);
    assert!(out.unwrap().contains("sd        no memory card"));
    let (out, _) = storage(&["bezel", "storage", "ls", "sd/video"], &bare, &mut media);
    assert!(out.unwrap_err().to_string().contains("no memory card"));
    assert!(Cli::try_parse_from(["bezel", "storage", "ls", "internal"]).is_err());
}

#[test]
fn put_sends_a_picture_with_progress_and_a_summary() {
    let connector = FakeConnector::default();
    let mut media =
        StubMedia::ready().with("/pics/My Logo.PNG", picture(MediaFormat::Png, 200_000));
    let (out, log) = storage(
        &["bezel", "storage", "put", "/pics/My Logo.PNG"],
        &connector,
        &mut media,
    );
    assert_eq!(
        out.unwrap(),
        "Turing Smart Screen 8.8\": stored internal/image/my_logo.png (195.3 KiB)\n"
    );
    assert!(
        log.starts_with("Upload /pics/My Logo.PNG (195.3 KiB, PNG 64x64)\n"),
        "{log}"
    );
    assert!(
        log.contains(
            "  to       internal/image/my_logo.png on Turing Smart Screen 8.8\" (internal flash)"
        ),
        "{log}"
    );
    assert!(
        log.contains("upload  [------------------------]   0%  0 B / 195.3 KiB"),
        "{log}"
    );
    assert!(
        log.contains("upload  [########################] 100%"),
        "{log}"
    );
    assert!(
        log.contains("verify  [########################] 100%  stored size checked"),
        "{log}"
    );
    assert!(!log.contains("ffmpeg"), "pictures need no converter: {log}");
    let stored = connector.log().storage;
    assert_eq!(
        stored.size(&path("internal/image/my_logo.png")),
        Some(200_000)
    );
}

#[test]
fn put_over_a_stored_file_needs_yes() {
    let stored = FakeStorage::default()
        .with_card(1 << 30)
        .with_file(path("sd/image/logo.png"), vec![7; 100]);
    let connector = FakeConnector::with_storage(stored);
    let mut media = StubMedia::ready().with("logo.png", picture(MediaFormat::Png, 5000));
    let mut args = vec!["bezel", "storage", "put", "logo.png", "sd/image/logo.png"];
    let (out, log) = storage(&args, &connector, &mut media);
    let err = out.unwrap_err().to_string();
    assert!(
        err.contains("replacing sd/image/logo.png needs --yes"),
        "{err}"
    );
    assert!(
        log.contains("  replaces sd/image/logo.png (100 B)"),
        "{log}"
    );
    assert!(log.contains("Add --yes to replace"), "{log}");
    let screen = connector.log().storage;
    assert!(
        !screen.calls.iter().any(StorageCall::changes_the_screen),
        "{:?}",
        screen.calls
    );

    args.push("--yes");
    let (out, _) = storage(&args, &connector, &mut media);
    assert!(out.unwrap().contains("stored sd/image/logo.png (4.9 KiB)"));
    assert_eq!(
        connector.log().storage.size(&path("sd/image/logo.png")),
        Some(5000)
    );
}

#[test]
fn put_converts_a_video_of_another_shape_by_cropping_it() {
    let connector = FakeConnector::default();
    let clip = video(Size::new(1920, 1080), 9_000_000, true);
    let mut media = StubMedia::ready().with("clip.mov", clip);
    let (out, log) = storage(
        &[
            "bezel",
            "storage",
            "put",
            "clip.mov",
            "internal/video",
            "--fps",
            "24",
        ],
        &connector,
        &mut media,
    );
    assert!(
        out.unwrap()
            .contains("stored internal/video/clip.mp4 (293.0 KiB, converted)")
    );
    // Landscape on a reverse-portrait panel: one quarter turn, then the
    // middle of the 1080x1920 picture with the panel's 1:4 shape.
    let target = media.targets[0];
    assert_eq!(target.size, Size::new(480, 1920));
    assert_eq!(target.quarter_turns, 1);
    assert_eq!(
        target.crop,
        Some(Rect {
            x: 300,
            y: 0,
            width: 480,
            height: 1920
        })
    );
    assert_eq!(target.frame_rate, Some(24));
    assert!(log.contains("  convert  to 480x1920 MP4 (H.264, no audio) with ffmpeg, turned 90°, keeping the middle 1920x480 of the clip"), "{log}");
    assert!(
        log.contains("convert [########################] 100%  10.0 s of 10.0 s of video"),
        "{log}"
    );
    assert!(!log.contains("warning"), "{log}");

    // A vertical screen: half a turn, and a clip already of the panel's
    // size is sent as it is unless an orientation is asked for.
    let native = video(Size::new(480, 1920), 4000, false);
    let mut media = StubMedia::ready().with("tall.mp4", native);
    let (out, log) = storage(
        &["bezel", "storage", "put", "tall.mp4"],
        &connector,
        &mut media,
    );
    assert!(
        out.unwrap()
            .contains("stored internal/video/tall.mp4 (3.9 KiB)\n")
    );
    assert!(
        log.contains("  as is    already in the screen's format"),
        "{log}"
    );
    assert!(media.targets.is_empty());
    let (out, _) = storage(
        &[
            "bezel",
            "storage",
            "put",
            "tall.mp4",
            "internal/video/tall_180.mp4",
            "--orientation",
            "vertical",
        ],
        &connector,
        &mut media,
    );
    out.unwrap();
    assert_eq!(media.targets[0].quarter_turns, 2);
    assert_eq!(media.targets[0].crop, None);
}

#[test]
fn files_over_the_screens_limit_are_refused_in_mib() {
    // D-2026-09-30-release-polish-12: 25 MiB per file on the 8.8".
    let cap = bezel_core::domain::storage::REV_C_MAX_UPLOAD_BYTES;
    let connector = FakeConnector::default();
    let mut media = StubMedia::ready()
        .with("big.mp4", video(Size::new(480, 1920), cap + 1, false))
        .with("trip.mov", video(Size::new(1920, 1080), 9_000_000, true));
    let (out, _) = storage(
        &["bezel", "storage", "put", "big.mp4"],
        &connector,
        &mut media,
    );
    assert_eq!(
        out.unwrap_err().to_string(),
        "refused: the file is 25.1 MiB and this screen takes files up to 25 MiB each; \
         nothing was sent. For a video: send a shorter clip, or a lower frame rate with \
         --fps (for example --fps 24)"
    );
    media.output_bytes = 30 * 1024 * 1024;
    let (out, _) = storage(
        &["bezel", "storage", "put", "trip.mov"],
        &connector,
        &mut media,
    );
    let err = out.unwrap_err().to_string();
    assert!(
        err.starts_with(
            "refused: converted, the video is 30 MiB and this screen takes files up to \
             25 MiB each; nothing was sent."
        ),
        "{err}"
    );
    assert!(err.contains("--fps"), "{err}");
    assert_eq!(
        media.targets[0].max_bytes,
        Some(cap),
        "the conversion is capped"
    );
    let log = connector.log().storage;
    assert!(log.files.is_empty());
    assert!(!log.calls.iter().any(StorageCall::changes_the_screen));
}

#[test]
fn without_ffmpeg_only_videos_in_the_screen_format_go() {
    let connector = FakeConnector::default();
    let mut media = StubMedia::missing()
        .with("ready.mp4", video(Size::new(480, 1920), 4000, false))
        .with("raw.mp4", video(Size::new(1920, 1080), 4000, true));
    let (out, log) = storage(
        &["bezel", "storage", "put", "ready.mp4"],
        &connector,
        &mut media,
    );
    out.unwrap();
    assert!(log.contains("warning: ffmpeg was not found"), "{log}");
    assert!(log.contains("sudo dnf install ffmpeg"), "{log}");

    let (out, _) = storage(
        &["bezel", "storage", "put", "raw.mp4"],
        &connector,
        &mut media,
    );
    let err = out.unwrap_err().to_string();
    assert!(err.contains("must be converted"), "{err}");
    assert!(err.contains("--ffmpeg PATH"), "{err}");
    let stored = connector.log().storage;
    assert_eq!(stored.files.len(), 1, "only the ready video");
}

#[test]
fn a_full_screen_lists_what_could_be_deleted() {
    let mut full = FakeStorage::default()
        .with_file(path("internal/video/big.mp4"), vec![1; 6000])
        .with_file(path("internal/image/small.png"), vec![1; 1000]);
    full.internal_total = 8000;
    let connector = FakeConnector::with_storage(full);
    let mut media = StubMedia::ready().with("new.png", picture(MediaFormat::Png, 2000));
    let (out, _) = storage(
        &["bezel", "storage", "put", "new.png"],
        &connector,
        &mut media,
    );
    let err = out.unwrap_err().to_string();
    assert!(
        err.contains("2.0 KiB does not fit in the 1000 B free"),
        "{err}"
    );
    let big = err.find("internal/video/big.mp4  5.9 KiB").unwrap();
    let small = err.find("internal/image/small.png  1000 B").unwrap();
    assert!(big < small, "largest first: {err}");
    assert!(err.contains("bezel storage rm <PATH> --yes"), "{err}");
    assert_eq!(connector.log().storage.files.len(), 2, "nothing deleted");

    let mut media = StubMedia::ready().with("huge.png", picture(MediaFormat::Png, 130_000_000));
    let (out, _) = storage(
        &["bezel", "storage", "put", "huge.png"],
        &connector,
        &mut media,
    );
    assert!(
        out.unwrap_err()
            .to_string()
            .contains("the file is 124 MiB and this screen takes files up to 25 MiB each")
    );
    let mut media = StubMedia::ready().with(
        "notes.txt",
        MediaInfo {
            format: MediaFormat::Other,
            bytes: 10,
            dimensions: None,
            video: None,
            has_audio: false,
        },
    );
    let (out, _) = storage(
        &["bezel", "storage", "put", "notes.txt"],
        &connector,
        &mut media,
    );
    assert!(out.unwrap_err().to_string().contains("is not a picture"));
    let (out, _) = storage(
        &["bezel", "storage", "put", "missing.png"],
        &connector,
        &mut media,
    );
    assert!(format!("{:#}", out.unwrap_err()).contains("cannot read missing.png"));
}

/// Stderr that cancels the upload once a line shows `when` (the first
/// 64 KiB of a 256 KiB upload by default), what Ctrl+C does from the
/// handler's thread.
struct CancelOnWrite {
    token: CancelToken,
    seen: Vec<u8>,
    when: &'static str,
}

impl Write for CancelOnWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.seen.extend_from_slice(buf);
        if String::from_utf8_lossy(&self.seen).contains(self.when) {
            self.token.cancel();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_cancelled_upload_says_how_to_delete_what_is_left() {
    let connector = FakeConnector::default();
    let mut media = StubMedia::ready().with("big.png", picture(MediaFormat::Png, 256 * 1024));
    let cancel = CancelToken::new();
    let mut log = CancelOnWrite {
        token: cancel.clone(),
        seen: Vec::new(),
        when: "64.0 KiB / 256.0 KiB",
    };
    let cli = Cli::try_parse_from(["bezel", "storage", "put", "big.png"]).unwrap();
    let Command::Storage(args) = &cli.command else {
        unreachable!("parsed as storage")
    };
    let mut archive = MemoryArchive::new();
    let mut kit = StorageKit {
        media: &mut media,
        cancel: &cancel,
        progress: ProgressStyle::Bar,
        log: &mut log,
        archive: &mut archive,
        archive_dir: None,
        theme_videos: &[],
        now: NOW,
    };
    let err = run(args, &FakeBus::turing_88(), &connector, &mut kit)
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "cancelled; an incomplete file of 64.0 KiB remains at internal/image/big.png: \
         delete it with `bezel storage rm internal/image/big.png --yes`"
    );
    let drawn = String::from_utf8(log.seen).unwrap();
    assert!(
        drawn.contains("\rupload  ["),
        "a bar redrawn in place: {drawn:?}"
    );
    assert!(drawn.ends_with('\n'), "the bar line is ended: {drawn:?}");
    assert_eq!(
        cancelled(&path("internal/image/x.png"), None).to_string(),
        "cancelled; nothing was stored"
    );
}

#[test]
fn play_stop_and_boot() {
    let connector = with_files(&[
        ("internal/video/intro.mp4", 3000),
        ("internal/image/logo.png", 30),
    ]);
    let mut media = StubMedia::ready();
    let (out, _) = storage(
        &["bezel", "storage", "play", "internal/video/intro.mp4"],
        &connector,
        &mut media,
    );
    assert!(out.unwrap().contains("looping internal/video/intro.mp4"));
    let (out, _) = storage(
        &[
            "bezel",
            "storage",
            "play",
            "internal/image/logo.png",
            "--once",
        ],
        &connector,
        &mut media,
    );
    assert!(out.unwrap().contains("showing internal/image/logo.png"));
    let (out, _) = storage(
        &[
            "bezel",
            "storage",
            "play",
            "internal/video/none.mp4",
            "--once",
        ],
        &connector,
        &mut media,
    );
    assert!(out.unwrap_err().to_string().contains("not stored"));
    let (out, _) = storage(&["bezel", "storage", "stop"], &connector, &mut media);
    assert_eq!(out.unwrap(), "Turing Smart Screen 8.8\": stopped\n");
    assert_eq!(connector.log().storage.playback, Playback::Idle);

    let calls_before = connector.log().storage.calls.len();
    let (out, log) = storage(
        &["bezel", "storage", "boot", "internal/video/intro.mp4"],
        &connector,
        &mut media,
    );
    assert!(out.unwrap_err().to_string().contains("needs --yes"));
    assert!(
        log.contains("Boot media: internal/video/intro.mp4 (video, looping)"),
        "{log}"
    );
    assert!(log.contains("the vendor default, about 67%"), "{log}");
    assert!(
        log.contains("with its sleep timer off: it does not go to sleep on its own"),
        "{log}"
    );
    assert_eq!(
        connector.log().storage.calls.len(),
        calls_before,
        "the screen was not opened"
    );

    let (out, log) = storage(
        &[
            "bezel",
            "storage",
            "boot",
            "internal/video/intro.mp4",
            "--brightness",
            "40",
            "--yes",
        ],
        &connector,
        &mut media,
    );
    assert_eq!(
        out.unwrap(),
        "Turing Smart Screen 8.8\": boots with internal/video/intro.mp4\n"
    );
    assert!(log.contains("boots with: 40% (--brightness)"), "{log}");
    let screen = connector.log();
    assert_eq!(screen.brightness, [Brightness::new(40).unwrap()]);
    assert_eq!(screen.storage.start_mode, Some(StartMode::Video));
    assert_eq!(
        screen.storage.playback,
        Playback::Video(path("internal/video/intro.mp4"), Repeat::Loop)
    );

    let (out, log) = storage(
        &["bezel", "storage", "boot", "default", "--yes"],
        &connector,
        &mut media,
    );
    assert!(
        out.unwrap()
            .ends_with("boots with its built-in start screen\n")
    );
    assert!(
        log.starts_with("Boot media: the screen's built-in start screen"),
        "{log}"
    );
    assert_eq!(connector.log().storage.start_mode, Some(StartMode::Default));
}

#[test]
fn what_a_screen_cannot_do_is_said_plainly() {
    let connector = FakeConnector::default();
    let mut link = open_screen(&weact_bus(), &connector, None).unwrap();
    let err = info(link.as_mut(), false).unwrap_err().to_string();
    assert_eq!(
        err,
        "this screen does not support reading its storage \
         (WeAct Studio Display FS 0.96\" has no storage)"
    );
    let err = play(link.as_mut(), &path("internal/video/a.mp4"), Repeat::Once)
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("this screen does not support playing a video once"),
        "{err}"
    );
    let unsupported = BezelError::Unsupported("no size query for this file".into());
    assert_eq!(
        screen_error(DELETING)(unsupported).to_string(),
        "this screen does not support deleting files (no size query for this file)"
    );
    let timeout = BezelError::Timeout("the screen; reconnect it".into());
    assert_eq!(
        screen_error(UPLOADING)(timeout).to_string(),
        "timeout talking to the screen; reconnect it",
        "other errors are printed as they are"
    );
}

#[test]
fn destinations_and_paths_parse() {
    assert_eq!(
        parse_destination("sd"),
        Ok(Destination::Medium(Medium::Card))
    );
    assert_eq!(
        parse_destination("internal/video/"),
        Ok(Destination::Folder(StorageLocation::new(
            Medium::Internal,
            MediaKind::Video
        )))
    );
    assert_eq!(
        parse_destination("internal/image/Logo.png"),
        Ok(Destination::File(
            StorageLocation::new(Medium::Internal, MediaKind::Image),
            "Logo.png".into()
        ))
    );
    assert!(parse_destination("usb").is_err());
    assert!(parse_destination("sd/music").is_err());
    assert!(parse_path("internal/video").is_err());
    assert_eq!(parse_boot("default"), Ok(BootMedia::Default));
    assert!(
        Cli::try_parse_from(["bezel", "storage", "rm"]).is_err(),
        "a path is required"
    );
    let cli =
        Cli::try_parse_from(["bezel", "storage", "put", "a.mp4", "--ffmpeg", "/opt/ff"]).unwrap();
    let Command::Storage(args) = &cli.command else {
        unreachable!("parsed as storage")
    };
    assert_eq!(args.ffmpeg(), Some(Path::new("/opt/ff")));
    assert!(args.cancellable());
    let cli = Cli::try_parse_from(["bezel", "storage", "stop"]).unwrap();
    let Command::Storage(args) = &cli.command else {
        unreachable!("parsed as storage")
    };
    assert_eq!(args.ffmpeg(), None);
    assert!(!args.cancellable());
}

#[test]
fn sizes_and_progress_read_well() {
    assert_eq!(size_text(0), "0 B");
    assert_eq!(size_text(1023), "1023 B");
    assert_eq!(size_text(1536), "1.5 KiB");
    assert_eq!(size_text(120_000_000), "114.4 MiB");
    assert_eq!(size_text(1 << 30), "1.0 GiB");
    assert_eq!(
        progress_line(Progress::new(JobPhase::Convert, 2500, 0)),
        "convert 2.5 s of video converted"
    );
    assert_eq!(
        progress_line(Progress::new(JobPhase::Verify, 0, 1)),
        "verify  [------------------------]   0%  checking the stored size"
    );
    let mut out = Vec::new();
    let mut log = Messages::new(&mut out);
    let mut view = ProgressView::new(ProgressStyle::Lines, &mut log);
    for done in [0, 10, 50, 90, 95, 100] {
        view.report(Progress::new(JobPhase::Upload, done, 100));
    }
    view.finish();
    log.check().unwrap();
    let lines = String::from_utf8(out).unwrap();
    assert_eq!(
        lines.lines().count(),
        5,
        "0%, 10%, 50%, 90% and 100%: {lines}"
    );
}

#[test]
fn a_summary_that_cannot_be_written_stops_before_the_screen_changes() {
    let clip = "internal/video/intro.mp4";
    let connector = with_files(&[(clip, 3000)]);
    let mut media = StubMedia::ready().with("new.png", picture(MediaFormat::Png, 100));
    let stderr_error = "could not write to the terminal (stderr): broken pipe";
    for args in [
        vec!["bezel", "storage", "rm", clip],
        vec!["bezel", "storage", "rm", clip, "--yes"],
        vec!["bezel", "storage", "boot", clip],
        vec!["bezel", "storage", "boot", clip, "--yes"],
        vec!["bezel", "storage", "put", "new.png"],
    ] {
        let mut closed = Closing::after(0);
        let out = storage_with(&args, &connector, &mut media, &mut closed);
        let err = format!("{:#}", out.unwrap_err());
        assert_eq!(err, stderr_error, "{args:?}");
    }
    let screen = connector.log().storage;
    let changed: Vec<&StorageCall> = screen
        .calls
        .iter()
        .filter(|c| c.changes_the_screen())
        .collect();
    assert!(changed.is_empty(), "{changed:?}");
    assert!(screen.files.contains_key(&path(clip)));
}

#[test]
fn a_progress_line_that_cannot_be_written_ends_the_finished_upload_with_its_error() {
    let connector = FakeConnector::default();
    let mut media = StubMedia::ready().with("new.png", picture(MediaFormat::Png, 100));
    // Room for the summary, not for the progress lines.
    let mut closing = Closing::after(200);
    let out = storage_with(
        &["bezel", "storage", "put", "new.png"],
        &connector,
        &mut media,
        &mut closing,
    );
    let err = format!("{:#}", out.unwrap_err());
    assert_eq!(
        err,
        "Turing Smart Screen 8.8\" stored internal/image/new.png: \
         could not write to the terminal (stderr): broken pipe"
    );
    let taken = String::from_utf8(closing.taken).unwrap();
    assert!(taken.starts_with("Upload new.png"), "{taken}");
    let stored = path("internal/image/new.png");
    assert_eq!(connector.log().storage.size(&stored), Some(100));
}

// ---------------------------------------------------------------- the manager

/// The catalog key of the simulated 8.8".
fn key() -> ScreenKey {
    ScreenKey::new(ModelId("turing-8.8"))
}

/// The `--fake` screen without the vendor's card files: only what Bezel
/// sent (and the partial of its interrupted upload).
fn light_storage() -> FakeStorage {
    let mut storage = demo::storage();
    storage
        .files
        .retain(|p, _| p.name.as_str().starts_with("bezel_"));
    storage
}

/// A screen, Bezel's catalog and the local files, kept between commands.
struct Session {
    bus: FakeBus,
    connector: FakeConnector,
    archive: MemoryArchive,
    media: StubMedia,
    videos: Vec<AssetRef>,
    archive_dir: Option<PathBuf>,
}

impl Session {
    fn on(bus: FakeBus, storage: FakeStorage, archive: MemoryArchive) -> Self {
        Self {
            bus,
            connector: FakeConnector::with_storage(storage),
            archive,
            media: StubMedia::ready(),
            videos: Vec::new(),
            archive_dir: None,
        }
    }

    /// The `--fake` screen: what Bezel sent and the user's vendor card.
    fn demo() -> Self {
        Self::on(FakeBus::turing_88(), demo::storage(), demo::archive())
    }

    /// [`light_storage`] with the demo catalog.
    fn light() -> Self {
        Self::on(FakeBus::turing_88(), light_storage(), demo::archive())
    }

    /// Runs `bezel storage <args>`, its stderr going to `log`.
    fn run_into(
        &mut self,
        args: &[&str],
        log: &mut dyn Write,
        cancel: &CancelToken,
    ) -> anyhow::Result<String> {
        let cli = Cli::try_parse_from(["bezel", "storage"].iter().chain(args))?;
        let Command::Storage(storage) = &cli.command else {
            anyhow::bail!("not a storage command")
        };
        let mut kit = StorageKit {
            media: &mut self.media,
            cancel,
            progress: ProgressStyle::Lines,
            log,
            archive: &mut self.archive,
            archive_dir: self.archive_dir.as_deref(),
            theme_videos: &self.videos,
            now: NOW,
        };
        run(storage, &self.bus, &self.connector, &mut kit)
    }

    /// Runs `bezel storage <args>`: stdout (or the error) and stderr.
    fn run(&mut self, args: &[&str]) -> (anyhow::Result<String>, String) {
        let mut log = Vec::new();
        let out = self.run_into(args, &mut log, &CancelToken::new());
        (out, String::from_utf8(log).unwrap())
    }

    /// What the screen stores, with sizes.
    fn files(&self) -> BTreeMap<String, usize> {
        let stored = self.connector.log().storage.files;
        stored
            .iter()
            .map(|(p, data)| (p.to_string(), data.len()))
            .collect()
    }

    /// The storage calls that changed the screen.
    fn writes(&self) -> Vec<StorageCall> {
        let calls = self.connector.log().storage.calls;
        calls
            .into_iter()
            .filter(StorageCall::changes_the_screen)
            .collect()
    }

    fn catalog(&self) -> Catalog {
        self.archive.saved().cloned().unwrap_or_default()
    }

    /// The entry at `at` of the 8.8"'s record, deleted or not.
    fn entry(&self, at: &str) -> Option<ArchiveEntry> {
        let catalog = self.catalog();
        let record = catalog.screen(&key())?;
        record
            .entries
            .iter()
            .find(|e| e.path.to_string() == at)
            .cloned()
    }

    fn state(&self, at: &str) -> Option<EntryState> {
        self.entry(at).map(|e| e.state)
    }
}

#[test]
fn mv_rename_restore_and_cleanup_without_yes_change_nothing() {
    let mut s = Session::demo();
    let before = s.files();
    let copies = s.archive.copies();
    let cases: [(&[&str], &str, &str); 4] = [
        (
            &[
                "mv",
                "internal/video/bezel_demo.mp4",
                "internal/image/bezel_demo.png",
                "--to",
                "sd",
            ],
            "  internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4     2.3 MiB\n  \
             internal/image/bezel_demo.png -> sd/image/bezel_demo.png    48.0 KiB\n2 files, \
             2.4 MiB to send, one at a time; each source is deleted only after its copy is \
             verified.\n",
            "moving 2 files needs --yes",
        ),
        (
            &["rename", "internal/video/bezel_demo.mp4", "Intro.mp4"],
            "  internal/video/bezel_demo.mp4 -> internal/video/intro.mp4     2.3 MiB\n",
            "renaming 1 file needs --yes",
        ),
        (
            &["restore", "sd"],
            "  sd/video/bezel_intro.mp4 -> sd/video/bezel_intro.mp4     1.2 MiB  (missing)\n\
             1 file, 1.2 MiB to send; 7.9 GiB free there; nothing is deleted.\n",
            "restoring 1 file needs --yes",
        ),
        (
            &["cleanup"],
            "Pre-checked, deleted by `bezel storage cleanup --yes`:\n  \
             internal/video/bezel_cut.mp4      320.0 KiB  pending:",
            "deleting 1 file needs --yes",
        ),
    ];
    for (args, listed, refusal) in cases {
        let (out, log) = s.run(args);
        assert_eq!(out.unwrap_err().to_string(), refusal, "{args:?}");
        assert!(log.contains(listed), "{args:?}: {log}");
        assert!(
            log.ends_with(&format!(
                "Nothing on the screen was changed. Add --yes to {}\n",
                match args[0] {
                    "mv" => "move them.",
                    "rename" => "rename it.",
                    "restore" => "restore it.",
                    _ => "delete what is pre-checked (1 file).",
                }
            )),
            "{log}"
        );
        assert!(s.writes().is_empty(), "{args:?}: {:?}", s.writes());
    }
    assert_eq!(s.files(), before, "the screen is as it was");
    // The catalog only followed the listings: the same entries and copies.
    assert_eq!(s.archive.copies(), copies);
    assert_eq!(
        s.state("sd/video/bezel_intro.mp4"),
        Some(EntryState::Missing)
    );
    assert_eq!(
        s.state("internal/video/bezel_cut.mp4"),
        Some(EntryState::Pending)
    );
    assert_eq!(s.catalog().screen(&key()).map(|r| r.entries.len()), Some(5));
}

#[test]
fn cleanup_dry_run_lists_without_deleting() {
    let mut s = Session::demo();
    let before = s.files();
    let (out, log) = s.run(&["cleanup", "--dry-run"]);
    let out = out.unwrap();
    assert!(log.is_empty(), "the list is the output: {log}");
    assert!(
        out.starts_with(
            "Cleanup suggestions for Turing Smart Screen 8.8\" (never the boot media Bezel \
             set nor a video your themes play):\nPre-checked, deleted by `bezel storage \
             cleanup --yes`:\n  internal/video/bezel_cut.mp4      320.0 KiB  pending: an \
             upload by Bezel that did not finish or failed its size check\nOnly listed"
        ),
        "only the interrupted upload is pre-checked: {out}"
    );
    // The vendor's five groups: each artifact is a variant of the group's
    // shortest name, which stays; `8.8APEX_2.mp4` is no artifact.
    for (artifact, kept) in [
        ("demon_open.mp4.mp4.mp4", "demon_open.mp4.mp4"),
        ("demon.mp401115025.mp4", "demon.mp4.mp4.mp4"),
        ("NVI.mp427034822.mp4", "NVI.mp4"),
        ("Rani.mp417075004.mp4", "Rani.mp4"),
        ("m04.mp424045157.mp4", "m04.mp4"),
    ] {
        let line = out
            .lines()
            .find(|l| l.starts_with(&format!("  sd/video/{artifact} ")))
            .unwrap();
        assert!(
            line.ends_with(&format!(
                "variant: a vendor copy of sd/video/{kept} with another size"
            )),
            "{line}"
        );
    }
    let apex = out.lines().find(|l| l.contains("8.8APEX_2.mp4")).unwrap();
    assert_eq!(
        apex,
        "  sd/video/8.8APEX_2.mp4              2.2 MiB  unused: no theme plays it"
    );
    assert!(!out.contains("bezel_demo") && !out.contains("bezel_loop"));
    assert!(
        out.ends_with(
            "1 file pre-checked (320.0 KiB to free), 12 files only listed.\nDry run: nothing \
             was deleted.\n"
        ),
        "{out}"
    );
    assert!(s.writes().is_empty());
    assert_eq!(s.files(), before);

    // Never with --yes either: the two do not go together.
    let (out, _) = s.run(&["cleanup", "--dry-run", "--yes"]);
    let err = out.unwrap_err().to_string();
    assert!(err.contains("cannot be used with"), "{err}");
    assert!(s.writes().is_empty());
}

/// The uploads, the size checks of `targets` and the deletes, in order.
fn trace(connector: &FakeConnector, targets: &[&str]) -> Vec<String> {
    let calls = connector.log().storage.calls;
    let traced = calls.iter().filter_map(|call| match call {
        StorageCall::Upload(p, _) => Some(format!("upload {p}")),
        StorageCall::Size(p) if targets.contains(&p.to_string().as_str()) => {
            Some(format!("size {p}"))
        }
        StorageCall::Delete(p) => Some(format!("delete {p}")),
        _ => None,
    });
    traced.collect()
}

#[test]
fn mv_sends_each_copy_and_deletes_its_source_only_after_checking_it() {
    let mut s = Session::light();
    let (out, log) = s.run(&[
        "mv",
        "internal/video/bezel_demo.mp4",
        "internal/image/bezel_demo.png",
        "--to",
        "sd",
        "--yes",
    ]);
    assert_eq!(
        out.unwrap(),
        "moved internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4 (2.3 MiB)\nmoved \
         internal/image/bezel_demo.png -> sd/image/bezel_demo.png (48.0 KiB)\n"
    );
    assert!(
        log.contains("[1/2] verify  [########################] 100%  stored size checked\n"),
        "{log}"
    );
    assert!(log.contains("[2/2] upload  ["), "{log}");
    let targets = ["sd/video/bezel_demo.mp4", "sd/image/bezel_demo.png"];
    let traced = trace(&s.connector, &targets);
    let wanted = [
        "upload sd/video/bezel_demo.mp4",
        "size sd/video/bezel_demo.mp4",
        "delete internal/video/bezel_demo.mp4",
        "upload sd/image/bezel_demo.png",
        "size sd/image/bezel_demo.png",
        "delete internal/image/bezel_demo.png",
    ];
    let mut at = traced.iter();
    for step in wanted {
        assert!(at.any(|t| t == step), "{step} in order: {traced:?}");
    }
    let files = s.files();
    assert_eq!(files.get("sd/video/bezel_demo.mp4"), Some(&(2_400 * 1024)));
    assert_eq!(files.get("sd/image/bezel_demo.png"), Some(&(48 * 1024)));
    assert!(!files.contains_key("internal/video/bezel_demo.mp4"));
    // The catalog follows: the files live on the card now, same copies.
    let moved = s.entry("sd/video/bezel_demo.mp4").unwrap();
    assert_eq!(
        (moved.state, moved.card, moved.sent_at),
        (EntryState::Stored, Some(demo::CARD_BYTES), NOW)
    );
    assert_eq!(s.entry("internal/video/bezel_demo.mp4"), None);
    assert!(s.archive.holds(&moved.content));
    assert_eq!(s.archive.copies(), 5);
}

#[test]
fn mv_skips_a_taken_name_unless_overwrite() {
    let storage = light_storage().with_file(path("sd/video/bezel_demo.mp4"), vec![1; 100]);
    let mut s = Session::on(FakeBus::turing_88(), storage, demo::archive());
    let args = ["mv", "internal/video/bezel_demo.mp4", "--to", "sd"];
    let (out, log) = s.run(&args);
    assert_eq!(out.unwrap(), "nothing to move\n");
    assert_eq!(
        log,
        "Skipped:\n  internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4: \
         sd/video/bezel_demo.mp4 is there (100 B); --overwrite replaces it\n"
    );
    let (out, log) = s.run(&[&args[..], &["--overwrite"]].concat());
    assert_eq!(out.unwrap_err().to_string(), "moving 1 file needs --yes");
    assert!(
        log.contains(
            "  internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4     2.3 MiB  replaces \
             sd/video/bezel_demo.mp4 (100 B)\n"
        ),
        "{log}"
    );
    assert!(s.writes().is_empty());
    let (out, _) = s.run(&[&args[..], &["--overwrite", "--yes"]].concat());
    assert!(out.unwrap().starts_with("moved "));
    assert_eq!(
        s.files().get("sd/video/bezel_demo.mp4"),
        Some(&(2_400 * 1024))
    );

    // A file Bezel did not send has no copy to send: skipped, with the way
    // to give it one.
    let foreign = light_storage().with_file(path("sd/video/AMD.mp4"), vec![1; 4000]);
    let mut s = Session::on(FakeBus::turing_88(), foreign, demo::archive());
    let (out, log) = s.run(&["mv", "sd/video/AMD.mp4", "--to", "internal", "--yes"]);
    assert_eq!(out.unwrap(), "nothing to move\n");
    assert!(
        log.contains("sd/video/AMD.mp4 -> internal/video/amd.mp4: Bezel has no local copy of it"),
        "{log}"
    );
    assert!(log.contains("`bezel storage catalog associate`"), "{log}");
    assert!(s.writes().is_empty());
    let (out, _) = s.run(&["mv", "sd/video/AMD.mp4", "--to", "sd"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "refused: sd/video/AMD.mp4 is already on that medium; nothing was sent or deleted"
    );
}

#[test]
fn a_cancelled_move_keeps_the_source_and_says_what_is_left() {
    let mut s = Session::light();
    let cancel = CancelToken::new();
    let mut log = CancelOnWrite {
        token: cancel.clone(),
        seen: Vec::new(),
        when: "[1/2] upload  [###",
    };
    let args = [
        "mv",
        "internal/video/bezel_demo.mp4",
        "internal/image/bezel_demo.png",
        "--to",
        "sd",
        "--yes",
    ];
    let err = s.run_into(&args, &mut log, &cancel).unwrap_err();
    assert_eq!(
        err.to_string(),
        "stopped at internal/video/bezel_demo.mp4 -> sd/video/bezel_demo.mp4 (while sending): \
         cancelled\n  internal/video/bezel_demo.mp4 stays where it was\n  an incomplete file \
         of 256.0 KiB remains at sd/video/bezel_demo.mp4: delete it with `bezel storage rm \
         sd/video/bezel_demo.mp4 --yes`\n  not started: internal/image/bezel_demo.png"
    );
    let files = s.files();
    assert_eq!(
        files.get("internal/video/bezel_demo.mp4"),
        Some(&(2_400 * 1024))
    );
    assert_eq!(files.get("sd/video/bezel_demo.mp4"), Some(&(256 * 1024)));
    assert!(files.contains_key("internal/image/bezel_demo.png"));
    assert!(
        !s.writes()
            .iter()
            .any(|w| matches!(w, StorageCall::Delete(_)))
    );
    // The partial is a leftover the cleanup assistant pre-checks.
    let (out, _) = s.run(&["cleanup", "--dry-run"]);
    let out = out.unwrap();
    let listed = out.find("Only listed").unwrap_or(out.len());
    assert!(
        out[..listed].contains("sd/video/bezel_demo.mp4"),
        "pre-checked: {out}"
    );

    // Cancelled once the copy is verified: both are there, and the
    // source's delete is offered.
    let mut s = Session::light();
    let cancel = CancelToken::new();
    let mut log = CancelOnWrite {
        token: cancel.clone(),
        seen: Vec::new(),
        when: "[1/1] verify  [########################] 100%",
    };
    let args = ["mv", "internal/image/bezel_demo.png", "--to", "sd", "--yes"];
    let err = s.run_into(&args, &mut log, &cancel).unwrap_err();
    assert_eq!(
        err.to_string(),
        "stopped at internal/image/bezel_demo.png -> sd/image/bezel_demo.png (while deleting \
         the source, its copy verified): cancelled\n  its copy at sd/image/bezel_demo.png is \
         verified and internal/image/bezel_demo.png is still there too: delete it with `bezel \
         storage rm internal/image/bezel_demo.png --yes`"
    );
    let files = s.files();
    assert!(files.contains_key("internal/image/bezel_demo.png"));
    assert!(files.contains_key("sd/image/bezel_demo.png"));
}

#[test]
fn a_copy_stored_short_stops_the_batch_and_names_the_leftover() {
    let mut storage = light_storage();
    storage.short_by = 10;
    let mut s = Session::on(FakeBus::turing_88(), storage, demo::archive());
    let (out, _) = s.run(&[
        "mv",
        "internal/image/bezel_demo.png",
        "internal/video/bezel_demo.mp4",
        "--to",
        "sd",
        "--yes",
    ]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "stopped at internal/image/bezel_demo.png -> sd/image/bezel_demo.png (while checking \
         the stored size): sd/image/bezel_demo.png was stored with 49142 bytes, not the \
         file's 49152: the stored size differs; delete it and send it again\n  \
         internal/image/bezel_demo.png stays where it was\n  an incomplete file of 48.0 KiB \
         remains at sd/image/bezel_demo.png: delete it with `bezel storage rm \
         sd/image/bezel_demo.png --yes`\n  not started: internal/video/bezel_demo.mp4"
    );
    assert!(s.files().contains_key("internal/image/bezel_demo.png"));

    // A batch that stops after some files names them too.
    let mut s = Session::light();
    let cancel = CancelToken::new();
    let mut log = CancelOnWrite {
        token: cancel.clone(),
        seen: Vec::new(),
        when: "[2/2] upload  [---",
    };
    let args = [
        "mv",
        "internal/image/bezel_demo.png",
        "internal/video/bezel_demo.mp4",
        "--to",
        "sd",
        "--yes",
    ];
    let err = s
        .run_into(&args, &mut log, &cancel)
        .unwrap_err()
        .to_string();
    assert!(
        err.ends_with(
            "\n  done before it:\n    moved internal/image/bezel_demo.png -> \
             sd/image/bezel_demo.png (48.0 KiB)"
        ),
        "{err}"
    );
}

#[test]
fn rename_and_restore_run_from_the_local_copies() {
    let mut s = Session::light();
    let (out, _) = s.run(&[
        "rename",
        "internal/video/bezel_demo.mp4",
        "Intro.mp4",
        "--yes",
    ]);
    assert_eq!(
        out.unwrap(),
        "renamed internal/video/bezel_demo.mp4 -> internal/video/intro.mp4 (2.3 MiB)\n"
    );
    let files = s.files();
    assert_eq!(files.get("internal/video/intro.mp4"), Some(&(2_400 * 1024)));
    assert!(!files.contains_key("internal/video/bezel_demo.mp4"));
    let (out, _) = s.run(&["rename", "internal/video/intro.mp4", "intro.mkv"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "refused: the new name must end in .mp4; nothing was sent or deleted"
    );

    // The missing card file comes back; then nothing is missing.
    let (out, log) = s.run(&["restore", "sd", "--yes"]);
    assert_eq!(
        out.unwrap(),
        "restored sd/video/bezel_intro.mp4 -> sd/video/bezel_intro.mp4 (1.2 MiB)\n"
    );
    assert!(log.contains("[1/1] verify"), "{log}");
    assert_eq!(
        s.files().get("sd/video/bezel_intro.mp4"),
        Some(&(1_200 * 1024))
    );
    let (out, _) = s.run(&["restore", "sd"]);
    assert_eq!(
        out.unwrap(),
        "nothing to restore: no file Bezel sent to the memory card is missing from it\n"
    );

    // A file deleted through Bezel is restored by name.
    let (out, _) = s.run(&["rm", "sd/video/bezel_loop.mp4", "--yes"]);
    out.unwrap();
    assert_eq!(
        s.state("sd/video/bezel_loop.mp4"),
        Some(EntryState::Deleted)
    );
    let (out, log) = s.run(&["restore", "sd", "BEZEL_LOOP.mp4", "--yes"]);
    assert_eq!(
        out.unwrap(),
        "restored sd/video/bezel_loop.mp4 -> sd/video/bezel_loop.mp4 (750.0 KiB)\n"
    );
    assert!(log.contains("750.0 KiB  (deleted)\n"), "{log}");
    assert_eq!(s.state("sd/video/bezel_loop.mp4"), Some(EntryState::Stored));
    // Already there with that size: skipped as present.
    let (out, log) = s.run(&["restore", "sd", "sd/video/bezel_loop.mp4"]);
    assert_eq!(out.unwrap(), "nothing to restore\n");
    assert!(log.contains(": already there with the same size"), "{log}");
    let (out, _) = s.run(&["restore", "sd", "nope.mp4"]);
    assert!(
        out.unwrap_err()
            .to_string()
            .starts_with("nope.mp4 is not in Bezel's catalog of this screen")
    );
}

#[test]
fn a_restore_that_does_not_fit_is_refused_before_anything_is_sent() {
    let mut storage = light_storage();
    storage.card_total = Some(1_000_000);
    let mut s = Session::on(FakeBus::turing_88(), storage, demo::archive());
    let (out, _) = s.run(&["restore", "sd", "--yes"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "refused: the files need 1.2 MiB and 226.6 KiB is free (973.4 KiB short); nothing \
         was sent or deleted"
    );
    assert!(s.writes().is_empty());
    let mut bare = light_storage();
    bare.card_total = None;
    bare.files
        .retain(|p, _| p.location.medium == Medium::Internal);
    let mut s = Session::on(FakeBus::turing_88(), bare, demo::archive());
    let (out, _) = s.run(&["restore", "sd", "--yes"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "refused: the screen has no memory card; nothing was sent or deleted"
    );
}

#[test]
fn cleanup_with_yes_deletes_only_the_pre_checked_files() {
    let mut s = Session::demo();
    let before = s.files();
    let (out, log) = s.run(&["cleanup", "--yes"]);
    assert_eq!(
        out.unwrap(),
        "deleted internal/video/bezel_cut.mp4 (320.0 KiB)\nfreed 320.0 KiB\n"
    );
    assert!(log.contains("Only listed"), "the whole list first: {log}");
    assert_eq!(
        s.writes(),
        [StorageCall::Delete(path("internal/video/bezel_cut.mp4"))]
    );
    let after = s.files();
    assert_eq!(after.len(), before.len() - 1);
    assert_eq!(
        s.state("internal/video/bezel_cut.mp4"),
        Some(EntryState::Deleted)
    );
    let (out, _) = s.run(&["cleanup"]);
    assert_eq!(out.unwrap(), "nothing pre-checked; nothing deleted\n");

    // A screen with nothing to suggest says so.
    let mut s = Session::on(
        FakeBus::turing_88(),
        FakeStorage::default(),
        MemoryArchive::new(),
    );
    let (out, _) = s.run(&["cleanup", "--dry-run"]);
    assert_eq!(
        out.unwrap(),
        "No cleanup suggestions for Turing Smart Screen 8.8\".\nDry run: nothing was \
         deleted.\n"
    );

    // Cancelled before the first delete: nothing goes, and it says so.
    let mut s = Session::light();
    let cancel = CancelToken::new();
    cancel.cancel();
    let err = s
        .run_into(&["cleanup", "--yes"], &mut Vec::new(), &cancel)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "stopped at internal/video/bezel_cut.mp4: cancelled; it was not deleted"
    );
    assert!(s.writes().is_empty());
}

#[test]
fn theme_videos_and_the_boot_media_are_protected() {
    let mut s = Session::demo();
    s.videos = vec![
        AssetRef("assets/AMD.mp4".into()),
        AssetRef("assets/bezel_demo.mp4".into()),
    ];
    let (out, _) = s.run(&["boot", "sd/video/Rani.mp4", "--yes"]);
    out.unwrap();
    assert_eq!(
        s.catalog().screen(&key()).and_then(|r| r.boot.clone()),
        Some(path("sd/video/Rani.mp4"))
    );
    let (out, _) = s.run(&["cleanup", "--dry-run"]);
    let out = out.unwrap();
    let listed = |name: &str| out.lines().any(|l| l.starts_with(&format!("  {name} ")));
    assert!(!listed("sd/video/AMD.mp4"), "a theme's video: {out}");
    assert!(!listed("sd/video/Rani.mp4"), "the boot media: {out}");
    assert!(listed("sd/video/NVI.mp4"), "{out}");
    assert!(
        out.contains("variant: a vendor copy of sd/video/Rani.mp4"),
        "{out}"
    );

    // Moving the boot media and renaming a theme's video warn.
    let (out, _) = s.run(&["boot", "internal/video/bezel_demo.mp4", "--yes"]);
    out.unwrap();
    let (_, log) = s.run(&["mv", "internal/video/bezel_demo.mp4", "--to", "sd"]);
    assert!(
        log.contains(
            "Warning: internal/video/bezel_demo.mp4 is the boot media Bezel set; the screen \
             boots the last file played, so set it again afterwards with `bezel storage boot`\n"
        ),
        "{log}"
    );
    let (_, log) = s.run(&["rename", "internal/video/bezel_demo.mp4", "demo.mp4"]);
    assert!(
        log.contains(
            "Warning: a theme plays internal/video/bezel_demo.mp4 by its name; after the \
             rename the theme no longer finds it\n"
        ),
        "{log}"
    );
    assert!(
        !s.writes()
            .iter()
            .any(|w| matches!(w, StorageCall::Upload(..) | StorageCall::Delete(_)))
    );
}

/// A Turing USB 8.8" holding a file Bezel sent (its size known only to
/// the catalog) and one it did not.
fn turing_usb_session() -> Session {
    let clip = path("internal/video/clip.mp4");
    let storage = FakeStorage::default()
        .with_card(demo::CARD_BYTES)
        .with_file_of_unknown_size(clip.clone(), vec![3; 2048])
        .with_file_of_unknown_size(path("internal/video/vendor.mp4"), vec![4; 4096]);
    let mut archive = MemoryArchive::new();
    let content = archive.keep(&[3; 2048]).unwrap();
    let mut entry = ArchiveEntry::pending(clip, None, 2048, content.clone(), 1);
    entry.state = EntryState::Stored;
    let mut catalog = Catalog::default();
    catalog.copies.insert(content);
    catalog
        .screen_mut(&ScreenKey::new(ModelId("turing-usb-8.8")))
        .record(entry);
    archive.save(&catalog).unwrap();
    Session::on(super::doubles::turing_usb_bus(), storage, archive)
}

#[test]
fn turing_usb_screens_refuse_what_ends_in_a_delete() {
    let mut s = turing_usb_session();
    let (out, _) = s.run(&["ls", "internal/video"]);
    assert_eq!(
        out.unwrap(),
        "internal/video/clip.mp4       2.0 KiB  stored   local copy\n\
         internal/video/vendor.mp4           ?  -\n\
         2 files, 2.0 KiB; 1 sent by Bezel\n"
    );
    let (out, log) = s.run(&["mv", "internal/video/clip.mp4", "--to", "sd", "--yes"]);
    assert_eq!(out.unwrap(), "nothing to move\n");
    assert!(
        log.contains("this screen cannot delete files through Bezel, so nothing can be moved"),
        "{log}"
    );
    let (out, _) = s.run(&["cleanup", "--dry-run"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "this screen does not support the cleanup assistant (Turing 8.8\" V1.x (USB) cannot \
         delete files through Bezel)"
    );
    let (out, _) = s.run(&["rm", "internal/video/clip.mp4", "--yes"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "this screen does not support deleting files (Turing 8.8\" V1.x (USB) cannot delete \
         files through Bezel)"
    );
    assert!(s.writes().is_empty());
}

#[test]
fn put_rm_and_boot_are_recorded_in_the_catalog() {
    let mut s = Session::light();
    s.media = StubMedia::ready().with("/pics/Logo.png", picture(MediaFormat::Png, 200_000));
    let (out, _) = s.run(&["put", "/pics/Logo.png"]);
    assert!(out.unwrap().contains("stored internal/image/logo.png"));
    let entry = s.entry("internal/image/logo.png").unwrap();
    assert_eq!(
        (
            entry.state,
            entry.size,
            entry.sent_at,
            entry.source.as_deref()
        ),
        (EntryState::Stored, 200_000, NOW, Some("/pics/Logo.png"))
    );
    assert_eq!(
        s.archive.read(&entry.content).unwrap(),
        Some(vec![0x42; 200_000]),
        "the exact bytes sent"
    );
    let (out, _) = s.run(&["ls", "internal/image"]);
    assert_eq!(
        out.unwrap(),
        "internal/image/bezel_demo.png    48.0 KiB  stored   local copy\n\
         internal/image/logo.png         195.3 KiB  stored   local copy\n\
         2 files, 243.3 KiB; 2 sent by Bezel\n"
    );

    let (out, _) = s.run(&["rm", "internal/image/logo.png", "--yes"]);
    assert_eq!(
        out.unwrap(),
        "deleted internal/image/logo.png (195.3 KiB)\n"
    );
    assert_eq!(
        s.state("internal/image/logo.png"),
        Some(EntryState::Deleted)
    );
    assert!(s.archive.holds(&entry.content), "kept for a restore");

    let (out, _) = s.run(&["boot", "internal/video/bezel_demo.mp4", "--yes"]);
    out.unwrap();
    let (out, _) = s.run(&["catalog"]);
    let out = out.unwrap();
    assert!(
        out.ends_with("Boot media set by Bezel: internal/video/bezel_demo.mp4\n"),
        "{out}"
    );
    let (out, _) = s.run(&["boot", "default", "--yes"]);
    out.unwrap();
    assert_eq!(
        s.catalog().screen(&key()).and_then(|r| r.boot.clone()),
        None
    );
}

#[test]
fn ls_shows_the_catalog_state_local_copies_and_missing_files() {
    let storage = light_storage().with_file(path("sd/video/AMD.mp4"), vec![1; 4000]);
    let mut s = Session::on(FakeBus::turing_88(), storage, demo::archive());
    let (out, _) = s.run(&["ls"]);
    assert_eq!(
        out.unwrap(),
        "internal/image/bezel_demo.png    48.0 KiB  stored   local copy\n\
         internal/video/bezel_cut.mp4    320.0 KiB  pending  local copy\n\
         internal/video/bezel_demo.mp4     2.3 MiB  stored   local copy\n\
         sd/video/AMD.mp4                  3.9 KiB  -\n\
         sd/video/bezel_loop.mp4         750.0 KiB  stored   local copy\n\
         5 files, 3.4 MiB; 4 sent by Bezel\n\
         Missing from the screen, sent by Bezel: sd/video/bezel_intro.mp4 (`bezel storage \
         restore` sends them again)\n"
    );
    let (out, _) = s.run(&["ls", "sd/video", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&out.unwrap()).unwrap();
    assert_eq!(json[0]["name"], "AMD.mp4");
    assert_eq!(json[0]["state"], serde_json::Value::Null);
    assert_eq!(json[0]["localCopy"], false);
    assert_eq!(json[1]["state"], "stored");
    assert_eq!(json[1]["localCopy"], true);
    assert_eq!(json[1]["sizeBytes"], 750 * 1024);

    // Copies cleared: still Bezel's, without a local copy.
    let (out, _) = s.run(&["cache", "clear", "--all", "--yes"]);
    out.unwrap();
    let (out, _) = s.run(&["ls", "internal/image"]);
    assert_eq!(
        out.unwrap(),
        "internal/image/bezel_demo.png    48.0 KiB  stored   no local copy\n\
         1 file, 48.0 KiB; 1 sent by Bezel\n"
    );
}

/// A file on disk for `catalog associate`, registered with the stub media
/// as a native 8.8" video of `bytes`.
fn original(dir: &Path, name: &str, bytes: u64, media: StubMedia) -> (String, StubMedia) {
    let file = dir.join(name);
    std::fs::write(&file, b"stub").unwrap();
    let text = file.to_string_lossy().into_owned();
    let media = media.with(&text, video(Size::new(480, 1920), bytes, false));
    (text, media)
}

#[test]
fn catalog_lists_associates_and_forgets() {
    let dir = std::env::temp_dir().join(format!("bezel-cli-associate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let storage = light_storage().with_file(path("sd/video/NVI.mp4"), vec![1; 4000]);
    let mut s = Session::on(FakeBus::turing_88(), storage, demo::archive());
    let (nvi, media) = original(&dir, "nvi.mp4", 4000, StubMedia::ready());
    let (other, media) = original(&dir, "other.mp4", 4000, media);
    let (_, media) = original(&dir, "short.mp4", 3999, media);
    s.media = media;
    let folder = dir.to_string_lossy().into_owned();

    let (out, _) = s.run(&["catalog"]);
    let out = out.unwrap();
    assert!(
        out.starts_with(
            "Bezel's catalog of Turing Smart Screen 8.8\" (turing-8.8), oldest first:\n  \
             internal/image/bezel_demo.png    48.0 KiB  stored      local copy     "
        ),
        "{out}"
    );
    assert!(
        out.contains("  sd/video/bezel_intro.mp4          1.2 MiB  missing     local copy"),
        "{out}"
    );
    assert!(
        out.ends_with("5 files, 5 with a local copy (`bezel storage cache` shows their use)\n"),
        "{out}"
    );

    let (out, log) = s.run(&["catalog", "associate", "sd/video/NVI.mp4", &folder]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "associating sd/video/NVI.mp4 needs --yes"
    );
    assert!(
        log.starts_with(&format!(
            "Associate sd/video/NVI.mp4 (3.9 KiB) on Turing Smart Screen 8.8\" with {nvi}:\n"
        )),
        "{log}"
    );
    assert!(
        log.contains(&format!(
            "  another candidate: {other} (pass it as FILE to choose it)\n"
        )),
        "{log}"
    );
    assert!(!log.contains("short.mp4"), "another size: {log}");
    assert_eq!(s.entry("sd/video/NVI.mp4"), None);

    let (out, _) = s.run(&["catalog", "associate", "sd/video/NVI.mp4", &folder, "--yes"]);
    assert_eq!(
        out.unwrap(),
        format!("associated sd/video/NVI.mp4 with {nvi} (3.9 KiB)\n")
    );
    let entry = s.entry("sd/video/NVI.mp4").unwrap();
    assert_eq!(entry.state, EntryState::Stored);
    assert_eq!(entry.source.as_deref(), Some(nvi.as_str()));
    // Now it can move: under its upload name.
    let (out, _) = s.run(&["mv", "sd/video/NVI.mp4", "--to", "internal", "--yes"]);
    assert_eq!(
        out.unwrap(),
        "moved sd/video/NVI.mp4 -> internal/video/nvi.mp4 (3.9 KiB)\n"
    );

    let (out, _) = s.run(&["catalog", "associate", "sd/video/NVI.mp4", &nvi]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "sd/video/NVI.mp4 is not stored on Turing Smart Screen 8.8\""
    );
    let missing = dir.join("missing.mp4");
    let (out, _) = s.run(&[
        "catalog",
        "associate",
        "internal/video/nvi.mp4",
        &missing.to_string_lossy(),
    ]);
    assert_eq!(
        out.unwrap_err().to_string(),
        format!("no file or folder at {}", missing.display())
    );
    let short = dir.join("short.mp4").to_string_lossy().into_owned();
    let (out, _) = s.run(&["catalog", "associate", "internal/video/nvi.mp4", &short]);
    assert_eq!(
        out.unwrap_err().to_string(),
        format!(
            "no video of exactly 3.9 KiB (4000 bytes) at {short}, the size of \
             internal/video/nvi.mp4; nothing was associated"
        )
    );

    let (out, log) = s.run(&["catalog", "forget", "internal/video/nvi.mp4"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "forgetting internal/video/nvi.mp4 needs --yes"
    );
    assert!(
        log.starts_with(
            "Forget internal/video/nvi.mp4 (3.9 KiB, stored) in Bezel's catalog of Turing \
             Smart Screen 8.8\"; its local copy (3.9 KiB) goes too. The screen is not changed.\n"
        ),
        "{log}"
    );
    let (out, _) = s.run(&["catalog", "forget", "internal/video/nvi.mp4", "--yes"]);
    assert_eq!(out.unwrap(), "forgot internal/video/nvi.mp4\n");
    assert_eq!(s.entry("internal/video/nvi.mp4"), None);
    assert!(!s.archive.holds(&entry.content));
    assert!(
        s.files().contains_key("internal/video/nvi.mp4"),
        "untouched"
    );
    let (out, _) = s.run(&["catalog", "forget", "internal/video/nvi.mp4"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "internal/video/nvi.mp4 is not in Bezel's catalog of Turing Smart Screen 8.8\""
    );

    let mut empty = Session::on(
        FakeBus::turing_88(),
        FakeStorage::default(),
        MemoryArchive::new(),
    );
    let (out, _) = empty.run(&["catalog", "ls"]);
    assert_eq!(
        out.unwrap(),
        "Bezel has not sent anything to Turing Smart Screen 8.8\" yet.\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cache_shows_clears_and_limits_the_local_copies() {
    let mut s = Session::light();
    s.archive_dir = Some(PathBuf::from("store"));
    let (out, _) = s.run(&["rm", "internal/video/bezel_demo.mp4", "--yes"]);
    out.unwrap();
    let (out, _) = s.run(&["cache"]);
    assert_eq!(
        out.unwrap(),
        "Bezel's local copies, in store:\n  5 copies, 6.1 MiB\n  of files deleted through \
         Bezel: 1 copy, 2.3 MiB of the 2.0 GiB limit (above it the oldest go)\n"
    );
    s.archive_dir = None;
    let (out, _) = s.run(&["cache", "info"]);
    assert!(
        out.unwrap()
            .starts_with("Bezel's local copies, in memory (--fake):\n")
    );

    let (out, log) = s.run(&["cache", "clear"]);
    assert_eq!(
        out.unwrap_err().to_string(),
        "clearing the local copies needs --yes"
    );
    assert_eq!(
        log,
        "Clear the local copies of files deleted through Bezel: 1 copy, 2.3 MiB. Their \
         catalog entries and thumbnails stay, without a local copy: they cannot be moved, \
         renamed or restored until associated again.\nNothing was removed. Add --yes to clear \
         them.\n"
    );
    let (out, _) = s.run(&["cache", "clear", "--yes"]);
    assert_eq!(out.unwrap(), "cleared 1 copy (2.3 MiB)\n");
    let (out, _) = s.run(&["cache", "clear", "--yes"]);
    assert_eq!(out.unwrap(), "nothing to clear\n");
    // Without its copy the deleted file cannot be restored.
    let (out, log) = s.run(&["restore", "internal", "bezel_demo.mp4", "--yes"]);
    assert_eq!(out.unwrap(), "nothing to restore\n");
    assert!(log.contains("Bezel has no local copy of it"), "{log}");

    // A lower limit drops the oldest copies of deleted files.
    let (out, _) = s.run(&["rm", "internal/image/bezel_demo.png", "--yes"]);
    out.unwrap();
    let (out, _) = s.run(&["cache", "--limit", "10KiB"]);
    assert_eq!(
        out.unwrap(),
        "limit of the local copies of deleted files: 10.0 KiB; removed the oldest 1 copy \
         (48.0 KiB)\n"
    );
    let (out, _) = s.run(&["cache", "--limit", "2GiB"]);
    assert_eq!(
        out.unwrap(),
        "limit of the local copies of deleted files: 2.0 GiB; nothing removed\n"
    );
    assert_eq!(s.catalog().limit, 2 << 30);
    let (out, _) = s.run(&["cache", "clear", "--all", "--yes"]);
    assert_eq!(out.unwrap(), "cleared 3 copies (3.7 MiB)\n");
    assert_eq!(s.archive.copies(), 0);
    assert!(
        s.writes()
            .iter()
            .all(|w| matches!(w, StorageCall::Delete(_)))
    );
    for wrong in [
        &["cache", "clear", "--limit", "1G"][..],
        &["cache", "--limit", "lots"],
    ] {
        let (out, _) = s.run(wrong);
        assert!(out.is_err(), "{wrong:?}");
    }
}

#[test]
fn sizes_parse_with_binary_and_decimal_units() {
    assert_eq!(parse_size("1048576"), Ok(1 << 20));
    assert_eq!(parse_size("2GiB"), Ok(2 << 30));
    assert_eq!(parse_size("2 G"), Ok(2 << 30));
    assert_eq!(parse_size("1.5gib"), Ok(3 << 29));
    assert_eq!(parse_size("500MiB"), Ok(500 << 20));
    assert_eq!(parse_size("10k"), Ok(10_240));
    assert_eq!(parse_size("1 TiB"), Ok(1 << 40));
    assert_eq!(parse_size("2GB"), Ok(2_000_000_000));
    assert_eq!(parse_size("3 MB"), Ok(3_000_000));
    assert_eq!(parse_size("7kb"), Ok(7_000));
    assert_eq!(parse_size("1TB"), Ok(1_000_000_000_000));
    assert_eq!(parse_size("12B"), Ok(12));
    for wrong in ["", "GiB", "2 parsecs", "1.2.3G", "99999999999999999999TiB"] {
        let err = parse_size(wrong).unwrap_err();
        assert!(
            err.contains("expected a size such as 2GiB"),
            "{wrong}: {err}"
        );
    }
}

#[test]
fn ctrl_c_and_the_catalog_follow_the_command() {
    let parsed = |args: &[&str]| {
        let cli = Cli::try_parse_from(["bezel", "storage"].iter().chain(args)).unwrap();
        let Command::Storage(storage) = cli.command else {
            unreachable!("parsed as storage")
        };
        storage
    };
    let mv = parsed(&["mv", "internal/video/a.mp4", "--to", "sd"]);
    assert!(mv.cancellable() && mv.uses_catalog());
    assert!(mv.cancel_note().unwrap().contains("its source stays"));
    assert!(
        parsed(&["put", "a.png"])
            .cancel_note()
            .unwrap()
            .contains("upload")
    );
    assert!(parsed(&["cleanup"]).cancellable());
    assert!(!parsed(&["cleanup", "--dry-run"]).cancellable());
    assert!(!parsed(&["catalog"]).cancellable());
    for args in [&["info"][..], &["stop"], &["play", "internal/video/a.mp4"]] {
        assert!(!parsed(args).uses_catalog(), "{args:?}");
    }
    for args in [
        &["ls"][..],
        &["cache"],
        &["catalog", "forget", "sd/video/a.mp4"],
    ] {
        assert!(parsed(args).uses_catalog(), "{args:?}");
    }
    let catalog = parsed(&["catalog", "-s", "/dev/ttyACM1"]);
    let StorageAction::Catalog(args) = &catalog.action else {
        unreachable!("catalog")
    };
    assert_eq!(args.target().screen.as_deref(), Some("/dev/ttyACM1"));
    let catalog = parsed(&["catalog", "ls", "-s", "/dev/ttyACM9"]);
    let StorageAction::Catalog(args) = &catalog.action else {
        unreachable!("catalog")
    };
    assert_eq!(args.target().screen.as_deref(), Some("/dev/ttyACM9"));
    let wrong: [&[&str]; 4] = [
        &["mv", "internal/video/a.mp4"],
        &["mv", "internal/video/a.mp4", "--to", "usb"],
        &["restore"],
        &["catalog", "associate", "sd/video/a.mp4"],
    ];
    for args in wrong {
        let all = ["bezel", "storage"].iter().chain(args);
        assert!(Cli::try_parse_from(all).is_err(), "{args:?}");
    }
}

#[test]
fn halts_read_as_what_to_do() {
    let entry = |p: &str, size| bezel_core::domain::storage::FileEntry {
        path: path(p),
        size,
    };
    assert_eq!(
        halt_text(
            &Halt::Conflict(entry("sd/video/a.mp4", Some(10))),
            "moving files"
        ),
        "sd/video/a.mp4 is there now (10 B) and replacing it was not confirmed (--overwrite)"
    );
    assert_eq!(
        halt_text(&Halt::NoLocalCopy, "moving files"),
        "its local copy is gone"
    );
    assert_eq!(
        halt_text(&Halt::SourceChanged, "moving files"),
        "the file is gone or changed since the list was made"
    );
    assert_eq!(
        halt_text(&Halt::Refused(Refusal::NoCard), "moving files"),
        "refused: the screen has no memory card"
    );
    assert_eq!(
        halt_text(
            &Halt::Failed(BezelError::Unsupported("no".into())),
            "moving files"
        ),
        "this screen does not support moving files (no)"
    );
    let refused = refused_plan(PlanRefusal::Unsendable {
        path: path("sd/video/big.mp4"),
        refusal: Refusal::TooLarge {
            bytes: 30 << 20,
            limit: 25 << 20,
        },
    });
    assert!(
        refused
            .to_string()
            .starts_with("sd/video/big.mp4: refused: the file is 30 MiB"),
        "{refused}"
    );
}

#[test]
fn every_cleanup_reason_reads_as_what_it_is() {
    use bezel_core::domain::cleanup::HANG_PARTIAL_BYTES;
    let hang = usize::try_from(HANG_PARTIAL_BYTES).unwrap();
    let storage = FakeStorage::default()
        .with_file(path("internal/video/clip.mp4"), vec![1; 1000])
        .with_file(path("internal/video/clip.mp4.mp4"), vec![2; 1000])
        .with_file(path("internal/video/bezel_test_cancel.mp4"), vec![3; hang])
        .with_file(path("internal/video/own.mp4"), vec![4; 500])
        .with_file(path("internal/image/a.png"), vec![5; 700])
        .with_file(path("internal/image/bb.png"), vec![6; 700]);
    let mut archive = MemoryArchive::new();
    let content = archive.keep(&[4; 600]).unwrap();
    let mut entry = ArchiveEntry::pending(path("internal/video/own.mp4"), None, 600, content, 1);
    entry.state = EntryState::Stored;
    let mut catalog = Catalog::default();
    catalog.screen_mut(&key()).record(entry);
    archive.save(&catalog).unwrap();
    let mut s = Session::on(FakeBus::turing_88(), storage, archive);
    let (out, _) = s.run(&["cleanup", "--dry-run"]);
    let out = out.unwrap();
    for line in [
        "  internal/video/bezel_test_cancel.mp4    28.2 MiB  hangPartial: exactly 29577216 \
         bytes, what an upload that hung the screen leaves",
        "  internal/video/clip.mp4.mp4               1000 B  duplicate: a vendor copy of \
         internal/video/clip.mp4, same size",
        "  internal/video/own.mp4                     500 B  sizeDiffers: Bezel stored 600 B here",
        "  internal/image/bb.png                      700 B  sameSize: exactly the size of \
         internal/image/a.png",
        "  internal/image/a.png                       700 B  unused: no theme plays it",
    ] {
        assert!(out.lines().any(|l| l == line), "{line}\nin\n{out}");
    }
    let (out, _) = s.run(&["cleanup", "--yes"]);
    assert_eq!(
        out.unwrap(),
        "deleted internal/video/bezel_test_cancel.mp4 (28.2 MiB)\ndeleted \
         internal/video/clip.mp4.mp4 (1000 B)\nfreed 28.2 MiB\n"
    );
    let left = s.files();
    assert!(left.contains_key("internal/video/clip.mp4"));
    assert!(left.contains_key("internal/video/own.mp4"));
}
