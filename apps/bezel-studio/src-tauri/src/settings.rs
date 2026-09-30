//! What the app remembers between runs: the last theme and the screen that
//! was showing it, so a start at login picks up where the user left off.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Remembered choices (`settings.json` in the app's config folder).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Location of the last theme opened or saved.
    pub last_theme: Option<String>,
    /// Key of the screen that was live when the app last changed it.
    pub live_screen: Option<String>,
}

/// Reads and writes [`Settings`] in one file.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    /// Settings stored at `path`.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where the file is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The stored settings; defaults when the file is missing or unreadable.
    pub fn load(&self) -> Settings {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Changes the stored settings with `edit` (read, edit, write).
    pub fn update(&self, edit: impl FnOnce(&mut Settings)) {
        let mut settings = self.load();
        edit(&mut settings);
        let write = || -> std::io::Result<()> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_vec_pretty(&settings).map_err(std::io::Error::other)?;
            std::fs::write(&self.path, json)
        };
        if let Err(e) = write() {
            tracing::warn!(path = %self.path.display(), "settings not saved: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_missing_or_broken_files() {
        let dir = std::env::temp_dir().join(format!("bezel-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = SettingsFile::new(dir.join("nested/settings.json"));
        assert_eq!(file.load(), Settings::default());
        file.update(|s| s.last_theme = Some("/t.bezeltheme".into()));
        file.update(|s| s.live_screen = Some("/dev/ttyACM1".into()));
        assert_eq!(
            file.load(),
            Settings {
                last_theme: Some("/t.bezeltheme".into()),
                live_screen: Some("/dev/ttyACM1".into()),
            }
        );
        std::fs::write(file.path(), b"{ broken").unwrap();
        assert_eq!(file.load(), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
