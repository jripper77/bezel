//! Ports: the traits adapters implement (driven) or call (driving).

use crate::Result;
use crate::domain::animation::Timeline;
use crate::domain::archive::{Catalog, ContentId};
use crate::domain::clock::{Language, LocalTime};
use crate::domain::discovery::{DesktopModePanel, Endpoint, MonitorModeConfirmed, Screen};
use crate::domain::frame::Frame;
use crate::domain::geometry::Orientation;
use crate::domain::gifs::{Collection, GifPage, GifQuery, Rendition};
use crate::domain::history::Histories;
use crate::domain::job::Job;
use crate::domain::media::{MediaInfo, MediaTools, StreamSpec, TranscodeTarget};
use crate::domain::poster::PosterSpec;
use crate::domain::screen::{Brightness, ScreenIdentity};
use crate::domain::sensor::{Quantities, SensorInfo, Snapshot, Wanted};
use crate::domain::standby::PlanB;
use crate::domain::storage::{
    Confirmed, FileName, RemotePath, Repeat, StorageInfo, StorageLocation,
};
use crate::domain::theme::{AssetRef, Theme};
use std::collections::BTreeMap;
use std::time::Duration;

/// Driven port: enumerates the USB endpoints the host can see, without
/// opening or writing to any of them.
pub trait DeviceBus {
    /// Every candidate endpoint currently connected. Adapters may pre-filter to
    /// the catalog's USB ids; unknown endpoints are ignored by the core anyway.
    fn endpoints(&self) -> Result<Vec<Endpoint>>;
}

/// Driven port: the HID interface of a Turing USB panel in desktop mode
/// (`docs/reverse-engineering/protocol-turing-usb.md` section 10: 64-byte
/// reports, report id 0). Nothing reaches it implicitly: both methods take
/// the proof of a confirmed switch, which only `app::leave_desktop_mode`
/// obtains from `Confirm::Yes` (D-2026-09-30-release-polish-8). Adapters
/// refuse an address that no longer leads to a panel in desktop mode. Not
/// validated on hardware.
pub trait DesktopModeHid {
    /// Sends the model query and waits up to 1 s for one report: the model
    /// byte it carries, or `None` when the panel does not answer (the vendor
    /// app then assumes an 8.8"; Bezel does not).
    fn query_model(
        &self,
        panel: &DesktopModePanel,
        confirmed: &MonitorModeConfirmed,
    ) -> Result<Option<u8>>;

    /// Sends the two back-to-monitor-mode reports. The panel answers
    /// nothing; it re-enumerates as its Turing USB self.
    fn back_to_monitor(
        &self,
        panel: &DesktopModePanel,
        confirmed: MonitorModeConfirmed,
    ) -> Result<()>;
}

/// Driven port: opens a discovered screen (waking it when needed) and
/// performs the handshake.
pub trait ScreenConnector {
    /// A live link to `screen`.
    fn connect(&self, screen: &Screen) -> Result<Box<dyn ScreenLink>>;

    /// Restarts a hung screen without a USB replug and returns once it is
    /// back on the bus (a [`Screen::restartable`] rev C screen: its MCU
    /// restarts the SoC, about 10 s; D-2026-09-30-release-polish-13).
    /// Disruptive, never destructive: what the screen plays stops, what it
    /// stores stays. Nobody may hold the screen's link meanwhile. The
    /// default, for screens that cannot be restarted: `Unsupported`.
    fn restart(&self, screen: &Screen) -> Result<()> {
        Err(crate::BezelError::Unsupported(format!(
            "restarting a {} screen",
            screen.family.slug()
        )))
    }
}

/// Host-side measurements of the most recent frame transfer.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransferStats {
    /// Bytes accepted by the serial wire, including framing and status query.
    pub bytes: u64,
    /// Rotation and RGBA to BGRA conversion time.
    pub conversion_ms: f64,
    /// Pixel difference encoding time.
    pub diff_ms: f64,
    /// Serial writes including drain time.
    pub write_ms: f64,
    /// Waiting for frame/status replies.
    pub reply_ms: f64,
    /// Full, partial or unchanged/keepalive frame.
    pub kind: &'static str,
}

