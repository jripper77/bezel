//! The storage tab's commands on the fake 8.8" (in-memory storage) with a
//! scripted media converter.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::job::{Job, JobPhase, Progress};
use bezel_core::domain::media::{
    FrameRate, MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget, VideoCodec,
    VideoPixelFormat, VideoTrack, cover_crop,
};
use bezel_core::domain::storage::{Repeat, StartMode};
use bezel_core::domain::theme::{AssetRef, Background, Theme};
use bezel_core::ports::{MediaLocation, MediaTranscoder, VideoFrames};
use bezel_core::{BezelError, Result};
use bezel_devices::fake::{FAKE_UPLOAD_CHUNK, FakeStorage, Playback, StorageCall};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_render::{SkiaRenderer, SystemFonts};
use bezel_sensors::FakeSensors;
use bezel_themes::FsThemeStore;

use super::*;
use crate::library::ThemeLibrary;
use crate::settings::SettingsFile;
use crate::studio::Studio;

const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 0,
    weekday: 2,
};
const KEY: &str = "/dev/ttyACM1";
/// The 8.8" panel in its native orientation.
const NATIVE: Size = Size::new(480, 1920);
/// Picture size of the local videos that need a conversion.
const WIDE: Size = Size::new(1920, 1080);
/// Bytes a scripted conversion writes.
const CONVERTED_BYTES: usize = 5000;

/// A media converter over real files on disk, scripted by name: `native*.mp4`
/// is already in the 8.8"'s profile, other videos (`.mp4`, `.mov`) are
/// 1920x1080 with sound, `.png` is an image, anything else is unknown. A
/// conversion writes a `native-converted-N.mp4` next to the source.
pub(crate) struct FakeMedia {
    ready: bool,
    tool: Option<PathBuf>,
    /// The conversions asked for.
    pub(crate) converted: Arc<Mutex<Vec<TranscodeTarget>>>,
    /// The videos decoded on the host, and how.
    pub(crate) streamed: Arc<Mutex<Vec<(MediaLocation, StreamSpec)>>>,
}

/// The color of every picture of a video [`FakeMedia`] decodes.
pub(crate) const STREAMED: Rgba = Rgba::opaque(0, 200, 0);

/// A decoded video whose pictures are all [`STREAMED`].
struct Solid(Frame);

impl VideoFrames for Solid {
    fn frame_at(&mut self, _: Duration) -> Result<&Frame> {
        Ok(&self.0)
    }
}

impl FakeMedia {
    /// With ffmpeg.
    pub(crate) fn ready() -> Self {
        Self {
            ready: true,
            tool: Some(PathBuf::from("/usr/bin/ffmpeg")),
            converted: Arc::default(),
            streamed: Arc::default(),
        }
    }

    /// Without ffmpeg.
    pub(crate) fn missing() -> Self {
        Self {
            ready: false,
            tool: None,
            converted: Arc::default(),
            streamed: Arc::default(),
        }
    }
}

fn video(size: Size, audio: bool) -> (MediaFormat, Option<Size>, Option<VideoTrack>, bool) {
    let track = VideoTrack {
        codec: VideoCodec::H264,
        pixel_format: Some(VideoPixelFormat::Yuv420p),
        b_frames: Some(false),
        frame_rate: FrameRate::new(24, 1),
        duration: Some(Duration::from_secs(2)),
    };
    (MediaFormat::Mp4, Some(size), Some(track), audio)
}

