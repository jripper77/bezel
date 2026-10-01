//! GIFs and stickers found through an online provider, and the user's
//! collection of them (D-2026-10-01-gif-sticker-search-2, -4, -5).
//!
//! A [`GifSource`](crate::ports::GifSource) answers a [`GifQuery`] with a
//! [`GifPage`] of [`GifItem`]s, each offered as several [`Rendition`]s (the
//! same picture in other sizes and formats). Adding an item to the
//! collection keeps its largest GIF rendition a rev C screen can take as it
//! is ([`download_rendition`]), once per content ([`ContentId`]), recorded
//! as a [`CollectedGif`] in the [`Collection`] index that a
//! [`GifCollection`](crate::ports::GifCollection) keeps. Everything here is
//! pure: the bytes come through the ports, the time from the caller.

use super::archive::ContentId;
use super::clock::Language;
use super::storage::{MIB, REV_C_MAX_UPLOAD_BYTES};

/// Results asked for per page.
pub const PAGE_SIZE: u32 = 24;

/// Most bytes read for one preview: 2 MiB.
pub const PREVIEW_LIMIT: u64 = 2 * MIB;

/// Most bytes of a collected GIF: the rev C upload cap, 25 MiB
/// (D-2026-09-30-release-polish-12), so the file can also be sent to a
/// screen as it is.
pub const ITEM_LIMIT: u64 = REV_C_MAX_UPLOAD_BYTES;

/// What is searched and collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum GifKind {
    /// An animated GIF.
    #[default]
    Gif,
    /// A sticker: a GIF with a transparent background.
    Sticker,
}

impl GifKind {
    /// Every kind.
    pub const ALL: [GifKind; 2] = [GifKind::Gif, GifKind::Sticker];

    /// Stable machine name (`gif`, `sticker`).
    pub const fn slug(self) -> &'static str {
        match self {
            GifKind::Gif => "gif",
            GifKind::Sticker => "sticker",
        }
    }

    /// The kind named by `slug`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.slug() == slug)
    }
}

/// Whether explicit results may show: the "Show explicit results" switch,
/// hidden whenever the app starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Explicit {
    /// Explicit results are filtered out (the default).
    #[default]
    Hidden,
    /// The user asked for explicit results.
    Shown,
}

impl Explicit {
    /// The provider's content filter for this choice.
    pub const fn filter(self) -> ContentFilter {
        match self {
            Explicit::Hidden => ContentFilter::Medium,
            Explicit::Shown => ContentFilter::Off,
        }
    }
}

/// The content rating filter a provider applies to its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentFilter {
    /// General and parental-guidance audiences (G and PG): safe on a desk
    /// screen without hiding everyday reaction GIFs, which a stricter
    /// filter drops.
    Medium,
    /// No filter: explicit results included.
    Off,
}

impl From<Explicit> for ContentFilter {
    fn from(explicit: Explicit) -> Self {
        explicit.filter()
    }
}

/// One page of a search: GIFs or stickers for a text, the trending ones
/// when the text is empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GifQuery {
    /// GIFs or stickers.
    pub kind: GifKind,
    /// What is searched, trimmed; empty for the trending items.
    pub text: String,
    /// The page, from 1.
    pub page: u32,
    /// Whether explicit results may show.
    pub explicit: Explicit,
    /// The user's language, for the provider's locale.
    pub language: Language,
}

impl GifQuery {
    /// The query for `text` (trimmed) at `page` (0 is taken as 1).
    pub fn new(
        kind: GifKind,
        text: &str,
        page: u32,
        explicit: Explicit,
        language: Language,
    ) -> Self {
        Self {
            kind,
            text: text.trim().to_string(),
            page: page.max(1),
            explicit,
            language,
        }
    }

    /// Whether this asks for the trending items (no text).
    pub fn trending(&self) -> bool {
        self.text.is_empty()
    }

    /// The content filter the provider applies.
    pub fn filter(&self) -> ContentFilter {
        self.explicit.filter()
    }