/// Driven port: one connected screen. Frames go in the orientation the user
/// looks at; the adapter rotates and encodes them for the panel and decides
/// between a full frame and a partial update.
pub trait ScreenLink: Send {
    /// Measurements when the adapter supports them; unavailable otherwise.
    fn last_transfer(&self) -> Option<TransferStats> {
        None
    }
    /// Who answered the handshake.
    fn identity(&self) -> &ScreenIdentity;
    /// Backlight level.
    fn set_brightness(&mut self, brightness: Brightness) -> Result<()>;
    /// Orientation of the frames that follow.
    fn set_orientation(&mut self, orientation: Orientation) -> Result<()>;
    /// Shows `frame`, whose size must be the panel size in the current
    /// orientation. On a screen playing a stored video, the frame's alpha is
    /// kept per pixel: A = 0 lets the video show through.
    fn present(&mut self, frame: &Frame) -> Result<()>;
    /// Turns the panel off. Some screens power down completely (a rev C
    /// SoC leaves the bus); the next connection wakes them.
    fn screen_off(&mut self) -> Result<()>;
    /// Turns the panel off at once, waiting for nothing: the `off` choice
    /// when the computer shuts down, within the shutdown's deadline
    /// (D-2026-10-03-power-off-standby-3; rev C: TURNOFF 0x83 alone, without
    /// waiting for the SoC to leave the bus). The default:
    /// [`Self::screen_off`].
    fn turn_off_now(&mut self) -> Result<()> {
        self.screen_off()
    }
    /// Hands the screen back to its standalone mode (clock, stored media).
    fn release(&mut self) -> Result<()>;
    /// The screen's stored files and device-side playback, over this same
    /// link. `None` (the default) for screens without storage; the storage
    /// use cases then answer `BezelError::Unsupported`.
    fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
        None
    }
}

/// Driven port: files stored on a screen (internal flash and memory card,
/// both reached only through the screen's protocol) and device-side playback.
///
/// Implemented by the device adapter of families with storage (rev C,
/// TUR_USB) and reached through [`ScreenLink::storage`]. Paths name one of
/// the four vendor folders plus a file name; the adapter maps them to its
/// family's device paths. Sizes are bytes. Nothing here is sent implicitly
/// (D-2026-09-30-device-protocols-2): every method runs only when a use case
/// in `app::storage` or the theme runtime calls it, and the methods that
/// destroy or persist take a [`Confirmed`] proof.
pub trait ScreenStorage {
    /// Capacity and use of the internal flash and of the card (`None` when
    /// no card is inserted; rev C: TF total <= 1024 KiB). Adapters convert
    /// the device's units to bytes and take any vendor reserve off `total`
    /// and `free` (rev C: 512 KiB of flash). A query.
    fn info(&mut self) -> Result<StorageInfo>;

    /// Names of the files in `location`'s folder, in the order the screen
    /// reports them; an empty folder gives an empty list. Names the screen
    /// reports that are not valid [`FileName`]s are skipped. Rev C and
    /// TUR_USB create the folder when it is missing (`nodir-createdone`),
    /// so callers list a card folder only when [`Self::info`] reports a card.
    fn list(&mut self, location: StorageLocation) -> Result<Vec<FileName>>;

    /// Size of a stored file in bytes; `None` when it is absent (the screens
    /// answer 0 for an absent file, so an empty file also reads as absent).
    /// `BezelError::Unsupported` means a file is stored at `path` but the
    /// screen cannot report its size (TUR_USB files Bezel did not write,
    /// D-2026-09-30-storage-video-7): the use cases take it as present with
    /// an unknown size. A query: never creates anything.
    fn size(&mut self, path: &RemotePath) -> Result<Option<u64>>;

    /// Writes `data` to `path`, creating the file or replacing it (callers
    /// have checked existence and confirmation; `app::storage::upload`
    /// needs `Confirm::Yes` to replace). Stops device-side playback first and
    /// follows the family's upload sequence (rev C: protocol section 13.4).
    ///
    /// Progress: [`crate::domain::job::JobPhase::Upload`] reports with
    /// `done` = bytes of `data` accepted so far and `total` = `data.len()`,
    /// at least at the start and after every block. Cancel: the adapter polls
    /// `job` between blocks; once cancelled it stops sending, puts the link
    /// back in a usable state (rev C: HELLO) and returns
    /// `BezelError::Cancelled { partial }` with the bytes a size query now
    /// finds at `path` (`None` when nothing is left). It never deletes the
    /// partial file itself. Does not verify: the use case does with
    /// [`Self::size`].
    fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()>;

    /// Deletes a stored file. Deleting an absent file is not an error.
    fn delete(&mut self, path: &RemotePath, confirmed: Confirmed) -> Result<()>;

    /// Stops whatever the screen plays and plays the stored video at `path`
    /// (rev C: PLAY_VIDEO, waiting for `play_video_success`). With
    /// [`Repeat::Loop`] it loops until [`Self::stop`]; frames presented
    /// meanwhile are drawn over it with their alpha. The firmware may also
    /// take it as the video of
    /// [`StartMode::Video`](crate::domain::storage::StartMode::Video).
    fn play_video(&mut self, path: &RemotePath, repeat: Repeat) -> Result<()>;