impl MediaTranscoder for FakeMedia {
    fn tools(&mut self) -> MediaTools {
        if self.ready {
            MediaTools::Ready {
                version: "7.1".into(),
            }
        } else {
            MediaTools::Missing {
                install_hints: vec!["sudo dnf install ffmpeg".into()],
            }
        }
    }

    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
        let path = Path::new(&source.0);
        let bytes = std::fs::metadata(path)
            .map_err(|e| BezelError::InvalidInput(e.to_string()))?
            .len();
        let name = file_name(path).to_lowercase();
        let extension = name.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
        let (format, dimensions, video, has_audio) = match extension {
            "png" => (MediaFormat::Png, Some(Size::new(64, 64)), None, false),
            "mp4" if name.starts_with("native") => video(NATIVE, false),
            "mp4" | "mov" => video(WIDE, true),
            _ => (MediaFormat::Other, None, None, false),
        };
        Ok(MediaInfo {
            format,
            bytes,
            dimensions,
            video,
            has_audio,
        })
    }

    fn transcode(
        &mut self,
        source: &MediaLocation,
        target: &TranscodeTarget,
        job: &mut Job<'_>,
    ) -> Result<MediaLocation> {
        if !self.ready {
            return Err(BezelError::Unsupported("no ffmpeg".into()));
        }
        let mut converted = self.converted.lock().unwrap();
        converted.push(*target);
        for done in [0, 1000, 2000] {
            job.checkpoint()?;
            job.report(Progress::new(JobPhase::Convert, done, 2000));
        }
        let dir = Path::new(&source.0).parent().unwrap();
        let output = dir.join(format!("native-converted-{}.mp4", converted.len()));
        std::fs::write(&output, vec![7; CONVERTED_BYTES]).unwrap();
        Ok(MediaLocation(output.display().to_string()))
    }

    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        std::fs::read(&source.0).map_err(|e| BezelError::InvalidInput(e.to_string()))
    }

    fn stream(&mut self, source: &MediaLocation, spec: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        if !self.ready {
            return Err(BezelError::Unsupported("no ffmpeg".into()));
        }
        self.streamed.lock().unwrap().push((source.clone(), spec));
        Ok(Box::new(Solid(Frame::filled(spec.size, STREAMED))))
    }
}

impl MediaSetup for FakeMedia {
    fn set_tool_path(&mut self, path: Option<PathBuf>) {
        self.ready = path.as_deref().is_some_and(|p| p.ends_with("ffmpeg"));
        self.tool = path.filter(|_| self.ready);
    }

    fn tool_in_use(&mut self) -> Option<PathBuf> {
        self.tool.clone().filter(|_| self.ready)
    }
}

struct Fixture {
    backend: Backend,
    connector: FakeConnector,
    /// The conversions the scripted converter ran.
    converted: Arc<Mutex<Vec<TranscodeTarget>>>,
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    /// A local file of `bytes` bytes called `name`.
    fn local(&self, name: &str, bytes: usize) -> PathBuf {
        let path = self.root.join("local").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let data: Vec<u8> = (0..bytes).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, data).unwrap();
        path
    }

    fn storage(&self) -> FakeStorage {
        self.connector.log().storage
    }

    /// Storage calls that change what the screen stores, shows or keeps.
    fn writes(&self) -> Vec<StorageCall> {
        let calls = self.storage().calls;
        calls
            .into_iter()
            .filter(StorageCall::changes_the_screen)
            .collect()
    }

    fn prepare(&self, local: &Path, medium: &str) -> PrepareDto {
        self.backend
            .prepare_upload(KEY, local, medium, TIME)
            .unwrap()
    }

    fn ready(&self, local: &Path, medium: &str) -> PreparedDto {
        match self.prepare(local, medium) {
            PrepareDto::Ready(ready) => ready,
            PrepareDto::Refused(refusal) => panic!("refused: {refusal:?}"),
        }
    }

    fn run(&self, ticket: u64, overwrite: Confirm) -> (UiResult<JobDto>, Vec<Progress>) {
        let mut seen = Vec::new();
        let result = self
            .backend
            .run_upload(ticket, overwrite, TIME, &mut |p| seen.push(p));
        (result, seen)
    }
}

