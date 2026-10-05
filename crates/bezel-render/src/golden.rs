//! Golden checks: small themes exercising every element kind, compared with
//! the pixels their geometry dictates. Text is checked by ink bounds, ink
//! amount and equivalence between renders (sensor/clock text vs the same
//! static text) rather than byte-exact glyphs, which vary across font
//! versions. Set `BEZEL_GOLDEN_DUMP=<dir>` to write each scene as a PNG.

use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::sensor::{DisplayFormat, keys};
use bezel_core::domain::theme::{
    AssetRef, Background, Binding, BoxF, Cap, Direction, ElementKind, Fit, FontSpec, GraphStyle,
    HAlign, Paint, Segments, ShapeKind, TextContent, TextStyle, Theme, VAlign,
};

use bezel_core::domain::geometry::Size;
use bezel_core::ports::Backdrop;

use crate::SkiaRenderer;
use crate::testkit::{
    FAMILY, FONT, Scene, TIME, assert_px, close, count, element, gif, ink, png, px, render,
    render_over, theme,
};

const BLACK: Rgba = Rgba::BLACK;
const WHITE: Rgba = Rgba::WHITE;
const RED: Rgba = Rgba::opaque(255, 0, 0);
const GREEN: Rgba = Rgba::opaque(0, 255, 0);
const BLUE: Rgba = Rgba::opaque(0, 0, 255);
const GRAY: Rgba = Rgba::opaque(60, 60, 60);
const CLEAR: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// Renders and, when asked, dumps the frame for a human look.
fn shot(r: &mut SkiaRenderer, name: &str, theme: &Theme, scene: &Scene) -> Frame {
    shot_over(r, name, theme, scene, Backdrop::Poster)
}

/// [`shot`] with what a video background shows.
fn shot_over(
    r: &mut SkiaRenderer,
    name: &str,
    theme: &Theme,
    scene: &Scene,
    backdrop: Backdrop<'_>,
) -> Frame {
    let frame = render_over(r, theme, scene, backdrop);
    if let Ok(dir) = std::env::var("BEZEL_GOLDEN_DUMP") {
        let size = frame.size();
        let image = image::RgbaImage::from_raw(size.width, size.height, frame.as_rgba().to_vec())
            .expect("frame bytes");
        image
            .save(std::path::Path::new(&dir).join(format!("{name}.png")))
            .expect("dump");
    }
    frame
}

fn solid(c: Rgba) -> Paint {
    Paint::solid(c)
}

fn binding(k: &str) -> Binding {
    Binding {
        key: crate::testkit::key(k),
        min: 0.0,
        max: 100.0,
    }
}

/// A 4x2 PNG: left half red, right half blue.
fn halves() -> Vec<u8> {
    png(4, 2, |x, _| if x < 2 { RED } else { BLUE })
}

pub(crate) fn backgrounds(r: &mut SkiaRenderer) {
    let color = Rgba::opaque(10, 20, 30);
    let f = shot(
        r,
        "bg_color",
        &theme(8, 6, Background::Color(color), vec![]),
        &Scene::empty(),
    );
    assert_eq!(count(&f, (0, 0, 8, 6), |p| p == color), 48);

    let scene = Scene::empty().asset("bg.png", halves());
    let image = |fit| Background::Image {
        asset: AssetRef("bg.png".into()),
        fit,
    };
    let f = shot(
        r,
        "bg_fill",
        &theme(16, 8, image(Fit::Fill), vec![]),
        &scene,
    );
    assert_px(&f, 2, 4, RED, 4);
    assert_px(&f, 13, 4, BLUE, 4);
    let f = shot(
        r,
        "bg_contain",
        &theme(16, 16, image(Fit::Contain), vec![]),
        &scene,
    );
    assert_px(&f, 2, 2, CLEAR, 0);
    assert_px(&f, 2, 8, RED, 4);
    assert_px(&f, 13, 8, BLUE, 4);
    assert_px(&f, 13, 13, CLEAR, 0);
    let f = shot(
        r,
        "bg_cover",
        &theme(8, 8, image(Fit::Cover), vec![]),
        &scene,
    );
    assert_px(&f, 0, 0, RED, 4);
    assert_px(&f, 7, 7, BLUE, 4);
    let f = shot(r, "bg_none", &theme(8, 8, image(Fit::None), vec![]), &scene);
    assert_px(&f, 1, 1, RED, 0);
    assert_px(&f, 3, 1, BLUE, 0);
    assert_px(&f, 6, 6, CLEAR, 0);

    let poster = Scene::empty().asset("poster.png", png(2, 2, |_, _| GREEN));
    let video = |poster: Option<&str>| Background::Video {
        asset: AssetRef("clip.mp4".into()),
        poster: poster.map(|p| AssetRef(p.into())),
        framing: None,
    };
    let f = shot(
        r,
        "bg_video_poster",
        &theme(8, 8, video(Some("poster.png")), vec![]),
        &poster,
    );
    assert_eq!(count(&f, (0, 0, 8, 8), |p| p == GREEN), 64);
    let f = shot(r, "bg_video", &theme(8, 8, video(None), vec![]), &poster);
    assert_px(&f, 4, 4, Rgba::opaque(16, 17, 22), 0);
    let f = render(r, &theme(8, 8, image(Fit::Fill), vec![]), &Scene::empty());
    assert_eq!(count(&f, (0, 0, 8, 8), |p| p == CLEAR), 64, "missing image");
    video_backdrops(r);
}