    /// Stops whatever the screen plays and shows the stored image at `path`
    /// (rev C: PLAY_IMAGE, waiting for `play_img_ok`). The firmware may also
    /// take it as the image of
    /// [`StartMode::Image`](crate::domain::storage::StartMode::Image).
    fn play_image(&mut self, path: &RemotePath) -> Result<()>;

    /// Stops device-side playback (video or image) and waits until the
    /// screen reports it stopped. Nothing is deleted.
    fn stop(&mut self) -> Result<()>;

    /// Persistently writes what the screen does on its own: its start mode
    /// after power-up and its sleep timer (rev C: OPTIONS 0x7D written
    /// whole, with the brightness this link last sent, `plan`'s start mode,
    /// no flip and `plan`'s timer; D-2026-10-03-power-off-standby-2 (3),
    /// (4)). Families that cannot: `Unsupported`, nothing sent.
    fn set_options(&mut self, plan: PlanB, confirmed: Confirmed) -> Result<()>;

    /// Restarts the screen's own system into its start mode, without
    /// waiting for it (rev C: RESTART 0x84; the `album` choice when the
    /// computer shuts down, D-2026-10-03-power-off-standby-3). Disruptive,
    /// never destructive: the link is gone afterwards. The default, for
    /// screens that cannot: `Unsupported`, nothing sent.
    fn restart(&mut self, confirmed: Confirmed) -> Result<()> {
        let _ = confirmed;
        Err(crate::BezelError::Unsupported(
            "restarting the screen's system".into(),
        ))
    }
}

/// Driven port: the local copies of what Bezel sends to screens, and their
/// catalog (D-2026-09-30-storage-manager-2, -5), kept in the user's data
/// folder by the disk adapter. Copies are addressed by content: the same
/// bytes are kept once, whatever screens and media hold them. The use cases
/// keep [`Catalog::copies`] in step with what the store holds.
pub trait ArchiveStore: Send {
    /// The catalog as last saved; an empty one with the default limit when
    /// none was saved yet. A catalog that cannot be read is an error naming
    /// it, never an empty catalog.
    fn load(&mut self) -> Result<Catalog>;

    /// Saves `catalog` atomically: if it fails, the previous one stays whole.
    fn save(&mut self, catalog: &Catalog) -> Result<()>;

    /// Keeps `bytes` as a local copy and returns their id, their SHA-256.
    /// Bytes already kept are stored once and give the same id.
    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId>;

    /// The bytes kept as `content`; `None` when no copy is kept.
    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>>;

    /// Removes the copy kept as `content` (a thumbnail of it stays).
    /// Removing a copy that is not kept is not an error.
    fn discard(&mut self, content: &ContentId) -> Result<()>;
}

/// Driven port: an online provider of GIFs and stickers, reached with the
/// user's own key (D-2026-10-01-gif-sticker-search-2). Nothing reaches it
/// unless a use case in `app::gifs` calls it on a user action. Shared by
/// the threads that run the user's requests, so its methods take `&self`.
///
/// Failures are [`BezelError::Service`](crate::BezelError::Service): the
/// key at its request limit is [`ServiceFailure::RateLimited`], a refused
/// key [`ServiceFailure::KeyRejected`], anything else
/// [`ServiceFailure::Unavailable`]. No error text, log line or `Debug`
/// output of an adapter carries the key.
///
/// [`ServiceFailure::RateLimited`]: crate::domain::error::ServiceFailure::RateLimited
/// [`ServiceFailure::KeyRejected`]: crate::domain::error::ServiceFailure::KeyRejected
/// [`ServiceFailure::Unavailable`]: crate::domain::error::ServiceFailure::Unavailable
pub trait GifSource: Send + Sync {
    /// The provider's name, recorded as the origin of what is collected
    /// from it.
    fn provider(&self) -> &str;

    /// One page of GIFs or stickers for `query` (the trending ones when its
    /// text is empty), [`PAGE_SIZE`](crate::domain::gifs::PAGE_SIZE) items
    /// at most, filtered as [`GifQuery::filter`] says. Adapters read the
    /// provider's answer tolerantly: unknown fields are ignored, renditions
    /// they cannot read are skipped and an item without a GIF is dropped.
    fn page(&self, query: &GifQuery) -> Result<GifPage>;