fn fixture_with(name: &str, storage: FakeStorage, media: FakeMedia) -> Fixture {
    let root = std::env::temp_dir().join(format!("bezel-storage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let connector = FakeConnector::with_storage(storage);
    let converted = Arc::clone(&media.converted);
    let theme = Theme::blank("Start", NATIVE, Orientation::ReversePortrait);
    let studio = Studio::new(
        Box::new(FakeSensors::demo()),
        Box::new(SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip)),
        Language::English,
        theme,
    );
    let backend = Backend {
        bus: Arc::new(FakeBus::turing_88()),
        connector: Arc::new(connector.clone()),
        store: Arc::new(FsThemeStore),
        library: ThemeLibrary::new(root.join("themes"), vec![]),
        settings: SettingsFile::new(root.join("settings.json")),
        system_language: Language::English,
        make_sensors: Arc::new(|_| Box::new(FakeSensors::demo())),
        fonts: Vec::new(),
        studio: crate::backend::Session::new(studio),
        storage: StorageState::new(Box::new(media), root.join("scratch")),
    };
    Fixture {
        backend,
        connector,
        converted,
        root,
    }
}

fn fixture(name: &str) -> Fixture {
    fixture_with(name, FakeStorage::default(), FakeMedia::ready())
}

fn remote_path(text: &str) -> RemotePath {
    RemotePath::parse(text).unwrap()
}

#[test]
fn the_tab_lists_capacity_and_the_files_of_both_media() {
    let storage = FakeStorage::default()
        .with_card(5_000_000)
        .with_file(remote_path("internal/image/logo.png"), vec![1; 100])
        .with_file(remote_path("sd/video/rain.mp4"), vec![2; 300]);
    let f = fixture_with("overview", storage, FakeMedia::ready());
    let dto = f.backend.storage_overview(KEY, TIME).unwrap();
    assert_eq!(dto.internal.used, 100);
    assert_eq!(dto.card.unwrap().free, 5_000_000 - 300);
    let folders: Vec<(&str, &str, usize)> = dto
        .folders
        .iter()
        .map(|f| (f.medium, f.kind, f.files.len()))
        .collect();
    assert_eq!(
        folders,
        vec![
            ("internal", "image", 1),
            ("internal", "video", 0),
            ("sd", "image", 0),
            ("sd", "video", 1)
        ]
    );
    let rain = &dto.folders[3].files[0];
    assert_eq!(
        (rain.path.as_str(), rain.name.as_str(), rain.size),
        ("sd/video/rain.mp4", "rain.mp4", Some(300))
    );
    assert!(f.writes().is_empty(), "the overview only asks");

    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["folders"][0]["files"][0]["medium"], "internal");
    assert!(json["card"]["total"].is_u64());

    // Without a card only the internal folders are listed (listing creates
    // the folder).
    let f = fixture("overview-nocard");
    let dto = f.backend.storage_overview(KEY, TIME).unwrap();
    assert_eq!(dto.card, None);
    assert_eq!(dto.folders.len(), 2);
    let err = f.backend.storage_overview("COM9", TIME).unwrap_err();
    assert_eq!(err.code(), "screenNotFound");
}

