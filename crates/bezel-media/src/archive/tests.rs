//! The disk store and its thumbnails, in temporary folders. Paths are only
//! joined, never spelled, so the tests run the same on Windows.

use std::ffi::OsString;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bezel_core::domain::archive::{
    ArchiveEntry, Catalog, ContentId, DEFAULT_CACHE_LIMIT, EntryState, ScreenKey,
};
use bezel_core::domain::device::ModelId;
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::job::Job;
use bezel_core::domain::media::{MediaInfo, MediaTools, StreamSpec, TranscodeTarget};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::domain::storage::{BootMedia, RemotePath};
use bezel_core::ports::{ArchiveStore, MediaLocation, MediaTranscoder, VideoFrames};
use bezel_core::{BezelError, Result};
use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};

use super::thumbs::fit;
use super::*;
use crate::FfmpegTranscoder;
use crate::mp4::fixtures::Movie;
use crate::probe::{self, HostSystem, Lookup};

fn encoded(picture: DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let pixels = RgbaImage::from_pixel(width, height, image::Rgba([10, 120, 230, 255]));
    encoded(DynamicImage::ImageRgba8(pixels), ImageFormat::Png)
}

fn rgb(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let pixels = RgbImage::from_pixel(width, height, image::Rgb([200, 40, 90]));
    encoded(DynamicImage::ImageRgb8(pixels), format)
}

fn dimensions(png: &[u8]) -> (u32, u32) {
    assert_eq!(image::guess_format(png).unwrap(), ImageFormat::Png);
    let picture = image::load_from_memory(png).unwrap();
    (picture.width(), picture.height())
}

/// The names of the files (not folders) in `dir`, sorted.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn remote(text: &str) -> RemotePath {
    RemotePath::parse(text).unwrap()
}

/// An entry of every state, both screens' records, a boot media and every
/// optional field set.
fn sample_catalog(ids: &[ContentId]) -> Catalog {
    let mut catalog = Catalog {
        limit: 3 << 30,
        ..Catalog::default()
    };
    catalog.copies.extend(ids.iter().cloned());
    let record = catalog.screen_mut(&ScreenKey::new(ModelId("turing-8.8")));
    let mut clip = ArchiveEntry::pending(
        remote("sd/video/clip.mp4"),
        Some(31_104_000_000),
        1234,
        ids[0].clone(),
        1_790_000_000,
    );
    clip.state = EntryState::Stored;
    clip.source = Some("C:\\Users\\u\\Videos\\Clip Ü.MP4".into());
    clip.duration = Some(Duration::new(10, 41_666_667));
    clip.resolution = Some(Size::new(480, 1920));
    record.record(clip);
    for (n, (path, state)) in [
        ("internal/image/a.png", EntryState::Pending),
        ("internal/video/demon_open.mp4.mp4.mp4", EntryState::Missing),
        ("sd/video/NVI.mp427034822.mp4", EntryState::Deleted),
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = ArchiveEntry::pending(
            remote(path),
            Some(64),
            99,
            ids[n + 1].clone(),
            1_790_000_100,
        );
        entry.state = state;
        record.record(entry);
    }
    record.set_boot(&BootMedia::File(remote("sd/video/clip.mp4")));
    let named = ScreenKey::named(ModelId("turing-8.8"), "desk");
    let entry = ArchiveEntry::pending(remote("internal/video/x.mp4"), None, 7, ids[0].clone(), 1);
    catalog.screen_mut(&named).record(entry);
    catalog
}

/// A transcoder without ffmpeg: nothing on its `PATH`, nothing configured.
fn without_ffmpeg() -> FfmpegTranscoder {
    let lookup = Lookup {
        configured: None,
        search_path: Some(OsString::new()),
    };
    FfmpegTranscoder::with_lookup(lookup, HostSystem::Other)
}

/// A transcoder with ffmpeg: reads files natively and takes plain pictures
/// of the size asked, recording what it was asked; a broken one fails.
#[derive(Default)]
struct Posters {
    taken: Vec<PosterSpec>,
    broken: bool,
}

impl MediaTranscoder for Posters {
    fn tools(&mut self) -> MediaTools {
        MediaTools::Ready {
            version: "fake".into(),
        }
    }

    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
        probe::probe_file(Path::new(&source.0), || None)
    }

    fn transcode(
        &mut self,
        _: &MediaLocation,
        _: &TranscodeTarget,
        _: &mut Job<'_>,
    ) -> Result<MediaLocation> {
        Err(BezelError::Unsupported("converting".into()))
    }

    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        Ok(fs::read(&source.0).unwrap())
    }

    fn stream(&mut self, _: &MediaLocation, _: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        Err(BezelError::Unsupported("decoding".into()))
    }

    fn poster(&mut self, _: &MediaLocation, spec: PosterSpec) -> Result<Frame> {
        self.taken.push(spec);
        if self.broken {
            return Err(BezelError::InvalidInput("no picture 1.0 s in".into()));
        }
        let color = Rgba {
            r: 250,
            g: 20,
            b: 60,
            a: 255,
        };
        Ok(Frame::filled(spec.size, color))
    }
}

