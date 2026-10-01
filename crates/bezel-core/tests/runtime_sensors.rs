//! What the theme runtime tells the sensors it shows
//! (D-2026-09-30-release-polish-11): before every sample, the sensors of
//! the theme's visible elements and those the caller shows beside it (an
//! editor's sensor list), so that `net.ping` is measured only while
//! something shown uses it. The sensors are the adapter's fake, which
//! records what it was told.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;

use bezel_core::app::ThemeRuntime;
use bezel_core::domain::clock::Language;
use bezel_core::domain::frame::Rgba;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::sensor::{DisplayFormat, SensorKey, Wanted, keys};
use bezel_core::domain::theme::{
    Binding, BoxF, Element, ElementId, ElementKind, GraphStyle, TextContent, TextStyle, Theme,
};
use bezel_sensors::FakeSensors;

fn key(text: &str) -> SensorKey {
    SensorKey::new(text).expect("key")
}

fn element(id: u32, kind: ElementKind) -> Element {
    Element {
        id: ElementId(id),
        name: format!("e{id}"),
        frame: BoxF::new(0.0, 0.0, 100.0, 40.0),
        opacity: 1.0,
        visible: true,
        locked: false,
        kind,
    }
}

/// A text element printing `sensor`.
fn value(id: u32, sensor: &str) -> Element {
    element(
        id,
        ElementKind::Text {
            content: TextContent::Sensor {
                key: key(sensor),
                format: DisplayFormat::default(),
                prefix: String::new(),
                suffix: String::new(),
            },
            style: TextStyle::default(),
        },
    )
}

/// A graph of `sensor`.
fn graph(id: u32, sensor: &str) -> Element {
    element(
        id,
        ElementKind::Graph {
            binding: Binding {
                key: key(sensor),
                min: 0.0,
                max: 100.0,
            },
            history: 30,
            style: GraphStyle::Line,
            color: Rgba::WHITE,
            fill: None,
            line_width: 2.0,
            autoscale: false,
        },
    )
}

fn theme(name: &str, elements: Vec<Element>) -> Theme {
    let mut theme = Theme::blank(name, Size::new(480, 1920), Orientation::Portrait);
    theme.elements = elements;
    theme
}

/// A theme of CPU values and no ping.
fn without_ping() -> Theme {
    theme(
        "cpu",
        vec![value(1, keys::CPU_TEMPERATURE), graph(2, keys::CPU_USAGE)],
    )
}

/// A theme that prints the ping.
fn with_ping() -> Theme {
    theme(
        "ping",
        vec![value(1, keys::CPU_TEMPERATURE), value(2, keys::NET_PING)],
    )
}

fn wanted(list: &[&str]) -> Wanted {
    list.iter().map(|k| key(k)).collect()
}

/// Samples once and returns what the sensors were told.
fn sample(runtime: &mut ThemeRuntime, sensors: &mut FakeSensors) -> Wanted {
    runtime.sample(sensors).expect("sample");
    sensors.wanted().cloned().expect("told before sampling")
}

#[test]
fn a_theme_without_a_ping_value_does_not_want_it() {
    let mut sensors = FakeSensors::demo();
    let mut hidden = value(3, keys::NET_PING);
    hidden.visible = false;
    let mut shown = without_ping();
    shown.elements.push(hidden);
    let mut runtime = ThemeRuntime::new(shown, BTreeMap::new(), Language::English);
    let told = sample(&mut runtime, &mut sensors);
    assert_eq!(told, wanted(&[keys::CPU_TEMPERATURE, keys::CPU_USAGE]));
    assert!(
        !told.contains(keys::NET_PING),
        "a hidden element is not shown"
    );
    assert_eq!(runtime.wanted(), &told);
}

#[test]
fn switching_themes_switches_the_ping_on_and_off() {
    let mut sensors = FakeSensors::demo();
    let mut runtime = ThemeRuntime::new(without_ping(), BTreeMap::new(), Language::English);
    assert!(!sample(&mut runtime, &mut sensors).contains(keys::NET_PING));

    runtime.replace_theme(with_ping());
    let told = sample(&mut runtime, &mut sensors);
    assert_eq!(told, wanted(&[keys::CPU_TEMPERATURE, keys::NET_PING]));

    runtime.replace(without_ping(), BTreeMap::new());
    assert!(!sample(&mut runtime, &mut sensors).contains(keys::NET_PING));
}

#[test]
fn what_the_caller_shows_beside_the_theme_is_wanted_too() {
    let mut sensors = FakeSensors::demo();
    let mut runtime = ThemeRuntime::new(without_ping(), BTreeMap::new(), Language::English);
    // An editor's sensor list shows the ping.
    runtime.want_also(wanted(&[keys::NET_PING, keys::GPU_USAGE]));
    let told = sample(&mut runtime, &mut sensors);
    let both = wanted(&[
        keys::CPU_TEMPERATURE,
        keys::CPU_USAGE,
        keys::GPU_USAGE,
        keys::NET_PING,
    ]);
    assert_eq!(told, both);
    // An edit keeps what the list shows.
    runtime.replace_theme(theme("empty", Vec::new()));
    assert_eq!(
        sample(&mut runtime, &mut sensors),
        wanted(&[keys::GPU_USAGE, keys::NET_PING])
    );
    // The list closed: only the theme's again.
    runtime.want_also(Wanted::nothing());
    assert_eq!(sample(&mut runtime, &mut sensors), Wanted::nothing());
    runtime.want_also(Wanted::All);
    assert_eq!(sample(&mut runtime, &mut sensors), Wanted::All);
}