#[test]
fn deleting_and_the_boot_media_need_the_dialogs_confirmation() {
    let storage = FakeStorage::default()
        .with_file(remote_path("internal/video/clip.mp4"), vec![1; 500])
        .with_file(remote_path("internal/image/logo.png"), vec![1; 50]);
    let f = fixture_with("confirm", storage, FakeMedia::ready());
    let clip = "internal/video/clip.mp4";

    let err = f
        .backend
        .delete_stored(KEY, clip, Confirm::No, TIME)
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    let err = f
        .backend
        .set_boot_media(KEY, Some(clip), Some(40), Confirm::No, TIME)
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    let err = f
        .backend
        .set_boot_media(KEY, None, Some(40), Confirm::No, TIME)
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert!(f.writes().is_empty(), "nothing reached the screen");
    assert!(
        f.connector.log().brightness.is_empty(),
        "not even the brightness"
    );
    let err = f
        .backend
        .set_boot_media(KEY, Some(clip), Some(101), Confirm::Yes, TIME)
        .unwrap_err();
    assert_eq!(err.code(), "brightnessRange");

    // The screen starts with the brightness set in the session.
    f.backend
        .set_boot_media(KEY, Some(clip), Some(40), Confirm::Yes, TIME)
        .unwrap();
    let storage = f.storage();
    assert_eq!(storage.start_mode, Some(StartMode::Video));
    assert_eq!(
        storage.playback,
        Playback::Video(remote_path(clip), Repeat::Loop)
    );
    let forty = Brightness::new(40).unwrap();
    assert_eq!(f.connector.log().brightness, [forty]);
    // None set: the link's level stays.
    f.backend
        .set_boot_media(KEY, None, None, Confirm::Yes, TIME)
        .unwrap();
    assert_eq!(f.storage().start_mode, Some(StartMode::Default));
    assert_eq!(f.connector.log().brightness, [forty]);

    f.backend
        .delete_stored(KEY, clip, Confirm::Yes, TIME)
        .unwrap();
    assert!(!f.storage().files.contains_key(&remote_path(clip)));
    assert_eq!(
        f.backend
            .delete_stored(KEY, "elsewhere/clip.mp4", Confirm::Yes, TIME)
            .unwrap_err()
            .code(),
        "invalidInput"
    );

    // Play and stop on a screen that is not live.
    f.backend
        .play_stored(KEY, "internal/image/logo.png", TIME)
        .unwrap();
    assert_eq!(
        f.storage().playback,
        Playback::Image(remote_path("internal/image/logo.png"))
    );
    f.backend.stop_playback(KEY, TIME).unwrap();
    assert_eq!(f.storage().playback, Playback::Idle);
    let err = f.backend.play_stored(KEY, clip, TIME).unwrap_err();
    assert_eq!(err.code(), "invalidInput", "{err:?}");
}

#[test]
fn an_upload_is_prepared_confirmed_sent_and_verified() {
    let f = fixture("upload");
    let local = f.local("Native Clip.mp4", 3000);
    let ready = f.ready(&local, "internal");
    assert_eq!(ready.source, "Native Clip.mp4");
    assert_eq!(ready.target.path, "internal/video/native_clip.mp4");
    assert_eq!((ready.bytes, ready.format.as_str()), (3000, "MP4"));
    assert_eq!(ready.convert, None, "already in the screen's profile");
    assert_eq!(ready.replaces, None);
    assert!(f.writes().is_empty(), "preparing only asks");

    let (result, seen) = f.run(ready.ticket, Confirm::No);
    let JobDto::Done { file, converted } = result.unwrap() else {
        panic!("not done");
    };
    assert_eq!(
        (file.path.as_str(), file.size),
        ("internal/video/native_clip.mp4", Some(3000))
    );
    assert!(!converted);
    assert!(
        seen.iter()
            .any(|p| p.phase == JobPhase::Upload && p.done == 3000)
    );
    assert_eq!(seen.last(), Some(&Progress::new(JobPhase::Verify, 1, 1)));
    let err = f.run(ready.ticket, Confirm::No).0.unwrap_err();
    assert_eq!(err.code(), "stale", "a ticket runs once");

    // An image goes to the image folder of the card.
    let f = fixture_with(
        "upload-card",
        FakeStorage::default().with_card(1_000_000),
        FakeMedia::ready(),
    );
    let ready = f.ready(&f.local("Logo.PNG", 400), "sd");
    assert_eq!(ready.target.path, "sd/image/logo.png");
    assert!(matches!(
        f.run(ready.ticket, Confirm::No).0,
        Ok(JobDto::Done { .. })
    ));
    let err = f
        .backend
        .prepare_upload(KEY, &f.local("x.png", 1), "cloud", TIME)
        .unwrap_err();
    assert_eq!(err.code(), "unknownMedium");
}

