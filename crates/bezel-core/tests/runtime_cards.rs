//! Timed playback uses monotonic runtime clocks and never edits saved faces.
#![allow(clippy::expect_used)]
use bezel_core::{
    Result,
    app::ThemeRuntime,
    domain::{
        clock::{Language, LocalTime},
        frame::{Frame, Rgba},
        geometry::{Orientation, Size},
        theme::{AssetRef, BoxF, Card, Element, ElementId, ElementKind, ShapeKind, Theme},
    },
    ports::{FrameRenderer, RenderContext},
};
use bezel_sensors::FakeSensors;
use std::{collections::BTreeMap, time::Duration};
const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 10,
    day: 8,
    hour: 12,
    minute: 0,
    second: 0,
    weekday: 3,
};
fn seconds(n: u64) -> Duration {
    Duration::from_secs(n)
}
#[derive(Default)]
struct Recorder {
    faces: Vec<usize>,
}
impl FrameRenderer for Recorder {
    fn render(
        &mut self,
        theme: &Theme,
        _: &BTreeMap<AssetRef, Vec<u8>>,
        _: RenderContext<'_>,
    ) -> Result<Frame> {
        self.faces
            .push(theme.elements[0].card.as_ref().expect("card").active_face);
        Ok(Frame::filled(theme.canvas, Rgba::BLACK))
    }
}
fn theme() -> Theme {
    let mut theme = Theme::blank("timed", Size::new(320, 480), Orientation::Portrait);
    theme.refresh_seconds = 60.0;
    theme.elements.push(Element {
        id: ElementId(1),
        name: "Card".into(),
        frame: BoxF::new(0.0, 0.0, 100.0, 100.0),
        opacity: 1.0,
        visible: true,
        locked: false,
        card_member: None,
        card: Some(Card {
            faces: vec!["A".into(), "B".into(), "C".into()],
            active_face: 0,
            rotation_seconds: Some(5),
            transition: None,
        }),
        kind: ElementKind::Shape {
            shape: ShapeKind::Rect { radius: 0.0 },
            fill: None,
            stroke: None,
            video_window: false,
            fade: None,
        },
    });
    theme
}
fn draw(rt: &mut ThemeRuntime, r: &mut Recorder, at: u64) {
    rt.render_at(r, TIME, seconds(at), Duration::ZERO)
        .expect("frame");
}
#[test]
fn timed_faces_schedule_between_sensor_samples_and_do_not_edit_saved_theme() {
    let saved = theme();
    let mut rt = ThemeRuntime::new(saved.clone(), BTreeMap::new(), Language::English);
    let mut r = Recorder::default();
    let mut sensors = FakeSensors::demo();
    rt.sample_on_time(&mut sensors, Duration::ZERO)
        .expect("sample");
    draw(&mut rt, &mut r, 0);
    assert_eq!(rt.next_due(), seconds(5));
    draw(&mut rt, &mut r, 4);
    draw(&mut rt, &mut r, 5);
    draw(&mut rt, &mut r, 10);
    draw(&mut rt, &mut r, 15);
    assert_eq!(r.faces, [0, 0, 1, 2, 0]);
    assert_eq!(rt.next_due(), seconds(20));
    assert_eq!(rt.theme(), &saved);
    assert_eq!(sensors.samples_taken(), 1);
}
#[test]
fn pause_manual_change_and_standby_restart_without_queued_switches() {
    let mut rt = ThemeRuntime::new(theme(), BTreeMap::new(), Language::English);
    let mut r = Recorder::default();
    draw(&mut rt, &mut r, 0);
    draw(&mut rt, &mut r, 5);
    rt.automatic_cards(false);
    draw(&mut rt, &mut r, 6);
    draw(&mut rt, &mut r, 100);
    assert_eq!(r.faces, [0, 1, 0, 0]);
    rt.automatic_cards(true);
    draw(&mut rt, &mut r, 101);
    draw(&mut rt, &mut r, 106);
    assert_eq!(r.faces.last(), Some(&1));
    let mut edited = rt.theme().clone();
    edited.elements[0].card.as_mut().expect("card").active_face = 2;
    rt.replace_theme(edited);
    draw(&mut rt, &mut r, 107);
    assert_eq!(r.faces.last(), Some(&2));
    draw(&mut rt, &mut r, 1000);
    assert_eq!(r.faces.last(), Some(&0));
    assert_eq!(rt.next_animation_change(seconds(1000)), Some(seconds(1005)));
    draw(&mut rt, &mut r, 1001);
    assert_eq!(r.faces.last(), Some(&0));
    draw(&mut rt, &mut r, 1);
    assert_eq!(r.faces.last(), Some(&2));
}
#[test]
fn hidden_single_face_and_disabled_cards_have_no_timer() {
    for case in 0..3 {
        let mut source = theme();
        match case {
            0 => source.elements[0].visible = false,
            1 => source.elements[0]
                .card
                .as_mut()
                .expect("card")
                .faces
                .truncate(1),
            _ => {
                source.elements[0]
                    .card
                    .as_mut()
                    .expect("card")
                    .rotation_seconds = None
            }
        }
        let mut rt = ThemeRuntime::new(source, BTreeMap::new(), Language::English);
        let mut r = Recorder::default();
        draw(&mut rt, &mut r, 0);
        draw(&mut rt, &mut r, 100);
        assert_eq!(r.faces, [0, 0]);
        assert_eq!(rt.next_animation_change(seconds(100)), None);
    }
}
