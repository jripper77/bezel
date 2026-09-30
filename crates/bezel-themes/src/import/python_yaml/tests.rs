//! A tiny Python theme laid out like the repository (`res/themes/<name>`
//! and `res/fonts`) in a temporary folder.

use std::fs;

use bezel_core::domain::sensor::keys;

use super::*;
use crate::import::{ImportReport, import_path, test_png};

const THEME: &str = r##"---
author: "@me"
display:
  DISPLAY_SIZE: 3.5"
  DISPLAY_ORIENTATION: landscape
  DISPLAY_RGB_LED: 0, 120, 255
static_images:
  BACKGROUND:
    PATH: background.png
    X: 0
    Y: 0
  ICON:
    PATH: icons/cpu.png
    X: 10
    Y: 20
  STRETCHED:
    PATH: junk.png
    WIDTH: 30
    HEIGHT: 40
  JUNK:
    PATH: junk.png
  EVIL:
    PATH: ../../x.png
  MISSING:
    PATH: nope.png
  NOPATH:
    X: 1
static_text:
  LABEL:
    TEXT: "CPU"
    X: 20
    Y: 18
    FONT: roboto/Roboto-Bold.ttf
    FONT_SIZE: 18
    FONT_COLOR: 150, 150, 150
    BACKGROUND_IMAGE: background.png
    ANCHOR: mm
  BOXED:
    TEXT: "Used:"
    X: 250
    Y: 260
    WIDTH: 100
    HEIGHT: 20
    FONT_COLOR: "#c8c8c8"
    BACKGROUND_COLOR: 20, 20, 20
    ANCHOR: rb
    ROTATION: 90
  MULTI:
    TEXT: "a\nbb"
    ALIGN: right
    ANCHOR: la
    BACKGROUND_IMAGE: background.png
  BASELINE:
    TEXT: 1.50
    WIDTH: 50
    ANCHOR: ls
    FONT_COLOR: purple-ish
  EMPTY:
    TEXT: ""
STATS:
  CPU:
    PERCENTAGE:
      INTERVAL: 2
      TEXT:
        SHOW: True
        X: 90
        Y: 10
        FONT_SIZE: 28
        FONT_COLOR: 255, 255, 255
        BACKGROUND_IMAGE: background.png
      RADIAL:
        SHOW: TRUE
        X: 70
        Y: 110
        RADIUS: 50
        WIDTH: 12
        ANGLE_START: 135
        ANGLE_END: 45
        ANGLE_STEPS: 10
        ANGLE_SEP: 3
        CLOCKWISE: True
        BAR_COLOR: 0, 200, 255
        BAR_BACKGROUND_COLOR: [0, 40, 60]
        DRAW_BAR_BACKGROUND: yes
        BAR_DECORATION: Ellipse
        SHOW_TEXT: True
        TEXT_OFFSET: [0, -10]
        CUSTOM_BBOX: [0, 0, 100, 55]
      LINE_GRAPH:
        SHOW: True
        X: 150
        Y: 60
        WIDTH: 140
        HEIGHT: 50
        HISTORY_SIZE: 60
        AUTOSCALE: False
        LINE_COLOR: 0, 200, 255
        AXIS: True
      GRAPH:
        SHOW: False
    FREQUENCY:
      INTERVAL: 5
      GRAPH:
        SHOW: True
        WIDTH: 20
        HEIGHT: 100
        MAX_VALUE: 5.3
        REVERSE_DIRECTION: True
        BAR_OUTLINE: True
    LOAD:
      FIVE:
        TEXT: {SHOW: True}
  GPU:
    INTERVAL: 1
    MEMORY:
      TEXT:
        SHOW: True
        X: 5
        Y: 5
  MEMORY:
    VIRTUAL:
      PERCENT_TEXT:
        SHOW: True
        UNIT_ML: 1
  DISK:
    MOUNT_POINT: /
    USED:
      TEXT: {SHOW: true}
  DATE:
    DAY:
      TEXT:
        SHOW: True
        FORMAT: "EEE d MMM zzz"
    HOUR:
      TEXT:
        SHOW: True
  WEATHER:
    TEMPERATURE:
      TEXT: {SHOW: True}
  CUSTOM:
    Example:
      GRAPH: {SHOW: True, WIDTH: 0, HEIGHT: 0}
      RADIAL: {SHOW: True, RADIUS: 0}
  BOGUS:
    TEXT: {SHOW: True}
  NET:
    ETH:
      DOWNLOAD:
        TEXT: {SHOW: True, FONT: "../escape.ttf"}
      SIDEWAYS:
        TEXT: {SHOW: True}