/// A video background played by the screen (a transparent base under the
/// elements) and decoded on the host (its frame covers the canvas).
fn video_backdrops(r: &mut SkiaRenderer) {
    let scene = Scene::empty().asset("poster.png", png(2, 2, |_, _| GREEN));
    let square = element(
        BoxF::new(2.0, 2.0, 4.0, 4.0),
        ElementKind::Shape {
            video_window: false,
            fade: None,
            shape: ShapeKind::Rect { radius: 0.0 },
            fill: Some(solid(RED)),
            stroke: None,
        },
    );
    let t = theme(
        8,
        8,
        Background::Video {
            asset: AssetRef("clip.mp4".into()),
            poster: Some(AssetRef("poster.png".into())),
            framing: None,
        },
        vec![square],
    );
    let f = shot_over(r, "bg_video_on_device", &t, &scene, Backdrop::OnDevice);
    assert_eq!(count(&f, (0, 0, 8, 8), |p| p == CLEAR), 48, "A = 0 around");
    assert_eq!(count(&f, (2, 2, 6, 6), |p| p == RED), 16, "the element");

    let still = Frame::filled(Size::new(8, 8), BLUE);
    let f = shot_over(r, "bg_video_host", &t, &scene, Backdrop::Frame(&still));
    assert_eq!(count(&f, (0, 0, 8, 8), |p| p == BLUE), 48);
    assert_px(&f, 3, 3, RED, 0);

    let wide = Frame::from_rgba(
        Size::new(4, 2),
        [RED, RED, BLUE, BLUE, RED, RED, BLUE, BLUE]
            .iter()
            .flat_map(|c| [c.r, c.g, c.b, c.a])
            .collect(),
    )
    .expect("4x2");
    let bare = theme(8, 8, t.background.clone(), vec![]);
    let f = shot_over(
        r,
        "bg_video_host_cover",
        &bare,
        &scene,
        Backdrop::Frame(&wide),
    );
    assert_px(&f, 0, 4, RED, 4);
    assert_px(&f, 7, 4, BLUE, 4);
    assert_eq!(count(&f, (0, 0, 8, 8), |p| p.a == 255), 64, "covered");

    let empty = Frame::filled(Size::new(0, 0), BLUE);
    let f = render_over(r, &bare, &scene, Backdrop::Frame(&empty));
    assert_px(&f, 4, 4, Rgba::opaque(16, 17, 22), 0);
}

fn text(content: TextContent, style: TextStyle, frame: BoxF) -> bezel_core::domain::theme::Element {
    element(frame, ElementKind::Text { content, style })
}

fn style(size: f32, align: HAlign, valign: VAlign) -> TextStyle {
    TextStyle {
        font: FontSpec {
            family: FAMILY.into(),
            ..FontSpec::default()
        },
        size,
        paint: solid(WHITE),
        align,
        valign,
        letter_spacing: 0.0,
    }
}

fn stat(s: &str) -> TextContent {
    TextContent::Static(s.into())
}

/// Renders one text element on a black 96x48 canvas.
fn text_frame(
    r: &mut SkiaRenderer,
    name: &str,
    content: TextContent,
    style: TextStyle,
    frame: BoxF,
    scene: &Scene,
) -> Frame {
    let t = theme(
        96,
        48,
        Background::Color(BLACK),
        vec![text(content, style, frame)],
    );
    shot(r, name, &t, scene)
}

pub(crate) fn texts(r: &mut SkiaRenderer) {
    let full = BoxF::new(0.0, 0.0, 96.0, 48.0);
    let empty = Scene::empty();
    let centered = style(24.0, HAlign::Center, VAlign::Middle);
    let f = text_frame(r, "text_center", stat("Hi"), centered.clone(), full, &empty);
    let (l, t, rr, b) = ink(&f, BLACK, 24).expect("ink");
    let (cx, cy) = ((l + rr) as f32 / 2.0, (t + b) as f32 / 2.0);
    assert!((cx - 47.5).abs() <= 2.0, "centered horizontally: {l}..{rr}");
    assert!((cy - 23.5).abs() <= 3.0, "centered vertically: {t}..{b}");
    let lit = count(&f, (0, 0, 96, 48), |p| p.r > 128);
    assert!((60..=260).contains(&lit), "ink amount {lit}");
    assert!(
        count(&f, (0, 0, 96, 48), |p| p.r != p.g || p.g != p.b) == 0,
        "white on black stays gray"
    );

    let boxed = BoxF::new(10.0, 5.0, 80.0, 38.0);
    let f = text_frame(
        r,
        "text_left_top",
        stat("Hi"),
        style(24.0, HAlign::Left, VAlign::Top),
        boxed,
        &empty,
    );
    let (l, t, _, _) = ink(&f, BLACK, 24).expect("ink");
    assert!(
        (10..=14).contains(&l) && (5..=15).contains(&t),
        "left/top at {l},{t}"
    );
    let f = text_frame(
        r,
        "text_right_bottom",
        stat("Hi"),
        style(24.0, HAlign::Right, VAlign::Bottom),
        boxed,
        &empty,
    );
    let (_, _, rr, b) = ink(&f, BLACK, 24).expect("ink");
    assert!(
        (84..=89).contains(&rr) && (30..=42).contains(&b),
        "right/bottom at {rr},{b}"
    );

    let clip = BoxF::new(20.0, 10.0, 40.0, 20.0);
    let f = text_frame(
        r,
        "text_clip",
        stat("WWWWWWWWWW"),
        centered.clone(),
        clip,
        &empty,
    );
    let (l, t, rr, b) = ink(&f, BLACK, 0).expect("ink");
    assert!(
        l >= 20 && t >= 10 && rr < 60 && b < 30,
        "clipped to the box: {l},{t},{rr},{b}"
    );
    assert!(l <= 21 && rr >= 58, "overflowing text reaches both sides");

    let narrow = |spacing| TextStyle {
        letter_spacing: spacing,
        ..style(20.0, HAlign::Left, VAlign::Top)
    };
    let tight = text_frame(r, "text_tight", stat("IIII"), narrow(0.0), full, &empty);
    let loose = text_frame(r, "text_loose", stat("IIII"), narrow(10.0), full, &empty);
    let width = |f: &Frame| ink(f, BLACK, 24).map(|(l, _, r, _)| r - l).expect("ink");
    let grown = width(&loose) as i64 - width(&tight) as i64;
    assert!((28..=32).contains(&grown), "3 gaps of 10 px, grew {grown}");

    let gradient = TextStyle {
        paint: Paint::Linear {
            angle: 0.0,
            stops: vec![(0.0, RED), (1.0, BLUE)],
        },
        ..style(40.0, HAlign::Center, VAlign::Middle)
    };
    let f = text_frame(r, "text_gradient", stat("MMMMM"), gradient, full, &empty);
    let reddish = |p: Rgba| p.r > 96 && p.r > p.b.saturating_add(32);
    let bluish = |p: Rgba| p.b > 96 && p.b > p.r.saturating_add(32);
    assert!(count(&f, (0, 0, 32, 48), reddish) > 20 && count(&f, (0, 0, 32, 48), bluish) == 0);
    assert!(count(&f, (64, 0, 96, 48), bluish) > 20 && count(&f, (64, 0, 96, 48), reddish) == 0);

    bound_texts(r, full);
}

