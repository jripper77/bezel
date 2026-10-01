//! Files stored on a screen (internal flash or memory card), and the rules
//! every upload, delete and boot-media change passes before a byte is sent.
//!
//! A screen stores media in four folders: internal or card × image or video
//! ([`StorageLocation`]). Their device paths are family knowledge and live in
//! the device adapters (rev C large screens: `/mnt/UDISK/img/`, ...); the core
//! only names files inside them ([`FileName`], [`RemotePath`]). Decisions
//! D-2026-09-30-storage-video-1 and -3: deleting, replacing a file and
//! changing the boot media need [`Confirm::Yes`]; the preflight never frees
//! space on its own.

use std::fmt;

use super::media::{
    ConvertOptions, Converter, MediaInfo, MediaKind, Mismatch, TranscodeTarget, UploadProfile,
};
use super::screen::Confirm;
use crate::BezelError;

/// Where a screen stores files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Medium {
    /// The screen's internal flash.
    Internal,
    /// The memory (TF/SD) card, reached only through the screen's protocol.
    Card,
}

impl Medium {
    /// Both media.
    pub const ALL: [Medium; 2] = [Medium::Internal, Medium::Card];

    /// Stable machine name (`internal`, `sd`).
    pub const fn slug(self) -> &'static str {
        match self {
            Medium::Internal => "internal",
            Medium::Card => "sd",
        }
    }

    /// The medium named by `slug`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.slug() == slug)
    }
}

/// One of the four folders a screen stores media in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StorageLocation {
    /// Internal flash or card.
    pub medium: Medium,
    /// Image or video folder.
    pub kind: MediaKind,
}

impl StorageLocation {
    /// The four folders, internal first, images first.
    pub const ALL: [StorageLocation; 4] = [
        StorageLocation::new(Medium::Internal, MediaKind::Image),
        StorageLocation::new(Medium::Internal, MediaKind::Video),
        StorageLocation::new(Medium::Card, MediaKind::Image),
        StorageLocation::new(Medium::Card, MediaKind::Video),
    ];

    /// A folder.
    pub const fn new(medium: Medium, kind: MediaKind) -> Self {
        Self { medium, kind }
    }
}

impl fmt::Display for StorageLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.medium.slug(), self.kind.slug())
    }
}

/// Longest device path any storage command carries (rev C UPLOAD_FILE: 240
/// payload bytes minus the LE32 file size).
pub const MAX_PATH_BYTES: usize = 236;

/// Longest folder path of any family (TUR_USB `/tmp/sdcard/mmcblk0p1/video/`).
/// Keeping names within `MAX_PATH_BYTES - LONGEST_ROOT_BYTES` makes every
/// device path fit without the core knowing the roots.
pub const LONGEST_ROOT_BYTES: usize = 28;

/// Why a string is not a usable file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// Nothing left.
    Empty,
    /// Longer than [`FileName::MAX_BYTES`].
    TooLong {
        /// Length of the name in bytes.
        bytes: usize,
        /// The limit.
        max: usize,
    },
    /// A character that is not allowed.
    Forbidden(char),
    /// `.` or `..`.
    Reserved,
    /// Upload names cannot start with a dot.
    LeadingDot,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => f.write_str("the name is empty"),
            NameError::TooLong { bytes, max } => {
                write!(f, "the name has {bytes} bytes (at most {max})")
            }
            NameError::Forbidden(c) => {
                write!(f, "{c:?} is not allowed (use a-z, 0-9, '_', '.' and '-')")
            }
            NameError::Reserved => f.write_str("'.' and '..' are not file names"),
            NameError::LeadingDot => f.write_str("the name cannot start with '.'"),
        }
    }
}

/// A file name on a screen: one path component, never a path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileName(String);

const fn upload_char(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | '-')
}

impl FileName {
    /// Longest name, in bytes, so that every family's device path stays
    /// within [`MAX_PATH_BYTES`].
    pub const MAX_BYTES: usize = MAX_PATH_BYTES - LONGEST_ROOT_BYTES;

    /// A name as a screen reported it in a listing: printable ASCII without
    /// `/` or `\`. Lenient, so that files put on a card by other means can
    /// still be played or deleted.
    pub fn parse(raw: &str) -> Result<Self, NameError> {
        if raw.is_empty() {
            return Err(NameError::Empty);
        }
        if raw == "." || raw == ".." {
            return Err(NameError::Reserved);
        }
        let bad = |c: &char| !(c.is_ascii_graphic() || *c == ' ') || *c == '/' || *c == '\\';
        if let Some(c) = raw.chars().find(bad) {
            return Err(NameError::Forbidden(c));
        }
        Self::within_limit(raw.to_string())
    }

    /// The name an upload gets: lower-cased, only `[a-z0-9_.-]`, not starting
    /// with a dot (the vendor's rule, D-2026-09-30-storage-video-3).
    pub fn for_upload(raw: &str) -> Result<Self, NameError> {
        let name = raw.to_ascii_lowercase();
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if let Some(c) = name.chars().find(|c| !upload_char(*c)) {
            return Err(NameError::Forbidden(c));
        }
        if name.starts_with('.') {
            return Err(NameError::LeadingDot);
        }
        Self::within_limit(name)
    }

