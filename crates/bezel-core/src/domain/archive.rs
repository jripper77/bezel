//! The catalog of what Bezel sent to each screen, and the local copies of
//! those files (D-2026-09-30-storage-manager-2, -5, -6, -7, -8, -10, -11).
//!
//! The screens list, report sizes, store, delete and play files; they cannot
//! send one back, rename or move it. So Bezel keeps the exact bytes of every
//! upload, addressed by content ([`ContentId`]: the same bytes are kept
//! once), and an [`ArchiveEntry`] per file it sent, keyed by screen
//! ([`ScreenKey`]), medium, folder and name. A card entry also keeps the
//! card's capacity, the only card trait the protocol shows, so a card of
//! another capacity shows the other card's entries as restorable
//! ([`Overview::other_card`]). A screen's record also keeps the boot media
//! Bezel set and what the screen does when the computer shuts down
//! ([`ScreenRecord::standby`]).
//!
//! Moving, renaming and restoring are re-uploads of a copy. This module plans
//! them ([`plan_move`], [`plan_copy`], [`plan_rename`], [`plan_restore`]); the
//! use cases run each [`Step`] alone: send the copy, check the stored size,
//! and only then delete the source. Everything here is pure: the time, the
//! listings and the bytes come from the callers and the `ArchiveStore` port.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

use super::cleanup::{Protected, artifact_base};
use super::device::ModelId;
use super::geometry::Size;
use super::media::{MediaInfo, MediaKind};
use super::screen::Brightness;
use super::standby::{PlanB, Standby, StoredPlanB};
use super::storage::{
    BootMedia, FileEntry, FileName, Medium, NameError, Refusal, RemotePath, StorageLocation,
    check_size,
};

/// Default size limit of the local copies of deleted files: 2 GiB, about 80
/// files at the 25 MiB rev C cap (D-2026-09-30-storage-manager-6).
pub const DEFAULT_CACHE_LIMIT: u64 = 2 << 30;

/// Which screen a catalog record belongs to: its model id plus an optional
/// name the user gives it. No screen can be told apart over USB, so two
/// screens of one model share a record unless they are named.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenKey {
    /// The catalog model id (`turing-8.8`), kept as text so that a catalog
    /// written by another version still loads.
    pub model: String,
    /// The user's name for the screen; `None`: the model alone.
    pub name: Option<String>,
}

impl ScreenKey {
    /// The key of an unnamed screen of `model`.
    pub fn new(model: ModelId) -> Self {
        Self {
            model: model.0.to_string(),
            name: None,
        }
    }

    /// The key of a screen of `model` the user named `name` (trimmed; a
    /// blank name means none).
    pub fn named(model: ModelId, name: &str) -> Self {
        let name = name.trim();
        Self {
            name: (!name.is_empty()).then(|| name.to_string()),
            ..Self::new(model)
        }
    }
}

impl fmt::Display for ScreenKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{} ({name})", self.model),
            None => f.write_str(&self.model),
        }
    }
}

/// The SHA-256 of a file's bytes as 64 lower-case hex digits: the name of its
/// local copy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentId(String);

const HEX: &[u8; 16] = b"0123456789abcdef";

impl ContentId {
    /// The id of a SHA-256 digest (the adapter hashes, the core writes it).
    pub fn from_digest(digest: [u8; 32]) -> Self {
        let nibbles = digest.iter().flat_map(|b| [b >> 4, b & 0x0f]);
        Self(nibbles.map(|n| char::from(HEX[usize::from(n)])).collect())
    }

    /// An id read back from a catalog: 64 hex digits in any case; `None`
    /// for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit());
        hex.then(|| Self(text.to_ascii_lowercase()))
    }

    /// The 64 hex digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a cataloged file stands (D-2026-09-30-storage-manager-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryState {
    /// Recorded before its upload started and not verified since: an upload
    /// that is running, was interrupted or failed its size check.
    Pending,
    /// Uploaded with its size verified, and listed when last looked.
    Stored,
    /// Absent from the screen's last listing (a formatted or replaced card).
    /// Never dropped on its own.
    Missing,
    /// Deleted through Bezel: only its copy counts against the limit.
    Deleted,
}

impl EntryState {
    /// Every state.
    pub const ALL: [EntryState; 4] = [
        EntryState::Pending,
        EntryState::Stored,
        EntryState::Missing,
        EntryState::Deleted,
    ];

    /// Stable machine name (`pending`, `stored`, `missing`, `deleted`).
    pub const fn slug(self) -> &'static str {
        match self {
            EntryState::Pending => "pending",
            EntryState::Stored => "stored",
            EntryState::Missing => "missing",
            EntryState::Deleted => "deleted",
        }
    }

    /// The state named by `slug`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.slug() == slug)
    }
}

/// One file Bezel sent to a screen, or a screen file associated with its
/// original on the PC (D-2026-09-30-storage-manager-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Medium, folder and name on the screen.
    pub path: RemotePath,
    /// On a card: the card's total capacity in bytes when the file was sent.
    /// `None` on the internal flash.
    pub card: Option<u64>,
    /// Size in bytes of the bytes sent.
    pub size: u64,
    /// The bytes sent (a converted video's output).
    pub content: ContentId,
    /// The file on the PC it came from, as the adapter names it.
    pub source: Option<String>,
    /// Play time, when probed.
    pub duration: Option<Duration>,
    /// Picture size, when probed.
    pub resolution: Option<Size>,
    /// When it was sent, in seconds since the Unix epoch (the adapter reads
    /// the clock).
    pub sent_at: u64,
    /// Where it stands.
    pub state: EntryState,
}

/// The card capacity an entry at `path` keeps: `card` on the card, none on
/// the internal flash.
fn card_of(path: &RemotePath, card: Option<u64>) -> Option<u64> {
    match path.location.medium {
        Medium::Internal => None,
        Medium::Card => card,
    }
}

impl ArchiveEntry {
    /// The entry of an upload about to start: [`EntryState::Pending`],
    /// nothing probed. `card` is the inserted card's capacity, kept only for
    /// a card path.
    pub fn pending(
        path: RemotePath,
        card: Option<u64>,
        size: u64,
        content: ContentId,
        sent_at: u64,
    ) -> Self {
        Self {
            card: card_of(&path, card),
            path,
            size,
            content,
            source: None,
            duration: None,
            resolution: None,
            sent_at,
            state: EntryState::Pending,
        }
    }

    /// Image or video: its folder's kind.
    pub fn kind(&self) -> MediaKind {
        self.path.location.kind
    }

    /// Whether the entry names `path` on the card of capacity `card`
    /// (ignored on the internal flash).
    pub fn is_at(&self, path: &RemotePath, card: Option<u64>) -> bool {
        self.path == *path && self.card == card_of(path, card)
    }
}

/// What a screen stores right now, as the use cases listed it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    /// Every file of both media, with the sizes the screen reports.
    pub files: Vec<FileEntry>,
    /// The inserted card's total capacity in bytes; `None` without a card.
    pub card: Option<u64>,
}

impl Listing {
    /// The listed file at `path`.
    pub fn file(&self, path: &RemotePath) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path == *path)
    }

    /// The listed file in `path`'s folder whose name equals `path`'s apart
    /// from letter case: a FAT card cannot tell the two apart, so writing
    /// `path` would replace it.
    pub fn clash(&self, path: &RemotePath) -> Option<&FileEntry> {
        self.files.iter().find(|f| same_file(&f.path, path))
    }

    /// Whether this listing shows `entry`'s medium: the internal flash
    /// always, a card only when it is the entry's card.
    fn shows(&self, entry: &ArchiveEntry) -> bool {
        match entry.path.location.medium {
            Medium::Internal => true,
            Medium::Card => self.card.is_some() && entry.card == self.card,
        }
    }
}

/// Two paths a screen may store as one file: the same folder and the same
/// name apart from letter case.
fn same_file(a: &RemotePath, b: &RemotePath) -> bool {
    a.location == b.location && a.name.as_str().eq_ignore_ascii_case(b.name.as_str())
}

/// A listed file and, when Bezel sent it, its entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The file as the screen lists it (its size `None` when the screen
    /// cannot tell: the entry's size is then the one to show).
    pub file: FileEntry,
    /// Its entry at the same place, unless deleted.
    pub entry: Option<ArchiveEntry>,
}

