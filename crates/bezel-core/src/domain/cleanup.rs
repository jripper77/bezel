//! The cleanup assistant (D-2026-09-30-storage-manager-3, -9): which files
//! on a screen are likely leftovers, as findings with stable reason codes.
//!
//! [`findings`] is pure: it reads a listing, the screen's catalog record and
//! the [`Protected`] files, and deletes nothing. Only exact signals are
//! pre-checked (a vendor name artifact of equal size, the rev C hang partial,
//! the file of an interrupted Bezel upload); anything merely probable is
//! listed, never chosen. Files Bezel sent and verified get no finding, and
//! the protected ones (the boot media Bezel set, every video a theme plays)
//! never appear.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::archive::{EntryState, Listing, ScreenRecord};
use super::media::{MediaKind, UploadProfile, video_name};
use super::storage::{FileEntry, RemotePath, StorageLocation};
use super::theme::AssetRef;

/// Where a rev C screen stops reading an upload and hangs
/// (D-2026-09-30-release-polish-12): a file of exactly this many bytes is
/// what such an upload left.
pub const HANG_PARTIAL_BYTES: u64 = 29_577_216;

/// Files the cleanup assistant never suggests and whose move or rename
/// warns: the boot media Bezel recorded and every video a library theme or
/// the live theme plays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Protected {
    boot: Option<RemotePath>,
    videos: BTreeSet<String>,
    /// The names of framed copies, their fingerprint zeroed.
    framed: BTreeSet<String>,
}

/// `name` (lower-case) with the fingerprint of a framed copy zeroed: the 8
/// hex digits after the last `_f` before the extension
/// (`amd_90_f1a2b3c4.mp4`: `amd_90_f00000000.mp4`); `None` without one.
fn zeroed_fingerprint(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    let (base, hex) = stem.rsplit_once("_f")?;
    let hex_digit = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    (hex.len() == 8 && hex.bytes().all(hex_digit)).then(|| format!("{base}_f00000000.{ext}"))
}

impl Protected {
    /// Protects the boot media Bezel recorded (`None`: none).
    pub fn new(boot: Option<RemotePath>) -> Self {
        Self {
            boot,
            videos: BTreeSet::new(),
            framed: BTreeSet::new(),
        }
    }

    /// Protects a theme's video `asset` under every name a screen of
    /// `profile` may store it as: [`super::media::device_video_name`] in all
    /// four turns, plain or with any framing (`<name>_f<8 hex>`), in the
    /// video folder of either medium.
    pub fn theme_video(&mut self, asset: &AssetRef, profile: &UploadProfile) {
        // Device-only themes reference an exact stored path, without a local asset.
        if let Some(raw) = asset.0.strip_prefix("screen://")
            && let Ok(path) = RemotePath::parse(raw)
            && path.location.kind == MediaKind::Video
        {
            self.videos.insert(path.name.as_str().to_ascii_lowercase());
            return;
        }
        for turns in 0..4 {
            let plain = video_name(asset, turns, None, profile);
            self.videos.insert(plain.as_str().to_ascii_lowercase());
            let framed = video_name(asset, turns, Some(0), profile);
            self.framed.insert(framed.as_str().to_ascii_lowercase());
        }
    }

    /// Whether `path` is the recorded boot media (letter case aside).
    pub fn is_boot(&self, path: &RemotePath) -> bool {
        let same = |boot: &RemotePath| {
            boot.location == path.location
                && boot.name.as_str().eq_ignore_ascii_case(path.name.as_str())
        };
        self.boot.as_ref().is_some_and(same)
    }

    /// Whether `path` is a video a theme plays (letter case aside).
    pub fn is_theme_video(&self, path: &RemotePath) -> bool {
        let name = path.name.as_str().to_ascii_lowercase();
        let framed = || zeroed_fingerprint(&name).is_some_and(|n| self.framed.contains(&n));
        path.location.kind == MediaKind::Video && (self.videos.contains(&name) || framed())
    }

    /// Whether `path` is protected.
    pub fn covers(&self, path: &RemotePath) -> bool {
        self.is_boot(path) || self.is_theme_video(path)
    }
}

