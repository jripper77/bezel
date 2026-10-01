//! The GIF and sticker use cases through the adapters' fakes: a scripted
//! provider (`bezel_media::collection::FakeGifSource`) and the collection
//! in memory (`bezel_media::collection::MemoryCollection`).
#![allow(clippy::expect_used)] // helpers of a failing test panic

use bezel_core::BezelError;
use bezel_core::app::gifs;
use bezel_core::domain::archive::ContentId;
use bezel_core::domain::clock::Language;
use bezel_core::domain::error::ServiceFailure;
use bezel_core::domain::gifs::{
    Explicit, GifItem, GifKind, GifPage, GifQuery, ITEM_LIMIT, Motion, PREVIEW_LIMIT, Rendition,
    RenditionFormat, Tier,
};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::MIB;
use bezel_core::ports::GifSource;
use bezel_media::collection::{CollectionCall, FakeGifSource, GifCall, MemoryCollection};

/// The time the use cases record, seconds since the Unix epoch.
const NOW: u64 = 1_790_000_000;

/// A GIF89a of `width` x `height` whose bytes differ from another `seed`'s.
fn gif(seed: u8, width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend([0x80, 0x00, 0x00, seed, seed, seed, 0x00, 0x00, 0x00]);
    bytes.extend([
        0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
    ]);
    bytes
}

fn rendition(id: &str, tier: Tier, format: RenditionFormat, side: u32, bytes: u64) -> Rendition {
    let ext = match format {
        RenditionFormat::Gif => "gif",
        RenditionFormat::Jpeg => "jpg",
    };
    Rendition {
        tier,
        format,
        location: format!("files/{id}/{tier:?}.{ext}"),
        width: side,
        height: side,
        bytes: Some(bytes),
    }
}

/// A result offered as a 30 MiB GIF (too large), a 4 MiB GIF (the one
/// collected), a small GIF and a small JPEG.
fn item(id: &str, title: &str, kind: GifKind) -> GifItem {
    use RenditionFormat::{Gif, Jpeg};
    GifItem {
        id: id.into(),
        title: title.into(),
        kind,
        page_url: Some(format!("https://gifs.example/{id}")),
        renditions: vec![
            rendition(id, Tier::Large, Gif, 1000, 30 * MIB),
            rendition(id, Tier::Medium, Gif, 498, 4 * MIB),
            rendition(id, Tier::Small, Gif, 200, 300_000),
            rendition(id, Tier::Small, Jpeg, 200, 20_000),
        ],
    }
}

fn location(item: &GifItem, tier: Tier, format: RenditionFormat) -> String {
    let found = item
        .renditions
        .iter()
        .find(|r| r.tier == tier && r.format == format);
    found.expect("rendition").location.clone()
}

/// A source serving `item`'s collected GIF as `bytes` and its small GIF.
fn serving(source: FakeGifSource, item: &GifItem, bytes: Vec<u8>) -> FakeGifSource {
    let small = gif(0xEE, 200, 200);
    source
        .with_file(&location(item, Tier::Medium, RenditionFormat::Gif), bytes)
        .with_file(&location(item, Tier::Small, RenditionFormat::Gif), small)
}

fn names(collection: &MemoryCollection) -> Vec<String> {
    let index = collection.saved().unwrap_or_default();
    index.items().iter().map(|i| i.name.clone()).collect()
}

#[test]
fn keeps_one_copy_per_content() {
    let party = item("101", "Party parrot", GifKind::Gif);
    let again = item("202", "Parrot (repost)", GifKind::Gif);
    let other = item("303", "Wave", GifKind::Sticker);
    let source = FakeGifSource::new();
    let source = serving(source, &party, gif(1, 498, 280));
    let source = serving(source, &again, gif(1, 498, 280));
    let source = serving(source, &other, gif(2, 320, 320));
    let mut collection = MemoryCollection::new();
    let store = collection.clone();

    let first =
        gifs::add_to_collection(&source, &mut collection, &party, None, NOW).expect("added");
    assert_eq!(
        (first.name.as_str(), first.kind, first.width, first.height),
        ("Party parrot", GifKind::Gif, 498, 280)
    );
    assert_eq!(first.bytes, gif(1, 498, 280).len() as u64);
    assert_eq!(first.added_at, NOW);
    assert_eq!(
        (first.origin.provider.as_str(), first.origin.id.as_str()),
        ("fake", "101")
    );
    assert_eq!(
        first.origin.page_url.as_deref(),
        Some("https://gifs.example/101")
    );
    assert_eq!(
        source.downloads()[0],
        (
            location(&party, Tier::Medium, RenditionFormat::Gif),
            ITEM_LIMIT
        ),
        "the largest GIF of at most 25 MiB, read with that limit"
    );
    assert_eq!(store.preview_of(&first.content), Some(gif(0xEE, 200, 200)));

    // The same bytes from another result: the item already there, one copy.
    let saves = store.saves();
    let repeat = gifs::add_to_collection(&source, &mut collection, &again, None, NOW + 60)
        .expect("same content");
    assert_eq!(repeat, first, "the item already collected, unchanged");
    assert_eq!(store.files(), 1);
    assert_eq!(store.saves(), saves, "nothing saved again");
    assert_eq!(names(&store), ["Party parrot"]);

    // Other bytes: a second item.
    let wave =
        gifs::add_to_collection(&source, &mut collection, &other, None, NOW + 120).expect("added");
    assert_ne!(wave.content, first.content);
    assert_eq!(store.files(), 2);
    assert_eq!(names(&store), ["Party parrot", "Wave"]);
    assert_eq!(
        store.saved().map(|i| i.total_bytes()),
        Some(first.bytes + wave.bytes)
    );
}

