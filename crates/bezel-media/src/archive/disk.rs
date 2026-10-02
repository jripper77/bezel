//! [`DiskArchive`]: the local copies, their catalog and their thumbnails in a
//! folder of the user's data (D-2026-09-30-storage-manager-5):
//! - `catalog.json`: the catalog, versioned by its `schema` number (the serde
//!   shapes live here, never in the core), replaced atomically: written whole
//!   to a temporary file next to it, flushed, then renamed over it;
//! - `files/<sha256>.<ext>`: one copy per content, its extension told from its
//!   bytes (`mp4`, `h264`, `png`, `jpg`, `gif`, `bmp`, else `bin`), found by
//!   its id whatever the extension;
//! - `thumbs/<sha256>.png`: thumbnails, made on demand and kept when the copy
//!   goes.
//!
//! A catalog or a copy that cannot be read is an error naming the file: the
//! store never panics and never answers with an empty catalog instead.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use bezel_core::domain::archive::{Catalog, ContentId};
use bezel_core::domain::media::MediaFormat;
use bezel_core::ports::{ArchiveStore, MediaTranscoder};
use bezel_core::{BezelError, Result};
use image::ImageFormat;

use super::{content_id, thumbs};
use crate::mp4;

const CATALOG: &str = "catalog.json";
const FILES: &str = "files";
const THUMBS: &str = "thumbs";
/// What [`extension_of`] can name a copy, looked for before listing the
/// folder.
const EXTENSIONS: [&str; 7] = ["mp4", "h264", "png", "jpg", "gif", "bmp", "bin"];
/// Times a replace or a removal the OS refuses as busy is tried again.
const RETRIES: u32 = 8;
/// Pause before the first retry; each one waits longer.
const PAUSE: Duration = Duration::from_millis(15);

/// The [`ArchiveStore`] in a folder: the catalog, one file per copy and the
/// thumbnails. It holds only the folder's path, so a clone reaches the same
/// store (for making thumbnails off the UI thread).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskArchive {
    root: PathBuf,
}

impl DiskArchive {
    /// The store in `root` (usually [`storage_dir`](super::storage_dir) of
    /// the user's data folder), its folders created when missing. Nothing is
    /// read until [`ArchiveStore::load`].
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let archive = Self { root: root.into() };
        for dir in [archive.files_dir(), archive.thumbs_dir()] {
            fs::create_dir_all(&dir).map_err(|e| failed("cannot create", &dir, &e))?;
        }
        Ok(archive)
    }

    /// The store's folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file holding the copy of `content`, whatever its extension;
    /// `None` when no copy is kept.
    pub fn copy_path(&self, content: &ContentId) -> Result<Option<PathBuf>> {
        let dir = self.files_dir();
        let usual = EXTENSIONS
            .iter()
            .map(|ext| dir.join(format!("{content}.{ext}")))
            .find(|path| path.is_file());
        match usual {
            Some(path) => Ok(Some(path)),
            None => Ok(self.copies_of(content)?.into_iter().next()),
        }
    }

    /// The thumbnail of `content` as PNG bytes, fitting
    /// [`THUMBNAIL_EDGE`](super::THUMBNAIL_EDGE) pixels
    /// (D-2026-09-30-storage-manager-10). One already made is kept in
    /// `thumbs/`, also after the copy is discarded; otherwise it is made from
    /// the copy: an image scaled down, a video's picture at 1 s taken by
    /// `media` ([`MediaTranscoder::poster`], ffmpeg). `None` without a copy,
    /// or for a video when `media` cannot take pictures (no ffmpeg): the
    /// caller shows the generic icon, and a later call tries again. Slow for
    /// videos: run it off the UI thread, on a clone.
    pub fn thumbnail(
        &self,
        content: &ContentId,
        media: &mut dyn MediaTranscoder,
    ) -> Result<Option<Vec<u8>>> {
        let kept = self.thumb_path(content);
        match fs::read(&kept) {
            Ok(png) => return Ok(Some(png)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(failed("cannot read", &kept, &e)),
        }
        let Some(copy) = self.copy_path(content)? else {
            return Ok(None);
        };
        let Some(png) = thumbs::make(&copy, media)? else {
            return Ok(None);
        };
        write_atomically(&kept, &png)?;
        Ok(Some(png))
    }

    fn catalog_path(&self) -> PathBuf {
        self.root.join(CATALOG)
    }

    fn files_dir(&self) -> PathBuf {
        self.root.join(FILES)
    }

    fn thumbs_dir(&self) -> PathBuf {
        self.root.join(THUMBS)
    }

    fn thumb_path(&self, content: &ContentId) -> PathBuf {
        self.thumbs_dir().join(format!("{content}.png"))
    }

    /// Every file of `files/` named `<id>` or `<id>.<anything>`: one, unless
    /// a copy was put there by hand. Temporary files start with a dot.
    fn copies_of(&self, content: &ContentId) -> Result<Vec<PathBuf>> {
        let dir = self.files_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(failed("cannot list", &dir, &e)),
        };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .filter(|entry| names_copy(&entry.file_name(), content))
            .map(|entry| entry.path())
            .collect();
        found.sort();
        Ok(found)
    }
}