    fn within_limit(name: String) -> Result<Self, NameError> {
        if name.len() > Self::MAX_BYTES {
            return Err(NameError::TooLong {
                bytes: name.len(),
                max: Self::MAX_BYTES,
            });
        }
        Ok(Self(name))
    }

    /// A valid upload name made from a host file name: its stem lower-cased,
    /// every other character turned into `_` (runs collapsed, ends trimmed),
    /// `media` when nothing is left, then `.extension`.
    pub fn suggest(host_name: &str, extension: &str) -> Self {
        let stem = match host_name.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem,
            _ => host_name,
        };
        let ext: String = extension
            .to_ascii_lowercase()
            .chars()
            .filter(|c| upload_char(*c) && *c != '.')
            .collect();
        let mut out = String::new();
        for c in stem.chars().map(|c| c.to_ascii_lowercase()) {
            let c = if upload_char(c) && c != '.' { c } else { '_' };
            if !(c == '_' && out.ends_with('_')) {
                out.push(c);
            }
        }
        let room = Self::MAX_BYTES - ext.len() - usize::from(!ext.is_empty());
        let mut stem = out.trim_matches('_').to_string();
        stem.truncate(room);
        if stem.is_empty() {
            stem.push_str("media");
        }
        if ext.is_empty() {
            Self(stem)
        } else {
            Self(format!("{stem}.{ext}"))
        }
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The part after the last dot, if any.
    pub fn extension(&self) -> Option<&str> {
        self.0.rsplit_once('.').map(|(_, ext)| ext)
    }
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A file on a screen: a folder and a name. Written `internal/video/88.mp4`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemotePath {
    /// The folder.
    pub location: StorageLocation,
    /// The file name.
    pub name: FileName,
}

impl RemotePath {
    /// A path.
    pub const fn new(location: StorageLocation, name: FileName) -> Self {
        Self { location, name }
    }

    /// Parses `<internal|sd>/<image|video>/<name>`; the name is read as a
    /// listed name ([`FileName::parse`]).
    pub fn parse(text: &str) -> crate::Result<Self> {
        let mut parts = text.splitn(3, '/');
        let medium = parts.next().and_then(Medium::from_slug);
        let kind = parts.next().and_then(MediaKind::from_slug);
        let (Some(medium), Some(kind), Some(name)) = (medium, kind, parts.next()) else {
            return Err(BezelError::InvalidInput(format!(
                "{text}: expected <internal|sd>/<image|video>/<name>"
            )));
        };
        let name =
            FileName::parse(name).map_err(|e| BezelError::InvalidInput(format!("{text}: {e}")))?;
        Ok(Self::new(StorageLocation::new(medium, kind), name))
    }
}

impl fmt::Display for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.location, self.name)
    }
}

/// Size and use of one medium, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capacity {
    /// Usable size (any vendor reserve already taken off).
    pub total: u64,
    /// In use.
    pub used: u64,
    /// Still available for uploads (any vendor reserve already taken off).
    pub free: u64,
}

/// What a screen reports about its storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StorageInfo {
    /// Internal flash.
    pub internal: Capacity,
    /// The memory card; `None` when no card is inserted.
    pub card: Option<Capacity>,
}

impl StorageInfo {
    /// The capacity of `medium`; `None` for a missing card.
    pub fn capacity(&self, medium: Medium) -> Option<Capacity> {
        match medium {
            Medium::Internal => Some(self.internal),
            Medium::Card => self.card,
        }
    }
}

/// A stored file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileEntry {
    /// Where it is.
    pub path: RemotePath,
    /// Its size in bytes, when known.
    pub size: Option<u64>,
}

/// Whether a stored video plays once or loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Repeat {
    /// Play to the end, then stop.
    Once,
    /// Loop until stopped.
    Loop,
}

/// What a screen shows on its own after power-up (rev C OPTIONS start mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartMode {
    /// Its built-in clock or logo.
    Default,
    /// The last stored image it played (the firmware's choice).
    Image,
    /// The last stored video it played (the firmware's choice).
    Video,
}

/// What the boot slot should show.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BootMedia {
    /// The screen's built-in start screen.
    Default,
    /// A stored image or video, played once so the firmware picks it.
    File(RemotePath),
}

impl BootMedia {
    /// The start mode that shows this media.
    pub fn start_mode(&self) -> StartMode {
        match self {
            BootMedia::Default => StartMode::Default,
            BootMedia::File(path) => match path.location.kind {
                MediaKind::Image => StartMode::Image,
                MediaKind::Video => StartMode::Video,
            },
        }
    }
}

