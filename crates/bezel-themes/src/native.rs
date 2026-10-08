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
    BezelError::ThemeFile(format!("{context}: {e}"))
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
            Err(BezelError::ThemeFile(format!(
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
    use bezel_core::domain::framing::{
        FramingPosition, PanelLayout, Permille, ResolvedFraming, VideoFit, VideoFraming, Zoom,
    };
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
            card: None,
            card_member: None,
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
                framing: None,
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
                            language: None,
                            casing: Default::default(),
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
                        video_window: false,
                        fade: None,
                        shape: ShapeKind::Rect { radius: 12.0 },
                        fill: Some(gradient.clone()),
                        stroke: Some((Rgba::WHITE, 2.0)),
                    },
                ),
                el(
                    6,
                    ElementKind::Shape {
                        video_window: false,
                        fade: None,
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
                        test_full: false,
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
    fn weather_location_round_trips_and_rejects_bad_coordinates() {
        use bezel_core::domain::weather::Weather;
        let (mut theme, _) = every_kind();
        let weather = Weather {
            city: "Roma".into(),
            latitude: 41.9028,
            longitude: 12.4964,
            language: Some(bezel_core::domain::clock::Language::Italian),
            fahrenheit: true,
            show_icon: true,
            icon_style: bezel_core::domain::weather::IconStyle::Filled,
            icon_gap: Some(18.0),
            icon_size: Some(64.0),
        };
        let element = theme
            .elements
            .iter_mut()
            .find(|e| matches!(e.kind, ElementKind::Text { .. }))
            .unwrap();
        let ElementKind::Text { content, .. } = &mut element.kind else {
            unreachable!()
        };
        *content = TextContent::Weather(weather);
        let dto = ThemeDto::from(&theme);
        assert_eq!(Theme::try_from(&dto).unwrap(), theme);
        let mut json = serde_json::to_value(dto).unwrap();
        let content = &mut json["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["kind"]["content"]["type"] == "weather")
            .unwrap()["kind"]["content"];
        content["latitude"] = serde_json::json!(91);
        let bad: ThemeDto = serde_json::from_value(json).unwrap();
        assert!(Theme::try_from(&bad).is_err());
    }

    #[test]
    fn ring_gradient_and_full_test_round_trip_with_legacy_defaults() {
        let (mut theme, _) = every_kind();
        let k = theme
            .elements
            .iter_mut()
            .find_map(|e| {
                if matches!(e.kind, ElementKind::Ring { .. }) {
                    Some(&mut e.kind)
                } else {
                    None
                }
            })
            .unwrap();
        if let ElementKind::Ring {
            fill, test_full, ..
        } = k
        {
            *fill = Paint::Arc {
                start: Rgba::opaque(0, 0, 255),
                end: Rgba::opaque(255, 0, 0),
                transition: 0.25,
            };
            *test_full = true;
        }
        let dto = ThemeDto::from(&theme);
        assert_eq!(Theme::try_from(&dto).unwrap(), theme);
        let mut json = serde_json::to_value(dto).unwrap();
        let ring = json["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["kind"]["type"] == "ring")
            .unwrap();
        assert_eq!(ring["kind"]["testFull"], true);
        assert_eq!(ring["kind"]["fill"]["transition"].as_f64(), Some(25.0));
        ring["kind"].as_object_mut().unwrap().remove("testFull");
        let legacy: ThemeDto = serde_json::from_value(json.clone()).unwrap();
        let legacy = Theme::try_from(&legacy).unwrap();
        assert!(legacy.elements.iter().any(|e| matches!(
            e.kind,
            ElementKind::Ring {
                test_full: false,
                ..
            }
        )));
        let ring = json["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["kind"]["type"] == "ring")
            .unwrap();
        ring["kind"]["fill"]["transition"] = serde_json::json!(101);
        let bad: ThemeDto = serde_json::from_value(json).unwrap();
        assert!(Theme::try_from(&bad).is_err());
    }

    #[test]
    fn device_background_and_window_transparency_round_trip_with_safe_defaults() {
        use bezel_core::domain::{
            gradient::Fade,
            storage::{RemotePath, Repeat},
        };
        let (mut theme, _) = every_kind();
        theme.background = Background::DeviceVideo {
            path: RemotePath::parse("sd/video/vendor.mp4").unwrap(),
            repeat: Repeat::Once,
            color: Rgba::BLACK,
        };
        let shape = theme
            .elements
            .iter_mut()
            .find_map(|e| match &mut e.kind {
                ElementKind::Shape {
                    video_window, fade, ..
                } => Some((video_window, fade)),
                _ => None,
            })
            .unwrap();
        *shape.0 = true;
        *shape.1 = Some(Fade {
            angle: 90.0,
            start: 0.2,
            end: 0.8,
        });
        let dto = ThemeDto::from(&theme);
        assert_eq!(Theme::try_from(&dto).unwrap(), theme);
        assert!(!theme.assets().iter().any(|a| a.0.contains("vendor")));
        let mut json = serde_json::to_value(dto).unwrap();
        let shape = json["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["kind"]["type"] == "shape")
            .unwrap();
        shape["kind"].as_object_mut().unwrap().remove("videoWindow");
        shape["kind"].as_object_mut().unwrap().remove("fade");
        let legacy: ThemeDto = serde_json::from_value(json.clone()).unwrap();
        let legacy = Theme::try_from(&legacy).unwrap();
        assert!(legacy.elements.iter().any(|e| matches!(
            e.kind,
            ElementKind::Shape {
                video_window: false,
                fade: None,
                ..
            }
        )));
        json["background"]["path"] = serde_json::json!("sd/image/wrong.png");
        let bad: ThemeDto = serde_json::from_value(json).unwrap();
        assert!(Theme::try_from(&bad).is_err());
    }

    #[test]
    fn clock_options_round_trip_and_old_themes_follow_system() {
        use bezel_core::domain::clock::{ClockCase, Language};
        let (mut theme, _) = every_kind();
        let content = theme
            .elements
            .iter_mut()
            .find_map(|e| match &mut e.kind {
                ElementKind::Text {
                    content: c @ TextContent::Clock { .. },
                    ..
                } => Some(c),
                _ => None,
            })
            .unwrap();
        *content = TextContent::Clock {
            pattern: "%A %e %B".into(),
            language: Some(Language::Italian),
            casing: ClockCase::Upper,
        };
        let bytes = manifest(&theme).unwrap();
        assert_eq!(parse_manifest(&bytes).unwrap(), theme);
        let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let clock = json["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["kind"]["content"]["type"] == "clock")
            .unwrap();
        clock["kind"]["content"]
            .as_object_mut()
            .unwrap()
            .remove("language");
        clock["kind"]["content"]
            .as_object_mut()
            .unwrap()
            .remove("casing");
        let old = parse_manifest(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(old.elements.iter().any(|e| matches!(
            &e.kind,
            ElementKind::Text {
                content: TextContent::Clock {
                    language: None,
                    casing: ClockCase::Normal,
                    ..
                },
                ..
            }
        )));
        let clock = json["elements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["kind"]["content"]["type"] == "clock")
            .unwrap();
        clock["kind"]["content"]["language"] = serde_json::json!("invalid");
        assert!(parse_manifest(&serde_json::to_vec(&json).unwrap()).is_err());
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
    fn theme_file_problems_are_theme_file_errors() {
        let root = scratch("errors");
        let missing = ThemeLocation(root.join("nope").display().to_string());
        let theme_file = |r: Result<_>| matches!(r, Err(BezelError::ThemeFile(_)));
        assert!(theme_file(FsThemeStore.load(&missing).map(|_| ())));
        assert!(theme_file(load_manifest(&missing).map(|_| ())));
        assert!(theme_file(parse_manifest(b"{").map(|_| ())));
        assert!(theme_file(
            safe_asset_path(&AssetRef("../x.png".into())).map(|_| ())
        ));
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

    /// A `theme.json` as Bezel wrote it before framing existed, shaped like
    /// the user's "Dragon Ball": a landscape 1920x480 theme over a 480x1920
    /// video, already turned for the 8.8" panel.
    const DRAGON_BALL: &str = r#"{
  "schema": 1,
  "name": "Dragon Ball",
  "canvas": {
    "width": 1920,
    "height": 480
  },
  "orientation": "landscape",
  "refreshSeconds": 1.0,
  "background": {
    "type": "video",
    "asset": "assets/dragon.mp4",
    "poster": "assets/poster-195.png"
  },
  "elements": []
}
"#;

    /// [`DRAGON_BALL`] with `framing` set to the JSON object `framing`.
    fn dragon_ball_framed(framing: &str) -> Result<Theme> {
        let poster = r#""poster": "assets/poster-195.png""#;
        parse_manifest(
            DRAGON_BALL
                .replace(poster, &format!("{poster}, \"framing\": {framing}"))
                .as_bytes(),
        )
    }

    fn framing_of(theme: &Theme) -> Option<VideoFraming> {
        match &theme.background {
            Background::Video { framing, .. } => *framing,
            other => panic!("not a video background: {other:?}"),
        }
    }

    fn with_framing(theme: &Theme, framing: VideoFraming) -> Theme {
        let Background::Video { asset, poster, .. } = theme.background.clone() else {
            panic!("not a video background")
        };
        Theme {
            background: Background::Video {
                asset,
                poster,
                framing: Some(framing),
            },
            ..theme.clone()
        }
    }

    fn framing_json(theme: &Theme) -> serde_json::Value {
        let json: serde_json::Value =
            serde_json::from_slice(&manifest(theme).expect("manifest")).expect("json");
        json["background"]["framing"].clone()
    }

    #[test]
    fn video_framing_round_trips_and_older_themes_load_as_auto() {
        // An older theme loads with Auto, which turns its pre-turned video
        // 270 degrees on the canvas (0 in total on the 8.8"), and is written
        // back byte for byte.
        let older = parse_manifest(DRAGON_BALL.as_bytes()).expect("older theme loads");
        assert_eq!(framing_of(&older), None, "Auto");
        let resolved = VideoFraming::default().resolve(
            Some(Size::new(480, 1920)),
            older.orientation,
            PanelLayout::for_canvas(older.canvas),
        );
        assert_eq!(resolved, ResolvedFraming::plain(3));
        assert_eq!(manifest(&older).expect("manifest"), DRAGON_BALL.as_bytes());
        // The default framing is not written: the same bytes again.
        let default = with_framing(&older, VideoFraming::default());
        assert_eq!(
            manifest(&default).expect("manifest"),
            DRAGON_BALL.as_bytes()
        );

        // Every field goes and comes back exactly, in a zip and a folder.
        let turned = VideoFraming {
            rotation: Some(3),
            fit: VideoFit::Contain,
            zoom: Zoom::from_percent(125),
            position: FramingPosition {
                x: Permille::CENTER,
                y: Permille::from_permille(400),
            },
            pad: Rgba::opaque(0x10, 0x20, 0x30),
        };
        let auto = VideoFraming {
            zoom: Zoom::MAX,
            position: FramingPosition {
                x: Permille::from_permille(333),
                y: Permille::START,
            },
            ..VideoFraming::default()
        };
        let root = scratch("framing");
        let assets: BTreeMap<_, _> = [(AssetRef("assets/dragon.mp4".into()), b"MP4".to_vec())]
            .into_iter()
            .collect();
        for (case, framing) in [("turned", turned), ("auto", auto)] {
            let theme = with_framing(&older, framing);
            for location in [root.join(format!("{case}.bezeltheme")), root.join(case)] {
                let loc = ThemeLocation(location.display().to_string());
                FsThemeStore.save(&loc, &theme, &assets).expect("saves");
                let (loaded, _) = FsThemeStore.load(&loc).expect("loads");
                assert_eq!(loaded, theme, "{loc:?}");
                assert_eq!(framing_of(&loaded), Some(framing), "{loc:?}");
            }
        }
        let _ = fs::remove_dir_all(&root);

        // The JSON shape (D-2026-10-01-video-background-framing-2); Auto
        // writes no rotation.
        assert_eq!(
            framing_json(&with_framing(&older, turned)),
            serde_json::json!({
                "rotation": 270,
                "fit": "contain",
                "zoom": 1.25,
                "position": {"x": 0.5, "y": 0.4},
                "padColor": "#102030ff"
            })
        );
        assert_eq!(
            framing_json(&with_framing(&older, auto)),
            serde_json::json!({
                "fit": "cover",
                "zoom": 4.0,
                "position": {"x": 0.333, "y": 0.0},
                "padColor": "#000000ff"
            })
        );
        for rotation in [0_u8, 1, 2] {
            let theme = with_framing(
                &older,
                VideoFraming {
                    rotation: Some(rotation),
                    ..VideoFraming::default()
                },
            );
            assert_eq!(
                framing_json(&theme)["rotation"],
                serde_json::json!(u32::from(rotation) * 90)
            );
            assert_eq!(
                parse_manifest(&manifest(&theme).expect("manifest")).ok(),
                Some(theme)
            );
        }

        // A missing key takes its default; an empty or default framing is Auto.
        let fit_only = dragon_ball_framed(r#"{"fit": "contain"}"#).expect("loads");
        assert_eq!(
            framing_of(&fit_only),
            Some(VideoFraming {
                fit: VideoFit::Contain,
                ..VideoFraming::default()
            })
        );
        for default in [
            "{}",
            r#"{"rotation": null}"#,
            r##"{"fit": "cover", "zoom": 1, "position": {"x": 0.5, "y": 0.5}, "padColor": "#000"}"##,
        ] {
            let theme = dragon_ball_framed(default).expect("loads");
            assert_eq!(framing_of(&theme), None, "{default}");
            assert_eq!(manifest(&theme).expect("manifest"), DRAGON_BALL.as_bytes());
        }
    }

    #[test]
    fn video_framing_numbers_are_clamped_and_other_rotations_refused() {
        let clamped =
            dragon_ball_framed(r#"{"zoom": 9, "position": {"x": -1, "y": 7}}"#).expect("loads");
        assert_eq!(
            framing_of(&clamped),
            Some(VideoFraming {
                zoom: Zoom::MAX,
                position: FramingPosition {
                    x: Permille::START,
                    y: Permille::END,
                },
                ..VideoFraming::default()
            })
        );
        let rounded =
            dragon_ball_framed(r#"{"zoom": 1.234, "position": {"y": 0.4004}}"#).expect("loads");
        assert_eq!(
            framing_of(&rounded),
            Some(VideoFraming {
                zoom: Zoom::from_percent(123),
                position: FramingPosition {
                    x: Permille::CENTER,
                    y: Permille::from_permille(400),
                },
                ..VideoFraming::default()
            })
        );
        let unzoomed = dragon_ball_framed(r#"{"zoom": 0.2}"#).expect("loads");
        assert_eq!(
            framing_of(&unzoomed),
            None,
            "clamped to no zoom: the default"
        );
        let whole = dragon_ball_framed(r#"{"rotation": 90.0}"#).expect("loads");
        assert_eq!(framing_of(&whole).and_then(|f| f.rotation), Some(1));

        for (bad, says) in [
            (
                r#"{"rotation": 45}"#,
                "rotation 45 is not 0, 90, 180 or 270",
            ),
            (r#"{"rotation": 360}"#, "rotation 360"),
            (r#"{"rotation": -90}"#, "rotation -90"),
            (r#"{"rotation": 90.5}"#, "rotation 90.5 is not"),
            (r#"{"rotation": "90"}"#, "invalid type"),
            (r#"{"fit": "stretch"}"#, "stretch"),
            (r#"{"padColor": "red"}"#, "padColor"),
        ] {
            match dragon_ball_framed(bad) {
                Err(BezelError::ThemeFile(message)) => {
                    assert!(message.contains(says), "{bad}: {message}");
                }
                other => panic!("{bad}: {other:?}"),
            }
        }
    }
    #[test]
    fn cards_round_trip_and_validate_ownership() {
        use bezel_core::domain::theme::{
            Card, CardDirection, CardEffect, CardMember, CardTransition,
        };
        let (mut theme, _) = every_kind();
        let parent = theme
            .elements
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Shape { .. }))
            .unwrap()
            .id;
        theme
            .elements
            .iter_mut()
            .find(|e| e.id == parent)
            .unwrap()
            .card = Some(Card {
            faces: vec!["Metrics".into(), "Music".into()],
            active_face: 1,
            transition: None,
        });
        let child = theme.elements.iter_mut().find(|e| e.id != parent).unwrap();
        let child_id = child.id;
        child.card_member = Some(CardMember {
            parent,
            face: Some(0),
        });
        theme
            .elements
            .iter_mut()
            .find(|e| e.id == parent)
            .unwrap()
            .card
            .as_mut()
            .unwrap()
            .transition = Some(CardTransition {
            effect: CardEffect::Flip,
            direction: CardDirection::Down,
            duration_ms: 650,
            include_base: true,
        });
        let bytes = manifest(&theme).unwrap();
        assert_eq!(parse_manifest(&bytes).unwrap(), theme);
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        for membership in [
            serde_json::json!({"parent": 999999, "face": 0}),
            serde_json::json!({"parent": parent.0, "face": 2}),
            serde_json::json!({"parent": child_id.0, "face": 0}),
        ] {
            let mut bad = json.clone();
            let child = bad["elements"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|e| e["id"] == child_id.0)
                .unwrap();
            child["cardMember"] = membership;
            assert!(parse_manifest(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for transition in [
            serde_json::json!({"effect":"unknown","direction":"left","durationMs":650}),
            serde_json::json!({"effect":"flip","direction":"unknown","durationMs":650}),
            serde_json::json!({"effect":"fade","direction":"left","durationMs":1}),
        ] {
            let mut bad = json.clone();
            let parent = bad["elements"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|e| e["id"] == parent.0)
                .unwrap();
            parent["card"]["transition"] = transition;
            assert!(parse_manifest(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for card in [
            serde_json::json!({"faces": [], "activeFace": 0}),
            serde_json::json!({"faces": ["A"], "activeFace": 1}),
            serde_json::json!({"faces": [" "], "activeFace": 0}),
        ] {
            let mut bad = json.clone();
            let parent = bad["elements"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|e| e["id"] == parent.0)
                .unwrap();
            parent["card"] = card;
            assert!(parse_manifest(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
}