    /// The same search, one page further ("Load more").
    pub fn next(&self) -> Self {
        Self {
            page: self.page.saturating_add(1),
            ..self.clone()
        }
    }
}

/// The file format of a rendition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenditionFormat {
    /// A GIF (animated or not).
    Gif,
    /// A JPEG still of the first picture.
    Jpeg,
}

impl RenditionFormat {
    /// The format `bytes` are in, from their signature; `None` for anything
    /// else.
    pub fn of(bytes: &[u8]) -> Option<Self> {
        if is_gif(bytes) {
            Some(RenditionFormat::Gif)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(RenditionFormat::Jpeg)
        } else {
            None
        }
    }
}

/// How large a rendition is among the ones a provider offers for an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// High definition.
    Large,
    /// Medium.
    Medium,
    /// Small: the picture of a result tile.
    Small,
    /// Extra small.
    Tiny,
}

/// The same picture in one size and format.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rendition {
    /// Its size among the item's renditions.
    pub tier: Tier,
    /// Its file format.
    pub format: RenditionFormat,
    /// Where the source reads it (for an online provider, the file's
    /// address). Opaque to the core.
    pub location: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Its size in bytes, when the provider tells.
    pub bytes: Option<u64>,
}

impl Rendition {
    /// Its area in pixels.
    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Whether it is in `format` and not known to be over `limit` bytes.
    fn fits(&self, format: RenditionFormat, limit: u64) -> bool {
        self.format == format && self.bytes.is_none_or(|bytes| bytes <= limit)
    }
}

/// The rendition an item is collected from: its largest GIF (by area) of at
/// most [`ITEM_LIMIT`] bytes. A rendition whose size is not told counts as
/// fitting (the download itself stops at the limit); `None` when the item
/// has no such GIF.
pub fn download_rendition(renditions: &[Rendition]) -> Option<&Rendition> {
    renditions
        .iter()
        .filter(|r| r.fits(RenditionFormat::Gif, ITEM_LIMIT))
        .max_by_key(|r| r.area())
}

/// Whether a preview moves or stands still.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Motion {
    /// The small GIF.
    #[default]
    Animated,
    /// The JPEG still, when motion is reduced.
    Still,
}

impl Motion {
    /// The format of a preview with this motion.
    pub const fn format(self) -> RenditionFormat {
        match self {
            Motion::Animated => RenditionFormat::Gif,
            Motion::Still => RenditionFormat::Jpeg,
        }
    }
}

/// The rendition an item's preview is read from: the small one in the
/// format of `motion`, else the smallest one in that format; `None` when
/// none is in that format with at most [`PREVIEW_LIMIT`] bytes.
pub fn preview_rendition(renditions: &[Rendition], motion: Motion) -> Option<&Rendition> {
    let fitting = || {
        renditions
            .iter()
            .filter(move |r| r.fits(motion.format(), PREVIEW_LIMIT))
    };
    fitting()
        .find(|r| r.tier == Tier::Small)
        .or_else(|| fitting().min_by_key(|r| r.area()))
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GifItem {
    /// The provider's id of the item.
    pub id: String,
    /// Its title (may be empty).
    pub title: String,
    /// GIF or sticker.
    pub kind: GifKind,
    /// The provider's page about the item, when it has one.
    pub page_url: Option<String>,
    /// The files the item is offered as.
    pub renditions: Vec<Rendition>,
}

impl GifItem {
    /// The rendition it is collected from ([`download_rendition`]).
    pub fn download(&self) -> Option<&Rendition> {
        download_rendition(&self.renditions)
    }

    /// The rendition its preview is read from ([`preview_rendition`]).
    pub fn preview(&self, motion: Motion) -> Option<&Rendition> {
        preview_rendition(&self.renditions, motion)
    }

    /// The name it is collected under: its title, else its id.
    pub fn name(&self) -> String {
        item_name(&self.title).unwrap_or_else(|| self.id.clone())
    }
}

/// A page of results.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GifPage {
    /// The items, in the provider's order.
    pub items: Vec<GifItem>,
    /// Whether another page follows.
    pub has_next: bool,
}