/// A screen's files next to its catalog record ([`Catalog::reconcile`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Overview {
    /// Every listed file, in the listing's order.
    pub listed: Vec<Listed>,
    /// Entries of the listed media that the screen no longer stores.
    pub missing: Vec<ArchiveEntry>,
    /// Card entries of another card (another capacity) or of a card that is
    /// not inserted: "on another card", restorable.
    pub other_card: Vec<ArchiveEntry>,
}

impl Overview {
    /// The default restore selection for `medium`
    /// (D-2026-09-30-storage-manager-8): its missing entries and, for the
    /// card, the other card's.
    pub fn restorable(&self, medium: Medium) -> Vec<ArchiveEntry> {
        let missing = self.missing.iter();
        let missing = missing.filter(|e| e.path.location.medium == medium);
        let other = self.other_card.iter().filter(|_| medium == Medium::Card);
        missing.chain(other).cloned().collect()
    }
}

/// The catalog record of one screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScreenRecord {
    /// Every file Bezel sent to it, oldest first.
    pub entries: Vec<ArchiveEntry>,
    /// The boot media Bezel last set, when it is a stored file (`None`: the
    /// default start screen, or never set through Bezel).
    pub boot: Option<RemotePath>,
    /// What the screen does when the computer shuts down
    /// (D-2026-10-03-power-off-standby-2): `keep` until the user chooses.
    pub standby: Standby,
    /// The plan B Bezel last stored on the screen, by the choice or the
    /// boot media; `None` when none was stored since records kept it (then
    /// [`ScreenRecord::plan_b`] derives it from the choice and the boot
    /// media).
    pub stored: Option<StoredPlanB>,
}

impl ScreenRecord {
    /// The entry at `path` (on the card of capacity `card`) that is not
    /// deleted.
    pub fn entry(&self, path: &RemotePath, card: Option<u64>) -> Option<&ArchiveEntry> {
        let live = |e: &&ArchiveEntry| e.state != EntryState::Deleted;
        self.entries
            .iter()
            .filter(live)
            .find(|e| e.is_at(path, card))
    }

    /// The entry at `path` that is not deleted, to change its state.
    pub fn entry_mut(&mut self, path: &RemotePath, card: Option<u64>) -> Option<&mut ArchiveEntry> {
        let live = |e: &&mut ArchiveEntry| e.state != EntryState::Deleted;
        self.entries
            .iter_mut()
            .filter(live)
            .find(|e| e.is_at(path, card))
    }

    /// Records `entry` as the newest, replacing any entry at its place
    /// (deleted ones too). The replaced entry comes back: its copy may now be
    /// unused ([`Catalog::is_referenced`]).
    pub fn record(&mut self, entry: ArchiveEntry) -> Option<ArchiveEntry> {
        let replaced = self.forget(&entry.path, entry.card);
        self.entries.push(entry);
        replaced
    }

    /// Removes the entry at `path` (deleted or not) and returns it.
    pub fn forget(&mut self, path: &RemotePath, card: Option<u64>) -> Option<ArchiveEntry> {
        let at = self.entries.iter().position(|e| e.is_at(path, card))?;
        Some(self.entries.remove(at))
    }

    /// Records the boot media Bezel set.
    pub fn set_boot(&mut self, boot: &BootMedia) {
        self.boot = match boot {
            BootMedia::File(path) => Some(path.clone()),
            BootMedia::Default => None,
        };
    }

    /// The boot media Bezel last set ([`BootMedia::Default`] when none).
    pub fn boot_media(&self) -> BootMedia {
        self.boot
            .clone()
            .map_or(BootMedia::Default, BootMedia::File)
    }

    /// The plan B on the screen as the record says: the one Bezel last
    /// stored ([`ScreenRecord::stored`]: by the choice, or by the boot media
    /// set after it, [`PlanB::with_boot`]); without one, the plan B the
    /// recorded choice writes next to the recorded boot media
    /// ([`Standby::plan_b`]).
    pub fn plan_b(&self) -> PlanB {
        self.stored.map_or_else(
            || self.standby.plan_b(self.boot_media().start_mode()),
            |stored| stored.plan,
        )
    }

    /// The backlight level stored with the last plan B, when the user chose
    /// one.
    pub fn start_brightness(&self) -> Option<Brightness> {
        self.stored.and_then(|stored| stored.brightness)
    }

    /// Marks every entry of the media `listing` shows as stored or missing
    /// by whether it is listed; a pending entry stays pending while listed.
    fn reconcile(&mut self, listing: &Listing) {
        for entry in &mut self.entries {
            if entry.state == EntryState::Deleted || !listing.shows(entry) {
                continue;
            }
            let listed = listing.file(&entry.path).is_some();
            entry.state = match (entry.state, listed) {
                (EntryState::Pending, true) => EntryState::Pending,
                (_, true) => EntryState::Stored,
                (_, false) => EntryState::Missing,
            };
        }
    }

    /// The listing next to this record.
    pub fn overview(&self, listing: &Listing) -> Overview {
        let listed = listing.files.iter().map(|file| Listed {
            file: file.clone(),
            entry: self.entry(&file.path, listing.card).cloned(),
        });
        let live = self
            .entries
            .iter()
            .filter(|e| e.state != EntryState::Deleted);
        let (shown, other): (Vec<_>, Vec<_>) = live.partition(|e| listing.shows(e));
        let missing = shown.into_iter().filter(|e| e.state == EntryState::Missing);
        Overview {
            listed: listed.collect(),
            missing: missing.cloned().collect(),
            other_card: other.into_iter().cloned().collect(),
        }
    }
}

/// Which local copies "Clear cache" removes (D-2026-09-30-storage-manager-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    /// The copies of files deleted through Bezel.
    Deleted,
    /// Every copy (`--all`).
    All,
}

/// The local copies in numbers, listed before "Clear cache" asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheInfo {
    /// Copies held.
    pub copies: usize,
    /// Their bytes, as cataloged.
    pub bytes: u64,
    /// Copies of files deleted through Bezel: what the limit counts and
    /// "Clear cache" removes.
    pub deleted_copies: usize,
    /// Their bytes.
    pub deleted_bytes: u64,
    /// The limit of the copies of deleted files, in bytes.
    pub limit: u64,
}

/// A held copy whose every entry is deleted.
struct DeletedCopy {
    content: ContentId,
    size: u64,
    sent_at: u64,
}

/// Everything Bezel keeps about the files it sent: one record per screen,
/// the copies the store holds and the limit of the copies of deleted files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    /// Size limit, in bytes, of the copies of files deleted through Bezel;
    /// copies of stored or missing files never count.
    pub limit: u64,
    /// One record per screen.
    pub screens: BTreeMap<ScreenKey, ScreenRecord>,
    /// The copies the store holds.
    pub copies: BTreeSet<ContentId>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            limit: DEFAULT_CACHE_LIMIT,
            screens: BTreeMap::new(),
            copies: BTreeSet::new(),
        }
    }
}

impl Catalog {
    /// The record of `key`, if it has one.
    pub fn screen(&self, key: &ScreenKey) -> Option<&ScreenRecord> {
        self.screens.get(key)
    }

    /// The record of `key`, created empty when it has none.
    pub fn screen_mut(&mut self, key: &ScreenKey) -> &mut ScreenRecord {
        self.screens.entry(key.clone()).or_default()
    }

    /// Whether the store holds a copy of `content`.
    pub fn has_copy(&self, content: &ContentId) -> bool {
        self.copies.contains(content)
    }

    fn entries(&self) -> impl Iterator<Item = &ArchiveEntry> {
        self.screens.values().flat_map(|r| r.entries.iter())
    }

    /// Whether any entry of any screen names `content`.
    pub fn is_referenced(&self, content: &ContentId) -> bool {
        self.entries().any(|e| e.content == *content)
    }

    /// Matches `key`'s entries against what the screen lists now: listed
    /// entries become stored (pending ones stay pending), absent ones
    /// missing; deleted entries and the entries of another card are left as
    /// they are. Other screens are untouched.
    pub fn reconcile(&mut self, key: &ScreenKey, listing: &Listing) -> Overview {
        match self.screens.get_mut(key) {
            Some(record) => {
                record.reconcile(listing);
                record.overview(listing)
            }
            None => ScreenRecord::default().overview(listing),
        }
    }

