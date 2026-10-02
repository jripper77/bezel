//! GIF and sticker search and the collection, behind the window's GIF
//! commands (D-2026-10-01-gif-sticker-search-2..-5): [`Gifs`] is its own
//! Tauri state, beside the [`Backend`], and runs the core's `app::gifs` use
//! cases over a [`GifSource`] made for the user's key and the user's
//! [`GifCollection`].
//!
//! - The key lives in [`KeyFile`] (`<config>/klipy.json`); the window only
//!   learns whether one is saved and its last 4 characters ([`KeyDto`]).
//!   Saving it asks nothing of the provider. From the command on, the key is
//!   a [`KlipyKey`], in the key file's JSON too: printing or logging it
//!   does not compile, no `String` of it is made, and its text is read in
//!   two places only, as an argument of `KlipyClient::new` in the source
//!   factory and by the key file's serializer (`write_key` in `key.rs`).
//! - Nothing reaches the provider at start or without a key: the source is
//!   made on the first search, preview or download after a key is saved,
//!   and only those commands, each a user action, use it. Making a source
//!   and each of those operations take a [`UserAsked`], which only a
//!   command's invocation gives: a request at start through this state
//!   does not compile.
//! - What the types cannot stop, the source guard
//!   (`tests::nothing_in_the_app_forges_an_invocation` in `lib.rs`,
//!   D-2026-10-01-gif-sticker-search-10) refuses by identifier in the
//!   studio's production code, raw names and the tokens of macro calls
//!   included:
//!   - the Tauri APIs that dispatch an invocation the window never sent,
//!     run a script in it or load a page in it (`eval`, `with_webview`,
//!     `on_message`, `navigate`, ...), and literals that are `javascript:`
//!     URLs;
//!   - [`UserAsked::of`] anywhere but in the bodies of the GIF commands
//!     that take the window's request (`search_gifs`, `gif_preview`,
//!     `collect_gif` in `commands.rs`), or [`UserAsked`] under another name
//!     (`use … as`, `type … =`, `<UserAsked>::`, a macro's tokens, an
//!     `impl` outside `asked.rs`);
//!   - `expose_secret` but as `KlipyClient::new`'s argument in the source
//!     factory and as `serialize_str`'s in the key file's serializer
//!     (`write_key`, which serde calls and no code names): never bound to
//!     a variable, never inside a macro call; and any macro in a function
//!     that reads the key's text;
//!   - a second `KlipyClient::new`; a print or a log macro, as anywhere but
//!     `diag` (D-2026-10-01-gif-sticker-search-12), and any panic or
//!     assertion in this module.
//!
//!   Code written to get past it otherwise (generated code, another crate)
//!   is left to code review.
//! - Pages answered in this session are kept by query (kind, text, filter,
//!   language, page) and asked once. The window names a result by its id,
//!   never by an address, and only results of the pages of the last search
//!   can be previewed or collected (`gifNotInResults` otherwise).
//! - Previews reach the window as `data:` URLs, so the CSP stays as it is.
//! - No request runs under a lock: a result's files are read before the
//!   collection is locked to keep them.
//! - A collection whose folder cannot be used is never replaced by one kept
//!   in memory: every use of it is `collectionUnavailable`, naming the
//!   folder (the Collection panel shows it), adding to it is refused before
//!   anything is read, and the next use opens it again.
//! - Using a collected item copies it into the theme through today's paths:
//!   [`Backend::add_image_bytes`] for an image element (an animated GIF
//!   stays an image, with its transparency), [`Backend::add_media`] of a copy
//!   in `<cache>/collection` for a background (an animated GIF becomes a
//!   video background with its poster).

mod asked;
mod key;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bezel_core::app::gifs as use_cases;
use bezel_core::domain::archive::ContentId;
use bezel_core::domain::clock::Language;
use bezel_core::domain::error::ServiceFailure;
use bezel_core::domain::gifs::{
    CollectedGif, Explicit, GifItem, GifKind, GifPage, GifQuery, ITEM_LIMIT, Motion, PREVIEW_LIMIT,
    Rendition, RenditionFormat,
};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::theme::AssetRef;
use bezel_core::ports::{GifCollection, GifSource};
use bezel_core::{BezelError, Result};
use bezel_media::archive::content_id;
use image::ImageFormat;

