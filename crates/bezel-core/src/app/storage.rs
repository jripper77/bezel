//! Storage use cases: what a screen stores, uploads (preflight, conversion,
//! sending, verification), deletes, device-side playback and the boot media.
//!
//! Every destructive or persistent step needs `Confirm::Yes`, checked here
//! before the port is called (D-2026-09-30-storage-video-1). Screens without
//! storage answer `BezelError::Unsupported`.

use crate::domain::job::{Job, JobPhase, Progress};
use crate::domain::media::{ConvertOptions, Converter, MediaInfo, MediaKind, UploadProfile};
use crate::domain::screen::{Brightness, Confirm};
use crate::domain::storage::{
    BootMedia, Confirmed, FileEntry, FileName, Medium, Operation, Refusal, RemotePath, Repeat,
    StorageInfo, StorageLocation, UploadAction, UploadCheck, UploadPlan, preflight,
};
use crate::ports::{MediaLocation, MediaTranscoder, ScreenLink, ScreenStorage};
use crate::{BezelError, Result};

/// The storage of the screen behind `link`, or `Unsupported`.
pub fn storage_of(link: &mut dyn ScreenLink) -> Result<&mut dyn ScreenStorage> {
    let name = link.identity().model.name;
    link.storage()
        .ok_or_else(|| BezelError::Unsupported(format!("{name} has no storage")))
}

fn profile_of(link: &dyn ScreenLink) -> Result<UploadProfile> {
    let model = link.identity().model;
    UploadProfile::for_model(model)
        .ok_or_else(|| BezelError::Unsupported(format!("{} stores no media", model.name)))
}

/// What a size query found at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    /// Nothing is stored there.
    Absent,
    /// A file is stored there: its size in bytes, when the screen can tell.
    Stored(Option<u64>),
}

impl Presence {
    /// Reads the answer of [`ScreenStorage::size`]: `Unsupported` means a
    /// file is there whose size the screen cannot report (TUR_USB files
    /// Bezel did not write, D-2026-09-30-storage-video-7).
    pub(crate) fn from_size(answer: Result<Option<u64>>) -> Result<Self> {
        match answer {
            Ok(Some(bytes)) => Ok(Self::Stored(Some(bytes))),
            Ok(None) => Ok(Self::Absent),
            Err(BezelError::Unsupported(_)) => Ok(Self::Stored(None)),
            Err(e) => Err(e),
        }
    }

    /// The size of a stored file, when known.
    pub(crate) fn size(self) -> Option<u64> {
        match self {
            Self::Stored(size) => size,
            Self::Absent => None,
        }
    }
}

/// Asks the screen what is stored at `path` (a size query).
pub(crate) fn presence(storage: &mut dyn ScreenStorage, path: &RemotePath) -> Result<Presence> {
    Presence::from_size(storage.size(path))
}

/// Capacity and use of the screen's internal flash and memory card.
pub fn info(link: &mut dyn ScreenLink) -> Result<StorageInfo> {
    storage_of(link)?.info()
}

/// The files in `location` with their sizes (one size query per file; a
/// file whose size the screen cannot report is listed with none). A card
/// folder is listed only when a card is present (`Refused(NoCard)`
/// otherwise), because listing creates the folder.
pub fn list(link: &mut dyn ScreenLink, location: StorageLocation) -> Result<Vec<FileEntry>> {
    let storage = storage_of(link)?;
    if location.medium == Medium::Card && storage.info()?.card.is_none() {
        return Err(BezelError::Refused(Refusal::NoCard));
    }
    let names = storage.list(location)?;
    names
        .into_iter()
        .map(|name| {
            let path = RemotePath::new(location, name);
            let size = presence(storage, &path)?.size();
            Ok(FileEntry { path, size })
        })
        .collect()
}

/// A local file to put on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadRequest {
    /// The local file.
    pub source: MediaLocation,
    /// Name on the screen as typed or suggested
    /// ([`UploadProfile::suggest_name`]); normalized by the preflight.
    pub name: String,
    /// Target folder.
    pub location: StorageLocation,
    /// Adjustments for a video (any of them means a conversion).
    pub options: ConvertOptions,
}

/// An upload that passed its preflight: what the confirmation dialog shows
/// (file, folder, size, conversion, replaced file) and what [`upload`] runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUpload {
    /// The local file.
    pub source: MediaLocation,
    /// The local file as probed.
    pub media: MediaInfo,
    /// Target, conversion and replaced file.
    pub plan: UploadPlan,
}

/// What an upload stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    /// Where.
    pub path: RemotePath,
    /// Size stored and verified, in bytes.
    pub bytes: u64,
    /// True when the file was converted first.
    pub converted: bool,
}