fn names_copy(file: &OsStr, content: &ContentId) -> bool {
    let id = content.as_str();
    file.to_str()
        .and_then(|name| name.strip_prefix(id))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

impl ArchiveStore for DiskArchive {
    fn load(&mut self) -> Result<Catalog> {
        let path = self.catalog_path();
        let text = match fs::read(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Catalog::default()),
            Err(e) => return Err(failed("cannot read", &path, &e)),
        };
        dto::decode(&text).map_err(|why| {
            BezelError::InvalidInput(format!(
                "{} is not a readable catalog: {why}",
                path.display()
            ))
        })
    }

    fn save(&mut self, catalog: &Catalog) -> Result<()> {
        write_atomically(&self.catalog_path(), &dto::encode(catalog)?)
    }

    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId> {
        let id = content_id(bytes);
        if let Some(kept) = self.copy_path(&id)? {
            if fs::read(&kept).is_ok_and(|held| held == bytes) {
                return Ok(id);
            }
            // A damaged copy: replaced by these bytes.
            remove(&kept)?;
        }
        let target = self
            .files_dir()
            .join(format!("{id}.{}", extension_of(bytes)));
        write_atomically(&target, bytes)?;
        Ok(id)
    }

    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        let Some(path) = self.copy_path(content)? else {
            return Ok(None);
        };
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(failed("cannot read", &path, &e)),
        };
        if content_id(&bytes) != *content {
            return Err(BezelError::InvalidInput(format!(
                "{} is damaged: its bytes are not the copy it names",
                path.display()
            )));
        }
        Ok(Some(bytes))
    }

    fn discard(&mut self, content: &ContentId) -> Result<()> {
        for path in self.copies_of(content)? {
            remove(&path)?;
        }
        Ok(())
    }
}

/// What the bytes of a copy are, from their first bytes: the formats screens
/// store (MP4, raw H.264, PNG, JPEG, GIF, BMP), else `Other`.
pub(super) fn format_of(bytes: &[u8]) -> MediaFormat {
    if mp4::sniff(bytes) {
        return MediaFormat::Mp4;
    }
    if let Ok(format) = image::guess_format(bytes) {
        return match format {
            ImageFormat::Png => MediaFormat::Png,
            ImageFormat::Jpeg => MediaFormat::Jpeg,
            ImageFormat::Gif => MediaFormat::Gif,
            ImageFormat::Bmp => MediaFormat::Bmp,
            _ => MediaFormat::Other,
        };
    }
    if bytes.starts_with(&[0, 0, 0, 1]) || bytes.starts_with(&[0, 0, 1]) {
        // An Annex-B start code: the TUR_USB raw H.264 stream.
        return MediaFormat::H264;
    }
    MediaFormat::Other
}

