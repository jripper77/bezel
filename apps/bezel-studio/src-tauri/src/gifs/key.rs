//! The user's KLIPY key on disk (D-2026-10-01-gif-sticker-search-3):
//! `<config>/klipy.json`, next to `settings.json` and not inside it, holding
//! the key and the customer id made for it. It is replaced atomically
//! (written whole to a temporary file next to it, then renamed over it) and,
//! on Unix, readable only by the user (0600); on Windows the profile's ACL
//! keeps it. Removing the key deletes the file.
//!
//! The key never leaves this module but towards the GIF source: no error
//! text, log line or `Debug` output carries it.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::messages::{ErrorCode, UiError, UiResult};

/// The key file's name, in the app's config folder.
pub const KEY_FILE: &str = "klipy.json";

/// Most characters of a key.
const MAX_KEY_CHARS: usize = 128;

/// Characters of a key of which the window may see the last 4: a shorter
/// key would show most of itself.
const SHOWN_FROM_CHARS: usize = 9;

/// Whether `text` can be a KLIPY key: 1 to 128 letters, digits, `_` and
/// `-` (it is a segment of the API's path).
pub fn is_valid_key(text: &str) -> bool {
    (1..=MAX_KEY_CHARS).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A saved key and the customer id made for it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedKey {
    key: String,
    customer_id: String,
}

impl SavedKey {
    /// `key` (checked with [`is_valid_key`] by the caller) and its
    /// `customer_id`.
    pub fn new(key: &str, customer_id: &str) -> Self {
        Self {
            key: key.to_string(),
            customer_id: customer_id.to_string(),
        }
    }

    /// The key: for the GIF source only.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The customer id made for the key.
    pub fn customer_id(&self) -> &str {
        &self.customer_id
    }

    /// The last 4 characters the window may show: only of a key of more
    /// than 8 characters.
    pub fn last4(&self) -> Option<String> {
        let shown = self.key.len().checked_sub(4)?;
        (self.key.len() >= SHOWN_FROM_CHARS).then(|| self.key[shown..].to_string())
    }

    /// Whether what was read can be used: a valid key and a customer id of
    /// the same characters.
    fn is_valid(&self) -> bool {
        is_valid_key(&self.key) && is_valid_key(&self.customer_id)
    }
}

/// Neither the key nor the customer id.
impl fmt::Debug for SavedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedKey").finish_non_exhaustive()
    }
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
        let saved = serde_json::from_slice::<SavedKey>(&bytes).ok();
        match saved.filter(SavedKey::is_valid) {
            Some(saved) => Ok(Some(saved)),
            None => Err(UiError::file(
                self.path.display(),
                "it does not hold a KLIPY key",
            )),
        }
    }

    /// Saves `saved`, replacing the file atomically, readable only by the
    /// user on Unix.
    pub fn save(&self, saved: &SavedKey) -> UiResult<()> {
        let json = serde_json::to_vec_pretty(saved).map_err(UiError::system)?;
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
pub fn invalid_key() -> UiError {
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