/// The stable reason code of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// A vendor name artifact of the same size as a shorter name: pre-checked.
    Duplicate,
    /// Exactly [`HANG_PARTIAL_BYTES`]: pre-checked.
    HangPartial,
    /// The file of a `pending` catalog entry: pre-checked.
    Pending,
    /// A vendor name artifact of another size: only listed.
    Variant,
    /// Another size than the `stored` catalog entry at its place: only listed.
    SizeDiffers,
    /// Exactly the size and kind of another file: only listed.
    SameSize,
    /// No theme plays it: only listed.
    Unused,
}

impl Code {
    /// Every code.
    pub const ALL: [Code; 7] = [
        Code::Duplicate,
        Code::HangPartial,
        Code::Pending,
        Code::Variant,
        Code::SizeDiffers,
        Code::SameSize,
        Code::Unused,
    ];

    /// The code as the studio and the CLI name it.
    pub const fn slug(self) -> &'static str {
        match self {
            Code::Duplicate => "duplicate",
            Code::HangPartial => "hangPartial",
            Code::Pending => "pending",
            Code::Variant => "variant",
            Code::SizeDiffers => "sizeDiffers",
            Code::SameSize => "sameSize",
            Code::Unused => "unused",
        }
    }

    /// Whether findings of this code start checked: only exact signals.
    pub const fn prechecked(self) -> bool {
        matches!(self, Code::Duplicate | Code::HangPartial | Code::Pending)
    }
}

/// Why a file is suggested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// A vendor name artifact with the size of `kept`, the shortest name of
    /// that size in its group.
    Duplicate {
        /// The file that stays.
        kept: RemotePath,
    },
    /// What a rev C upload that hung left.
    HangPartial,
    /// The file of an interrupted Bezel upload, or of one that failed its
    /// size check.
    Pending,
    /// A vendor name artifact of `kept` (the shortest name of its group)
    /// with another size.
    Variant {
        /// The file that stays.
        kept: RemotePath,
    },
    /// Not the size Bezel stored at this place.
    SizeDiffers {
        /// The cataloged size.
        cataloged: u64,
    },
    /// Exactly the size and kind of `kept`.
    SameSize {
        /// The file of that size with the shortest name.
        kept: RemotePath,
    },
    /// No theme plays it.
    Unused,
}

impl Reason {
    /// The reason's code.
    pub const fn code(&self) -> Code {
        match self {
            Reason::Duplicate { .. } => Code::Duplicate,
            Reason::HangPartial => Code::HangPartial,
            Reason::Pending => Code::Pending,
            Reason::Variant { .. } => Code::Variant,
            Reason::SizeDiffers { .. } => Code::SizeDiffers,
            Reason::SameSize { .. } => Code::SameSize,
            Reason::Unused => Code::Unused,
        }
    }
}

/// A file the assistant suggests deleting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file as listed.
    pub file: FileEntry,
    /// Why.
    pub reason: Reason,
}

impl Finding {
    /// The reason's code.
    pub const fn code(&self) -> Code {
        self.reason.code()
    }

    /// Whether the finding starts checked.
    pub const fn prechecked(&self) -> bool {
        self.code().prechecked()
    }
}

/// The `x.ext` a vendor name artifact stands for, lower-cased: the vendor
/// app's re-conversions name a file `x.ext.ext...` or `x.ext<digits>.ext`
/// (`NVI.mp427034822.mp4`: `nvi.mp4`). `None` for any other name
/// (`8.8APEX_2.mp4`, `m04.mp4`).
pub fn artifact_base(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let (stem, ext) = lower.rsplit_once('.')?;
    if ext.is_empty() {
        return None;
    }
    let suffix = format!(".{ext}");
    let mut core = stem;
    while let Some(at) = core.rfind(&suffix) {
        let tail = &core[at + suffix.len()..];
        if at == 0 || !tail.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        core = &core[..at];
    }
    (core.len() < stem.len()).then(|| format!("{core}{suffix}"))
}

/// What the catalog alone says of a listed file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// Bezel sent it and verified it, or it is protected: no finding; it
    /// stays.
    Own,
    /// A finding.
    Found(Reason),
    /// Nothing yet.
    Open,
}

fn settle(verdict: &mut Verdict, reason: Reason) {
    if *verdict == Verdict::Open {
        *verdict = Verdict::Found(reason);
    }
}

/// Whether `verdict` already starts checked (an unfinished Bezel upload,
/// the hang partial): such a file never stays for another.
fn checked(verdict: &Verdict) -> bool {
    matches!(verdict, Verdict::Found(reason) if reason.code().prechecked())
}