/// The extension of a copy's file: its format's usual one, else `bin`.
fn extension_of(bytes: &[u8]) -> &'static str {
    format_of(bytes)
        .extensions()
        .first()
        .copied()
        .unwrap_or("bin")
}

/// An I/O failure on `path`.
pub(crate) fn failed(what: &str, path: &Path, e: &io::Error) -> BezelError {
    BezelError::Transport(format!("{what} {}: {e}", path.display()))
}

/// Writes `bytes` to `path` so that `path` is always whole: into a temporary
/// file next to it, flushed to disk and closed, then renamed over it. When
/// that fails the previous file stays and the temporary one is removed.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| failed("cannot create", dir, &e))?;
    }
    let temp = temporary(path);
    let written = write_closed(&temp, bytes).and_then(|()| patiently(|| fs::rename(&temp, path)));
    written.map_err(|e| {
        // Best effort: a leftover temporary file is never read.
        let _ = fs::remove_file(&temp);
        failed("cannot write", path, &e)
    })
}

/// Writes `bytes` to `path` and closes it: Windows renames only a closed
/// file.
fn write_closed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// A temporary name next to `path`, unique in this process and among
/// processes: `.<name>.<pid>-<n>.tmp`.
fn temporary(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.{}-{n}.tmp", std::process::id()))
}

/// Removes `path`; a file already gone is not an error.
pub(crate) fn remove(path: &Path) -> Result<()> {
    match patiently(|| fs::remove_file(path)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(failed("cannot remove", path, &e)),
        _ => Ok(()),
    }
}

/// Runs `op`, again while the OS calls the file busy: Windows refuses to
/// replace or remove a file another program (an indexer, an antivirus) holds
/// open without sharing it. Bounded; elsewhere `op` runs once.
fn patiently<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut tries = 0;
    loop {
        match op() {
            Err(e) if tries < RETRIES && busy(&e) => {
                tries += 1;
                thread::sleep(PAUSE * tries);
            }
            done => return done,
        }
    }
}

/// Whether `e` is Windows saying another handle holds the file (access
/// denied, sharing or lock violation).
fn busy(e: &io::Error) -> bool {
    cfg!(windows)
        && (matches!(
            e.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy
        ) || matches!(e.raw_os_error(), Some(32 | 33)))
}

/// `catalog.json`, schema 1: serde shapes and their mapping to the core's
/// catalog.
mod dto {
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Duration;

    use bezel_core::domain::archive::{
        ArchiveEntry, Catalog, ContentId, EntryState, ScreenKey, ScreenRecord,
    };
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::storage::RemotePath;
    use bezel_core::{BezelError, Result};
    use serde::{Deserialize, Serialize};

    /// The schema this build writes and the newest it reads.
    const SCHEMA: u32 = 1;

    type R<T> = std::result::Result<T, String>;

