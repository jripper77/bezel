//! [`DiskCollection`]: the user's GIFs and stickers in a folder of their data
//! (D-2026-10-01-gif-sticker-search-5):
//! - `collection.json`: the index, versioned by its `schema` number (the
//!   serde shapes live here, never in the core), replaced atomically: written
//!   whole to a temporary file next to it, flushed, then renamed over it;
//! - `files/<sha256>.gif`: one GIF per content;
//! - `previews/<sha256>.gif`: the preview of each GIF.
//!
//! Files are named by content id only (a SHA-256 in hex): the names the user
//! gives live in the index and never reach a path. An index that cannot be
//! read is an error naming the file, which is left as it is: the store never
//! answers with an empty collection instead, so a collection never vanishes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bezel_core::domain::archive::ContentId;
use bezel_core::domain::gifs::Collection;
use bezel_core::ports::GifCollection;
use bezel_core::{BezelError, Result};

use crate::archive::{content_id, failed, remove, write_atomically};

const INDEX: &str = "collection.json";
const FILES: &str = "files";
const PREVIEWS: &str = "previews";

/// The [`GifCollection`] in a folder: the index, one file per GIF and the
/// previews. It holds only the folder's path, so a clone reaches the same
/// collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskCollection {
    root: PathBuf,
}

impl DiskCollection {
    /// The collection in `root` (usually [`collection_dir`](super::collection_dir)
    /// of the user's data folder), its folders created when missing. Nothing
    /// is read until [`GifCollection::load`].
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let collection = Self { root: root.into() };
        for dir in [collection.files_dir(), collection.previews_dir()] {
            fs::create_dir_all(&dir).map_err(|e| failed("cannot create", &dir, &e))?;
        }
        Ok(collection)
    }

    /// The collection's folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX)
    }

    fn files_dir(&self) -> PathBuf {
        self.root.join(FILES)
    }

    fn previews_dir(&self) -> PathBuf {
        self.root.join(PREVIEWS)
    }

    fn file_path(&self, content: &ContentId) -> PathBuf {
        self.files_dir().join(gif_name(content))
    }

    fn preview_path(&self, content: &ContentId) -> PathBuf {
        self.previews_dir().join(gif_name(content))
    }
}

/// The file name of what is kept for `content`: its hex SHA-256, never a
/// name the user typed.
fn gif_name(content: &ContentId) -> String {
    format!("{content}.gif")
}

/// The bytes in `path`; `None` when there is no such file.
fn read_if_any(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(failed("cannot read", path, &e)),
    }
}

impl GifCollection for DiskCollection {
    fn load(&mut self) -> Result<Collection> {
        let path = self.index_path();
        let Some(text) = read_if_any(&path)? else {
            return Ok(Collection::default());
        };
        dto::decode(&text).map_err(|why| {
            BezelError::InvalidInput(format!(
                "{} is not a readable collection: {why}",
                path.display()
            ))
        })
    }

    fn save(&mut self, index: &Collection) -> Result<()> {
        write_atomically(&self.index_path(), &dto::encode(index)?)
    }

    fn keep(&mut self, bytes: &[u8]) -> Result<ContentId> {
        let id = content_id(bytes);
        let path = self.file_path(&id);
        // Kept once: the same bytes are not written again. A damaged copy
        // is replaced by these bytes.
        if fs::read(&path).is_ok_and(|held| held == bytes) {
            return Ok(id);
        }
        write_atomically(&path, bytes)?;
        Ok(id)
    }

    fn read(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        let path = self.file_path(content);
        let Some(bytes) = read_if_any(&path)? else {
            return Ok(None);
        };
        if content_id(&bytes) != *content {
            return Err(BezelError::InvalidInput(format!(
                "{} is damaged: its bytes are not the GIF it names",
                path.display()
            )));
        }
        Ok(Some(bytes))
    }

    fn keep_preview(&mut self, content: &ContentId, bytes: &[u8]) -> Result<()> {
        write_atomically(&self.preview_path(content), bytes)
    }

    fn read_preview(&mut self, content: &ContentId) -> Result<Option<Vec<u8>>> {
        read_if_any(&self.preview_path(content))
    }

    fn discard(&mut self, content: &ContentId) -> Result<()> {
        remove(&self.file_path(content))?;
        remove(&self.preview_path(content))
    }
}

/// `collection.json`, schema 1: serde shapes and their mapping to the core's
/// index.
mod dto {
    use bezel_core::domain::archive::ContentId;
    use bezel_core::domain::gifs::{CollectedGif, Collection, GifKind, GifOrigin, item_name};
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
    struct IndexDto {
        schema: u32,
        #[serde(default)]
        items: Vec<ItemDto>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ItemDto {
        /// The SHA-256 of the GIF: the name of its file and of its preview.
        content: String,
        name: String,
        /// `gif` or `sticker`.
        kind: String,
        width: u32,
        height: u32,
        bytes: u64,
        /// Seconds since the Unix epoch.
        added_at: u64,
        source: SourceDto,
    }

    #[derive(Serialize, Deserialize)]
    struct SourceDto {
        provider: String,
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    }

    /// `index` as the text of `collection.json`.
    pub(super) fn encode(index: &Collection) -> Result<Vec<u8>> {
        let dto = IndexDto {
            schema: SCHEMA,
            items: index.items().iter().map(ItemDto::of).collect(),
        };
        serde_json::to_vec_pretty(&dto).map_err(|e| {
            BezelError::InvalidInput(format!("the collection cannot be written as JSON: {e}"))
        })
    }

    /// The index in `text`, or why it is not one.
    pub(super) fn decode(text: &[u8]) -> R<Collection> {
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
        let dto: IndexDto = serde_json::from_slice(text).map_err(|e| e.to_string())?;
        let items = dto.items.into_iter().map(ItemDto::into_core);
        Ok(Collection::new(items.collect::<R<Vec<_>>>()?))
    }

    impl ItemDto {
        fn of(item: &CollectedGif) -> Self {
            Self {
                content: item.content.to_string(),
                name: item.name.clone(),
                kind: item.kind.slug().to_string(),
                width: item.width,
                height: item.height,
                bytes: item.bytes,
                added_at: item.added_at,
                source: SourceDto {
                    provider: item.origin.provider.clone(),
                    id: item.origin.id.clone(),
                    url: item.origin.page_url.clone(),
                },
            }
        }

        fn into_core(self) -> R<CollectedGif> {
            let content = ContentId::parse(&self.content)
                .ok_or_else(|| format!("{:?} is not a SHA-256", self.content))?;
            let at = |why: String| format!("{content}: {why}");
            let kind = GifKind::from_slug(&self.kind)
                .ok_or_else(|| at(format!("unknown kind {:?}", self.kind)))?;
            let name = item_name(&self.name).ok_or_else(|| at("it has no name".to_string()))?;
            Ok(CollectedGif {
                content,
                name,
                kind,
                width: self.width,
                height: self.height,
                bytes: self.bytes,
                added_at: self.added_at,
                origin: GifOrigin {
                    provider: self.source.provider,
                    id: self.source.id,
                    page_url: self.source.url,
                },
            })
        }
    }
}
