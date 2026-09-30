//! The theme library on disk: the user's themes (writable) and the themes
//! that ship with the app (read-only; saving one writes a copy).
//!
//! The window only reaches theme files the library allows (defence in depth
//! for a webview that asks for any path): the library's folders, and the
//! files the user picked in a native dialog during this session
//! ([`ThemeLibrary::grant`]).

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use bezel_core::domain::theme::Theme;
use bezel_core::ports::ThemeLocation;
use bezel_themes::native::{EXTENSION, MANIFEST, load_manifest};

/// A theme found in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeEntry {
    /// Where it lives.
    pub location: ThemeLocation,
    /// Its manifest.
    pub theme: Theme,
    /// Ships with the app (read-only).
    pub bundled: bool,
}

/// The folders themes are read from.
#[derive(Debug, Clone)]
pub struct ThemeLibrary {
    user: PathBuf,
    bundled: Vec<PathBuf>,
    /// Theme files outside the folders the user picked in this session.
    granted: Arc<Mutex<BTreeSet<PathBuf>>>,
}

/// An absolute path without `.` or `..`: what it names is what it says.
fn plain(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|c| {
            matches!(
                c,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

/// Whether `path` is a plain path strictly inside `dir`.
fn inside(path: &Path, dir: &Path) -> bool {
    plain(path) && path != dir && path.starts_with(dir)
}

impl ThemeLibrary {
    /// A library writing to `user` and also listing `bundled` folders.
    pub fn new(user: PathBuf, bundled: Vec<PathBuf>) -> Self {
        Self {
            user,
            bundled,
            granted: Arc::default(),
        }
    }

    fn granted(&self) -> std::sync::MutexGuard<'_, BTreeSet<PathBuf>> {
        self.granted.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Lets the window reach `location` from now on: a file the user picked
    /// in a native dialog (or the theme the app itself reopens at start).
    pub fn grant(&self, location: &ThemeLocation) {
        let path = PathBuf::from(&location.0);
        if plain(&path) {
            self.granted().insert(path);
        }
    }

    /// Whether the window may open or write `location`: inside one of the
    /// library's folders, or granted in this session.
    pub fn allows(&self, location: &ThemeLocation) -> bool {
        let path = Path::new(&location.0);
        self.is_user(location)
            || self.is_bundled(location)
            || (plain(path) && self.granted().contains(path))
    }

    /// True for a location inside the user's folder.
    pub fn is_user(&self, location: &ThemeLocation) -> bool {
        inside(Path::new(&location.0), &self.user)
    }

    /// Where new and copied themes are saved.
    pub fn user_dir(&self) -> &Path {
        &self.user
    }

    /// Every readable theme: the user's first, then the bundled ones, each
    /// group by name. Unreadable entries are skipped with a warning.
    pub fn list(&self) -> Vec<ThemeEntry> {
        let mut user = scan(&self.user, false);
        let mut bundled: Vec<ThemeEntry> = self
            .bundled
            .iter()
            .flat_map(|dir| scan(dir, true))
            .collect();
        for group in [&mut user, &mut bundled] {
            group.sort_by(|a, b| {
                a.theme
                    .name
                    .to_lowercase()
                    .cmp(&b.theme.name.to_lowercase())
                    .then_with(|| a.location.cmp(&b.location))
            });
        }
        user.extend(bundled);
        user
    }

    /// True for a location inside a bundled folder.
    pub fn is_bundled(&self, location: &ThemeLocation) -> bool {
        let path = Path::new(&location.0);
        self.bundled.iter().any(|dir| inside(path, dir))
    }

    /// A free `.bezeltheme` location in the user folder named after `name`.
    pub fn new_location(&self, name: &str) -> ThemeLocation {
        let slug = slug(name);
        let mut n = 1;
        loop {
            let suffix = if n == 1 {
                String::new()
            } else {
                format!("-{n}")
            };
            let path = self.user.join(format!("{slug}{suffix}.{EXTENSION}"));
            if !path.exists() {
                return ThemeLocation(path.display().to_string());
            }
            n += 1;
        }
    }

    /// Where saving the theme opened from `current` should write: the same
    /// place for a theme of the user's folder or a file granted in this
    /// session, a new file in the user's folder otherwise (a bundled theme,
    /// or any other place).
    pub fn save_location(&self, current: Option<&ThemeLocation>, name: &str) -> ThemeLocation {
        match current {
            Some(location) if self.allows(location) && !self.is_bundled(location) => {
                location.clone()
            }
            _ => self.new_location(name),
        }
    }
}

/// True for a theme in Bezel's own format: a `.bezeltheme` file or a folder
/// with a `theme.json`.
pub fn is_native_theme(path: &Path) -> bool {
    if path.is_dir() {
        path.join(MANIFEST).is_file()
    } else {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
    }
}

fn scan(dir: &Path, bundled: bool) -> Vec<ThemeEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_native_theme(p))
        .filter_map(|path| {
            let location = ThemeLocation(path.display().to_string());
            match load_manifest(&location) {
                Ok(theme) => Some(ThemeEntry {
                    location,
                    theme,
                    bundled,
                }),
                Err(e) => {
                    tracing::warn!(theme = %location.0, "skipped: {e}");
                    None
                }
            }
        })
        .collect()
}

/// A file-name-safe version of a theme name.
fn slug(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let joined = mapped
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if joined.is_empty() {
        "theme".into()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_core::ports::ThemeStore;
    use bezel_themes::FsThemeStore;
    use std::collections::BTreeMap;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bezel-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn save(at: PathBuf, name: &str) -> ThemeLocation {
        let location = ThemeLocation(at.display().to_string());
        let theme = Theme::blank(name, Size::new(480, 1920), Orientation::Portrait);
        FsThemeStore
            .save(&location, &theme, &BTreeMap::new())
            .unwrap();
        location
    }

    #[test]
    fn lists_user_then_bundled_themes_by_name() {
        let root = scratch("list");
        let (user, bundled) = (root.join("user"), root.join("bundled"));
        save(user.join("b.bezeltheme"), "beta");
        save(user.join("folder"), "Alpha");
        save(bundled.join("z.bezeltheme"), "Aurora");
        std::fs::write(user.join("broken.bezeltheme"), b"not a zip").unwrap();
        std::fs::write(user.join("notes.txt"), b"x").unwrap();
        let library = ThemeLibrary::new(user.clone(), vec![bundled.clone()]);
        let names: Vec<(String, bool)> = library
            .list()
            .into_iter()
            .map(|e| (e.theme.name, e.bundled))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Alpha".into(), false),
                ("beta".into(), false),
                ("Aurora".into(), true)
            ]
        );
        assert!(
            ThemeLibrary::new(root.join("missing"), vec![])
                .list()
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn saving_never_writes_into_bundled_themes() {
        let root = scratch("save");
        let (user, bundled) = (root.join("user"), root.join("bundled"));
        let shipped = save(bundled.join("aurora.bezeltheme"), "Aurora");
        let mine = save(user.join("mine.bezeltheme"), "Mine");
        let library = ThemeLibrary::new(user.clone(), vec![bundled]);
        assert!(library.is_bundled(&shipped));
        assert_eq!(library.save_location(Some(&mine), "Mine"), mine);
        let copy = library.save_location(Some(&shipped), "Aurora");
        assert_eq!(Path::new(&copy.0), user.join("Aurora.bezeltheme"));
        assert_eq!(library.user_dir(), user.as_path());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_folders_and_granted_files_are_reachable() {
        let root = scratch("allows");
        let (user, bundled) = (root.join("user"), root.join("bundled"));
        let library = ThemeLibrary::new(user.clone(), vec![bundled.clone()]);
        let at = |p: &Path| ThemeLocation(p.display().to_string());
        assert!(library.allows(&at(&user.join("mine.bezeltheme"))));
        assert!(library.allows(&at(&bundled.join("shipped"))));
        let outside = root.join("Desktop").join("x.bezeltheme");
        for refused in [
            outside.clone(),
            user.clone(),
            user.join("..").join("Desktop").join("x.bezeltheme"),
            PathBuf::from("relative.bezeltheme"),
            PathBuf::from("/etc/passwd"),
        ] {
            assert!(!library.allows(&at(&refused)), "{}", refused.display());
        }
        // Saving a theme from elsewhere writes a copy in the user folder…
        let copy = library.save_location(Some(&at(&outside)), "X");
        assert_eq!(Path::new(&copy.0), user.join("X.bezeltheme"));
        // …until the user picks that file in a dialog.
        library.grant(&at(&outside));
        library.grant(&at(Path::new("dir/../x")));
        assert!(library.allows(&at(&outside)));
        assert!(library.clone().allows(&at(&outside)), "shared by copies");
        assert_eq!(
            library.save_location(Some(&at(&outside)), "X"),
            at(&outside)
        );
        assert!(!library.allows(&at(Path::new("dir/../x"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn new_locations_are_free_and_file_safe() {
        let root = scratch("names");
        let library = ThemeLibrary::new(root.clone(), vec![]);
        let first = library.new_location("Meu tema: CPU/GPU");
        assert!(
            first.0.ends_with("Meu-tema-CPU-GPU.bezeltheme"),
            "{}",
            first.0
        );
        save(PathBuf::from(&first.0), "x");
        let second = library.new_location("Meu tema: CPU/GPU");
        assert!(
            second.0.ends_with("Meu-tema-CPU-GPU-2.bezeltheme"),
            "{}",
            second.0
        );
        assert!(library.new_location("???").0.ends_with("theme.bezeltheme"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