pub use self::asked::UserAsked;
pub use self::key::{KEY_FILE, KeyFile, KlipyKey, SavedKey};
use crate::backend::Backend;
use crate::clock::unix_seconds;
use crate::dto::{AddedMediaDto, CollectedDto, CollectedUsersDto, GifPageDto, KeyDto};
use crate::messages::{ErrorCode, UiError, UiResult};

/// Makes the source for a saved key and its customer id, when the user
/// asked for something it serves. Making one asks nothing of the provider.
pub type SourceFactory =
    Arc<dyn Fn(&UserAsked, &KlipyKey, &str) -> Arc<dyn GifSource> + Send + Sync>;

/// The GIF provider: its sources, and the customer id each new key gets.
pub struct Provider {
    /// Makes the source for a key.
    pub source: SourceFactory,
    /// A new customer id, made once per saved key.
    pub customer_id: fn() -> Result<String>,
}

/// Opens the user's collection: `collectionUnavailable` while its folder
/// cannot be used.
pub type CollectionOpener = Box<dyn Fn() -> UiResult<Box<dyn GifCollection>> + Send + Sync>;

/// Opens, with `open` (the composition root's adapter), the collection kept
/// in `folder`; a folder that cannot be used is `collectionUnavailable`,
/// naming it.
pub fn collection_in<C: GifCollection + 'static>(
    folder: PathBuf,
    open: fn(&Path) -> Result<C>,
) -> CollectionOpener {
    Box::new(move || match open(&folder) {
        Ok(collection) => Ok(Box::new(collection)),
        Err(e) => Err(collection_unavailable(&folder, &e)),
    })
}

/// The collection in `folder` cannot be used, because of `error`.
fn collection_unavailable(folder: &Path, error: &BezelError) -> UiError {
    let reason = match error {
        BezelError::Transport(detail) => detail.clone(),
        other => other.to_string(),
    };
    UiError::new(ErrorCode::CollectionUnavailable)
        .arg("folder", folder.display())
        .arg("reason", reason)
}

/// State managed by Tauri for the GIF commands.
pub type SharedGifs = Arc<Gifs>;

/// The user's collection, and how to open it again while it is not open.
struct Shelf {
    open: CollectionOpener,
    opened: Option<Box<dyn GifCollection>>,
}

/// The GIF search and the collection.
pub struct Gifs {
    key: KeyFile,
    provider: Provider,
    /// The source for the saved key, made when first needed.
    source: Mutex<Option<Arc<dyn GifSource>>>,
    searches: Mutex<Searches>,
    collection: Mutex<Shelf>,
    /// Where a collected GIF is copied to be added as a background.
    scratch: PathBuf,
}

/// The source, without its key.
impl fmt::Debug for Gifs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gifs")
            .field("key", &self.key)
            .field("scratch", &self.scratch)
            .finish_non_exhaustive()
    }
}

/// What the user can do with a collected item in the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// An image element.
    Image,
    /// The theme's background.
    Background,
}

impl Target {
    /// The target named `name` (`image`, `background`).
    pub fn parse(name: &str) -> UiResult<Self> {
        match name {
            "image" => Ok(Self::Image),
            "background" => Ok(Self::Background),
            _ => Err(invalid(format!("use as \"{name}\""))),
        }
    }
}

/// The pages answered in this session and the results of the last search.
#[derive(Debug, Default)]
struct Searches {
    /// Every page answered, by its query.
    pages: HashMap<GifQuery, GifPage>,
    /// Searches asked so far: an answer is the last search's only when no
    /// other search was asked after it.
    asked: u64,
    /// The last search (its first page's query).
    last: Option<GifQuery>,
    /// The results of the last search's pages, by id.
    items: HashMap<String, GifItem>,
    /// Previews read for those results, by id and motion.
    previews: HashMap<(String, Motion), Vec<u8>>,
}