/// Sensor and clock texts render exactly like the equivalent static text.
fn bound_texts(r: &mut SkiaRenderer, full: BoxF) {
    let centered = style(16.0, HAlign::Center, VAlign::Middle);
    let scene = Scene::empty().with(keys::CPU_USAGE, 42.4);
    let sensor = |k: &str| TextContent::Sensor {
        key: crate::testkit::key(k),
        format: DisplayFormat::default(),
        prefix: "CPU ".into(),
        suffix: "!".into(),
    };
    let same = |r: &mut SkiaRenderer, a: TextContent, b: &str, s: &TextStyle| {
        let x = text_frame(r, "text_bound", a, s.clone(), full, &scene);
        let y = text_frame(r, "text_static", stat(b), s.clone(), full, &scene);
        x == y
    };
    assert!(same(r, sensor(keys::CPU_USAGE), "CPU 42%!", &centered));
    assert!(
        same(r, sensor(keys::GPU_USAGE), "CPU —!", &centered),
        "unavailable dash"
    );
    let clock = TextContent::Clock {
        pattern: "%H:%M %a".into(),
        language: None,
        casing: Default::default(),
    };
    assert_eq!(TIME.hour, 21);
    assert!(same(r, clock, "21:05 Wed", &centered));
    assert!(same(
        r,
        TextContent::Clock {
            pattern: "%a %b".into(),
            language: Some(bezel_core::domain::clock::Language::Italian),
            casing: bezel_core::domain::clock::ClockCase::Upper
        },
        "MER SET",
        &centered
    ));
    assert!(
        !same(r, stat("21:05"), "21:06", &centered),
        "different text differs"
    );

    let bundled = TextStyle {
        font: FontSpec {
            family: "Not Installed".into(),
            asset: Some(AssetRef("font.ttf".into())),
            ..FontSpec::default()
        },
        ..centered.clone()
    };
    let with_font = Scene::empty().asset("font.ttf", FONT.to_vec());
    let a = text_frame(
        r,
        "text_asset_font",
        stat("Bezel"),
        bundled.clone(),
        full,
        &with_font,
    );
    let b = text_frame(
        r,
        "text_family",
        stat("Bezel"),
        centered.clone(),
        full,
        &with_font,
    );
    assert_eq!(a, b, "bundled font asset");
    let fallback = text_frame(
        r,
        "text_missing_font",
        stat("Bezel"),
        bundled,
        full,
        &Scene::empty(),
    );
    assert!(
        ink(&fallback, BLACK, 24).is_some(),
        "missing font falls back"
    );
    let italic = TextStyle {
        font: FontSpec {
            italic: true,
            ..centered.font.clone()
        },
        ..centered
    };
    let slanted = text_frame(
        r,
        "text_italic",
        stat("Bezel"),
        italic,
        full,
        &Scene::empty(),
    );
    assert_ne!(slanted, b, "italic is slanted");
    let nothing = text_frame(
        r,
        "text_empty",
        stat(""),
        style(0.0, HAlign::Left, VAlign::Top),
        full,
        &scene,
    );
    assert!(ink(&nothing, BLACK, 0).is_none());
}