/// A storage operation that cannot be undone or that persists on the screen.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Operation {
    /// Deleting a stored file.
    Delete(RemotePath),
    /// Uploading over a stored file.
    Overwrite(RemotePath),
    /// Changing the boot media (persistent).
    Boot(BootMedia),
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operation::Delete(path) => write!(f, "deleting {path}"),
            Operation::Overwrite(path) => write!(f, "replacing {path}"),
            Operation::Boot(BootMedia::File(path)) => {
                write!(f, "making {path} the boot media")
            }
            Operation::Boot(BootMedia::Default) => f.write_str("restoring the default boot screen"),
        }
    }
}

/// Proof that the user confirmed one [`Operation`]. Only [`Confirmed::require`]
/// makes one, and only from [`Confirm::Yes`]; the storage port's destructive
/// methods take it, so no code path reaches them unconfirmed.
#[derive(Debug)]
pub struct Confirmed {
    _proof: (),
}

impl Confirmed {
    /// The proof for `operation`, or `NotConfirmed` for [`Confirm::No`].
    pub fn require(confirm: Confirm, operation: &Operation) -> crate::Result<Self> {
        match confirm {
            Confirm::Yes => Ok(Self { _proof: () }),
            Confirm::No => Err(BezelError::NotConfirmed(operation.to_string())),
        }
    }
}

/// Bytes in a mebibyte, the unit size limits are shown in.
pub const MIB: u64 = 1 << 20;

/// The vendor's upload limit ("120 MB", read as the smaller decimal value):
/// the per-file cap of Turing USB screens.
pub const MAX_UPLOAD_BYTES: u64 = 120_000_000;

/// The per-file cap of rev C screens, 25 MiB
/// (D-2026-09-30-release-polish-12): the 8.8" (ROM 1.90) keeps a whole
/// upload in memory and stops reading at exactly 29,577,216 bytes, then
/// hangs until it is restarted; 24 MiB passed and the vendor app's largest
/// stored files are 24.6 MiB.
pub const REV_C_MAX_UPLOAD_BYTES: u64 = 25 * MIB;

/// Sizes the screens parse as signed 32-bit numbers: files this large or
/// larger read back as absent.
pub const DEVICE_SIZE_LIMIT: u64 = 1 << 31;

/// Why an upload was refused before anything was converted, sent or deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The name cannot be used.
    InvalidName(NameError),
    /// The name's extension does not match what will be stored.
    WrongExtension {
        /// The normalized name.
        name: FileName,
        /// Extensions that fit.
        accepted: &'static [&'static str],
    },
    /// An image into a video folder, or the reverse, or neither.
    WrongKind {
        /// The folder's kind.
        location: MediaKind,
        /// The file's kind; `None` when it is neither an image nor a video.
        file: Option<MediaKind>,
    },
    /// The file does not match the screen and cannot be converted.
    WrongProfile(Vec<Mismatch>),
    /// The file needs a conversion (listed differences, or the requested
    /// adjustments when empty) and no converter is installed.
    NeedsConverter(Vec<Mismatch>),
    /// The file is empty.
    EmptyFile,
    /// The file is over the screen's per-file limit.
    TooLarge {
        /// File size.
        bytes: u64,
        /// The limit.
        limit: u64,
    },
    /// The converted video is still over the screen's per-file limit (its
    /// bitrate could not be capped enough, or the source's duration is
    /// unknown): a shorter clip or a lower frame rate makes it fit.
    ConvertedTooLarge {
        /// Size of the conversion's output.
        bytes: u64,
        /// The limit.
        limit: u64,
    },
    /// The target is the memory card and none is inserted.
    NoCard,
    /// The file does not fit. Nothing is deleted: `candidates` lists the
    /// files on that medium, largest first, for an explicit, confirmed delete.
    NoSpace {
        /// Bytes to store.
        needed: u64,
        /// Bytes available.
        free: u64,
        /// Stored files the user may choose to delete.
        candidates: Vec<FileEntry>,
    },
}

impl Refusal {
    /// A [`Refusal::NoSpace`] with its candidates largest first (unknown
    /// sizes last, then by path).
    pub fn no_space(needed: u64, free: u64, mut candidates: Vec<FileEntry>) -> Self {
        candidates.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        Refusal::NoSpace {
            needed,
            free,
            candidates,
        }
    }
}

