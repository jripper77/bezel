//! The user's KLIPY key (D-2026-10-01-gif-sticker-search-3, -10).
//!
//! - [`KlipyKey`] is the key from the command boundary on: the
//!   `save_klipy_key` command's argument is one, checked as the window's
//!   invocation is read, and so is the key the file holds. It has no
//!   `Display`, no `Serialize` and no conversion to a string, and its
//!   `Debug` is the same for every key, so printing or logging it does not
//!   compile, or shows nothing of it. Its text is read by one crate-private
//!   accessor, [`KlipyKey::expose_secret`], in two places only, which the
//!   source guard (`tests::nothing_in_the_app_forges_an_invocation`) checks
//!   in production code: `key.expose_secret()` as a direct argument of
//!   `KlipyClient::new` in the source factory (`klipy_source` in `lib.rs`),
//!   and as the argument of `serialize_str` in [`write_key`], the key
//!   file's serializer, which serde calls for the file's `key` and no code
//!   names. It refuses the accessor anywhere else: bound to a variable,
//!   inside any macro call's tokens, through a path, in another function or
//!   file. Neither function calls a macro: the guard refuses any macro in a
//!   function that reads the key's text.
//! - No `String` of the key is made on its way in either: the window's
//!   argument and the file's `key` are read where serde holds them
//!   ([`KeyText`]) straight into [`KlipyKey::parse`]. The file's JSON
//!   ([`KeyJson`]) holds a [`KlipyKey`], so an error about the file has no
//!   key text to quote (the critic of round 2, iter 4).
//! - The key file is `<config>/klipy.json`, next to `settings.json` and not
//!   inside it, holding the key and the customer id made for it. It is
//!   replaced atomically (written whole to a temporary file next to it, then
//!   renamed over it) and, on Unix, readable only by the user (0600); on
//!   Windows the profile's ACL keeps it. Removing the key deletes the file.
//!   No error text it gives quotes the key: a file that does not hold a
//!   usable key (cut short, not JSON, a key that is not a string or not a
//!   key, a customer id that is not one) is the same fixed `fileError`.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tauri::Runtime;
use tauri::ipc::{CommandArg, CommandItem, InvokeError};

use crate::messages::{ErrorCode, UiError, UiResult};

/// The key file's name, in the app's config folder.
pub const KEY_FILE: &str = "klipy.json";

/// Most characters of a key.
const MAX_KEY_CHARS: usize = 128;

/// Characters of a key of which the window may see the last 4: a shorter
/// key would show most of itself.
const SHOWN_FROM_CHARS: usize = 9;

/// Why a key file cannot be used, whatever it holds: fixed text.
const NOT_A_KEY_FILE: &str = "it does not hold a KLIPY key";

/// Whether `text` can be a KLIPY key or a customer id: 1 to 128 letters,
/// digits, `_` and `-` (each is a segment of the API's path).
fn is_valid_key(text: &str) -> bool {
    (1..=MAX_KEY_CHARS).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A KLIPY key: 1 to 128 letters, digits, `_` and `-`. Nothing shows it:
/// no `Display`, no `Serialize`, no `Deref`, `AsRef<str>` or conversion to
/// a `String`, and a `Debug` that prints only `KlipyKey(..)`.
#[derive(Clone, PartialEq, Eq)]
pub struct KlipyKey(String);

impl KlipyKey {
    /// `text` as a key; `invalidInput`, which never quotes it, when it
    /// cannot be one.
    pub fn parse(text: &str) -> UiResult<Self> {
        if is_valid_key(text) {
            Ok(Self(text.to_string()))
        } else {
            Err(invalid_key())
        }
    }

    /// The key's text: a direct argument of `KlipyClient::new` in the
    /// source factory (`klipy_source`), and of `serialize_str` in the key
    /// file's serializer ([`write_key`]), only. The source guard refuses
    /// this accessor anywhere else in production code, a macro call's
    /// tokens included.
    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }

    /// The last 4 characters the window may show: only of a key of more
    /// than 8 characters.
    pub fn last4(&self) -> Option<String> {
        let shown = self.0.len().checked_sub(4)?;
        (self.0.len() >= SHOWN_FROM_CHARS).then(|| self.0[shown..].to_string())
    }
}

/// The same for every key: nothing of it.
impl fmt::Debug for KlipyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KlipyKey(..)")
    }
}

/// The window's key, as a command's argument: the same JSON string as
/// before, read where the invocation holds it ([`KeyText`]) and checked as
/// the invocation is read. Not through `Deserialize`, which Tauri turns
/// into a plain-text rejection: a key that cannot be one rejects with
/// `invalidInput`, as the window expects (`keyFailure`).
impl<'de, R: Runtime> CommandArg<'de, R> for KlipyKey {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let (name, arg) = (command.name, command.key);
        let key = command
            .deserialize_str(KeyText)
            .map_err(|e| tauri::Error::InvalidArgs(name, arg, e))?;
        Ok(key?)
    }
}

/// Reads a key's text where serde holds it into [`KlipyKey::parse`]'s
/// answer, the window's argument and the key file's `key` alike: no
/// `String` of the key is made on the way.
struct KeyText;