extra_top: 1
"##;

struct Repo {
    root: PathBuf,
    theme: PathBuf,
}

impl Repo {
    fn new(tag: &str, yaml: &str, with_fonts: bool) -> Repo {
        let root = std::env::temp_dir().join(format!("bezel-py-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let theme = root.join("res/themes/Tiny");
        fs::create_dir_all(theme.join("icons")).expect("dirs");
        fs::write(theme.join("theme.yaml"), yaml).expect("yaml");
        fs::write(theme.join("background.png"), test_png(480, 320)).expect("png");
        fs::write(theme.join("icons/cpu.png"), test_png(16, 16)).expect("png");
        fs::write(theme.join("junk.png"), b"not an image").expect("junk");
        if with_fonts {
            let fonts = root.join("res/fonts");
            fs::create_dir_all(fonts.join("roboto")).expect("dirs");
            fs::create_dir_all(fonts.join("roboto-mono")).expect("dirs");
            fs::write(fonts.join("roboto/Roboto-Bold.ttf"), b"BOLD").expect("font");
            fs::write(fonts.join("roboto-mono/RobotoMono-Regular.ttf"), b"MONO").expect("font");
        }
        Repo { root, theme }
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn has(report: &ImportReport, needle: &str) -> bool {
    report.warnings.iter().any(|w| w.contains(needle))
}

fn find<'t>(theme: &'t Theme, name: &str) -> &'t bezel_core::domain::theme::Element {
    theme
        .elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| {
            panic!(
                "{name}: {:#?}",
                theme.elements.iter().map(|e| &e.name).collect::<Vec<_>>()
            )
        })
}

fn sensor_key_of(kind: &ElementKind) -> Option<&str> {
    match kind {
        ElementKind::Text {
            content: TextContent::Sensor { key, .. },
            ..
        } => Some(key.as_str()),
        _ => None,
    }
}

#[test]
fn imports_every_widget_kind() {
    let repo = Repo::new("all", THEME, true);
    let (theme, assets, report) = import_dir(&repo.theme).expect("imports");
    assert_eq!(theme.name, "Tiny");
    assert_eq!(theme.canvas, Size::new(480, 320));
    assert_eq!(theme.orientation, Orientation::Landscape);
    assert_eq!(theme.refresh_seconds, 1.0);
    assert_eq!(
        theme.background,
        Background::Image {
            asset: AssetRef("assets/background.png".into()),
            fit: Fit::Fill
        }
    );

    let icon = find(&theme, "Image: ICON");
    assert_eq!(icon.frame, BoxF::new(10.0, 20.0, 16.0, 16.0));
    assert_eq!(
        find(&theme, "Image: STRETCHED").frame,
        BoxF::new(0.0, 0.0, 30.0, 40.0)
    );

    let label = find(&theme, "Text: LABEL");
    let ElementKind::Text { style, content } = &label.kind else {
        panic!()
    };
    assert_eq!(*content, TextContent::Static("CPU".into()));
    assert_eq!(
        (style.align, style.valign),
        (HAlign::Center, VAlign::Middle)
    );
    assert_eq!(label.frame.center(), (20.0, 18.0));
    assert_eq!(style.font.family, "Roboto");
    assert_eq!(style.font.weight, 700);
    assert_eq!(
        style.font.asset,
        Some(AssetRef("assets/fonts/roboto/Roboto-Bold.ttf".into()))
    );
    assert_eq!(style.paint, Paint::Solid(Rgba::opaque(150, 150, 150)));
    assert!(
        theme
            .elements
            .iter()
            .all(|e| e.name != "Text: LABEL background")
    );

    let boxed = find(&theme, "Text: BOXED");
    assert_eq!(boxed.frame, BoxF::new(250.0, 260.0, 100.0, 20.0));
    let ElementKind::Text { style, .. } = &boxed.kind else {
        panic!()
    };
    assert_eq!((style.align, style.valign), (HAlign::Right, VAlign::Bottom));
    assert_eq!(style.font.family, "RobotoMono");
    assert_eq!(style.paint, Paint::Solid(Rgba::opaque(200, 200, 200)));
    let background = find(&theme, "Text: BOXED background");
    assert_eq!(background.id.0 + 1, boxed.id.0);
    assert!(matches!(
        background.kind,
        ElementKind::Shape { fill: Some(Paint::Solid(c)), .. } if c == Rgba::opaque(20, 20, 20)
    ));

    let multi = find(&theme, "Text: MULTI");
    assert!(matches!(&multi.kind, ElementKind::Text { style, .. } if style.align == HAlign::Right));
    let baseline = find(&theme, "Text: BASELINE");
    assert!(matches!(
        &baseline.kind,
        ElementKind::Text { content: TextContent::Static(t), style } if t == "1.50" && style.paint == Paint::Solid(Rgba::BLACK)
    ));
    assert_eq!(baseline.frame, BoxF::new(0.0, 0.0, 50.0, 10.0));

    let cpu = find(&theme, "CPU.PERCENTAGE.TEXT");
    assert_eq!(sensor_key_of(&cpu.kind), Some("cpu.usage"));
    let ring = find(&theme, "CPU.PERCENTAGE.RADIAL");
    assert_eq!(ring.frame, BoxF::new(20.0, 60.0, 100.0, 100.0));
    let ElementKind::Ring {
        start_angle,
        sweep,
        thickness,
        clockwise,
        track,
        cap,
        segments,
        ..
    } = &ring.kind
    else {
        panic!()
    };
    assert_eq!((*start_angle, *sweep, *thickness), (225.0, 270.0, 12.0));
    assert!(*clockwise);
    assert_eq!(*track, Some(Paint::Solid(Rgba::opaque(0, 40, 60))));
    assert_eq!(*cap, Cap::Round);
    assert_eq!(
        *segments,
        Some(Segments {
            count: 10,
            gap: 3.0
        })
    );
    assert_eq!(
        find(&theme, "CPU.PERCENTAGE.RADIAL background").frame,
        BoxF::new(20.0, 60.0, 100.0, 55.0)
    );
    let ring_text = find(&theme, "CPU.PERCENTAGE.RADIAL text");
    assert_eq!(ring_text.frame.center(), (70.0, 100.0));
    assert_eq!(sensor_key_of(&ring_text.kind), Some("cpu.usage"));

    let graph = find(&theme, "CPU.PERCENTAGE.LINE_GRAPH");
    assert!(matches!(
        graph.kind,
        ElementKind::Graph {
            history: 60,
            autoscale: false,
            style: GraphStyle::Line,
            ..
        }
    ));
    assert!(
        theme
            .elements
            .iter()
            .all(|e| e.name != "CPU.PERCENTAGE.GRAPH")
    );

    let freq = find(&theme, "CPU.FREQUENCY.GRAPH");
    let ElementKind::Bar {
        binding,
        direction,
        track,
        ..
    } = &freq.kind
    else {
        panic!()
    };
    assert_eq!(binding.key.as_str(), "cpu.frequency");
    assert_eq!(binding.max, 5300.0);
    assert_eq!(*direction, Direction::TopToBottom);
    assert_eq!(*track, Some(Paint::Solid(Rgba::WHITE)));
    assert!(matches!(
        find(&theme, "CPU.FREQUENCY.GRAPH outline").kind,
        ElementKind::Shape {
            stroke: Some(_),
            ..
        }
    ));

    for (name, key) in [
        ("CPU.LOAD.FIVE.TEXT", "cpu.load.5"),
        ("GPU.MEMORY.TEXT", "gpu.memory.used"),
        ("MEMORY.VIRTUAL.PERCENT_TEXT", "memory.percent"),
        ("DISK.USED.TEXT", "disk.root.used"),
        ("WEATHER.TEMPERATURE.TEXT", "weather.temperature"),
    ] {
        assert_eq!(sensor_key_of(&find(&theme, name).kind), Some(key), "{name}");
    }
    let disk = find(&theme, "DISK.USED.TEXT");
    assert!(matches!(
        &disk.kind,
        ElementKind::Text { content: TextContent::Sensor { format, .. }, .. } if format.bytes == ByteUnits::Decimal
    ));
    let clock = |name: &str| match &find(&theme, name).kind {
        ElementKind::Text {
            content: TextContent::Clock { pattern },
            ..
        } => pattern.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(clock("DATE.DAY.TEXT"), "%a %e %b");
    assert_eq!(clock("DATE.HOUR.TEXT"), "%I:%M:%S %p");

    let mut referenced = theme.assets();
    referenced.sort();
    let mut bundled: Vec<AssetRef> = assets.keys().cloned().collect();
    bundled.sort();
    assert_eq!(referenced, bundled);
    assert_eq!(
        assets[&AssetRef("assets/fonts/roboto-mono/RobotoMono-Regular.ttf".into())],
        b"MONO".to_vec()
    );

    for needle in [
        "the backplate LED color",
        "static_images.JUNK: junk.png is not a PNG",
        "static_images.EVIL: the path \"../../x.png\" leaves the theme folder",
        "static_images.MISSING:",
        "static_images.NOPATH: no PATH",
        "static_text.BOXED: ROTATION is not used",
        "static_text.EMPTY: no TEXT",
        "Text: BASELINE: the FONT_COLOR",
        "STATS.CPU.PERCENTAGE.LINE_GRAPH: line graph axes",
        "STATS.MEMORY.VIRTUAL.PERCENT_TEXT: UNIT_ML is not used",
        "STATS.DISK: MOUNT_POINT is not used",
        "the date/time field zzz",
        "STATS.WEATHER.TEMPERATURE.TEXT: weather is not supported yet",
        "STATS.CUSTOM.Example.GRAPH: custom Python data classes",
        "a bar without a size was dropped",
        "a radial bar without a radius was dropped",
        "STATS.BOGUS.TEXT: unknown sensor",
        "STATS.NET.ETH.SIDEWAYS.TEXT: unknown sensor",
        "the font path \"../escape.ttf\" leaves the fonts folder",
        "the top-level key extra_top is not used",
    ] {
        assert!(has(&report, needle), "{needle}: {:#?}", report.warnings);
    }
}

#[test]
fn defaults_missing_fonts_and_import_path() {
    let yaml = "static_text:\n  T: {TEXT: x, BACKGROUND_IMAGE: big.png}\nstatic_images:\n  BIG: {PATH: big.png}\n  SMALL: {PATH: big.png, WIDTH: 500, HEIGHT: 500}\ndisplay:\n  DISPLAY_SIZE: 7.7\"\n  DISPLAY_ORIENTATION: sideways\nSTATS:\n  CPU:\n    TEMPERATURE:\n      TEXT: {SHOW: True, SHOW_UNIT: False}\n      RADIAL: {SHOW: True, RADIUS: 5, WIDTH: 50, BAR_DECORATION: Star, ANGLE_START: 405, ANGLE_END: 405}\n";
    let repo = Repo::new("defaults", yaml, false);
    fs::write(repo.theme.join("big.png"), test_png(500, 600)).expect("png");
    let (theme, assets, report) = import_path(&repo.theme).expect("imports");
    assert_eq!(theme.canvas, Size::new(320, 480));
    assert_eq!(
        theme.background,
        Background::Image {
            asset: AssetRef("assets/big.png".into()),
            fit: Fit::None
        },
        "static images are drawn first whatever the key order"
    );
    assert_eq!(theme.elements[0].name, "Image: SMALL");
    assert_eq!(assets.len(), 1, "no fonts without res/fonts");
    let text = find(&theme, "CPU.TEMPERATURE.TEXT");
    let ElementKind::Text {
        content: TextContent::Sensor { format, .. },
        style,
    } = &text.kind
    else {
        panic!()
    };
    assert!(!format.show_unit);
    assert_eq!(style.font.asset, None);
    assert!(matches!(
        find(&theme, "CPU.TEMPERATURE.TEXT background").kind,
        ElementKind::Shape {
            fill: Some(Paint::Solid(Rgba::WHITE)),
            ..
        }
    ));
    let ElementKind::Ring {
        thickness,
        sweep,
        start_angle,
        clockwise,
        ..
    } = &find(&theme, "CPU.TEMPERATURE.RADIAL").kind
    else {
        panic!()
    };
    assert_eq!(
        (*thickness, *sweep, *start_angle, *clockwise),
        (5.0, 360.0, 134.0, false)
    );
    for needle in [
        "the display size 7.7\" is unknown",
        "orientation Some(\"sideways\") is unknown",
        "res/fonts) was not found",
        "the bar decoration \"Star\" is unknown",
    ] {
        assert!(has(&report, needle), "{needle}: {:#?}", report.warnings);
    }
    let (again, _, _) = import_path(&repo.theme.join("theme.yaml")).expect("imports the file");
    assert_eq!(again, theme);
}

#[test]
fn refuses_broken_files() {
    let repo = Repo::new("broken", "- just\n- a list\n", true);
    let e = import_dir(&repo.theme).expect_err("refused");
    assert!(e.contains("not a mapping"), "{e}");
    fs::write(repo.theme.join("theme.yaml"), [0xff, 0xfe, 0x00]).expect("write");
    assert!(
        import_dir(&repo.theme)
            .expect_err("refused")
            .contains("UTF-8")
    );
    fs::write(repo.theme.join("theme.yaml"), "a: [").expect("write");
    assert!(import_dir(&repo.theme).is_err());
    fs::remove_file(repo.theme.join("theme.yaml")).expect("rm");
    assert!(import_dir(&repo.theme).is_err());
}

#[test]
fn panels_colors_faces_and_date_patterns() {
    assert_eq!(panel("8.8\""), Some(Size::new(480, 1920)));
    assert_eq!(panel(" 0.96\" "), Some(Size::new(80, 160)));
    for (s, size) in [
        ("2.1\"", (480, 480)),
        ("2.8\"", (480, 480)),
        ("4.6\"", (320, 960)),
        ("5\"", (480, 800)),
        ("5.2\"", (720, 1280)),
        ("8\"", (800, 1280)),
        ("9.2\"", (480, 1920)),
        ("12.3\"", (720, 1920)),
    ] {
        assert_eq!(panel(s), Some(Size::new(size.0, size.1)), "{s}");
    }
    assert_eq!(panel("6\""), None);

    let text = |s: &str| Node::Str(s.to_string());
    assert_eq!(parse_color(&text("255,0,0")), Some(Rgba::opaque(255, 0, 0)));
    assert_eq!(
        parse_color(&text("rgb(1, 2, 3)")),
        Some(Rgba::opaque(1, 2, 3))
    );
    assert_eq!(parse_color(&text("#fff")), Some(Rgba::WHITE));
    assert_eq!(
        parse_color(&text("#11223344")),
        Some(Rgba::opaque(0x11, 0x22, 0x33))
    );
    assert_eq!(parse_color(&text("Gold")), Some(Rgba::opaque(255, 215, 0)));
    assert_eq!(
        parse_color(&text("300, -5, 7.9")),
        Some(Rgba::opaque(255, 0, 7))
    );
    assert_eq!(parse_color(&text("1, 2")), None);
    assert_eq!(parse_color(&text("a, b, c")), None);
    assert_eq!(
        parse_color(&Node::Seq(vec![Node::Int(1), Node::Int(2), Node::Int(3)])),
        Some(Rgba::opaque(1, 2, 3))
    );
    assert_eq!(parse_color(&Node::Seq(vec![Node::Int(1)])), None);
    assert_eq!(parse_color(&Node::Int(0)), None);

    assert_eq!(
        font_face("JetBrainsMono-ExtraBold"),
        ("JetBrainsMono".into(), 800, false)
    );
    assert_eq!(font_face("Roboto-BoldItalic"), ("Roboto".into(), 700, true));
    assert_eq!(
        font_face("GeneraleMonoA"),
        ("GeneraleMonoA".into(), 400, false)
    );
    assert_eq!(
        font_face("RobotoMono-Light"),
        ("RobotoMono".into(), 300, false)
    );
    assert_eq!(
        font_face("fusion-pixel-10px-monospaced-zh_hans"),
        ("fusion-pixel-10px-monospaced-zh_hans".into(), 400, false)
    );
    assert_eq!(font_face("Roboto-Black").1, 900);
    assert_eq!(font_face("X-Medium").1, 500);
    assert_eq!(font_face("X-Thin").1, 100);
    assert_eq!(font_face("X-SemiBold").1, 600);
    assert_eq!(font_face("X-ExtraLight").1, 200);

    let mut im = Importer {
        dir: PathBuf::new(),
        fonts: None,
        b: Builder::default(),
        canvas: Size::new(1, 1),
        background: None,
        intervals: Vec::new(),
    };
    let cases = [
        ("short", true, "%m/%d/%y"),
        ("long", true, "%B %e, %Y"),
        ("full", true, "%A, %B %e, %Y"),
        ("short", false, "%I:%M %p"),
        ("full", false, "%I:%M:%S %p"),
        ("HH:mm:ss", false, "%H:%M:%S"),
        ("MM/dd/yyyy", true, "%m/%d/%Y"),
        ("yy.M.d", true, "%y.%m.%e"),
        ("EEEE MMMM", true, "%A %B"),
        ("h 'o''clock' a", false, "%I o'clock %p"),
        ("''HH''", false, "'%H'"),
        ("100% K", false, "100%% %I"),
    ];
    for (format, date, want) in cases {
        assert_eq!(im.cldr(format, date), want, "{format}");
    }
    assert!(im.b.report.is_clean(), "{:?}", im.b.report.warnings);
    assert_eq!(im.cldr("HH:mm G", false), "%H:%M");
    assert!(has(&im.b.report, "field G"));
}

/// The key and whether the import report gets a note, for a `STATS` path.
fn mapped(path: &[&str], widget: &str) -> Option<(String, bool)> {
    let path: Vec<String> = path.iter().map(|p| (*p).to_string()).collect();
    source(&path, widget).map(|s| (s.key, s.note.is_some()))
}

#[test]
fn stats_bind_to_the_keys_bezel_publishes() {
    let bound = |key: &str| Some((key.to_string(), false));
    assert_eq!(mapped(&["PING"], "TEXT"), bound(keys::NET_PING));
    assert_eq!(mapped(&["GPU", "FPS"], "TEXT"), bound(keys::GPU_FPS));
    assert_eq!(
        mapped(&["GPU", "FAN_SPEED"], "RADIAL"),
        bound(keys::GPU_FAN)
    );
    assert_eq!(
        mapped(&["NET", "ETH", "DOWNLOADED"], "TEXT"),
        bound(keys::NET_DOWN_TOTAL)
    );
    assert_eq!(
        mapped(&["NET", "WLO", "UPLOADED"], "TEXT"),
        bound(keys::NET_UP_TOTAL)
    );
    assert_eq!(
        mapped(&["DISK", "USED"], "GRAPH"),
        bound(keys::ROOT_DISK_PERCENT)
    );
    assert_eq!(
        mapped(&["DISK", "FREE"], "TEXT"),
        bound(keys::ROOT_DISK_FREE)
    );
    assert_eq!(
        mapped(&["MEMORY", "SWAP"], "TEXT"),
        bound(keys::SWAP_PERCENT)
    );
    // A percent the Python app guesses from the RPM stays unmeasured, and
    // the report says what to bind instead.
    assert_eq!(
        mapped(&["CPU", "FAN_SPEED"], "TEXT"),
        Some(("cpu.fan.percent".to_string(), true))
    );
}
