//! The native format: `theme.json` + `assets/…`, zipped as `.bezeltheme` or
//! laid out in a folder.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use bezel_core::domain::theme::{AssetRef, Theme};
use bezel_core::ports::{ThemeLocation, ThemeStore};
use bezel_core::{BezelError, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::dto::ThemeDto;

/// Name of the manifest inside a theme.
pub const MANIFEST: &str = "theme.json";
/// File extension of zipped themes.
pub const EXTENSION: &str = "bezeltheme";

/// Reads and writes native themes on the file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsThemeStore;

fn io(context: &str, e: impl std::fmt::Display) -> BezelError {
    BezelError::Transport(format!("{context}: {e}"))
}

/// An asset path is relative, uses `/`, and never leaves the theme.
pub fn safe_asset_path(asset: &AssetRef) -> Result<PathBuf> {
    let path = Path::new(&asset.0);
    let ok = !asset.0.is_empty()
        && !asset.0.contains('\\')
        && path.components().all(|c| matches!(c, Component::Normal(_)));
    if ok {
        Ok(path.to_path_buf())
    } else {
        Err(io("unsafe asset path", &asset.0))
    }
}

/// Parses `theme.json` bytes into a theme.
pub fn parse_manifest(bytes: &[u8]) -> Result<Theme> {
    let dto: ThemeDto = serde_json::from_slice(bytes).map_err(|e| io(MANIFEST, e))?;
    Theme::try_from(&dto).map_err(|e| io(MANIFEST, e.0))
}

/// Pretty `theme.json` bytes of a theme.
pub fn manifest(theme: &Theme) -> Result<Vec<u8>> {
    let mut bytes =
        serde_json::to_vec_pretty(&ThemeDto::from(theme)).map_err(|e| io(MANIFEST, e))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn is_zip(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXTENSION) || path.is_file()
}

/// True for a theme in Bezel's own format: a folder with a `theme.json`, a
/// `.bezeltheme` file, or a `theme.json` itself. Anything else is another
/// app's theme, for the importers.
pub fn is_native(path: &Path) -> bool {
    if path.is_dir() {
        return path.join(MANIFEST).is_file();
    }
    path.file_name().is_some_and(|n| n == MANIFEST)
        || path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// Where [`FsThemeStore`] loads the native theme at `path` from: the folder
/// of a `theme.json`, else `path` itself.
pub fn native_location(path: &Path) -> ThemeLocation {
    let folder = match path.file_name() {
        Some(name) if name == MANIFEST => path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
        _ => path,
    };
    ThemeLocation(folder.to_string_lossy().into_owned())
}

fn load_folder(dir: &Path) -> Result<(Theme, BTreeMap<AssetRef, Vec<u8>>)> {
    let theme = parse_manifest(
        &fs::read(dir.join(MANIFEST)).map_err(|e| io(&dir.display().to_string(), e))?,
    )?;
    let mut assets = BTreeMap::new();
    for asset in theme.assets() {
        let path = dir.join(safe_asset_path(&asset)?);
        match fs::read(&path) {
            Ok(bytes) => {
                assets.insert(asset, bytes);
            }
            Err(e) => tracing::warn!(asset = %asset.0, "missing asset: {e}"),
        }
    }
    Ok((theme, assets))
}

fn open_zip(file: &Path) -> Result<ZipArchive<fs::File>> {
    let handle = fs::File::open(file).map_err(|e| io(&file.display().to_string(), e))?;
    ZipArchive::new(handle).map_err(|e| io("zip", e))
}

fn read_entry<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>> {
    match zip.by_name(name) {
        Ok(mut entry) => {
            let mut out = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            entry.read_to_end(&mut out).map_err(|e| io(name, e))?;
            Ok(Some(out))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(io(name, e)),
    }
}

/// Reads only the manifest of the theme at `location` (a folder or a
/// `.bezeltheme`), without its assets: enough to list a theme library.
pub fn load_manifest(location: &ThemeLocation) -> Result<Theme> {
    let path = Path::new(&location.0);
    if path.is_dir() {
        let bytes = fs::read(path.join(MANIFEST)).map_err(|e| io(&location.0, e))?;
        parse_manifest(&bytes)
    } else {
        let mut zip = open_zip(path)?;
        parse_manifest(&read_entry(&mut zip, MANIFEST)?.ok_or_else(|| io("zip", "no theme.json"))?)
    }
}

fn load_zip(file: &Path) -> Result<(Theme, BTreeMap<AssetRef, Vec<u8>>)> {
    let mut zip = open_zip(file)?;
    let mut read = |name: &str| read_entry(&mut zip, name);
    let theme = parse_manifest(&read(MANIFEST)?.ok_or_else(|| io("zip", "no theme.json"))?)?;
    let mut assets = BTreeMap::new();
    for asset in theme.assets() {
        safe_asset_path(&asset)?;
        match read(&asset.0)? {
            Some(bytes) => {
                assets.insert(asset, bytes);
            }
            None => tracing::warn!(asset = %asset.0, "missing asset in zip"),
        }
    }
    Ok((theme, assets))
}

fn save_folder(dir: &Path, theme: &Theme, assets: &BTreeMap<AssetRef, Vec<u8>>) -> Result<()> {
    let ctx = dir.display().to_string();
    fs::create_dir_all(dir).map_err(|e| io(&ctx, e))?;
    for (asset, bytes) in assets {
        let path = dir.join(safe_asset_path(asset)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io(&ctx, e))?;
        }
        fs::write(&path, bytes).map_err(|e| io(&ctx, e))?;
    }
    fs::write(dir.join(MANIFEST), manifest(theme)?).map_err(|e| io(&ctx, e))
}

/// A zipped theme as bytes.
pub fn zip_bytes(theme: &Theme, assets: &BTreeMap<AssetRef, Vec<u8>>) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(MANIFEST, deflate)
        .map_err(|e| io("zip", e))?;
    zip.write_all(&manifest(theme)?).map_err(|e| io("zip", e))?;
    for (asset, bytes) in assets {
        safe_asset_path(asset)?;
        zip.start_file(asset.0.as_str(), deflate)
            .map_err(|e| io("zip", e))?;
        zip.write_all(bytes).map_err(|e| io("zip", e))?;
    }
    Ok(zip.finish().map_err(|e| io("zip", e))?.into_inner())
}

impl ThemeStore for FsThemeStore {
    fn load(&self, location: &ThemeLocation) -> Result<(Theme, BTreeMap<AssetRef, Vec<u8>>)> {
        let path = Path::new(&location.0);
        if path.is_dir() {
            load_folder(path)
        } else if is_zip(path) {
            load_zip(path)
        } else {
            Err(BezelError::ScreenNotFound(format!(
                "theme not found: {}",
                location.0
            )))
        }
    }

    fn save(
        &self,
        location: &ThemeLocation,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
    ) -> Result<()> {
        let path = Path::new(&location.0);
        if path.extension().is_some_and(|e| e == EXTENSION) {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent).map_err(|e| io(&location.0, e))?;
            }
            fs::write(path, zip_bytes(theme, assets)?).map_err(|e| io(&location.0, e))
        } else {
            save_folder(path, theme, assets)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::geometry::{Orientation, Size};
    use bezel_core::domain::sensor::{DisplayFormat, SensorKey, TemperatureUnit};
    use bezel_core::domain::theme::{
        Background, Binding, BoxF, Cap, Direction, Element, ElementId, ElementKind, Fit,
        GraphStyle, HAlign, Paint, Segments, ShapeKind, TextContent, TextStyle, VAlign,
    };

    fn key(k: &str) -> SensorKey {
        SensorKey::new(k).expect("key")
    }

    fn binding(k: &str) -> Binding {
        Binding {
            key: key(k),
            min: 0.0,
            max: 100.0,
        }
    }

    fn el(id: u32, kind: ElementKind) -> Element {
        Element {
            id: ElementId(id),
            name: format!("element {id}"),
            frame: BoxF::new(10.0 * id as f32, 20.0, 120.0, 40.5),
            opacity: 0.75,
            visible: id != 3,
            locked: id == 2,
            kind,
        }
    }

    /// A theme using every element kind and option.
    pub(crate) fn every_kind() -> (Theme, BTreeMap<AssetRef, Vec<u8>>) {
        let gradient = Paint::Linear {
            angle: 90.0,
            stops: vec![
                (0.0, Rgba::opaque(0, 200, 255)),
                (
                    1.0,
                    Rgba {
                        r: 160,
                        g: 0,
                        b: 255,
                        a: 128,
                    },
                ),
            ],
        };
        let mut style = TextStyle {
            size: 42.0,
            align: HAlign::Center,
            valign: VAlign::Middle,
            letter_spacing: 1.5,
            paint: gradient.clone(),
            ..TextStyle::default()
        };
        style.font.asset = Some(AssetRef("assets/fonts/Inter.ttf".into()));
        style.font.weight = 700;
        let theme = Theme {
            name: "Every kind".into(),
            canvas: Size::new(480, 1920),
            orientation: Orientation::ReversePortrait,
            background: Background::Video {
                asset: AssetRef("assets/bg.mp4".into()),
                poster: Some(AssetRef("assets/bg.png".into())),
            },
            refresh_seconds: 0.5,
            elements: vec![
                el(
                    1,
                    ElementKind::Text {
                        content: TextContent::Static("Olá".into()),
                        style: style.clone(),
                    },
                ),
                el(
                    2,
                    ElementKind::Text {
                        content: TextContent::Sensor {
                            key: key("cpu.temperature"),
                            format: DisplayFormat {
                                decimals: Some(1),
                                show_unit: false,
                                temperature: TemperatureUnit::Fahrenheit,
                                ..DisplayFormat::default()
                            },
                            prefix: "CPU ".into(),
                            suffix: " °F".into(),
                        },
                        style: TextStyle::default(),
                    },
                ),
                el(
                    3,
                    ElementKind::Text {
                        content: TextContent::Clock {
                            pattern: "%H:%M".into(),
                        },
                        style,
                    },
                ),
                el(
                    4,
                    ElementKind::Image {
                        asset: AssetRef("assets/logo.png".into()),
                        fit: Fit::Cover,
                    },
                ),
                el(
                    5,
                    ElementKind::Shape {
                        shape: ShapeKind::Rect { radius: 12.0 },
                        fill: Some(gradient.clone()),
                        stroke: Some((Rgba::WHITE, 2.0)),
                    },
                ),
                el(
                    6,
                    ElementKind::Shape {
                        shape: ShapeKind::Ellipse,
                        fill: None,
                        stroke: None,
                    },
                ),
                el(
                    7,
                    ElementKind::Bar {
                        binding: binding("gpu.usage"),
                        direction: Direction::BottomToTop,
                        fill: Paint::solid(Rgba::opaque(0, 255, 0)),
                        track: Some(Paint::solid(Rgba::BLACK)),
                        radius: 4.0,
                        segments: Some(Segments {
                            count: 10,
                            gap: 2.0,
                        }),
                    },
                ),
                el(
                    8,
                    ElementKind::Ring {
                        binding: binding("cpu.usage"),
                        start_angle: -135.0,
                        sweep: 270.0,
                        thickness: 18.0,
                        clockwise: false,
                        fill: gradient.clone(),
                        track: None,
                        cap: Cap::Round,
                        segments: None,
                    },
                ),
                el(
                    9,
                    ElementKind::Needle {
                        binding: binding("cpu.usage"),
                        asset: Some(AssetRef("assets/needle.png".into())),
                        pivot: (0.5, 0.8),
                        start_angle: -120.0,
                        sweep: 240.0,
                        color: Rgba::WHITE,
                        width: 3.0,
                    },
                ),
                el(
                    10,
                    ElementKind::Graph {
                        binding: binding("memory.percent"),
                        history: 60,
                        style: GraphStyle::Area,
                        color: Rgba::opaque(255, 128, 0),
                        fill: Some(gradient),
                        line_width: 2.0,
                        autoscale: true,
                    },
                ),
            ],
        };
        let assets = theme
            .assets()
            .into_iter()
            .map(|a| (a.clone(), a.0.as_bytes().to_vec()))
            .collect();
        (theme, assets)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bezel-themes-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trip_zip_and_folder() {
        let (theme, assets) = every_kind();
        let root = scratch("roundtrip");
        for location in [root.join("t.bezeltheme"), root.join("folder")] {
            let loc = ThemeLocation(location.display().to_string());
            FsThemeStore.save(&loc, &theme, &assets).expect("saves");
            let (loaded, loaded_assets) = FsThemeStore.load(&loc).expect("loads");
            assert_eq!(loaded, theme, "{loc:?}");
            assert_eq!(loaded_assets, assets, "{loc:?}");
            assert_eq!(load_manifest(&loc).expect("manifest"), theme, "{loc:?}");
        }
        let missing = ThemeLocation(root.join("missing.bezeltheme").display().to_string());
        assert!(load_manifest(&missing).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unsafe_asset_paths_are_refused() {
        for bad in [
            "",
            "/etc/passwd",
            "../x.png",
            "assets/../../x",
            "a\\b.png",
            "./a.png",
        ] {
            assert!(safe_asset_path(&AssetRef(bad.into())).is_err(), "{bad}");
        }
        assert!(safe_asset_path(&AssetRef("assets/a b.png".into())).is_ok());
        let (mut theme, _) = every_kind();
        theme.background = Background::Image {
            asset: AssetRef("../evil.png".into()),
            fit: Fit::Fill,
        };
        let mut assets = BTreeMap::new();
        assets.insert(AssetRef("../evil.png".into()), vec![1]);
        assert!(zip_bytes(&theme, &assets).is_err());
    }

    #[test]
    fn broken_or_missing_themes_are_errors() {
        let root = scratch("broken");
        let missing = ThemeLocation(root.join("nope").display().to_string());
        assert!(FsThemeStore.load(&missing).is_err());
        fs::create_dir_all(&root).expect("dir");
        fs::write(root.join(MANIFEST), br#"{"schema": 99}"#).expect("write");
        assert!(
            FsThemeStore
                .load(&ThemeLocation(root.display().to_string()))
                .is_err()
        );
        let json = String::from_utf8(manifest(&every_kind().0).expect("manifest")).expect("utf8");
        for (from, to) in [
            ("\"reverse-portrait\"", "\"sideways\""),
            ("\"schema\": 1", "\"schema\": 2"),
            ("\"center\"", "\"justify\""),
        ] {
            fs::write(root.join(MANIFEST), json.replace(from, to)).expect("write");
            assert!(
                FsThemeStore
                    .load(&ThemeLocation(root.display().to_string()))
                    .is_err(),
                "{to}"
            );
        }
        fs::write(root.join("t.bezeltheme"), b"not a zip").expect("write");
        assert!(
            FsThemeStore
                .load(&ThemeLocation(
                    root.join("t.bezeltheme").display().to_string()
                ))
                .is_err()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn native_themes_are_told_apart_from_other_apps() {
        let root = scratch("native");
        let folder = root.join("folder");
        fs::create_dir_all(&folder).expect("dir");
        fs::write(folder.join(MANIFEST), b"{}").expect("write");
        let python = root.join("python");
        fs::create_dir_all(&python).expect("dir");
        fs::write(python.join("theme.yaml"), b"").expect("write");
        assert!(is_native(&folder));
        assert!(is_native(&folder.join(MANIFEST)));
        assert!(is_native(&root.join("Zipped.BezelTheme")));
        assert!(!is_native(&python));
        assert!(!is_native(&python.join("theme.yaml")));
        assert!(!is_native(&root.join("vendor.turtheme")));

        let at = |path: &Path| native_location(path).0;
        let shown = |path: &Path| path.to_string_lossy().into_owned();
        assert_eq!(at(&folder.join(MANIFEST)), shown(&folder));
        assert_eq!(at(&folder), shown(&folder));
        assert_eq!(at(Path::new(MANIFEST)), ".");
        let zipped = root.join("t.bezeltheme");
        assert_eq!(at(&zipped), shown(&zipped));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_assets_load_the_theme_anyway() {
        let (theme, _) = every_kind();
        let root = scratch("partial");
        let loc = ThemeLocation(root.display().to_string());
        FsThemeStore
            .save(&loc, &theme, &BTreeMap::new())
            .expect("saves");
        let (loaded, assets) = FsThemeStore.load(&loc).expect("loads");
        assert_eq!(loaded, theme);
        assert!(assets.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
