//! The disk collection in temporary folders. Paths are only joined, never
//! spelled, so the tests run the same on Windows.

use std::fs;
use std::io::Write;
use std::path::Path;

use bezel_core::BezelError;
use bezel_core::domain::archive::ContentId;
use bezel_core::domain::gifs::{CollectedGif, Collection, GifKind, GifOrigin};
use bezel_core::ports::GifCollection;

use super::*;
use crate::archive::content_id;

/// A one-pixel GIF89a.
const PIXEL: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
    0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
];

/// [`PIXEL`] in another colour: another content.
fn red_pixel() -> Vec<u8> {
    let mut gif = PIXEL.to_vec();
    gif[13..16].copy_from_slice(&[0xFF, 0x00, 0x00]);
    gif
}

fn item(content: &ContentId, name: &str, kind: GifKind) -> CollectedGif {
    CollectedGif {
        content: content.clone(),
        name: name.to_string(),
        kind,
        width: 1,
        height: 1,
        bytes: PIXEL.len() as u64,
        added_at: 1_790_000_000,
        origin: GifOrigin {
            provider: "fake".into(),
            id: format!("id-{name}"),
            page_url: Some("https://example.invalid/gif".into()),
        },
    }
}

/// The names of the files (not folders) in `dir`, sorted.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Every file and folder under `dir`, as `a/b` paths relative to it, sorted.
fn tree(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().unwrap().is_dir() {
            found.extend(
                tree(&entry.path())
                    .into_iter()
                    .map(|p| format!("{name}/{p}")),
            );
        }
        found.push(name);
    }
    found.sort();
    found
}

/// Whether `a` and `b` are links to one file: a byte appended through `a`
/// shows through `b`. The byte is taken away again.
fn same_file(a: &Path, b: &Path) -> bool {
    let before = fs::read(a).unwrap();
    let mut file = fs::OpenOptions::new().append(true).open(a).unwrap();
    file.write_all(b"!").unwrap();
    drop(file);
    let shared = fs::read(b).unwrap().len() == before.len() + 1;
    fs::write(a, &before).unwrap();
    shared
}

/// Saves of the index in a collection's folder fail while this is held, and
/// the index is left as it is: on Unix the folder is read-only, so no
/// temporary file can be made next to the index; on Windows the index is held
/// open without delete sharing, so nothing can replace it.
struct SavesBlocked {
    #[cfg(unix)]
    root: std::path::PathBuf,
    #[cfg(windows)]
    _held: fs::File,
}

impl SavesBlocked {
    /// `None` when this process writes through a read-only folder anyway
    /// (root on Unix): the failure cannot be staged.
    #[cfg(unix)]
    fn new(root: &Path) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o555)).unwrap();
        let blocked = Self {
            root: root.to_path_buf(),
        };
        let probe = root.join("probe");
        if fs::File::create(&probe).is_ok() {
            fs::remove_file(&probe).unwrap();
            return None;
        }
        Some(blocked)
    }

    #[cfg(windows)]
    fn new(root: &Path) -> Option<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(root.join("collection.json"))
            .unwrap();
        Some(Self { _held: held })
    }
}

#[cfg(unix)]
impl Drop for SavesBlocked {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.root, fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn saves_atomically_by_content() {
    let data = tempfile::tempdir().unwrap();
    let root = collection_dir(data.path());
    assert_eq!(root, data.path().join("bezel").join("collection"));
    let mut disk = DiskCollection::open(&root).unwrap();
    assert_eq!(disk.root(), root);
    // Nothing saved yet: an empty collection, and no file for it.
    assert_eq!(disk.load().unwrap(), Collection::default());
    assert_eq!(tree(&root), ["files", "previews"]);

    // One file per content, named by its SHA-256.
    let id = disk.keep(PIXEL).unwrap();
    assert_eq!(id, content_id(PIXEL));
    let kept = root.join("files").join(format!("{id}.gif"));
    let link = data.path().join("link.gif");
    fs::hard_link(&kept, &link).unwrap();
    assert_eq!(disk.keep(PIXEL).unwrap(), id);
    assert!(same_file(&kept, &link), "kept once: not written again");
    let red = red_pixel();
    let red_id = disk.keep(&red).unwrap();
    assert_ne!(red_id, id);
    assert_eq!(disk.read(&id).unwrap().as_deref(), Some(PIXEL));
    assert_eq!(disk.read(&red_id).unwrap(), Some(red.clone()));

    // Previews next to them, a later one replacing the first.
    disk.keep_preview(&id, b"GIF89a first").unwrap();
    disk.keep_preview(&id, b"GIF89a small").unwrap();
    assert_eq!(
        disk.read_preview(&id).unwrap(),
        Some(b"GIF89a small".to_vec())
    );
    assert_eq!(disk.read_preview(&red_id).unwrap(), None);