pub(crate) fn images(r: &mut SkiaRenderer) {
    let quadrants = png(8, 8, |x, y| match (x < 4, y < 4) {
        (true, true) => RED,
        (false, true) => GREEN,
        (true, false) => BLUE,
        (false, false) => WHITE,
    });
    let scene = Scene::empty().asset("q.png", quadrants);
    let img = |fit| ElementKind::Image {
        asset: AssetRef("q.png".into()),
        fit,
    };
    let bg = Background::Color(BLACK);
    let t = theme(
        32,
        32,
        bg.clone(),
        vec![element(BoxF::new(8.0, 8.0, 16.0, 16.0), img(Fit::Fill))],
    );
    let f = shot(r, "image_fill", &t, &scene);
    assert_px(&f, 11, 11, RED, 8);
    assert_px(&f, 20, 11, GREEN, 8);
    assert_px(&f, 11, 20, BLUE, 8);
    assert_px(&f, 20, 20, WHITE, 8);
    assert_px(&f, 4, 4, BLACK, 0);
    assert_px(&f, 27, 27, BLACK, 0);

    let mut half = element(BoxF::new(0.0, 0.0, 32.0, 32.0), img(Fit::Fill));
    half.opacity = 0.5;
    let f = shot(
        r,
        "image_opacity",
        &theme(32, 32, bg.clone(), vec![half]),
        &scene,
    );
    assert_px(&f, 4, 4, Rgba::opaque(128, 0, 0), 2);

    let anim = Scene::empty().asset("a.gif", gif(4, 4, &[(RED, 1000), (BLUE, 1000)]));
    let t = theme(
        8,
        8,
        bg.clone(),
        vec![element(
            BoxF::new(0.0, 0.0, 8.0, 8.0),
            ElementKind::Image {
                asset: AssetRef("a.gif".into()),
                fit: Fit::Fill,
            },
        )],
    );
    for (ms, expected) in [
        (0, RED),
        (999, RED),
        (1000, BLUE),
        (2000, RED),
        (59_500, BLUE),
    ] {
        let scene = Scene {
            animation: std::time::Duration::from_millis(ms),
            ..Scene::empty().asset("a.gif", gif(4, 4, &[(RED, 1000), (BLUE, 1000)]))
        };
        let f = render(r, &t, &scene);
        assert_px(&f, 4, 4, expected, 2);
    }
    let f = shot(r, "image_gif", &t, &anim);
    assert_px(&f, 4, 4, RED, 2);

    let missing = theme(
        8,
        8,
        bg,
        vec![element(BoxF::new(0.0, 0.0, 8.0, 8.0), img(Fit::Contain))],
    );
    let f = render(r, &missing, &Scene::empty());
    assert_eq!(
        count(&f, (0, 0, 8, 8), |p| p == BLACK),
        64,
        "missing asset draws nothing"
    );
    assert!(
        r.problems().iter().any(|p| p.contains("q.png is missing")),
        "{:?}",
        r.problems()
    );
}

fn shape_frame(
    r: &mut SkiaRenderer,
    name: &str,
    shape: ShapeKind,
    fill: Option<Paint>,
    stroke: Option<(Rgba, f32)>,
) -> Frame {
    let kind = ElementKind::Shape {
        video_window: false,
        fade: None,
        shape,
        fill,
        stroke,
    };
    let t = theme(
        32,
        32,
        Background::Color(BLACK),
        vec![element(BoxF::new(4.0, 4.0, 24.0, 24.0), kind)],
    );
    shot(r, name, &t, &Scene::empty())
}

pub(crate) fn shapes(r: &mut SkiaRenderer) {
    let square = ShapeKind::Rect { radius: 0.0 };
    let f = shape_frame(r, "shape_rect", square, Some(solid(RED)), None);
    assert_eq!(count(&f, (0, 0, 32, 32), |p| p == RED), 24 * 24);
    assert_px(&f, 3, 3, BLACK, 0);
    assert_px(&f, 28, 28, BLACK, 0);

    let f = shape_frame(
        r,
        "shape_rounded",
        ShapeKind::Rect { radius: 8.0 },
        Some(solid(RED)),
        None,
    );
    assert_px(&f, 4, 4, BLACK, 0);
    assert_px(&f, 16, 4, RED, 0);
    assert_px(&f, 16, 16, RED, 0);
    let corner_loss = 24 * 24 - count(&f, (0, 0, 32, 32), |p| p.r > 127);
    assert!(
        (45..=65).contains(&corner_loss),
        "4 corners of r=8 lose ~55 px, lost {corner_loss}"
    );

    let f = shape_frame(
        r,
        "shape_ellipse",
        ShapeKind::Ellipse,
        Some(solid(RED)),
        None,
    );
    assert_px(&f, 16, 16, RED, 0);
    assert_px(&f, 16, 5, RED, 0);
    assert_px(&f, 5, 5, BLACK, 0);
    let area = count(&f, (0, 0, 32, 32), |p| p.r > 127) as f32;
    assert!((area - 452.4).abs() < 12.0, "π·12² ≈ 452, got {area}");

    let f = shape_frame(r, "shape_outline", square, None, Some((WHITE, 2.0)));
    assert_px(&f, 4, 16, WHITE, 0);
    assert_px(&f, 5, 16, WHITE, 0);
    assert_px(&f, 6, 16, BLACK, 0);
    assert_px(&f, 16, 16, BLACK, 0);
    assert_px(&f, 3, 16, BLACK, 0);

    let gradient = Paint::Linear {
        angle: 90.0,
        stops: vec![(0.0, RED), (1.0, BLUE)],
    };
    let f = shape_frame(
        r,
        "shape_gradient",
        square,
        Some(gradient),
        Some((WHITE, 1.0)),
    );
    assert_px(&f, 4, 16, WHITE, 0);
    let (top, bottom) = (px(&f, 16, 6), px(&f, 16, 25));
    assert!(
        top.r > 200 && top.b < 55 && bottom.b > 200 && bottom.r < 55,
        "{top:?} {bottom:?}"
    );
}

fn bar(direction: Direction, segments: Option<Segments>, radius: f32) -> ElementKind {
    ElementKind::Bar {
        binding: binding(keys::CPU_USAGE),
        direction,
        fill: solid(GREEN),
        track: Some(solid(GRAY)),
        radius,
        segments,
    }
}

fn bar_frame(
    r: &mut SkiaRenderer,
    name: &str,
    kind: ElementKind,
    value: Option<f64>,
    (w, h): (u32, u32),
    frame: BoxF,
) -> Frame {
    let scene = match value {
        Some(v) => Scene::empty().with(keys::CPU_USAGE, v),
        None => Scene::empty(),
    };
    shot(
        r,
        name,
        &theme(w, h, Background::Color(BLACK), vec![element(frame, kind)]),
        &scene,
    )
}