#[test]
fn replacing_a_file_needs_the_overwrite_confirmation() {
    let f = fixture("replace");
    let local = f.local("native.mp4", 2000);
    let first = f.ready(&local, "internal");
    f.run(first.ticket, Confirm::No).0.unwrap();

    let again = f.ready(&local, "internal");
    let replaces = again.replaces.clone().unwrap();
    assert_eq!(
        (replaces.name.as_str(), replaces.size),
        ("native.mp4", Some(2000))
    );
    let uploads = |f: &Fixture| {
        f.writes()
            .iter()
            .filter(|c| matches!(c, StorageCall::Upload(..)))
            .count()
    };
    let err = f.run(again.ticket, Confirm::No).0.unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert_eq!(uploads(&f), 1, "nothing was sent over the file");

    let confirmed = f.ready(&local, "internal");
    assert!(matches!(
        f.run(confirmed.ticket, Confirm::Yes).0,
        Ok(JobDto::Done { .. })
    ));
    assert_eq!(uploads(&f), 2);
}

#[test]
fn a_cancelled_upload_reports_the_partial_file() {
    let f = fixture("cancel");
    assert!(!f.backend.cancel_job(), "nothing runs");
    let local = f.local("native-big.mp4", FAKE_UPLOAD_CHUNK * 5);
    let ready = f.ready(&local, "internal");
    let mut seen = Vec::new();
    let result = f
        .backend
        .run_upload(ready.ticket, Confirm::No, TIME, &mut |p| {
            seen.push(p);
            if p.phase == JobPhase::Upload && p.done > 0 {
                assert!(f.backend.cancel_job());
            }
        });
    let JobDto::Cancelled { path, partial } = result.unwrap() else {
        panic!("not cancelled");
    };
    assert_eq!(path, "internal/video/native-big.mp4");
    assert_eq!(partial, Some(FAKE_UPLOAD_CHUNK as u64));
    assert!(!f.backend.storage.is_busy(), "the screen is free again");
    // The partial file is deleted only on request, with confirmation.
    let listed = f.backend.storage_overview(KEY, TIME).unwrap();
    assert_eq!(
        listed.folders[1].files[0].size,
        Some(FAKE_UPLOAD_CHUNK as u64)
    );
    f.backend
        .delete_stored(KEY, &path, Confirm::Yes, TIME)
        .unwrap();
    assert!(f.storage().files.is_empty());
}

#[test]
fn a_video_to_convert_is_turned_like_the_theme_and_cropped() {
    let f = fixture("convert");
    // The screen is mounted horizontally.
    f.backend
        .studio()
        .set_theme(Theme::blank("Wide", NATIVE, Orientation::Landscape));
    let local = f.local("Férias 2026.mov", 9000);
    let ready = f.ready(&local, "internal");
    assert_eq!(ready.target.path, "internal/video/f_rias_2026.mp4");
    let convert = ready.convert.unwrap();
    assert_eq!((convert.width, convert.height), (480, 1920));
    assert_eq!(convert.quarter_turns, 1, "landscape → reverse portrait");
    assert!(convert.cropped);

    let (result, seen) = f.run(ready.ticket, Confirm::No);
    let JobDto::Done { file, converted } = result.unwrap() else {
        panic!("not done");
    };
    assert!(converted);
    assert_eq!(file.size, Some(CONVERTED_BYTES as u64));
    assert_eq!(seen[0], Progress::new(JobPhase::Convert, 0, 2000));
    let targets = f.converted.lock().unwrap().clone();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].quarter_turns, 1);
    assert_eq!(targets[0].crop, cover_crop(WIDE, 1, NATIVE));
}