impl<'de> Visitor<'de> for KeyText {
    type Value = UiResult<KlipyKey>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a KLIPY key")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        Ok(KlipyKey::parse(text))
    }
}

/// A saved key and the customer id made for it.
#[derive(Clone, PartialEq, Eq)]
pub struct SavedKey {
    key: KlipyKey,
    customer_id: String,
}

impl SavedKey {
    /// `key` and its `customer_id`.
    pub fn new(key: KlipyKey, customer_id: &str) -> Self {
        Self {
            key,
            customer_id: customer_id.to_string(),
        }
    }

    /// The key.
    pub fn key(&self) -> &KlipyKey {
        &self.key
    }

    /// The customer id made for the key.
    pub fn customer_id(&self) -> &str {
        &self.customer_id
    }

    /// The last 4 characters of the key the window may show.
    pub fn last4(&self) -> Option<String> {
        self.key.last4()
    }
}

/// Neither the key nor the customer id.
impl fmt::Debug for SavedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedKey").finish_non_exhaustive()
    }
}

/// The key file's JSON: `{"key": …, "customerId": …}`. Its key is a
/// [`KlipyKey`], read by [`read_key`] and written by [`write_key`]: no
/// `String` of it is made, so no error about the file can quote it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyJson {
    #[serde(serialize_with = "write_key", deserialize_with = "read_key")]
    key: KlipyKey,
    customer_id: String,
}

impl KeyJson {
    /// What was read, when it can be used: a customer id of the key's
    /// characters.
    fn saved(self) -> Option<SavedKey> {
        let Self { key, customer_id } = self;
        is_valid_key(&customer_id).then_some(SavedKey { key, customer_id })
    }
}

/// The key file's `key`, read by [`KlipyKey::parse`] where serde holds it:
/// anything else (not a string, not a key, cut short) is an error of fixed
/// text, which quotes nothing of what the file holds.
fn read_key<'de, D: Deserializer<'de>>(deserializer: D) -> Result<KlipyKey, D::Error> {
    match deserializer.deserialize_str(KeyText) {
        Ok(Ok(key)) => Ok(key),
        _ => Err(de::Error::custom(NOT_A_KEY_FILE)),
    }
}

/// The key file's `key`: the key's text, written for the file. Serde calls
/// it, through [`KeyJson`]'s attribute; no code names it, and it calls no
/// macro (the source guard).
fn write_key<S: Serializer>(key: &KlipyKey, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(key.expose_secret())
}

/// The key file at one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFile {
    path: PathBuf,
}

impl KeyFile {
    /// The key file at `path` (`<config>/klipy.json`). Nothing is read
    /// until [`Self::load`].
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where the file is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved key; `None` when none is saved. A file that cannot be read
    /// or does not hold a key is `fileError`, which never quotes it.
    pub fn load(&self) -> UiResult<Option<SavedKey>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(UiError::file(self.path.display(), e)),
        };
        let saved = serde_json::from_slice::<KeyJson>(&bytes).ok();
        match saved.and_then(KeyJson::saved) {
            Some(saved) => Ok(Some(saved)),
            None => Err(UiError::file(self.path.display(), NOT_A_KEY_FILE)),
        }
    }

    /// Saves `saved`, replacing the file atomically, readable only by the
    /// user on Unix. The key's text is written by [`write_key`].
    pub fn save(&self, saved: &SavedKey) -> UiResult<()> {
        let json = KeyJson {
            key: saved.key.clone(),
            customer_id: saved.customer_id.clone(),
        };
        let json = serde_json::to_vec_pretty(&json).map_err(UiError::system)?;
        write_private(&self.path, &json).map_err(|e| UiError::file(self.path.display(), e))
    }

    /// Deletes the file; no file is not an error.
    pub fn remove(&self) -> UiResult<()> {
        match fs::remove_file(&self.path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                Err(UiError::file(self.path.display(), e))
            }
            _ => Ok(()),
        }
    }
}

/// A key the window sent that cannot be one; the message never quotes it.
fn invalid_key() -> UiError {
    UiError::new(ErrorCode::InvalidInput).arg(
        "detail",
        "a KLIPY key has 1 to 128 letters, digits, _ and -",
    )
}

/// Writes `bytes` to a new private file next to `path`, then renames it
/// over `path`; on failure the temporary file is removed and `path` stays
/// as it was.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_file_name(format!(".{KEY_FILE}.{}.tmp", std::process::id()));
    // A leftover from a crash is replaced, never reopened with its rights.
    let _ = fs::remove_file(&temp);
    let written = write_closed(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        // Best effort: a leftover temporary file is never read.
        let _ = fs::remove_file(&temp);
    }
    written
}

/// Writes `bytes` to the new file `path` and closes it.
fn write_closed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = create_private(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// A new file readable and writable only by the user.
#[cfg(unix)]
fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// A new file, private through the profile folder's ACL.
#[cfg(not(unix))]
fn create_private(path: &Path) -> io::Result<File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}
