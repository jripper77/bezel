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
//!
//! The catalog (D-2026-09-30-storage-manager-5): every upload (a file, the
//! theme's video), delete and boot media goes through the core's
//! `app::manager`, which records it with the exact bytes sent in the store
//! of local copies the storage manager ([`crate::manager`]) shows.
//!
//! The final state of a shutdown (D-2026-10-03-power-off-standby-3,
//! [`crate::power`]): from [`StorageState::enter_final_state`] on, no
//! operation gets the screens ([`StorageState::claim`] answers `busy`), the
//! running job is cancelled and the shutdown waits for it
//! ([`StorageState::wait_until_idle`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bezel_core::BezelError;
use bezel_core::app::manager::Manager;
use bezel_core::app::storage::{self, PreparedUpload, UploadRequest};
use bezel_core::domain::archive::{Catalog, ContentId, TransferPlan};
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
use bezel_core::ports::{ArchiveStore, MediaLocation, MediaTranscoder, ScreenLink};

use crate::backend::Backend;
use crate::clock::unix_seconds;
use crate::diag::{self, DiagCode};
use crate::dto::{
    ConversionDto, FolderDto, JobDto, MediaToolsDto, PrepareDto, PreparedDto, RefusalDto,
    StorageDto, StoredFileDto, media_summary,
};
use crate::manager::{Copies, PendingPlan, Pictures, Shown};
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
    /// Another converter looking for its tool where this one does: the
    /// thumbnails' own, so that they never wait for a running job.
    fn spare(&self) -> Box<dyn MediaSetup>;
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
        && std::fs::remove_file(file).is_err()
    {
        diag::report(DiagCode::VideoCopyNotRemoved);
    }
}

/// What the storage commands share: the media converter, the store of local
/// copies, the one running operation, its cancel token, and the upload or
/// plan waiting for confirmation.
pub struct StorageState {
    media: SharedMedia,
    /// The thumbnails' converter ([`MediaSetup::spare`]).
    picture_media: Mutex<Box<dyn MediaSetup>>,
    archive: Mutex<Box<dyn ArchiveStore>>,
    pictures: Box<dyn Pictures>,
    scratch: PathBuf,
    busy: AtomicBool,
    /// The computer is shutting down: no operation gets the screens.
    ending: AtomicBool,
    cancel: Mutex<Option<CancelToken>>,
    pending: Mutex<Option<Pending>>,
    plan: Mutex<Option<PendingPlan>>,
    shown: Mutex<Shown>,
    tickets: AtomicU64,
}

/// The catalog and the local copies, locked for each call only
/// ([`StorageState::archive_per_call`]): a use case that talks to a screen
/// between its reads and saves of the catalog holds it while it reads or
/// saves, never while the screen works (review W1 of iteration 3 of
/// power-off-standby).
pub(crate) struct ArchivePerCall<'a>(&'a Mutex<Box<dyn ArchiveStore>>);

impl ArchiveStore for ArchivePerCall<'_> {
    fn load(&mut self) -> bezel_core::Result<Catalog> {
        lock(self.0).load()
    }

    fn save(&mut self, catalog: &Catalog) -> bezel_core::Result<()> {
        lock(self.0).save(catalog)
    }

    fn keep(&mut self, bytes: &[u8]) -> bezel_core::Result<ContentId> {
        lock(self.0).keep(bytes)
    }

    fn read(&mut self, content: &ContentId) -> bezel_core::Result<Option<Vec<u8>>> {
        lock(self.0).read(content)
    }

    fn discard(&mut self, content: &ContentId) -> bezel_core::Result<()> {
        lock(self.0).discard(content)
    }
}

/// The claim on the screen of the running storage operation.
pub(crate) struct Claim<'a>(&'a AtomicBool);

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How often [`StorageState::wait_until_idle`] looks at the running job.
const IDLE_POLL: Duration = Duration::from_millis(10);

impl StorageState {
    /// Storage commands over `media`, recording what they send in
    /// `copies`; copies of theme videos to send go to `scratch`.
    pub fn new(media: Box<dyn MediaSetup>, copies: Copies, scratch: PathBuf) -> Self {
        Self {
            picture_media: Mutex::new(media.spare()),
            media: Arc::new(Mutex::new(media)),
            archive: Mutex::new(copies.store),
            pictures: copies.pictures,
            scratch,
            busy: AtomicBool::new(false),
            ending: AtomicBool::new(false),
            cancel: Mutex::new(None),
            pending: Mutex::new(None),
            plan: Mutex::new(None),
            shown: Mutex::default(),
            tickets: AtomicU64::new(0),
        }
    }