#[test]
fn copies_are_the_exact_bytes_sent_and_survive_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let root = storage_dir(dir.path());
    assert_eq!(root, dir.path().join("bezel").join("storage"));
    let mut store = DiskArchive::open(&root).unwrap();
    assert_eq!(store.root(), root);
    // Nothing saved yet: the default catalog, and no file for it.
    assert_eq!(store.load().unwrap(), Catalog::default());
    assert_eq!(store.load().unwrap().limit, DEFAULT_CACHE_LIMIT);
    assert_eq!(file_names(&root), Vec::<String>::new());

    let video = Movie::in_rev_c_profile().bytes();
    let picture = png(64, 32);
    let stream = vec![0, 0, 0, 1, 0x67, 0x64, 0x00, 0x0a];
    let other = b"neither a picture nor a video".to_vec();
    let sent = [&video, &picture, &stream, &other];
    let ids: Vec<ContentId> = sent.iter().map(|b| store.keep(b).unwrap()).collect();
    assert_eq!(ids[0], content_id(&video));
    // Named by their SHA-256 and the extension of their bytes.
    let files = root.join("files");
    let mut expected: Vec<String> = ["mp4", "png", "h264", "bin"]
        .iter()
        .zip(&ids)
        .map(|(ext, id)| format!("{id}.{ext}"))
        .collect();
    expected.sort();
    assert_eq!(file_names(&files), expected);
    // Sent to the card too: the same id, still one copy.
    assert_eq!(store.keep(&video).unwrap(), ids[0]);
    assert_eq!(file_names(&files), expected);

    let catalog = sample_catalog(&ids);
    store.save(&catalog).unwrap();
    // Replaced whole: no temporary file is left next to it.
    assert_eq!(file_names(&root), ["catalog.json"]);
    let text = fs::read_to_string(root.join("catalog.json")).unwrap();
    for field in [
        r#""schema": 1"#,
        r#""limit": 3221225472"#,
        r#""model": "turing-8.8""#,
        r#""name": "desk""#,
        r#""boot": "sd/video/clip.mp4""#,
        r#""path": "sd/video/NVI.mp427034822.mp4""#,
        r#""durationNs": 10041666667"#,
        r#""sentAt": 1790000000"#,
        r#""state": "deleted""#,
    ] {
        assert!(text.contains(field), "{field} in {text}");
    }

    // Another process opening the folder reads the same catalog and bytes.
    let mut reopened = DiskArchive::open(storage_dir(dir.path())).unwrap();
    assert_eq!(reopened, store);
    assert_eq!(reopened.load().unwrap(), catalog);
    for (id, bytes) in ids.iter().zip(sent) {
        assert_eq!(reopened.read(id).unwrap().as_ref(), Some(bytes));
    }
    let mut changed = catalog.clone();
    changed.limit = 1 << 30;
    changed.copies.remove(&ids[3]);
    reopened.save(&changed).unwrap();
    assert_eq!(store.load().unwrap(), changed);

    // Discarded once; again is not an error; the other copies stay.
    reopened.discard(&ids[3]).unwrap();
    reopened.discard(&ids[3]).unwrap();
    assert_eq!(reopened.read(&ids[3]).unwrap(), None);
    assert_eq!(reopened.copy_path(&ids[3]).unwrap(), None);
    assert_eq!(file_names(&files).len(), 3);
    assert_eq!(store.read(&ids[0]).unwrap(), Some(video));
}