    /// The bytes of `rendition`, read up to `limit` bytes: a larger file
    /// is `BezelError::InvalidInput` naming the limit, and nothing past it
    /// is read. Files never carry the user's key.
    fn download(&self, rendition: &Rendition, limit: u64) -> Result<Vec<u8>>;
}

/// Driven port: the user's collection of GIFs and stickers
/// (D-2026-10-01-gif-sticker-search-5), kept in the user's data folder by
/// the disk adapter. Shaped like [`ArchiveStore`]: an index saved whole and
/// bytes addressed by content, so the same GIF is kept once; each kept GIF
/// may also have a small preview, under the same id.
pub trait GifCollection: Send {
    /// The index as last saved; an empty one when none was saved yet. An
    /// index that cannot be read is an error naming it, never an empty
    /// collection.
    fn load(&mut self) -> Result<Collection>;

    /// Saves `index` atomically: if it fails, the previous one stays whole.
    fn save(&mut self, index: &Collection) -> Result<()>;

    /// Keeps `bytes` (a GIF) and returns their id, their SHA-256. Bytes
    /// already kept are stored once and give the same id.
    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId>;

    /// The bytes kept as `content`; `None` when none are kept.
    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>>;

    /// Keeps `bytes` (a small GIF) as the preview of the GIF kept as
    /// `content`, replacing an earlier one.
    fn keep_preview(&mut self, content: &ContentId, bytes: &[u8]) -> Result<()>;

    /// The preview of the GIF kept as `content`; `None` when it has none.
    fn read_preview(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>>;

    /// Removes the bytes kept as `content` and their preview. Removing what
    /// is not kept is not an error.
    fn discard(&mut self, content: &ContentId) -> Result<()>;
}

/// Where a local media file lives for a [`MediaTranscoder`] (a path on the
/// host for disk-backed adapters). Opaque to the core.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaLocation(pub String);

/// Driven port: media files on the host. Inspects them, converts videos into
/// a screen's profile and decodes videos into frames for screens that cannot
/// play stored ones. Implemented around an external converter that is looked
/// up, never bundled (D-2026-09-30-storage-video-2); without it, inspection
/// of MP4 files and still images still works.
pub trait MediaTranscoder: Send {
    /// Whether the converter can be used right now, with install hints when
    /// it cannot. Cheap enough to call before every job.
    fn tools(&mut self) -> MediaTools;

    /// What `source` is. MP4 files (header parsed natively) and still images
    /// are inspected without the converter; other formats need it and fail
    /// with `BezelError::Unsupported` when it is missing.
    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo>;

    /// Converts `source` into `target` and returns where the output is (a
    /// file the adapter manages). Progress: [`crate::domain::job::JobPhase::Convert`]
    /// in milliseconds of media time written / the source's duration (0 when
    /// unknown). Cancel: the adapter polls `job`; once cancelled it stops the
    /// conversion, deletes the partial output and returns
    /// `BezelError::Cancelled { partial: None }`. Without the converter:
    /// `BezelError::Unsupported`.
    fn transcode(
        &mut self,
        source: &MediaLocation,
        target: &TranscodeTarget,
        job: &mut Job<'_>,
    ) -> Result<MediaLocation>;

    /// The bytes of a local media file (a source or a conversion output),
    /// for an upload. Uploads are at most the screen's per-file limit
    /// (`UploadProfile::max_upload_bytes`: 25 MiB on rev C, 120 MB at most).
    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>>;

    /// Decodes `source` (a video or an animated GIF) on the host into RGBA
    /// frames of exactly `spec.size` at `spec.fps` (adapters may cap it; the
    /// ffmpeg one at [`crate::domain::media::PREVIEW_FPS`]): the whole picture, never
    /// turned or cropped, scaled to that size ([`StreamSpec::raw`] keeps its
    /// shape). The caller frames each picture
    /// (D-2026-10-01-video-background-framing-3). Without the converter:
    /// `BezelError::Unsupported`.
    fn stream(&mut self, source: &MediaLocation, spec: StreamSpec) -> Result<Box<dyn VideoFrames>>;