pub(crate) fn bars(r: &mut SkiaRenderer) {
    let wide = ((64, 16), BoxF::new(0.0, 4.0, 64.0, 8.0));
    let tall = ((16, 64), BoxF::new(4.0, 0.0, 8.0, 64.0));
    let f = bar_frame(
        r,
        "bar_ltr",
        bar(Direction::LeftToRight, None, 0.0),
        Some(50.0),
        wide.0,
        wide.1,
    );
    assert_eq!(count(&f, (0, 8, 64, 9), |p| p == GREEN), 32);
    assert_eq!(count(&f, (0, 8, 64, 9), |p| p == GRAY), 32);
    assert_px(&f, 10, 2, BLACK, 0);
    let f = bar_frame(
        r,
        "bar_rtl",
        bar(Direction::RightToLeft, None, 0.0),
        Some(25.0),
        wide.0,
        wide.1,
    );
    assert_px(&f, 60, 8, GREEN, 0);
    assert_px(&f, 40, 8, GRAY, 0);
    let f = bar_frame(
        r,
        "bar_btt",
        bar(Direction::BottomToTop, None, 0.0),
        Some(25.0),
        tall.0,
        tall.1,
    );
    assert_px(&f, 8, 60, GREEN, 0);
    assert_px(&f, 8, 20, GRAY, 0);
    assert_eq!(count(&f, (8, 0, 9, 64), |p| p == GREEN), 16);
    let f = bar_frame(
        r,
        "bar_ttb",
        bar(Direction::TopToBottom, None, 0.0),
        Some(25.0),
        tall.0,
        tall.1,
    );
    assert_px(&f, 8, 4, GREEN, 0);
    assert_px(&f, 8, 40, GRAY, 0);
    let f = bar_frame(
        r,
        "bar_unavailable",
        bar(Direction::LeftToRight, None, 0.0),
        None,
        wide.0,
        wide.1,
    );
    assert_eq!(count(&f, (0, 4, 64, 12), |p| p == GRAY), 64 * 8);

    let blocks = Some(Segments { count: 4, gap: 4.0 });
    let f = bar_frame(
        r,
        "bar_segments",
        bar(Direction::LeftToRight, blocks, 0.0),
        Some(60.0),
        wide.0,
        wide.1,
    );
    for (x, expected) in [
        (6, GREEN),
        (15, BLACK),
        (20, GREEN),
        (36, GREEN),
        (40, GRAY),
        (49, BLACK),
        (55, GRAY),
    ] {
        assert_px(&f, x, 8, expected, 0);
    }
    let f = bar_frame(
        r,
        "bar_rounded",
        bar(Direction::LeftToRight, None, 4.0),
        Some(50.0),
        wide.0,
        wide.1,
    );
    assert_px(&f, 10, 8, GREEN, 0);
    assert!(px(&f, 0, 4).g < 128, "rounded corner is cut");
    assert_px(&f, 50, 8, GRAY, 0);
}

fn ring(
    start: f32,
    sweep: f32,
    clockwise: bool,
    cap: Cap,
    segments: Option<Segments>,
) -> ElementKind {
    ElementKind::Ring {
        test_full: false,
        binding: binding(keys::CPU_USAGE),
        start_angle: start,
        sweep,
        thickness: 8.0,
        clockwise,
        fill: solid(GREEN),
        track: Some(solid(GRAY)),
        cap,
        segments,
    }
}

/// The pixel on the ring's center line at `angle`.
fn on_ring(f: &Frame, angle: f32) -> Rgba {
    let (x, y) = crate::path::polar(32.0, 32.0, 28.0, angle);
    px(f, x as u32, y as u32)
}

fn ring_frame(r: &mut SkiaRenderer, name: &str, kind: ElementKind, value: Option<f64>) -> Frame {
    bar_frame(
        r,
        name,
        kind,
        value,
        (64, 64),
        BoxF::new(0.0, 0.0, 64.0, 64.0),
    )
}

pub(crate) fn rings(r: &mut SkiaRenderer) {
    let f = ring_frame(
        r,
        "ring_cw",
        ring(0.0, 360.0, true, Cap::Butt, None),
        Some(25.0),
    );
    assert!(close(on_ring(&f, 45.0), GREEN, 0) && close(on_ring(&f, 80.0), GREEN, 0));
    assert!(close(on_ring(&f, 180.0), GRAY, 0) && close(on_ring(&f, 270.0), GRAY, 0));
    assert_px(&f, 32, 32, BLACK, 0);
    assert_px(&f, 0, 0, BLACK, 0);
    let f = ring_frame(
        r,
        "ring_ccw",
        ring(0.0, 360.0, false, Cap::Butt, None),
        Some(25.0),
    );
    assert!(close(on_ring(&f, -45.0), GREEN, 0) && close(on_ring(&f, 45.0), GRAY, 0));

    let f = ring_frame(
        r,
        "ring_gauge",
        ring(-120.0, 240.0, true, Cap::Butt, None),
        Some(50.0),
    );
    assert!(close(on_ring(&f, -60.0), GREEN, 0) && close(on_ring(&f, 60.0), GRAY, 0));
    assert!(close(on_ring(&f, 180.0), BLACK, 0), "outside the sweep");
    let f = ring_frame(
        r,
        "ring_unavailable",
        ring(-120.0, 240.0, true, Cap::Butt, None),
        None,
    );
    assert!(close(on_ring(&f, -60.0), GRAY, 0));

    let blocks = Some(Segments {
        count: 4,
        gap: 10.0,
    });
    let f = ring_frame(
        r,
        "ring_segments",
        ring(0.0, 360.0, true, Cap::Butt, blocks),
        Some(50.0),
    );
    for (angle, expected) in [
        (40.0, GREEN),
        (85.0, BLACK),
        (130.0, GREEN),
        (220.0, GRAY),
        (355.0, BLACK),
    ] {
        assert!(close(on_ring(&f, angle), expected, 0), "at {angle}°");
    }
    let butt = ring_frame(
        r,
        "ring_butt",
        ring(0.0, 90.0, true, Cap::Butt, None),
        Some(100.0),
    );
    let round = ring_frame(
        r,
        "ring_round",
        ring(0.0, 90.0, true, Cap::Round, None),
        Some(100.0),
    );
    assert!(close(on_ring(&butt, -5.0), BLACK, 0) && close(on_ring(&round, -5.0), GREEN, 0));
    assert!(close(on_ring(&round, 45.0), GREEN, 0));
}