#[test]
fn refuses_a_download_that_is_not_a_gif() {
    let fake = item("404", "Not a GIF", GifKind::Sticker);
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01".to_vec();
    let source = serving(FakeGifSource::new(), &fake, png);
    let mut collection = MemoryCollection::new();
    let store = collection.clone();

    let refused = gifs::add_to_collection(&source, &mut collection, &fake, None, NOW);
    assert!(
        matches!(refused, Err(BezelError::InvalidInput(ref text)) if text.contains("not a GIF")),
        "{refused:?}"
    );
    assert_eq!(store.files(), 0, "nothing kept");
    assert_eq!(store.saves(), 0, "the index is not saved");
    assert_eq!(store.calls(), [CollectionCall::Load]);
    assert_eq!(
        source.downloads(),
        [(
            location(&fake, Tier::Medium, RenditionFormat::Gif),
            ITEM_LIMIT
        )],
        "no preview read for it"
    );
}

#[test]
fn renames_and_deletes_items() {
    let party = item("101", "Party parrot", GifKind::Gif);
    let source = serving(FakeGifSource::new(), &party, gif(1, 64, 64));
    let mut collection = MemoryCollection::new();
    let store = collection.clone();
    let added =
        gifs::add_to_collection(&source, &mut collection, &party, None, NOW).expect("added");
    let id = added.content.clone();

    let renamed = gifs::rename(&mut collection, &id, "  Dancing parrot ").expect("renamed");
    assert_eq!(renamed.map(|i| i.name), Some("Dancing parrot".into()));
    assert_eq!(names(&store), ["Dancing parrot"]);
    let empty = gifs::rename(&mut collection, &id, "   ");
    assert!(
        matches!(empty, Err(BezelError::InvalidInput(_))),
        "{empty:?}"
    );
    assert_eq!(
        names(&store),
        ["Dancing parrot"],
        "an empty name changes nothing"
    );
    let unknown = ContentId::from_digest([7; 32]);
    let saves = store.saves();
    assert_eq!(gifs::rename(&mut collection, &unknown, "x"), Ok(None));
    assert_eq!(store.saves(), saves, "nothing to rename, nothing saved");

    // Deleting needs a confirmation; without it nothing is read or changed.
    let calls = store.calls().len();
    assert_eq!(
        gifs::delete(&mut collection, &id, Confirm::No),
        Err(BezelError::NotConfirmed(
            "deleting an item of the collection".into()
        ))
    );
    assert_eq!(store.calls().len(), calls);
    assert!(store.holds(&id));

    let deleted = gifs::delete(&mut collection, &id, Confirm::Yes).expect("deleted");
    assert_eq!(deleted.map(|i| i.name), Some("Dancing parrot".into()));
    assert_eq!(names(&store), Vec::<String>::new());
    assert!(!store.holds(&id), "its bytes are gone");
    assert_eq!(store.preview_of(&id), None, "and its preview");
    assert_eq!(gifs::delete(&mut collection, &id, Confirm::Yes), Ok(None));
}

#[test]
fn search_leaves_out_items_that_cannot_be_collected() {
    let party = item("101", "Party parrot", GifKind::Sticker);
    let mut huge = item("102", "Huge", GifKind::Sticker);
    huge.renditions
        .retain(|r| r.tier == Tier::Large || r.format == RenditionFormat::Jpeg);
    let page = GifPage {
        items: vec![party.clone(), huge],
        has_next: true,
    };
    let source = FakeGifSource::new().with_page(GifKind::Sticker, "parrot", 2, page);
    let query = GifQuery::new(
        GifKind::Sticker,
        " parrot ",
        2,
        Explicit::Hidden,
        Language::PortugueseBr,
    );

    let found = gifs::search(&source, &query).expect("a page");
    assert_eq!(found.items, [party]);
    assert!(found.has_next);
    assert_eq!(source.calls(), [GifCall::Page(query)]);

    let limited = FakeGifSource::new().failing(BezelError::Service(ServiceFailure::RateLimited));
    let trending = GifQuery::new(GifKind::Gif, "", 1, Explicit::Shown, Language::English);
    assert_eq!(
        gifs::search(&limited, &trending),
        Err(BezelError::Service(ServiceFailure::RateLimited))
    );
}

