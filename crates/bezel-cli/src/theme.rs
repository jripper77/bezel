//! Themes on the command line: finding the theme a command names (a path,
//! or the name of a bundled theme), loading it (native, or another app's
//! theme converted on the fly), `bezel render` and `bezel import`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use bezel_core::app::ThemeRuntime;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::theme::{AssetRef, Theme};
use bezel_core::ports::{ThemeLocation, ThemeStore};
use bezel_themes::import::import_path;
use bezel_themes::native::{EXTENSION, is_native, native_location};

use crate::Rendering;
use crate::messages::Messages;

pub use crate::sensors::WARM_UP;

/// A theme ready to draw.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The theme.
    pub theme: Theme,
    /// Its asset bytes.
    pub assets: BTreeMap<AssetRef, Vec<u8>>,
    /// What could not be converted exactly (themes of other apps only).
    pub warnings: Vec<String>,
}

/// The folders searched, in order, for the themes that ship with Bezel:
/// `override_dir` (`$BEZEL_THEMES_DIR`), `<exe dir>/../share/bezel/themes`
/// (packages and `install-local.sh`), then `<data home>/bezel/themes`.
pub fn bundled_candidates(
    override_dir: Option<PathBuf>,
    exe: Option<&Path>,
    data_home: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = override_dir.into_iter().collect();
    if let Some(bin) = exe.and_then(Path::parent) {
        out.push(bin.join("..").join("share").join("bezel").join("themes"));
    }
    out.extend(data_home.map(|d| d.join("bezel").join("themes")));
    out
}

/// The user's data folder from the environment: `$XDG_DATA_HOME`,
/// `$HOME/.local/share`, or `%APPDATA%` on Windows.
pub fn data_home(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    non_empty("XDG_DATA_HOME")
        .or_else(|| non_empty("HOME").map(|h| h.join(".local").join("share")))
        .or_else(|| non_empty("APPDATA"))
}

/// The first existing folder of `candidates`.
pub fn first_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|d| d.is_dir()).cloned()
}

/// Names of the themes in a bundled folder, sorted.
pub fn bundled_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_native(p))
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy();
            Some(
                name.strip_suffix(&format!(".{EXTENSION}"))
                    .unwrap_or(&name)
                    .to_string(),
            )
        })
        .collect();
    names.sort();
    names
}

/// The theme file or folder `arg` names: the path itself when it exists,
/// else the bundled theme of that name.
pub fn resolve(arg: &Path, bundled: Option<&Path>) -> anyhow::Result<PathBuf> {
    if arg.exists() {
        return Ok(arg.to_path_buf());
    }
    let is_name = arg.components().count() == 1;
    if let Some(dir) = bundled.filter(|_| is_name) {
        let file = dir.join(format!("{}.{EXTENSION}", arg.display()));
        if let Some(found) = [dir.join(arg), file].into_iter().find(|p| p.exists()) {
            return Ok(found);
        }
        let names = bundled_names(dir);
        anyhow::bail!(
            "no theme at {}; bundled themes: {}",
            arg.display(),
            names.join(", ")
        );
    }
    anyhow::bail!("no theme at {}", arg.display())
}

/// Folders whose fonts a theme at `theme` may use: `fonts/` next to it (the
/// layout of the bundled themes) and the bundled `fonts/`, without repeats.
pub fn font_dirs(theme: &Path, bundled: Option<&Path>) -> Vec<PathBuf> {
    let beside = theme.parent().map(|p| {
        if p.as_os_str().is_empty() {
            PathBuf::from("fonts")
        } else {
            p.join("fonts")
        }
    });
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in beside.into_iter().chain(bundled.map(|b| b.join("fonts"))) {
        let real = dir.canonicalize().ok();
        let seen = out.iter().any(|d| d.canonicalize().ok() == real);
        if dir.is_dir() && !seen {
            out.push(dir);
        }
    }
    out
}

