//! KLIPY's answers, read tolerantly into the core's types.
//!
//! A page is `{"result": true, "data": {"data": [item, ...], "current_page",
//! "per_page", "has_next": bool, "meta"}}`; an item is `{"id": number,
//! "slug", "title", "file": {"hd"|"md"|"sm"|"xs": {"gif"|"jpg"|"png"|...:
//! {"url", "width", "height", "size"}}}, "tags", "type", "blur_preview"}`
//! (as recorded from the real API on 2026-10-01: a GIF comes as GIF, JPEG,
//! WebP, MP4 and WebM files, a sticker as GIF, PNG, WebP and WebM ones).
//! Unknown fields (tags, the inline blur preview, the page's meta, WebP,
//! MP4 and WebM files, ads) are ignored; a rendition without an address on
//! the file host or without a size in pixels is skipped; an item without an
//! id or without a GIF is dropped. A refused key is answered with
//! `{"result": false, "errors": {"message": ["The provided API key is
//! invalid."]}}`.

use bezel_core::domain::gifs::{
    GifItem, GifKind, GifPage, PAGE_SIZE, Rendition, RenditionFormat, Tier,
};
use serde_json::Value;

/// KLIPY's sizes, from the largest.
const TIERS: [(&str, Tier); 4] = [
    ("hd", Tier::Large),
    ("md", Tier::Medium),
    ("sm", Tier::Small),
    ("xs", Tier::Tiny),
];

/// The formats read: GIFs, a GIF's JPEG still and a sticker's PNG one; the
/// others (WebP, MP4, WebM) are not used.
const FORMATS: [(&str, RenditionFormat); 3] = [
    ("gif", RenditionFormat::Gif),
    ("jpg", RenditionFormat::Jpeg),
    ("png", RenditionFormat::Png),
];

/// KLIPY's site, where an item's page lives under its slug.
const SITE: &str = "https://klipy.com";

/// The page in `body`, its items of `kind` with renditions only under
/// `files` (the file host's origin); `None` when `body` is not a page.
pub(crate) fn read_page(body: &[u8], kind: GifKind, files: &str) -> Option<GifPage> {
    let answer: Value = serde_json::from_slice(body).ok()?;
    if answer.get("result").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let data = answer.get("data")?;
    let items = data.get("data")?.as_array()?;
    let host = format!("{files}/");
    let items = items
        .iter()
        .filter_map(|item| read_item(item, kind, &host))
        .take(PAGE_SIZE as usize)
        .collect();
    let has_next = data.get("has_next").and_then(Value::as_bool) == Some(true);
    Some(GifPage { items, has_next })
}

/// Whether `body` is KLIPY's answer to a key it does not know: `"result":
/// false` with an error message about the key. Only tells yes or no: the
/// message itself is never carried further.
pub(crate) fn refuses_the_key(body: &[u8]) -> bool {
    let Ok(answer) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    if answer.get("result").and_then(Value::as_bool) != Some(false) {
        return false;
    }
    let Some(message) = answer.get("errors").and_then(|e| e.get("message")) else {
        return false;
    };
    let texts: Vec<&str> = match message {
        Value::String(text) => vec![text.as_str()],
        Value::Array(texts) => texts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    texts.into_iter().any(names_the_key)
}

/// Whether `text` speaks of the key: the word "key", in any case.
fn names_the_key(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("key"))
}

fn read_item(item: &Value, kind: GifKind, host: &str) -> Option<GifItem> {
    let id = read_id(item.get("id")?)?;
    let renditions = item
        .get("file")
        .map(|file| read_renditions(file, host))
        .unwrap_or_default();
    if !renditions.iter().any(|r| r.format == RenditionFormat::Gif) {
        return None;
    }
    let title = item.get("title").and_then(Value::as_str).unwrap_or("");
    let page_url = item
        .get("slug")
        .and_then(Value::as_str)
        .filter(|slug| is_slug(slug))
        .map(|slug| format!("{SITE}/{}/{slug}", section(kind)));
    Some(GifItem {
        id,
        title: title.trim().to_string(),
        kind,
        page_url,
        renditions,
    })
}