    /// Held copies whose every entry is deleted, oldest first (by the last
    /// time one of their files was sent).
    fn deleted_copies(&self) -> Vec<DeletedCopy> {
        let mut seen: BTreeMap<&ContentId, (bool, u64, u64)> = BTreeMap::new();
        for entry in self.entries().filter(|e| self.has_copy(&e.content)) {
            let slot = seen.entry(&entry.content).or_insert((true, entry.size, 0));
            slot.0 &= entry.state == EntryState::Deleted;
            slot.2 = slot.2.max(entry.sent_at);
        }
        let deleted = seen.into_iter().filter(|(_, (all, ..))| *all);
        let mut out: Vec<DeletedCopy> = deleted
            .map(|(content, (_, size, sent_at))| DeletedCopy {
                content: content.clone(),
                size,
                sent_at,
            })
            .collect();
        out.sort_by_key(|c| c.sent_at);
        out
    }

    /// The copies in numbers.
    pub fn cache(&self) -> CacheInfo {
        let mut sizes: BTreeMap<&ContentId, u64> = BTreeMap::new();
        for entry in self.entries().filter(|e| self.has_copy(&e.content)) {
            sizes.insert(&entry.content, entry.size);
        }
        let deleted = self.deleted_copies();
        CacheInfo {
            copies: self.copies.len(),
            bytes: sizes.values().sum(),
            deleted_copies: deleted.len(),
            deleted_bytes: deleted.iter().map(|c| c.size).sum(),
            limit: self.limit,
        }
    }

    /// Drops the copies of deleted files, oldest first, until they fit the
    /// limit; the copies of stored, pending or missing files never go. The
    /// dropped ids come back to be discarded from the store; their entries
    /// stay, with no local copy.
    pub fn evict(&mut self) -> Vec<ContentId> {
        let deleted = self.deleted_copies();
        let mut total: u64 = deleted.iter().map(|c| c.size).sum();
        let mut dropped = Vec::new();
        for copy in deleted {
            if total <= self.limit {
                break;
            }
            total -= copy.size;
            self.copies.remove(&copy.content);
            dropped.push(copy.content);
        }
        dropped
    }

    /// "Clear cache": drops the copies `scope` names and returns their ids
    /// to be discarded from the store. Entries stay, with no local copy (not
    /// movable or restorable until associated again).
    pub fn clear(&mut self, scope: Clear) -> Vec<ContentId> {
        let gone: Vec<ContentId> = match scope {
            Clear::Deleted => self
                .deleted_copies()
                .into_iter()
                .map(|c| c.content)
                .collect(),
            Clear::All => self.copies.iter().cloned().collect(),
        };
        for content in &gone {
            self.copies.remove(content);
        }
        gone
    }
}

/// A file on the PC offered as the original of a screen file
/// (D-2026-09-30-storage-manager-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Where it is on the PC, as the adapter names it.
    pub source: String,
    /// The file as probed.
    pub media: MediaInfo,
}

/// What is known of the screen file whose original is looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sought<'a> {
    /// The file on the screen.
    pub path: &'a RemotePath,
    /// Its size in bytes.
    pub size: u64,
    /// The picture size such a file has on this screen (videos: the panel's
    /// native size, 480x1920 on the 8.8"), when known.
    pub resolution: Option<Size>,
    /// Its play time, when known (from an earlier association).
    pub duration: Option<Duration>,
}

/// How likely a candidate is; compared field by field, more is likelier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Likeness {
    same_name: bool,
    common_prefix: usize,
    resolution: bool,
    duration: bool,
}

/// A name's comparable part: the stem of the name a vendor artifact stands
/// for, lower-case letters and digits only (`NVI.mp427034822.mp4`: `nvi`).
fn name_key(name: &str) -> String {
    let base = artifact_base(name).unwrap_or_else(|| name.to_ascii_lowercase());
    let stem = base
        .rsplit_once('.')
        .map_or(base.as_str(), |(stem, _)| stem);
    let alnum = stem.chars().filter(char::is_ascii_alphanumeric);
    alnum.map(|c| c.to_ascii_lowercase()).collect()
}

fn likeness(sought: &Sought<'_>, wanted: &str, candidate: &Candidate) -> Likeness {
    let file = candidate
        .source
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let key = name_key(file);
    let common_prefix = wanted
        .chars()
        .zip(key.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let duration = candidate.media.video.and_then(|v| v.duration);
    Likeness {
        same_name: key == wanted,
        common_prefix,
        resolution: sought.resolution.is_some() && candidate.media.dimensions == sought.resolution,
        duration: match (sought.duration, duration) {
            (Some(a), Some(b)) => a.abs_diff(b) <= Duration::from_secs(1),
            _ => false,
        },
    }
}

/// The candidates that can be `sought`'s original, likeliest first: exactly
/// its size and kind, then ranked by name, resolution and duration (equal
/// ones by source). The user still confirms each pair.
pub fn rank_candidates(sought: &Sought<'_>, candidates: Vec<Candidate>) -> Vec<Candidate> {
    let kind = sought.path.location.kind;
    let wanted = name_key(sought.path.name.as_str());
    let fits = candidates
        .into_iter()
        .filter(|c| c.media.bytes == sought.size && c.media.kind() == Some(kind));
    let mut ranked: Vec<(Likeness, Candidate)> =
        fits.map(|c| (likeness(sought, &wanted, &c), c)).collect();
    ranked.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.source.cmp(&y.source)));
    ranked.into_iter().map(|(_, c)| c).collect()
}

/// The name a file gets when it is sent again: the upload rule
/// ([`FileName::for_upload`]: `NVI.mp4` becomes `nvi.mp4`), or a suggested
/// name with the same extension when its name breaks the rule.
pub fn upload_name(name: &FileName) -> FileName {
    FileName::for_upload(name.as_str())
        .unwrap_or_else(|_| FileName::suggest(name.as_str(), name.extension().unwrap_or_default()))
}

/// What a plan does with its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transfer {
    /// To the other medium; each source is deleted once its copy is verified.
    Move,
    /// To the other medium; the sources stay.
    Copy,
    /// A new name on the same medium; the source is deleted once its copy
    /// is verified.
    Rename,
    /// Cataloged files sent again from their copies; nothing is deleted.
    Restore,
}

impl Transfer {
    /// Whether each step ends by deleting its source.
    pub const fn deletes_source(self) -> bool {
        matches!(self, Transfer::Move | Transfer::Rename)
    }

    /// Stable machine name (`move`, `copy`, `rename`, `restore`).
    pub const fn slug(self) -> &'static str {
        match self {
            Transfer::Move => "move",
            Transfer::Copy => "copy",
            Transfer::Rename => "rename",
            Transfer::Restore => "restore",
        }
    }
}

/// Whether a screen deletes files through Bezel: TUR_USB screens do not
/// (D-2026-09-30-storage-manager-11), so nothing that ends in a delete runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deletes {
    /// Deleting works.
    Supported,
    /// Deleting is not offered.
    Unsupported,
}

/// One file of a plan, run alone: send the copy to `target`, check its
/// size, then (move, rename) delete `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The file moved, copied or renamed; for a restore, where its entry was.
    pub source: RemotePath,
    /// Where the copy goes.
    pub target: RemotePath,
    /// Bytes sent, and the size the target must then report.
    pub size: u64,
    /// The copy sent.
    pub content: ContentId,
    /// The file the target replaces, whose overwrite the user confirmed.
    pub replaces: Option<FileEntry>,
}

/// Why a file of a plan is left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// A file of the target's name is there (or another step sends one) and
    /// its overwrite was not confirmed.
    Conflict(FileEntry),
    /// No local copy matches the file: Bezel did not send it, its copy was
    /// cleared, or the screen's file has another size.
    NoLocalCopy,
    /// The step ends in a delete and the screen cannot delete through Bezel.
    DeleteUnsupported,
    /// Restore: the same name with the same size is already there.
    Present,
}

impl Skip {
    /// Stable reason code.
    pub const fn code(&self) -> &'static str {
        match self {
            Skip::Conflict(_) => "conflict",
            Skip::NoLocalCopy => "noLocalCopy",
            Skip::DeleteUnsupported => "deleteUnsupported",
            Skip::Present => "present",
        }
    }
}

/// A file a plan leaves out, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The file.
    pub source: RemotePath,
    /// Where it would have gone.
    pub target: RemotePath,
    /// Why not.
    pub skip: Skip,
}

/// What the confirmation of a plan warns about (D-2026-09-30-storage-manager-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The file is the boot media Bezel set: the firmware boots the last
    /// file played, which is gone from there afterwards.
    BootMedia(RemotePath),
    /// A theme plays this video by its name, which the rename changes.
    ThemeVideo(RemotePath),
}