/// Loads the theme at `path`: native themes through `store`, other apps'
/// themes (`.turtheme`, `theme.yaml`, a Python theme folder) converted.
pub fn load(store: &dyn ThemeStore, path: &Path) -> anyhow::Result<Loaded> {
    if is_native(path) {
        let (theme, assets) = store
            .load(&native_location(path))
            .with_context(|| format!("cannot read the theme {}", path.display()))?;
        return Ok(Loaded {
            theme,
            assets,
            warnings: Vec::new(),
        });
    }
    let (theme, assets, report) = import_path(path)?;
    Ok(Loaded {
        theme,
        assets,
        warnings: report.warnings,
    })
}

fn orientation_name(o: Orientation) -> &'static str {
    match o {
        Orientation::Portrait => "vertical",
        Orientation::ReversePortrait => "vertical-flipped",
        Orientation::Landscape => "horizontal",
        Orientation::ReverseLandscape => "horizontal-flipped",
    }
}

/// One line describing a theme: name, size and orientation.
pub fn describe(theme: &Theme) -> String {
    format!(
        "{} ({}x{} {})",
        theme.name,
        theme.canvas.width,
        theme.canvas.height,
        orientation_name(theme.orientation)
    )
}

/// `warning: …` lines for the conversion warnings.
pub fn warning_lines(warnings: &[String]) -> String {
    warnings.iter().map(|w| format!("warning: {w}\n")).collect()
}

fn write_png(frame: &Frame, output: &Path) -> anyhow::Result<()> {
    let size = frame.size();
    let image = image::RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())
        .context("the frame does not match its size")?;
    image
        .save_with_format(output, image::ImageFormat::Png)
        .with_context(|| format!("cannot write {}", output.display()))
}

/// `bezel render`: one frame of the theme at `path`, with the sensors of
/// `kit`, written to `output` as a PNG of the canvas size. The sensors are
/// sampled once and, after `pause(WARM_UP)`, again for the frame.
/// Conversion warnings go to `log`.
pub fn render(
    kit: &mut Rendering<'_>,
    path: &Path,
    output: &Path,
    pause: &mut dyn FnMut(Duration),
    log: &mut dyn Write,
) -> anyhow::Result<String> {
    let loaded = load(kit.store, path)?;
    let mut log = Messages::new(log);
    write!(log, "{}", warning_lines(&loaded.warnings));
    log.check()?;
    kit.sensors.sample().context("cannot read the sensors")?;
    pause(WARM_UP);
    let line = describe(&loaded.theme);
    let mut runtime = ThemeRuntime::new(loaded.theme, loaded.assets, kit.language);
    let frame = runtime.frame(kit.sensors, kit.renderer, (kit.clock)())?;
    write_png(&frame, output)?;
    Ok(format!("{}: {line}\n", output.display()))
}

