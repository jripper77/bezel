//! The storage tab of the "Tela" panel (D-2026-09-30-storage-video-6): what
//! a screen stores, uploads with progress and cancel, deletes, playback, the
//! boot media and sending the theme's video, over the core's `app::storage`
//! use cases and the theme runtime's video decision.
//!
//! Screen access: one storage operation runs at a time ([`StorageState`]).
//! When the screen is live, the operation borrows the live link from the
//! session: frames pause, and the session lock is not held while the screen
//! works, so previews keep rendering and the refresh loop keeps sampling. The
//! link goes back when the operation ends: a full frame follows at once, and
//! after an operation that may change what the screen plays (upload, delete,
//! boot media) the theme's video is started again. When the screen is not
//! live, the operation opens it and closes it after. Meanwhile turning live
//! on, releasing the screen or setting its brightness answer `busy`: the
//! screen's port has one owner.
//!
//! Confirmation: deleting, replacing a file and changing the boot media take
//! the user's [`Confirm`], which only the Tauri commands make (from the
//! answer of the UI's confirmation dialog, which names the file); the core
//! refuses `Confirm::No` before any byte is sent.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bezel_core::BezelError;
use bezel_core::app::storage::{self, PreparedUpload, UploadRequest};
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::{CancelToken, Job, Progress};
use bezel_core::domain::media::{
    ConvertOptions, MediaInfo, MediaKind, UploadProfile, fitting_options,
};
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::storage::{
    BootMedia, Medium, RemotePath, Repeat, StorageLocation, UploadAction,
};
use bezel_core::domain::theme::AssetRef;
use bezel_core::ports::{MediaLocation, MediaTranscoder, ScreenLink};

use crate::backend::Backend;
use crate::dto::{
    ConversionDto, FolderDto, JobDto, MediaToolsDto, PrepareDto, PreparedDto, RefusalDto,
    StorageDto, StoredFileDto, media_summary,
};
use crate::messages::{ErrorCode, UiError, UiResult};
use crate::studio::{Resume, SharedMedia};

/// The media converter as the studio drives it: the core's port, plus where
/// its external tool is, which the settings' Locate button changes. The
/// composition root implements it for `bezel_media::FfmpegTranscoder`.
pub trait MediaSetup: MediaTranscoder {
    /// Looks for the tool at `path` first (the program or its folder), then
    /// on `PATH`.
    fn set_tool_path(&mut self, path: Option<PathBuf>);
    /// The tool in use, when a usable one exists.
    fn tool_in_use(&mut self) -> Option<PathBuf>;
}

/// An upload that passed its preflight, waiting for the user's confirmation.
struct Pending {
    ticket: u64,
    screen: String,
    prepared: PreparedUpload,
    /// A copy of the theme's video written for the upload, removed after it.
    scratch: Option<PathBuf>,
}

impl Pending {
    fn discard(self) {
        remove_scratch(self.scratch.as_deref());
    }
}

fn remove_scratch(file: Option<&Path>) {
    if let Some(file) = file
        && let Err(e) = std::fs::remove_file(file)
    {
        tracing::warn!(file = %file.display(), "copy of the theme video not removed: {e}");
    }
}

/// What the storage commands share: the media converter, the one running
/// operation, its cancel token and the upload waiting for confirmation.
pub struct StorageState {
    media: SharedMedia,
    scratch: PathBuf,
    busy: AtomicBool,
    cancel: Mutex<Option<CancelToken>>,
    pending: Mutex<Option<Pending>>,
    tickets: AtomicU64,
}

