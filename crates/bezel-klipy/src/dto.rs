//! KLIPY's answers, read tolerantly into the core's types.
//!
//! A page is `{"result": true, "data": {"data": [item, ...], "has_next":
//! bool}}`; an item is `{"id", "slug", "title", "file": {"hd"|"md"|"sm"|"xs":
//! {"gif"|"jpg"|...: {"url", "width", "height", "size"}}}}`. Unknown fields
//! (tags, blur previews, WebP and MP4 files, ads) are ignored; a rendition
//! without an address on the file host or without a size in pixels is
//! skipped; an item without an id or without a GIF is dropped.

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

/// The formats read; the others (WebP, MP4, WebM) are not used.
const FORMATS: [(&str, RenditionFormat); 2] = [
    ("gif", RenditionFormat::Gif),
    ("jpg", RenditionFormat::Jpeg),
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
            ["4170656934727386", "8840391275611024"],
            "no ad, no GIF-less item"
        );
        let first = &gifs.items[0];
        assert_eq!(first.title, "Happy Cat Dance");
        assert_eq!(
            first.page_url.as_deref(),
            Some("https://klipy.com/gifs/happy-cat-dance-xT4uQ")
        );
        assert_eq!(first.renditions.len(), 8, "4 sizes x GIF and JPEG");
        let large = first.download().expect("a GIF to collect");
        assert_eq!(
            (large.tier, large.width, large.height),
            (Tier::Large, 498, 280)
        );
        assert_eq!(large.bytes, Some(2_481_220));
        assert_eq!(
            large.location,
            "https://static.klipy.com/ii/example/1001/hd.gif"
        );

        // A rendition without a size in pixels, or off the file host, is
        // skipped; the item stays with the rest.
        let second = &gifs.items[1];
        assert_eq!(second.title, "", "no title");
        let tiers: Vec<_> = second
            .renditions
            .iter()
            .map(|r| (r.tier, r.format))
            .collect();
        assert_eq!(
            tiers,
            [
                (Tier::Medium, RenditionFormat::Gif),
                (Tier::Small, RenditionFormat::Gif),
                (Tier::Small, RenditionFormat::Jpeg),
            ]
        );
        assert_eq!(
            second.page_url, None,
            "a slug that is not one is not linked"
        );

        let stickers = page(
            include_str!("../fixtures/stickers-trending.json"),
            GifKind::Sticker,
        )
        .expect("a page");
        assert!(!stickers.has_next);
        assert_eq!(stickers.items.len(), 1);
        assert_eq!(stickers.items[0].kind, GifKind::Sticker);
        assert_eq!(
            stickers.items[0].page_url.as_deref(),
            Some("https://klipy.com/stickers/party-parrot-Qm3")
        );
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