impl Searches {
    /// Takes `page`, the answer to `query`, as the last search's: a first
    /// page or another search starts it over, a further page adds to it.
    fn answered(&mut self, query: &GifQuery, page: &GifPage) {
        let search = GifQuery {
            page: 1,
            ..query.clone()
        };
        if query.page == 1 || self.last.as_ref() != Some(&search) {
            self.items.clear();
        }
        self.last = Some(search);
        for item in &page.items {
            self.items.insert(item.id.clone(), item.clone());
        }
        let items = &self.items;
        self.previews.retain(|(id, _), _| items.contains_key(id));
    }

    /// The result `id` of the last search.
    fn item(&self, id: &str) -> UiResult<GifItem> {
        self.items
            .get(id)
            .cloned()
            .ok_or_else(|| UiError::new(ErrorCode::GifNotInResults).arg("item", id))
    }

    /// The preview of `id` read with `motion`, when it was.
    fn preview(&self, id: &str, motion: Motion) -> Option<Vec<u8>> {
        self.previews.get(&(id.to_string(), motion)).cloned()
    }

    /// Keeps the preview `bytes` of `id` while it is a result of the last
    /// search.
    fn keep_preview(&mut self, id: &str, motion: Motion, bytes: Vec<u8>) {
        if self.items.contains_key(id) {
            self.previews.insert((id.to_string(), motion), bytes);
        }
    }
}

/// The query the window asked for: `kind` (`gif`, `sticker`), `text`
/// (empty: the trending ones), `page` from 1, `explicit` results shown or
/// not, in the app's `language`.
pub fn query(
    kind: &str,
    text: &str,
    page: u32,
    explicit: bool,
    language: Language,
) -> UiResult<GifQuery> {
    let kind = GifKind::from_slug(kind).ok_or_else(|| invalid(format!("GIF kind \"{kind}\"")))?;
    if page == 0 {
        return Err(invalid("page 0: pages start at 1"));
    }
    let explicit = if explicit {
        Explicit::Shown
    } else {
        Explicit::Hidden
    };
    Ok(GifQuery::new(kind, text, page, explicit, language))
}

impl Gifs {
    /// The search and the collection: the key in `key`, sources from
    /// `provider`, the GIFs kept in the collection `open` opens (now, and
    /// again at each use while it cannot be), a background's copy made in
    /// `scratch`. Nothing is read nor asked here.
    pub fn new(key: KeyFile, provider: Provider, open: CollectionOpener, scratch: PathBuf) -> Self {
        // A folder that cannot be used now is said by each use, which tries
        // it again.
        let opened = open().ok();
        Self {
            key,
            provider,
            source: Mutex::default(),
            searches: Mutex::default(),
            collection: Mutex::new(Shelf { open, opened }),
            scratch,
        }
    }

    fn searches(&self) -> MutexGuard<'_, Searches> {
        self.searches.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `act` on the collection, under its lock, opening it first when
    /// it is not open: `collectionUnavailable` while its folder cannot be
    /// used.
    fn with_collection<T>(
        &self,
        act: impl FnOnce(&mut dyn GifCollection) -> UiResult<T>,
    ) -> UiResult<T> {
        let mut shelf = self
            .collection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut collection = match shelf.opened.take() {
            Some(collection) => collection,
            None => (shelf.open)()?,
        };
        let done = act(collection.as_mut());
        shelf.opened = Some(collection);
        done
    }

    // ---------------------------------------------------------------- key --

    /// Whether a key is saved.
    pub fn key_status(&self) -> UiResult<KeyDto> {
        Ok(key_dto(self.key.load()?.as_ref()))
    }

    /// Saves `key` (with a new customer id, unless it is the key already
    /// saved); nothing is asked of the provider. A refused key shows at the
    /// first search.
    pub fn save_key(&self, key: KlipyKey) -> UiResult<KeyDto> {
        let kept = self.key.load().ok().flatten();
        let customer_id = match kept {
            Some(saved) if *saved.key() == key => saved.customer_id().to_string(),
            _ => (self.provider.customer_id)()?,
        };
        let saved = SavedKey::new(key, &customer_id);
        self.key.save(&saved)?;
        self.start_over();
        Ok(key_dto(Some(&saved)))
    }