fn from_catalog(file: &FileEntry, record: Option<&ScreenRecord>, card: Option<u64>) -> Verdict {
    let entry = record.and_then(|r| r.entry(&file.path, card));
    match entry {
        Some(e) if e.state != EntryState::Pending && file.size.is_none_or(|s| s == e.size) => {
            Verdict::Own
        }
        _ if file.size == Some(HANG_PARTIAL_BYTES) => Verdict::Found(Reason::HangPartial),
        Some(e) if e.state == EntryState::Pending => Verdict::Found(Reason::Pending),
        Some(e) => Verdict::Found(Reason::SizeDiffers { cataloged: e.size }),
        None => Verdict::Open,
    }
}

/// Shortest name first, then by path.
fn shorter(a: &FileEntry, b: &FileEntry) -> Ordering {
    let len = |f: &FileEntry| f.path.name.as_str().len();
    len(a).cmp(&len(b)).then_with(|| a.path.cmp(&b.path))
}

/// Groups each folder's vendor name artifacts with the name they stand for;
/// in a group, equal sizes are duplicates of the shortest name of that size
/// and other sizes variants of the group's shortest name.
fn name_groups(files: &[FileEntry], verdicts: &mut [Verdict]) {
    let mut groups: BTreeMap<(StorageLocation, String), Vec<usize>> = BTreeMap::new();
    let mut with_artifact = BTreeSet::new();
    for (i, file) in files.iter().enumerate() {
        let name = file.path.name.as_str();
        let base = artifact_base(name);
        let key = (
            file.path.location,
            base.clone().unwrap_or_else(|| name.to_ascii_lowercase()),
        );
        if base.is_some() {
            with_artifact.insert(key.clone());
        }
        groups.entry(key).or_default().push(i);
    }
    for (key, mut members) in groups {
        if members.len() > 1 && with_artifact.contains(&key) {
            members.sort_by(|a, b| shorter(&files[*a], &files[*b]));
            judge_group(files, &members, verdicts);
        }
    }
}

/// `members`: one name group, shortest name first. The file that stays (for
/// the group, or for the others of its size) is never one that already
/// starts checked: otherwise the assistant would check every copy of the
/// same bytes.
fn judge_group(files: &[FileEntry], members: &[usize], verdicts: &mut [Verdict]) {
    let open: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&i| !checked(&verdicts[i]))
        .collect();
    let Some((&kept, _)) = open.split_first() else {
        return;
    };
    let mut by_size: BTreeMap<u64, usize> = BTreeMap::new();
    for &i in &open {
        let size = files[i].size;
        let reason = match size.and_then(|s| by_size.get(&s).copied()) {
            Some(first) => Reason::Duplicate {
                kept: files[first].path.clone(),
            },
            None => {
                if let Some(size) = size {
                    by_size.insert(size, i);
                }
                if i == kept {
                    continue;
                }
                Reason::Variant {
                    kept: files[kept].path.clone(),
                }
            }
        };
        settle(&mut verdicts[i], reason);
    }
}

/// Lists the files of exactly the size and kind of a shorter-named one.
fn same_sizes(files: &[FileEntry], verdicts: &mut [Verdict]) {
    let mut classes: BTreeMap<(MediaKind, u64), Vec<usize>> = BTreeMap::new();
    for (i, file) in files.iter().enumerate() {
        if let Some(size) = file.size {
            let kind = file.path.location.kind;
            classes.entry((kind, size)).or_default().push(i);
        }
    }
    for mut members in classes.into_values().filter(|m| m.len() > 1) {
        members.sort_by(|a, b| shorter(&files[*a], &files[*b]));
        let kept = files[members[0]].path.clone();
        for &i in &members[1..] {
            let kept = kept.clone();
            settle(&mut verdicts[i], Reason::SameSize { kept });
        }
    }
}