impl Warning {
    /// Stable warning code.
    pub const fn code(&self) -> &'static str {
        match self {
            Warning::BootMedia(_) => "bootMedia",
            Warning::ThemeVideo(_) => "themeVideo",
        }
    }
}

/// A planned move, copy, rename or restore: what the one confirmation lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPlan {
    /// What the plan does.
    pub transfer: Transfer,
    /// The files sent, in order, one at a time.
    pub steps: Vec<Step>,
    /// The files left out.
    pub skipped: Vec<Skipped>,
    /// What the confirmation warns about.
    pub warnings: Vec<Warning>,
}

/// Why no plan could be made. Nothing was sent or deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRefusal {
    /// The target is the card and none is inserted.
    NoCard,
    /// The file is not on the screen.
    NotListed(RemotePath),
    /// The file is already on the target medium.
    SameMedium(RemotePath),
    /// The new name breaks the upload rule.
    InvalidName(NameError),
    /// A new name must keep the file's extension (`None`: no extension).
    ExtensionChanged {
        /// The extension the new name must have.
        expected: Option<String>,
    },
    /// The new name is the file's name, letter case aside (a FAT card
    /// cannot tell them apart).
    SameName,
    /// A file the screen cannot take (empty, or over its per-file limit).
    Unsendable {
        /// Where it would go.
        path: RemotePath,
        /// Why not.
        refusal: Refusal,
    },
    /// Restore: the files do not fit the medium's free space.
    NoSpace {
        /// Bytes to send.
        needed: u64,
        /// Bytes free.
        free: u64,
    },
}

impl PlanRefusal {
    /// Stable reason code.
    pub const fn code(&self) -> &'static str {
        match self {
            PlanRefusal::NoCard => "noCard",
            PlanRefusal::NotListed(_) => "notListed",
            PlanRefusal::SameMedium(_) => "sameMedium",
            PlanRefusal::InvalidName(_) => "invalidName",
            PlanRefusal::ExtensionChanged { .. } => "extensionChanged",
            PlanRefusal::SameName => "sameName",
            PlanRefusal::Unsendable { .. } => "unsendable",
            PlanRefusal::NoSpace { .. } => "noSpace",
        }
    }
}

impl fmt::Display for PlanRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanRefusal::NoCard => f.write_str("the screen has no memory card"),
            PlanRefusal::NotListed(path) => write!(f, "{path} is not on the screen"),
            PlanRefusal::SameMedium(path) => write!(f, "{path} is already on that medium"),
            PlanRefusal::InvalidName(e) => write!(f, "invalid file name: {e}"),
            PlanRefusal::ExtensionChanged {
                expected: Some(ext),
            } => write!(f, "the new name must end in .{ext}"),
            PlanRefusal::ExtensionChanged { expected: None } => {
                f.write_str("the new name cannot have an extension")
            }
            PlanRefusal::SameName => {
                f.write_str("the new name is the file's name (letter case aside)")
            }
            PlanRefusal::Unsendable { path, refusal } => write!(f, "{path}: {refusal}"),
            PlanRefusal::NoSpace { needed, free } => write!(
                f,
                "the files need {needed} bytes and {free} are free ({} bytes short); \
                 nothing was sent",
                needed.saturating_add(1).saturating_sub(*free)
            ),
        }
    }
}

/// One screen as the plans see it.
#[derive(Debug, Clone, Copy)]
pub struct ScreenView<'a> {
    /// The catalog, with the copies it holds.
    pub catalog: &'a Catalog,
    /// The screen.
    pub key: &'a ScreenKey,
    /// What the screen lists now.
    pub listing: &'a Listing,
    /// Whether it deletes through Bezel.
    pub deletes: Deletes,
    /// Its boot media and the videos themes play.
    pub protected: &'a Protected,
}

impl ScreenView<'_> {
    /// The entry whose copy is the listed `file`: not pending, of the same
    /// size (when the screen tells it), with its copy held.
    fn copy_of(&self, file: &FileEntry) -> Option<&ArchiveEntry> {
        let record = self.catalog.screen(self.key)?;
        let entry = record.entry(&file.path, self.listing.card)?;
        let same = file.size.is_none_or(|size| size == entry.size);
        let usable = entry.state != EntryState::Pending && same;
        (usable && self.catalog.has_copy(&entry.content)).then_some(entry)
    }
}