/// Whether `bytes` are a GIF: the GIF87a or GIF89a signature and a whole
/// logical screen descriptor.
pub fn is_gif(bytes: &[u8]) -> bool {
    gif_size(bytes).is_some()
}

/// The width and height a GIF declares (its logical screen); `None` when
/// `bytes` are not a GIF.
pub fn gif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let signed = bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a");
    let screen = bytes.get(6..10).filter(|_| signed && bytes.len() >= 13)?;
    let width = u16::from_le_bytes([screen[0], screen[1]]);
    let height = u16::from_le_bytes([screen[2], screen[3]]);
    Some((u32::from(width), u32::from(height)))
}

/// A collected item's name from what the user typed: trimmed; `None` when
/// nothing is left.
pub fn item_name(text: &str) -> Option<String> {
    let name = text.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Where a collected item came from, kept so the choice of provider stays
/// reversible.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GifOrigin {
    /// The provider's name ([`GifSource::provider`](crate::ports::GifSource::provider)).
    pub provider: String,
    /// The provider's id of the item.
    pub id: String,
    /// The provider's page about the item, when it has one.
    pub page_url: Option<String>,
}

/// One item of the user's collection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CollectedGif {
    /// Its bytes' SHA-256: the name of the kept file and of its preview.
    pub content: ContentId,
    /// Its name (the title, editable; never empty).
    pub name: String,
    /// GIF or sticker.
    pub kind: GifKind,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Its size in bytes.
    pub bytes: u64,
    /// When it was added, in seconds since the Unix epoch (the caller reads
    /// the clock).
    pub added_at: u64,
    /// Where it came from.
    pub origin: GifOrigin,
}

/// The index of the collection: one item per content, in the order they
/// were added.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Collection {
    items: Vec<CollectedGif>,
}

impl Collection {
    /// An index of `items` (as read back from storage); an item whose
    /// content an earlier one already has is dropped.
    pub fn new(items: Vec<CollectedGif>) -> Self {
        let mut index = Self::default();
        for item in items {
            index.add(item);
        }
        index
    }

    /// The items, in the order they were added.
    pub fn items(&self) -> &[CollectedGif] {
        &self.items
    }