/// The cleanup findings for one screen (D-2026-09-30-storage-manager-9), in
/// the listing's order: at most one per file, the strongest reason first
/// (hang partial, pending, size differs, duplicate, variant, same size,
/// unused). `record` is the screen's catalog record (`None`: Bezel sent it
/// nothing); its verified files and the `protected` ones get no finding,
/// though they still count as the name or size another file repeats.
pub fn findings(
    listing: &Listing,
    record: Option<&ScreenRecord>,
    protected: &Protected,
) -> Vec<Finding> {
    let files = &listing.files;
    let verdict = |f: &FileEntry| {
        if protected.covers(&f.path) {
            Verdict::Own
        } else {
            from_catalog(f, record, listing.card)
        }
    };
    let mut verdicts: Vec<Verdict> = files.iter().map(verdict).collect();
    name_groups(files, &mut verdicts);
    same_sizes(files, &mut verdicts);
    files
        .iter()
        .zip(verdicts)
        .filter_map(|(file, verdict)| {
            let reason = match verdict {
                Verdict::Own => return None,
                Verdict::Found(reason) => reason,
                Verdict::Open => Reason::Unused,
            };
            Some(Finding {
                file: file.clone(),
                reason,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::archive::{ArchiveEntry, ContentId};
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;
    use crate::domain::storage::Medium;

    /// `bezel storage ls --json` on the user's 8.8" (2026-09-30), exact
    /// bytes: the vendor app's files on the card. Each re-conversion by the
    /// vendor app made a file of another size.
    const USER_CARD: &[(&str, u64)] = &[
        ("demon_open.mp4.mp4.mp4", 25_483_784),
        ("demon.mp4.mp4.mp4", 13_237_564),
        ("demon.mp401115025.mp4", 13_257_991),
        ("8.8APEX_2.mp4", 2_259_535),
        ("demon_open.mp4.mp4", 25_800_984),
        ("AMD.mp4", 4_079_432),
        ("NVI.mp427034822.mp4", 5_352_433),
        ("NVI.mp4", 5_680_675),
        ("Rani.mp4", 6_007_182),
        ("m04.mp4", 876_578),
        ("Rani.mp417075004.mp4", 5_646_986),
        ("m04.mp424045157.mp4", 841_053),
    ];

    /// The same listing's internal videos (sizes as listed, rounded).
    const USER_INTERNAL: &[(&str, u64)] = &[
        ("earth.mp4", 2_516_582),
        ("DARIUS.mp4", 7_444_889),
        ("jyanme.mp4", 4_404_019),
        ("dragon.mp4", 2_621_440),
        ("aniya.mp4", 3_040_870),
    ];

    /// The card's capacity: 29.7 GiB.
    const USER_CARD_BYTES: u64 = 31_890_132_172;

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).expect("path")
    }

    fn file(text: &str, size: u64) -> FileEntry {
        FileEntry {
            path: path(text),
            size: Some(size),
        }
    }

    fn user_listing() -> Listing {
        let internal = USER_INTERNAL
            .iter()
            .map(|(n, s)| (format!("internal/video/{n}"), *s));
        let card = USER_CARD.iter().map(|(n, s)| (format!("sd/video/{n}"), *s));
        Listing {
            files: internal.chain(card).map(|(p, s)| file(&p, s)).collect(),
            card: Some(USER_CARD_BYTES),
        }
    }

    fn codes(found: &[Finding]) -> Vec<(String, &'static str)> {
        let code = |f: &Finding| (f.file.path.to_string(), f.code().slug());
        found.iter().map(code).collect()
    }

    #[test]
    fn finds_the_users_vendor_duplicates_and_the_hang_partial() {
        let mut listing = user_listing();
        listing
            .files
            .push(file("sd/video/bezel_test_cancel.mp4", 29_577_216));
        let found = findings(&listing, None, &Protected::default());

        // The five vendor groups: each artifact is listed as a variant of
        // the group's shortest name, which stays.
        let variants: Vec<(&str, &str)> = found
            .iter()
            .filter_map(|f| match &f.reason {
                Reason::Variant { kept } => Some((f.file.path.name.as_str(), kept.name.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            variants,
            [
                ("demon_open.mp4.mp4.mp4", "demon_open.mp4.mp4"),
                ("demon.mp401115025.mp4", "demon.mp4.mp4.mp4"),
                ("NVI.mp427034822.mp4", "NVI.mp4"),
                ("Rani.mp417075004.mp4", "Rani.mp4"),
                ("m04.mp424045157.mp4", "m04.mp4"),
            ]
        );
        let groups: BTreeSet<String> = found
            .iter()
            .filter_map(|f| artifact_base(f.file.path.name.as_str()))
            .collect();
        let expected = [
            "demon.mp4",
            "demon_open.mp4",
            "m04.mp4",
            "nvi.mp4",
            "rani.mp4",
        ];
        assert_eq!(groups, expected.map(String::from).into());

        // No pair has equal sizes, so nothing of the user's is pre-checked:
        // only the partial the firmware hang left.
        let checked: Vec<Finding> = found.iter().filter(|f| f.prechecked()).cloned().collect();
        assert_eq!(
            codes(&checked),
            [("sd/video/bezel_test_cancel.mp4".to_string(), "hangPartial")]
        );
        assert!(found.iter().all(|f| f.code() != Code::Duplicate));

        // `8.8APEX_2.mp4` and `AMD.mp4` are no artifacts; the rest is only
        // listed as unused, nothing is left out.
        for name in [
            "8.8APEX_2.mp4",
            "AMD.mp4",
            "NVI.mp4",
            "m04.mp4",
            "demon.mp4.mp4.mp4",
        ] {
            assert_eq!(
                artifact_base(name).is_some(),
                name.starts_with("demon"),
                "{name}"
            );
            let finding = found.iter().find(|f| f.file.path.name.as_str() == name);
            assert_eq!(finding.map(Finding::code), Some(Code::Unused), "{name}");
        }
        assert_eq!(found.len(), listing.files.len());
        let internal = found
            .iter()
            .filter(|f| f.file.path.location.medium == Medium::Internal);
        assert!(internal.into_iter().all(|f| f.code() == Code::Unused));
    }

    #[test]
    fn never_suggests_the_boot_media_or_a_theme_video() {
        let profile = model_by_id(ModelId("turing-8.8")).and_then(UploadProfile::for_model);
        let mut protected = Protected::new(Some(path("sd/video/boot.mp4")));
        protected.theme_video(
            &AssetRef("assets/AMD.mp4".into()),
            &profile.expect("profile"),
        );
        let listing = Listing {
            files: vec![
                // The boot media, though it has the hang partial's size.
                file("sd/video/BOOT.mp4", HANG_PARTIAL_BYTES),
                // The theme's video in all four turns, on either medium.
                file("internal/video/amd.mp4", 100),
                file("sd/video/amd_90.mp4", 200),
                file("internal/video/amd_180.mp4", 300),
                file("sd/video/AMD_270.mp4", 400),
                file("sd/video/amd.mp4", 100),
                // Repeats of them are still found, and kept beside them.
                file("internal/video/amd.mp4.mp4", 100),
                file("sd/video/amd_90.mp4.mp4", 201),
                // An image of a theme video's name is no theme video.
                file("internal/image/amd.mp4", 7),
            ],
            card: Some(USER_CARD_BYTES),
        };
        let mut record = ScreenRecord::default();
        record.record(ArchiveEntry::pending(
            path("sd/video/amd_90.mp4"),
            listing.card,
            200,
            ContentId::from_digest([1; 32]),
            1,
        ));
        let found = findings(&listing, Some(&record), &protected);
        assert!(found.iter().all(|f| !protected.covers(&f.file.path)));
        assert_eq!(
            codes(&found),
            [
                ("internal/video/amd.mp4.mp4".to_string(), "duplicate"),
                ("sd/video/amd_90.mp4.mp4".to_string(), "variant"),
                ("internal/image/amd.mp4".to_string(), "unused"),
            ]
        );
        assert_eq!(
            found[0].reason,
            Reason::Duplicate {
                kept: path("internal/video/amd.mp4")
            }
        );
        assert!(protected.is_boot(&path("sd/video/boot.mp4")));
        assert!(!protected.is_boot(&path("internal/video/boot.mp4")));
        assert!(!Protected::default().covers(&path("sd/video/boot.mp4")));
    }

    #[test]
    fn exact_device_theme_videos_are_protected_without_bezel_names() {
        let profile = model_by_id(ModelId("turing-8.8"))
            .and_then(UploadProfile::for_model)
            .unwrap();
        let mut protected = Protected::default();
        protected.theme_video(&AssetRef("screen://sd/video/AMD.mp4".into()), &profile);
        assert!(protected.is_theme_video(&path("sd/video/amd.mp4")));
        assert!(!protected.is_theme_video(&path("sd/video/bezel_AMD.mp4")));
        assert!(!protected.is_theme_video(&path("sd/image/amd.mp4")));
    }

    #[test]
    fn framed_copies_of_a_theme_video_are_protected_too() {
        let profile = model_by_id(ModelId("turing-8.8")).and_then(UploadProfile::for_model);
        let mut protected = Protected::default();
        protected.theme_video(
            &AssetRef("assets/dragon.mp4".into()),
            &profile.expect("profile"),
        );
        // D-2026-10-01-video-background-framing-4: a re-framed video has a
        // file of its own and Bezel deletes none.
        for kept in [
            "internal/video/dragon.mp4",
            "internal/video/dragon_f8ec2b24d.mp4",
            "sd/video/DRAGON_270_F1A2B3C4D.mp4",
            "sd/video/dragon_90_f00000000.mp4",
        ] {
            assert!(protected.is_theme_video(&path(kept)), "{kept}");
        }
        for other in [
            "internal/video/dragon_f8ec2b24.mp4",
            "internal/video/dragon_f8ec2b24dd.mp4",
            "internal/video/dragon_fzzzzzzzz.mp4",
            "internal/video/dragon_45_f8ec2b24d.mp4",
            "internal/video/dragonball_f8ec2b24d.mp4",
            "internal/video/dragon_f8ec2b24d.h264",
            "internal/image/dragon_f8ec2b24d.mp4",
            "internal/video/dragon",
        ] {
            assert!(!protected.is_theme_video(&path(other)), "{other}");
        }
        let listing = Listing {
            files: vec![
                file("internal/video/dragon.mp4", 2_588_343),
                file("internal/video/dragon_f8ec2b24d.mp4", 3_000_000),
                file("internal/video/m04.mp4", 876_578),
            ],
            card: None,
        };
        let found = findings(&listing, None, &protected);
        assert_eq!(
            codes(&found),
            [("internal/video/m04.mp4".to_string(), "unused")]
        );
    }

    #[test]
    fn only_exact_signals_are_prechecked() {
        let mut record = ScreenRecord::default();
        let entry = |text: &str, size, state| {
            let content = ContentId::from_digest([size as u8; 32]);
            let mut entry = ArchiveEntry::pending(path(text), Some(9), size, content, 1);
            entry.state = state;
            entry
        };
        record.record(entry("sd/video/mine.mp4", 50, EntryState::Stored));
        record.record(entry("sd/video/changed.mp4", 60, EntryState::Stored));
        record.record(entry("sd/video/half.mp4", 70, EntryState::Pending));
        record.record(entry("sd/video/gone.mp4", 80, EntryState::Deleted));
        let listing = Listing {
            files: vec![
                file("sd/video/mine.mp4", 50),
                file("sd/video/mine.mp4.mp4", 50),
                file("sd/video/changed.mp4", 61),
                file("sd/video/half.mp4", 12),
                file("sd/video/gone.mp4", 80),
                file("sd/video/x.mp4", 10),
                file("sd/video/x.mp4.mp4", 10),
                file("sd/video/x.mp4.mp4.mp4", 10),
                file("sd/video/x.mp41.mp4", 11),
                file("sd/video/x.mp42.mp4", 11),
                FileEntry {
                    path: path("sd/video/x.mp43.mp4"),
                    size: None,
                },
                file("internal/video/copy_of_changed.mp4", 61),
                file("internal/image/copy.png", 61),
            ],
            card: Some(9),
        };
        let found = findings(&listing, Some(&record), &Protected::default());
        let expect = [
            ("sd/video/mine.mp4.mp4", "duplicate"),
            ("sd/video/changed.mp4", "sizeDiffers"),
            ("sd/video/half.mp4", "pending"),
            ("sd/video/gone.mp4", "unused"),
            ("sd/video/x.mp4", "unused"),
            ("sd/video/x.mp4.mp4", "duplicate"),
            ("sd/video/x.mp4.mp4.mp4", "duplicate"),
            ("sd/video/x.mp41.mp4", "variant"),
            ("sd/video/x.mp42.mp4", "duplicate"),
            ("sd/video/x.mp43.mp4", "variant"),
            ("internal/video/copy_of_changed.mp4", "sameSize"),
            ("internal/image/copy.png", "unused"),
        ];
        let expect: Vec<(String, &str)> = expect.iter().map(|(p, c)| (p.to_string(), *c)).collect();
        assert_eq!(codes(&found), expect, "Bezel's own verified file gets none");
        let reason = |p: &str| {
            found
                .iter()
                .find(|f| f.file.path == path(p))
                .map(|f| &f.reason)
        };
        assert_eq!(
            reason("sd/video/x.mp42.mp4"),
            Some(&Reason::Duplicate {
                kept: path("sd/video/x.mp41.mp4")
            })
        );
        assert_eq!(
            reason("internal/video/copy_of_changed.mp4"),
            Some(&Reason::SameSize {
                kept: path("sd/video/changed.mp4")
            })
        );
        assert_eq!(
            reason("sd/video/changed.mp4"),
            Some(&Reason::SizeDiffers { cataloged: 60 })
        );
        let slugs = Code::ALL.map(Code::slug);
        assert_eq!(
            slugs,
            [
                "duplicate",
                "hangPartial",
                "pending",
                "variant",
                "sizeDiffers",
                "sameSize",
                "unused"
            ]
        );
        let prechecked: Vec<&str> = Code::ALL
            .into_iter()
            .filter(|c| c.prechecked())
            .map(Code::slug)
            .collect();
        assert_eq!(prechecked, ["duplicate", "hangPartial", "pending"]);
        assert_eq!(Reason::HangPartial.code(), Code::HangPartial);
    }

    /// The files `found` pre-checks.
    fn checked_paths(found: &[Finding]) -> Vec<String> {
        let checked = found.iter().filter(|f| f.prechecked());
        checked.map(|f| f.file.path.to_string()).collect()
    }

    #[test]
    fn the_file_a_group_keeps_is_never_prechecked() {
        // An unfinished Bezel upload (pre-checked) whose bytes the vendor
        // app repeated: one copy of them stays unchecked.
        let pending = |text: &str| {
            let content = ContentId::from_digest([7; 32]);
            ArchiveEntry::pending(path(text), Some(9), 100, content, 1)
        };
        // Two artifact names of equal size next to their base file.
        let mut record = ScreenRecord::default();
        record.record(pending("sd/video/clip.mp4"));
        let listing = Listing {
            files: vec![
                file("sd/video/clip.mp4", 100),
                file("sd/video/clip.mp4.mp4", 100),
                file("sd/video/clip.mp41.mp4", 100),
            ],
            card: Some(9),
        };
        let found = findings(&listing, Some(&record), &Protected::default());
        assert_eq!(
            checked_paths(&found),
            ["sd/video/clip.mp4", "sd/video/clip.mp41.mp4"]
        );
        let at = |p: &str| found.iter().find(|f| f.file.path == path(p));
        assert_eq!(
            at("sd/video/clip.mp41.mp4").map(|f| &f.reason),
            Some(&Reason::Duplicate {
                kept: path("sd/video/clip.mp4.mp4")
            }),
            "the file that stays is the shortest name not checked"
        );
        assert!(!at("sd/video/clip.mp4.mp4").is_some_and(Finding::prechecked));

        // A group whose shortest name is itself an artifact.
        let mut record = ScreenRecord::default();
        record.record(pending("sd/video/demo.mp4.mp4"));
        let listing = Listing {
            files: vec![
                file("sd/video/demo.mp4.mp4", 100),
                file("sd/video/demo.mp4.mp4.mp4", 100),
                file("sd/video/demo.mp4123.mp4", 100),
            ],
            card: Some(9),
        };
        let found = findings(&listing, Some(&record), &Protected::default());
        let checked = checked_paths(&found);
        assert_eq!(
            checked,
            ["sd/video/demo.mp4.mp4", "sd/video/demo.mp4.mp4.mp4"]
        );
        let kept = found.iter().find_map(|f| match &f.reason {
            Reason::Duplicate { kept } => Some(kept.clone()),
            _ => None,
        });
        assert_eq!(kept, Some(path("sd/video/demo.mp4123.mp4")));
        // No duplicate or variant names a checked file as the one that stays.
        assert!(found.iter().all(|f| match &f.reason {
            Reason::Duplicate { kept } | Reason::Variant { kept } => {
                !checked.contains(&kept.to_string())
            }
            _ => true,
        }));
    }

    #[test]
    fn vendor_artifacts_are_named_after_their_original() {
        for (name, base) in [
            ("demon_open.mp4.mp4.mp4", Some("demon_open.mp4")),
            ("NVI.mp427034822.mp4", Some("nvi.mp4")),
            ("x.mp41.mp4.mp4", Some("x.mp4")),
            ("logo.png.png", Some("logo.png")),
            ("8.8APEX_2.mp4", None),
            ("m04.mp4", None),
            ("clip.mp4x.mp4", None),
            (".mp4.mp4", None),
            ("noext", None),
            ("trailing.", None),
        ] {
            assert_eq!(artifact_base(name).as_deref(), base, "{name}");
        }
    }
}
