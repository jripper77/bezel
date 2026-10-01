//! GIF and sticker use cases (D-2026-10-01-gif-sticker-search-4, -5):
//! searching a provider, reading a result's preview, and the user's
//! collection (add, rename, delete).
//!
//! The provider is reached only from these functions, each run on a user
//! action; the collection's bytes are addressed by content, so the same GIF
//! is kept once whatever results it came from.

use crate::domain::archive::ContentId;
use crate::domain::gifs::{
    CollectedGif, GifItem, GifOrigin, GifPage, GifQuery, ITEM_LIMIT, Motion, PREVIEW_LIMIT,
    Rendition, RenditionFormat, gif_size, is_gif,
};
use crate::domain::screen::Confirm;
use crate::ports::{GifCollection, GifSource};
use crate::{BezelError, Result};

/// One page of results for `query`. Items that cannot be collected (no GIF
/// of at most 25 MiB) are left out.
pub fn search(source: &dyn GifSource, query: &GifQuery) -> Result<GifPage> {
    let mut page = source.page(query)?;
    page.items.retain(|item| item.download().is_some());
    Ok(page)
}

/// The preview of `item`: its small GIF, or its JPEG still for
/// [`Motion::Still`], read up to 2 MiB; `None` when it has no such
/// rendition. Bytes that are not in the rendition's format are
/// `InvalidInput`.
pub fn preview(source: &dyn GifSource, item: &GifItem, motion: Motion) -> Result<Option<Vec<u8>>> {
    let Some(rendition) = item.preview(motion) else {
        return Ok(None);
    };
    let bytes = read(source, rendition, PREVIEW_LIMIT)?;
    if RenditionFormat::of(&bytes) != Some(rendition.format) {
        return Err(BezelError::InvalidInput(format!(
            "the preview of {} is not a {:?} file",
            item.name(),
            rendition.format
        )));
    }
    Ok(Some(bytes))
}

/// Adds `item` to the collection: its largest GIF of at most 25 MiB, read
/// with that limit. Bytes that are not a GIF are `InvalidInput` and nothing
/// is kept. A GIF the collection already holds (the same bytes from another
/// result) gives the item already there, unchanged. A new item is named
/// after its title and keeps a preview: `preview` when the caller already
/// read one and it is a GIF, else the item's small GIF, read now (a preview
/// that cannot be read leaves the item without one). `added_at` is the time
/// recorded, in seconds since the Unix epoch.
pub fn add_to_collection(
    source: &dyn GifSource,
    collection: &mut dyn GifCollection,
    item: &GifItem,
    preview: Option<&[u8]>,
    added_at: u64,
) -> Result<CollectedGif> {
    let mut index = collection.load()?;
    let rendition = item.download().ok_or_else(|| {
        BezelError::InvalidInput(format!("{} has no GIF of at most 25 MiB", item.name()))
    })?;
    let bytes = read(source, rendition, ITEM_LIMIT)?;
    let Some((width, height)) = gif_size(&bytes) else {
        return Err(BezelError::InvalidInput(format!(
            "what was downloaded for {} is not a GIF",
            item.name()
        )));
    };
    let content = collection.keep(&bytes)?;
    if let Some(existing) = index.get(&content) {
        return Ok(existing.clone());
    }
    if let Some(small) = preview_of(source, item, preview) {
        collection.keep_preview(&content, &small)?;
    }
    let collected = CollectedGif {
        content,
        name: item.name(),
        kind: item.kind,
        width,
        height,
        bytes: bytes.len() as u64,
        added_at,
        origin: GifOrigin {
            provider: source.provider().to_string(),
            id: item.id.clone(),
            page_url: item.page_url.clone(),
        },
    };
    index.add(collected.clone());
    collection.save(&index)?;
    Ok(collected)
}

/// Renames the item kept as `content` to `name`, trimmed: the renamed item,
/// `None` when the collection has no such item (nothing is saved), and
/// `InvalidInput` for an empty name.
pub fn rename(
    collection: &mut dyn GifCollection,
    content: &ContentId,
    name: &str,
) -> Result<Option<CollectedGif>> {
    let mut index = collection.load()?;
    let Some(renamed) = index.rename(content, name)?.cloned() else {
        return Ok(None);
    };
    collection.save(&index)?;
    Ok(Some(renamed))
}

/// Deletes the item kept as `content` from the collection, with its bytes
/// and preview: the deleted item, `None` when there is no such item. Needs
/// `Confirm::Yes`; with `Confirm::No` nothing is read or changed. Themes
/// that use the GIF keep their own copy.
pub fn delete(
    collection: &mut dyn GifCollection,
    content: &ContentId,
    confirm: Confirm,
) -> Result<Option<CollectedGif>> {
    if confirm == Confirm::No {
        return Err(BezelError::NotConfirmed(
            "deleting an item of the collection".to_string(),
        ));
    }
    let mut index = collection.load()?;
    let Some(deleted) = index.remove(content) else {
        return Ok(None);
    };
    collection.save(&index)?;
    collection.discard(content)?;
    Ok(Some(deleted))
}

/// The bytes of `rendition`, read with `limit`, refused when the source
/// gave more.
fn read(source: &dyn GifSource, rendition: &Rendition, limit: u64) -> Result<Vec<u8>> {
    let bytes = source.download(rendition, limit)?;
    if bytes.len() as u64 > limit {
        return Err(BezelError::InvalidInput(format!(
            "the file is larger than {limit} bytes"
        )));
    }
    Ok(bytes)
}

/// The preview a new item keeps: `given` when it is a GIF of at most 2 MiB,
/// else the item's small GIF; `None` when there is none or it cannot be
/// read.
fn preview_of(source: &dyn GifSource, item: &GifItem, given: Option<&[u8]>) -> Option<Vec<u8>> {
    let fits = |bytes: &&[u8]| is_gif(bytes) && bytes.len() as u64 <= PREVIEW_LIMIT;
    match given.filter(fits) {
        Some(bytes) => Some(bytes.to_vec()),
        None => preview(source, item, Motion::Animated).ok().flatten(),
    }
}