fn needle(asset: Option<&str>, pivot: (f32, f32)) -> ElementKind {
    ElementKind::Needle {
        binding: binding(keys::CPU_USAGE),
        asset: asset.map(|a| AssetRef(a.into())),
        pivot,
        start_angle: -90.0,
        sweep: 180.0,
        color: WHITE,
        width: 3.0,
    }
}

pub(crate) fn needles(r: &mut SkiaRenderer) {
    let whole = BoxF::new(0.0, 0.0, 64.0, 64.0);
    let lit = |f: &Frame, x, y| px(f, x, y).r > 200;
    let f = bar_frame(
        r,
        "needle_up",
        needle(None, (0.5, 0.5)),
        Some(50.0),
        (64, 64),
        whole,
    );
    assert!(lit(&f, 32, 10) && !lit(&f, 32, 54) && !lit(&f, 10, 32));
    let f = bar_frame(
        r,
        "needle_right",
        needle(None, (0.5, 0.5)),
        Some(100.0),
        (64, 64),
        whole,
    );
    assert!(lit(&f, 54, 32) && !lit(&f, 32, 10));
    let f = bar_frame(
        r,
        "needle_rest",
        needle(None, (0.5, 0.5)),
        None,
        (64, 64),
        whole,
    );
    assert!(lit(&f, 10, 32) && !lit(&f, 54, 32));

    let scene = Scene::empty()
        .with(keys::CPU_USAGE, 100.0)
        .asset("needle.png", png(4, 32, |_, _| WHITE));
    let kind = needle(Some("needle.png"), (0.5, 1.0));
    let t = theme(
        64,
        64,
        Background::Color(BLACK),
        vec![element(BoxF::new(30.0, 0.0, 4.0, 32.0), kind)],
    );
    let f = shot(r, "needle_image", &t, &scene);
    assert!(lit(&f, 50, 31) && lit(&f, 50, 32) && lit(&f, 62, 32));
    assert!(!lit(&f, 32, 16) && !lit(&f, 50, 28) && !lit(&f, 50, 36));
}

fn graph(style: GraphStyle, history: u16, autoscale: bool) -> ElementKind {
    ElementKind::Graph {
        binding: binding(keys::CPU_USAGE),
        history,
        style,
        color: WHITE,
        fill: None,
        line_width: 2.0,
        autoscale,
    }
}

fn graph_frame(
    r: &mut SkiaRenderer,
    name: &str,
    kind: ElementKind,
    values: &[Option<f64>],
) -> Frame {
    let t = theme(
        64,
        32,
        Background::Color(BLACK),
        vec![element(BoxF::new(0.0, 0.0, 64.0, 32.0), kind)],
    );
    shot(
        r,
        name,
        &t,
        &Scene::empty().history(keys::CPU_USAGE, values),
    )
}

pub(crate) fn graphs(r: &mut SkiaRenderer) {
    let values = [Some(0.0), Some(100.0), None, Some(50.0), Some(50.0)];
    let f = graph_frame(r, "graph_line", graph(GraphStyle::Line, 5, false), &values);
    assert_px(&f, 56, 16, WHITE, 8);
    assert_px(&f, 8, 16, WHITE, 64);
    assert_px(&f, 32, 16, BLACK, 0);
    assert_px(&f, 40, 16, BLACK, 0);
    assert_px(&f, 56, 25, BLACK, 0);

    let f = graph_frame(r, "graph_area", graph(GraphStyle::Area, 5, false), &values);
    assert_px(&f, 56, 16, WHITE, 8);
    assert_px(&f, 56, 25, Rgba::opaque(72, 72, 72), 1);
    assert_px(&f, 56, 8, BLACK, 0);
    assert_px(&f, 32, 25, BLACK, 0);

    let bars = [Some(25.0), Some(50.0), Some(100.0), None];
    let f = graph_frame(r, "graph_bars", graph(GraphStyle::Bars, 4, false), &bars);
    for (x, y, expected) in [
        (8, 28, WHITE),
        (8, 20, BLACK),
        (24, 20, WHITE),
        (40, 2, WHITE),
        (56, 28, BLACK),
        (15, 28, BLACK),
    ] {
        assert_px(&f, x, y, expected, 0);
    }

    let f = graph_frame(
        r,
        "graph_autoscale",
        graph(GraphStyle::Line, 2, true),
        &[Some(40.0), Some(60.0)],
    );
    let (_, top, _, bottom) = ink(&f, BLACK, 32).expect("ink");
    assert!(
        top <= 5 && bottom >= 26,
        "fills the height: {top}..{bottom}"
    );
    let f = graph_frame(
        r,
        "graph_empty",
        graph(GraphStyle::Line, 5, true),
        &[None, None],
    );
    assert!(ink(&f, BLACK, 0).is_none());
}