    /// How many items it holds.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether it holds no item.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The bytes of every item.
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }

    /// The item kept as `content`.
    pub fn get(&self, content: &ContentId) -> Option<&CollectedGif> {
        self.items.iter().find(|i| &i.content == content)
    }

    /// Adds `item` at the end, unless an item of the same content is
    /// already there; whether it was added.
    pub fn add(&mut self, item: CollectedGif) -> bool {
        if self.get(&item.content).is_some() {
            return false;
        }
        self.items.push(item);
        true
    }

    /// Renames the item kept as `content` to `name` ([`item_name`]): the
    /// renamed item, `None` when there is no such item, `InvalidInput` for
    /// an empty name.
    pub fn rename(
        &mut self,
        content: &ContentId,
        name: &str,
    ) -> crate::Result<Option<&CollectedGif>> {
        let name = item_name(name)
            .ok_or_else(|| crate::BezelError::InvalidInput("the name is empty".to_string()))?;
        let Some(item) = self.items.iter_mut().find(|i| &i.content == content) else {
            return Ok(None);
        };
        item.name = name;
        Ok(Some(item))
    }

    /// Removes the item kept as `content` and returns it.
    pub fn remove(&mut self, content: &ContentId) -> Option<CollectedGif> {
        let at = self.items.iter().position(|i| &i.content == content)?;
        Some(self.items.remove(at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-pixel GIF89a.
    const PIXEL: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
    ];

    fn rendition(tier: Tier, format: RenditionFormat, side: u32, bytes: Option<u64>) -> Rendition {
        let ext = match format {
            RenditionFormat::Gif => "gif",
            RenditionFormat::Jpeg => "jpg",
        };
        Rendition {
            tier,
            format,
            location: format!("files/{tier:?}.{ext}"),
            width: side,
            height: side,
            bytes,
        }
    }

    fn content(n: u8) -> ContentId {
        ContentId::from_digest([n; 32])
    }

    fn collected(n: u8, name: &str) -> CollectedGif {
        CollectedGif {
            content: content(n),
            name: name.to_string(),
            kind: GifKind::Sticker,
            width: 200,
            height: 100,
            bytes: u64::from(n) * 10,
            added_at: 1_790_000_000,
            origin: GifOrigin {
                provider: "fake".into(),
                id: format!("id-{n}"),
                page_url: None,
            },
        }
    }

    #[test]
    fn explicit_maps_to_the_filter() {
        assert_eq!(Explicit::default(), Explicit::Hidden, "hidden at start");
        assert_eq!(Explicit::Hidden.filter(), ContentFilter::Medium);
        assert_eq!(Explicit::Shown.filter(), ContentFilter::Off);
        assert_eq!(ContentFilter::from(Explicit::Shown), ContentFilter::Off);
        assert_eq!(ContentFilter::from(Explicit::Hidden), ContentFilter::Medium);
        let query = |explicit| GifQuery::new(GifKind::Gif, "cat", 1, explicit, Language::English);
        assert_eq!(query(Explicit::Hidden).filter(), ContentFilter::Medium);
        assert_eq!(query(Explicit::Shown).filter(), ContentFilter::Off);
    }

    #[test]
    fn picks_the_largest_gif_under_25_mib() {
        use RenditionFormat::{Gif, Jpeg};
        let renditions = [
            rendition(Tier::Tiny, Gif, 90, Some(100_000)),
            rendition(Tier::Large, Jpeg, 1000, Some(300_000)),
            rendition(Tier::Large, Gif, 1000, Some(30 * MIB)),
            rendition(Tier::Medium, Gif, 500, Some(25 * MIB)),
            rendition(Tier::Small, Gif, 200, Some(2 * MIB)),
        ];
        let picked = download_rendition(&renditions).expect("a GIF fits");
        assert_eq!(
            (picked.tier, picked.format, picked.bytes),
            (Tier::Medium, Gif, Some(25 * MIB)),
            "the largest GIF of at most 25 MiB, not the larger one over it nor the JPEG"
        );
        assert_eq!(ITEM_LIMIT, 26_214_400);

        // Any order; a size the provider does not tell still counts.
        let mut reordered = renditions.to_vec();
        reordered.reverse();
        reordered.push(rendition(Tier::Large, Gif, 800, None));
        assert_eq!(download_rendition(&reordered).map(|r| r.width), Some(800));

        // No GIF at most 25 MiB: nothing to collect.
        let none = [
            rendition(Tier::Large, Gif, 1000, Some(25 * MIB + 1)),
            rendition(Tier::Small, Jpeg, 200, Some(10_000)),
        ];
        assert_eq!(download_rendition(&none), None);
    }

    #[test]
    fn previews_are_the_small_rendition_of_their_format() {
        use RenditionFormat::{Gif, Jpeg};
        let renditions = vec![
            rendition(Tier::Large, Gif, 1000, Some(9 * MIB)),
            rendition(Tier::Small, Gif, 200, Some(400_000)),
            rendition(Tier::Tiny, Gif, 90, Some(90_000)),
            rendition(Tier::Small, Jpeg, 200, Some(20_000)),
        ];
        let item = GifItem {
            id: "42".into(),
            title: "  ".into(),
            kind: GifKind::Gif,
            page_url: None,
            renditions,
        };
        let pick = |motion| item.preview(motion).map(|r| (r.tier, r.format));
        assert_eq!(pick(Motion::Animated), Some((Tier::Small, Gif)));
        assert_eq!(pick(Motion::Still), Some((Tier::Small, Jpeg)));
        assert_eq!(item.download().map(|r| r.tier), Some(Tier::Large));
        assert_eq!(item.name(), "42", "a blank title names it by id");

        // No small one under 2 MiB: the smallest one that fits.
        let large_small = [
            rendition(Tier::Small, Gif, 200, Some(3 * MIB)),
            rendition(Tier::Medium, Gif, 400, Some(MIB)),
            rendition(Tier::Tiny, Gif, 90, None),
        ];
        let tiny = preview_rendition(&large_small, Motion::Animated);
        assert_eq!(tiny.map(|r| r.tier), Some(Tier::Tiny));
        assert_eq!(preview_rendition(&large_small, Motion::Still), None);
    }

    #[test]
    fn queries_are_trimmed_and_paged_from_one() {
        let query = GifQuery::new(
            GifKind::Sticker,
            "  party  ",
            0,
            Explicit::Shown,
            Language::PortugueseBr,
        );
        assert_eq!((query.text.as_str(), query.page), ("party", 1));
        assert!(!query.trending());
        assert_eq!(query.next().page, 2);
        assert_eq!(query.next().text, "party");
        let trending = GifQuery::new(GifKind::Gif, "   ", 3, Explicit::Hidden, Language::English);
        assert!(trending.trending());
        assert_eq!(GifKind::from_slug("sticker"), Some(GifKind::Sticker));
        assert_eq!(GifKind::from_slug("gif").map(GifKind::slug), Some("gif"));
        assert_eq!(GifKind::from_slug("webp"), None);
    }

    #[test]
    fn gifs_are_told_by_their_signature() {
        assert!(is_gif(PIXEL));
        assert_eq!(gif_size(PIXEL), Some((1, 1)));
        let mut wide = PIXEL.to_vec();
        wide[3..6].copy_from_slice(b"87a");
        wide[6..10].copy_from_slice(&[0x40, 0x01, 0xF0, 0x00]);
        assert_eq!(gif_size(&wide), Some((320, 240)));
        assert!(!is_gif(&PIXEL[..12]), "cut before the screen descriptor");
        assert!(!is_gif(b"\x89PNG\r\n\x1a\n0000000000"));
        assert!(!is_gif(b"GIF90a0000000000"));
        assert_eq!(RenditionFormat::of(PIXEL), Some(RenditionFormat::Gif));
        assert_eq!(
            RenditionFormat::of(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(RenditionFormat::Jpeg)
        );
        assert_eq!(RenditionFormat::of(b"<html>"), None);
    }

    #[test]
    fn the_index_holds_one_item_per_content() {
        let mut index = Collection::new(vec![
            collected(1, "one"),
            collected(2, "two"),
            collected(1, "again"),
        ]);
        assert_eq!(index.len(), 2);
        assert_eq!(index.get(&content(1)).map(|i| i.name.as_str()), Some("one"));
        assert_eq!(index.total_bytes(), 30);
        assert!(!index.add(collected(2, "dup")));
        assert!(index.add(collected(3, "three")));
        let names: Vec<&str> = index.items().iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["one", "two", "three"], "in the order added");

        let renamed = index.rename(&content(2), "  Party  ").expect("named");
        assert_eq!(renamed.map(|i| i.name.as_str()), Some("Party"));
        assert_eq!(index.rename(&content(9), "x"), Ok(None));
        assert!(matches!(
            index.rename(&content(2), " \t "),
            Err(crate::BezelError::InvalidInput(_))
        ));
        assert_eq!(
            index.get(&content(2)).map(|i| i.name.as_str()),
            Some("Party")
        );

        assert_eq!(
            index.remove(&content(1)).map(|i| i.name),
            Some("one".into())
        );
        assert_eq!(index.remove(&content(1)), None);
        assert!(!index.is_empty());
        assert!(Collection::default().is_empty());
    }
}