    /// Deletes the saved key: searching needs a key again.
    pub fn remove_key(&self) -> UiResult<KeyDto> {
        self.key.remove()?;
        self.start_over();
        Ok(key_dto(None))
    }

    /// Drops the source and what this session was answered, for a new key
    /// or none.
    fn start_over(&self) {
        *self.source.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self.searches() = Searches::default();
    }

    /// The source for the saved key, for what the user `asked`:
    /// `klipyNoKey` without one.
    fn source(&self, asked: &UserAsked) -> UiResult<Arc<dyn GifSource>> {
        let mut source = self.source.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(source) = source.as_ref() {
            return Ok(Arc::clone(source));
        }
        let saved = self
            .key
            .load()?
            .ok_or_else(|| UiError::new(ErrorCode::KlipyNoKey))?;
        let made = (self.provider.source)(asked, saved.key(), saved.customer_id());
        *source = Some(Arc::clone(&made));
        Ok(made)
    }

    // ------------------------------------------------------------- search --

    /// The page `query` asks for, as the user `asked`: from this session's
    /// answers, else asked of the provider. Its results become the last
    /// search's.
    pub fn search(&self, asked: &UserAsked, query: &GifQuery) -> UiResult<GifPageDto> {
        let source = self.source(asked)?;
        let (known, ticket) = {
            let mut searches = self.searches();
            searches.asked += 1;
            (searches.pages.get(query).cloned(), searches.asked)
        };
        let page = match known {
            Some(page) => page,
            None => use_cases::search(source.as_ref(), query)?,
        };
        let mut searches = self.searches();
        searches.pages.insert(query.clone(), page.clone());
        // A slow answer to a search the user already replaced is kept, but
        // its results are not the last search's.
        if searches.asked == ticket {
            searches.answered(query, &page);
        }
        Ok(GifPageDto::of(query, &page))
    }

    /// The preview of the result `id` of the last search, as the user
    /// `asked`, as a `data:` URL of its bytes' media type: its small GIF,
    /// or when `still` its still (a GIF's JPEG, a sticker's PNG); `None`
    /// when it has no such file.
    pub fn preview(&self, asked: &UserAsked, id: &str, still: bool) -> UiResult<Option<String>> {
        let source = self.source(asked)?;
        let motion = if still {
            Motion::Still
        } else {
            Motion::Animated
        };
        let (item, known) = {
            let searches = self.searches();
            (searches.item(id)?, searches.preview(id, motion))
        };
        let bytes = match known {
            Some(bytes) => Some(bytes),
            None => use_cases::preview(source.as_ref(), &item, motion)?,
        };
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let url = file_data_url(&bytes);
        self.searches().keep_preview(id, motion, bytes);
        Ok(url)
    }

    // --------------------------------------------------------- collection --

    /// Adds the result `id` of the last search to the collection, as the
    /// user `asked`: refused before anything is read when the collection
    /// cannot be used; its files are read first, then kept (once per
    /// content) under the collection's lock.
    pub fn collect(&self, asked: &UserAsked, id: &str) -> UiResult<CollectedDto> {
        self.with_collection(|_| Ok(()))?;
        let source = self.source(asked)?;
        let (item, preview) = {
            let searches = self.searches();
            (searches.item(id)?, searches.preview(id, Motion::Animated))
        };
        let fetched = Fetched::read(source.as_ref(), &item, preview.is_none())?;
        let collected = self.with_collection(|collection| {
            let added = use_cases::add_to_collection(
                &fetched,
                collection,
                &item,
                preview.as_deref(),
                unix_seconds(),
            );
            Ok(added?)
        })?;
        self.dto(&collected, Motion::Animated)
    }

    /// The collection, the last added first; previews still when `still`.
    pub fn list(&self, still: bool) -> UiResult<Vec<CollectedDto>> {
        let motion = if still {
            Motion::Still
        } else {
            Motion::Animated
        };
        self.with_collection(|collection| {
            let index = collection.load()?;
            index
                .items()
                .iter()
                .rev()
                .map(|item| {
                    let preview = collection.read_preview(&item.content)?;
                    Ok(CollectedDto::of(item, preview_data_url(preview, motion)))
                })
                .collect()
        })
    }

