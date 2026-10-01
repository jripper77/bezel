//! A video background from the Media panel: "Add video…" and files dropped
//! on the canvas or the panel. A video, or a GIF of several pictures, is
//! copied into the theme's assets (safe name, [`Studio::add_video`]) with its
//! poster: a PNG of the theme's canvas taken through the core's media port
//! (`MediaTranscoder::poster`, framed by `PosterSpec`). Without ffmpeg there
//! is no poster and the video is still added: previews then draw the
//! renderer's poster-less background, and the UI says what ffmpeg is for.
//! A picture, a GIF of one picture included, is added as an image.
//!
//! [`Studio::add_video`]: crate::studio::Studio::add_video

use std::path::Path;

use bezel_core::BezelError;
use bezel_core::domain::media::{MediaFormat, MediaInfo};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::ports::{MediaLocation, MediaTranscoder};

use crate::backend::{Backend, MAX_FILE_BYTES, read_limited};
use crate::dto::AddedMediaDto;
use crate::media::{IMAGE_EXTENSIONS, VIDEO_EXTENSIONS, extension_of, is_animated_gif, png_of};
use crate::messages::{ErrorCode, UiError, UiResult};

/// Largest video accepted for a background, bytes (the importer's limit for
/// the videos of other apps' themes). What a screen takes is smaller and is
/// checked when the video is sent to it (D-2026-09-30-release-polish-12).
pub const MAX_VIDEO_BYTES: u64 = 512 * 1024 * 1024;

