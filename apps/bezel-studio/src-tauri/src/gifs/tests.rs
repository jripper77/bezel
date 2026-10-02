//! The studio's GIF state over the fake provider ([`FakeGifSource`]), a real
//! collection on disk ([`DiskCollection`] in a temporary folder) and a real
//! backend (the Skia renderer, the scripted converter): the commands' own
//! code, without Tauri.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bezel_core::domain::clock::Language;
use bezel_core::domain::frame::Rgba;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::gifs::{GifItem, GifKind, GifPage, Rendition, RenditionFormat, Tier};
use bezel_core::domain::poster::PosterSpec;
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::theme::{
    AssetRef, Background, BoxF, Element, ElementId, ElementKind, Fit, Theme,
};
use bezel_core::ports::{GifSource, MediaLocation, ThemeLocation};
use bezel_devices::FakeBus;
use bezel_devices::fake::FakeStorage;
use bezel_media::archive::MemoryArchive;
use bezel_media::collection::{DiskCollection, FakeGifSource, GifCall, collection_dir};
use bezel_themes::dto::ThemeDto;
use serde_json::Value;

use super::*;
use crate::manager::Copies;
use crate::storage::tests::{FakeMedia, Fixture as Studio, TIME, fixture_on};
use crate::studio::Motion as Playback;

/// An obvious fake key: long enough for its last 4 characters to show.
const KEY: &str = "fake-KLIPY_key-0123456789abcdef";

/// `text` as a key.
fn klipy(text: &str) -> KlipyKey {
    KlipyKey::parse(text).unwrap()
}

/// The user's action behind each search, preview and collect of these
/// tests: what a command the window invoked gives the GIF state.
const ASKED: UserAsked = UserAsked::in_a_test();

/// A theme's background color under the sticker.
const BLUE: Rgba = Rgba::opaque(0, 0, 255);

/// The sticker's opaque half.
const RED: Rgba = Rgba::opaque(255, 0, 0);

struct Fixture {
    /// The backend, in a temporary folder removed when dropped.
    studio: Studio,
    gifs: Gifs,
    source: FakeGifSource,
    /// The keys and customer ids sources were made with.
    made: Arc<Mutex<Vec<(String, String)>>>,
    /// The posters the scripted converter took.
    posters: Arc<Mutex<Vec<(MediaLocation, PosterSpec)>>>,
}

impl Fixture {
    fn backend(&self) -> &Backend {
        &self.studio.backend
    }

    fn key_path(&self) -> PathBuf {
        self.studio.root.join("config").join(KEY_FILE)
    }

    fn scratch(&self) -> PathBuf {
        self.studio.root.join("cache").join("collection")
    }

    fn made(&self) -> Vec<(String, String)> {
        self.made.lock().unwrap().clone()
    }

    /// The pages asked of the provider.
    fn pages_asked(&self) -> usize {
        let calls = self.source.calls();
        calls
            .iter()
            .filter(|c| matches!(c, GifCall::Page(_)))
            .count()
    }

    /// Saves the key and searches `text` among `kind`, page `page`.
    fn search(&self, kind: &str, text: &str, page: u32) -> GifPageDto {
        let query = query(kind, text, page, false, Language::English).unwrap();
        self.gifs.search(&ASKED, &query).unwrap()
    }

    /// Searches `text` among `kind` and collects its first result.
    fn collect_first(&self, kind: &str, text: &str) -> CollectedDto {
        let page = self.search(kind, text, 1);
        self.gifs.collect(&ASKED, &page.items[0].id).unwrap()
    }
}

/// The GIF state as the app composes it, over `source` and a collection in
/// the fixture's folder, beside a backend editing a vertical 8.8" theme.
fn fixture(name: &str, source: FakeGifSource) -> Fixture {
    let media = FakeMedia::ready();
    let posters = Arc::clone(&media.posters);
    let studio = fixture_on(
        &format!("gifs-{name}"),
        FakeBus::turing_88(),
        FakeStorage::default(),
        media,
        Copies::in_memory(MemoryArchive::new()),
    );
    let made = Arc::new(Mutex::new(Vec::new()));
    let root = studio.root.clone();
    let gifs = gifs_in(&root, &root.join("data"), &source, &made);
    Fixture {
        studio,
        gifs,
        source,
        made,
        posters,
    }
}

/// The GIF state as the app composes it, its key in `root` and its
/// collection in the data folder `data` (on disk, as the app opens it),
/// over `source`; the sources made are recorded in `made`.
fn gifs_in(
    root: &Path,
    data: &Path,
    source: &FakeGifSource,
    made: &Arc<Mutex<Vec<(String, String)>>>,
) -> Gifs {
    let factory: SourceFactory = {
        let source = source.clone();
        let made = Arc::clone(made);
        Arc::new(
            move |_: &UserAsked, key: &KlipyKey, customer: &str| -> Arc<dyn GifSource> {
                made.lock()
                    .unwrap()
                    .push((key.expose_secret().to_string(), customer.to_string()));
                Arc::new(source.clone())
            },
        )
    };
    Gifs::new(
        KeyFile::new(root.join("config").join(KEY_FILE)),
        Provider {
            source: factory,
            customer_id: bezel_klipy::new_customer_id,
        },
        collection_in(collection_dir(data), |dir| DiskCollection::open(dir)),
        root.join("cache").join("collection"),
    )
}