#[test]
fn thumbnails_fit_160_px_and_videos_need_ffmpeg() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let thumbs = dir.path().join("thumbs");
    let mut no_ffmpeg = without_ffmpeg();
    // Images, decoded natively: scaled down to fit 160 px keeping their
    // shape; a smaller one is kept as it is.
    for (bytes, fitted) in [
        (png(640, 320), (160, 80)),
        (rgb(300, 1200, ImageFormat::Jpeg), (40, 160)),
        (rgb(200, 100, ImageFormat::Gif), (160, 80)),
        (rgb(100, 40, ImageFormat::Bmp), (100, 40)),
    ] {
        let id = store.keep(&bytes).unwrap();
        let thumb = store.thumbnail(&id, &mut no_ffmpeg).unwrap().unwrap();
        assert_eq!(dimensions(&thumb), fitted);
        // Kept as thumbs/<sha256>.png, and still there without the copy.
        let kept = thumbs.join(format!("{id}.png"));
        assert_eq!(fs::read(&kept).unwrap(), thumb);
        store.discard(&id).unwrap();
        assert_eq!(store.thumbnail(&id, &mut no_ffmpeg).unwrap(), Some(thumb));
    }
    let never = content_id(b"never kept");
    assert_eq!(store.thumbnail(&never, &mut no_ffmpeg).unwrap(), None);

    // Videos without ffmpeg: none, and none kept, so a later call with
    // ffmpeg makes it. A raw H.264 stream cannot even be read without it.
    let video = store.keep(&Movie::in_rev_c_profile().bytes()).unwrap();
    let stream = store.keep(&[0, 0, 0, 1, 0x67, 0x64]).unwrap();
    for id in [&video, &stream] {
        assert_eq!(store.thumbnail(id, &mut no_ffmpeg).unwrap(), None);
        assert!(!thumbs.join(format!("{id}.png")).exists());
    }
    // With ffmpeg: the 480x1920 clip's picture 1 s in, fitting 160 px.
    let mut ffmpeg = Posters::default();
    let thumb = store.thumbnail(&video, &mut ffmpeg).unwrap().unwrap();
    assert_eq!(dimensions(&thumb), (40, 160));
    assert_eq!(ffmpeg.taken.len(), 1);
    assert_eq!(ffmpeg.taken[0].size, Size::new(40, 160));
    assert_eq!(ffmpeg.taken[0].at, Duration::from_secs(1));
    // Made once, then read back.
    assert_eq!(store.thumbnail(&video, &mut ffmpeg).unwrap(), Some(thumb));
    assert_eq!(ffmpeg.taken.len(), 1);
}

#[test]
fn a_broken_copy_is_an_error_not_a_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let mut broken = Posters {
        broken: true,
        ..Posters::default()
    };
    let mut picture = png(8, 8);
    picture.truncate(40);
    let damaged = store.keep(&picture).unwrap();
    let err = store.thumbnail(&damaged, &mut broken).unwrap_err();
    assert!(err.to_string().contains("not a readable image"), "{err}");
    let video = store.keep(&Movie::in_rev_c_profile().bytes()).unwrap();
    let err = store.thumbnail(&video, &mut broken).unwrap_err();
    assert!(err.to_string().contains("no picture"), "{err}");
    // Bytes nothing can read have none.
    let unknown = store.keep(b"neither a picture nor a video").unwrap();
    assert_eq!(store.thumbnail(&unknown, &mut broken).unwrap(), None);
    assert_eq!(file_names(&dir.path().join("thumbs")), Vec::<String>::new());
}

#[test]
fn thumbnails_never_grow_nor_lose_a_side() {
    assert_eq!(fit(Size::new(1920, 480), 160), Size::new(160, 40));
    assert_eq!(fit(Size::new(480, 1920), 160), Size::new(40, 160));
    assert_eq!(fit(Size::new(333, 333), 160), Size::new(160, 160));
    assert_eq!(fit(Size::new(161, 1), 160), Size::new(160, 1));
    assert_eq!(fit(Size::new(10_000, 2), 160), Size::new(160, 1));
    assert_eq!(fit(Size::new(160, 90), 160), Size::new(160, 90));
    assert_eq!(fit(Size::new(2, 1), 160), Size::new(2, 1));
}

#[test]
fn a_damaged_catalog_is_an_error_naming_it_and_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let path = dir.path().join("catalog.json");
    let sha = content_id(b"x").to_string();
    let entry = |path: &str, state: &str| {
        format!(
            r#"{{"schema": 1, "limit": 5, "screens": [{{"model": "turing-8.8", "entries": [
            {{"path": "{path}", "size": 1, "content": "{sha}", "sentAt": 1, "state": "{state}"}}]}}]}}"#
        )
    };
    let twice = r#"{"schema": 1, "limit": 5, "screens": [{"model": "m"}, {"model": "m"}]}"#;
    let cases: Vec<(String, &str)> = vec![
        (String::new(), "EOF"),
        (r#"{"schema": 1, "limit": "#.into(), "EOF"),
        (r#""text""#.into(), "invalid type"),
        (r#"{"limit": 5}"#.into(), "no schema number"),
        (
            r#"{"schema": 2, "limit": 5, "screens": "renamed"}"#.into(),
            "newer Bezel (schema 2",
        ),
        (r#"{"schema": 0, "limit": 5}"#.into(), "unknown schema 0"),
        (
            r#"{"schema": 1, "limit": 5, "copies": ["abc"]}"#.into(),
            "\"abc\" is not a SHA-256",
        ),
        (
            entry("sd/video/a.mp4", "gone"),
            "sd/video/a.mp4: unknown state",
        ),
        (
            entry("floppy/video/a.mp4", "stored"),
            "\"floppy/video/a.mp4\" is not a screen path",
        ),
        (twice.into(), "screen m is listed twice"),
    ];
    for (text, why) in cases {
        fs::write(&path, &text).unwrap();
        let err = store.load().unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, BezelError::InvalidInput(_)), "{message}");
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains(why), "{why:?} in {message}");
        assert_eq!(fs::read_to_string(&path).unwrap(), text, "never reset");
    }
    // The good shape of the same entry loads.
    fs::write(&path, entry("sd/video/a.mp4", "stored")).unwrap();
    let catalog = store.load().unwrap();
    assert_eq!(catalog.limit, 5);
    let record = catalog.screen(&ScreenKey::new(ModelId("turing-8.8")));
    assert_eq!(record.unwrap().entries[0].state, EntryState::Stored);

    // A store cannot live inside a file.
    let err = DiskArchive::open(&path).unwrap_err();
    assert!(err.to_string().contains("cannot create"), "{err}");
}

