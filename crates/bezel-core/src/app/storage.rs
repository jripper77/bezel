//! Storage use cases: what a screen stores, uploads (preflight, conversion,
//! sending, verification), deletes, device-side playback and the boot media.
//!
//! Every destructive or persistent step needs `Confirm::Yes`, checked here
//! before the port is called (D-2026-09-30-storage-video-1). Screens without
//! storage answer `BezelError::Unsupported`.

use crate::domain::job::{Job, JobPhase, Progress};
use crate::domain::media::{ConvertOptions, Converter, MediaInfo, MediaKind, UploadProfile};
use crate::domain::screen::Confirm;
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

/// Capacity and use of the screen's internal flash and memory card.
pub fn info(link: &mut dyn ScreenLink) -> Result<StorageInfo> {
    storage_of(link)?.info()
}

/// The files in `location` with their sizes (one size query per file). A
/// card folder is listed only when a card is present
/// (`Refused(NoCard)` otherwise), because listing creates the folder.
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
            let size = storage.size(&path)?;
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
/// left on the screen (offer a confirmed [`delete`]).
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
    if confirm == Confirm::No && storage.size(path)?.is_some() {
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
                entry.size = storage.size(&entry.path)?;
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
            let size = storage.size(&entry.path)?;
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
            "{path} holds {} of {bytes} bytes after the upload",
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
    match storage.size(path)? {
        Some(_) => Ok(()),
        None => Err(BezelError::InvalidInput(format!(
            "{path} is not stored on the screen"
        ))),
    }
}

fn play_stored(storage: &mut dyn ScreenStorage, path: &RemotePath, repeat: Repeat) -> Result<()> {
    ensure_stored(storage, path)?;
    match path.location.kind {
        MediaKind::Video => storage.play_video(path, repeat),
        MediaKind::Image => storage.play_image(path),
    }
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
/// With `Confirm::No` the port is not called.
pub fn set_boot_media(link: &mut dyn ScreenLink, boot: &BootMedia, confirm: Confirm) -> Result<()> {
    let confirmed = Confirmed::require(confirm, &Operation::Boot(boot.clone()))?;
    let storage = storage_of(link)?;
    if let BootMedia::File(path) = boot {
        play_stored(storage, path, Repeat::Loop)?;
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