#[test]
fn without_ffmpeg_a_video_to_convert_is_refused_inline() {
    let f = fixture_with("no-ffmpeg", FakeStorage::default(), FakeMedia::missing());
    let tools = f.backend.media_tools();
    assert!(!tools.ready);
    assert_eq!(
        tools.install_hints,
        vec!["sudo dnf install ffmpeg".to_string()]
    );

    let PrepareDto::Refused(refusal) = f.prepare(&f.local("clip.mp4", 900), "internal") else {
        panic!("not refused");
    };
    assert_eq!(refusal.code, "needsConverter");
    let codes: Vec<&str> = refusal.mismatches.iter().map(|m| m.code).collect();
    assert_eq!(codes, vec!["audio", "resolution"]);
    assert_eq!(refusal.mismatches[1].found.as_deref(), Some("1920x1080"));
    assert_eq!(refusal.mismatches[1].expected.as_deref(), Some("480x1920"));
    // A video already in the profile still goes.
    let ready = f.ready(&f.local("native.mp4", 900), "internal");
    assert_eq!(ready.convert, None);

    // Locate: a file that is no ffmpeg changes nothing.
    let rejected = f.backend.locate_ffmpeg(Path::new("/home/me/notes.txt"));
    assert_eq!(rejected.rejected.as_deref(), Some("/home/me/notes.txt"));
    assert!(!rejected.ready);
    assert_eq!(f.backend.settings.load().ffmpeg_path, None);
    let located = f.backend.locate_ffmpeg(Path::new("/opt/ffmpeg/bin/ffmpeg"));
    assert!(located.ready);
    assert_eq!(located.rejected, None);
    assert_eq!(
        located.configured.as_deref(),
        Some("/opt/ffmpeg/bin/ffmpeg")
    );
    assert_eq!(
        f.backend.settings.load().ffmpeg_path.as_deref(),
        Some("/opt/ffmpeg/bin/ffmpeg")
    );
    assert!(matches!(
        f.prepare(&f.local("clip.mp4", 900), "internal"),
        PrepareDto::Ready(_)
    ));
}

#[test]
fn a_full_medium_lists_candidates_and_deletes_nothing() {
    let mut storage =
        FakeStorage::default().with_file(remote_path("internal/video/old.mp4"), vec![1; 6000]);
    storage.internal_total = 10_000;
    let f = fixture_with("full", storage, FakeMedia::ready());
    let PrepareDto::Refused(refusal) = f.prepare(&f.local("native.mp4", 5000), "internal") else {
        panic!("not refused");
    };
    assert_eq!(refusal.code, "noSpace");
    assert_eq!((refusal.bytes, refusal.limit), (Some(5000), Some(4000)));
    assert_eq!(refusal.candidates.len(), 1);
    assert_eq!(refusal.candidates[0].path, "internal/video/old.mp4");
    assert_eq!(refusal.candidates[0].size, Some(6000));
    assert!(f.writes().is_empty(), "nothing is deleted on its own");
    let PrepareDto::Refused(refusal) = f.prepare(&f.local("notes.txt", 10), "internal") else {
        panic!("not refused");
    };
    assert_eq!(refusal.code, "wrongKind");
    let PrepareDto::Refused(refusal) = f.prepare(&f.local("empty.png", 0), "internal") else {
        panic!("not refused");
    };
    assert_eq!(refusal.code, "emptyFile");
    let PrepareDto::Refused(refusal) = f.prepare(&f.local("a.png", 10), "sd") else {
        panic!("not refused");
    };
    assert_eq!(refusal.code, "noCard");
}

