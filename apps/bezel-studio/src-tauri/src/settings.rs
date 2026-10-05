//! What the app remembers between runs: the last theme and the screen that
//! was showing it, so a start at login picks up where the user left off, and
//! how each screen is used (vertical or horizontal), for its next new theme.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bezel_core::domain::clock::Language;
use bezel_core::domain::geometry::Orientation;
use bezel_sensors::SensorOptions;
use serde::{Deserialize, Serialize};

use crate::diag::{self, DiagCode};
use crate::dto::{orientation_slug, parse_orientation};
use crate::texts::parse_language;

/// Remembered choices (`settings.json` in the app's config folder).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Location of the last theme opened or saved.
    pub last_theme: Option<String>,
    /// Use the light Windows runtime after closing the editor; absent means enabled.
    pub light_on_close: Option<bool>,
    /// Key of the screen that was live when the app last changed it.
    pub live_screen: Option<String>,
    /// The orientation last used with each screen (`portrait`, `landscape`…),
    /// by screen key.
    pub screen_orientations: BTreeMap<String, String>,
    /// The ffmpeg chosen with the storage tab's Locate button (the program
    /// or its folder); `PATH` is searched after it.
    pub ffmpeg_path: Option<String>,
    /// The language the user chose (`pt-BR` or `en`); without one the app
    /// follows the system's.
    pub language: Option<String>,
    /// The host `net.ping` measures; without one, the sensors' default.
    pub ping_host: Option<String>,
    /// The folder of MangoHud's logs read for `gpu.fps`; without one,
    /// MangoHud's own `output_folder`.
    pub mangohud_dir: Option<String>,
    /// Which themes the Themes tab lists ([`THEME_SCOPES`]); without one,
    /// those for the screen in use when one is known.
    pub themes_shown: Option<String>,
    /// The orientation of the themes the Themes tab lists ([`THEME_AXES`]);
    /// without one, both.
    pub themes_axis: Option<String>,
}

/// What the Themes tab can list: the themes that fit the screen in use, or
/// all of them.
pub const THEME_SCOPES: [&str; 2] = ["screen", "all"];

/// The orientations the Themes tab can list: both, or one of them.
pub const THEME_AXES: [&str; 3] = ["all", "vertical", "horizontal"];

/// `text` when it is one of `known`.
fn known(known: &[&'static str], text: Option<&str>) -> Option<&'static str> {
    text.and_then(|t| known.iter().copied().find(|k| *k == t))
}

impl Settings {
    /// The options of the sensors that take settings.
    pub fn sensor_options(&self) -> SensorOptions {
        let defaults = SensorOptions::default();
        SensorOptions {
            ping_host: self.ping_host.clone().unwrap_or(defaults.ping_host),
            mangohud_dir: self.mangohud_dir.as_ref().map(PathBuf::from),
        }
    }

    /// Which themes the Themes tab lists, if a valid choice was stored.
    pub fn themes_shown(&self) -> Option<&'static str> {
        known(&THEME_SCOPES, self.themes_shown.as_deref())
    }

    /// The orientation of the themes the Themes tab lists (`all` unless a
    /// valid one was stored).
    pub fn themes_axis(&self) -> &'static str {
        known(&THEME_AXES, self.themes_axis.as_deref()).unwrap_or(THEME_AXES[0])
    }

    /// The language the user chose, if a valid one was stored.
    pub fn language(&self) -> Option<Language> {
        self.language.as_deref().and_then(parse_language)
    }

    /// The orientation last used with `screen`, if a valid one was stored.
    pub fn orientation_for(&self, screen: &str) -> Option<Orientation> {
        self.screen_orientations
            .get(screen)
            .and_then(|slug| parse_orientation(slug))
    }

    /// Remembers `orientation` as the one last used with `screen`.
    pub fn remember_orientation(&mut self, screen: &str, orientation: Orientation) {
        self.screen_orientations.insert(
            screen.to_string(),
            orientation_slug(orientation).to_string(),
        );
    }
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
        if write().is_err() {
            diag::report(DiagCode::SettingsNotSaved);
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
                ..Settings::default()
            }
        );
        std::fs::write(file.path(), b"{ broken").unwrap();
        assert_eq!(file.load(), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_enable_light_and_explicit_off_round_trips() {
        let old: Settings = serde_json::from_str(r#"{"lastTheme":"saved.bezeltheme"}"#).unwrap();
        assert!(old.light_on_close.unwrap_or(true));
        let off: Settings = serde_json::from_str(r#"{"lightOnClose":false}"#).unwrap();
        assert_eq!(off.light_on_close, Some(false));
        assert_eq!(
            serde_json::from_str::<Settings>(&serde_json::to_string(&off).unwrap()).unwrap(),
            off
        );
    }

    #[test]
    fn remembers_the_orientation_of_each_screen() {
        let dir = std::env::temp_dir().join(format!("bezel-orient-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = SettingsFile::new(dir.join("settings.json"));
        assert_eq!(file.load().orientation_for("/dev/ttyACM1"), None);
        file.update(|s| s.remember_orientation("/dev/ttyACM1", Orientation::Landscape));
        file.update(|s| s.remember_orientation("COM3", Orientation::ReversePortrait));
        file.update(|s| s.remember_orientation("/dev/ttyACM1", Orientation::ReverseLandscape));
        let loaded = file.load();
        assert_eq!(
            loaded.orientation_for("/dev/ttyACM1"),
            Some(Orientation::ReverseLandscape)
        );
        assert_eq!(
            loaded.orientation_for("COM3"),
            Some(Orientation::ReversePortrait)
        );
        let json = std::fs::read_to_string(file.path()).unwrap();
        assert!(json.contains("\"screenOrientations\""), "{json}");
        assert!(json.contains("\"reverse-landscape\""), "{json}");

        // A file from an older version, or a value edited by hand, still loads.
        std::fs::write(
            file.path(),
            br#"{"lastTheme": "/t.bezeltheme", "screenOrientations": {"k": "sideways"}}"#,
        )
        .unwrap();
        let old = file.load();
        assert_eq!(old.last_theme.as_deref(), Some("/t.bezeltheme"));
        assert_eq!(old.orientation_for("k"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remembers_which_themes_the_gallery_lists() {
        let mut settings = Settings::default();
        assert_eq!(settings.themes_shown(), None, "not chosen yet");
        assert_eq!(settings.themes_axis(), "all");
        settings.themes_shown = Some("all".into());
        settings.themes_axis = Some("vertical".into());
        assert_eq!(settings.themes_shown(), Some("all"));
        assert_eq!(settings.themes_axis(), "vertical");
        // Edited by hand: ignored, never an error.
        settings.themes_shown = Some("mine".into());
        settings.themes_axis = Some("diagonal".into());
        assert_eq!(settings.themes_shown(), None);
        assert_eq!(settings.themes_axis(), "all");
    }
}
