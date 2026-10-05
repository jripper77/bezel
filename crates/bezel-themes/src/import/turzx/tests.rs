//! Synthetic `.turtheme` streams built by hand with the NRBF test writer.

use bezel_core::domain::sensor::keys;

use super::*;
use crate::import::nrbf::write::{Stream, Ty};
use crate::import::test_png;

/// A value of the synthetic object tree.
enum N {
    I(i32),
    L(i64),
    H(i16),
    D(f64),
    F(f32),
    B(bool),
    S(String),
    Null,
    O(&'static str, Vec<(&'static str, N)>),
    Bytes(Vec<u8>),
    Items(Vec<N>),
}

use N::*;

fn s(text: &str) -> N {
    S(text.to_string())
}

struct Enc {
    s: Stream,
    next: i32,
}

impl Enc {
    fn id(&mut self) -> i32 {
        self.next += 1;
        self.next
    }

    fn ty(n: &N) -> Ty<'static> {
        match n {
            I(_) => Ty::Prim(8),
            L(_) => Ty::Prim(9),
            H(_) => Ty::Prim(7),
            D(_) => Ty::Prim(6),
            F(_) => Ty::Prim(11),
            B(_) => Ty::Prim(1),
            S(_) => Ty::Str,
            Null => Ty::Obj,
            O(name, _) => Ty::Class(name, 2),
            Bytes(_) => Ty::PrimArray(2),
            Items(_) => Ty::ObjArray,
        }
    }

    fn value(&mut self, n: &N) {
        match n {
            I(v) => {
                self.s.i32(*v);
            }
            L(v) => {
                self.s.raw(&v.to_le_bytes());
            }
            H(v) => {
                self.s.raw(&v.to_le_bytes());
            }
            D(v) => {
                self.s.raw(&v.to_le_bytes());
            }
            F(v) => {
                self.s.raw(&v.to_le_bytes());
            }
            B(v) => {
                self.s.u8(u8::from(*v));
            }
            S(t) => {
                let id = self.id();
                self.s.string(id, t);
            }
            Null => {
                self.s.null();
            }
            O(name, members) => {
                let id = self.id();
                let types: Vec<(&str, Ty<'_>)> =
                    members.iter().map(|(m, v)| (*m, Self::ty(v))).collect();
                self.s.class(id, name, &types, Some(2));
                for (_, v) in members {
                    self.value(v);
                }
            }
            Bytes(b) => {
                let id = self.id();
                self.s.bytes(id, b);
            }
            Items(items) => {
                let id = self.id();
                self.s.u8(16).i32(id).i32(items.len() as i32);
                for item in items {
                    self.value(item);
                }
            }
        }
    }
}

fn encode(root: &N) -> Vec<u8> {
    let mut e = Enc {
        s: Stream::default(),
        next: 0,
    };
    e.s.header(1).library(2, "UsbMonitorL, Version=1.1.0.0");
    e.value(root);
    e.s.end().raw(&[0; 16]);
    e.s.0
}

const OBS: &str = "System.Collections.ObjectModel.ObservableCollection`1[[UsbMonitorL.GraphItem, UsbMonitorL, Version=1.1.0.0, Culture=neutral, PublicKeyToken=null]]";
const MON: &str = "System.Collections.ObjectModel.ObservableCollection`1+SimpleMonitor[[UsbMonitorL.GraphItem, UsbMonitorL, Version=1.1.0.0, Culture=neutral, PublicKeyToken=null]]";
const LIST: &str = "System.Collections.Generic.List`1[[UsbMonitorL.GraphItem, UsbMonitorL, Version=1.1.0.0, Culture=neutral, PublicKeyToken=null]]";

fn color(state: i16, value: i64, known: i16) -> N {
    O(
        "System.Drawing.Color",
        vec![
            ("name", Null),
            ("value", L(value)),
            ("knownColor", H(known)),
            ("state", H(state)),
        ],
    )
}

fn known(k: i16) -> N {
    color(1, 0, k)
}

fn argb(v: u32) -> N {
    color(2, i64::from(v), 0)
}

fn empty() -> N {
    color(0, 0, 0)
}

fn bitmap(w: u32, h: u32) -> N {
    O(
        "System.Drawing.Bitmap",
        vec![("Data", Bytes(test_png(w, h)))],
    )
}

fn m_data(data: &str, sub: Option<&str>, value: &str) -> N {
    O(
        "UsbMonitorL.M_Data",
        vec![
            ("<DataQueue>k__BackingField", Null),
            ("<queueLen>k__BackingField", I(0)),
            ("<ShowUnit>k__BackingField", B(true)),
            ("<DataName>k__BackingField", s(data)),
            ("<Sanma_Eng_Name>k__BackingField", s(data)),
            ("<SubName>k__BackingField", sub.map_or(Null, s)),
            ("<ValueWithUnit>k__BackingField", s(value)),
            ("_Value", s(value)),
        ],
    )
}

