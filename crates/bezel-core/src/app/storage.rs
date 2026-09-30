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

#[cfg(test)]
pub(crate) mod doubles {
    //! Recording doubles of the storage and media ports. The adapter crates
    //! ship the real fakes, but the core's unit tests cannot link them (they
    //! depend on this crate), so these only record what reached the "screen".

    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;
    use crate::domain::frame::Frame;
    use crate::domain::geometry::{Orientation, Size};
    use crate::domain::media::{MediaTools, StreamSpec, TranscodeTarget};
    use crate::domain::screen::{Brightness, ScreenIdentity};
    use crate::domain::storage::{Capacity, StartMode};
    use crate::ports::VideoFrames;

    /// One call that reached the storage port.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Call {
        Info,
        List(StorageLocation),
        Size(RemotePath),
        Upload(RemotePath, usize),
        Delete(RemotePath),
        PlayVideo(RemotePath, Repeat),
        PlayImage(RemotePath),
        Stop,
        StartMode(StartMode),
    }

    impl Call {
        /// True for calls that change what the screen stores, shows or keeps.
        pub(crate) fn changes_the_screen(&self) -> bool {
            !matches!(self, Call::Info | Call::List(_) | Call::Size(_))
        }
    }

    /// A screen whose storage records every call.
    pub(crate) struct Screen {
        identity: ScreenIdentity,
        has_storage: bool,
        pub(crate) info: StorageInfo,
        pub(crate) files: BTreeMap<RemotePath, u64>,
        pub(crate) calls: Vec<Call>,
        /// Bytes the screen "loses" on upload (to fail verification).
        pub(crate) short_by: u64,
    }

    impl Screen {
        fn of(id: &'static str, has_storage: bool) -> Self {
            let model = model_by_id(ModelId(id)).expect("catalog model");
            let capacity = Capacity {
                total: 1_000_000_000,
                used: 0,
                free: 1_000_000_000,
            };
            Self {
                identity: ScreenIdentity {
                    model,
                    firmware: None,
                },
                has_storage,
                info: StorageInfo {
                    internal: capacity,
                    card: None,
                },
                files: BTreeMap::new(),
                calls: Vec::new(),
                short_by: 0,
            }
        }

        /// A Turing 8.8" with 1 GB of free flash and no card.
        pub(crate) fn turing_88() -> Self {
            Self::of("turing-8.8", true)
        }

        /// A Turing 3.5" (rev A): no storage.
        pub(crate) fn turing_35() -> Self {
            Self::of("turing-3.5", false)
        }

        /// With a stored file.
        pub(crate) fn with_file(mut self, path: &str, size: u64) -> Self {
            let path = RemotePath::parse(path).expect("path");
            self.files.insert(path, size);
            self
        }
    }

    impl ScreenLink for Screen {
        fn identity(&self) -> &ScreenIdentity {
            &self.identity
        }
        fn set_brightness(&mut self, _: Brightness) -> Result<()> {
            Ok(())
        }
        fn set_orientation(&mut self, _: Orientation) -> Result<()> {
            Ok(())
        }
        fn present(&mut self, _: &Frame) -> Result<()> {
            Ok(())
        }
        fn screen_off(&mut self) -> Result<()> {
            Ok(())
        }
        fn release(&mut self) -> Result<()> {
            Ok(())
        }
        fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
            if self.has_storage { Some(self) } else { None }
        }
    }

    impl ScreenStorage for Screen {
        fn info(&mut self) -> Result<StorageInfo> {
            self.calls.push(Call::Info);
            Ok(self.info)
        }
        fn list(&mut self, location: StorageLocation) -> Result<Vec<FileName>> {
            self.calls.push(Call::List(location));
            let names = self.files.keys().filter(|p| p.location == location);
            Ok(names.map(|p| p.name.clone()).collect())
        }
        fn size(&mut self, path: &RemotePath) -> Result<Option<u64>> {
            self.calls.push(Call::Size(path.clone()));
            Ok(self.files.get(path).copied())
        }
        fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()> {
            self.calls.push(Call::Upload(path.clone(), data.len()));
            let total = data.len() as u64;
            job.report(Progress::new(JobPhase::Upload, 0, total));
            if job.is_cancelled() {
                let partial = total / 2;
                self.files.insert(path.clone(), partial);
                return Err(BezelError::Cancelled {
                    partial: Some(partial),
                });
            }
            self.files.insert(path.clone(), total - self.short_by);
            job.report(Progress::new(JobPhase::Upload, total, total));
            Ok(())
        }
        fn delete(&mut self, path: &RemotePath, _: Confirmed) -> Result<()> {
            self.calls.push(Call::Delete(path.clone()));
            self.files.remove(path);
            Ok(())
        }
        fn play_video(&mut self, path: &RemotePath, repeat: Repeat) -> Result<()> {
            self.calls.push(Call::PlayVideo(path.clone(), repeat));
            Ok(())
        }
        fn play_image(&mut self, path: &RemotePath) -> Result<()> {
            self.calls.push(Call::PlayImage(path.clone()));
            Ok(())
        }
        fn stop(&mut self) -> Result<()> {
            self.calls.push(Call::Stop);
            Ok(())
        }
        fn set_start_mode(&mut self, mode: StartMode, _: Confirmed) -> Result<()> {
            self.calls.push(Call::StartMode(mode));
            Ok(())
        }
    }

    /// Local media files held in memory; conversions produce an in-profile
    /// MP4 of the target size.
    pub(crate) struct Media {
        converter: Converter,
        files: BTreeMap<String, MediaInfo>,
        pub(crate) calls: Vec<String>,
        /// Size of every conversion output, in bytes.
        pub(crate) output_bytes: u64,
        /// Resolution of every conversion output (`None`: the target's).
        pub(crate) output_size: Option<Size>,
    }

    impl Media {
        pub(crate) fn new(converter: Converter) -> Self {
            Self {
                converter,
                files: BTreeMap::new(),
                calls: Vec::new(),
                output_bytes: 2000,
                output_size: None,
            }
        }

        pub(crate) fn with(mut self, location: &str, info: MediaInfo) -> Self {
            self.files.insert(location.to_string(), info);
            self
        }
    }

    impl MediaTranscoder for Media {
        fn tools(&mut self) -> MediaTools {
            match self.converter {
                Converter::Available => MediaTools::Ready {
                    version: "test".into(),
                },
                Converter::Missing => MediaTools::Missing {
                    install_hints: Vec::new(),
                },
            }
        }
        fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
            self.calls.push(format!("probe {}", source.0));
            self.files
                .get(&source.0)
                .cloned()
                .ok_or_else(|| BezelError::InvalidInput(format!("no file {}", source.0)))
        }
        fn transcode(
            &mut self,
            source: &MediaLocation,
            target: &TranscodeTarget,
            job: &mut Job<'_>,
        ) -> Result<MediaLocation> {
            self.calls.push(format!("transcode {}", source.0));
            job.report(Progress::new(JobPhase::Convert, 0, 10_000));
            job.checkpoint()?;
            let size = self.output_size.unwrap_or(target.size);
            let info = crate::domain::media::tests::mp4(size, self.output_bytes);
            let output = format!("{}.converted.mp4", source.0);
            self.files.insert(output.clone(), info);
            job.report(Progress::new(JobPhase::Convert, 10_000, 10_000));
            Ok(MediaLocation(output))
        }
        fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
            self.calls.push(format!("load {}", source.0));
            let info = self.probe(source)?;
            Ok(vec![0; usize::try_from(info.bytes).unwrap_or(0)])
        }
        fn stream(&mut self, _: &MediaLocation, _: StreamSpec) -> Result<Box<dyn VideoFrames>> {
            Err(BezelError::Unsupported("no host decoding in tests".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::doubles::{Call, Media, Screen};
    use super::*;
    use crate::domain::frame::Frame;
    use crate::domain::geometry::{Orientation, Size};
    use crate::domain::job::CancelToken;
    use crate::domain::media::tests::{mp4, still};
    use crate::domain::media::{MediaFormat, StreamSpec};
    use crate::domain::screen::Brightness;
    use crate::domain::storage::{Capacity, StartMode};

    const NATIVE: Size = Size::new(480, 1920);

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).expect("path")
    }

    fn request(source: &str, name: &str, location: &str) -> UploadRequest {
        let location = path(&format!("{location}/x")).location;
        UploadRequest {
            source: MediaLocation(source.into()),
            name: name.into(),
            location,
            options: ConvertOptions::default(),
        }
    }

    /// Runs `upload` and returns its result with the progress it reported.
    fn run_upload(
        screen: &mut Screen,
        media: &mut Media,
        prepared: &PreparedUpload,
        confirm: Confirm,
        token: &CancelToken,
    ) -> (Result<Uploaded>, Vec<Progress>) {
        let mut seen = Vec::new();
        let mut sink = |p: Progress| seen.push(p);
        let mut job = Job::new(token, &mut sink);
        let result = upload(screen, media, prepared, confirm, &mut job);
        (result, seen)
    }

    #[test]
    fn screens_without_storage_are_unsupported() {
        let mut screen = Screen::turing_35();
        let unsupported = |r: Result<()>| matches!(r, Err(BezelError::Unsupported(_)));
        assert!(unsupported(info(&mut screen).map(|_| ())));
        assert!(unsupported(stop(&mut screen)));
        let clip = path("internal/video/a.mp4");
        assert!(unsupported(play(&mut screen, &clip, Repeat::Loop)));
        assert!(unsupported(delete(&mut screen, &clip, Confirm::Yes)));
        let loc = clip.location;
        assert!(unsupported(list(&mut screen, loc).map(|_| ())));
        let mut media = Media::new(Converter::Missing);
        let req = request("a.mp4", "a.mp4", "internal/video");
        assert!(unsupported(
            prepare_upload(&mut screen, &mut media, &req).map(|_| ())
        ));
        assert!(unsupported(
            suggest_name(&screen, "a.mp4", &mp4(NATIVE, 1)).map(|_| ())
        ));
        let err = storage_of(&mut screen).map(|_| ()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "not supported: Turing Smart Screen 3.5\" has no storage"
        );
        assert!(screen.calls.is_empty());
    }

    #[test]
    fn listing_reports_sizes_and_never_touches_a_missing_card() {
        let mut screen = Screen::turing_88()
            .with_file("internal/video/b.mp4", 20)
            .with_file("internal/video/a.mp4", 10)
            .with_file("internal/image/logo.png", 5);
        let video = path("internal/video/x").location;
        let entries = list(&mut screen, video).unwrap();
        let listed: Vec<(String, Option<u64>)> = entries
            .iter()
            .map(|e| (e.path.to_string(), e.size))
            .collect();
        assert_eq!(
            listed,
            [
                ("internal/video/a.mp4".to_string(), Some(10)),
                ("internal/video/b.mp4".to_string(), Some(20))
            ]
        );
        screen.calls.clear();
        let card = path("sd/video/x").location;
        assert_eq!(
            list(&mut screen, card),
            Err(BezelError::Refused(Refusal::NoCard))
        );
        assert_eq!(screen.calls, [Call::Info], "no LIST_DIR on a missing card");
        screen.info.card = Some(Capacity::default());
        assert!(list(&mut screen, card).unwrap().is_empty());
        assert_eq!(info(&mut screen).unwrap().card, Some(Capacity::default()));
    }

    #[test]
    fn an_upload_is_sent_verified_and_reports_progress() {
        let mut screen = Screen::turing_88();
        let mut media = Media::new(Converter::Missing).with("clip.mp4", mp4(NATIVE, 1000));
        let prepared = prepare_upload(
            &mut screen,
            &mut media,
            &request("clip.mp4", "Clip.mp4", "internal/video"),
        )
        .unwrap();
        assert_eq!(prepared.plan.action, UploadAction::AsIs { bytes: 1000 });
        assert!(screen.calls.iter().all(|c| !c.changes_the_screen()));
        let (done, progress) = run_upload(
            &mut screen,
            &mut media,
            &prepared,
            Confirm::No,
            &CancelToken::new(),
        );
        let clip = path("internal/video/clip.mp4");
        assert_eq!(
            done.unwrap(),
            Uploaded {
                path: clip.clone(),
                bytes: 1000,
                converted: false
            }
        );
        assert!(screen.calls.contains(&Call::Upload(clip.clone(), 1000)));
        assert_eq!(screen.calls.last(), Some(&Call::Size(clip.clone())));
        let phases: Vec<(JobPhase, u64, u64)> = progress
            .iter()
            .map(|p| (p.phase, p.done, p.total))
            .collect();
        assert_eq!(
            phases,
            [
                (JobPhase::Upload, 0, 1000),
                (JobPhase::Upload, 1000, 1000),
                (JobPhase::Verify, 0, 1),
                (JobPhase::Verify, 1, 1)
            ]
        );
    }

    #[test]
    fn a_video_off_profile_is_converted_then_checked_again() {
        let mut screen = Screen::turing_88();
        let landscape = mp4(Size::new(1920, 1080), 5000);
        let mut media = Media::new(Converter::Available).with("trip.mov", landscape.clone());
        let name = suggest_name(&screen, "Trip.MOV", &landscape)
            .unwrap()
            .unwrap();
        let req = request("trip.mov", name.as_str(), "internal/video");
        let prepared = prepare_upload(&mut screen, &mut media, &req).unwrap();
        assert!(matches!(prepared.plan.action, UploadAction::Convert(t) if t.size == NATIVE));
        let (done, progress) = run_upload(
            &mut screen,
            &mut media,
            &prepared,
            Confirm::No,
            &CancelToken::new(),
        );
        let done = done.unwrap();
        assert!(done.converted);
        assert_eq!(done.bytes, 2000);
        assert_eq!(done.path.to_string(), "internal/video/trip.mp4");
        assert_eq!(progress[0].phase, JobPhase::Convert);
        assert!(
            media
                .calls
                .contains(&"load trip.mov.converted.mp4".to_string())
        );

        // An output that still misses the profile is refused, nothing is sent.
        let mut screen = Screen::turing_88();
        media.output_size = Some(Size::new(1, 1));
        let (refused, _) = run_upload(
            &mut screen,
            &mut media,
            &prepared,
            Confirm::No,
            &CancelToken::new(),
        );
        assert!(matches!(
            refused,
            Err(BezelError::Refused(Refusal::WrongProfile(_)))
        ));
        assert!(screen.calls.iter().all(|c| !c.changes_the_screen()));

        // Without a converter the preflight says so.
        let mut bare = Media::new(Converter::Missing).with("trip.mov", landscape);
        let refused = prepare_upload(&mut screen, &mut bare, &req);
        assert!(matches!(
            refused,
            Err(BezelError::Refused(Refusal::NeedsConverter(_)))
        ));
    }

    #[test]
    fn a_full_medium_lists_sized_candidates_and_deletes_nothing() {
        let mut screen = Screen::turing_88()
            .with_file("internal/video/old.mp4", 700)
            .with_file("internal/image/logo.png", 100);
        screen.info.internal.free = 500;
        let mut media = Media::new(Converter::Missing).with("clip.mp4", mp4(NATIVE, 1000));
        let refused = prepare_upload(
            &mut screen,
            &mut media,
            &request("clip.mp4", "clip.mp4", "internal/video"),
        );
        let Err(BezelError::Refused(Refusal::NoSpace { candidates, .. })) = refused else {
            unreachable!("expected NoSpace, got {refused:?}");
        };
        let sizes: Vec<Option<u64>> = candidates.iter().map(|c| c.size).collect();
        assert_eq!(sizes, [Some(700), Some(100)]);
        assert!(screen.calls.iter().all(|c| !c.changes_the_screen()));
        assert_eq!(screen.files.len(), 2);
    }

    #[test]
    fn cancelling_stops_the_upload_and_reports_the_partial_file() {
        let mut screen = Screen::turing_88();
        let mut media = Media::new(Converter::Missing).with("clip.mp4", mp4(NATIVE, 1000));
        let prepared = prepare_upload(
            &mut screen,
            &mut media,
            &request("clip.mp4", "clip.mp4", "internal/video"),
        )
        .unwrap();

        // Cancelled before anything was sent.
        let token = CancelToken::new();
        token.cancel();
        let (result, _) = run_upload(&mut screen, &mut media, &prepared, Confirm::No, &token);
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        assert!(screen.calls.iter().all(|c| !c.changes_the_screen()));

        // Cancelled during the transfer: the adapter reports what is left.
        let token = CancelToken::new();
        let remote = token.clone();
        let mut sink = |p: Progress| {
            if p.phase == JobPhase::Upload {
                remote.cancel();
            }
        };
        let mut job = Job::new(&token, &mut sink);
        let result = upload(&mut screen, &mut media, &prepared, Confirm::No, &mut job);
        assert_eq!(result, Err(BezelError::Cancelled { partial: Some(500) }));
        assert!(!screen.calls.iter().any(|c| matches!(c, Call::Delete(_))));
    }

    #[test]
    fn verification_and_races_are_caught() {
        let mut screen = Screen::turing_88();
        screen.short_by = 1;
        let mut media = Media::new(Converter::Missing).with("clip.mp4", mp4(NATIVE, 1000));
        let req = request("clip.mp4", "clip.mp4", "internal/video");
        let prepared = prepare_upload(&mut screen, &mut media, &req).unwrap();
        let (result, _) = run_upload(
            &mut screen,
            &mut media,
            &prepared,
            Confirm::No,
            &CancelToken::new(),
        );
        let err = result.unwrap_err();
        assert_eq!(
            err,
            BezelError::Transport(
                "internal/video/clip.mp4 holds 999 of 1000 bytes after the upload".into()
            )
        );
        // The same file now exists: without Yes it is not replaced...
        screen.calls.clear();
        let (result, _) = run_upload(
            &mut screen,
            &mut media,
            &prepared,
            Confirm::No,
            &CancelToken::new(),
        );
        assert!(matches!(result, Err(BezelError::NotConfirmed(_))));
        assert!(screen.calls.iter().all(|c| !c.changes_the_screen()));
        // ...with Yes it is.
        screen.short_by = 0;
        let (result, _) = run_upload(
            &mut screen,
            &mut media,
            &prepared,
            Confirm::Yes,
            &CancelToken::new(),
        );
        assert!(result.is_ok());

        // A source that changed size since the preflight is refused.
        let mut changed = Media::new(Converter::Missing).with("clip.mp4", mp4(NATIVE, 10));
        let (result, _) = run_upload(
            &mut screen,
            &mut changed,
            &prepared,
            Confirm::Yes,
            &CancelToken::new(),
        );
        assert!(matches!(result, Err(BezelError::InvalidInput(_))));
    }

    #[test]
    fn play_stop_and_boot_media() {
        let mut screen = Screen::turing_88()
            .with_file("internal/video/loop.mp4", 10)
            .with_file("internal/image/logo.png", 5);
        let video = path("internal/video/loop.mp4");
        let image = path("internal/image/logo.png");
        play(&mut screen, &video, Repeat::Once).unwrap();
        play(&mut screen, &image, Repeat::Loop).unwrap();
        stop(&mut screen).unwrap();
        let missing = path("internal/video/none.mp4");
        assert!(matches!(
            play(&mut screen, &missing, Repeat::Loop),
            Err(BezelError::InvalidInput(_))
        ));
        let writes: Vec<Call> = screen
            .calls
            .drain(..)
            .filter(Call::changes_the_screen)
            .collect();
        assert_eq!(
            writes,
            [
                Call::PlayVideo(video.clone(), Repeat::Once),
                Call::PlayImage(image.clone()),
                Call::Stop
            ]
        );

        set_boot_media(&mut screen, &BootMedia::File(video.clone()), Confirm::Yes).unwrap();
        set_boot_media(&mut screen, &BootMedia::File(image.clone()), Confirm::Yes).unwrap();
        set_boot_media(&mut screen, &BootMedia::Default, Confirm::Yes).unwrap();
        let writes: Vec<Call> = screen
            .calls
            .drain(..)
            .filter(Call::changes_the_screen)
            .collect();
        assert_eq!(
            writes,
            [
                Call::PlayVideo(video, Repeat::Loop),
                Call::StartMode(StartMode::Video),
                Call::PlayImage(image),
                Call::StartMode(StartMode::Image),
                Call::StartMode(StartMode::Default)
            ]
        );
        let missing_boot = set_boot_media(&mut screen, &BootMedia::File(missing), Confirm::Yes);
        assert!(matches!(missing_boot, Err(BezelError::InvalidInput(_))));
        assert!(screen.calls.iter().all(|c| !c.changes_the_screen()));
    }

    #[test]
    fn doubles_behave() {
        let mut media = Media::new(Converter::Missing).with("a.png", still(MediaFormat::Png, 3));
        let spec = StreamSpec {
            size: NATIVE,
            fps: 10,
        };
        assert!(media.stream(&MediaLocation("a.png".into()), spec).is_err());
        assert!(media.probe(&MediaLocation("none".into())).is_err());
        assert_eq!(media.load(&MediaLocation("a.png".into())).unwrap().len(), 3);
        let mut screen = Screen::turing_88();
        assert!(screen.set_brightness(Brightness::MAX).is_ok());
        assert!(screen.set_orientation(Orientation::Landscape).is_ok());
        assert!(
            screen
                .present(&Frame::filled(NATIVE, Default::default()))
                .is_ok()
        );
        assert!(screen.screen_off().is_ok());
        assert!(screen.release().is_ok());
    }
}