fn rendition(tier: Tier, format: RenditionFormat, location: String, side: u32) -> Rendition {
    Rendition {
        tier,
        format,
        location,
        width: side * 2,
        height: side,
        bytes: None,
    }
}

/// The still a result of `kind` comes with, as KLIPY offers them: a GIF's
/// is a JPEG, a sticker's a PNG; its format and file extension.
fn still_of(kind: GifKind) -> (RenditionFormat, &'static str) {
    match kind {
        GifKind::Gif => (RenditionFormat::Jpeg, "jpg"),
        GifKind::Sticker => (RenditionFormat::Png, "png"),
    }
}

/// A result offered as a large GIF, a small one and a still.
fn item(id: &str, kind: GifKind, title: &str) -> GifItem {
    let (still, extension) = still_of(kind);
    GifItem {
        id: id.to_string(),
        title: title.to_string(),
        kind,
        page_url: Some(format!("https://klipy.com/{}s/{id}", kind.slug())),
        renditions: vec![
            rendition(
                Tier::Large,
                RenditionFormat::Gif,
                format!("f/{id}.gif"),
                240,
            ),
            rendition(
                Tier::Small,
                RenditionFormat::Gif,
                format!("f/{id}-s.gif"),
                60,
            ),
            rendition(Tier::Small, still, format!("f/{id}-s.{extension}"), 60),
        ],
    }
}

/// A JPEG's first bytes: what a GIF's still is checked for.
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, b'j', b'p', b'g'];

/// A PNG's first bytes: what a sticker's still is checked for.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

/// `source` serving `item`'s files: `gif` as its large and small GIF.
fn serving(source: FakeGifSource, item: &GifItem, gif: &[u8]) -> FakeGifSource {
    let id = &item.id;
    let (format, extension) = still_of(item.kind);
    let still = if format == RenditionFormat::Png {
        PNG
    } else {
        JPEG
    };
    source
        .with_file(&format!("f/{id}.gif"), gif.to_vec())
        .with_file(&format!("f/{id}-s.gif"), gif.to_vec())
        .with_file(&format!("f/{id}-s.{extension}"), still.to_vec())
}

/// `source` answering `items` (with `gif` as each one's files) to the page
/// `page` of the search for `text` among `kind`.
fn with_results(
    source: FakeGifSource,
    kind: GifKind,
    text: &str,
    page: u32,
    items: &[(&str, &str, Vec<u8>)],
    has_next: bool,
) -> FakeGifSource {
    let results: Vec<GifItem> = items
        .iter()
        .map(|(id, title, _)| item(id, kind, title))
        .collect();
    let mut source = source.with_page(
        kind,
        text,
        page,
        GifPage {
            items: results.clone(),
            has_next,
        },
    );
    for (result, (_, _, gif)) in results.iter().zip(items) {
        source = serving(source, result, gif);
    }
    source
}

/// A GIF of `count` pictures (2x2, opaque).
fn gif(count: u8) -> Vec<u8> {
    crate::media::tests::gif(count)
}

/// A sticker of 2 pictures: 8x8, its left half transparent, its right half
/// red.
fn sticker() -> Vec<u8> {
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame as Picture, Rgba as Pixel, RgbaImage};
    let picture = RgbaImage::from_fn(8, 8, |x, _| {
        if x < 4 {
            Pixel([0, 0, 0, 0])
        } else {
            Pixel([RED.r, RED.g, RED.b, 255])
        }
    });
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        let pictures = (0..2).map(|_| {
            Picture::from_parts(picture.clone(), 0, 0, Delay::from_numer_denom_ms(100, 1))
        });
        encoder.encode_frames(pictures).unwrap();
    }
    bytes
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap()
}

/// D-2026-10-01-gif-sticker-search-3: making the state reads nothing and
/// asks nothing (the app makes it at start); without a key no command
/// reaches the provider, not even with a page answered before; saving a key
/// asks nothing either: only the first search does.
#[test]
fn no_request_at_start_or_without_key() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "cat",
        1,
        &[("a1", "Cat", gif(2))],
        false,
    );
    let f = fixture("no-key", source);
    assert!(f.source.calls().is_empty(), "nothing at start");
    assert!(f.made().is_empty(), "no source made at start");
    assert!(!f.key_path().exists());
    assert_eq!(
        f.gifs.key_status().unwrap(),
        KeyDto {
            configured: false,
            last4: None
        }
    );

    let cat = query("gif", "cat", 1, false, Language::English).unwrap();
    assert_eq!(
        f.gifs.search(&ASKED, &cat).unwrap_err().code(),
        "klipyNoKey"
    );
    assert_eq!(
        f.gifs.preview(&ASKED, "a1", false).unwrap_err().code(),
        "klipyNoKey"
    );
    assert_eq!(
        f.gifs.collect(&ASKED, "a1").unwrap_err().code(),
        "klipyNoKey"
    );
    assert!(
        f.gifs.list(false).unwrap().is_empty(),
        "the collection is local"
    );
    assert!(f.source.calls().is_empty(), "nothing without a key");
    assert!(f.made().is_empty());

    f.gifs.save_key(klipy(KEY)).unwrap();
    assert!(f.source.calls().is_empty(), "saving a key asks nothing");
    assert!(f.made().is_empty());

    assert_eq!(f.search("gif", "cat", 1).items.len(), 1);
    assert_eq!(f.source.calls(), [GifCall::Page(cat.clone())]);
    assert_eq!(f.made().len(), 1);

    f.gifs.remove_key().unwrap();
    assert_eq!(
        f.gifs.search(&ASKED, &cat).unwrap_err().code(),
        "klipyNoKey"
    );
    assert_eq!(
        f.gifs.preview(&ASKED, "a1", false).unwrap_err().code(),
        "klipyNoKey"
    );
    assert_eq!(f.source.calls().len(), 1, "nothing once removed");
}