impl TransferPlan {
    fn new(transfer: Transfer) -> Self {
        Self {
            transfer,
            steps: Vec::new(),
            skipped: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Bytes the steps send.
    pub fn bytes(&self) -> u64 {
        self.steps.iter().map(|s| s.size).sum()
    }

    /// A target another step already sends.
    fn planned(&self, target: &RemotePath) -> Option<FileEntry> {
        let step = self.steps.iter().find(|s| same_file(&s.target, target))?;
        Some(FileEntry {
            path: step.target.clone(),
            size: Some(step.size),
        })
    }

    fn skip(&mut self, source: &RemotePath, target: RemotePath, skip: Skip) {
        let source = source.clone();
        self.skipped.push(Skipped {
            source,
            target,
            skip,
        });
    }

    /// Plans the listed `source` to `target` (move, copy, rename).
    fn add(
        &mut self,
        view: &ScreenView<'_>,
        source: &RemotePath,
        target: RemotePath,
        overwrite: &[RemotePath],
    ) -> Result<(), PlanRefusal> {
        let listed = view.listing.file(source);
        let file = listed.ok_or_else(|| PlanRefusal::NotListed(source.clone()))?;
        if self.transfer.deletes_source() && view.deletes == Deletes::Unsupported {
            self.skip(source, target, Skip::DeleteUnsupported);
            return Ok(());
        }
        let Some(entry) = view.copy_of(file) else {
            self.skip(source, target, Skip::NoLocalCopy);
            return Ok(());
        };
        let (size, content) = (entry.size, entry.content.clone());
        if let Some(planned) = self.planned(&target) {
            self.skip(source, target, Skip::Conflict(planned));
            return Ok(());
        }
        let replaces = view.listing.clash(&target).cloned();
        if let Some(other) = &replaces
            && !overwrite.contains(&target)
        {
            self.skip(source, target, Skip::Conflict(other.clone()));
            return Ok(());
        }
        self.warn(view.protected, source);
        let source = source.clone();
        self.steps.push(Step {
            source,
            target,
            size,
            content,
            replaces,
        });
        Ok(())
    }

    fn warn(&mut self, protected: &Protected, source: &RemotePath) {
        if self.transfer.deletes_source() && protected.is_boot(source) {
            self.warnings.push(Warning::BootMedia(source.clone()));
        }
        if self.transfer == Transfer::Rename && protected.is_theme_video(source) {
            self.warnings.push(Warning::ThemeVideo(source.clone()));
        }
    }

    /// Plans `entry` back onto `to` (restore).
    fn restore(
        &mut self,
        view: &ScreenView<'_>,
        entry: &ArchiveEntry,
        to: Medium,
        cap: u64,
        overwrite: &[RemotePath],
    ) -> Result<(), PlanRefusal> {
        let location = StorageLocation::new(to, entry.kind());
        let target = RemotePath::new(location, entry.path.name.clone());
        let source = &entry.path;
        if !view.catalog.has_copy(&entry.content) {
            self.skip(source, target, Skip::NoLocalCopy);
            return Ok(());
        }
        if let Some(planned) = self.planned(&target) {
            self.skip(source, target, Skip::Conflict(planned));
            return Ok(());
        }
        let replaces = view.listing.clash(&target).cloned();
        if let Some(other) = &replaces {
            let skip = if other.size == Some(entry.size) {
                Some(Skip::Present)
            } else {
                (!overwrite.contains(&target)).then(|| Skip::Conflict(other.clone()))
            };
            if let Some(skip) = skip {
                self.skip(source, target, skip);
                return Ok(());
            }
        }
        check_size(entry.size, cap).map_err(|refusal| PlanRefusal::Unsendable {
            path: target.clone(),
            refusal,
        })?;
        self.steps.push(Step {
            source: source.clone(),
            target,
            size: entry.size,
            content: entry.content.clone(),
            replaces,
        });
        Ok(())
    }
}

fn across(
    view: &ScreenView<'_>,
    transfer: Transfer,
    sources: &[RemotePath],
    to: Medium,
    overwrite: &[RemotePath],
) -> Result<TransferPlan, PlanRefusal> {
    if to == Medium::Card && view.listing.card.is_none() {
        return Err(PlanRefusal::NoCard);
    }
    let mut plan = TransferPlan::new(transfer);
    for source in sources {
        if source.location.medium == to {
            return Err(PlanRefusal::SameMedium(source.clone()));
        }
        let location = StorageLocation::new(to, source.location.kind);
        let target = RemotePath::new(location, upload_name(&source.name));
        plan.add(view, source, target, overwrite)?;
    }
    Ok(plan)
}

/// Plans moving the listed `sources` to the other medium `to`
/// (D-2026-09-30-storage-manager-7): each one is sent from its local copy
/// under its upload name ([`upload_name`]), verified, and only then deleted
/// from where it was. A file whose name is taken on `to` is skipped unless
/// its target path is in `overwrite`; a file without a local copy is skipped,
/// and on a screen that cannot delete every file is. Moving the boot media
/// adds a warning. The per-file preflight (kind, limit, free space) runs
/// before each upload.
pub fn plan_move(
    view: &ScreenView<'_>,
    sources: &[RemotePath],
    to: Medium,
    overwrite: &[RemotePath],
) -> Result<TransferPlan, PlanRefusal> {
    across(view, Transfer::Move, sources, to, overwrite)
}

/// Plans copying the listed `sources` to the other medium `to`: as
/// [`plan_move`], but the sources stay, so it also runs on screens that
/// cannot delete.
pub fn plan_copy(
    view: &ScreenView<'_>,
    sources: &[RemotePath],
    to: Medium,
    overwrite: &[RemotePath],
) -> Result<TransferPlan, PlanRefusal> {
    across(view, Transfer::Copy, sources, to, overwrite)
}

/// Plans renaming the listed `source` to `new_name` on its medium: the upload
/// rule ([`FileName::for_upload`]), the same extension, a name that is not
/// the file's own apart from letter case. Then as [`plan_move`]; renaming
/// the boot media or a video a theme plays adds a warning.
pub fn plan_rename(
    view: &ScreenView<'_>,
    source: &RemotePath,
    new_name: &str,
    overwrite: &[RemotePath],
) -> Result<TransferPlan, PlanRefusal> {
    let name = FileName::for_upload(new_name).map_err(PlanRefusal::InvalidName)?;
    let expected = source.name.extension().map(str::to_ascii_lowercase);
    if name.extension() != expected.as_deref() {
        return Err(PlanRefusal::ExtensionChanged { expected });
    }
    if name.as_str().eq_ignore_ascii_case(source.name.as_str()) {
        return Err(PlanRefusal::SameName);
    }
    let mut plan = TransferPlan::new(Transfer::Rename);
    plan.add(
        view,
        source,
        RemotePath::new(source.location, name),
        overwrite,
    )?;
    Ok(plan)
}

/// The space a restore must fit in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    /// Free bytes on the target medium.
    pub free: u64,
    /// The screen's per-file limit (`UploadProfile::max_upload_bytes`).
    pub cap: u64,
}

/// Plans restoring the `selection` of entries onto `to`
/// (D-2026-09-30-storage-manager-8), oldest sent first, under their names.
/// A file of the same name and size there is skipped as present, another
/// size is a conflict skipped unless its target path is in `overwrite`, a
/// file without a copy is skipped. Before anything is sent, every file must
/// fit the per-file limit and their total the free space (as each upload
/// needs, less than it); otherwise the plan is refused. Nothing is deleted.
pub fn plan_restore(
    view: &ScreenView<'_>,
    selection: &[ArchiveEntry],
    to: Medium,
    room: Room,
    overwrite: &[RemotePath],
) -> Result<TransferPlan, PlanRefusal> {
    if to == Medium::Card && view.listing.card.is_none() {
        return Err(PlanRefusal::NoCard);
    }
    let mut chosen: Vec<&ArchiveEntry> = selection.iter().collect();
    chosen.sort_by_key(|e| e.sent_at);
    let mut plan = TransferPlan::new(Transfer::Restore);
    for entry in chosen {
        plan.restore(view, entry, to, room.cap, overwrite)?;
    }
    let needed = plan.bytes();
    if needed > 0 && needed >= room.free {
        return Err(PlanRefusal::NoSpace {
            needed,
            free: room.free,
        });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::model_by_id;
    use crate::domain::media::{
        MediaFormat, UploadProfile, VideoCodec, VideoPixelFormat, VideoTrack,
    };
    use crate::domain::theme::AssetRef;

    const CARD: u64 = 31_890_132_172; // the user's card, 29.7 GiB
    const OTHER_CARD: u64 = 7_948_206_080;

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).expect("path")
    }

    fn file(text: &str, size: u64) -> FileEntry {
        FileEntry {
            path: path(text),
            size: Some(size),
        }
    }

    fn id(byte: u8) -> ContentId {
        ContentId::from_digest([byte; 32])
    }

    /// A stored entry at `text`, its content numbered `n`, sent at `n`.
    fn stored(text: &str, card: Option<u64>, size: u64, n: u8) -> ArchiveEntry {
        let mut entry = ArchiveEntry::pending(path(text), card, size, id(n), u64::from(n));
        entry.state = EntryState::Stored;
        entry
    }

    fn key() -> ScreenKey {
        ScreenKey::new(ModelId("turing-8.8"))
    }

    fn catalog(entries: Vec<ArchiveEntry>) -> Catalog {
        let mut catalog = Catalog {
            copies: entries.iter().map(|e| e.content.clone()).collect(),
            ..Catalog::default()
        };
        catalog.screen_mut(&key()).entries = entries;
        catalog
    }

    fn listing(files: &[(&str, u64)], card: Option<u64>) -> Listing {
        Listing {
            files: files.iter().map(|(p, s)| file(p, *s)).collect(),
            card,
        }
    }

    fn names(entries: &[ArchiveEntry]) -> Vec<String> {
        entries.iter().map(|e| e.path.to_string()).collect()
    }

    #[test]
    fn entries_match_listings_per_screen_and_card() {
        let mut catalog = catalog(vec![
            stored("internal/video/bezel_a.mp4", None, 10, 1),
            stored("sd/video/bezel_b.mp4", Some(CARD), 20, 2),
            stored("sd/video/bezel_c.mp4", Some(OTHER_CARD), 30, 3),
            stored("internal/video/gone.mp4", None, 40, 4),
            ArchiveEntry::pending(path("sd/video/half.mp4"), Some(CARD), 50, id(5), 5),
        ]);
        let named = ScreenKey::named(ModelId("turing-8.8"), " desk ");
        catalog
            .screen_mut(&named)
            .record(stored("internal/video/gone.mp4", None, 40, 4));
        let now = listing(
            &[
                ("internal/video/bezel_a.mp4", 10),
                ("sd/video/bezel_b.mp4", 20),
                ("sd/video/half.mp4", 7),
                ("sd/video/AMD.mp4", 4_079_432),
            ],
            Some(CARD),
        );
        let overview = catalog.reconcile(&key(), &now);
        let matched: Vec<Option<EntryState>> = overview
            .listed
            .iter()
            .map(|l| l.entry.as_ref().map(|e| e.state))
            .collect();
        use EntryState::{Missing, Pending, Stored};
        assert_eq!(matched, [Some(Stored), Some(Stored), Some(Pending), None]);
        assert_eq!(names(&overview.missing), ["internal/video/gone.mp4"]);
        assert_eq!(names(&overview.other_card), ["sd/video/bezel_c.mp4"]);
        let state = |c: &Catalog, k: &ScreenKey, p: &str, card| {
            let record = c.screen(k).expect("record");
            record.entry(&path(p), card).map(|e| e.state)
        };
        assert_eq!(
            state(&catalog, &key(), "sd/video/bezel_c.mp4", Some(OTHER_CARD)),
            Some(Stored),
            "another card's entries stay as they were"
        );
        assert_eq!(
            state(&catalog, &named, "internal/video/gone.mp4", None),
            Some(Stored),
            "another screen is untouched"
        );
        // The default restore: the internal flash gets its missing file, the
        // card the other card's.
        assert_eq!(
            names(&overview.restorable(Medium::Internal)),
            ["internal/video/gone.mp4"]
        );
        assert_eq!(
            names(&overview.restorable(Medium::Card)),
            ["sd/video/bezel_c.mp4"]
        );

        // The other card goes in, and the internal file is back: the first
        // card's entries are now the other ones, nothing is lost.
        let swapped = listing(
            &[
                ("internal/video/bezel_a.mp4", 10),
                ("internal/video/gone.mp4", 40),
                ("sd/video/bezel_c.mp4", 30),
            ],
            Some(OTHER_CARD),
        );
        let overview = catalog.reconcile(&key(), &swapped);
        assert_eq!(overview.listed.len(), 3);
        assert!(overview.listed.iter().all(|l| l.entry.is_some()));
        assert!(overview.missing.is_empty());
        assert_eq!(
            names(&overview.other_card),
            ["sd/video/bezel_b.mp4", "sd/video/half.mp4"]
        );
        assert_eq!(
            state(&catalog, &key(), "internal/video/gone.mp4", None),
            Some(Stored)
        );
        assert_eq!(
            state(&catalog, &key(), "sd/video/bezel_b.mp4", Some(CARD)),
            Some(Stored)
        );

        // Without a card only the internal flash is matched; a pending
        // upload that left nothing is missing.
        let no_card = listing(&[], None);
        let overview = catalog.reconcile(&key(), &no_card);
        assert_eq!(
            names(&overview.missing),
            ["internal/video/bezel_a.mp4", "internal/video/gone.mp4"]
        );
        assert_eq!(overview.other_card.len(), 3);
        let half = listing(&[], Some(CARD));
        catalog.reconcile(&key(), &half);
        assert_eq!(
            state(&catalog, &key(), "sd/video/half.mp4", Some(CARD)),
            Some(Missing)
        );
        // A deleted entry is never matched, and a screen without a record
        // lists its files alone.
        let record = catalog.screen_mut(&key());
        record
            .entry_mut(&path("internal/video/bezel_a.mp4"), None)
            .expect("entry")
            .state = EntryState::Deleted;
        let overview = catalog.reconcile(&key(), &now);
        assert_eq!(overview.listed[0].entry, None);
        let stranger = ScreenKey::new(ModelId("turing-5"));
        let overview = catalog.reconcile(&stranger, &now);
        assert!(overview.listed.iter().all(|l| l.entry.is_none()));
        assert!(catalog.screen(&stranger).is_none());
    }