fn joined(items: &[Mismatch]) -> String {
    let parts: Vec<String> = items.iter().map(ToString::to_string).collect();
    parts.join("; ")
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::InvalidName(e) => write!(f, "invalid file name: {e}"),
            Refusal::WrongExtension { name, accepted } => {
                write!(f, "{name} must end in .{}", accepted.join(" or ."))
            }
            Refusal::WrongKind { location, file } => match file {
                Some(MediaKind::Image) => write!(f, "an image cannot go into a {location} folder"),
                Some(MediaKind::Video) => write!(f, "a video cannot go into an {location} folder"),
                None => write!(f, "not an image or a video the screen can store"),
            },
            Refusal::WrongProfile(m) => {
                write!(f, "the file does not suit the screen: {}", joined(m))
            }
            Refusal::NeedsConverter(m) if m.is_empty() => f.write_str(
                "the requested adjustments need the media converter, which is not installed",
            ),
            Refusal::NeedsConverter(m) => write!(
                f,
                "the file must be converted ({}) and the media converter is not installed",
                joined(m)
            ),
            Refusal::EmptyFile => f.write_str("the file is empty"),
            Refusal::TooLarge { bytes, limit } => write!(
                f,
                "the file is {} MiB, over the screen's {} MiB limit per file",
                mib_text(*bytes, Rounding::Up),
                mib_text(*limit, Rounding::Down)
            ),
            Refusal::ConvertedTooLarge { bytes, limit } => write!(
                f,
                "the converted video is {} MiB, over the screen's {} MiB limit per file; \
                 use a shorter clip or a lower frame rate",
                mib_text(*bytes, Rounding::Up),
                mib_text(*limit, Rounding::Down)
            ),
            Refusal::NoCard => f.write_str("the screen has no memory card"),
            Refusal::NoSpace { needed, free, .. } => write!(
                f,
                "{needed} bytes do not fit in the {free} bytes free; delete files first"
            ),
        }
    }
}

/// An upload to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadCheck<'a> {
    /// The requested name (normalized by the check).
    pub name: &'a str,
    /// Target folder.
    pub location: StorageLocation,
    /// The file as probed.
    pub media: &'a MediaInfo,
    /// Whether a conversion can run.
    pub converter: Converter,
    /// Adjustments the user asked for (a video with any is converted).
    pub options: ConvertOptions,
}

/// How the file reaches the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadAction {
    /// Sent as it is.
    AsIs {
        /// File size.
        bytes: u64,
    },
    /// Converted first; size and space are checked again on the output.
    Convert(TranscodeTarget),
}

/// An upload that passed its preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadPlan {
    /// Where the file goes.
    pub path: RemotePath,
    /// As is, or converted first.
    pub action: UploadAction,
    /// The stored file it replaces, which needs [`Confirm::Yes`].
    pub replaces: Option<FileEntry>,
}

/// Which way [`mib_text`] rounds a size that is not a whole tenth of a MiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    /// Down: a limit never reads larger than it is.
    Down,
    /// Up: a file over a limit never reads as equal to it.
    Up,
}

/// `bytes` in MiB as people read it: whole when exact (`25`), else with one
/// decimal (`30.2`).
pub fn mib_text(bytes: u64, rounding: Rounding) -> String {
    let (scaled, mib) = (u128::from(bytes) * 10, u128::from(MIB));
    let tenths = match rounding {
        Rounding::Down => scaled / mib,
        Rounding::Up => scaled.div_ceil(mib),
    };
    match tenths % 10 {
        0 => format!("{}", tenths / 10),
        tenth => format!("{}.{tenth}", tenths / 10),
    }
}

/// Refuses an empty file or one over `cap`, the screen's per-file limit
/// ([`UploadProfile::max_upload_bytes`]), always kept below the devices'
/// 2 GiB ceiling.
pub fn check_size(bytes: u64, cap: u64) -> Result<(), Refusal> {
    let limit = cap.min(DEVICE_SIZE_LIMIT - 1);
    if bytes == 0 {
        return Err(Refusal::EmptyFile);
    }
    if bytes > limit {
        return Err(Refusal::TooLarge { bytes, limit });
    }
    Ok(())
}

/// The preflight of an upload (D-2026-09-30-storage-video-3): the name, the
/// kind of folder, the screen's profile (converting a video when a converter
/// is available), the size limits, the card and the free space, in that
/// order. `stored` lists the files on the target medium (sizes optional);
/// it is only read: when the file does not fit, the refusal lists candidates
/// and nothing is ever deleted.
pub fn preflight(
    check: &UploadCheck<'_>,
    profile: &UploadProfile,
    info: &StorageInfo,
    stored: &[FileEntry],
) -> Result<UploadPlan, Refusal> {
    let name = FileName::for_upload(check.name).map_err(Refusal::InvalidName)?;
    let kind = check.location.kind;
    let found = check.media.kind();
    if found != Some(kind) {
        return Err(Refusal::WrongKind {
            location: kind,
            file: found,
        });
    }
    let medium = check.location.medium;
    let free = info.capacity(medium).ok_or(Refusal::NoCard)?.free;
    let action = choose_action(check, profile)?;
    let accepted = match &action {
        UploadAction::AsIs { .. } => check.media.format.extensions(),
        UploadAction::Convert(target) => target.format.extensions(),
    };
    if !name.extension().is_some_and(|e| accepted.contains(&e)) {
        return Err(Refusal::WrongExtension { name, accepted });
    }
    let path = RemotePath::new(check.location, name);
    if let UploadAction::AsIs { bytes } = action {
        check_size(bytes, profile.max_upload_bytes)?;
        if bytes >= free {
            let on_medium = stored.iter().filter(|e| e.path.location.medium == medium);
            return Err(Refusal::no_space(bytes, free, on_medium.cloned().collect()));
        }
    }
    let replaces = stored.iter().find(|e| e.path == path).cloned();
    Ok(UploadPlan {
        path,
        action,
        replaces,
    })
}