    /// The media converter, for the session to decode a theme's video on
    /// this computer (screens that cannot play videos).
    pub fn shared_media(&self) -> SharedMedia {
        Arc::clone(&self.media)
    }

    /// Holds the screens for one operation (a storage job, a restart,
    /// opening a screen): the others answer `busy` until it is dropped. In
    /// the final state of a shutdown every claim answers `busy`. The flag is
    /// read after the claim is taken, so a shutdown that set it either sees
    /// the claim ([`Self::wait_until_idle`]) or the claim sees the flag.
    pub(crate) fn claim(&self) -> UiResult<Claim<'_>> {
        let claim = self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Claim(&self.busy))
            .map_err(|_| UiError::new(ErrorCode::Busy))?;
        self.refuse_while_shutting_down()?;
        Ok(claim)
    }

    /// Whether a storage operation holds a screen.
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// `busy` while a storage operation holds a screen, or the computer is
    /// shutting down.
    pub fn ensure_idle(&self) -> UiResult<()> {
        if self.is_busy() {
            return Err(UiError::new(ErrorCode::Busy));
        }
        self.refuse_while_shutting_down()
    }

    /// `busy` in the final state of a shutdown.
    pub(crate) fn refuse_while_shutting_down(&self) -> UiResult<()> {
        if self.ending.load(Ordering::SeqCst) {
            return Err(UiError::new(ErrorCode::Busy));
        }
        Ok(())
    }

    /// The final state of a shutdown starts: from now on no claim is given
    /// and the running job is asked to stop.
    pub(crate) fn enter_final_state(&self) {
        self.ending.store(true, Ordering::SeqCst);
        self.cancel();
    }

    /// The shutdown was cancelled: claims are given again.
    pub(crate) fn leave_final_state(&self) {
        self.ending.store(false, Ordering::SeqCst);
    }

    /// Waits until no operation holds the screens, at `deadline` at the
    /// latest, asking the running job to stop meanwhile (one started just
    /// before the final state included); whether none holds them.
    pub(crate) fn wait_until_idle(&self, deadline: Instant) -> bool {
        loop {
            self.cancel();
            if !self.is_busy() {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            std::thread::sleep(IDLE_POLL.min(deadline - now));
        }
    }

    /// The media converter, waiting while a job uses it.
    pub(crate) fn media(&self) -> MutexGuard<'_, Box<dyn MediaSetup>> {
        lock(&self.media)
    }

    /// The thumbnails' converter.
    pub(crate) fn picture_media(&self) -> MutexGuard<'_, Box<dyn MediaSetup>> {
        lock(&self.picture_media)
    }

    /// The catalog and the local copies; only under the claim.
    pub(crate) fn archive(&self) -> MutexGuard<'_, Box<dyn ArchiveStore>> {
        lock(&self.archive)
    }

    /// The catalog and the local copies, each call locking them on its own:
    /// for the shutdown, whose screens may hang between its reads and saves
    /// ([`ArchivePerCall`]).
    pub(crate) fn archive_per_call(&self) -> ArchivePerCall<'_> {
        ArchivePerCall(&self.archive)
    }

    /// Where files written to be sent wait for their upload (`<cache>/sending`).
    pub(crate) fn scratch(&self) -> &Path {
        &self.scratch
    }

    /// The thumbnails of the local copies.
    pub(crate) fn pictures(&self) -> &dyn Pictures {
        self.pictures.as_ref()
    }

    /// What the last storage manager overview listed.
    pub(crate) fn shown(&self) -> MutexGuard<'_, Shown> {
        lock(&self.shown)
    }

    /// Registers the cancel token of the job that starts.
    pub(crate) fn start_job(&self) -> CancelToken {
        let token = CancelToken::new();
        *lock(&self.cancel) = Some(token.clone());
        token
    }

    /// The running job ended: Cancel has nothing to stop.
    pub(crate) fn end_job(&self) {
        *lock(&self.cancel) = None;
    }

    fn ticket(&self) -> u64 {
        self.tickets.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Keeps `plan` of `screen` for its confirmation (replacing any other)
    /// and names it.
    pub(crate) fn keep_plan(&self, screen: &str, plan: TransferPlan) -> u64 {
        let ticket = self.ticket();
        *lock(&self.plan) = Some(PendingPlan {
            ticket,
            screen: screen.to_string(),
            plan,
        });
        ticket
    }

    /// The plan `ticket` names, once; `stale` when it was run, replaced or
    /// dropped since.
    pub(crate) fn take_plan(&self, ticket: u64) -> UiResult<PendingPlan> {
        let mut plan = lock(&self.plan);
        match plan.take() {
            Some(p) if p.ticket == ticket => Ok(p),
            other => {
                *plan = other;
                Err(UiError::new(ErrorCode::Stale))
            }
        }
    }

    /// Drops the plan waiting for confirmation: a job that may change the
    /// screen is about to run, after which the plan no longer holds.
    pub(crate) fn forget_plan(&self) {
        *lock(&self.plan) = None;
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
        let ticket = self.ticket();
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

/// A screen path the UI sent (`internal/video/a.mp4`).
pub(crate) fn remote(path: &str) -> UiResult<RemotePath> {
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
            if self.show_now(time).is_err() {
                diag::report(DiagCode::LiveStoppedAfterStorageJob);
            }
        }
    }

    /// Runs `work` on `screen` (the claim already held).
    pub(crate) fn on_screen<R>(
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

    /// Runs `work` on `screen` as the one storage operation. One that may
    /// change the screen (`Resume::Video`) drops the plan waiting for
    /// confirmation.
    fn with_screen<T>(
        &self,
        screen: &str,
        resume: Resume,
        time: LocalTime,
        work: impl FnOnce(&mut dyn ScreenLink) -> bezel_core::Result<T>,
    ) -> UiResult<T> {
        let _claim = self.storage.claim()?;
        if resume == Resume::Video {
            self.storage.forget_plan();
        }
        self.on_screen(screen, resume, time, work).and_then(flat)
    }

    /// Playing or stopping files on a live screen would be hidden by the
    /// theme's frames, or stop its video: refused, by either port of the
    /// live screen.
    fn refuse_while_live(&self, screen: &str) -> UiResult<()> {
        if self.studio().is_live(screen) {
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
            self.storage
                .picture_media()
                .set_tool_path(Some(path.to_path_buf()));
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
    /// converted as the runtime says (`MissingVideo::options`: the theme's
    /// framing on the panel, D-2026-10-01-video-background-framing-3). A
    /// video already in the screen's profile whose framing leaves it as it
    /// is (a panel-native video in a turned theme, like the Dragon Ball's)
    /// is sent as it is under the same name, within the screen's size cap;
    /// any other framing needs ffmpeg, and is refused without it.
    pub fn prepare_theme_video(&self, screen: &str, time: LocalTime) -> UiResult<PrepareDto> {
        let (missing, bytes) = {
            let studio = self.studio();
            let missing = studio
                .missing_video(screen)
                .ok_or_else(|| UiError::new(ErrorCode::NoVideo))?;
            let bytes = studio.assets().get(&missing.asset).cloned();
            let bytes = bytes.ok_or_else(|| {
                UiError::new(ErrorCode::VideoNotInTheme).arg("asset", &missing.asset.0)
            })?;
            (missing, bytes)
        };
        let file = self.storage.write_scratch(&missing.asset, &bytes)?;
        let location = MediaLocation(file.display().to_string());
        let source = file_name(&file);
        self.prepare(screen, time, source, Some(file), move |_, _| {
            Ok(missing.upload_request(location))
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
        self.storage.forget_plan();
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
    /// through [`Self::cancel_job`], recorded in the catalog with the exact
    /// bytes sent (D-2026-09-30-storage-manager-5).
    fn upload_pending(
        &self,
        pending: &Pending,
        confirm: Confirm,
        time: LocalTime,
        progress: &mut dyn FnMut(Progress),
    ) -> UiResult<bezel_core::Result<storage::Uploaded>> {
        let token = self.storage.start_job();
        let sent_at = unix_seconds();
        let result = self.on_screen(&pending.screen, Resume::Video, time, |link| {
            let mut store = self.storage.archive();
            let mut media = self.storage.media();
            let mut job = Job::new(&token, progress);
            Manager::new(link, store.as_mut()).upload(
                media.as_mut(),
                &pending.prepared,
                confirm,
                sent_at,
                &mut job,
            )
        });
        self.storage.end_job();
        result
    }

    /// Asks the running upload to stop; `false` when none runs.
    pub fn cancel_job(&self) -> bool {
        self.storage.cancel()
    }

    // --------------------------------------------------- files and boot --

    /// Deletes a stored file and marks its catalog entry deleted; `confirm`
    /// is the user's answer to the dialog naming it.
    pub fn delete_stored(
        &self,
        screen: &str,
        path: &str,
        confirm: Confirm,
        time: LocalTime,
    ) -> UiResult<()> {
        let path = remote(path)?;
        self.with_screen(screen, Resume::Video, time, |link| {
            let mut store = self.storage.archive();
            Manager::new(link, store.as_mut()).delete(&path, confirm)
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
            let mut store = self.storage.archive();
            Manager::new(link, store.as_mut()).set_boot_media(&boot, brightness, confirm)
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