    #[test]
    fn eviction_drops_only_copies_of_deleted_files() {
        let deleted = |text: &str, size, n| {
            let mut entry = stored(text, None, size, n);
            entry.state = EntryState::Deleted;
            entry
        };
        let mut missing = stored("sd/video/lost.mp4", Some(CARD), 900, 2);
        missing.state = EntryState::Missing;
        let mut shared = deleted("internal/video/twin.mp4", 1000, 6);
        shared.content = id(1);
        let mut catalog = catalog(vec![
            stored("internal/video/kept.mp4", None, 1000, 1),
            missing,
            deleted("internal/video/oldest.mp4", 300, 3),
            deleted("internal/video/older.mp4", 200, 4),
            deleted("internal/video/newest.mp4", 100, 5),
            shared,
        ]);
        catalog.limit = 250;
        assert_eq!(
            catalog.cache(),
            CacheInfo {
                copies: 5,
                bytes: 2500,
                deleted_copies: 3,
                deleted_bytes: 600,
                limit: 250,
            }
        );
        // Over the limit by 350: the oldest deleted copy goes, then the next
        // one, until the rest fit. The stored and missing files' copies stay
        // (the deleted twin shares the stored file's copy).
        assert_eq!(catalog.evict(), [id(3), id(4)]);
        assert!(catalog.has_copy(&id(1)) && catalog.has_copy(&id(2)));
        assert!(catalog.has_copy(&id(5)));
        assert_eq!(catalog.cache().deleted_bytes, 100);
        assert!(catalog.evict().is_empty(), "within the limit");
        // The entries stay, with no local copy.
        let record = catalog.screen(&key()).expect("record");
        assert_eq!(record.entries.len(), 6);
        assert!(catalog.is_referenced(&id(3)));
        // Even a limit of zero never takes a copy a restore needs.
        catalog.limit = 0;
        assert_eq!(catalog.evict(), [id(5)]);
        assert_eq!(catalog.copies.len(), 2);
        assert_eq!(Catalog::default().limit, 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn clearing_the_cache_keeps_every_entry() {
        let mut gone = stored("internal/video/gone.mp4", None, 10, 2);
        gone.state = EntryState::Deleted;
        let mut catalog = catalog(vec![stored("internal/video/a.mp4", None, 20, 1), gone]);
        catalog.copies.insert(id(9)); // a copy no entry names
        assert_eq!(catalog.cache().copies, 3);
        assert_eq!(catalog.cache().bytes, 30);
        assert_eq!(catalog.clear(Clear::Deleted), [id(2)]);
        assert_eq!(catalog.clear(Clear::All), [id(1), id(9)]);
        assert!(catalog.copies.is_empty());
        assert_eq!(catalog.screen(&key()).expect("record").entries.len(), 2);
    }

    #[test]
    fn records_replace_by_place_and_keep_the_boot_media() {
        let mut record = ScreenRecord::default();
        let first = stored("sd/video/clip.mp4", Some(CARD), 10, 1);
        assert_eq!(record.record(first.clone()), None);
        let other_card = stored("sd/video/clip.mp4", Some(OTHER_CARD), 10, 1);
        assert_eq!(
            record.record(other_card),
            None,
            "another card, another place"
        );
        let again = stored("sd/video/clip.mp4", Some(CARD), 12, 2);
        assert_eq!(record.record(again), Some(first));
        assert_eq!(record.entries.len(), 2);
        assert_eq!(record.entries[1].size, 12, "the newest last");
        let internal = ArchiveEntry::pending(path("internal/image/a.png"), Some(CARD), 1, id(3), 3);
        assert_eq!(internal.card, None, "no card on the internal flash");
        assert_eq!(internal.kind(), MediaKind::Image);
        assert!(record.forget(&path("internal/image/a.png"), None).is_none());
        record.set_boot(&BootMedia::File(path("sd/video/clip.mp4")));
        assert_eq!(record.boot, Some(path("sd/video/clip.mp4")));
        assert_eq!(
            record.boot_media(),
            BootMedia::File(path("sd/video/clip.mp4"))
        );
        record.set_boot(&BootMedia::Default);
        assert_eq!(record.boot, None);
        assert_eq!(record.boot_media(), BootMedia::Default);
    }

    #[test]
    fn a_record_keeps_its_standby_choice_and_its_plan_b() {
        use crate::domain::standby::SleepMinutes;
        use crate::domain::storage::StartMode;

        let mut record = ScreenRecord::default();
        assert_eq!(record.standby, Standby::Keep, "keep until chosen");
        assert_eq!(record.plan_b(), PlanB::new(StartMode::Default, 0));
        let three = SleepMinutes::new(3).expect("minutes");
        record.standby = Standby::Off(three);
        record.set_boot(&BootMedia::File(path("internal/image/logo.png")));
        assert_eq!(record.plan_b(), PlanB::new(StartMode::Image, 3));
        record.standby = Standby::Album;
        assert_eq!(record.plan_b(), PlanB::new(StartMode::Image, 0));
        assert_eq!(record.start_brightness(), None);

        // What was stored last is what the record says (review W7): the
        // boot media set after the album, with its level.
        let forty = Brightness::new(40).expect("level");
        record.stored = Some(StoredPlanB {
            plan: PlanB::new(StartMode::Video, 0),
            brightness: Some(forty),
        });
        assert_eq!(record.plan_b(), PlanB::new(StartMode::Video, 0));
        assert_eq!(record.start_brightness(), Some(forty));
        let shown = record.stored.map(|s| s.to_string());
        assert_eq!(
            shown.as_deref(),
            Some("start mode 2, sleep timer off, brightness 40%")
        );
    }

    fn view<'a>(
        catalog: &'a Catalog,
        key: &'a ScreenKey,
        listing: &'a Listing,
        deletes: Deletes,
        protected: &'a Protected,
    ) -> ScreenView<'a> {
        ScreenView {
            catalog,
            key,
            listing,
            deletes,
            protected,
        }
    }