pub(crate) fn composition(r: &mut SkiaRenderer) {
    let rect = |fill: Rgba, frame: BoxF| {
        element(
            frame,
            ElementKind::Shape {
                video_window: false,
                fade: None,
                shape: ShapeKind::Rect { radius: 0.0 },
                fill: Some(solid(fill)),
                stroke: None,
            },
        )
    };
    let mut hidden = rect(BLUE, BoxF::new(0.0, 0.0, 16.0, 16.0));
    hidden.visible = false;
    let mut faded = rect(WHITE, BoxF::new(12.0, 12.0, 4.0, 4.0));
    faded.opacity = 0.5;
    let elements = vec![
        rect(RED, BoxF::new(0.0, 0.0, 10.0, 10.0)),
        rect(GREEN, BoxF::new(5.0, 5.0, 10.0, 10.0)),
        hidden,
        faded,
        rect(RED, BoxF::new(-8.0, 14.0, 10.0, 10.0)),
    ];
    let f = shot(
        r,
        "composition",
        &theme(16, 16, Background::Color(BLACK), elements),
        &Scene::empty(),
    );
    assert_px(&f, 2, 2, RED, 0);
    assert_px(&f, 7, 7, GREEN, 0);
    assert_px(&f, 13, 2, BLACK, 0);
    assert_px(&f, 13, 13, Rgba::opaque(128, 255, 128), 1);
    assert_px(&f, 1, 15, RED, 0);
    assert_px(&f, 2, 15, BLACK, 0);

    let translucent = Rgba { a: 128, ..RED };
    let t = theme(
        4,
        4,
        Background::Color(CLEAR),
        vec![rect(translucent, BoxF::new(0.0, 0.0, 4.0, 4.0))],
    );
    let f = render(r, &t, &Scene::empty());
    assert_px(&f, 1, 1, translucent, 1);
    let f = render(
        r,
        &theme(0, 0, Background::Color(BLACK), vec![]),
        &Scene::empty(),
    );
    assert!(f.as_rgba().is_empty());
}

#[test]
fn weather_text_and_vector_icons_render_without_external_assets() {
    fn weather_frame(
        r: &mut SkiaRenderer,
        name: &str,
        content: TextContent,
        style: TextStyle,
        frame: BoxF,
        scene: &Scene,
    ) -> Frame {
        shot(
            r,
            name,
            &theme(
                300,
                120,
                Background::Color(BLACK),
                vec![text(content, style, frame)],
            ),
            scene,
        )
    }
    use bezel_core::domain::clock::Language;
    use bezel_core::domain::weather::Weather;
    let mut renderer = crate::testkit::renderer();
    let mut w = Weather {
        city: "Roma".into(),
        latitude: 41.9,
        longitude: 12.5,
        language: Some(Language::Italian),
        fahrenheit: false,
        show_icon: false,
        icon_style: Default::default(),
        icon_gap: None,
        icon_size: None,
    };
    let [temperature, code] = w.keys();
    let scene = Scene::empty()
        .with(temperature.as_str(), 20.0)
        .with(code.as_str(), 3.0);
    let full = BoxF::new(0.0, 0.0, 300.0, 120.0);
    let style = style(24.0, HAlign::Left, VAlign::Middle);
    let actual = weather_frame(
        &mut renderer,
        "weather_text",
        TextContent::Weather(w.clone()),
        style.clone(),
        full,
        &scene,
    );
    let expected = weather_frame(
        &mut renderer,
        "weather_expected",
        stat("Roma\n20\u{00b0}C\nNuvoloso"),
        style.clone(),
        full,
        &scene,
    );
    assert_eq!(actual, expected);
    w.show_icon = true;
    let cloudy = weather_frame(
        &mut renderer,
        "weather_cloudy",
        TextContent::Weather(w.clone()),
        style.clone(),
        full,
        &scene,
    );
    assert_ne!(cloudy, actual);
    w.icon_style = bezel_core::domain::weather::IconStyle::Filled;
    w.icon_size = Some(64.0);
    w.icon_gap = Some(20.0);
    let configured = weather_frame(
        &mut renderer,
        "weather_configured",
        TextContent::Weather(w.clone()),
        style.clone(),
        full,
        &scene,
    );
    assert_ne!(configured, cloudy);
    let scene = scene.with(code.as_str(), 0.0);
    let sunny = weather_frame(
        &mut renderer,
        "weather_sunny",
        TextContent::Weather(w),
        style,
        full,
        &scene,
    );
    assert_ne!(sunny, cloudy);
}