    /// Renames the item `id` to `name` (trimmed, not empty).
    pub fn rename(&self, id: &str, name: &str) -> UiResult<CollectedDto> {
        let content = content_of(id)?;
        let renamed =
            self.with_collection(|collection| Ok(use_cases::rename(collection, &content, name)?))?;
        let renamed = renamed.ok_or_else(|| not_in_collection(id))?;
        self.dto(&renamed, Motion::Animated)
    }

    /// Deletes the item `id`, its GIF and its preview: only `Confirm::Yes`
    /// (the dialog that named the themes using it) does. Themes keep their
    /// own copy.
    pub fn delete(&self, id: &str, confirm: Confirm) -> UiResult<()> {
        let content = content_of(id)?;
        self.with_collection(|collection| {
            use_cases::delete(collection, &content, confirm)?
                .map(drop)
                .ok_or_else(|| not_in_collection(id))
        })
    }

    /// The user's themes, and whether the open one, that hold the bytes of
    /// the item `id`: the delete dialog names them.
    pub fn users(&self, backend: &Backend, id: &str) -> UiResult<CollectedUsersDto> {
        let item = self.collected(id)?;
        let holds = |assets: &BTreeMap<AssetRef, Vec<u8>>| {
            assets
                .values()
                .any(|bytes| same_content(bytes, &item.content, item.bytes))
        };
        let themes = backend
            .library
            .list()
            .into_iter()
            .filter(|entry| !entry.bundled)
            .filter(|entry| {
                backend
                    .store
                    .load(&entry.location)
                    .is_ok_and(|(_, assets)| holds(&assets))
            })
            .map(|entry| entry.theme.name)
            .collect();
        // Hashed outside the session's lock: only files of the same size.
        let candidates: Vec<Vec<u8>> = {
            let studio = backend.studio();
            let assets = studio.assets().values();
            assets
                .filter(|bytes| bytes.len() as u64 == item.bytes)
                .cloned()
                .collect()
        };
        let open_theme = candidates
            .iter()
            .any(|bytes| same_content(bytes, &item.content, item.bytes));
        Ok(CollectedUsersDto { themes, open_theme })
    }

    /// Copies the item `id` into the theme as `target` through today's
    /// paths: an image element takes its bytes as they are; a background is
    /// added from a copy named after it, an animated GIF as a video with its
    /// poster.
    pub fn use_in_theme(
        &self,
        backend: &Backend,
        id: &str,
        target: Target,
    ) -> UiResult<AddedMediaDto> {
        let (item, bytes) = self.item(id)?;
        let file_name = format!("{}.gif", file_stem(&item.name));
        match target {
            Target::Image => {
                let size = bytes.len() as u64;
                let added = backend.add_image_bytes(Path::new(&file_name), bytes)?;
                Ok(AddedMediaDto {
                    reference: added.reference,
                    kind: "image",
                    poster: None,
                    bytes: size,
                    duration_ms: None,
                    poster_error: None,
                })
            }
            Target::Background => {
                let copy = self.scratch.join(&file_name);
                let written = std::fs::create_dir_all(&self.scratch)
                    .and_then(|()| std::fs::write(&copy, &bytes));
                written.map_err(|e| UiError::file(copy.display(), e))?;
                let added = backend.add_media(&copy);
                // Best effort: the theme holds its own copy now.
                let _ = std::fs::remove_file(&copy);
                added
            }
        }
    }

    /// The collected item `id`.
    fn collected(&self, id: &str) -> UiResult<CollectedGif> {
        let content = content_of(id)?;
        let index = self.with_collection(|collection| Ok(collection.load()?))?;
        index
            .get(&content)
            .cloned()
            .ok_or_else(|| not_in_collection(id))
    }

    /// The collected item `id` and its bytes.
    fn item(&self, id: &str) -> UiResult<(CollectedGif, Vec<u8>)> {
        let item = self.collected(id)?;
        let bytes = self
            .with_collection(|collection| Ok(collection.read(&item.content)?))?
            .ok_or_else(|| not_in_collection(id))?;
        Ok((item, bytes))
    }