    #[test]
    fn moves_copies_and_renames_send_one_verified_copy_each() {
        let screen = key();
        let catalog = catalog(vec![
            stored("sd/video/NVI.mp4", Some(CARD), 5_680_675, 1),
            stored("sd/video/boot.mp4", Some(CARD), 300, 2),
            stored("internal/video/amd.mp4", None, 400, 3),
            stored("sd/video/taken.mp4", Some(CARD), 500, 4),
        ]);
        let now = listing(
            &[
                ("sd/video/NVI.mp4", 5_680_675),
                ("sd/video/boot.mp4", 300),
                ("sd/video/Rani.mp4", 6_007_182),
                ("sd/video/taken.mp4", 500),
                ("internal/video/amd.mp4", 400),
                ("internal/video/TAKEN.mp4", 77),
            ],
            Some(CARD),
        );
        let mut protected = Protected::new(Some(path("sd/video/boot.mp4")));
        let profile = model_by_id(ModelId("turing-8.8")).and_then(UploadProfile::for_model);
        protected.theme_video(
            &AssetRef("assets/AMD.mp4".into()),
            &profile.expect("profile"),
        );
        let rev_c = view(&catalog, &screen, &now, Deletes::Supported, &protected);
        let sources = [
            path("sd/video/NVI.mp4"),
            path("sd/video/boot.mp4"),
            path("sd/video/Rani.mp4"),
            path("sd/video/taken.mp4"),
        ];
        let plan = plan_move(&rev_c, &sources, Medium::Internal, &[]).expect("plan");
        let steps: Vec<(String, String, u64)> = plan
            .steps
            .iter()
            .map(|s| (s.source.to_string(), s.target.to_string(), s.size))
            .collect();
        assert_eq!(
            steps,
            [
                (
                    "sd/video/NVI.mp4".into(),
                    "internal/video/nvi.mp4".into(),
                    5_680_675
                ),
                (
                    "sd/video/boot.mp4".into(),
                    "internal/video/boot.mp4".into(),
                    300
                ),
            ]
        );
        let skips: Vec<(String, &str)> = plan
            .skipped
            .iter()
            .map(|s| (s.source.to_string(), s.skip.code()))
            .collect();
        assert_eq!(
            skips,
            [
                ("sd/video/Rani.mp4".into(), "noLocalCopy"),
                ("sd/video/taken.mp4".into(), "conflict"),
            ]
        );
        assert_eq!(
            plan.warnings,
            [Warning::BootMedia(path("sd/video/boot.mp4"))]
        );
        assert_eq!(plan.warnings[0].code(), "bootMedia");
        assert_eq!(plan.bytes(), 5_680_975);
        assert!(plan.transfer.deletes_source());

        // A confirmed overwrite replaces the file of that name, whatever its
        // letter case (a FAT card cannot tell them apart).
        let confirmed = [path("internal/video/taken.mp4")];
        let plan = plan_move(&rev_c, &sources[3..], Medium::Internal, &confirmed).expect("plan");
        assert_eq!(
            plan.steps[0].replaces,
            Some(file("internal/video/TAKEN.mp4", 77))
        );

        // TUR_USB cannot delete: a move skips everything, a copy runs and
        // warns about nothing.
        let usb = view(&catalog, &screen, &now, Deletes::Unsupported, &protected);
        let plan = plan_move(&usb, &sources[..2], Medium::Internal, &[]).expect("plan");
        assert!(plan.steps.is_empty());
        assert!(
            plan.skipped
                .iter()
                .all(|s| s.skip == Skip::DeleteUnsupported)
        );
        let plan = plan_copy(&usb, &sources[..2], Medium::Internal, &[]).expect("plan");
        assert_eq!(plan.steps.len(), 2);
        assert!(plan.warnings.is_empty() && !plan.transfer.deletes_source());

        // Renaming keeps the medium and the extension and warns about a
        // theme's video.
        let amd = path("internal/video/amd.mp4");
        let plan = plan_rename(&rev_c, &amd, "AMD_Logo.MP4", &[]).expect("plan");
        assert_eq!(plan.steps[0].target, path("internal/video/amd_logo.mp4"));
        assert_eq!(plan.warnings, [Warning::ThemeVideo(amd.clone())]);
        assert_eq!(plan.warnings[0].code(), "themeVideo");
        let taken = plan_rename(&rev_c, &amd, "taken.mp4", &[]).expect("plan");
        assert!(matches!(taken.skipped[0].skip, Skip::Conflict(_)));
        for (name, refusal) in [
            (
                "amd.mov",
                PlanRefusal::ExtensionChanged {
                    expected: Some("mp4".into()),
                },
            ),
            ("AMD.mp4", PlanRefusal::SameName),
            (
                "my amd.mp4",
                PlanRefusal::InvalidName(NameError::Forbidden(' ')),
            ),
        ] {
            assert_eq!(plan_rename(&rev_c, &amd, name, &[]), Err(refusal), "{name}");
        }
        let ghost = path("internal/video/ghost.mp4");
        assert_eq!(
            plan_rename(&rev_c, &ghost, "spirit.mp4", &[]),
            Err(PlanRefusal::NotListed(ghost))
        );
        assert_eq!(
            plan_move(&rev_c, std::slice::from_ref(&amd), Medium::Internal, &[]),
            Err(PlanRefusal::SameMedium(amd.clone()))
        );
        let no_card = listing(&[("internal/video/amd.mp4", 400)], None);
        let bare = view(&catalog, &screen, &no_card, Deletes::Supported, &protected);
        assert_eq!(
            plan_copy(&bare, &[amd], Medium::Card, &[]),
            Err(PlanRefusal::NoCard)
        );
    }

    #[test]
    fn a_move_never_sends_the_wrong_bytes() {
        let screen = key();
        // The screen's file has another size than the copy: it is not the
        // file Bezel sent, so it is not moved (deleting it would lose it).
        let catalog = catalog(vec![
            stored("sd/video/a.mp4", Some(CARD), 10, 1),
            ArchiveEntry::pending(path("sd/video/b.mp4"), Some(CARD), 10, id(2), 2),
            stored("sd/video/c.mp4", Some(CARD), 10, 3),
            stored("sd/video/C.mp4", Some(CARD), 10, 4),
            stored("sd/video/Weird Name.MP4", Some(CARD), 10, 5),
        ]);
        let mut copies = catalog.clone();
        copies.copies.remove(&id(3));
        let now = listing(
            &[
                ("sd/video/a.mp4", 11),
                ("sd/video/b.mp4", 10),
                ("sd/video/c.mp4", 10),
                ("sd/video/C.mp4", 10),
                ("sd/video/Weird Name.MP4", 10),
            ],
            Some(CARD),
        );
        let protected = Protected::default();
        let rev_c = view(&copies, &screen, &now, Deletes::Supported, &protected);
        let sources: Vec<RemotePath> = now.files.iter().map(|f| f.path.clone()).collect();
        let plan = plan_move(&rev_c, &sources, Medium::Internal, &[]).expect("plan");
        let skips: Vec<&str> = plan.skipped.iter().map(|s| s.skip.code()).collect();
        // a: another size; b: pending; c: its copy was cleared.
        assert_eq!(skips, ["noLocalCopy", "noLocalCopy", "noLocalCopy"]);
        let targets: Vec<String> = plan.steps.iter().map(|s| s.target.to_string()).collect();
        assert_eq!(
            targets,
            ["internal/video/c.mp4", "internal/video/weird_name.mp4"],
            "upload names; a name the rule refuses gets a suggested one"
        );
        // Two sources that land on one name: the second is a conflict, even
        // with an overwrite confirmed.
        let rev_c = view(&catalog, &screen, &now, Deletes::Supported, &protected);
        let twins = [path("sd/video/c.mp4"), path("sd/video/C.mp4")];
        let confirmed = [path("internal/video/c.mp4")];
        let plan = plan_move(&rev_c, &twins, Medium::Internal, &confirmed).expect("plan");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(
            plan.skipped[0].skip,
            Skip::Conflict(file("internal/video/c.mp4", 10))
        );
    }

