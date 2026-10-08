//! Animated images on their own cadence (T-7.11): the runtime says when the
//! next frame is due (the next sample, or a visible GIF's next frame, at
//! most 30 a second), samples the sensors once per refresh only, and draws
//! every frame as the images are at that moment, so a slow screen skips
//! frames instead of falling behind. The sensors and the screen are the
//! adapters' fakes; the renderer is a recording double defined here (no
//! adapter ships a fake of it). The clock is the test's: no real waits.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::Result;
use bezel_core::app::{ThemeRuntime, open_screen};
use bezel_core::domain::animation::{MIN_FRAME_STEP, Timeline};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::theme::{AssetRef, BoxF, Element, ElementId, ElementKind, Fit, Theme};
use bezel_core::ports::{FrameRenderer, RenderContext};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_sensors::FakeSensors;

const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 0,
    weekday: 2,
};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A renderer that records the animation clock of every frame and says
/// that `*.gif` assets are animations of four frames of `delay` each.
struct Clockwork {
    delay: Duration,
    drawn: Vec<Duration>,
    asked: Vec<String>,
}

impl Clockwork {
    fn new(delay: Duration) -> Self {
        Self {
            delay,
            drawn: Vec::new(),
            asked: Vec::new(),
        }
    }
}

impl FrameRenderer for Clockwork {
    fn render(
        &mut self,
        theme: &Theme,
        _: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame> {
        self.drawn.push(context.animation);
        Ok(Frame::filled(theme.canvas, Rgba::BLACK))
    }

    fn animation(&mut self, asset: &AssetRef, _: &BTreeMap<AssetRef, Vec<u8>>) -> Option<Timeline> {
        self.asked.push(asset.0.clone());
        asset
            .0
            .ends_with(".gif")
            .then(|| Timeline::new(vec![self.delay; 4]))
            .flatten()
    }
}

fn image(id: u32, asset: &str) -> Element {
    Element {
        card: None,
        card_member: None,
        id: ElementId(id),
        name: asset.into(),
        frame: BoxF::new(100.0, 100.0, 64.0, 64.0),
        opacity: 1.0,
        visible: true,
        locked: false,
        kind: ElementKind::Image {
            asset: AssetRef(asset.into()),
            fit: Fit::Fill,
        },
    }
}

/// A horizontal 8.8" theme refreshed every second with `elements`.
fn theme(elements: Vec<Element>) -> Theme {
    let mut theme = Theme::blank("anim", Size::new(480, 1920), Orientation::Landscape);
    theme.elements = elements;
    theme
}

fn runtime(elements: Vec<Element>) -> ThemeRuntime {
    ThemeRuntime::new(theme(elements), BTreeMap::new(), Language::English)
}

/// Runs a live loop for `until` on the test's clock, each send taking
/// `send`: when each frame was drawn, and how many samples were taken.
fn live_loop(
    rt: &mut ThemeRuntime,
    renderer: &mut Clockwork,
    send: Duration,
    until: Duration,
) -> (Vec<Duration>, usize) {
    let connector = FakeConnector::default();
    let mut screen = open_screen(&FakeBus::turing_88(), &connector, None).expect("screen");
    screen
        .set_orientation(Orientation::Landscape)
        .expect("turns");
    let mut sensors = FakeSensors::demo();
    let mut now = Duration::ZERO;
    let mut drawn = Vec::new();
    while now < until {
        let frame = rt
            .live_frame(&mut sensors, renderer, TIME, now)
            .expect("frame");
        drawn.push(now);
        screen.present(&frame).expect("shown");
        now += send;
        now = now.max(rt.next_due());
    }
    assert_eq!(connector.log().frames.len(), drawn.len());
    (drawn, sensors.samples_taken())
}

#[test]
fn without_an_animation_frames_follow_the_refresh() {
    let mut rt = runtime(vec![image(1, "assets/logo.png")]);
    let mut r = Clockwork::new(ms(100));
    assert_eq!(rt.next_due(), Duration::ZERO, "the first frame at once");
    let (drawn, samples) = live_loop(&mut rt, &mut r, Duration::ZERO, ms(3_000));
    assert_eq!(drawn, [ms(0), ms(1_000), ms(2_000)]);
    assert_eq!(samples, 3, "one sample per frame");
    assert_eq!(rt.next_due(), ms(3_000));
    assert_eq!(rt.next_animation_change(ms(0)), None);
    assert_eq!(r.asked, ["assets/logo.png"], "asked once, a still image");
}

#[test]
fn a_visible_gif_brings_frames_at_its_own_times() {
    let mut rt = runtime(vec![
        image(1, "assets/logo.png"),
        image(2, "assets/spin.gif"),
    ]);
    let mut r = Clockwork::new(ms(100));
    let (drawn, samples) = live_loop(&mut rt, &mut r, Duration::ZERO, ms(2_000));
    let every_100: Vec<Duration> = (0..20).map(|n| ms(n * 100)).collect();
    assert_eq!(drawn, every_100, "the GIF's frame times");
    assert_eq!(r.drawn, every_100, "each frame draws the GIF of its moment");
    assert_eq!(
        samples, 2,
        "sensors still once per second, not per GIF frame"
    );
    assert_eq!(rt.next_due(), ms(2_000));
    assert_eq!(r.asked.len(), 2, "each image asked once");
}

#[test]
fn a_fast_gif_is_drawn_at_most_thirty_times_a_second() {
    let mut rt = runtime(vec![image(1, "assets/fast.gif")]);
    let mut r = Clockwork::new(ms(10));
    let (drawn, _) = live_loop(&mut rt, &mut r, Duration::ZERO, ms(990));
    assert_eq!(drawn.len(), 30, "a 100 fps GIF");
    assert!(
        drawn.windows(2).all(|w| w[1] - w[0] >= MIN_FRAME_STEP),
        "{drawn:?}"
    );
}

#[test]
fn a_slow_send_skips_ahead_instead_of_falling_behind() {
    let mut rt = runtime(vec![image(1, "assets/spin.gif")]);
    let mut r = Clockwork::new(ms(100));
    // Each frame takes 350 ms to reach the screen: the frame after one is
    // due at once, and shows the GIF as it is then (frame 3), never 1 and 2.
    let (drawn, samples) = live_loop(&mut rt, &mut r, ms(350), ms(1_400));
    assert_eq!(drawn, [ms(0), ms(350), ms(700), ms(1_050)]);
    assert_eq!(r.drawn, drawn, "real time, no backlog");
    assert_eq!(samples, 2, "at 0 and when the second was due (1050 ms)");
    let timeline = Timeline::new(vec![ms(100); 4]).expect("animated");
    let shown: Vec<usize> = drawn.iter().map(|t| timeline.frame_at(*t)).collect();
    assert_eq!(shown, [0, 3, 3, 2]);
}

#[test]
fn a_hidden_gif_does_not_animate() {
    let mut hidden = image(1, "assets/spin.gif");
    hidden.visible = false;
    let mut clear = image(2, "assets/fade.gif");
    clear.opacity = 0.0;
    let mut away = image(3, "assets/away.gif");
    away.frame = BoxF::new(1920.0, 0.0, 64.0, 64.0);
    let mut rt = runtime(vec![hidden, clear, away]);
    let mut r = Clockwork::new(ms(100));
    let (drawn, _) = live_loop(&mut rt, &mut r, Duration::ZERO, ms(2_000));
    assert_eq!(drawn, [ms(0), ms(1_000)], "the theme's refresh only");
    assert!(r.asked.is_empty(), "nothing shown to ask about");

    // Shown by an edit: it animates from the next frame on.
    let mut shown = rt.theme().clone();
    shown.elements[0].visible = true;
    rt.replace_theme(shown);
    let mut sensors = FakeSensors::demo();
    rt.live_frame(&mut sensors, &mut r, TIME, ms(2_000))
        .expect("frame");
    assert_eq!(rt.next_due(), ms(2_100));
}

#[test]
fn previews_say_when_their_animation_changes_and_leave_the_screen_alone() {
    let mut rt = runtime(vec![image(1, "assets/spin.gif")]);
    let mut r = Clockwork::new(ms(100));
    let (_, next) = rt.preview(&mut r, TIME, ms(1_234)).expect("preview");
    assert_eq!(next, Some(ms(1_300)));
    assert_eq!(r.drawn, [ms(1_234)]);
    assert_eq!(rt.next_due(), Duration::ZERO, "no screen frame drawn yet");
    // `render_with` draws at the last clock given.
    rt.render_with(&mut r, TIME, Default::default())
        .expect("frame");
    assert_eq!(r.drawn[1], ms(1_234));

    let mut still = runtime(vec![image(1, "assets/logo.png")]);
    let (_, next) = still.preview(&mut r, TIME, ms(5)).expect("preview");
    assert_eq!(next, None);
}

#[test]
fn new_assets_are_asked_about_again() {
    let mut rt = runtime(vec![image(1, "assets/spin.gif")]);
    let mut r = Clockwork::new(ms(100));
    rt.preview(&mut r, TIME, ms(0)).expect("preview");
    rt.preview(&mut r, TIME, ms(40)).expect("preview");
    assert_eq!(r.asked.len(), 1, "remembered");
    rt.add_asset(AssetRef("assets/spin.gif".into()), vec![1, 2, 3]);
    rt.preview(&mut r, TIME, ms(80)).expect("preview");
    let assets = rt.assets().clone();
    rt.replace(rt.theme().clone(), assets);
    rt.preview(&mut r, TIME, ms(120)).expect("preview");
    assert_eq!(r.asked.len(), 3, "new bytes, then a new document");
}

#[test]
fn the_refresh_stays_within_the_callers_limit() {
    let mut t = theme(Vec::new());
    t.refresh_seconds = 30.0;
    let mut rt = ThemeRuntime::new(t, BTreeMap::new(), Language::English);
    assert_eq!(rt.refresh(), Duration::from_secs(30));
    rt.limit_refresh(2.0);
    assert_eq!(rt.refresh(), Duration::from_secs(2));
    let mut sensors = FakeSensors::demo();
    assert!(rt.sample_due(Duration::ZERO));
    assert!(rt.sample_on_time(&mut sensors, ms(500)).expect("sample"));
    assert_eq!(rt.next_sample(), ms(2_500));
    assert!(!rt.sample_on_time(&mut sensors, ms(2_000)).expect("early"));
    // Far behind: the count starts again from now.
    assert!(rt.sample_on_time(&mut sensors, ms(9_000)).expect("late"));
    assert_eq!(rt.next_sample(), ms(11_000));
    assert_eq!(sensors.samples_taken(), 2);
}

#[test]
fn sharing_preview_keeps_the_animation_boundary_and_live_cadence() {
    let theme = theme(vec![image(1, "a.gif")]);
    let mut runtime = ThemeRuntime::new(theme, BTreeMap::new(), Language::English);
    runtime.reuse_recent_frames(true);
    let mut renderer = Clockwork::new(ms(40));
    runtime
        .sample_on_time(&mut FakeSensors::demo(), Duration::ZERO)
        .unwrap();
    runtime.preview(&mut renderer, TIME, ms(25)).unwrap();
    runtime
        .render_at(&mut renderer, TIME, ms(30), Duration::ZERO)
        .unwrap();
    assert!(runtime.render_reused());
    // Reuse keeps the original drawing clock; it doesn't defer the next live frame.
    assert_eq!(runtime.next_due(), ms(25) + MIN_FRAME_STEP);
    runtime.preview(&mut renderer, TIME, ms(40)).unwrap();
    assert!(!runtime.render_reused());
    assert_eq!(renderer.drawn, vec![ms(25), ms(40)]);
}