    /// `item` as the window gets it, with its kept preview.
    fn dto(&self, item: &CollectedGif, motion: Motion) -> UiResult<CollectedDto> {
        let preview =
            self.with_collection(|collection| Ok(collection.read_preview(&item.content)?))?;
        Ok(CollectedDto::of(item, preview_data_url(preview, motion)))
    }
}

/// The window's view of `saved`.
fn key_dto(saved: Option<&SavedKey>) -> KeyDto {
    KeyDto {
        configured: saved.is_some(),
        last4: saved.and_then(SavedKey::last4),
    }
}

/// The files of one result read ahead: the source that adding it to the
/// collection reads from under the collection's lock, which asks nothing.
struct Fetched {
    provider: String,
    files: Vec<(String, Vec<u8>)>,
}

impl Fetched {
    /// The GIF `item` is collected from and, `with_preview`, its small GIF
    /// (a preview that cannot be read is left out, as the core does).
    fn read(source: &dyn GifSource, item: &GifItem, with_preview: bool) -> Result<Self> {
        let mut files = Vec::new();
        if let Some(gif) = item.download() {
            files.push((gif.location.clone(), source.download(gif, ITEM_LIMIT)?));
        }
        let preview = item
            .preview(Motion::Animated)
            .filter(|small| with_preview && !files.iter().any(|(at, _)| *at == small.location));
        if let Some(small) = preview
            && let Ok(bytes) = source.download(small, PREVIEW_LIMIT)
        {
            files.push((small.location.clone(), bytes));
        }
        Ok(Self {
            provider: source.provider().to_string(),
            files,
        })
    }
}

impl GifSource for Fetched {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn page(&self, _query: &GifQuery) -> Result<GifPage> {
        Err(not_fetched())
    }

    fn download(&self, rendition: &Rendition, _limit: u64) -> Result<Vec<u8>> {
        self.files
            .iter()
            .find(|(at, _)| *at == rendition.location)
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(not_fetched)
    }
}

fn not_fetched() -> BezelError {
    BezelError::Service(ServiceFailure::Unavailable(
        "a file that was not read ahead".to_string(),
    ))
}

/// The id of a collected item as the window names it (its SHA-256).
fn content_of(id: &str) -> UiResult<ContentId> {
    ContentId::parse(id).ok_or_else(|| not_in_collection(id))
}

fn not_in_collection(id: &str) -> UiError {
    UiError::new(ErrorCode::NotInCollection).arg("item", id)
}

fn invalid(detail: impl fmt::Display) -> UiError {
    UiError::new(ErrorCode::InvalidInput).arg("detail", detail)
}

/// Whether `bytes` are the GIF kept as `content`, of `size` bytes.
fn same_content(bytes: &[u8], content: &ContentId, size: u64) -> bool {
    bytes.len() as u64 == size && content_id(bytes) == *content
}

/// A file name's stem for a collected item named `name`: its letters and
/// digits, the rest as single dashes (never a path), `gif` when nothing is
/// left.
fn file_stem(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let stem: Vec<&str> = mapped.split('-').filter(|p| !p.is_empty()).collect();
    let stem: String = stem.join("-").chars().take(64).collect();
    if stem.is_empty() { "gif".into() } else { stem }
}

/// A preview file (a GIF, a JPEG or a PNG still) as a `data:` URL of the
/// media type its bytes are in; `None` for anything else.
fn file_data_url(bytes: &[u8]) -> Option<String> {
    let mime = RenditionFormat::of(bytes).map(RenditionFormat::mime)?;
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

/// A kept preview (a small GIF) as a `data:` URL: as it is, or its first
/// picture as a PNG for [`Motion::Still`].
fn preview_data_url(preview: Option<Vec<u8>>, motion: Motion) -> Option<String> {
    let preview = preview?;
    match motion {
        Motion::Animated => file_data_url(&preview),
        Motion::Still => {
            let picture = image::load_from_memory_with_format(&preview, ImageFormat::Gif).ok()?;
            let mut png = Cursor::new(Vec::new());
            picture.write_to(&mut png, ImageFormat::Png).ok()?;
            Some(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            ))
        }
    }
}
