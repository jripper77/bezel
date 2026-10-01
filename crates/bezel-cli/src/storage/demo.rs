//! What `--fake` stores: the simulated 8.8" with a few files Bezel sent and
//! the user's real memory card, filled by the vendor app (fresh in every
//! process), and Bezel's catalog of those files in memory, so that every
//! `bezel --fake storage` command has something to show and to do.

use bezel_core::domain::archive::{ArchiveEntry, Catalog, EntryState, ScreenKey};
use bezel_core::domain::device::ModelId;
use bezel_core::domain::storage::{Medium, RemotePath};
use bezel_core::ports::ArchiveStore;
use bezel_devices::fake::FakeStorage;
use bezel_media::archive::MemoryArchive;

/// Usable space of the simulated memory card: 8 GiB.
pub const CARD_BYTES: u64 = 8 << 30;

/// The user's card on the 8.8" (`bezel storage ls --json`, 2026-09-30):
/// the vendor app's twelve videos on `sd/video`, exact bytes. Each
/// re-conversion by the vendor app left a file of another size.
pub const VENDOR_CARD: &[(&str, u64)] = &[
    ("demon_open.mp4.mp4.mp4", 25_483_784),
    ("demon.mp4.mp4.mp4", 13_237_564),
    ("demon.mp401115025.mp4", 13_257_991),
    ("8.8APEX_2.mp4", 2_259_535),
    ("demon_open.mp4.mp4", 25_800_984),
    ("AMD.mp4", 4_079_432),
    ("NVI.mp427034822.mp4", 5_352_433),
    ("NVI.mp4", 5_680_675),
    ("Rani.mp4", 6_007_182),
    ("m04.mp4", 876_578),
    ("Rani.mp417075004.mp4", 5_646_986),
    ("m04.mp424045157.mp4", 841_053),
];

/// The model the simulated screen is.
const MODEL: ModelId = ModelId("turing-8.8");

/// How a file Bezel sent is cataloged, and whether it is on the screen.
#[derive(Clone, Copy)]
struct Sent {
    path: &'static str,
    /// Bytes Bezel sent (and keeps as the local copy).
    bytes: usize,
    state: EntryState,
    /// Bytes on the screen: `bytes`, fewer for an interrupted upload, 0 for
    /// a file gone from it.
    on_screen: usize,
}

/// What Bezel sent the simulated screen, oldest first: three files it
/// stores, an upload that did not finish (a pending entry over a partial
/// file) and a card file that is gone (restorable).
const SENT: [Sent; 5] = [
    Sent {
        path: "internal/image/bezel_demo.png",
        bytes: 48 * 1024,
        state: EntryState::Stored,
        on_screen: 48 * 1024,
    },
    Sent {
        path: "internal/video/bezel_demo.mp4",
        bytes: 2_400 * 1024,
        state: EntryState::Stored,
        on_screen: 2_400 * 1024,
    },
    Sent {
        path: "sd/video/bezel_loop.mp4",
        bytes: 750 * 1024,
        state: EntryState::Stored,
        on_screen: 750 * 1024,
    },
    Sent {
        path: "sd/video/bezel_intro.mp4",
        bytes: 1_200 * 1024,
        state: EntryState::Missing,
        on_screen: 0,
    },
    Sent {
        path: "internal/video/bezel_cut.mp4",
        bytes: 1_800 * 1024,
        state: EntryState::Pending,
        on_screen: 320 * 1024,
    },
];

/// When the first of [`SENT`] was sent (2026-09-25, seconds since the Unix
/// epoch); the others follow an hour apart.
const FIRST_SENT: u64 = 1_790_330_400;

/// The bytes of a simulated file: `len` copies of one byte.
fn bytes(len: usize) -> Vec<u8> {
    vec![0x5a; len]
}

fn path(text: &str) -> Option<RemotePath> {
    RemotePath::parse(text).ok()
}

/// The simulated screen's storage: 1 GiB of internal flash, an 8 GiB card
/// with the vendor app's videos, and what Bezel sent it.
pub fn storage() -> FakeStorage {
    let mut storage = FakeStorage::default().with_card(CARD_BYTES);
    for (name, size) in VENDOR_CARD {
        let len = usize::try_from(*size).unwrap_or(0);
        if let Some(at) = path(&format!("sd/video/{name}")) {
            storage = storage.with_file(at, bytes(len));
        }
    }
    for sent in SENT.iter().filter(|s| s.on_screen > 0) {
        if let Some(at) = path(sent.path) {
            storage = storage.with_file(at, bytes(sent.on_screen));
        }
    }
    storage
}

/// Bezel's catalog of [`storage`]'s screen, with the local copies, in
/// memory.
pub fn archive() -> MemoryArchive {
    let mut archive = MemoryArchive::new();
    let mut catalog = Catalog::default();
    let key = ScreenKey::new(MODEL);
    for (n, sent) in (0_u64..).zip(SENT) {
        let (Some(at), Ok(content)) = (path(sent.path), archive.keep(&bytes(sent.bytes))) else {
            continue;
        };
        let card = (at.location.medium == Medium::Card).then_some(CARD_BYTES);
        let sent_at = FIRST_SENT + n * 3600;
        let size = sent.bytes as u64;
        let mut entry = ArchiveEntry::pending(at, card, size, content.clone(), sent_at);
        entry.state = sent.state;
        catalog.copies.insert(content);
        catalog.screen_mut(&key).record(entry);
    }
    let _ = archive.save(&catalog);
    archive
}