fn font(name: &str, points: i32, color: N, align: i32, gradient: i32, second: N) -> N {
    O(
        "UsbMonitorL.FontConfig",
        vec![
            ("<isBold>k__BackingField", B(true)),
            ("<name>k__BackingField", s(name)),
            ("<size>k__BackingField", I(points)),
            ("<interval>k__BackingField", F(0.0)),
            ("<color>k__BackingField", color),
            ("<GrColor>k__BackingField", second),
            ("<GrDirection>k__BackingField", I(gradient)),
            (
                "<alignment>k__BackingField",
                O(
                    "UsbMonitorL.TextAlignment",
                    vec![("displayName", s("中")), ("index", I(align))],
                ),
            ),
        ],
    )
}

/// A layer: the class's own members, then the `GraphItem` base members.
fn layer(
    class: &'static str,
    kind: &str,
    (x, y): (i32, i32),
    own: Vec<(&'static str, N)>,
    md: N,
    fc: N,
) -> N {
    let base = if class == "UsbMonitorL.GraphItem" {
        [
            "<TypeName>k__BackingField",
            "<hide>k__BackingField",
            "<posX>k__BackingField",
            "<posY>k__BackingField",
            "<m_data>k__BackingField",
            "<fontConfig>k__BackingField",
        ]
    } else {
        [
            "GraphItem+<TypeName>k__BackingField",
            "GraphItem+<hide>k__BackingField",
            "GraphItem+<posX>k__BackingField",
            "GraphItem+<posY>k__BackingField",
            "GraphItem+<m_data>k__BackingField",
            "GraphItem+<fontConfig>k__BackingField",
        ]
    };
    let mut members = own;
    members.extend([
        (base[0], s(kind)),
        (base[1], B(false)),
        (base[2], I(x)),
        (base[3], I(y)),
        (base[4], md),
        (base[5], fc),
    ]);
    O(class, members)
}

fn hidden(mut layer: N) -> N {
    if let O(_, members) = &mut layer {
        for (name, value) in members.iter_mut() {
            if name.ends_with("<hide>k__BackingField") {
                *value = B(true);
            }
        }
    }
    layer
}

fn image(at: (i32, i32), w: u32, h: u32) -> N {
    layer(
        "UsbMonitorL.GraphImage",
        "Image",
        at,
        vec![
            ("zoom_rate", D(1.0)),
            ("bitmap", bitmap(w, h)),
            ("O_bitmap", Null),
            ("ImgName", s("logo.png")),
        ],
        Null,
        Null,
    )
}

fn data(data_name: &str, sub: Option<&str>, at: (i32, i32), fc: N) -> N {
    layer(
        "UsbMonitorL.GraphItem",
        "Data",
        at,
        vec![("fahrenheit", B(false))],
        m_data(data_name, sub, "88%"),
        fc,
    )
}

fn plain_font() -> N {
    font("Arial", 12, known(164), 0, 0, empty())
}

fn theme(w: i32, h: i32, video: Option<&str>, items: Vec<N>) -> N {
    let size = items.len() as i32;
    let mut slots = items;
    slots.extend([Null, Null]);
    O(
        "UsbMonitorL.Theme",
        vec![
            ("<name>k__BackingField", s("Synthetic")),
            ("<isLanscape>k__BackingField", B(true)),
            ("<width>k__BackingField", I(w)),
            ("<height>k__BackingField", I(h)),
            (
                "<themePath>k__BackingField",
                s("C:\\Users\\author\\t.turtheme"),
            ),
            ("<videoName>k__BackingField", video.map_or(Null, s)),
            (
                "<GraphList>k__BackingField",
                O(
                    OBS,
                    vec![
                        ("_monitor", O(MON, vec![("_busyCount", I(0))])),
                        (
                            "Collection`1+items",
                            O(
                                LIST,
                                vec![
                                    ("_items", Items(slots)),
                                    ("_size", I(size)),
                                    ("_version", I(0)),
                                ],
                            ),
                        ),
                    ],
                ),
            ),
        ],
    )
}

fn run(root: &N, video: Option<&[u8]>) -> Imported {
    let bytes = encode(root);
    assert!(looks_like_turtheme(&bytes));
    let mut find = |_: &str| video.map(<[u8]>::to_vec);
    import(&bytes, "fallback", &mut find).expect("imports")
}

fn has(report: &crate::import::ImportReport, needle: &str) -> bool {
    report
        .warnings
        .iter()
        .any(|w| w.to_string().contains(needle))
}

fn key(k: &str) -> SensorKey {
    SensorKey::new(k).expect("key")
}

#[test]
fn maps_every_layer_kind() {
    let items = vec![
        image((0, 0), 480, 1920),
        image((10, 20), 50, 60),
        layer(
            "UsbMonitorL.GraphItem",
            "Text",
            (240, 100),
            vec![],
            m_data("StaticText", None, "Hello"),
            font("Arial", 24, known(164), 1, 0, empty()),
        ),
        layer(
            "UsbMonitorL.GraphItem",
            "Data",
            (400, 10),
            vec![("fahrenheit", B(true))],
            m_data("CPUTEMP", None, "52°"),
            font("GeForce", 15, argb(0xFF3E_E7FF), 2, 1, known(141)),
        ),
        data("TIME", Some("h:m:s"), (0, 0), plain_font()),
        data("FOO", None, (0, 0), plain_font()),
        layer(
            "UsbMonitorL.GraphStatuBar",
            "StatuBar",
            (100, 300),
            vec![
                ("direction", I(2)),
                ("trBack", B(false)),
                ("useGradient", B(true)),
                ("fillBack", B(false)),
                ("lineWidth", I(2)),
                ("width", I(200)),
                ("height", I(10)),
                ("radius", I(3)),
                ("FrontColor", known(140)),
                ("BackColor", known(35)),
                ("GradientColor", known(66)),
                ("useSubsection", B(true)),
            ],
            m_data("GPULOAD", None, "88%"),
            Null,
        ),
        layer(
            "UsbMonitorL.GraphArchBar",
            "ArchBar",
            (50, 60),
            vec![
                ("useBlock", B(true)),
                ("trBack", B(true)),
                ("archWidth", I(18)),
                ("diameter", I(100)),
                ("startPer", I(25)),
                ("FrontColor", argb(0xFFE2_138B)),
                ("BackColor", known(95)),
            ],
            m_data("CPULOAD", None, "88%"),
            Null,
        ),
        layer(
            "UsbMonitorL.GraphClock",
            "Clock",
            (-215, 52),
            vec![
                ("centerX", I(238)),
                ("centerY", I(274)),
                ("angle", I(0)),
                ("endAngle", I(265)),
                ("bitmap", bitmap(200, 20)),
                ("O_bitmap", Null),
            ],
            m_data("GPURAMLOAD", None, "88%"),
            Null,
        ),
        layer(
            "UsbMonitorL.GraphLine",
            "Chart",
            (35, 544),
            vec![
                ("LineColor", known(151)),
                ("FillColor", argb(0x2887_CEEB)),
                ("BorderColor", known(151)),
                ("lineWidth", I(1)),
                ("rollDirection", B(true)),
                ("_width", I(300)),
                ("maxValue", D(1000.0)),
                ("_height", I(50)),
                ("borderWidth", I(1)),
                ("columnWidth", I(5)),
            ],
            m_data("DOWNDSPEED", None, "0KB/s"),
            Null,
        ),
        data("DRVLOAD", Some("D"), (0, 0), plain_font()),
        hidden(image((5, 5), 8, 8)),
        layer(
            "UsbMonitorL.GraphAnimation",
            "Animation",
            (0, 0),
            vec![("bitmap", bitmap(16, 16))],
            Null,
            Null,
        ),
        layer(
            "UsbMonitorL.GraphStatuBar",
            "StatuBar",
            (0, 0),
            vec![
                ("direction", I(1)),
                ("trBack", B(true)),
                ("width", I(100)),
                ("height", I(4)),
                ("BackColor", empty()),
                ("GraphItem+revert", B(true)),
            ],
            m_data("RAMVALID", None, "8888M"),
            Null,
        ),
        data("DAY", Some("Day_cn"), (0, 0), plain_font()),
        layer("UsbMonitorL.GraphItem", "Text", (0, 0), vec![], Null, Null),
        layer("UsbMonitorL.GraphItem", "Blink", (0, 0), vec![], Null, Null),
        layer("UsbMonitorL.GraphLine", "Chart", (0, 0), vec![], Null, Null),
    ];
    let (theme, assets, report) = run(&theme(480, 1920, None, items), None);
    assert_eq!(theme.name, "Synthetic");
    assert_eq!(theme.canvas, Size::new(480, 1920));
    assert_eq!(theme.orientation, Orientation::Portrait);
    let Background::Image { asset: bg, fit } = &theme.background else {
        panic!("{:?}", theme.background)
    };
    assert_eq!(*fit, Fit::Fill);
    assert!(bg.0.starts_with("assets/background-"), "{bg:?}");
    let ids: Vec<u32> = theme.elements.iter().map(|e| e.id.0).collect();
    assert_eq!(ids, (1..=ids.len() as u32).collect::<Vec<_>>());
    let by_name = |prefix: &str| {
        theme
            .elements
            .iter()
            .find(|e| e.name.starts_with(prefix))
            .unwrap_or_else(|| panic!("{prefix}"))
    };

    let logo = by_name("Image: logo.png");
    assert_eq!(logo.frame, BoxF::new(10.0, 20.0, 50.0, 60.0));

    let hello = by_name("Text: Hello");
    let ElementKind::Text { content, style } = &hello.kind else {
        panic!()
    };
    assert_eq!(*content, TextContent::Static("Hello".into()));
    assert_eq!(style.size, 32.0, "24 pt at 96 dpi");
    assert_eq!(style.font.family, "Arial");
    assert_eq!(style.font.weight, 700);
    assert_eq!(style.align, HAlign::Center);
    assert_eq!(style.paint, Paint::Solid(Rgba::WHITE));
    assert_eq!(hello.frame.x + hello.frame.width / 2.0, 240.0);

    let temp = by_name("Data: CPUTEMP");
    let ElementKind::Text { content, style } = &temp.kind else {
        panic!()
    };
    let TextContent::Sensor { key: k, format, .. } = content else {
        panic!("{content:?}")
    };
    assert_eq!(*k, key("cpu.temperature"));
    assert_eq!(format.temperature, TemperatureUnit::Fahrenheit);
    assert_eq!(format.decimals, Some(0));
    assert!(format.show_unit);
    assert_eq!(style.align, HAlign::Right);
    assert_eq!(temp.frame.x + temp.frame.width, 400.0);
    assert_eq!(
        style.paint,
        Paint::Linear {
            angle: 0.0,
            stops: vec![
                (0.0, Rgba::opaque(0x3e, 0xe7, 0xff)),
                (1.0, Rgba::opaque(255, 0, 0))
            ],
        }
    );

    let time = by_name("Data: TIME");
    assert!(matches!(
        &time.kind,
        ElementKind::Text { content: TextContent::Clock { pattern, .. }, .. } if pattern == "%H:%M:%S"
    ));
    let foo = by_name("Data: FOO");
    assert!(matches!(
        &foo.kind,
        ElementKind::Text { content: TextContent::Sensor { key: k, .. }, .. } if k.as_str() == "vendor.FOO"
    ));

    let border = by_name("Bar: GPULOAD border");
    assert_eq!(border.frame, BoxF::new(98.0, 298.0, 14.0, 204.0));
    let bar = theme
        .elements
        .iter()
        .find(|e| e.name == "Bar: GPULOAD")
        .expect("bar");
    assert_eq!(
        bar.id.0,
        border.id.0 + 1,
        "the border is drawn under the bar"
    );
    assert_eq!(bar.frame, BoxF::new(100.0, 300.0, 10.0, 200.0));
    let ElementKind::Bar {
        binding,
        direction,
        fill,
        track,
        radius,
        segments,
    } = &bar.kind
    else {
        panic!("{:?}", bar.kind)
    };
    assert_eq!(binding.key, key("gpu.usage"));
    assert_eq!((binding.min, binding.max), (0.0, 100.0));
    assert_eq!(*direction, Direction::BottomToTop);
    assert!(matches!(fill, Paint::Linear { angle, .. } if *angle == 0.0));
    assert_eq!(*track, Some(Paint::Solid(Rgba::BLACK)));
    assert_eq!(*radius, 3.0);
    assert_eq!(
        *segments,
        Some(Segments {
            count: 20,
            gap: 2.0
        })
    );

    let ring = by_name("Ring: CPULOAD");
    assert_eq!(ring.frame, BoxF::new(40.5, 50.5, 119.0, 119.0));
    let ElementKind::Ring {
        start_angle,
        sweep,
        thickness,
        clockwise,
        track,
        segments,
        cap,
        ..
    } = &ring.kind
    else {
        panic!()
    };
    assert_eq!((*start_angle, *sweep, *thickness), (90.0, 360.0, 19.0));
    assert!(*clockwise);
    assert_eq!(*track, None);
    assert_eq!(*cap, Cap::Butt);
    assert_eq!(
        *segments,
        Some(Segments {
            count: 19,
            gap: 4.0
        })
    );

    let needle = by_name("Needle: GPURAMLOAD");
    assert_eq!(needle.frame, BoxF::new(23.0, 326.0, 200.0, 20.0));
    let ElementKind::Needle {
        binding,
        asset,
        pivot,
        start_angle,
        sweep,
        ..
    } = &needle.kind
    else {
        panic!()
    };
    assert_eq!(binding.key, key("gpu.memory.percent"));
    assert_eq!(*pivot, (1.075, -2.6));
    assert_eq!((*start_angle, *sweep), (0.0, 265.0));
    assert!(assets.contains_key(asset.as_ref().expect("needle picture")));

    let chart = by_name("Chart: DOWNDSPEED");
    let ElementKind::Graph {
        binding,
        history,
        style,
        color,
        fill,
        ..
    } = &chart.kind
    else {
        panic!()
    };
    assert_eq!(binding.key, key("net.down"));
    assert_eq!(binding.max, 1000.0 * 1024.0);
    assert_eq!(*history, 60);
    assert_eq!(*style, GraphStyle::Area);
    assert_eq!(*color, Rgba::opaque(135, 206, 235));
    assert_eq!(
        *fill,
        Some(Paint::Solid(Rgba {
            r: 135,
            g: 206,
            b: 235,
            a: 0x28
        }))
    );
    assert!(matches!(
        by_name("Chart: DOWNDSPEED border").kind,
        ElementKind::Shape { stroke: Some((_, w)), fill: None, .. } if w == 1.0
    ));

    let drive = by_name("Data: DRVLOAD D");
    assert!(matches!(
        &drive.kind,
        ElementKind::Text { content: TextContent::Sensor { key: k, .. }, .. } if k.as_str() == "disk.D:.percent"
    ));
    assert!(theme.elements.iter().any(|e| !e.visible));
    let free = by_name("Bar: RAMVALID");
    let ElementKind::Bar {
        binding,
        direction,
        track,
        ..
    } = &free.kind
    else {
        panic!()
    };
    assert_eq!(binding.key, key(keys::MEMORY_AVAILABLE_PERCENT));
    assert_eq!(*direction, Direction::RightToLeft);
    assert_eq!(*track, None);

    for needle in [
        "the data source \"FOO\" is unknown",
        "DRVLOAD: Windows drive letters",
        "a video layer above the background",
        "inverted bars",
        "charts scrolling from right to left",
        "weekday format Day_cn",
        "a text layer without font settings",
        "unknown type \"Blink\"",
        "a chart without a data source",
        "fonts are not stored in .turtheme files; install them or pick others: Arial, GeForce",
    ] {
        assert!(has(&report, needle), "{needle}: {:#?}", report.warnings);
    }
    for asset in theme.assets() {
        if !asset.0.ends_with(".mp4") {
            assert!(assets.contains_key(&asset), "{asset:?}");
        }
    }
    assert_eq!(assets.len(), theme.assets().len());
}

#[test]
fn video_backgrounds_use_the_poster_and_bundle_the_video_when_found() {
    let animation = |file: N| {
        layer(
            "UsbMonitorL.GraphAnimation",
            "Animation",
            (0, 0),
            vec![
                ("bitmap", bitmap(1920, 480)),
                ("videoName", Null),
                ("FilePath", file),
            ],
            Null,
            Null,
        )
    };
    let root = theme(
        1920,
        480,
        None,
        vec![animation(s("D:\\8.8\\video\\m04.mp4"))],
    );
    let (t, assets, report) = run(&root, Some(b"MP4"));
    assert_eq!(t.orientation, Orientation::Landscape);
    let Background::Video { asset, poster, .. } = &t.background else {
        panic!("{:?}", t.background)
    };
    assert_eq!(asset.0, "assets/m04.mp4");
    assert_eq!(assets[asset], b"MP4".to_vec());
    assert!(assets.contains_key(poster.as_ref().expect("poster")));
    assert!(report.is_clean(), "{:?}", report.warnings);

    let root = theme(1920, 480, Some("AMD.mp4"), vec![image((0, 0), 1920, 480)]);
    let (t, assets, report) = run(&root, None);
    let Background::Video { asset, poster, .. } = &t.background else {
        panic!("{:?}", t.background)
    };
    assert_eq!(asset.0, "assets/AMD.mp4");
    assert!(!assets.contains_key(asset));
    assert!(poster.is_some());
    assert!(has(&report, "copy it into the theme as assets/AMD.mp4"));

    let (t, _, report) = run(&theme(1920, 480, None, vec![animation(Null)]), None);
    assert!(matches!(t.background, Background::Image { .. }));
    assert!(has(&report, "has no file name"));
}

#[test]
fn vendor_video_backgrounds_import_with_auto_framing() {
    use bezel_core::domain::framing::{PanelLayout, ResolvedFraming, VideoFraming};

    // The vendor's own transform (a crop, a quarter turn) and the source
    // size, as a theme saved with a pre-turned 480x1920 video carries them.
    let rectangle = |x, y, w, h| {
        O(
            "System.Drawing.Rectangle",
            vec![("x", I(x)), ("y", I(y)), ("width", I(w)), ("height", I(h))],
        )
    };
    let animation = || {
        layer(
            "UsbMonitorL.GraphAnimation",
            "Animation",
            (0, 0),
            vec![
                ("bitmap", bitmap(1920, 480)),
                ("videoName", s("dragon.mp4")),
                (
                    "crop",
                    O(
                        "UsbMonitorL.TransFormInfo",
                        vec![
                            ("ret", I(1)),
                            ("rotate", I(1)),
                            ("rect", rectangle(0, 240, 480, 1440)),
                        ],
                    ),
                ),
                ("direction", I(3)),
                ("FilePath", s("D:\\8.8\\video\\4801920\\dragon.mp4")),
                ("SWith", I(480)),
                ("SHeight", I(1920)),
            ],
            Null,
            Null,
        )
    };
    let video = Size::new(480, 1920);
    for (w, h, auto_turns) in [(1920, 480, 3), (480, 1920, 0)] {
        let (t, assets, report) = run(&theme(w, h, None, vec![animation()]), Some(b"MP4"));
        let Background::Video { asset, framing, .. } = &t.background else {
            panic!("{:?}", t.background)
        };
        assert_eq!(asset.0, "assets/dragon.mp4");
        assert_eq!(assets[asset], b"MP4".to_vec());
        assert_eq!(*framing, None, "{w}x{h}: Auto");
        assert!(report.is_clean(), "{:?}", report.warnings);
        // Auto: the landscape theme turns the panel-native video 270
        // degrees on its canvas, the portrait one leaves it.
        let resolved = VideoFraming::default().resolve(
            Some(video),
            t.orientation,
            PanelLayout::for_canvas(t.canvas),
        );
        assert_eq!(resolved, ResolvedFraming::plain(auto_turns), "{w}x{h}");
        // A .bezeltheme of it writes no framing.
        let json = String::from_utf8(crate::native::manifest(&t).expect("manifest")).expect("utf8");
        assert!(!json.contains("framing"), "{json}");
        assert_eq!(crate::native::parse_manifest(json.as_bytes()).ok(), Some(t));
    }
}

#[test]
fn backgrounds_that_do_not_cover_the_canvas_become_layers() {
    let (t, _, _) = run(
        &theme(480, 1920, None, vec![image((0, 0), 470, 1880)]),
        None,
    );
    assert_eq!(t.background, Background::Color(Rgba::BLACK));
    assert_eq!(t.elements.len(), 1);
    assert_eq!(t.elements[0].frame, BoxF::new(0.0, 0.0, 470.0, 1880.0));

    let (t, _, _) = run(
        &theme(480, 1920, None, vec![image((0, 0), 481, 1921)]),
        None,
    );
    assert!(matches!(
        t.background,
        Background::Image { fit: Fit::None, .. }
    ));
    assert!(t.elements.is_empty());

    let text_first = vec![data("CPULOAD", None, (0, 0), plain_font())];
    let (t, _, _) = run(&theme(480, 1920, None, text_first), None);
    assert_eq!(t.background, Background::Color(Rgba::BLACK));
    assert_eq!(t.elements.len(), 1);

    let (t, _, report) = run(&theme(480, 1920, None, vec![]), None);
    assert!(t.elements.is_empty());
    assert!(has(&report, "no layers"));

    let broken = layer(
        "UsbMonitorL.GraphImage",
        "Image",
        (0, 0),
        vec![(
            "bitmap",
            O(
                "System.Drawing.Bitmap",
                vec![("Data", Bytes(b"BMxx".to_vec()))],
            ),
        )],
        Null,
        Null,
    );
    let (t, _, report) = run(&theme(480, 1920, None, vec![broken]), None);
    assert!(t.elements.is_empty());
    assert!(has(&report, "not PNG, GIF or JPEG"));
}

#[test]
fn refuses_what_is_not_a_vendor_theme() {
    let mut none = |_: &str| None;
    assert!(import(b"junk", "x", &mut none).is_err());
    let not_theme = encode(&O("UsbMonitorL.GraphItem", vec![]));
    let e = import(&not_theme, "x", &mut none).expect_err("refused");
    assert!(e.contains("not a UsbMonitorL.Theme"), "{e}");
    let bad_size = encode(&theme(0, 1920, None, vec![]));
    let e = import(&bad_size, "x", &mut none).expect_err("refused");
    assert!(e.contains("bad canvas size"), "{e}");
    let gadget = encode(&O(
        "UsbMonitorL.Theme",
        vec![("x", O("System.Diagnostics.Process", vec![]))],
    ));
    let e = import(&gadget, "x", &mut none).expect_err("refused");
    assert!(e.contains("is not allowed"), "{e}");
    let unnamed = encode(&O(
        "UsbMonitorL.Theme",
        vec![("width", I(10)), ("height", I(10))],
    ));
    let (t, _, _) = import(&unnamed, "", &mut none).expect("imports");
    assert_eq!(t.name, "Imported theme");
    assert!(!looks_like_turtheme(b"\0short"));
}

#[test]
fn class_whitelist() {
    for ok in [
        "UsbMonitorL.Theme",
        "System.Drawing.Color",
        OBS,
        MON,
        LIST,
        "System.Collections.Generic.List`1[[System.String, mscorlib, Version=4.0.0.0]]",
        "System.Collections.Generic.Queue`1[[System.String, mscorlib, Version=4.0.0.0]]",
    ] {
        assert!(allowed_class(ok), "{ok}");
    }
    for bad in [
        "System.Diagnostics.Process",
        "System.Collections.Generic.List`1[[System.Object, mscorlib]]",
        "System.Collections.Generic.List`1[[System.String, mscorlib, Version=4.0.0.0]",
        "lcd207.Theme",
    ] {
        assert!(!allowed_class(bad), "{bad}");
    }
}

fn importer() -> Importer {
    Importer {
        b: Builder::default(),
        fonts: BTreeSet::new(),
        bitmaps: HashMap::new(),
    }
}

#[test]
fn clock_formats() {
    let mut im = importer();
    let cases = [
        ("TIME", None, "%H:%M"),
        ("TIME", Some("hm"), "%H:%M"),
        ("TIME", Some("mm"), "%M"),
        ("TIME", Some("s"), "%S"),
        ("TIME", Some("h_24"), "%H"),
        ("TIME", Some("hh"), "%I"),
        ("TIME", Some("weird"), "%H:%M"),
        ("DATE", Some("Y-M-D"), "%Y-%m-%d"),
        ("DATE", Some("yyyy"), "%Y"),
        ("DATE", Some("MM"), "%m"),
        ("DATE", Some("M_en"), "%b"),
        ("DATE", Some("M_cn"), "%m月"),
        ("DATE", Some("dd"), "%d"),
        ("DATE", Some("weird"), "%Y-%m-%d"),
        ("DAY", Some("Day_en"), "%a"),
        ("DAY", Some("Num"), "%a"),
        ("DAY", Some("weird"), "%a"),
        ("APM", None, "%p"),
    ];
    for (data, sub, want) in cases {
        assert_eq!(
            im.clock_pattern(data, sub).as_deref(),
            Some(want),
            "{data} {sub:?}"
        );
    }
    assert_eq!(im.clock_pattern("CPULOAD", None), None);
    assert!(has(&im.b.report, "Chinese month names"));
    assert!(has(&im.b.report, "the DATE format \"weird\" is unknown"));
}

#[test]
fn sensors_and_ranges() {
    let mut im = importer();
    let s = im.sensor("CPUCLOCK_G", None, false).expect("sensor");
    assert_eq!(
        (s.key, s.max, s.decimals),
        (key("cpu.frequency"), 6000.0, None)
    );
    let s = im.sensor("GPUTEMP", None, true).expect("sensor");
    assert!((s.min + 17.78).abs() < 0.01 && (s.max - 121.11).abs() < 0.01);
    let s = im.sensor("HDDTEMP", Some(" 1 "), false).expect("sensor");
    assert_eq!(s.key, key("disk.1.temperature"));
    let s = im.sensor("DRVLOAD", None, false).expect("sensor");
    assert_eq!(s.key, key("disk.C:.percent"));
    let s = im.sensor("RAM_GB", None, false).expect("sensor");
    assert_eq!((s.key, s.decimals), (key("memory.used"), Some(1)));
    let s = im.sensor("odd name", None, false).expect("sensor");
    assert_eq!(s.key, key("vendor.odd_name"));
    assert!(has(&im.b.report, "odd name"));
    assert!(im.sensor("", None, false).is_some());
    assert_eq!(sensor_key(" "), SensorKey::new("_"));
    assert_eq!(
        short("a very long piece of text that goes on"),
        "a very long piece of tex…"
    );
    assert_eq!(px(f64::INFINITY), 100_000.0);
}

fn obj_of(n: &N) -> Graph {
    Graph::parse(&encode(n), &Limits::default(), &|_: &str| true).expect("graph")
}

#[test]
fn colors_follow_the_state_bits() {
    let mut im = importer();
    let mut color_of = |n: N| {
        let g = obj_of(&n);
        let obj = Obj {
            graph: &g,
            class: g.class(g.root()).expect("root"),
        };
        im.color(obj)
    };
    assert_eq!(color_of(known(164)), Some(Rgba::WHITE));
    assert_eq!(color_of(known(13)), Some(Rgba::opaque(0, 120, 215)));
    assert_eq!(color_of(known(999)), None);
    assert_eq!(color_of(argb(0x8011_2233)).map(|c| c.a), Some(0x80));
    assert_eq!(color_of(empty()), None);
    let named = |name: &str| {
        O(
            "System.Drawing.Color",
            vec![("name", s(name)), ("state", H(8))],
        )
    };
    assert_eq!(color_of(named("Red")), Some(Rgba::opaque(255, 0, 0)));
    assert_eq!(color_of(named("Nope")), None);
    for needle in [
        "Windows 10 defaults",
        "known color 999 is unknown",
        "color name \"Nope\" is unknown",
    ] {
        assert!(has(&im.b.report, needle), "{needle}");
    }
}

#[test]
fn needles_can_run_backwards_and_rings_can_fill_from_the_end() {
    let needle = layer(
        "UsbMonitorL.GraphClock",
        "Clock",
        (0, 0),
        vec![
            ("angle", I(10)),
            ("endAngle", I(200)),
            ("offset", F(0.5)),
            ("revert", B(true)),
            ("bitmap", bitmap(10, 10)),
        ],
        m_data("CPUFAN", None, "0R"),
        Null,
    );
    let ring = layer(
        "UsbMonitorL.GraphArchBar",
        "ArchBar",
        (0, 0),
        vec![
            ("revert", B(true)),
            ("round", B(true)),
            ("totalAngel", I(270)),
            ("archWidth", I(7)),
        ],
        m_data("TIME", Some("s"), "12"),
        Null,
    );
    let (t, _, report) = run(&theme(100, 100, None, vec![needle, ring]), None);
    let ElementKind::Needle {
        start_angle,
        sweep,
        binding,
        ..
    } = &t.elements[0].kind
    else {
        panic!("{:?}", t.elements[0])
    };
    assert_eq!((*start_angle, *sweep), (110.0, -200.0));
    assert_eq!((binding.key.as_str(), binding.max), ("cpu.fan", 8000.0));
    let ElementKind::Ring {
        start_angle,
        sweep,
        clockwise,
        cap,
        thickness,
        binding,
        track,
        ..
    } = &t.elements[1].kind
    else {
        panic!("{:?}", t.elements[1])
    };
    assert_eq!((*start_angle, *sweep, *clockwise), (270.0, 270.0, false));
    assert_eq!((*cap, *thickness), (Cap::Round, 7.0));
    assert_eq!(binding.key.as_str(), "vendor.TIME");
    assert_eq!(*track, Some(Paint::Solid(LIGHT_GRAY)));
    assert!(has(&report, "bound to TIME are not supported"));
}

#[test]
fn text_paint_needs_a_second_color_and_no_spacing() {
    let mut im = importer();
    let mut paint_of = |n: N, spacing: f64| {
        let g = obj_of(&n);
        let obj = Obj {
            graph: &g,
            class: g.class(g.root()).expect("root"),
        };
        im.text_paint(obj, Rgba::WHITE, spacing)
    };
    let with = |dir: i32, second: N| font("F", 10, known(164), 0, dir, second);
    for (dir, angle) in [(2, 90.0), (3, 45.0), (4, 135.0)] {
        assert!(matches!(
            paint_of(with(dir, known(35)), 0.0),
            Paint::Linear { angle: a, .. } if a == angle
        ));
    }
    assert_eq!(paint_of(with(2, known(35)), 1.5), Paint::Solid(Rgba::WHITE));
    assert_eq!(paint_of(with(2, empty()), 0.0), Paint::Solid(Rgba::WHITE));
    assert_eq!(paint_of(with(9, known(35)), 0.0), Paint::Solid(Rgba::WHITE));
}

#[test]
fn imported_sensors_use_the_catalog_keys() {
    let mut im = importer();
    for (data, expected) in [
        ("CPUFAN", keys::CPU_FAN),
        ("CPUVOLTAGE", keys::CPU_VOLTAGE),
        ("GPUVOLTAGE", keys::GPU_VOLTAGE),
        ("WATERPUMP", keys::FAN_PUMP),
        ("CASEFAN1", keys::FAN_CASE_1),
        ("CASEFAN2", keys::FAN_CASE_2),
        ("FPS", keys::GPU_FPS),
        ("Volume", keys::SYSTEM_VOLUME),
    ] {
        let s = im.sensor(data, None, false).expect("sensor");
        assert_eq!(s.key, key(expected), "{data}");
    }
    // The GPU fan is a duty: a full bar is 100 %, not the vendor's 8000 RPM.
    let fan = im.sensor("GPUFAN", None, false).expect("sensor");
    assert_eq!((fan.key, fan.max), (key(keys::GPU_FAN), 100.0));
    assert!(has(
        &im.b.report,
        "GPUFAN: the vendor app shows the GPU fan in RPM"
    ));
    // FPS is measured now; the volume is not, and the report says so.
    assert!(!has(&im.b.report, "FPS"), "{:?}", im.b.report.warnings);
    assert!(
        has(&im.b.report, "Volume: not supported yet"),
        "{:?}",
        im.b.report.warnings
    );
    assert!(!has(&im.b.report, "does not measure"));
}