#[test]
fn on_a_live_screen_a_job_borrows_the_link_and_frames_pause() {
    let f = fixture("live");
    f.backend.set_live(true, Some(KEY), TIME).unwrap();
    let frames = || f.connector.log().frames.len();
    assert_eq!(frames(), 1);
    let local = f.local("native.mp4", FAKE_UPLOAD_CHUNK * 3);
    let ready = f.ready(&local, "internal");
    assert_eq!(
        frames(),
        2,
        "a frame after the preflight gave the link back"
    );

    let mut checked = false;
    let result = f
        .backend
        .run_upload(ready.ticket, Confirm::No, TIME, &mut |p| {
            if p.phase != JobPhase::Upload || checked {
                return;
            }
            checked = true;
            // The session is not locked: previews render and the loop samples,
            // but no frame reaches the screen.
            let theme = f.backend.session().theme;
            assert!(f.backend.render(&theme, TIME).is_ok());
            f.backend.tick(TIME);
            assert_eq!(f.connector.log().frames.len(), 2);
            // The screen's port has one owner meanwhile.
            assert!(
                f.backend
                    .set_brightness(KEY, 50)
                    .unwrap_err()
                    .to_string()
                    .contains("in use")
            );
            assert!(
                f.backend
                    .release(KEY)
                    .unwrap_err()
                    .to_string()
                    .contains("storage operation")
            );
            assert_eq!(
                f.backend.storage_overview(KEY, TIME).unwrap_err().code(),
                "busy"
            );
        });
    assert!(checked);
    assert!(matches!(result, Ok(JobDto::Done { .. })));
    assert_eq!(frames(), 3, "the link came back with a frame");
    assert_eq!(f.backend.sample().live.as_deref(), Some(KEY));
    f.backend.tick(TIME);
    assert_eq!(frames(), 4, "frames resumed");

    // The theme would hide what the screen plays: no play or stop while live.
    let err = f
        .backend
        .play_stored(KEY, "internal/video/native.mp4", TIME)
        .unwrap_err();
    assert_eq!(err.code(), "live");
    assert_eq!(
        f.backend.stop_playback(KEY, TIME).unwrap_err().code(),
        "live"
    );
}

#[test]
fn live_mode_turned_off_during_a_job_closes_the_link_after_it() {
    let f = fixture("live-off");
    f.backend.set_live(true, Some(KEY), TIME).unwrap();
    let ready = f.ready(&f.local("native.mp4", FAKE_UPLOAD_CHUNK * 2), "internal");
    let before = f.connector.log().frames.len();
    let result = f
        .backend
        .run_upload(ready.ticket, Confirm::No, TIME, &mut |p| {
            if p.phase == JobPhase::Upload && p.done == 0 {
                f.backend.set_live(false, None, TIME).unwrap();
                let err = f.backend.set_live(true, Some(KEY), TIME).unwrap_err();
                assert_eq!(err.code(), "busy", "{err}");
            }
        });
    assert!(matches!(result, Ok(JobDto::Done { .. })));
    assert_eq!(f.backend.sample().live, None);
    assert_eq!(
        f.connector.log().frames.len(),
        before,
        "no frame after live mode ended"
    );
}

fn video_theme() -> (Theme, BTreeMap<AssetRef, Vec<u8>>) {
    let mut theme = Theme::blank("Video", NATIVE, Orientation::ReversePortrait);
    theme.background = Background::Video {
        asset: AssetRef("assets/intro.mp4".into()),
        poster: None,
    };
    let mut assets = BTreeMap::new();
    assets.insert(AssetRef("assets/intro.mp4".into()), vec![3; 7000]);
    (theme, assets)
}

fn alpha_at(frame: &Frame, x: u32, y: u32) -> u8 {
    let width = frame.size().width as usize;
    frame.as_rgba()[(y as usize * width + x as usize) * 4 + 3]
}