fn not_media(path: &Path) -> UiError {
    UiError::new(ErrorCode::NotMedia).arg("file", path.display())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What the converter learnt of a video file: what it is (`Err`: a format
/// only ffmpeg reads, and no ffmpeg) and its poster.
struct Inspected {
    info: Result<MediaInfo, BezelError>,
    poster: Result<bezel_core::domain::frame::Frame, BezelError>,
}

/// Probes the video at `location` and takes its poster for a canvas of
/// `canvas`. `None` when the file is not a moving picture.
fn inspect(
    media: &mut dyn MediaTranscoder,
    location: &MediaLocation,
    canvas: bezel_core::domain::geometry::Size,
) -> Option<Inspected> {
    let info = match media.probe(location) {
        Ok(info) if info.video.is_some() || info.format == MediaFormat::Gif => Ok(info),
        Ok(_) => return None,
        Err(e @ BezelError::Unsupported(_)) => Err(e),
        Err(_) => return None,
    };
    let poster = match &info {
        Ok(info) => media.poster(location, PosterSpec::for_canvas(canvas, info)),
        Err(e) => Err(e.clone()),
    };
    Some(Inspected { info, poster })
}

impl Backend {
    /// Adds the file at `path`, picked with "Add video…" or dropped on the
    /// window: a video or an animated GIF becomes a video for a background,
    /// with its poster; a picture (a GIF of one picture too) is added like
    /// [`Backend::add_image`].
    pub fn add_media(&self, path: &Path) -> UiResult<AddedMediaDto> {
        let extension = extension_of(&file_name(path));
        let gif = extension == "gif";
        if !gif && IMAGE_EXTENSIONS.contains(&extension.as_str()) {
            let bytes = read_limited(path, MAX_FILE_BYTES)?;
            return self.add_still(path, bytes);
        }
        if !gif && !VIDEO_EXTENSIONS.contains(&extension.as_str()) {
            return Err(not_media(path));
        }
        let bytes = read_limited(path, MAX_VIDEO_BYTES)?;
        if gif && !is_animated_gif(&bytes) {
            return self.add_still(path, bytes);
        }
        self.add_moving(path, bytes)
    }

    /// Adds the video (or animated GIF) at `path`, whose content is `bytes`,
    /// with its poster when ffmpeg can take one.
    pub(crate) fn add_moving(&self, path: &Path, bytes: Vec<u8>) -> UiResult<AddedMediaDto> {
        let canvas = self.studio().theme().canvas;
        let location = MediaLocation(path.display().to_string());
        let inspected = {
            let mut media = self.storage.media();
            inspect(media.as_mut(), &location, canvas)
        };
        let Inspected { info, poster } = inspected.ok_or_else(|| not_media(path))?;
        let duration = info.ok().and_then(|i| i.video).and_then(|t| t.duration);
        let (png, poster_error) = match poster {
            Ok(frame) => match png_of(&frame) {
                Some(png) => (Some(png), None),
                None => (
                    None,
                    Some(UiError::system("the poster could not be encoded")),
                ),
            },
            Err(e) => (None, Some(UiError::from(e))),
        };
        let size = bytes.len() as u64;
        let (video, poster) = self
            .studio()
            .add_video(&file_name(path), bytes, png, duration);
        Ok(AddedMediaDto {
            reference: video.0,
            kind: "video",
            poster_error: poster_error.filter(|_| poster.is_none()),
            poster: poster.map(|p| p.0),
            bytes: size,
            duration_ms: duration.map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        })
    }

    /// Adds the picture at `path`, whose content is `bytes`.
    fn add_still(&self, path: &Path, bytes: Vec<u8>) -> UiResult<AddedMediaDto> {
        let size = bytes.len() as u64;
        let added = self.add_image_bytes(path, bytes)?;
        Ok(AddedMediaDto {
            reference: added.reference,
            kind: "image",
            poster: None,
            bytes: size,
            duration_ms: None,
            poster_error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use bezel_core::domain::clock::{Language, LocalTime};
    use bezel_core::domain::frame::Rect;
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_core::domain::theme::{AssetRef, Background, Theme};
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;
    use bezel_themes::FsThemeStore;

    use super::*;
    use crate::backend::Session;
    use crate::library::ThemeLibrary;
    use crate::settings::SettingsFile;
    use crate::storage::StorageState;
    use crate::storage::tests::{FakeMedia, POSTER};
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

    struct Fixture {
        backend: Backend,
        posters: Arc<std::sync::Mutex<Vec<(MediaLocation, PosterSpec)>>>,
        root: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    impl Fixture {
        /// A local file called `name` holding `bytes`.
        fn local(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.root.join("local").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    /// A backend editing a horizontal 8.8" theme, with `media`.
    fn fixture(name: &str, media: FakeMedia) -> Fixture {
        let root = std::env::temp_dir().join(format!("bezel-video-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let posters = Arc::clone(&media.posters);
        let theme = Theme::blank("Wide", Size::new(480, 1920), Orientation::Landscape);
        let studio = Studio::new(
            Box::new(FakeSensors::demo()),
            Box::new(SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip)),
            Language::English,
            theme,
        );
        let backend = Backend {
            bus: Arc::new(FakeBus::turing_88()),
            connector: Arc::new(FakeConnector::default()),
            hid: Arc::new(bezel_devices::FakeHid::default()),
            store: Arc::new(FsThemeStore),
            library: ThemeLibrary::new(root.join("themes"), vec![]),
            settings: SettingsFile::new(root.join("settings.json")),
            system_language: Language::English,
            make_sensors: Arc::new(|_| Box::new(FakeSensors::demo())),
            udev: None,
            fonts: Vec::new(),
            studio: Session::new(studio),
            storage: StorageState::new(Box::new(media), root.join("scratch")),
            thumbnails: crate::thumbnails::tests::thumbnails(root.join("thumbnails")),
        };
        Fixture {
            backend,
            posters,
            root,
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(width, height, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn a_video_is_copied_under_a_safe_name_with_its_poster() {
        let f = fixture("add", FakeMedia::ready());
        let clip = f.local("Férias na Praia.MP4", &[7; 4096]);
        let added = f.backend.add_media(&clip).unwrap();
        assert_eq!(added.reference, "assets/f-rias-na-praia.mp4");
        assert_eq!(added.kind, "video");
        assert_eq!(added.bytes, 4096);
        assert_eq!(added.duration_ms, Some(2000));
        assert_eq!(
            added.poster.as_deref(),
            Some("assets/f-rias-na-praia-poster.png")
        );
        assert!(added.poster_error.is_none());
        // The poster covers the 1920x480 canvas, one second into the 2 s
        // clip; a 16:9 picture keeps its middle band.
        let (source, spec) = f.posters.lock().unwrap()[0].clone();
        assert_eq!(source.0, clip.display().to_string());
        assert_eq!(spec.size, Size::new(1920, 480));
        assert_eq!(spec.at, Duration::from_secs(1));
        assert_eq!(spec.crop, Some(Rect::new(0, 300, 1920, 480)));
        let studio = f.backend.studio();
        let assets = studio.assets();
        assert_eq!(assets[&AssetRef(added.reference.clone())], vec![7; 4096]);
        let poster = &assets[&AssetRef(added.poster.clone().unwrap())];
        let picture = image::load_from_memory(poster).unwrap().to_rgba8();
        assert_eq!((picture.width(), picture.height()), (1920, 480));
        let [r, g, b, a] = picture.get_pixel(0, 0).0;
        assert_eq!((r, g, b, a), (POSTER.r, POSTER.g, POSTER.b, POSTER.a));
    }

    #[test]
    fn without_ffmpeg_the_video_comes_without_a_poster() {
        let f = fixture("no-ffmpeg", FakeMedia::missing());
        let clip = f.local("clip.mp4", &[1; 100]);
        let added = f.backend.add_media(&clip).unwrap();
        assert_eq!(
            (added.kind, added.poster.as_deref()),
            ("video", None),
            "added all the same"
        );
        assert_eq!(
            added.duration_ms,
            Some(2000),
            "MP4 headers are read natively"
        );
        assert_eq!(added.poster_error.unwrap().code(), "unsupported");
        // A container only ffmpeg reads is taken at its word.
        let mkv = f.local("show.mkv", &[2; 100]);
        let added = f.backend.add_media(&mkv).unwrap();
        assert_eq!((added.kind, added.duration_ms), ("video", None));
        assert_eq!(added.poster_error.unwrap().code(), "unsupported");
        assert!(f.posters.lock().unwrap().is_empty());
        assert_eq!(f.backend.studio().assets().len(), 2);
    }

    #[test]
    fn animated_gifs_move_and_pictures_stay_pictures() {
        let f = fixture("gif", FakeMedia::ready());
        let animated = f.local("Ondas.gif", &crate::media::tests::gif(3));
        let added = f.backend.add_media(&animated).unwrap();
        assert_eq!(
            (added.kind, added.reference.as_str()),
            ("video", "assets/ondas.gif")
        );
        assert_eq!(added.poster.as_deref(), Some("assets/ondas-poster.png"));
        assert_eq!(
            f.posters.lock().unwrap()[0].1.at,
            Duration::ZERO,
            "its first picture"
        );
        let one = f.local("still-logo.gif", &crate::media::tests::gif(1));
        let added = f.backend.add_media(&one).unwrap();
        assert_eq!((added.kind, added.poster), ("image", None));
        let photo = f.local("Foto.PNG", &png(4, 4));
        let added = f.backend.add_media(&photo).unwrap();
        assert_eq!(
            (added.kind, added.reference.as_str()),
            ("image", "assets/foto.png")
        );
        assert_eq!(f.posters.lock().unwrap().len(), 1);
        // The listing tells the GIF moves and names each video's poster.
        let assets = f.backend.assets();
        let ondas = assets
            .iter()
            .find(|a| a.reference == "assets/ondas.gif")
            .unwrap();
        assert!(ondas.animated);
        assert_eq!(ondas.kind, "image", "still offered to image elements");
        assert_eq!(ondas.poster.as_deref(), Some("assets/ondas-poster.png"));
        assert_eq!(ondas.duration_ms, Some(2000));
        let logo = assets
            .iter()
            .find(|a| a.reference == "assets/still-logo.gif")
            .unwrap();
        assert!(!logo.animated);
    }

    #[test]
    fn what_is_not_a_video_or_a_picture_is_refused() {
        let f = fixture("refused", FakeMedia::ready());
        for (name, bytes) in [
            ("notes.txt", b"hello".as_slice()),
            ("song.mp3", b"ID3".as_slice()),
        ] {
            let err = f.backend.add_media(&f.local(name, bytes)).unwrap_err();
            assert_eq!(err.code(), "notMedia", "{name}");
            assert!(err.to_string().contains(name), "{err}");
        }
        // Probed as something that does not move.
        let odd = f.local("odd.avi", &[0; 10]);
        assert_eq!(f.backend.add_media(&odd).unwrap_err().code(), "notMedia");
        let gone = f.root.join("gone.mp4");
        assert_eq!(f.backend.add_media(&gone).unwrap_err().code(), "fileError");
        assert!(f.backend.studio().assets().is_empty());
    }

    #[test]
    fn a_video_background_lists_its_poster_and_renders_over_it() {
        let f = fixture("background", FakeMedia::ready());
        let added = f.backend.add_media(&f.local("clip.mov", &[3; 64])).unwrap();
        let mut theme = f.backend.session().theme;
        theme.background = bezel_themes::dto::BackgroundDto::Video {
            asset: added.reference.clone(),
            poster: added.poster.clone(),
        };
        let frame = f
            .backend
            .render(&theme, TIME, std::time::Instant::now())
            .unwrap();
        assert_eq!(
            &frame[12..16],
            &[POSTER.r, POSTER.g, POSTER.b, 255],
            "the poster"
        );
        let studio = f.backend.studio();
        assert!(matches!(
            &studio.theme().background,
            Background::Video {
                poster: Some(_),
                ..
            }
        ));
        drop(studio);
        let listed = f.backend.assets();
        let clip = listed
            .iter()
            .find(|a| a.reference == added.reference)
            .unwrap();
        assert_eq!((clip.kind, clip.bytes), ("video", 64));
        assert_eq!(clip.poster, added.poster);
        assert_eq!(clip.data_url, None, "videos are not decoded for the list");
        let poster = listed
            .iter()
            .find(|a| Some(&a.reference) == added.poster.as_ref());
        assert!(poster.unwrap().data_url.is_some(), "the poster's thumbnail");
    }
}