fn choose_action(
    check: &UploadCheck<'_>,
    profile: &UploadProfile,
) -> Result<UploadAction, Refusal> {
    let mismatches = profile.mismatches(check.location.kind, check.media);
    let as_is = UploadAction::AsIs {
        bytes: check.media.bytes,
    };
    match check.location.kind {
        MediaKind::Image if mismatches.is_empty() => Ok(as_is),
        MediaKind::Image => Err(Refusal::WrongProfile(mismatches)),
        MediaKind::Video if mismatches.is_empty() && check.options.is_identity() => Ok(as_is),
        MediaKind::Video => match check.converter {
            Converter::Available => Ok(UploadAction::Convert(
                profile.transcode_target(check.options),
            )),
            Converter::Missing => Err(Refusal::NeedsConverter(mismatches)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::geometry::Size;
    use crate::domain::media::MediaFormat;
    use crate::domain::media::tests::{mp4, profile, still};
    use crate::ports::ScreenStorage;

    const NATIVE: Size = Size::new(480, 1920);
    const INTERNAL_VIDEO: StorageLocation =
        StorageLocation::new(Medium::Internal, MediaKind::Video);

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).expect("path")
    }

    fn entry(text: &str, size: Option<u64>) -> FileEntry {
        FileEntry {
            path: path(text),
            size,
        }
    }

    fn info(free: u64, card: Option<u64>) -> StorageInfo {
        let capacity = |free| Capacity {
            total: 1_000_000_000,
            used: 1_000_000_000 - free,
            free,
        };
        StorageInfo {
            internal: capacity(free),
            card: card.map(capacity),
        }
    }

    fn check<'a>(
        name: &'a str,
        location: StorageLocation,
        media: &'a MediaInfo,
    ) -> UploadCheck<'a> {
        UploadCheck {
            name,
            location,
            media,
            converter: Converter::Missing,
            options: ConvertOptions::default(),
        }
    }

    fn run(
        check: &UploadCheck<'_>,
        info: &StorageInfo,
        stored: &[FileEntry],
    ) -> Result<UploadPlan, Refusal> {
        preflight(check, &profile("turing-8.8").unwrap(), info, stored)
    }

    /// The storage port's destructive methods, as the compiler sees them:
    /// both take the proof of a confirmation.
    type Delete =
        fn(&mut (dyn ScreenStorage + 'static), &RemotePath, Confirmed) -> crate::Result<()>;
    type SetStartMode =
        fn(&mut (dyn ScreenStorage + 'static), StartMode, Confirmed) -> crate::Result<()>;

    #[test]
    fn destructive_operations_require_confirm_yes() {
        let video = path("internal/video/clip.mp4");
        let operations = [
            (
                Operation::Delete(video.clone()),
                "deleting internal/video/clip.mp4",
            ),
            (
                Operation::Overwrite(video.clone()),
                "replacing internal/video/clip.mp4",
            ),
            (
                Operation::Boot(BootMedia::File(video.clone())),
                "making internal/video/clip.mp4 the boot media",
            ),
            (
                Operation::Boot(BootMedia::Default),
                "restoring the default boot screen",
            ),
        ];
        for (op, text) in &operations {
            let refused = Confirmed::require(Confirm::No, op).unwrap_err();
            assert_eq!(refused, BezelError::NotConfirmed((*text).to_string()));
            assert_eq!(refused.to_string(), format!("{text} needs confirmation"));
            assert!(Confirmed::require(Confirm::Yes, op).is_ok());
        }

        // Only `Confirmed::require` with `Confirm::Yes` makes a `Confirmed`
        // (its field is private), and the port's delete and start-mode
        // writes take one: no code path reaches them unconfirmed. These
        // coercions stop compiling if either signature loses the proof.
        let _: Delete = <dyn ScreenStorage>::delete;
        let _: SetStartMode = <dyn ScreenStorage>::set_start_mode;
        // That the use cases refuse before any byte reaches the screen
        // (overwrite included) runs through the device fake:
        // tests/storage.rs, `replacing_deleting_and_the_boot_slot_need_confirmation`.
    }

    #[test]
    fn preflight_rejects_bad_names_sizes_and_full_storage() {
        let clip = mp4(NATIVE, 1000);
        let roomy = info(500_000_000, None);
        let plan = run(&check("Clip.MP4", INTERNAL_VIDEO, &clip), &roomy, &[]).unwrap();
        assert_eq!(plan.path, path("internal/video/clip.mp4"));
        assert_eq!(plan.action, UploadAction::AsIs { bytes: 1000 });
        assert_eq!(plan.replaces, None);

        // Bad names.
        let long = format!("{}.mp4", "a".repeat(FileName::MAX_BYTES - 3));
        for (name, error) in [
            ("", NameError::Empty),
            ("my clip.mp4", NameError::Forbidden(' ')),
            ("../clip.mp4", NameError::Forbidden('/')),
            ("vídeo.mp4", NameError::Forbidden('í')),
            (".clip.mp4", NameError::LeadingDot),
            (
                long.as_str(),
                NameError::TooLong {
                    bytes: 209,
                    max: 208,
                },
            ),
        ] {
            let refused = run(&check(name, INTERNAL_VIDEO, &clip), &roomy, &[]);
            assert_eq!(refused, Err(Refusal::InvalidName(error)), "{name:?}");
        }
        let refused = run(&check("clip.mov", INTERNAL_VIDEO, &clip), &roomy, &[]);
        assert!(matches!(
            refused,
            Err(Refusal::WrongExtension {
                accepted: &["mp4"],
                ..
            })
        ));

        // Sizes: empty, over the 8.8"'s 25 MiB, over the devices' 2 GiB
        // ceiling.
        for (bytes, refusal) in [
            (0, Refusal::EmptyFile),
            (
                REV_C_MAX_UPLOAD_BYTES + 1,
                Refusal::TooLarge {
                    bytes: REV_C_MAX_UPLOAD_BYTES + 1,
                    limit: REV_C_MAX_UPLOAD_BYTES,
                },
            ),
            (
                DEVICE_SIZE_LIMIT,
                Refusal::TooLarge {
                    bytes: DEVICE_SIZE_LIMIT,
                    limit: REV_C_MAX_UPLOAD_BYTES,
                },
            ),
        ] {
            let media = mp4(NATIVE, bytes);
            assert_eq!(
                run(&check("clip.mp4", INTERNAL_VIDEO, &media), &roomy, &[]),
                Err(refusal)
            );
        }
        let at_the_cap = mp4(NATIVE, REV_C_MAX_UPLOAD_BYTES);
        assert!(run(&check("clip.mp4", INTERNAL_VIDEO, &at_the_cap), &roomy, &[]).is_ok());

        // Wrong profile: the resolution of a landscape export.
        let landscape = mp4(Size::new(1920, 480), 1000);
        let refused = run(&check("clip.mp4", INTERNAL_VIDEO, &landscape), &roomy, &[]);
        assert_eq!(
            refused,
            Err(Refusal::NeedsConverter(vec![Mismatch::Resolution {
                expected: NATIVE,
                found: Some(Size::new(1920, 480))
            }]))
        );
        let mut with_converter = check("clip.mp4", INTERNAL_VIDEO, &landscape);
        with_converter.converter = Converter::Available;
        let plan = run(&with_converter, &roomy, &[]).unwrap();
        assert!(matches!(plan.action, UploadAction::Convert(t) if t.size == NATIVE));

        // Wrong kind, no card.
        let png = still(MediaFormat::Png, 10);
        assert_eq!(
            run(&check("logo.png", INTERNAL_VIDEO, &png), &roomy, &[]),
            Err(Refusal::WrongKind {
                location: MediaKind::Video,
                file: Some(MediaKind::Image)
            })
        );
        let card_video = StorageLocation::new(Medium::Card, MediaKind::Video);
        assert_eq!(
            run(&check("clip.mp4", card_video, &clip), &roomy, &[]),
            Err(Refusal::NoCard)
        );

        // Full storage: refused with the files on that medium, largest first.
        let stored = [
            entry("internal/image/logo.png", Some(10)),
            entry("internal/video/old.mp4", Some(900)),
            entry("internal/video/unknown.mp4", None),
            entry("sd/video/elsewhere.mp4", Some(5000)),
        ];
        let full = info(1000, Some(10_000));
        let refused = run(&check("clip.mp4", INTERNAL_VIDEO, &clip), &full, &stored);
        let Err(Refusal::NoSpace {
            needed,
            free,
            candidates,
        }) = refused
        else {
            unreachable!("expected NoSpace, got {refused:?}");
        };
        assert_eq!((needed, free), (1000, 1000), "the vendor needs size < free");
        let order: Vec<String> = candidates.iter().map(|c| c.path.to_string()).collect();
        assert_eq!(
            order,
            [
                "internal/video/old.mp4",
                "internal/image/logo.png",
                "internal/video/unknown.mp4"
            ]
        );
        assert_eq!(stored.len(), 4, "the listing is only read");

        // Replacing is detected (and needs Confirm::Yes in the use case).
        let plan = run(&check("old.mp4", INTERNAL_VIDEO, &clip), &roomy, &stored).unwrap();
        assert_eq!(
            plan.replaces,
            Some(entry("internal/video/old.mp4", Some(900)))
        );
    }

    #[test]
    fn each_file_is_capped_at_the_profiles_limit() {
        // D-2026-09-30-release-polish-12: 25 MiB on rev C, the vendor's
        // 120 MB on TUR_USB, both refused before anything is sent.
        let roomy = info(500_000_000, None);
        let over_rev_c = REV_C_MAX_UPLOAD_BYTES + 1;
        let rev_c = run(
            &check("clip.mp4", INTERNAL_VIDEO, &mp4(NATIVE, over_rev_c)),
            &roomy,
            &[],
        );
        assert_eq!(
            rev_c,
            Err(Refusal::TooLarge {
                bytes: over_rev_c,
                limit: 26_214_400
            })
        );
        let usb = profile("turing-usb-8.8").unwrap();
        let stream = |bytes| {
            let mut media = mp4(NATIVE, bytes);
            media.format = MediaFormat::H264;
            media.video = media.video.map(|v| crate::domain::media::VideoTrack {
                b_frames: Some(false),
                ..v
            });
            media
        };
        let fits = stream(over_rev_c);
        let plan = preflight(
            &check("clip.h264", INTERNAL_VIDEO, &fits),
            &usb,
            &roomy,
            &[],
        );
        assert_eq!(
            plan.unwrap().action,
            UploadAction::AsIs { bytes: over_rev_c }
        );
        let over_usb = stream(MAX_UPLOAD_BYTES + 1);
        let refused = preflight(
            &check("clip.h264", INTERNAL_VIDEO, &over_usb),
            &usb,
            &roomy,
            &[],
        );
        assert_eq!(
            refused,
            Err(Refusal::TooLarge {
                bytes: MAX_UPLOAD_BYTES + 1,
                limit: MAX_UPLOAD_BYTES
            })
        );
        // A conversion is planned with its output capped at the limit; the
        // use case checks the output's real size again.
        let wide = mp4(Size::new(1920, 1080), 1);
        let mut convert = check("clip.mp4", INTERNAL_VIDEO, &wide);
        convert.converter = Converter::Available;
        let plan = run(&convert, &roomy, &[]).unwrap();
        let UploadAction::Convert(target) = plan.action else {
            unreachable!("expected a conversion");
        };
        assert_eq!(target.max_bytes, Some(REV_C_MAX_UPLOAD_BYTES));
        // A cap above the 2 GiB the screens can parse is held below it.
        assert_eq!(
            check_size(DEVICE_SIZE_LIMIT, u64::MAX),
            Err(Refusal::TooLarge {
                bytes: DEVICE_SIZE_LIMIT,
                limit: DEVICE_SIZE_LIMIT - 1
            })
        );
    }

    #[test]
    fn sizes_read_in_mib() {
        assert_eq!(mib_text(REV_C_MAX_UPLOAD_BYTES, Rounding::Down), "25");
        assert_eq!(mib_text(REV_C_MAX_UPLOAD_BYTES + 1, Rounding::Up), "25.1");
        assert_eq!(mib_text(REV_C_MAX_UPLOAD_BYTES + 1, Rounding::Down), "25");
        assert_eq!(mib_text(MAX_UPLOAD_BYTES, Rounding::Down), "114.4");
        assert_eq!(mib_text(29_577_216, Rounding::Up), "28.3");
        assert_eq!(mib_text(0, Rounding::Up), "0");
        assert_eq!(mib_text(u64::MAX, Rounding::Down), "17592186044415.9");
        assert_eq!(mib_text(u64::MAX, Rounding::Up), "17592186044416");
    }

    #[test]
    fn images_upload_as_they_are_and_adjusted_videos_convert() {
        let roomy = info(500_000_000, Some(500_000_000));
        let jpeg = still(MediaFormat::Jpeg, 10);
        let image_folder = StorageLocation::new(Medium::Card, MediaKind::Image);
        let plan = run(&check("logo.jpeg", image_folder, &jpeg), &roomy, &[]).unwrap();
        assert_eq!(plan.action, UploadAction::AsIs { bytes: 10 });
        let mut narrow = profile("turing-8.8").unwrap();
        narrow.image_formats = &[MediaFormat::Png];
        let refused = preflight(
            &check("logo.jpg", image_folder, &jpeg),
            &narrow,
            &roomy,
            &[],
        );
        assert!(matches!(refused, Err(Refusal::WrongProfile(_))));
        let unknown = still(MediaFormat::Other, 10);
        assert_eq!(
            run(&check("x.webp", image_folder, &unknown), &roomy, &[]),
            Err(Refusal::WrongKind {
                location: MediaKind::Image,
                file: None
            })
        );

        // An in-profile video with a requested rotation is converted.
        let clip = mp4(NATIVE, 1000);
        let mut rotated = check("clip.mp4", INTERNAL_VIDEO, &clip);
        rotated.options.quarter_turns = 1;
        assert_eq!(
            run(&rotated, &roomy, &[]),
            Err(Refusal::NeedsConverter(Vec::new()))
        );
        rotated.converter = Converter::Available;
        let plan = run(&rotated, &roomy, &[]).unwrap();
        let UploadAction::Convert(target) = plan.action else {
            unreachable!("expected a conversion");
        };
        assert_eq!(target.quarter_turns, 1);
        assert_eq!(target.format, MediaFormat::Mp4);
    }

    #[test]
    fn names_paths_and_locations() {
        assert_eq!(
            FileName::parse("My Clip.MP4").unwrap().as_str(),
            "My Clip.MP4"
        );
        assert_eq!(FileName::parse(".."), Err(NameError::Reserved));
        assert_eq!(FileName::parse(""), Err(NameError::Empty));
        assert_eq!(FileName::parse("a/b"), Err(NameError::Forbidden('/')));
        assert_eq!(FileName::parse("a\tb"), Err(NameError::Forbidden('\t')));
        assert!(
            FileName::parse("._clip.mp4").is_ok(),
            "card junk stays addressable"
        );
        assert_eq!(FileName::for_upload("A-b_c.D").unwrap().as_str(), "a-b_c.d");
        assert_eq!(FileName::for_upload("noext").unwrap().extension(), None);
        assert_eq!(FileName::suggest("???.png", "PNG").as_str(), "media.png");
        assert_eq!(FileName::suggest(".bashrc", "").as_str(), "bashrc");
        let long = FileName::suggest(&"x".repeat(400), "mp4");
        assert_eq!(long.as_str().len(), FileName::MAX_BYTES);
        assert!(FileName::for_upload(long.as_str()).is_ok());
        assert_eq!(FileName::MAX_BYTES + LONGEST_ROOT_BYTES, MAX_PATH_BYTES);

        let p = path("sd/image/Logo.PNG");
        assert_eq!(
            p.location,
            StorageLocation::new(Medium::Card, MediaKind::Image)
        );
        assert_eq!(p.to_string(), "sd/image/Logo.PNG");
        for bad in [
            "usb/video/a.mp4",
            "sd/audio/a.mp3",
            "sd/video",
            "sd/video/a/b.mp4",
        ] {
            assert!(
                matches!(RemotePath::parse(bad), Err(BezelError::InvalidInput(_))),
                "{bad}"
            );
        }
        assert_eq!(StorageLocation::ALL.len(), 4);
        assert_eq!(StorageLocation::ALL[3].to_string(), "sd/video");
        assert_eq!(Medium::from_slug("internal"), Some(Medium::Internal));
        assert_eq!(Medium::from_slug("usb"), None);
        assert_eq!(info(5, None).capacity(Medium::Card), None);
        assert_eq!(BootMedia::File(p).start_mode(), StartMode::Image);
        assert_eq!(
            BootMedia::File(path("sd/video/a.mp4")).start_mode(),
            StartMode::Video
        );
        assert_eq!(BootMedia::Default.start_mode(), StartMode::Default);
    }

    #[test]
    fn refusals_explain_themselves() {
        let texts = [
            (
                Refusal::InvalidName(NameError::Forbidden(' ')),
                "invalid file name: ' ' is not allowed",
            ),
            (
                Refusal::InvalidName(NameError::TooLong {
                    bytes: 300,
                    max: 208,
                }),
                "300 bytes",
            ),
            (Refusal::InvalidName(NameError::Reserved), "'.' and '..'"),
            (Refusal::InvalidName(NameError::Empty), "empty"),
            (
                Refusal::InvalidName(NameError::LeadingDot),
                "start with '.'",
            ),
            (
                Refusal::WrongExtension {
                    name: FileName::for_upload("a.jpeg").unwrap(),
                    accepted: &["png"],
                },
                "a.jpeg must end in .png",
            ),
            (
                Refusal::WrongKind {
                    location: MediaKind::Video,
                    file: Some(MediaKind::Image),
                },
                "an image cannot go into a video folder",
            ),
            (
                Refusal::WrongKind {
                    location: MediaKind::Image,
                    file: Some(MediaKind::Video),
                },
                "a video cannot go into an image folder",
            ),
            (
                Refusal::WrongKind {
                    location: MediaKind::Video,
                    file: None,
                },
                "not an image or a video",
            ),
            (
                Refusal::WrongProfile(vec![Mismatch::Audio, Mismatch::BFrames]),
                "audio track; the video may use B-frames",
            ),
            (
                Refusal::NeedsConverter(vec![Mismatch::Audio]),
                "must be converted (the file has an audio track)",
            ),
            (Refusal::NeedsConverter(Vec::new()), "requested adjustments"),
            (Refusal::EmptyFile, "empty"),
            (
                Refusal::TooLarge {
                    bytes: 30 * MIB + MIB / 5,
                    limit: REV_C_MAX_UPLOAD_BYTES,
                },
                "the file is 30.2 MiB, over the screen's 25 MiB limit per file",
            ),
            (
                Refusal::ConvertedTooLarge {
                    bytes: REV_C_MAX_UPLOAD_BYTES + 1,
                    limit: REV_C_MAX_UPLOAD_BYTES,
                },
                "the converted video is 25.1 MiB, over the screen's 25 MiB limit per file; \
                 use a shorter clip or a lower frame rate",
            ),
            (Refusal::NoCard, "no memory card"),
            (
                Refusal::no_space(10, 5, Vec::new()),
                "10 bytes do not fit in the 5 bytes free",
            ),
        ];
        for (refusal, text) in texts {
            assert!(refusal.to_string().contains(text), "{refusal} / {text}");
        }
        let err = BezelError::Refused(Refusal::NoCard);
        assert_eq!(err.to_string(), "refused: the screen has no memory card");
    }
}
