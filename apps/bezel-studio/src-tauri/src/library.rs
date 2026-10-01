//! The theme library on disk: the user's themes (writable) and the themes
//! that ship with the app (read-only; saving one writes a copy).
//!
//! The window only reaches theme files the library allows (defence in depth
//! for a webview that asks for any path): the library's folders, and the
//! files the user picked in a native dialog during this session
//! ([`ThemeLibrary::grant`]).
//!
//! Each theme also tells the screens it fits (the core's rule against the
//! device catalog) and the [`revision`] of its files, which changes whenever
//! they do.

use std::collections::BTreeSet;
use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use bezel_core::domain::catalog::MODELS;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::theme::Theme;
use bezel_core::ports::ThemeLocation;
use bezel_themes::native::{EXTENSION, is_native, load_manifest};

/// A theme found in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeEntry {
    /// Where it lives.
    pub location: ThemeLocation,
    /// Its manifest.
    pub theme: Theme,
    /// Ships with the app (read-only).
    pub bundled: bool,
    /// What its files are now ([`revision`]).
    pub revision: u64,
}

/// The catalog models whose panel `theme` fits: its canvas is the panel
/// turned the theme's way up (the core's rule, [`Theme::misfit`]).
pub fn fitting_models(theme: &Theme) -> Vec<&'static DeviceModel> {
    MODELS
        .iter()
        .filter(|m| theme.misfit(m.panel).is_none())
        .collect()
}

/// The diagonal of the screen a theme fitting `models` was made for, in
/// hundredths of an inch: the one they all share. `None` without a model,
/// or when the panel comes in several sizes (480x480 is 2.1" to 3.4").
pub fn made_for(models: &[&DeviceModel]) -> Option<u16> {
    let (first, rest) = models.split_first()?;
    rest.iter()
        .all(|m| m.diagonal_hundredths == first.diagonal_hundredths)
        .then_some(first.diagonal_hundredths)
}

/// Files of up to this size count by their content in a [`revision`];
/// larger ones by their size and modification time.
const READ_WHOLE_BYTES: u64 = 1024 * 1024;

/// Most files of a theme folder a [`revision`] looks at.
const MAX_REVISION_FILES: usize = 512;

/// Deepest folder of a theme folder a [`revision`] looks into.
const MAX_REVISION_DEPTH: usize = 4;

/// FNV-1a, 64 bits: a hash that stays the same between runs and builds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fnv(u64);

impl Fnv {
    pub(crate) const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    /// Adds `bytes`, preceded by their length (so fields never run together).
    pub(crate) fn field(&mut self, bytes: &[u8]) -> &mut Self {
        for b in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }

    pub(crate) const fn finish(&self) -> u64 {
        self.0
    }
}

fn modified_nanos(meta: &Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}

/// The regular files of the folder `dir`, at most [`MAX_REVISION_DEPTH`]
/// folders down (links are not followed).
fn files_in(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth < MAX_REVISION_DEPTH {
            files_in(&entry.path(), depth + 1, out);
        } else if kind.is_file() {
            out.push(entry.path());
        }
    }
}

/// What the files of the theme at `location` are now: a hash of their
/// names, sizes and contents (modification times for files over 1 MiB). It
/// changes whenever the theme is saved; `None` when nothing is there.
pub fn revision(location: &ThemeLocation) -> Option<u64> {
    let root = Path::new(&location.0);
    let mut files = Vec::new();
    if std::fs::metadata(root).ok()?.is_dir() {
        files_in(root, 0, &mut files);
        files.sort();
    } else {
        files.push(root.to_path_buf());
    }
    let mut hash = Fnv::new();
    for path in files.iter().take(MAX_REVISION_FILES) {
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        let name = path.strip_prefix(root).unwrap_or(path);
        hash.field(name.to_string_lossy().as_bytes())
            .field(&meta.len().to_le_bytes());
        match (meta.len() <= READ_WHOLE_BYTES)
            .then(|| std::fs::read(path).ok())
            .flatten()
        {
            Some(bytes) => hash.field(&bytes),
            None => hash.field(&modified_nanos(&meta).to_le_bytes()),
        };
    }
    Some(hash.finish())
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

fn scan(dir: &Path, bundled: bool) -> Vec<ThemeEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_native(p))
        .filter_map(|path| {
            let location = ThemeLocation(path.display().to_string());
            match load_manifest(&location) {
                Ok(theme) => Some(ThemeEntry {
                    revision: revision(&location).unwrap_or_default(),
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
    fn a_theme_names_the_screen_it_was_made_for_from_the_catalog() {
        let ids =
            |theme: &Theme| -> Vec<&str> { fitting_models(theme).iter().map(|m| m.id.0).collect() };
        let wide = Theme::blank("w", Size::new(480, 1920), Orientation::Landscape);
        assert_eq!(wide.canvas, Size::new(1920, 480));
        assert_eq!(ids(&wide), vec!["turing-8.8", "turing-usb-8.8"]);
        assert_eq!(made_for(&fitting_models(&wide)), Some(880));
        // One panel size shared by several diagonals: only the pixels tell.
        let square = Theme::blank("s", Size::new(480, 480), Orientation::Portrait);
        assert!(ids(&square).contains(&"turing-2.1"));
        assert!(ids(&square).contains(&"turing-3.4"));
        assert_eq!(made_for(&fitting_models(&square)), None);
        let small = Theme::blank("3.5", Size::new(320, 480), Orientation::Landscape);
        assert_eq!(made_for(&fitting_models(&small)), Some(350));
        // A canvas no panel has fits nothing.
        let odd = Theme::blank("odd", Size::new(333, 777), Orientation::Portrait);
        assert!(fitting_models(&odd).is_empty());
        assert_eq!(made_for(&[]), None);
    }

    #[test]
    fn the_revision_changes_when_the_theme_is_saved_again() {
        let root = scratch("revision");
        let folder = save(root.join("folder"), "Folder");
        let file = save(root.join("file.bezeltheme"), "File");
        let (first_folder, first_file) = (revision(&folder).unwrap(), revision(&file).unwrap());
        assert_eq!(
            revision(&folder),
            Some(first_folder),
            "stable while untouched"
        );
        assert_ne!(first_folder, first_file);
        for (location, first) in [(&folder, first_folder), (&file, first_file)] {
            let theme = Theme::blank("Renamed", Size::new(480, 1920), Orientation::Landscape);
            FsThemeStore
                .save(location, &theme, &BTreeMap::new())
                .unwrap();
            assert_ne!(revision(location), Some(first), "{}", location.0);
        }
        let missing = ThemeLocation(root.join("missing").display().to_string());
        assert_eq!(revision(&missing), None);
        let listed = ThemeLibrary::new(root.clone(), vec![]).list();
        assert!(
            listed
                .iter()
                .all(|e| Some(e.revision) == revision(&e.location))
        );
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