/// `bezel import`: converts the theme at `source` into a native theme at
/// `output` (a `.bezeltheme` file, or a folder for any other name).
pub fn import(store: &dyn ThemeStore, source: &Path, output: &Path) -> anyhow::Result<String> {
    anyhow::ensure!(
        !output.exists(),
        "{} already exists; choose another name",
        output.display()
    );
    let loaded = load(store, source)?;
    let location = ThemeLocation(output.to_string_lossy().into_owned());
    store
        .save(&location, &loaded.theme, &loaded.assets)
        .with_context(|| format!("cannot write {}", output.display()))?;
    Ok(format!(
        "{}{}: {}, {} elements, {} assets\n",
        warning_lines(&loaded.warnings),
        output.display(),
        describe(&loaded.theme),
        loaded.theme.elements.len(),
        loaded.assets.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::clock::{Language, LocalTime};
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::sensor::{DisplayFormat, SensorKey, keys};
    use bezel_core::domain::theme::{
        BoxF, Element, ElementId, ElementKind, TextContent, TextStyle,
    };
    use bezel_render::{SkiaRenderer, SystemFonts};
    use bezel_sensors::FakeSensors;
    use bezel_themes::FsThemeStore;
    use bezel_themes::native::MANIFEST;

    const TIME: LocalTime = LocalTime {
        year: 2026,
        month: 9,
        day: 30,
        hour: 21,
        minute: 5,
        second: 0,
        weekday: 2,
    };

    fn now() -> LocalTime {
        TIME
    }

    /// A fresh folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bezel-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 3.5" landscape theme with a CPU usage text.
    fn small_theme() -> Theme {
        let mut theme = Theme::blank("Small", Size::new(320, 480), Orientation::Landscape);
        theme.background = bezel_core::domain::theme::Background::Color(Rgba::opaque(1, 2, 3));
        theme.elements.push(Element {
            id: ElementId(1),
            name: "usage".into(),
            frame: BoxF::new(10.0, 10.0, 200.0, 40.0),
            opacity: 1.0,
            visible: true,
            locked: false,
            kind: ElementKind::Text {
                content: TextContent::Sensor {
                    key: SensorKey::new(keys::CPU_USAGE).unwrap(),
                    format: DisplayFormat::default(),
                    prefix: String::new(),
                    suffix: String::new(),
                },
                style: TextStyle::default(),
            },
        });
        theme
    }

    fn save(theme: &Theme, path: &Path) {
        FsThemeStore
            .save(
                &ThemeLocation(path.display().to_string()),
                theme,
                &BTreeMap::new(),
            )
            .unwrap();
    }

    const PYTHON_THEME: &str = "---\ndisplay:\n  DISPLAY_SIZE: 3.5\"\n  DISPLAY_ORIENTATION: landscape\nstatic_text:\n  LABEL:\n    TEXT: \"CPU\"\n    X: 20\n    Y: 18\n";

    #[test]
    fn bundled_themes_are_found_in_order() {
        let exe = Path::new("/opt/bezel/bin/bezel");
        let found = bundled_candidates(
            Some(PathBuf::from("/override")),
            Some(exe),
            Some(PathBuf::from("/home/u/.local/share")),
        );
        assert_eq!(
            found,
            vec![
                PathBuf::from("/override"),
                PathBuf::from("/opt/bezel/bin/../share/bezel/themes"),
                PathBuf::from("/home/u/.local/share/bezel/themes"),
            ]
        );
        assert!(bundled_candidates(None, None, None).is_empty());
        let dir = scratch("first");
        assert_eq!(
            first_dir(&[dir.join("missing"), dir.clone()]),
            Some(dir.clone())
        );
        assert_eq!(first_dir(&[dir.join("missing")]), None);
    }

    #[test]
    fn data_home_follows_xdg_then_home_then_appdata() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        assert_eq!(
            data_home(env(&[("XDG_DATA_HOME", "/x"), ("HOME", "/h")])),
            Some(PathBuf::from("/x"))
        );
        assert_eq!(
            data_home(env(&[("XDG_DATA_HOME", ""), ("HOME", "/h")])),
            Some(PathBuf::from("/h/.local/share"))
        );
        assert_eq!(
            data_home(env(&[("APPDATA", "C:\\Users\\u\\AppData\\Roaming")])),
            Some(PathBuf::from("C:\\Users\\u\\AppData\\Roaming"))
        );
        assert_eq!(data_home(env(&[])), None);
    }

    #[test]
    fn themes_resolve_by_path_or_bundled_name() {
        let dir = scratch("resolve");
        save(&small_theme(), &dir.join("small"));
        save(&small_theme(), &dir.join("zipped.bezeltheme"));
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        assert_eq!(bundled_names(&dir), vec!["small", "zipped"]);
        assert!(bundled_names(&dir.join("missing")).is_empty());

        let direct = dir.join("small");
        assert_eq!(resolve(&direct, None).unwrap(), direct);
        assert_eq!(resolve(Path::new("small"), Some(&dir)).unwrap(), direct);
        assert_eq!(
            resolve(Path::new("zipped"), Some(&dir)).unwrap(),
            dir.join("zipped.bezeltheme")
        );
        let err = resolve(Path::new("nope"), Some(&dir)).unwrap_err();
        assert!(err.to_string().contains("small, zipped"), "{err}");
        let err = resolve(Path::new("a/nope"), Some(&dir)).unwrap_err();
        assert_eq!(err.to_string(), "no theme at a/nope");
    }

    #[test]
    fn fonts_come_from_beside_the_theme_and_the_bundle() {
        let dir = scratch("fonts");
        let bundled = dir.join("bundled");
        std::fs::create_dir_all(bundled.join("fonts")).unwrap();
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        let theme = dir.join("mine");
        assert_eq!(
            font_dirs(&theme, Some(&bundled)),
            vec![dir.join("fonts"), bundled.join("fonts")]
        );
        let inside = bundled.join("small");
        assert_eq!(
            font_dirs(&inside, Some(&bundled)),
            vec![bundled.join("fonts")]
        );
        // Tests run in the crate folder, which has no `fonts/`.
        assert!(font_dirs(Path::new("relative"), None).is_empty());
    }

    #[test]
    fn native_and_foreign_themes_load() {
        let dir = scratch("load");
        save(&small_theme(), &dir.join("folder"));
        save(&small_theme(), &dir.join("small.bezeltheme"));
        for path in [
            dir.join("folder"),
            dir.join("folder").join(MANIFEST),
            dir.join("small.bezeltheme"),
        ] {
            let loaded = load(&FsThemeStore, &path).unwrap();
            assert_eq!(loaded.theme, small_theme(), "{}", path.display());
            assert!(loaded.warnings.is_empty());
        }
        let python = dir.join("python");
        std::fs::create_dir_all(&python).unwrap();
        std::fs::write(python.join("theme.yaml"), PYTHON_THEME).unwrap();
        let loaded = load(&FsThemeStore, &python).unwrap();
        assert_eq!(loaded.theme.canvas, Size::new(480, 320));
        assert!(!loaded.warnings.is_empty());
        assert_eq!(
            warning_lines(&["a".into(), "b".into()]),
            "warning: a\nwarning: b\n"
        );
        std::fs::write(dir.join("junk.txt"), b"hello").unwrap();
        assert!(load(&FsThemeStore, &dir.join("junk.txt")).is_err());
        assert!(load(&FsThemeStore, &dir.join("broken.bezeltheme")).is_err());
    }

    #[test]
    fn render_writes_the_frame_after_a_warm_up() {
        let dir = scratch("render");
        save(&small_theme(), &dir.join("small"));
        let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
        let mut sensors = FakeSensors::demo();
        let mut kit = Rendering {
            store: &FsThemeStore,
            renderer: &mut renderer,
            sensors: &mut sensors,
            clock: &now,
            language: Language::English,
            bundled: None,
        };
        let mut pauses = Vec::new();
        let mut log = Vec::new();
        let png = dir.join("out.png");
        let out = render(
            &mut kit,
            &dir.join("small"),
            &png,
            &mut |d| pauses.push(d),
            &mut log,
        )
        .unwrap();
        assert_eq!(
            out,
            format!("{}: Small (480x320 horizontal)\n", png.display())
        );
        assert_eq!(pauses, vec![WARM_UP]);
        assert!(log.is_empty());
        let image = image::open(&png).unwrap();
        assert_eq!((image.width(), image.height()), (480, 320));
        assert_eq!(sensors.samples_taken(), 2);
        assert_eq!(
            describe(&Theme::blank(
                "x",
                Size::new(80, 160),
                Orientation::ReverseLandscape
            )),
            "x (160x80 horizontal-flipped)"
        );
        assert_eq!(
            describe(&Theme::blank(
                "x",
                Size::new(80, 160),
                Orientation::ReversePortrait
            )),
            "x (80x160 vertical-flipped)"
        );
    }

    #[test]
    fn import_converts_into_a_new_native_theme() {
        let dir = scratch("import");
        let python = dir.join("python");
        std::fs::create_dir_all(&python).unwrap();
        std::fs::write(python.join("theme.yaml"), PYTHON_THEME).unwrap();
        let out = dir.join("out.bezeltheme");
        let text = import(&FsThemeStore, &python, &out).unwrap();
        assert!(text.starts_with("warning: "), "{text}");
        assert!(
            text.contains("python (480x320 horizontal), 2 elements, 0 assets"),
            "{text}"
        );
        let loaded = load(&FsThemeStore, &out).unwrap();
        assert_eq!(loaded.theme.canvas, Size::new(480, 320));
        let again = import(&FsThemeStore, &python, &out).unwrap_err();
        assert!(again.to_string().contains("already exists"), "{again}");
        let folder = dir.join("folder");
        import(&FsThemeStore, &out, &folder).unwrap();
        assert!(folder.join(MANIFEST).is_file());
    }
}