    #[test]
    fn restore_checks_space_and_the_cap_before_anything() {
        let screen = key();
        let mut big = stored("sd/video/big.mp4", Some(OTHER_CARD), 30_000_000, 9);
        big.sent_at = 0;
        let mut catalog = catalog(vec![
            stored("sd/video/second.mp4", Some(OTHER_CARD), 200, 3),
            stored("sd/video/first.mp4", Some(OTHER_CARD), 100, 1),
            stored("sd/video/there.mp4", Some(OTHER_CARD), 50, 4),
            stored("sd/video/clash.mp4", Some(OTHER_CARD), 60, 5),
            stored("sd/video/nocopy.mp4", Some(OTHER_CARD), 70, 6),
        ]);
        catalog.copies.remove(&id(6));
        let now = listing(
            &[("sd/video/there.mp4", 50), ("sd/video/Clash.mp4", 61)],
            Some(CARD),
        );
        let protected = Protected::default();
        let rev_c = view(&catalog, &screen, &now, Deletes::Unsupported, &protected);
        let overview = catalog.clone().reconcile(&key(), &now);
        let selection = overview.restorable(Medium::Card);
        assert_eq!(selection.len(), 5);
        let room = Room {
            free: 1000,
            cap: 26_214_400,
        };
        let plan = plan_restore(&rev_c, &selection, Medium::Card, room, &[]).expect("plan");
        let targets: Vec<String> = plan.steps.iter().map(|s| s.target.to_string()).collect();
        assert_eq!(
            targets,
            ["sd/video/first.mp4", "sd/video/second.mp4"],
            "oldest sent first"
        );
        let skips: Vec<(String, &str)> = plan
            .skipped
            .iter()
            .map(|s| (s.source.to_string(), s.skip.code()))
            .collect();
        assert_eq!(
            skips,
            [
                ("sd/video/there.mp4".into(), "present"),
                ("sd/video/clash.mp4".into(), "conflict"),
                ("sd/video/nocopy.mp4".into(), "noLocalCopy"),
            ]
        );
        assert!(plan.warnings.is_empty() && !plan.transfer.deletes_source());
        assert_eq!(plan.transfer.slug(), "restore");
        // A confirmed overwrite sends the conflicting one too.
        let confirmed = [path("sd/video/clash.mp4")];
        let plan = plan_restore(&rev_c, &selection, Medium::Card, room, &confirmed).expect("plan");
        assert_eq!(plan.bytes(), 360);
        // Too little room: refused before the first byte, saying by how much.
        let tight = Room { free: 360, ..room };
        let refused = plan_restore(&rev_c, &selection, Medium::Card, tight, &confirmed);
        assert_eq!(
            refused,
            Err(PlanRefusal::NoSpace {
                needed: 360,
                free: 360
            })
        );
        let text = refused.unwrap_err().to_string();
        assert!(text.contains("(1 bytes short)"), "{text}");
        // A file over the per-file limit refuses the whole restore.
        let mut with_big = selection.clone();
        with_big.push(big);
        catalog.copies.insert(id(9));
        let rev_c = view(&catalog, &screen, &now, Deletes::Unsupported, &protected);
        let refused = plan_restore(&rev_c, &with_big, Medium::Card, room, &[]).unwrap_err();
        assert_eq!(refused.code(), "unsendable");
        assert!(
            refused
                .to_string()
                .starts_with("sd/video/big.mp4: the file is 28.7 MiB")
        );
        // Onto the internal flash, under the same names; no card, no restore.
        let plan = plan_restore(&rev_c, &selection[..2], Medium::Internal, room, &[]);
        assert_eq!(
            plan.expect("plan").steps[0].target,
            path("internal/video/first.mp4")
        );
        let no_card = listing(&[], None);
        let bare = view(&catalog, &screen, &no_card, Deletes::Supported, &protected);
        assert_eq!(
            plan_restore(&bare, &selection, Medium::Card, room, &[]),
            Err(PlanRefusal::NoCard)
        );
    }

    fn video(bytes: u64, size: Size, seconds: Option<u64>) -> MediaInfo {
        MediaInfo {
            format: MediaFormat::Mp4,
            bytes,
            dimensions: Some(size),
            video: Some(VideoTrack {
                codec: VideoCodec::H264,
                pixel_format: Some(VideoPixelFormat::Yuv420p),
                b_frames: Some(false),
                frame_rate: None,
                duration: seconds.map(Duration::from_secs),
            }),
            has_audio: false,
        }
    }

    #[test]
    fn candidates_need_the_exact_size_and_kind() {
        let native = Size::new(480, 1920);
        let wide = Size::new(1920, 480);
        let screen = path("sd/video/NVI.mp427034822.mp4");
        let sought = Sought {
            path: &screen,
            size: 5_352_433,
            resolution: Some(native),
            duration: Some(Duration::from_secs(30)),
        };
        let candidate = |source: &str, media| Candidate {
            source: source.into(),
            media,
        };
        let png = MediaInfo {
            format: MediaFormat::Png,
            bytes: 5_352_433,
            dimensions: Some(native),
            video: None,
            has_audio: false,
        };
        let ranked = rank_candidates(
            &sought,
            vec![
                candidate(
                    "/home/u/clips/other.mp4",
                    video(5_352_433, native, Some(30)),
                ),
                candidate("/home/u/clips/nvi.mp4", video(5_352_434, native, Some(30))),
                candidate("C:\\clips\\NVIDIA wide.mp4", video(5_352_433, wide, None)),
                candidate("/home/u/clips/NVI.mp4", video(5_352_433, wide, Some(90))),
                candidate(
                    "/home/u/clips/nvi (copy).mp4",
                    video(5_352_433, native, Some(30)),
                ),
                candidate("/home/u/clips/nvi.png", png),
            ],
        );
        let order: Vec<&str> = ranked.iter().map(|c| c.source.as_str()).collect();
        assert_eq!(
            order,
            [
                "/home/u/clips/NVI.mp4",
                "/home/u/clips/nvi (copy).mp4",
                "C:\\clips\\NVIDIA wide.mp4",
                "/home/u/clips/other.mp4",
            ]
        );
        let unknown = Sought {
            resolution: None,
            duration: None,
            ..sought
        };
        let ranked = rank_candidates(
            &unknown,
            vec![
                candidate("/b/x.mp4", video(5_352_433, native, Some(1))),
                candidate("/a/x.mp4", video(5_352_433, wide, None)),
            ],
        );
        assert_eq!(ranked[0].source, "/a/x.mp4", "a tie goes by source");
    }

    #[test]
    fn keys_ids_and_states_read_back() {
        let digest: [u8; 32] = std::array::from_fn(|i| i as u8 * 8);
        let content = ContentId::from_digest(digest);
        assert_eq!(&content.as_str()[..8], "00081018");
        assert_eq!(content.as_str().len(), 64);
        assert_eq!(
            ContentId::parse(&content.as_str().to_uppercase()),
            Some(content.clone())
        );
        assert_eq!(content.to_string(), content.as_str());
        assert_eq!(ContentId::parse("abc"), None);
        assert_eq!(ContentId::parse(&"g".repeat(64)), None);
        assert_eq!(key().to_string(), "turing-8.8");
        assert_eq!(
            ScreenKey::named(ModelId("turing-8.8"), "Desk").to_string(),
            "turing-8.8 (Desk)"
        );
        assert_eq!(ScreenKey::named(ModelId("turing-8.8"), "  "), key());
        for state in EntryState::ALL {
            assert_eq!(EntryState::from_slug(state.slug()), Some(state));
        }
        assert_eq!(EntryState::from_slug("gone"), None);
        assert_eq!(
            upload_name(&FileName::parse("NVI.mp4").expect("name")).as_str(),
            "nvi.mp4"
        );
        let slugs = [
            Transfer::Move,
            Transfer::Copy,
            Transfer::Rename,
            Transfer::Restore,
        ]
        .map(Transfer::slug);
        assert_eq!(slugs, ["move", "copy", "rename", "restore"]);
        let texts = [
            (PlanRefusal::NoCard, "noCard", "no memory card"),
            (
                PlanRefusal::NotListed(path("sd/video/a.mp4")),
                "notListed",
                "sd/video/a.mp4 is not on the screen",
            ),
            (
                PlanRefusal::SameMedium(path("sd/video/a.mp4")),
                "sameMedium",
                "already on that medium",
            ),
            (
                PlanRefusal::InvalidName(NameError::Empty),
                "invalidName",
                "invalid file name: the name is empty",
            ),
            (
                PlanRefusal::ExtensionChanged {
                    expected: Some("mp4".into()),
                },
                "extensionChanged",
                "must end in .mp4",
            ),
            (
                PlanRefusal::ExtensionChanged { expected: None },
                "extensionChanged",
                "cannot have an extension",
            ),
            (PlanRefusal::SameName, "sameName", "letter case aside"),
            (
                PlanRefusal::NoSpace {
                    needed: 10,
                    free: 4,
                },
                "noSpace",
                "need 10 bytes and 4 are free (7 bytes short); nothing was sent",
            ),
        ];
        for (refusal, code, text) in texts {
            assert_eq!(refusal.code(), code);
            assert!(refusal.to_string().contains(text), "{refusal}");
        }
        let skips = [
            Skip::Conflict(file("sd/video/a.mp4", 1)),
            Skip::NoLocalCopy,
            Skip::DeleteUnsupported,
            Skip::Present,
        ]
        .map(|s| s.code());
        assert_eq!(
            skips,
            ["conflict", "noLocalCopy", "deleteUnsupported", "present"]
        );
    }
}