    // Names live only in the index: whatever the user typed, the folder
    // holds content ids, the index and nothing temporary.
    let index = Collection::new(vec![
        item(&id, "..\\../escape: CON *?", GifKind::Gif),
        item(&red_id, "Party", GifKind::Sticker),
    ]);
    disk.save(&index).unwrap();
    let mut expected = vec![
        "collection.json".to_string(),
        "files".into(),
        format!("files/{id}.gif"),
        format!("files/{red_id}.gif"),
        "previews".into(),
        format!("previews/{id}.gif"),
    ];
    expected.sort();
    assert_eq!(tree(&root), expected);
    let text = fs::read_to_string(root.join("collection.json")).unwrap();
    assert!(text.contains(r#""schema": 1"#), "{text}");
    let mut reopened = DiskCollection::open(collection_dir(data.path())).unwrap();
    assert_eq!(reopened, disk);
    assert_eq!(reopened.load().unwrap(), index);

    // A save replaces the index whole, never rewriting it in place: a link
    // to the old index still reads it as it was.
    let old = data.path().join("old.json");
    fs::hard_link(root.join("collection.json"), &old).unwrap();
    let mut renamed = index.clone();
    renamed.rename(&id, "Cat").unwrap();
    reopened.save(&renamed).unwrap();
    assert_eq!(fs::read_to_string(&old).unwrap(), text);
    assert_eq!(disk.load().unwrap(), renamed);

    // A save that fails leaves the index whole and no temporary file.
    if let Some(blocked) = SavesBlocked::new(&root) {
        let err = disk.save(&Collection::default()).unwrap_err();
        drop(blocked);
        let index_path = root.join("collection.json");
        assert!(
            err.to_string().contains(&index_path.display().to_string()),
            "{err}"
        );
        assert_eq!(disk.load().unwrap(), renamed);
        assert_eq!(file_names(&root), ["collection.json"]);
    }

    // Discarding takes the GIF and its preview; again is not an error; the
    // other GIF stays.
    disk.discard(&id).unwrap();
    disk.discard(&id).unwrap();
    assert_eq!(disk.read(&id).unwrap(), None);
    assert_eq!(disk.read_preview(&id).unwrap(), None);
    assert_eq!(file_names(&root.join("files")), [format!("{red_id}.gif")]);
    assert_eq!(file_names(&root.join("previews")), Vec::<String>::new());
    assert_eq!(disk.read(&red_id).unwrap(), Some(red));
}

#[test]
fn unreadable_index_is_an_error() {
    let data = tempfile::tempdir().unwrap();
    let root = collection_dir(data.path());
    let mut disk = DiskCollection::open(&root).unwrap();
    let path = root.join("collection.json");
    let sha = content_id(PIXEL).to_string();
    let index = |content: &str, kind: &str, name: &str| {
        format!(
            r#"{{"schema": 1, "items": [{{"content": "{content}", "name": "{name}",
            "kind": "{kind}", "width": 2, "height": 3, "bytes": 43, "addedAt": 7,
            "source": {{"provider": "fake", "id": "9"}}}}]}}"#
        )
    };
    let cases: Vec<(String, &str)> = vec![
        (String::new(), "EOF"),
        (r#"{"schema": 1, "items": ["#.into(), "EOF"),
        (r#""text""#.into(), "invalid type"),
        (r#"{"items": []}"#.into(), "no schema number"),
        (
            r#"{"schema": 2, "items": "renamed"}"#.into(),
            "newer Bezel (schema 2",
        ),
        (r#"{"schema": 0}"#.into(), "unknown schema 0"),
        (
            index("../../escape", "gif", "x"),
            "\"../../escape\" is not a SHA-256",
        ),
        (index(&sha, "webp", "x"), "unknown kind \"webp\""),
        (index(&sha, "gif", "   "), "it has no name"),
    ];
    for (text, why) in cases {
        fs::write(&path, &text).unwrap();
        let err = disk.load().unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, BezelError::InvalidInput(_)), "{message}");
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains(why), "{why:?} in {message}");
        assert_eq!(fs::read_to_string(&path).unwrap(), text, "never reset");
    }

    // An index that cannot be read at all is an error naming it too.
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let err = disk.load().unwrap_err().to_string();
    assert!(err.contains("cannot read"), "{err}");
    assert!(err.contains(&path.display().to_string()), "{err}");
    fs::remove_dir(&path).unwrap();

    // A missing index is an empty collection; the good shape loads.
    assert_eq!(disk.load().unwrap(), Collection::default());
    fs::write(&path, index(&sha.to_uppercase(), "sticker", " Party ")).unwrap();
    let loaded = disk.load().unwrap();
    let mut expected = item(&content_id(PIXEL), "Party", GifKind::Sticker);
    (expected.width, expected.height, expected.added_at) = (2, 3, 7);
    expected.origin.id = "9".into();
    expected.origin.page_url = None;
    assert_eq!(loaded.items(), [expected]);

    // A collection cannot live inside a file.
    let err = DiskCollection::open(&path).unwrap_err();
    assert!(err.to_string().contains("cannot create"), "{err}");
}

#[test]
fn a_damaged_gif_is_an_error_and_keeping_it_again_repairs_it() {
    let data = tempfile::tempdir().unwrap();
    let mut disk = DiskCollection::open(data.path()).unwrap();
    let id = disk.keep(PIXEL).unwrap();
    let file = data.path().join("files").join(format!("{id}.gif"));
    fs::write(&file, b"bit rot").unwrap();
    let err = disk.read(&id).unwrap_err().to_string();
    assert!(err.contains("damaged"), "{err}");
    assert!(err.contains(&file.display().to_string()), "{err}");
    assert_eq!(disk.keep(PIXEL).unwrap(), id);
    assert_eq!(disk.read(&id).unwrap().as_deref(), Some(PIXEL));
    // Never kept: nothing to read, nothing to discard.
    let other = content_id(b"other");
    assert_eq!(disk.read(&other).unwrap(), None);
    disk.discard(&other).unwrap();
}