#[test]
fn arc_gradient_full_test_direction_midpoint_caps_and_segments() {
    let mut r = crate::testkit::renderer();
    let make = |cw, test, mid, cap, segments| {
        let mut k = ring(0.0, 270.0, cw, cap, segments);
        if let ElementKind::Ring {
            fill, test_full, ..
        } = &mut k
        {
            *fill = Paint::Arc {
                start: BLUE,
                end: RED,
                transition: mid,
            };
            *test_full = test;
        }
        k
    };
    for cw in [true, false] {
        let dir = if cw { 1.0 } else { -1.0 };
        let full = ring_frame(
            &mut r,
            "ring_gradient_test_full",
            make(cw, true, 0.5, Cap::Butt, None),
            None,
        );
        let normal = ring_frame(
            &mut r,
            "ring_gradient_sensor",
            make(cw, false, 0.5, Cap::Butt, None),
            Some(40.0),
        );
        assert!(close(on_ring(&normal, dir * 180.0), GRAY, 0));
        assert!(on_ring(&full, dir * 180.0).r > on_ring(&full, dir * 180.0).b);
        assert!(close(
            on_ring(&full, dir * 135.0),
            Rgba::opaque(128, 0, 128),
            8
        ));
        let shifted = ring_frame(
            &mut r,
            "ring_gradient_midpoint",
            make(cw, true, 0.25, Cap::Butt, None),
            Some(0.0),
        );
        assert!(on_ring(&shifted, dir * 135.0).r > on_ring(&full, dir * 135.0).r + 30);
    }
    let caps = ring_frame(
        &mut r,
        "ring_gradient_caps",
        make(true, true, 0.5, Cap::Round, None),
        Some(0.0),
    );
    assert!(close(on_ring(&caps, -5.0), BLUE, 4));
    assert!(close(on_ring(&caps, 275.0), RED, 4));
    let segmented = ring_frame(
        &mut r,
        "ring_gradient_segments",
        make(
            true,
            true,
            0.5,
            Cap::Butt,
            Some(Segments {
                count: 3,
                gap: 10.0,
            }),
        ),
        Some(0.0),
    );
    assert!(close(on_ring(&segmented, 85.0), BLACK, 0));
    assert!(on_ring(&segmented, 220.0).r > 200);
    // Ordinary rings also support testing, and resume their real value when off.
    let mut k = ring(0.0, 360.0, true, Cap::Butt, None);
    if let ElementKind::Ring { test_full, .. } = &mut k {
        *test_full = true;
    }
    let full = ring_frame(&mut r, "ring_solid_test_full", k, Some(0.0));
    assert!(close(on_ring(&full, 270.0), GREEN, 0));
}

#[test]
fn device_video_windows_clear_alpha_with_border_layer_order_and_fade() {
    use bezel_core::domain::{
        gradient::Fade,
        storage::{RemotePath, Repeat},
    };
    let mut r = crate::testkit::renderer();
    let window = |shape, fade| {
        element(
            BoxF::new(8.0, 8.0, 48.0, 48.0),
            ElementKind::Shape {
                shape,
                video_window: true,
                fade,
                fill: Some(solid(RED)),
                stroke: Some((WHITE, 2.0)),
            },
        )
    };
    let background = Background::DeviceVideo {
        path: RemotePath::parse("sd/video/demo.mp4").unwrap(),
        repeat: Repeat::Loop,
        color: GREEN,
    };
    let mut t = theme(
        64,
        64,
        background,
        vec![window(ShapeKind::Rect { radius: 8.0 }, None)],
    );
    let scene = Scene::empty();
    // Preview must not poison the cached layer sent to the physical screen.
    let preview = render(&mut r, &t, &scene);
    assert_eq!(px(&preview, 32, 32).a, 255);
    let f = render_over(&mut r, &t, &scene, Backdrop::OnDevice);
    assert_px(&f, 32, 32, CLEAR, 0);
    assert_px(&f, 2, 2, GREEN, 0);
    assert_px(&f, 8, 8, GREEN, 0);
    assert_px(&f, 9, 32, WHITE, 0);
    let rect = element(
        BoxF::new(24.0, 24.0, 16.0, 16.0),
        ElementKind::Shape {
            shape: ShapeKind::Rect { radius: 0.0 },
            fill: Some(solid(RED)),
            stroke: None,
            video_window: false,
            fade: None,
        },
    );
    t.elements.push(rect.clone());
    let f = render_over(&mut r, &t, &scene, Backdrop::OnDevice);
    assert_px(&f, 32, 32, RED, 0);
    t.elements = vec![rect, window(ShapeKind::Ellipse, None)];
    let f = render_over(&mut r, &t, &scene, Backdrop::OnDevice);
    assert_px(&f, 32, 32, CLEAR, 0);
    assert_px(&f, 8, 8, GREEN, 0);
    t.elements = vec![window(
        ShapeKind::Rect { radius: 0.0 },
        Some(Fade {
            angle: 0.0,
            start: 1.0,
            end: 0.0,
        }),
    )];
    let f = render_over(&mut r, &t, &scene, Backdrop::OnDevice);
    assert!(px(&f, 16, 32).a < 60 && px(&f, 48, 32).a > 200);
    t.elements[0].opacity = 0.5;
    let f = render_over(&mut r, &t, &scene, Backdrop::OnDevice);
    assert!(px(&f, 16, 32).a > 140);
    t.elements[0].visible = false;
    let f = render_over(&mut r, &t, &scene, Backdrop::OnDevice);
    assert_px(&f, 32, 32, CLEAR, 0); // No visible windows: video fills the screen.
}

#[test]
fn ordinary_shape_linear_transparency_blends_and_rotates() {
    use bezel_core::domain::gradient::Fade;
    let mut r = crate::testkit::renderer();
    let mut t = theme(
        64,
        32,
        Background::Color(CLEAR),
        vec![element(
            BoxF::new(0.0, 0.0, 64.0, 32.0),
            ElementKind::Shape {
                shape: ShapeKind::Rect { radius: 0.0 },
                fill: Some(solid(RED)),
                stroke: None,
                video_window: false,
                fade: Some(Fade {
                    angle: 0.0,
                    start: 1.0,
                    end: 0.0,
                }),
            },
        )],
    );
    let f = render(&mut r, &t, &Scene::empty());
    assert!(px(&f, 0, 16).a > 250 && px(&f, 63, 16).a < 5);
    assert!((i16::from(px(&f, 32, 16).a) - 128).abs() < 5);
    if let ElementKind::Shape { fade, .. } = &mut t.elements[0].kind {
        fade.as_mut().unwrap().angle = 90.0;
    }
    let f = render(&mut r, &t, &Scene::empty());
    assert!(px(&f, 32, 0).a > 250 && px(&f, 32, 31).a < 5);
}