/// The preflight of `request`: probes the file, reads the storage info and
/// the target medium's listings. Only queries: nothing is converted, written
/// or deleted. Refusals come as `BezelError::Refused` (a full medium lists
/// its files with sizes as delete candidates).
pub fn prepare_upload(
    link: &mut dyn ScreenLink,
    media: &mut dyn MediaTranscoder,
    request: &UploadRequest,
) -> Result<PreparedUpload> {
    let profile = profile_of(link)?;
    let storage = storage_of(link)?;
    let probed = media.probe(&request.source)?;
    let check = UploadCheck {
        name: &request.name,
        location: request.location,
        media: &probed,
        converter: media.tools().converter(),
        options: request.options,
    };
    let plan = checked(storage, &profile, &check)?;
    Ok(PreparedUpload {
        source: request.source.clone(),
        media: probed,
        plan,
    })
}

/// Runs a prepared upload: converts when the plan says so, sends and
/// verifies the stored size. Replacing a file needs `Confirm::Yes`, checked
/// first: with `Confirm::No` the port is not called at all. A file that
/// appeared at the target since the preflight is refused the same way.
///
/// Progress goes to `job` (convert, upload, verify); cancelling returns
/// `BezelError::Cancelled` whose `partial` names what an interrupted upload
/// left on the screen (offer a confirmed [`delete`]). A stored size that
/// differs from the file's fails with `BezelError::Transport` telling the
/// user to delete the file and send it again (bytes an earlier cancelled
/// upload left queued on the screen can land in this file); nothing is
/// deleted here.
pub fn upload(
    link: &mut dyn ScreenLink,
    media: &mut dyn MediaTranscoder,
    prepared: &PreparedUpload,
    confirm: Confirm,
    job: &mut Job<'_>,
) -> Result<Uploaded> {
    let path = &prepared.plan.path;
    let overwrite = Operation::Overwrite(path.clone());
    if prepared.plan.replaces.is_some() {
        Confirmed::require(confirm, &overwrite)?;
    }
    let profile = profile_of(link)?;
    let storage = storage_of(link)?;
    if confirm == Confirm::No && presence(storage, path)? != Presence::Absent {
        Confirmed::require(confirm, &overwrite)?;
    }
    let (source, bytes) = match &prepared.plan.action {
        UploadAction::AsIs { bytes } => (prepared.source.clone(), *bytes),
        UploadAction::Convert(target) => {
            job.checkpoint()?;
            let output = media.transcode(&prepared.source, target, job)?;
            let bytes = recheck(storage, media, &profile, path, &output)?;
            (output, bytes)
        }
    };
    let data = media.load(&source)?;
    if data.len() as u64 != bytes {
        return Err(BezelError::InvalidInput(format!(
            "{} changed while its upload was prepared",
            source.0
        )));
    }
    job.checkpoint()?;
    storage.upload(path, &data, job)?;
    verify(storage, path, bytes, job)?;
    Ok(Uploaded {
        path: path.clone(),
        bytes,
        converted: matches!(prepared.plan.action, UploadAction::Convert(_)),
    })
}

/// The preflight against the screen's current storage.
fn checked(
    storage: &mut dyn ScreenStorage,
    profile: &UploadProfile,
    check: &UploadCheck<'_>,
) -> Result<UploadPlan> {
    let info = storage.info()?;
    let stored = stored_on(storage, &info, check.location.medium)?;
    match preflight(check, profile, &info, &stored) {
        Ok(mut plan) => {
            if let Some(entry) = &mut plan.replaces {
                entry.size = presence(storage, &entry.path)?.size();
            }
            Ok(plan)
        }
        Err(Refusal::NoSpace {
            needed,
            free,
            candidates,
        }) => {
            let candidates = with_sizes(storage, candidates)?;
            Err(BezelError::Refused(Refusal::no_space(
                needed, free, candidates,
            )))
        }
        Err(refusal) => Err(BezelError::Refused(refusal)),
    }
}

/// Names on both folders of `medium`, without sizes; nothing for a missing card.
fn stored_on(
    storage: &mut dyn ScreenStorage,
    info: &StorageInfo,
    medium: Medium,
) -> Result<Vec<FileEntry>> {
    let mut out = Vec::new();
    if info.capacity(medium).is_none() {
        return Ok(out);
    }
    for kind in MediaKind::ALL {
        let location = StorageLocation::new(medium, kind);
        for name in storage.list(location)? {
            let path = RemotePath::new(location, name);
            out.push(FileEntry { path, size: None });
        }
    }
    Ok(out)
}

fn with_sizes(storage: &mut dyn ScreenStorage, entries: Vec<FileEntry>) -> Result<Vec<FileEntry>> {
    entries
        .into_iter()
        .map(|entry| {
            let size = presence(storage, &entry.path)?.size();
            Ok(FileEntry { size, ..entry })
        })
        .collect()
}

/// The conversion output must now fit the profile, the limits and the space.
fn recheck(
    storage: &mut dyn ScreenStorage,
    media: &mut dyn MediaTranscoder,
    profile: &UploadProfile,
    path: &RemotePath,
    output: &MediaLocation,
) -> Result<u64> {
    let converted = media.probe(output)?;
    let check = UploadCheck {
        name: path.name.as_str(),
        location: path.location,
        media: &converted,
        converter: Converter::Missing,
        options: ConvertOptions::default(),
    };
    let plan = checked(storage, profile, &check).map_err(|e| match e {
        BezelError::Refused(Refusal::NeedsConverter(m)) => {
            BezelError::Refused(Refusal::WrongProfile(m))
        }
        other => other,
    })?;
    match plan.action {
        UploadAction::AsIs { bytes } => Ok(bytes),
        UploadAction::Convert(_) => Err(BezelError::Refused(Refusal::WrongProfile(Vec::new()))),
    }
}

