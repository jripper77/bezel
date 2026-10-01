//! Associating a screen file with its original on the PC
//! (D-2026-09-30-storage-manager-10): the files or folder the user picked
//! are searched for files of exactly the screen file's size, the core ranks
//! them, and the pair the user confirms gives the file its local copy.

use std::path::{Path, PathBuf};

use bezel_core::BezelError;
use bezel_core::domain::archive::ScreenKey;
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::screen::Confirm;
use bezel_core::ports::MediaLocation;

use super::{CandidateDto, CandidatesDto, CatalogEntryDto, ManagedFileDto, protected, protection};
use crate::backend::Backend;
use crate::clock::unix_seconds;
use crate::dto::StoredFileDto;
use crate::messages::UiResult;
use crate::storage::remote;
use crate::studio::Resume;

/// How deep a picked folder is searched.
const MAX_DEPTH: usize = 3;

/// Most files of a picked folder looked at.
const MAX_FILES: usize = 4096;

/// The files under `dir`, at most [`MAX_DEPTH`] folders down (links are not
/// followed), until `out` holds [`MAX_FILES`].
fn files_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_FILES {
            return;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth < MAX_DEPTH {
            files_in(&entry.path(), depth + 1, out);
        } else if kind.is_file() {
            out.push(entry.path());
        }
    }
}

/// The files `sources` name (files, and the files of folders), of exactly
/// `size` bytes when it is known, each once, in order.
pub(super) fn expand_sources(sources: &[String], size: Option<u64>) -> Vec<MediaLocation> {
    let mut files = Vec::new();
    for source in sources {
        let path = PathBuf::from(source);
        if path.is_dir() {
            files_in(&path, 0, &mut files);
        } else {
            files.push(path);
        }
    }
    let fits = |path: &PathBuf| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && size.is_none_or(|s| m.len() == s))
    };
    files.retain(fits);
    files.sort();
    files.dedup();
    files
        .into_iter()
        .map(|p| MediaLocation(p.display().to_string()))
        .collect()
}

impl Backend {
    /// The originals of the screen file at `path` among `sources` (files or
    /// folders the user picked), likeliest first: exactly its size and kind,
    /// ranked by name, resolution and duration. Only queries the screen.
    pub fn associate_candidates(
        &self,
        screen: &str,
        path: &str,
        sources: &[String],
        time: LocalTime,
    ) -> UiResult<CandidatesDto> {
        let path = remote(path)?;
        let size = self
            .storage
            .shown()
            .file(screen, &path.to_string())
            .and_then(|f| f.size);
        let locations = expand_sources(sources, size);
        self.with_desk(screen, Resume::Frames, time, |desk| {
            let mut media = self.storage.media();
            let found = desk
                .manager()
                .originals(media.as_mut(), &path, &locations)?;
            let candidates = found.iter().map(|c| CandidateDto::of(c, &path));
            Ok(CandidatesDto {
                candidates: candidates.collect(),
            })
        })
    }

    /// Associates the screen file at `path` with its original `source`, the
    /// pair the user confirmed (`confirm`): the original's bytes become the
    /// file's local copy, so it gets a thumbnail and can be moved. The file
    /// as the list now shows it. Only queries the screen.
    pub fn associate_original(
        &self,
        screen: &str,
        path: &str,
        source: &str,
        confirm: Confirm,
        time: LocalTime,
    ) -> UiResult<ManagedFileDto> {
        let path = remote(path)?;
        if confirm == Confirm::No {
            let asked = format!("associating {path} with {source}");
            return Err(BezelError::NotConfirmed(asked).into());
        }
        let original = MediaLocation(source.to_string());
        let (dto, content) = self.with_desk(screen, Resume::Frames, time, |desk| {
            let mut media = self.storage.media();
            let entry =
                desk.manager()
                    .associate(media.as_mut(), &path, &original, unix_seconds())?;
            let model = desk.model();
            let catalog = desk.store.load()?;
            let boot = catalog
                .screen(&ScreenKey::new(model.id))
                .and_then(|r| r.boot.clone());
            let protected = protected(boot, desk.videos, model);
            let dto = ManagedFileDto {
                file: StoredFileDto::at(&path, Some(entry.size)),
                entry: Some(CatalogEntryDto::of(
                    &entry,
                    catalog.has_copy(&entry.content),
                )),
                finding: None,
                protected: protection(&protected, &path),
            };
            Ok((dto, entry.content))
        })?;
        self.storage.shown().set_content(screen, &path, content);
        Ok(dto)
    }
}