#[test]
fn sending_the_theme_video_lets_the_live_screen_play_it() {
    let f = fixture("theme-video");
    let (theme, assets) = video_theme();
    f.backend.studio().start(theme, assets, None);
    let err = f.backend.prepare_theme_video(KEY, TIME).unwrap_err();
    assert_eq!(err.code(), "noVideo", "not live");

    f.backend.set_live(true, Some(KEY), TIME).unwrap();
    let video = f.backend.sample().video.unwrap();
    assert_eq!(
        (video.state, video.path.as_deref()),
        ("missing", Some("internal/video/intro.mp4"))
    );
    assert!(f.writes().is_empty(), "nothing is sent when live starts");
    let poster = f.connector.log().frames.last().cloned().unwrap();
    assert_eq!(alpha_at(&poster, 0, 0), 255, "the poster until it is sent");

    let ready = match f.backend.prepare_theme_video(KEY, TIME).unwrap() {
        PrepareDto::Ready(ready) => ready,
        PrepareDto::Refused(r) => panic!("refused: {r:?}"),
    };
    assert_eq!(ready.source, "intro.mp4");
    assert_eq!(ready.target.path, "internal/video/intro.mp4");
    let convert = ready.convert.unwrap();
    assert_eq!((convert.quarter_turns, convert.cropped), (0, true));
    let scratch = f.root.join("scratch").join("intro.mp4");
    assert!(scratch.is_file());

    let (result, _) = f.run(ready.ticket, Confirm::No);
    assert!(matches!(
        result,
        Ok(JobDto::Done {
            converted: true,
            ..
        })
    ));
    assert!(!scratch.exists(), "the copy is removed after the upload");
    let video = f.backend.sample().video.unwrap();
    assert_eq!(video.state, "onDevice");
    assert_eq!(
        f.storage().playback,
        Playback::Video(remote_path("internal/video/intro.mp4"), Repeat::Loop)
    );
    let overlay = f.connector.log().frames.last().cloned().unwrap();
    assert_eq!(alpha_at(&overlay, 0, 0), 0, "the video shows through");
    // The preview keeps the poster.
    let preview = f.backend.render(&f.backend.session().theme, TIME).unwrap();
    assert_eq!(preview[8 + 3], 255);

    // Another background stops the video on the screen.
    let mut plain = f.backend.session().theme;
    plain.background = bezel_themes::dto::BackgroundDto::Color {
        color: "#000000ff".into(),
    };
    f.backend.push(&plain, TIME).unwrap();
    assert_eq!(f.storage().playback, Playback::Idle);
    assert_eq!(f.backend.sample().video, None);
}

#[test]
fn the_progress_throttle_keeps_phase_changes_ends_and_steps() {
    let mut throttle = ProgressThrottle::default();
    let up = |done| Progress::new(JobPhase::Upload, done, 100_000);
    assert!(throttle.pass(up(0)));
    assert!(!throttle.pass(up(100)));
    assert!(throttle.pass(up(500)), "half a percent");
    assert!(!throttle.pass(up(600)));
    assert!(throttle.pass(up(100_000)), "the end");
    assert!(throttle.pass(Progress::new(JobPhase::Verify, 0, 1)));
    assert!(throttle.pass(Progress::new(JobPhase::Convert, 5, 0)));
    assert!(
        throttle.pass(Progress::new(JobPhase::Convert, 6, 0)),
        "unknown total"
    );
}

#[test]
fn core_errors_keep_a_code_the_ui_translates() {
    let x = || "x".to_string();
    let cases = [
        (BezelError::ScreenNotFound(x()), "screenNotFound"),
        (
            BezelError::AccessDenied {
                address: x(),
                reason: x(),
            },
            "accessDenied",
        ),
        (
            BezelError::InUse {
                address: "a".into(),
                holders: vec!["b".into(), "c".into()],
            },
            "inUse",
        ),
        (BezelError::Timeout(x()), "timeout"),
        (BezelError::InvalidInput(x()), "invalidInput"),
        (BezelError::Transport(x()), "transport"),
        (BezelError::Unsupported(x()), "unsupported"),
        (BezelError::Cancelled { partial: None }, "cancelled"),
        (BezelError::NotConfirmed(x()), "notConfirmed"),
        (
            BezelError::Refused(bezel_core::domain::storage::Refusal::EmptyFile),
            "refused",
        ),
        (BezelError::ThemeFile(x()), "themeFile"),
    ];
    for (e, code) in cases {
        let english = e.to_string();
        let ui = UiError::from(e);
        assert_eq!(ui.code(), code);
        if code != "cancelled" {
            assert_eq!(ui.to_string(), english, "the core's own sentence");
        }
    }
    let busy = UiError::from(BezelError::InUse {
        address: "a".into(),
        holders: vec!["b".into(), "c".into()],
    });
    assert_eq!(busy.value("holders"), Some("b, c"));
}