/// What the window gets from a GIF command, an answer or an error, as the
/// IPC sends it.
fn answer<T: serde::Serialize>(got: UiResult<T>) -> String {
    match got {
        Ok(value) => json(&value),
        Err(error) => json(&error),
    }
}

/// The first part of `key` longer than its last 4 characters (any 5 of its
/// characters in a row) that `answer` shows, `root` (a folder the answer
/// may name) left out.
fn shown_of(answer: &str, key: &str, root: &Path) -> Option<String> {
    let root = root.display().to_string();
    let escaped = json(&root);
    let answer = answer
        .replace(escaped.trim_matches('"'), "")
        .replace(&root, "");
    let characters: Vec<char> = key.chars().collect();
    characters
        .windows(5)
        .map(|part| part.iter().collect::<String>())
        .find(|part| answer.contains(part.as_str()))
}

/// Key files that cannot be used, each with the key text it holds: written
/// by hand or damaged, they reach the window only as errors (the critic of
/// round 2, iter 4), which must quote nothing of that text. `customer` is a
/// customer id that can be one.
fn unusable_key_files(customer: &str) -> Vec<(String, &'static str)> {
    // Characters no path, code or message here holds five of in a row.
    const HELD: &str = "Zq7Xw2Vb9Nm4Lk8Jh3Gf6Dd1";
    const DIGITS: &str = "7351902846";
    vec![
        // A key that can be one, its customer id not.
        (
            format!(r#"{{"key": "{HELD}", "customerId": "not an id!"}}"#),
            HELD,
        ),
        (format!(r#"{{"key": "{HELD}", "customerId": 42}}"#), HELD),
        (format!(r#"{{"key": "{HELD}"}}"#), HELD),
        // A key with a space or a line break after it, or before it.
        (
            format!(r#"{{"key": "{HELD} ", "customerId": "{customer}"}}"#),
            HELD,
        ),
        (
            format!(r#"{{"key": "{HELD}\n", "customerId": "{customer}"}}"#),
            HELD,
        ),
        (
            format!(r#"{{"key": "\r\n{HELD}", "customerId": "{customer}"}}"#),
            HELD,
        ),
        // A key that is not a string.
        (
            format!(r#"{{"key": {DIGITS}, "customerId": "{customer}"}}"#),
            DIGITS,
        ),
        (
            format!(r#"{{"key": ["{HELD}"], "customerId": "{customer}"}}"#),
            HELD,
        ),
        (
            format!(r#"{{"key": {{"text": "{HELD}"}}, "customerId": "{customer}"}}"#),
            HELD,
        ),
        // Cut short: after the key, inside it, a line of it.
        (
            format!(r#"{{"key": "{HELD}", "customerId": "{customer}""#),
            HELD,
        ),
        (format!(r#"{{"key": "{HELD}"#), HELD),
        (format!("{HELD}\n"), HELD),
    ]
}

/// D-2026-10-01-gif-sticker-search-3: the key goes to the file (0600 on
/// Unix) and to the source, nowhere else: not in any answer the window
/// gets, errors included, nor in a `Debug` output; the window sees its last
/// 4 characters. A new key gets a new customer id; removing deletes the
/// file. A key file that cannot be used (the critic of round 2, iter 4)
/// gets every key command and GIF command an answer that quotes nothing of
/// the key text it holds beyond its last 4 characters.
#[test]
fn key_never_reaches_the_window() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Sticker,
        "star",
        1,
        &[("s1", "Star", gif(2))],
        false,
    );
    let f = fixture("key", source);
    let saved = f.gifs.save_key(klipy(KEY)).unwrap();
    assert_eq!(
        saved,
        KeyDto {
            configured: true,
            last4: Some("cdef".into())
        }
    );

    let page = f.search("sticker", "star", 1);
    let preview = f.gifs.preview(&ASKED, "s1", false).unwrap();
    let still = f.gifs.preview(&ASKED, "s1", true).unwrap();
    let collected = f.gifs.collect(&ASKED, "s1").unwrap();
    let mut window = vec![
        json(&saved),
        json(&f.gifs.key_status().unwrap()),
        json(&page),
        json(&preview),
        json(&still),
        json(&collected),
        json(&f.gifs.list(true).unwrap()),
        json(&f.gifs.rename(&collected.id, "Mine").unwrap()),
        json(&f.gifs.users(f.backend(), &collected.id).unwrap()),
        json(&f.gifs.preview(&ASKED, "s9", false).unwrap_err()),
        json(&KlipyKey::parse("not a key!").unwrap_err()),
        json(&KlipyKey::parse(&format!("{KEY}/x")).unwrap_err()),
    ];
    // What the provider said goes to the window as a code, never with the
    // key.
    for failure in [
        ServiceFailure::RateLimited,
        ServiceFailure::KeyRejected,
        ServiceFailure::Unavailable("timed out".into()),
    ] {
        let failing = fixture(
            "key-failing",
            FakeGifSource::new().failing(BezelError::Service(failure)),
        );
        failing.gifs.save_key(klipy(KEY)).unwrap();
        let star = query("sticker", "star", 1, false, Language::English).unwrap();
        window.push(json(&failing.gifs.search(&ASKED, &star).unwrap_err()));
    }
    window.push(format!("{:?}", f.gifs));
    window.push(format!("{:?}", f.gifs.key.load().unwrap()));
    window.push(format!("{:?}", klipy(KEY)));
    for answer in &window {
        assert!(!answer.contains(KEY), "{answer}");
    }
    assert!(window.iter().any(|a| a.contains("klipyKeyRejected")));

    // The file holds the key and its customer id, for the user only; the
    // source got both.
    let file: Value = serde_json::from_slice(&std::fs::read(f.key_path()).unwrap()).unwrap();
    let customer = file["customerId"].as_str().unwrap().to_string();
    assert_eq!(file["key"], KEY);
    assert_eq!(customer.len(), 32);
    assert!(customer.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(f.made(), [(KEY.to_string(), customer.clone())]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(f.key_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // Saving it again keeps its customer id; another key gets a new one, and
    // a short key shows no characters.
    f.gifs.save_key(klipy(KEY)).unwrap();
    assert_eq!(f.gifs.key.load().unwrap().unwrap().customer_id(), customer);
    let short = f.gifs.save_key(klipy("abcd1234")).unwrap();
    assert_eq!((short.configured, short.last4), (true, None));
    assert_ne!(f.gifs.key.load().unwrap().unwrap().customer_id(), customer);

    for bad in ["", "two words", "slash/es", "ç", &"k".repeat(129)] {
        let error = KlipyKey::parse(bad).unwrap_err();
        assert_eq!(error.code(), "invalidInput");
        assert!(
            bad.is_empty() || !error.to_string().contains(bad),
            "{error}"
        );
    }
    assert!(
        f.gifs.key_status().unwrap().configured,
        "the saved key stays"
    );

    assert_eq!(
        f.gifs.remove_key().unwrap(),
        KeyDto {
            configured: false,
            last4: None
        }
    );
    assert!(!f.key_path().exists());
    assert!(!f.gifs.key_status().unwrap().configured);
    f.gifs.remove_key().unwrap();

    // Key files that cannot be used: what the window gets from `klipy_key`,
    // `search_gifs`, `gif_preview`, `collect_gif`, `save_klipy_key` and
    // `remove_klipy_key`, the file being that one each time.
    let star = query("sticker", "star", 1, false, Language::English).unwrap();
    let asked = f.source.calls().len();
    for (file, held) in &unusable_key_files(&customer) {
        let write = || std::fs::write(f.key_path(), file).unwrap();
        write();
        let status = f.gifs.key_status();
        assert_eq!(status.as_ref().err().map(UiError::code), Some("fileError"));
        let mut window = vec![answer(status)];
        window.push(answer(f.gifs.search(&ASKED, &star)));
        window.push(answer(f.gifs.preview(&ASKED, "s1", false)));
        window.push(answer(f.gifs.preview(&ASKED, "s1", true)));
        window.push(answer(f.gifs.collect(&ASKED, "s1")));
        window.push(answer(f.gifs.save_key(klipy(KEY))));
        write();
        window.push(answer(f.gifs.remove_key()));
        for got in &window {
            assert_eq!(shown_of(got, held, &f.studio.root), None, "{file}: {got}");
        }
        assert_eq!(window.iter().filter(|a| a.contains("fileError")).count(), 5);
    }
    assert_eq!(
        f.source.calls().len(),
        asked,
        "nothing asked with those files"
    );
}

/// D-2026-10-01-gif-sticker-search-10: a [`KlipyKey`]'s `Debug`, plain or
/// pretty, and its saved key's, show no character of it beyond what its
/// last 4 show the window: `KlipyKey(..)` for every key.
#[test]
fn a_key_prints_nothing_of_itself() {
    // Keys of characters that `KlipyKey(..)` and `SavedKey { .. }` do not
    // hold, long and short.
    for text in ["0123456789-_ABCDEFGHJ", "ZQ_-0987654321MNOPRTU", "1234"] {
        let key = klipy(text);
        assert_eq!(format!("{key:?}"), "KlipyKey(..)");
        let shown = key.last4().unwrap_or_default();
        let saved = SavedKey::new(key.clone(), "customer-1");
        for printed in [
            format!("{key:?}"),
            format!("{key:#?}"),
            format!("{saved:?}"),
            format!("{:?}", Some(&key)),
        ] {
            let rest = printed.replace(&shown, "");
            assert!(!rest.chars().any(|c| text.contains(c)), "{printed}");
        }
    }
}

/// D-2026-10-01-gif-sticker-search-4: the window names results by id and
/// only those of the pages of the last search can be previewed or collected
/// (`gifNotInResults`, asking nothing); a page answered in this session is
/// not asked again, and its search becomes the last one.
#[test]
fn only_items_of_the_last_search() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "cat",
        1,
        &[("a1", "Cat 1", gif(2)), ("a2", "Cat 2", gif(3))],
        true,
    );
    let source = with_results(
        source,
        GifKind::Gif,
        "cat",
        2,
        &[("a3", "Cat 3", gif(4))],
        false,
    );
    let source = with_results(
        source,
        GifKind::Gif,
        "dog",
        1,
        &[("b1", "Dog", gif(5))],
        false,
    );
    let f = fixture("last-search", source);
    f.gifs.save_key(klipy(KEY)).unwrap();

    let first = f.search("gif", "  cat ", 1);
    assert_eq!(
        (first.kind, first.text.as_str(), first.page),
        ("gif", "cat", 1)
    );
    assert!(first.has_next);
    let ids: Vec<&str> = first.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["a1", "a2"]);
    assert_eq!((first.items[0].width, first.items[0].height), (480, 240));
    assert_eq!(f.search("gif", "cat", 2).items[0].id, "a3");
    for id in ["a1", "a3"] {
        let url = f.gifs.preview(&ASKED, id, false).unwrap().unwrap();
        assert!(url.starts_with("data:image/gif;base64,"), "{url}");
    }

    f.search("gif", "dog", 1);
    let asked = f.source.calls().len();
    for id in ["a1", "a3", "zz"] {
        for error in [
            f.gifs.preview(&ASKED, id, false).unwrap_err(),
            f.gifs.collect(&ASKED, id).unwrap_err(),
        ] {
            assert_eq!(error.code(), "gifNotInResults", "{id}");
            assert_eq!(error.value("item"), Some(id));
        }
    }
    assert_eq!(f.source.calls().len(), asked, "nothing asked for them");
    assert!(f.gifs.preview(&ASKED, "b1", false).unwrap().is_some());
    assert_eq!(f.gifs.collect(&ASKED, "b1").unwrap().source.id, "b1");

    // Back to the first search: answered already, not asked again; its
    // results are the last search's again, and a preview read is kept.
    let pages = f.pages_asked();
    assert_eq!(f.search("gif", "cat", 1).items.len(), 2);
    assert_eq!(f.pages_asked(), pages);
    let asked = f.source.calls().len();
    assert!(f.gifs.preview(&ASKED, "a1", false).unwrap().is_some());
    assert_eq!(
        f.gifs.preview(&ASKED, "b1", false).unwrap_err().code(),
        "gifNotInResults"
    );
    assert_eq!(
        f.gifs.preview(&ASKED, "a3", false).unwrap_err().code(),
        "gifNotInResults"
    );
    assert_eq!(f.source.calls().len(), asked + 1, "only a1's preview");
    assert!(f.gifs.preview(&ASKED, "a1", false).unwrap().is_some());
    assert_eq!(f.source.calls().len(), asked + 1, "a1's preview kept");
}

/// A source whose answer to the search for `ca` comes only after the
/// search for `cat` was asked and answered: a slow answer to a search the
/// user typed past.
struct Late {
    inner: FakeGifSource,
    gifs: Arc<std::sync::OnceLock<std::sync::Weak<Gifs>>>,
}

impl GifSource for Late {
    fn provider(&self) -> &str {
        self.inner.provider()
    }

    fn page(&self, query: &GifQuery) -> bezel_core::Result<GifPage> {
        if query.text == "ca"
            && let Some(gifs) = self.gifs.get().and_then(std::sync::Weak::upgrade)
        {
            let cat = super::query("gif", "cat", 1, false, Language::English).unwrap();
            gifs.search(&ASKED, &cat).unwrap();
        }
        self.inner.page(query)
    }

    fn download(&self, rendition: &Rendition, limit: u64) -> bezel_core::Result<Vec<u8>> {
        self.inner.download(rendition, limit)
    }
}

/// The last search is the one asked last: an older search answered later
/// does not take its place.
#[test]
fn a_late_answer_does_not_replace_a_newer_search() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "ca",
        1,
        &[("c0", "Ca", gif(2))],
        false,
    );
    let source = with_results(
        source,
        GifKind::Gif,
        "cat",
        1,
        &[("c1", "Cat", gif(3))],
        false,
    );
    let f = fixture("late", FakeGifSource::new());
    let cell = Arc::new(std::sync::OnceLock::new());
    let late = Arc::new(Late {
        inner: source,
        gifs: Arc::clone(&cell),
    });
    let root = f.studio.root.join("late");
    let gifs = Arc::new(Gifs::new(
        KeyFile::new(root.join(KEY_FILE)),
        Provider {
            source: Arc::new(
                move |_: &UserAsked, _: &KlipyKey, _: &str| -> Arc<dyn GifSource> { late.clone() },
            ),
            customer_id: bezel_klipy::new_customer_id,
        },
        collection_in(root.join("collection"), |dir| DiskCollection::open(dir)),
        root.join("scratch"),
    ));
    cell.set(Arc::downgrade(&gifs)).unwrap();
    gifs.save_key(klipy(KEY)).unwrap();

    let ca = query("gif", "ca", 1, false, Language::English).unwrap();
    assert_eq!(
        gifs.search(&ASKED, &ca).unwrap().items[0].id,
        "c0",
        "still answered"
    );
    assert!(
        gifs.preview(&ASKED, "c1", false).unwrap().is_some(),
        "cat's results"
    );
    assert_eq!(
        gifs.preview(&ASKED, "c0", false).unwrap_err().code(),
        "gifNotInResults"
    );
}

/// The canvas pixel at `(x, y)` of a preview frame ([`crate::backend::frame_bytes`]).
fn pixel(frame: &[u8], x: u32, y: u32) -> Rgba {
    let width = u32::from_le_bytes(frame[0..4].try_into().unwrap());
    let at = 12 + ((y * width + x) * 4) as usize;
    Rgba {
        r: frame[at],
        g: frame[at + 1],
        b: frame[at + 2],
        a: frame[at + 3],
    }
}

/// A theme of the 8.8" in `orientation` on a blue background with the
/// image `asset` in a 400-pixel square at its center, where the UI puts an
/// image; and that square's top-left corner.
fn centered_image(name: &str, orientation: Orientation, asset: &str) -> (Theme, (u32, u32)) {
    let mut theme = Theme::blank(name, Size::new(480, 1920), orientation);
    theme.background = Background::Color(BLUE);
    let canvas = theme.canvas;
    let (left, top) = ((canvas.width - 400) / 2, (canvas.height - 400) / 2);
    theme.elements.push(Element {
        id: ElementId(1),
        name: asset.to_string(),
        frame: BoxF::new(left as f32, top as f32, 400.0, 400.0),
        opacity: 1.0,
        visible: true,
        locked: false,
        kind: ElementKind::Image {
            asset: AssetRef(asset.to_string()),
            fit: Fit::Fill,
        },
    });
    (theme, (left, top))
}

/// D-2026-10-01-gif-sticker-search-5: a sticker added as an image keeps its
/// transparency (its bytes as they are, through `add_image_bytes`: no
/// poster, no video); the real renderer composites it over the theme's
/// background, in a vertical and a horizontal theme alike.
#[test]
fn sticker_alpha_shows_the_background() {
    let star = sticker();
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Sticker,
        "star",
        1,
        &[("s1", "Party Star!", star.clone())],
        false,
    );
    let f = fixture("alpha", source);
    f.gifs.save_key(klipy(KEY)).unwrap();
    let collected = f.collect_first("sticker", "star");
    assert_eq!(collected.kind, "sticker");
    for orientation in [Orientation::Portrait, Orientation::Landscape] {
        f.backend()
            .new_theme(None, "Sticker", Some(orientation))
            .unwrap();
        let added = f
            .gifs
            .use_in_theme(f.backend(), &collected.id, Target::Image)
            .unwrap();
        assert_eq!(
            (
                added.reference.as_str(),
                added.kind,
                added.poster.as_deref()
            ),
            ("assets/party-star.gif", "image", None),
            "{orientation:?}"
        );
        assert_eq!(added.bytes, star.len() as u64);
        let (theme, (left, top)) = centered_image("Sticker", orientation, &added.reference);
        let frame = f
            .backend()
            .render(
                &ThemeDto::from(&theme),
                TIME,
                Instant::now(),
                Playback::Reduced,
            )
            .unwrap();
        let middle = top + 200;
        assert_eq!(pixel(&frame, left + 100, middle), BLUE, "{orientation:?}");
        assert_eq!(pixel(&frame, left + 300, middle), RED, "{orientation:?}");
        assert_eq!(pixel(&frame, 10, 10), BLUE);
    }
    assert!(f.posters.lock().unwrap().is_empty(), "no poster taken");
}

/// D-2026-10-01-gif-sticker-search-5: an animated GIF used as the
/// background goes through today's "Add video…" path, from a copy named
/// after it: a video background with its poster (its first picture),
/// exactly what adding the same file gives today.
#[test]
fn animated_gif_background_as_today() {
    let waves = gif(3);
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "waves",
        1,
        &[("w1", "Ocean Waves", waves.clone())],
        false,
    );
    let f = fixture("background", source);
    f.gifs.save_key(klipy(KEY)).unwrap();
    let collected = f.collect_first("gif", "waves");
    let added = f
        .gifs
        .use_in_theme(f.backend(), &collected.id, Target::Background)
        .unwrap();

    let today = fixture("background-today", FakeGifSource::new());
    let local = today.studio.root.join("local").join("ocean-waves.gif");
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::fs::write(&local, &waves).unwrap();
    let expected = today.backend().add_media(&local).unwrap();
    assert_eq!(json(&added), json(&expected));
    assert_eq!(
        (
            added.kind,
            added.reference.as_str(),
            added.poster.as_deref()
        ),
        (
            "video",
            "assets/ocean-waves.gif",
            Some("assets/ocean-waves-poster.png")
        )
    );

    let posters = f.posters.lock().unwrap().clone();
    let copy = f.scratch().join("Ocean-Waves.gif");
    assert_eq!(posters.len(), 1);
    assert_eq!(posters[0].0, MediaLocation(copy.display().to_string()));
    assert_eq!(posters[0].1.at, Duration::ZERO, "its first picture");
    assert!(!copy.exists(), "the copy is removed once added");
    let studio = f.backend().studio();
    assert_eq!(studio.assets()[&AssetRef(added.reference.clone())], waves);
}

/// D-2026-10-01-gif-sticker-search-5: the delete dialog names the user's
/// themes and says whether the open one holds the item's bytes; only a
/// confirmed delete removes it, and themes keep their own copy.
#[test]
fn delete_names_themes_using_it() {
    let party = gif(2);
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "party",
        1,
        &[("p1", "Party", party.clone()), ("p2", "Other", gif(3))],
        false,
    );
    let f = fixture("delete", source);
    f.gifs.save_key(klipy(KEY)).unwrap();
    f.search("gif", "party", 1);
    let used = f.gifs.collect(&ASKED, "p1").unwrap();
    let unused = f.gifs.collect(&ASKED, "p2").unwrap();
    let backend = f.backend();

    backend
        .new_theme(None, "Party Night", Some(Orientation::Landscape))
        .unwrap();
    let added = f
        .gifs
        .use_in_theme(backend, &used.id, Target::Image)
        .unwrap();
    let (theme, _) = centered_image("Party Night", Orientation::Landscape, &added.reference);
    let saved = backend.save(&ThemeDto::from(&theme), None).unwrap();
    backend.new_theme(None, "Plain", None).unwrap();
    backend.save(&backend.session().theme, None).unwrap();

    let users = f.gifs.users(backend, &used.id).unwrap();
    assert_eq!(
        users,
        CollectedUsersDto {
            themes: vec!["Party Night".into()],
            open_theme: false
        }
    );
    f.gifs
        .use_in_theme(backend, &used.id, Target::Background)
        .unwrap();
    assert!(f.gifs.users(backend, &used.id).unwrap().open_theme);
    let none = f.gifs.users(backend, &unused.id).unwrap();
    assert_eq!((none.themes.len(), none.open_theme), (0, false));

    let refused = f.gifs.delete(&used.id, Confirm::No).unwrap_err();
    assert_eq!(refused.code(), "notConfirmed");
    assert_eq!(f.gifs.list(false).unwrap().len(), 2);

    f.gifs.delete(&used.id, Confirm::Yes).unwrap();
    let left: Vec<String> = f
        .gifs
        .list(false)
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(left, std::slice::from_ref(&unused.id));
    let location = ThemeLocation(saved.location);
    let (_, assets) = backend.store.load(&location).unwrap();
    assert!(assets.values().any(|bytes| *bytes == party), "its own copy");

    for error in [
        f.gifs.delete(&used.id, Confirm::Yes).unwrap_err(),
        f.gifs.users(backend, &used.id).unwrap_err(),
        f.gifs
            .use_in_theme(backend, &used.id, Target::Image)
            .unwrap_err(),
        f.gifs.rename(&used.id, "Back").unwrap_err(),
        f.gifs.rename("not-a-sha256", "Back").unwrap_err(),
    ] {
        assert_eq!(error.code(), "notInCollection");
    }
}

/// The collection lists the last added first, with moving or still
/// previews; renaming trims and refuses an empty name.
#[test]
fn the_collection_lists_renames_and_shows_stills() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "",
        1,
        &[("t1", "Trending one", gif(2)), ("t2", "  ", gif(3))],
        false,
    );
    let f = fixture("list", source);
    f.gifs.save_key(klipy(KEY)).unwrap();
    let trending = f.search("gif", "", 1);
    assert_eq!(trending.text, "");
    let still = f.gifs.preview(&ASKED, "t1", true).unwrap().unwrap();
    assert!(still.starts_with("data:image/jpeg;base64,"), "{still}");
    let first = f.gifs.collect(&ASKED, "t1").unwrap();
    let second = f.gifs.collect(&ASKED, "t2").unwrap();
    assert_eq!(second.name, "t2", "a blank title names it by id");
    assert_eq!(
        (first.width, first.height, first.bytes),
        (2, 2, gif(2).len() as u64)
    );
    assert!(first.added_at > 1_767_225_600);
    assert_eq!(first.source.provider, "fake");
    assert_eq!(
        first.source.url.as_deref(),
        Some("https://klipy.com/gifs/t1")
    );
    assert!(first.preview.unwrap().starts_with("data:image/gif;base64,"));
    // The same bytes again: the item already there.
    assert_eq!(f.gifs.collect(&ASKED, "t1").unwrap().id, first.id);

    let listed = f.gifs.list(true).unwrap();
    let names: Vec<&str> = listed.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["t2", "Trending one"], "the last added first");
    let png = listed[0].preview.as_deref().unwrap();
    assert!(png.starts_with("data:image/png;base64,"), "{png}");

    let renamed = f.gifs.rename(&first.id, "  Dance  ").unwrap();
    assert_eq!(renamed.name, "Dance");
    assert_eq!(
        f.gifs.rename(&first.id, " ").unwrap_err().code(),
        "invalidInput"
    );
    assert_eq!(f.gifs.list(false).unwrap()[1].name, "Dance");
}

/// D-2026-10-01-gif-sticker-search-7: a still preview's `data:` URL takes
/// the media type of its bytes: a GIF's JPEG, a sticker's PNG.
#[test]
fn stills_take_the_media_type_of_their_bytes() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "wave",
        1,
        &[("g1", "Wave", gif(2))],
        false,
    );
    let source = with_results(
        source,
        GifKind::Sticker,
        "wave",
        1,
        &[("s1", "Hand", gif(3))],
        false,
    );
    let f = fixture("stills", source);
    f.gifs.save_key(klipy(KEY)).unwrap();
    for (kind, id, still) in [
        ("gif", "g1", "data:image/jpeg;base64,"),
        ("sticker", "s1", "data:image/png;base64,"),
    ] {
        f.search(kind, "wave", 1);
        let url = f.gifs.preview(&ASKED, id, true).unwrap().unwrap();
        assert!(url.starts_with(still), "{url}");
        let moving = f.gifs.preview(&ASKED, id, false).unwrap().unwrap();
        assert!(moving.starts_with("data:image/gif;base64,"), "{moving}");
    }
}

/// The window's query and target are checked; a provider's failure is its
/// code, and a download that is not a GIF is refused and keeps nothing.
#[test]
fn queries_failures_and_files_that_are_not_gifs() {
    let english = Language::English;
    assert_eq!(
        query("webp", "x", 1, false, english).unwrap_err().code(),
        "invalidInput"
    );
    assert_eq!(
        query("gif", "x", 0, false, english).unwrap_err().code(),
        "invalidInput"
    );
    let shown = query("sticker", " x ", 2, true, Language::PortugueseBr).unwrap();
    assert_eq!(
        (shown.kind, shown.text.as_str(), shown.page, shown.explicit),
        (GifKind::Sticker, "x", 2, Explicit::Shown)
    );
    assert_eq!(Target::parse("video").unwrap_err().code(), "invalidInput");
    assert_eq!(Target::parse("background").unwrap(), Target::Background);

    let limited = fixture(
        "limited",
        FakeGifSource::new().failing(BezelError::Service(ServiceFailure::RateLimited)),
    );
    limited.gifs.save_key(klipy(KEY)).unwrap();
    let any = query("gif", "x", 1, false, english).unwrap();
    assert_eq!(
        limited.gifs.search(&ASKED, &any).unwrap_err().code(),
        "klipyRateLimited"
    );

    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "html",
        1,
        &[("h1", "Not a GIF", b"<html>not a gif</html>".to_vec())],
        false,
    );
    let f = fixture("not-gif", source);
    f.gifs.save_key(klipy(KEY)).unwrap();
    f.search("gif", "html", 1);
    assert_eq!(
        f.gifs.collect(&ASKED, "h1").unwrap_err().code(),
        "invalidInput"
    );
    assert!(f.gifs.list(false).unwrap().is_empty());
    assert_eq!(
        f.gifs.preview(&ASKED, "h1", false).unwrap_err().code(),
        "invalidInput"
    );

    std::fs::write(f.key_path(), b"{\"key\": \"bad key\"}").unwrap();
    let unreadable = f.gifs.key_status().unwrap_err();
    assert_eq!(unreadable.code(), "fileError");
    assert!(!unreadable.to_string().contains("bad key"), "{unreadable}");
}

/// Review W2: a collection whose folder cannot be used is an error naming
/// the folder (the Collection panel shows it), never a collection kept in
/// memory that vanishes on quit: adding is refused before anything is
/// downloaded, every use says why, and the collection opens once its
/// folder can be used, keeping what is added from then on.
#[test]
fn a_collection_folder_that_cannot_be_used_is_said_not_lost() {
    let source = with_results(
        FakeGifSource::new(),
        GifKind::Gif,
        "cat",
        1,
        &[("a1", "Cat", gif(2))],
        false,
    );
    let f = fixture("unusable", source);
    // A file where the collection's folder goes: it cannot be made.
    let data = f.studio.root.join("unusable-data");
    let folder = collection_dir(&data);
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(&folder, b"not a folder").unwrap();
    let gifs = gifs_in(&f.studio.root, &data, &f.source, &f.made);
    gifs.save_key(klipy(KEY)).unwrap();

    let unusable = |error: UiError| {
        assert_eq!(error.code(), "collectionUnavailable", "{error}");
        let text = error.to_string();
        assert!(text.contains(&folder.display().to_string()), "{text}");
    };
    unusable(gifs.list(false).unwrap_err());
    // The search is not the collection's: it works.
    let cat = query("gif", "cat", 1, false, Language::English).unwrap();
    assert_eq!(gifs.search(&ASKED, &cat).unwrap().items.len(), 1);
    unusable(gifs.collect(&ASKED, "a1").unwrap_err());
    assert!(
        f.source.downloads().is_empty(),
        "refused before anything is downloaded"
    );
    let id = content_id(&gif(2)).to_string();
    unusable(gifs.rename(&id, "Cat").unwrap_err());
    unusable(gifs.delete(&id, Confirm::Yes).unwrap_err());
    unusable(gifs.users(f.backend(), &id).unwrap_err());
    unusable(
        gifs.use_in_theme(f.backend(), &id, Target::Image)
            .unwrap_err(),
    );

    // Once the folder can be used, the collection opens and keeps the item.
    std::fs::remove_file(&folder).unwrap();
    assert!(gifs.list(false).unwrap().is_empty());
    let kept = gifs.collect(&ASKED, "a1").unwrap();
    assert_eq!(kept.id, id);
    let on_disk = DiskCollection::open(&folder).unwrap().load().unwrap();
    assert_eq!(
        on_disk.items().len(),
        1,
        "kept in the folder, not in memory"
    );
}

/// The HTTP client prints request paths, which hold the key, only at the
/// `log` crate's trace level: the studio compiles that level out.
#[test]
fn the_http_client_cannot_log_request_paths() {
    assert!(log::STATIC_MAX_LEVEL <= log::LevelFilter::Debug);
    assert!(!log::log_enabled!(target: "ureq::util", log::Level::Trace));
    assert!(!log::log_enabled!(target: "ureq_proto::util", log::Level::Trace));
}

#[test]
fn file_stems_are_never_paths() {
    assert_eq!(file_stem("Party Star!"), "Party-Star");
    assert_eq!(file_stem("../../etc/passwd"), "etc-passwd");
    assert_eq!(file_stem("C:\\x:y*z?"), "C-x-y-z");
    assert_eq!(file_stem(" / "), "gif");
    assert_eq!(file_stem(&"a".repeat(100)).len(), 64);
}