#[test]
fn previews_are_small_and_in_their_format() {
    let party = item("101", "Party parrot", GifKind::Gif);
    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];
    let source = serving(FakeGifSource::new(), &party, gif(1, 64, 64)).with_file(
        &location(&party, Tier::Small, RenditionFormat::Jpeg),
        jpeg.clone(),
    );

    let moving = gifs::preview(&source, &party, Motion::Animated).expect("read");
    assert_eq!(moving, Some(gif(0xEE, 200, 200)));
    assert_eq!(
        gifs::preview(&source, &party, Motion::Still),
        Ok(Some(jpeg))
    );
    assert!(
        source
            .downloads()
            .iter()
            .all(|(_, limit)| *limit == PREVIEW_LIMIT)
    );

    // A still that is not a JPEG is refused; no rendition, no preview.
    let wrong = FakeGifSource::new().with_file(
        &location(&party, Tier::Small, RenditionFormat::Jpeg),
        gif(3, 1, 1),
    );
    assert!(matches!(
        gifs::preview(&wrong, &party, Motion::Still),
        Err(BezelError::InvalidInput(_))
    ));
    let mut bare = party.clone();
    bare.renditions.retain(|r| r.tier == Tier::Large);
    assert_eq!(gifs::preview(&wrong, &bare, Motion::Animated), Ok(None));
}

#[test]
fn a_new_item_keeps_the_preview_already_read_when_it_is_a_gif() {
    let party = item("101", "Party parrot", GifKind::Gif);
    let source = serving(FakeGifSource::new(), &party, gif(1, 64, 64));
    let mut collection = MemoryCollection::new();
    let store = collection.clone();
    let shown = gif(9, 200, 200);
    let added = gifs::add_to_collection(&source, &mut collection, &party, Some(&shown), NOW)
        .expect("added");
    assert_eq!(store.preview_of(&added.content), Some(shown));
    assert_eq!(source.downloads().len(), 1, "no preview read again");

    // A still shown with reduced motion is not a GIF: the small GIF is read.
    let other = item("202", "Wave", GifKind::Sticker);
    let source = serving(source, &other, gif(2, 64, 64));
    let still = [0xFF, 0xD8, 0xFF, 0xE0];
    let wave = gifs::add_to_collection(&source, &mut collection, &other, Some(&still), NOW)
        .expect("added");
    assert_eq!(store.preview_of(&wave.content), Some(gif(0xEE, 200, 200)));
    assert_eq!(
        source.downloads().last().map(|(_, limit)| *limit),
        Some(PREVIEW_LIMIT)
    );

    // A preview that cannot be read leaves the item without one.
    let third = item("303", "", GifKind::Gif);
    let source = source.with_file(
        &location(&third, Tier::Medium, RenditionFormat::Gif),
        gif(3, 8, 8),
    );
    let unnamed =
        gifs::add_to_collection(&source, &mut collection, &third, None, NOW).expect("added");
    assert_eq!(unnamed.name, "303", "named by its id without a title");
    assert_eq!(store.preview_of(&unnamed.content), None);
    assert_eq!(store.files(), 3);
}

#[test]
fn a_failed_download_keeps_nothing() {
    let party = item("101", "Party parrot", GifKind::Gif);
    let limited = FakeGifSource::new().failing(BezelError::Service(ServiceFailure::RateLimited));
    let mut collection = MemoryCollection::new();
    let store = collection.clone();
    assert_eq!(
        gifs::add_to_collection(&limited, &mut collection, &party, None, NOW),
        Err(BezelError::Service(ServiceFailure::RateLimited))
    );
    assert_eq!((store.files(), store.saves()), (0, 0));

    // Nothing to collect: no download at all.
    let mut stills = party.clone();
    stills
        .renditions
        .retain(|r| r.format == RenditionFormat::Jpeg || r.tier == Tier::Large);
    let source = FakeGifSource::new();
    let refused = gifs::add_to_collection(&source, &mut collection, &stills, None, NOW);
    assert!(
        matches!(refused, Err(BezelError::InvalidInput(_))),
        "{refused:?}"
    );
    assert!(source.calls().is_empty());
}

/// A provider that sends more than the limit it was given.
struct Oversized;

impl GifSource for Oversized {
    fn provider(&self) -> &str {
        "oversized"
    }

    fn page(&self, _: &GifQuery) -> bezel_core::Result<GifPage> {
        Ok(GifPage::default())
    }

    fn download(&self, _: &Rendition, limit: u64) -> bezel_core::Result<Vec<u8>> {
        let mut bytes = gif(1, 8, 8);
        bytes.resize(usize::try_from(limit).expect("limit") + 1, 0);
        Ok(bytes)
    }
}

#[test]
fn bytes_over_the_limit_are_refused_whatever_the_source_does() {
    let party = item("101", "Party parrot", GifKind::Gif);
    let mut collection = MemoryCollection::new();
    let store = collection.clone();
    let refused = gifs::add_to_collection(&Oversized, &mut collection, &party, None, NOW);
    assert!(
        matches!(refused, Err(BezelError::InvalidInput(ref text)) if text.contains("26214400")),
        "{refused:?}"
    );
    assert_eq!((store.files(), store.saves()), (0, 0));
    assert!(matches!(
        gifs::preview(&Oversized, &party, Motion::Animated),
        Err(BezelError::InvalidInput(_))
    ));
}