fn verify(
    storage: &mut dyn ScreenStorage,
    path: &RemotePath,
    bytes: u64,
    job: &mut Job<'_>,
) -> Result<()> {
    job.report(Progress::new(JobPhase::Verify, 0, 1));
    let stored = storage.size(path)?;
    if stored != Some(bytes) {
        return Err(BezelError::Transport(format!(
            "{path} was stored with {} bytes, not the file's {bytes}: \
             the stored size differs; delete it and send it again",
            stored.unwrap_or(0)
        )));
    }
    job.report(Progress::new(JobPhase::Verify, 1, 1));
    Ok(())
}

/// Deletes a stored file. Needs `Confirm::Yes`; with `Confirm::No` the port
/// is not called.
pub fn delete(link: &mut dyn ScreenLink, path: &RemotePath, confirm: Confirm) -> Result<()> {
    let confirmed = Confirmed::require(confirm, &Operation::Delete(path.clone()))?;
    storage_of(link)?.delete(path, confirmed)
}

fn ensure_stored(storage: &mut dyn ScreenStorage, path: &RemotePath) -> Result<()> {
    match presence(storage, path)? {
        Presence::Stored(_) => Ok(()),
        Presence::Absent => Err(BezelError::InvalidInput(format!(
            "{path} is not stored on the screen"
        ))),
    }
}

/// Plays `path` as its folder says: a video with `repeat`, an image.
fn start_playing(storage: &mut dyn ScreenStorage, path: &RemotePath, repeat: Repeat) -> Result<()> {
    match path.location.kind {
        MediaKind::Video => storage.play_video(path, repeat),
        MediaKind::Image => storage.play_image(path),
    }
}

fn play_stored(storage: &mut dyn ScreenStorage, path: &RemotePath, repeat: Repeat) -> Result<()> {
    ensure_stored(storage, path)?;
    start_playing(storage, path, repeat)
}

/// Plays a stored file on the screen (videos with `repeat`; images ignore it).
pub fn play(link: &mut dyn ScreenLink, path: &RemotePath, repeat: Repeat) -> Result<()> {
    play_stored(storage_of(link)?, path, repeat)
}

/// Stops device-side playback.
pub fn stop(link: &mut dyn ScreenLink) -> Result<()> {
    storage_of(link)?.stop()
}

/// Sets what the screen shows on its own after power-up: a stored file is
/// played (videos loop) so the firmware picks it, then the start mode is
/// written; both under one `Confirm::Yes` (D-2026-09-30-storage-video-5).
///
/// The screen keeps the backlight level it boots with alongside the start
/// mode: `brightness` is set first, so it boots with that level (`None`
/// keeps the one the link last set). With `Confirm::No` the port is not
/// called; a file that is not stored is refused before anything is sent.
pub fn set_boot_media(
    link: &mut dyn ScreenLink,
    boot: &BootMedia,
    brightness: Option<Brightness>,
    confirm: Confirm,
) -> Result<()> {
    let confirmed = Confirmed::require(confirm, &Operation::Boot(boot.clone()))?;
    let storage = storage_of(link)?;
    if let BootMedia::File(path) = boot {
        ensure_stored(storage, path)?;
    }
    if let Some(level) = brightness {
        link.set_brightness(level)?;
    }
    let storage = storage_of(link)?;
    if let BootMedia::File(path) = boot {
        start_playing(storage, path, Repeat::Loop)?;
    }
    storage.set_start_mode(boot.start_mode(), confirmed)
}

/// A suggested upload name for a host file, when the screen can store it.
pub fn suggest_name(
    link: &dyn ScreenLink,
    host_name: &str,
    media: &MediaInfo,
) -> Result<Option<FileName>> {
    Ok(profile_of(link)?.suggest_name(host_name, media))
}

#[cfg(test)]
mod tests {
    //! The use cases run through the adapters' fakes in `tests/storage.rs`;
    //! here only what needs no port.

    use super::*;

    #[test]
    fn unknown_size_counts_as_present() {
        let unknown = Presence::from_size(Err(BezelError::Unsupported("no size".into())));
        assert_eq!(unknown, Ok(Presence::Stored(None)));
        assert_eq!(unknown.map(Presence::size), Ok(None));
        let known = Presence::from_size(Ok(Some(12)));
        assert_eq!(known.map(Presence::size), Ok(Some(12)));
        assert_eq!(Presence::from_size(Ok(None)), Ok(Presence::Absent));
        assert_eq!(Presence::Absent.size(), None);
        let lost = BezelError::Timeout("the screen".into());
        assert_eq!(Presence::from_size(Err(lost.clone())), Err(lost));
    }
}