    /// Read first, so that a newer file says so rather than failing on a
    /// field it changed.
    #[derive(Deserialize)]
    struct Header {
        schema: Option<u32>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CatalogDto {
        schema: u32,
        limit: u64,
        #[serde(default)]
        copies: Vec<String>,
        #[serde(default)]
        screens: Vec<ScreenDto>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ScreenDto {
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// `<internal|sd>/<image|video>/<name>`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        boot: Option<String>,
        #[serde(default)]
        entries: Vec<EntryDto>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct EntryDto {
        /// `<internal|sd>/<image|video>/<name>`.
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        card: Option<u64>,
        size: u64,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        /// Nanoseconds, so that a probed duration reloads exactly.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ns: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution: Option<SizeDto>,
        sent_at: u64,
        /// `pending`, `stored`, `missing` or `deleted`.
        state: String,
    }

    #[derive(Serialize, Deserialize)]
    struct SizeDto {
        width: u32,
        height: u32,
    }

    /// `catalog` as the text of `catalog.json`.
    pub(super) fn encode(catalog: &Catalog) -> Result<Vec<u8>> {
        let dto = CatalogDto {
            schema: SCHEMA,
            limit: catalog.limit,
            copies: catalog.copies.iter().map(|c| c.to_string()).collect(),
            screens: catalog.screens.iter().map(ScreenDto::of).collect(),
        };
        serde_json::to_vec_pretty(&dto).map_err(|e| {
            BezelError::InvalidInput(format!("the catalog cannot be written as JSON: {e}"))
        })
    }

    /// The catalog in `text`, or why it is not one.
    pub(super) fn decode(text: &[u8]) -> R<Catalog> {
        let header: Header = serde_json::from_slice(text).map_err(|e| e.to_string())?;
        match header.schema {
            Some(SCHEMA) => {}
            Some(newer) if newer > SCHEMA => {
                return Err(format!(
                    "it was written by a newer Bezel (schema {newer}; this one reads {SCHEMA})"
                ));
            }
            Some(other) => return Err(format!("unknown schema {other}")),
            None => return Err("it has no schema number".to_string()),
        }
        let dto: CatalogDto = serde_json::from_slice(text).map_err(|e| e.to_string())?;
        dto.into_core()
    }

    fn content(text: &str) -> R<ContentId> {
        ContentId::parse(text).ok_or_else(|| format!("{text:?} is not a SHA-256"))
    }

    fn path(text: &str) -> R<RemotePath> {
        RemotePath::parse(text).map_err(|_| format!("{text:?} is not a screen path"))
    }

    impl CatalogDto {
        fn into_core(self) -> R<Catalog> {
            let copies = self.copies.iter().map(|c| content(c));
            let mut catalog = Catalog {
                limit: self.limit,
                screens: BTreeMap::new(),
                copies: copies.collect::<R<BTreeSet<_>>>()?,
            };
            for screen in self.screens {
                let (key, record) = screen.into_core()?;
                if catalog.screens.contains_key(&key) {
                    return Err(format!("screen {key} is listed twice"));
                }
                catalog.screens.insert(key, record);
            }
            Ok(catalog)
        }
    }

    impl ScreenDto {
        fn of((key, record): (&ScreenKey, &ScreenRecord)) -> Self {
            Self {
                model: key.model.clone(),
                name: key.name.clone(),
                boot: record.boot.as_ref().map(RemotePath::to_string),
                entries: record.entries.iter().map(EntryDto::of).collect(),
            }
        }

        fn into_core(self) -> R<(ScreenKey, ScreenRecord)> {
            let key = ScreenKey {
                model: self.model,
                name: self.name,
            };
            let in_screen = |why: String| format!("screen {key}: {why}");
            let boot = self.boot.as_deref().map(path).transpose();
            let entries = self.entries.into_iter().map(EntryDto::into_core);
            let record = ScreenRecord {
                boot: boot.map_err(in_screen)?,
                entries: entries.collect::<R<Vec<_>>>().map_err(in_screen)?,
            };
            Ok((key, record))
        }
    }

    impl EntryDto {
        fn of(entry: &ArchiveEntry) -> Self {
            Self {
                path: entry.path.to_string(),
                card: entry.card,
                size: entry.size,
                content: entry.content.to_string(),
                source: entry.source.clone(),
                duration_ns: entry
                    .duration
                    .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)),
                resolution: entry.resolution.map(|s| SizeDto {
                    width: s.width,
                    height: s.height,
                }),
                sent_at: entry.sent_at,
                state: entry.state.slug().to_string(),
            }
        }

        fn into_core(self) -> R<ArchiveEntry> {
            let at = |why: String| format!("{}: {why}", self.path);
            let state = EntryState::from_slug(&self.state)
                .ok_or_else(|| at(format!("unknown state {:?}", self.state)))?;
            Ok(ArchiveEntry {
                path: path(&self.path)?,
                card: self.card,
                size: self.size,
                content: content(&self.content).map_err(at)?,
                source: self.source,
                duration: self.duration_ns.map(Duration::from_nanos),
                resolution: self.resolution.map(|s| Size::new(s.width, s.height)),
                sent_at: self.sent_at,
                state,
            })
        }
    }
}