    /// The poster of the video (or animated GIF) `source` for a theme: the
    /// picture shown `spec.at` after the start, turned, cropped, scaled and
    /// padded into `spec.size` as its framing says ([`PosterSpec::geometry`];
    /// no crop and no pad: scaled to cover `spec.size`). Without the
    /// converter, and for adapters that take no pictures:
    /// `BezelError::Unsupported`.
    fn poster(&mut self, source: &MediaLocation, spec: PosterSpec) -> Result<Frame> {
        let _ = (source, spec);
        Err(crate::BezelError::Unsupported(
            "taking a poster from a video".into(),
        ))
    }
}

/// Frames of a video decoded on the host, looping.
pub trait VideoFrames: Send {
    /// The frame to show `elapsed` after playback started. Past the end the
    /// video starts over; the caller owns the clock and the cadence.
    fn frame_at(&mut self, elapsed: Duration) -> Result<&Frame>;
}

/// Driven port: measures the machine. Adapters time their own samples (rates
/// are per second of real elapsed time between two `sample` calls).
pub trait SensorSource: Send {
    /// The sensors this machine offers right now.
    fn catalog(&mut self) -> Result<Vec<SensorInfo>>;
    /// Current readings of every sensor in the catalog.
    fn sample(&mut self) -> Result<Snapshot>;
    /// Which sensors the caller shows from now on, replacing what it said
    /// before (D-2026-09-30-release-polish-11). A source whose measuring
    /// reaches outside this machine (`net.ping` sends packets) measures that
    /// sensor only while it is wanted, and reads it as unavailable otherwise;
    /// until the first call nothing is wanted. Sources that measure
    /// everything anyway ignore it (the default).
    fn want(&mut self, wanted: &Wanted) {
        let _ = wanted;
    }
}

/// What a frame shows under the theme's elements when its background is a
/// video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backdrop<'a> {
    /// The theme's own background (for a video: its poster). The default;
    /// every other background ignores the backdrop.
    #[default]
    Poster,
    /// The screen plays the video itself: the frame is an overlay on a
    /// transparent base (A = 0 outside the elements).
    OnDevice,
    /// A frame of the video decoded on the host, drawn as the background
    /// (covering the canvas).
    Frame(&'a Frame),
}

/// Everything a frame depends on besides the theme.
#[derive(Debug, Clone, Copy)]
pub struct RenderContext<'a> {
    /// Current readings.
    pub snapshot: &'a Snapshot,
    /// Graph histories.
    pub histories: &'a Histories,
    /// What each sensor measures (units of sensor text).
    pub quantities: &'a Quantities,
    /// Local wall-clock time for clock elements.
    pub time: LocalTime,
    /// How long the theme has been showing: animated images show their
    /// frame of that moment (they loop; [`FrameRenderer::animation`]).
    pub animation: Duration,
    /// Language of day and month names.
    pub language: Language,
    /// What a video background shows.
    pub backdrop: Backdrop<'a>,
}

/// Native card drawing costs, separate from serial delivery.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    /// Face surfaces reused in the last render.
    pub face_hits: u32,
    /// Face surfaces freshly drawn in the last render.
    pub face_misses: u32,
    /// Time drawing fresh face contents, milliseconds.
    pub face_draw_ms: f64,
    /// Other card work: buffers, caching, projection and composition, milliseconds.
    pub face_compose_ms: f64,
}

/// Driven port: draws a theme into a frame of its canvas size.
pub trait FrameRenderer: Send {
    /// Renders `theme` with `assets` in `context`.
    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame>;

    /// Select transient state for a document when a renderer is shared.
    /// Font/image caches may be shared, but transition clocks must be isolated.
    fn set_scene(&mut self, _scene: u64) {}

    /// An opaque content revision, changed when assets/readings/theme change.
    /// Zero means no revision tracking: adapters must not reuse asset-dependent faces.
    fn set_content_revision(&mut self, _revision: u64) {}

    /// Drawing costs of the last frame, when available.
    fn render_stats(&self) -> RenderStats {
        RenderStats::default()
    }

    /// Next transition frame on the RenderContext animation clock.
    fn next_change(&self) -> Option<Duration> {
        None
    }
    /// Disable transient card motion for reduced-motion previews.
    fn set_motion(&mut self, _allowed: bool) {}

    /// The frame times of the image `asset` when it is an animation (an
    /// animated GIF): what [`RenderContext::animation`] picks its frame
    /// from. `None` for a still image, an asset that cannot be read, or a
    /// renderer that does not animate images (the default).
    fn animation(
        &mut self,
        asset: &AssetRef,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
    ) -> Option<Timeline> {
        let _ = (asset, assets);
        None
    }
}

/// Where a theme lives for a [`ThemeStore`] (a file or folder for disk stores).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThemeLocation(pub String);

/// Driven port: reads and writes themes with their assets.
pub trait ThemeStore {
    /// Loads a theme and its assets.
    fn load(&self, location: &ThemeLocation) -> Result<(Theme, BTreeMap<AssetRef, Vec<u8>>)>;
    /// Saves a theme and its assets.
    fn save(
        &self,
        location: &ThemeLocation,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
    ) -> Result<()>;
}