/// The claim on the screen of the running storage operation.
struct Claim<'a>(&'a AtomicBool);

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl StorageState {
    /// Storage commands over `media`; copies of theme videos go to `scratch`.
    pub fn new(media: Box<dyn MediaSetup>, scratch: PathBuf) -> Self {
        Self {
            media: Arc::new(Mutex::new(media)),
            scratch,
            busy: AtomicBool::new(false),
            cancel: Mutex::new(None),
            pending: Mutex::new(None),
            tickets: AtomicU64::new(0),
        }
    }

    /// The media converter, for the session to decode a theme's video on
    /// this computer (screens that cannot play videos).
    pub fn shared_media(&self) -> SharedMedia {
        Arc::clone(&self.media)
    }

    fn claim(&self) -> UiResult<Claim<'_>> {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Claim(&self.busy))
            .map_err(|_| UiError::new(ErrorCode::Busy))
    }

    /// Whether a storage operation holds a screen.
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// `busy` while a storage operation holds a screen.
    pub fn ensure_idle(&self) -> UiResult<()> {
        if self.is_busy() {
            return Err(UiError::new(ErrorCode::Busy));
        }
        Ok(())
    }

    fn media(&self) -> MutexGuard<'_, Box<dyn MediaSetup>> {
        lock(&self.media)
    }

    /// Asks the running upload to stop; `false` when none runs.
    pub fn cancel(&self) -> bool {
        match lock(&self.cancel).as_ref() {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn keep(&self, screen: &str, prepared: PreparedUpload, scratch: Option<PathBuf>) -> u64 {
        let ticket = self.tickets.fetch_add(1, Ordering::SeqCst) + 1;
        let previous = lock(&self.pending).replace(Pending {
            ticket,
            screen: screen.to_string(),
            prepared,
            scratch,
        });
        if let Some(previous) = previous {
            previous.discard();
        }
        ticket
    }

    fn take(&self, ticket: u64) -> UiResult<Pending> {
        let mut pending = lock(&self.pending);
        match pending.take() {
            Some(p) if p.ticket == ticket => Ok(p),
            other => {
                *pending = other;
                Err(UiError::new(ErrorCode::Stale))
            }
        }
    }

    /// Writes the theme's video `asset` where the converter can read it.
    fn write_scratch(&self, asset: &AssetRef, bytes: &[u8]) -> UiResult<PathBuf> {
        let name = asset.0.rsplit(['/', '\\']).next().unwrap_or("video");
        let file = self.scratch.join(name);
        std::fs::create_dir_all(&self.scratch)
            .and_then(|()| std::fs::write(&file, bytes))
            .map_err(|e| UiError::file(file.display(), e))?;
        Ok(file)
    }
}

/// How an operation reaches the screen.
enum Access {
    /// The live link, borrowed from the session.
    Live(Box<dyn ScreenLink>),
    /// A link opened for the operation.
    Own(Box<dyn ScreenLink>),
}

impl Access {
    fn link(&mut self) -> &mut dyn ScreenLink {
        match self {
            Access::Live(link) | Access::Own(link) => link.as_mut(),
        }
    }
}

fn remote(path: &str) -> UiResult<RemotePath> {
    Ok(RemotePath::parse(path)?)
}

fn flat<T>(result: bezel_core::Result<T>) -> UiResult<T> {
    Ok(result?)
}

/// The adjustments of a video that must be converted anyway: it stands like
/// the edited theme in `orientation` on the screen (the core's
/// `fitting_options`: turned to the panel and cropped to cover it). A video
/// already in the screen's profile, and every image, goes as it is.
fn upload_options(
    model: &DeviceModel,
    orientation: Orientation,
    media: &MediaInfo,
) -> ConvertOptions {
    let in_profile = UploadProfile::for_model(model)
        .is_some_and(|profile| profile.mismatches(MediaKind::Video, media).is_empty());
    if in_profile {
        return ConvertOptions::default();
    }
    fitting_options(model, orientation, media)
}

/// Capacity and every folder's files; a folder that cannot be listed says
/// why and the others are still listed.
fn overview(link: &mut dyn ScreenLink) -> bezel_core::Result<StorageDto> {
    let info = storage::info(link)?;
    let media: &[Medium] = if info.card.is_some() {
        &Medium::ALL
    } else {
        &[Medium::Internal]
    };
    let mut folders = Vec::new();
    for medium in media {
        for kind in MediaKind::ALL {
            let (files, error) = match storage::list(link, StorageLocation::new(*medium, kind)) {
                Ok(entries) => (entries.iter().map(StoredFileDto::from).collect(), None),
                Err(e) => (Vec::new(), Some(UiError::from(e))),
            };
            folders.push(FolderDto {
                medium: medium.slug(),
                kind: kind.slug(),
                files,
                error,
            });
        }
    }
    Ok(StorageDto {
        internal: info.internal.into(),
        card: info.card.map(Into::into),
        folders,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn prepared_dto(ticket: u64, source: String, prepared: &PreparedUpload) -> PreparedDto {
    let plan = &prepared.plan;
    let (format, dimensions) = media_summary(&prepared.media);
    PreparedDto {
        ticket,
        source,
        target: StoredFileDto::at(&plan.path, None),
        bytes: prepared.media.bytes,
        format,
        dimensions,
        convert: match &plan.action {
            UploadAction::AsIs { .. } => None,
            UploadAction::Convert(target) => Some(ConversionDto {
                width: target.size.width,
                height: target.size.height,
                quarter_turns: target.quarter_turns,
                cropped: target.crop.is_some(),
            }),
        },
        replaces: plan.replaces.as_ref().map(StoredFileDto::from),
    }
}

impl Backend {
    /// Borrows the live link of `screen`, or opens the screen.
    fn acquire(&self, screen: &str) -> UiResult<Access> {
        let lent = self.idle_studio().lend_live_link(screen)?;
        match lent {
            Some(link) => Ok(Access::Live(link)),
            None => self.connect(screen).map(Access::Own),
        }
    }

    /// Gives a borrowed live link back with a frame now (an opened link is
    /// closed).
    fn give_back(&self, screen: &str, access: Access, resume: Resume, time: LocalTime) {
        if let Access::Live(link) = access {
            let unwanted = self.studio().return_live_link(screen, link, resume);
            drop(unwanted);
            if let Err(e) = self.show_now(time) {
                tracing::warn!(screen, "live screen stopped after a storage job: {e}");
            }
        }
    }

    /// Runs `work` on `screen` (the claim already held).
    fn on_screen<R>(
        &self,
        screen: &str,
        resume: Resume,
        time: LocalTime,
        work: impl FnOnce(&mut dyn ScreenLink) -> R,
    ) -> UiResult<R> {
        let mut access = self.acquire(screen)?;
        let result = work(access.link());
        self.give_back(screen, access, resume, time);
        Ok(result)
    }

    /// Runs `work` on `screen` as the one storage operation.
    fn with_screen<T>(
        &self,
        screen: &str,
        resume: Resume,
        time: LocalTime,
        work: impl FnOnce(&mut dyn ScreenLink) -> bezel_core::Result<T>,
    ) -> UiResult<T> {
        let _claim = self.storage.claim()?;
        self.on_screen(screen, resume, time, work).and_then(flat)
    }

    /// Playing or stopping files on a live screen would be hidden by the
    /// theme's frames, or stop its video: refused.
    fn refuse_while_live(&self, screen: &str) -> UiResult<()> {
        if self.studio().live_key() == Some(screen) {
            return Err(UiError::new(ErrorCode::Live));
        }
        Ok(())
    }

    // ----------------------------------------------------------- queries --

    /// Capacity and files of `screen`.
    pub fn storage_overview(&self, screen: &str, time: LocalTime) -> UiResult<StorageDto> {
        self.with_screen(screen, Resume::Frames, time, overview)
    }

    /// Whether ffmpeg can convert videos.
    pub fn media_tools(&self) -> MediaToolsDto {
        let tools = self.storage.media().tools();
        MediaToolsDto::of(&tools, self.settings.load().ffmpeg_path)
    }

    /// Uses the ffmpeg at `path` (the program or its folder) from now on and
    /// remembers it, when it is usable; otherwise nothing changes and the
    /// answer names it as rejected.
    pub fn locate_ffmpeg(&self, path: &Path) -> MediaToolsDto {
        let previous = self.settings.load().ffmpeg_path.map(PathBuf::from);
        let (tools, accepted) = {
            let mut media = self.storage.media();
            media.set_tool_path(Some(path.to_path_buf()));
            let in_use = media.tool_in_use();
            let accepted = in_use
                .as_deref()
                .is_some_and(|used| used == path || used.parent() == Some(path));
            if !accepted {
                media.set_tool_path(previous);
            }
            (media.tools(), accepted)
        };
        if accepted {
            let chosen = path.display().to_string();
            self.settings.update(|s| s.ffmpeg_path = Some(chosen));
            return MediaToolsDto::of(&tools, self.settings.load().ffmpeg_path);
        }
        MediaToolsDto {
            rejected: Some(path.display().to_string()),
            ..MediaToolsDto::of(&tools, self.settings.load().ffmpeg_path)
        }
    }

    // ----------------------------------------------------------- uploads --

    /// The preflight of `request_for`'s upload: the summary to confirm, or
    /// the refusal to explain. Only queries reach the screen.
    fn prepare(
        &self,
        screen: &str,
        time: LocalTime,
        source: String,
        scratch: Option<PathBuf>,
        request_for: impl FnOnce(
            &mut dyn ScreenLink,
            &mut dyn MediaTranscoder,
        ) -> bezel_core::Result<UploadRequest>,
    ) -> UiResult<PrepareDto> {
        let checked = self.with_screen(screen, Resume::Frames, time, |link| {
            let mut media = self.storage.media();
            let media: &mut dyn MediaTranscoder = media.as_mut();
            let request = request_for(link, media)?;
            Ok(storage::prepare_upload(link, media, &request))
        });
        match checked {
            Ok(Ok(prepared)) => {
                let ticket = self.storage.keep(screen, prepared.clone(), scratch);
                Ok(PrepareDto::Ready(prepared_dto(ticket, source, &prepared)))
            }
            Ok(Err(BezelError::Refused(refusal))) => {
                remove_scratch(scratch.as_deref());
                Ok(PrepareDto::Refused(RefusalDto::from(&refusal)))
            }
            Ok(Err(e)) => {
                remove_scratch(scratch.as_deref());
                Err(e.into())
            }
            Err(e) => {
                remove_scratch(scratch.as_deref());
                Err(e)
            }
        }
    }

    /// Prepares sending the local file `source` to `medium` (`internal` or
    /// `sd`) of `screen`: the folder follows the file (image or video), the
    /// name is suggested from it, and a video that needs a conversion is
    /// turned and cropped to stand like the edited theme.
    pub fn prepare_upload(
        &self,
        screen: &str,
        source: &Path,
        medium: &str,
        time: LocalTime,
    ) -> UiResult<PrepareDto> {
        let medium = Medium::from_slug(medium)
            .ok_or_else(|| UiError::new(ErrorCode::UnknownMedium).arg("medium", medium))?;
        let orientation = self.studio().theme().orientation;
        let host_name = file_name(source);
        let location = MediaLocation(source.display().to_string());
        let name = host_name.clone();
        self.prepare(screen, time, host_name, None, move |link, media| {
            let probed = media.probe(&location)?;
            let kind = probed.kind().unwrap_or(MediaKind::Image);
            let suggested = storage::suggest_name(link, &name, &probed)?;
            Ok(UploadRequest {
                options: upload_options(link.identity().model, orientation, &probed),
                name: suggested.map_or(name, |n| n.to_string()),
                location: StorageLocation::new(medium, kind),
                source: location,
            })
        })
    }

    /// Prepares "Send to screen" for the live screen missing the theme's
    /// video (D-2026-09-30-storage-video-4): where the runtime looks for it,
    /// turned to the panel and cropped to cover it.
    pub fn prepare_theme_video(&self, screen: &str, time: LocalTime) -> UiResult<PrepareDto> {
        let (missing, bytes, orientation) = {
            let studio = self.studio();
            let missing = studio
                .missing_video(screen)
                .ok_or_else(|| UiError::new(ErrorCode::NoVideo))?;
            let bytes = studio.assets().get(&missing.asset).cloned();
            let bytes = bytes.ok_or_else(|| {
                UiError::new(ErrorCode::VideoNotInTheme).arg("asset", &missing.asset.0)
            })?;
            (missing, bytes, studio.theme().orientation)
        };
        let file = self.storage.write_scratch(&missing.asset, &bytes)?;
        let location = MediaLocation(file.display().to_string());
        let source = file_name(&file);
        self.prepare(screen, time, source, Some(file), move |link, media| {
            let mut request = missing.upload_request(location);
            let probed = media.probe(&request.source)?;
            // The runtime's turns for this theme and screen, and the crop.
            request.options = fitting_options(link.identity().model, orientation, &probed);
            Ok(request)
        })
    }

    /// Runs the prepared upload `ticket`: converts, sends and verifies,
    /// reporting to `progress`. `overwrite` is the user's answer about the
    /// replaced file the summary named. A cancel is an answer, not an error:
    /// it says what the interrupted upload left on the screen; so is a
    /// conversion whose output the screen would not take (refused before a
    /// byte is sent).
    pub fn run_upload(
        &self,
        ticket: u64,
        overwrite: Confirm,
        time: LocalTime,
        progress: &mut dyn FnMut(Progress),
    ) -> UiResult<JobDto> {
        let _claim = self.storage.claim()?;
        let pending = self.storage.take(ticket)?;
        let path = pending.prepared.plan.path.to_string();
        let result = self.upload_pending(&pending, overwrite, time, progress);
        pending.discard();
        match result? {
            Ok(done) => Ok(JobDto::Done {
                file: StoredFileDto::at(&done.path, Some(done.bytes)),
                converted: done.converted,
            }),
            Err(BezelError::Cancelled { partial }) => Ok(JobDto::Cancelled { path, partial }),
            Err(BezelError::Refused(refusal)) => Ok(JobDto::Refused(RefusalDto::from(&refusal))),
            Err(e) => Err(e.into()),
        }
    }

    /// Runs `pending` on its screen (the claim already held), cancellable
    /// through [`Self::cancel_job`].
    fn upload_pending(
        &self,
        pending: &Pending,
        confirm: Confirm,
        time: LocalTime,
        progress: &mut dyn FnMut(Progress),
    ) -> UiResult<bezel_core::Result<storage::Uploaded>> {
        let token = CancelToken::new();
        *lock(&self.storage.cancel) = Some(token.clone());
        let result = self.on_screen(&pending.screen, Resume::Video, time, |link| {
            let mut media = self.storage.media();
            let mut job = Job::new(&token, progress);
            storage::upload(link, media.as_mut(), &pending.prepared, confirm, &mut job)
        });
        *lock(&self.storage.cancel) = None;
        result
    }

    /// Asks the running upload to stop; `false` when none runs.
    pub fn cancel_job(&self) -> bool {
        self.storage.cancel()
    }

    // --------------------------------------------------- files and boot --

    /// Deletes a stored file; `confirm` is the user's answer to the dialog
    /// naming it.
    pub fn delete_stored(
        &self,
        screen: &str,
        path: &str,
        confirm: Confirm,
        time: LocalTime,
    ) -> UiResult<()> {
        let path = remote(path)?;
        self.with_screen(screen, Resume::Video, time, |link| {
            storage::delete(link, &path, confirm)
        })
    }

    /// Plays a stored file on a screen that is not live (videos loop).
    pub fn play_stored(&self, screen: &str, path: &str, time: LocalTime) -> UiResult<()> {
        self.refuse_while_live(screen)?;
        let path = remote(path)?;
        self.with_screen(screen, Resume::Frames, time, |link| {
            storage::play(link, &path, Repeat::Loop)
        })
    }

    /// Stops what a screen that is not live plays.
    pub fn stop_playback(&self, screen: &str, time: LocalTime) -> UiResult<()> {
        self.refuse_while_live(screen)?;
        self.with_screen(screen, Resume::Frames, time, storage::stop)
    }

    /// Sets what the screen shows on its own after power-up: `path`, or the
    /// built-in screen for `None`, and the brightness it starts with:
    /// `brightness` (percent), the level the user set in this session, is
    /// sent first; `None` leaves the link's (the screen's default on a link
    /// opened for this). `confirm` is the user's answer to the dialog naming
    /// both (the choice persists on the screen).
    pub fn set_boot_media(
        &self,
        screen: &str,
        path: Option<&str>,
        brightness: Option<u8>,
        confirm: Confirm,
        time: LocalTime,
    ) -> UiResult<()> {
        let boot = match path {
            Some(path) => BootMedia::File(remote(path)?),
            None => BootMedia::Default,
        };
        let brightness = brightness
            .map(|percent| {
                Brightness::new(percent).ok_or_else(|| UiError::new(ErrorCode::BrightnessRange))
            })
            .transpose()?;
        self.with_screen(screen, Resume::Video, time, |link| {
            storage::set_boot_media(link, &boot, brightness, confirm)
        })
    }
}

/// Lets through the progress reports worth an event: the first of each
/// phase, the last, and one per half percent (every report when the total is
/// unknown).
#[derive(Debug, Default)]
pub struct ProgressThrottle {
    last: Option<Progress>,
}

/// Reports per phase at most (plus the first and the last).
const PROGRESS_STEPS: u64 = 200;

impl ProgressThrottle {
    /// Whether `progress` is worth showing.
    pub fn pass(&mut self, progress: Progress) -> bool {
        let show = match self.last {
            None => true,
            Some(last) if last.phase != progress.phase => true,
            Some(_) if progress.total == 0 || progress.done >= progress.total => true,
            Some(last) => {
                progress.done.saturating_sub(last.done) * PROGRESS_STEPS >= progress.total
            }
        };
        if show {
            self.last = Some(progress);
        }
        show
    }
}

#[cfg(test)]
pub(crate) mod tests;