/// An id as KLIPY gives it, a number or a text.
fn read_id(id: &Value) -> Option<String> {
    match id {
        Value::Number(n) => n.as_u64().map(|n| n.to_string()),
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

fn read_renditions(file: &Value, host: &str) -> Vec<Rendition> {
    let mut renditions = Vec::new();
    for (size, tier) in TIERS {
        let Some(formats) = file.get(size) else {
            continue;
        };
        for (name, format) in FORMATS {
            let read = formats
                .get(name)
                .and_then(|r| read_rendition(r, tier, format, host));
            renditions.extend(read);
        }
    }
    renditions
}

fn read_rendition(
    rendition: &Value,
    tier: Tier,
    format: RenditionFormat,
    host: &str,
) -> Option<Rendition> {
    let location = rendition.get("url")?.as_str()?;
    if !location.starts_with(host) {
        return None;
    }
    let side = |name| {
        let side = rendition.get(name)?.as_u64()?;
        u32::try_from(side).ok().filter(|&side| side > 0)
    };
    Some(Rendition {
        tier,
        format,
        location: location.to_string(),
        width: side("width")?,
        height: side("height")?,
        bytes: rendition.get("size").and_then(Value::as_u64),
    })
}

fn is_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The part of KLIPY's site (and of its API) for `kind`.
pub(crate) const fn section(kind: GifKind) -> &'static str {
    match kind {
        GifKind::Gif => "gifs",
        GifKind::Sticker => "stickers",
    }
}

#[cfg(test)]
mod tests {
    use bezel_core::domain::gifs::Motion;

    use super::*;

    const FILES: &str = "https://static.klipy.com";

    fn page(json: &str, kind: GifKind) -> Option<GifPage> {
        read_page(json.as_bytes(), kind, FILES)
    }

    #[test]
    fn reads_the_recorded_pages() {
        let gifs =
            page(include_str!("../fixtures/gifs-search.json"), GifKind::Gif).expect("a page");
        assert!(gifs.has_next);
        let ids: Vec<&str> = gifs.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            ["2484942301552561", "5797057055690732", "9082712784681109"]
        );
        let first = &gifs.items[0];
        assert_eq!(first.title, "Goatplaybanjo's Chatty Cat");
        assert_eq!(first.kind, GifKind::Gif);
        assert_eq!(
            first.page_url.as_deref(),
            Some("https://klipy.com/gifs/goatplaybanjo-chat-4--k3UjPXVTp")
        );
        let formats: Vec<_> = first
            .renditions
            .iter()
            .map(|r| (r.tier, r.format))
            .collect();
        assert_eq!(
            formats,
            [
                (Tier::Large, RenditionFormat::Gif),
                (Tier::Large, RenditionFormat::Jpeg),
                (Tier::Medium, RenditionFormat::Gif),
                (Tier::Medium, RenditionFormat::Jpeg),
                (Tier::Small, RenditionFormat::Gif),
                (Tier::Small, RenditionFormat::Jpeg),
                (Tier::Tiny, RenditionFormat::Gif),
                (Tier::Tiny, RenditionFormat::Jpeg),
            ],
            "4 sizes x GIF and JPEG; WebP, MP4 and WebM left out"
        );
        let still = first.preview(Motion::Still).expect("a still");
        assert_eq!(
            (still.tier, still.format, still.bytes),
            (Tier::Small, RenditionFormat::Jpeg, Some(8291))
        );

        // KLIPY's "md" GIF can be larger than its "hd" one: the largest by
        // area is collected.
        let kitten = gifs.items[2].download().expect("a GIF to collect");
        assert_eq!(
            (kitten.tier, kitten.width, kitten.height, kitten.bytes),
            (Tier::Medium, 512, 640, Some(2_525_411))
        );
        assert!(kitten.location.starts_with("https://static.klipy.com/ii/"));

        let stickers = page(
            include_str!("../fixtures/stickers-trending.json"),
            GifKind::Sticker,
        )
        .expect("a page");
        assert!(stickers.has_next);
        assert_eq!(stickers.items.len(), 2);
        let doraemon = &stickers.items[0];
        assert_eq!(doraemon.kind, GifKind::Sticker);
        assert_eq!(doraemon.title, "Doraemon Sleeping Peacefully with Zzzs");
        assert_eq!(
            doraemon.page_url.as_deref(),
            Some("https://klipy.com/stickers/doraemon-sleeping-sticker")
        );
        assert!(
            doraemon
                .renditions
                .iter()
                .all(|r| r.format != RenditionFormat::Jpeg),
            "a sticker has no JPEG"
        );
        let still = doraemon.preview(Motion::Still).expect("a PNG still");
        assert_eq!(
            (still.tier, still.format, still.width, still.height),
            (Tier::Small, RenditionFormat::Png, 191, 200)
        );
        let moving = doraemon.preview(Motion::Animated).map(|r| r.format);
        assert_eq!(moving, Some(RenditionFormat::Gif));
    }

    #[test]
    fn skips_what_it_cannot_read() {
        let json = r#"{"result": true, "data": {"data": [
            {"type": "ad", "content": "<div></div>", "width": 300, "height": 250},
            {"id": 11, "slug": "webp-only", "file": {"hd": {"webp":
                {"url": "https://static.klipy.com/ii/a/1.webp", "width": 9, "height": 9}}}},
            {"id": "  ", "file": {"sm": {"gif":
                {"url": "https://static.klipy.com/ii/a/2.gif", "width": 9, "height": 9}}}},
            {"id": 12, "slug": "Not a slug!", "title": "  ", "file": {
                "hd": {"gif": {"url": "https://static.klipy.com/ii/a/3.gif", "height": 9}},
                "md": {"gif": {"url": "https://static.klipy.com/ii/a/4.gif", "width": 9, "height": 9}},
                "sm": {"png": {"url": "https://cdn.example.com/ii/a/5.png", "width": 9, "height": 9},
                       "jpg": {"url": "https://static.klipy.com/ii/a/6.jpg", "width": 9, "height": 0}}}}
        ], "has_next": false, "meta": {"item_min_width": 80}}}"#;
        let read = page(json, GifKind::Gif).expect("a page");
        assert_eq!(read.items.len(), 1, "no ad, no GIF-less or id-less item");
        let item = &read.items[0];
        assert_eq!((item.id.as_str(), item.title.as_str()), ("12", ""));
        assert_eq!(item.page_url, None, "a slug that is not one is not linked");
        let kept: Vec<_> = item
            .renditions
            .iter()
            .map(|r| r.location.as_str())
            .collect();
        assert_eq!(
            kept,
            ["https://static.klipy.com/ii/a/4.gif"],
            "no size in pixels, off the file host: skipped"
        );
    }

    #[test]
    fn tells_a_refused_key() {
        assert!(refuses_the_key(include_bytes!(
            "../fixtures/invalid-key-404.json"
        )));
        assert!(refuses_the_key(
            br#"{"result": false, "errors": {"message": "Invalid KEY"}}"#
        ));
        for other in [
            &br#"{"result": false, "errors": {"message": ["Too many keywords."]}}"#[..],
            br#"{"result": true, "errors": {"message": ["The provided API key is invalid."]}}"#,
            br#"{"result": false, "errors": {"other": ["key"]}}"#,
            br#"{"result": false}"#,
            b"<html>key</html>",
            include_bytes!("../fixtures/gifs-search.json"),
        ] {
            assert!(
                !refuses_the_key(other),
                "{}",
                String::from_utf8_lossy(other)
            );
        }
    }

    #[test]
    fn what_is_not_a_page_is_none() {
        assert_eq!(page("not json", GifKind::Gif), None);
        assert_eq!(
            page(r#"{"result": false, "data": {"data": []}}"#, GifKind::Gif),
            None
        );
        assert_eq!(page(r#"{"result": true}"#, GifKind::Gif), None);
        assert_eq!(page(r#"{"data": {"data": {}}}"#, GifKind::Gif), None);
        let empty = page(r#"{"data": {"data": [], "has_next": "yes"}}"#, GifKind::Gif);
        assert_eq!(
            empty,
            Some(GifPage::default()),
            "an odd has_next is no next page"
        );
    }

    #[test]
    fn ids_are_numbers_or_texts() {
        assert_eq!(read_id(&Value::from(42u64)), Some("42".into()));
        assert_eq!(read_id(&Value::from(" a1 ")), Some("a1".into()));
        assert_eq!(read_id(&Value::from("  ")), None);
        assert_eq!(read_id(&Value::from(-1)), None);
        assert_eq!(read_id(&Value::Null), None);
    }

    #[test]
    fn pages_hold_at_most_24_items() {
        let item = r#"{"id": 1, "file": {"sm": {"gif": {"url": "https://static.klipy.com/a.gif", "width": 1, "height": 1}}}}"#;
        let items = vec![item; 30].join(",");
        let json = format!(r#"{{"data": {{"data": [{items}], "has_next": true}}}}"#);
        assert_eq!(page(&json, GifKind::Gif).map(|p| p.items.len()), Some(24));
    }
}