#[test]
fn a_damaged_copy_is_an_error_and_sending_it_again_repairs_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let bytes = png(8, 8);
    let id = store.keep(&bytes).unwrap();
    let file = store.copy_path(&id).unwrap().unwrap();
    assert_eq!(file, dir.path().join("files").join(format!("{id}.png")));
    fs::write(&file, b"bit rot").unwrap();
    let err = store.read(&id).unwrap_err().to_string();
    assert!(err.contains("damaged"), "{err}");
    assert!(err.contains(&file.display().to_string()), "{err}");
    assert_eq!(store.keep(&bytes).unwrap(), id);
    assert_eq!(store.read(&id).unwrap(), Some(bytes));
}

#[test]
fn copies_are_found_by_id_whatever_their_extension() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let files = dir.path().join("files");
    let bytes = b"a clip an older Bezel kept as .mov".to_vec();
    let id = content_id(&bytes);
    let mov = files.join(format!("{id}.mov"));
    fs::write(&mov, &bytes).unwrap();
    // A temporary file and another name starting with the id are no copy.
    let partial = files.join(format!(".{id}.bin.1-0.tmp"));
    fs::write(&partial, b"partial").unwrap();
    fs::write(files.join(format!("{id}x.bin")), b"other").unwrap();
    fs::create_dir(files.join(format!("{id}.dir"))).unwrap();

    assert_eq!(store.copy_path(&id).unwrap(), Some(mov.clone()));
    assert_eq!(store.read(&id).unwrap(), Some(bytes.clone()));
    assert_eq!(store.keep(&bytes).unwrap(), id);
    assert_eq!(file_names(&files).len(), 3, "kept once");
    store.discard(&id).unwrap();
    assert!(!mov.exists() && partial.exists());
    assert_eq!(store.read(&id).unwrap(), None);
}

#[test]
fn a_save_replaces_a_catalog_another_reader_holds_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let first = Catalog {
        limit: 1,
        ..Catalog::default()
    };
    store.save(&first).unwrap();
    let path: PathBuf = dir.path().join("catalog.json");
    let reader = fs::File::open(&path).unwrap();
    let second = Catalog {
        limit: 2,
        ..Catalog::default()
    };
    store.save(&second).unwrap();
    drop(reader);
    assert_eq!(store.load().unwrap(), second);
    assert_eq!(file_names(dir.path()), ["catalog.json"]);
}

/// The fake ffmpeg answers a picture request with `AAAABBBB`: two pixels.
#[cfg(unix)]
#[test]
fn a_video_thumbnail_is_the_picture_ffmpeg_takes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DiskArchive::open(dir.path()).unwrap();
    let tiny = Movie {
        width: 2,
        height: 1,
        ..Movie::in_rev_c_profile()
    };
    let id = store.keep(&tiny.bytes()).unwrap();
    let lookup = Lookup {
        configured: None,
        search_path: Some(crate::fakes::dir().join("ready").into_os_string()),
    };
    let mut media = FfmpegTranscoder::with_lookup(lookup, HostSystem::Debian);
    // Off the UI thread: a clone reaches the same store.
    let worker = store.clone();
    let thumb = worker.thumbnail(&id, &mut media).unwrap().unwrap();
    let picture = image::load_from_memory(&thumb).unwrap().to_rgba8();
    assert_eq!(picture.dimensions(), (2, 1));
    assert_eq!(picture.into_raw(), b"AAAABBBB");
    assert_eq!(store.thumbnail(&id, &mut media).unwrap(), Some(thumb));
}
