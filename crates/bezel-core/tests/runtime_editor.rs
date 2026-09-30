//! The theme runtime as an editor drives it (D-2026-09-30-studio-app-4):
//! every edit swapped in place with the assets and the graph histories
//! kept, the readings of the last sample for the sensor panel and the units
//! of a catalog read again. The sensors are the adapter's fake; the renderer
//! is a recording double defined here (no adapter ships a fake of it).
#![allow(clippy::expect_used)] // helpers of a failing test panic

use std::collections::BTreeMap;

use bezel_core::Result;
use bezel_core::app::ThemeRuntime;
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, SensorKey, Snapshot};
use bezel_core::domain::theme::{
    AssetRef, Binding, BoxF, Element, ElementId, ElementKind, GraphStyle, Theme,
};
use bezel_core::ports::{FrameRenderer, RenderContext};
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

fn key(text: &str) -> SensorKey {
    SensorKey::new(text).expect("key")
}

/// What one frame was drawn from.
#[derive(Debug, Clone, PartialEq)]
struct Drawn {
    theme: String,
    assets: Vec<AssetRef>,
    history: Vec<Option<f64>>,
    usage: Reading,
    unit: Option<Quantity>,
}

/// A renderer that records what each frame was drawn from (the history and
/// reading of `cpu.usage`) and returns a blank frame of the canvas.
#[derive(Default)]
struct Recorder {
    seen: Vec<Drawn>,
}

impl FrameRenderer for Recorder {
    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame> {
        let usage = key("cpu.usage");
        self.seen.push(Drawn {
            theme: theme.name.clone(),
            assets: assets.keys().cloned().collect(),
            history: context.histories.get(&usage),
            usage: context.snapshot.get(&usage),
            unit: context.quantities.get(&usage),
        });
        Ok(Frame::filled(theme.canvas, Rgba::default()))
    }
}

/// A theme graphing `cpu.usage` over `history` samples.
fn graphing(name: &str, history: u16) -> Theme {
    let mut theme = Theme::blank(name, Size::new(480, 1920), Orientation::Portrait);
    theme.elements.push(Element {
        id: ElementId(1),
        name: "usage".into(),
        frame: BoxF {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 80.0,
        },
        opacity: 1.0,
        visible: true,
        locked: false,
        kind: ElementKind::Graph {
            binding: Binding {
                key: key("cpu.usage"),
                min: 0.0,
                max: 100.0,
            },
            history,
            style: GraphStyle::Line,
            color: Rgba::WHITE,
            fill: None,
            line_width: 1.0,
            autoscale: false,
        },
    });
    theme
}

fn info(text: &str, quantity: Quantity) -> SensorInfo {
    SensorInfo {
        key: key(text),
        category: Category::Cpu,
        label: text.into(),
        quantity,
        source: "test".into(),
    }
}

/// Sensors whose `cpu.usage` reads 10, 20, 30… (then stays at the last).
fn usage(values: &[f64]) -> FakeSensors {
    let script = values
        .iter()
        .map(|v| {
            let mut snapshot = Snapshot::default();
            snapshot.insert(key("cpu.usage"), Reading::Value(*v));
            snapshot
        })
        .collect();
    FakeSensors::new(vec![info("cpu.usage", Quantity::Percent)], script)
}

#[test]
fn edits_keep_the_assets_and_the_graph_history() {
    let logo = AssetRef("assets/logo.png".into());
    let assets = BTreeMap::from([(logo.clone(), vec![1, 2, 3])]);
    let mut rt = ThemeRuntime::new(graphing("first", 3), assets, Language::English);
    let mut sensors = usage(&[10.0, 20.0, 30.0]);
    let mut r = Recorder::default();
    rt.sample(&mut sensors).expect("sample");
    rt.sample(&mut sensors).expect("sample");

    rt.replace_theme(graphing("edited", 3));
    let clip = AssetRef("assets/clip.png".into());
    rt.add_asset(clip.clone(), vec![4]);
    rt.render(&mut r, TIME, Default::default()).expect("frame");
    assert_eq!(rt.theme().name, "edited");
    assert_eq!(rt.assets().len(), 2);
    assert_eq!(r.seen[0].assets, [clip.clone(), logo.clone()]);
    assert_eq!(r.seen[0].history, [Some(10.0), Some(20.0)], "history kept");

    // A shorter graph keeps the newest samples; new bytes replace old ones.
    rt.sample(&mut sensors).expect("sample");
    rt.replace_theme(graphing("shorter", 2));
    rt.add_asset(clip.clone(), vec![5, 6]);
    assert_eq!(rt.assets()[&clip], [5, 6]);
    rt.render(&mut r, TIME, Default::default()).expect("frame");
    assert_eq!(r.seen[1].history, [Some(20.0), Some(30.0)]);

    // Another document brings its own assets and still keeps the history.
    rt.replace(graphing("other", 3), BTreeMap::new());
    rt.render(&mut r, TIME, Default::default()).expect("frame");
    assert!(r.seen[2].assets.is_empty());
    assert_eq!(r.seen[2].history, [Some(20.0), Some(30.0)]);
}

#[test]
fn the_last_sample_feeds_the_sensor_panel() {
    let mut rt = ThemeRuntime::new(graphing("t", 4), BTreeMap::new(), Language::English);
    assert!(rt.snapshot().is_empty(), "nothing sampled yet");
    assert!(rt.quantities().is_empty());
    let mut sensors = usage(&[10.0, 42.0]);
    rt.sample(&mut sensors).expect("sample");
    rt.sample(&mut sensors).expect("sample");
    assert_eq!(rt.snapshot().get(&key("cpu.usage")), Reading::Value(42.0));
    assert_eq!(
        rt.quantities().get(&key("cpu.usage")),
        Some(Quantity::Percent),
        "the first sample reads the catalog"
    );

    let mut r = Recorder::default();
    rt.render_with(&mut r, TIME, Default::default())
        .expect("preview");
    assert_eq!(r.seen[0].usage, Reading::Value(42.0));
    assert_eq!(r.seen[0].history, [Some(10.0), Some(42.0)]);
}

#[test]
fn a_catalog_read_again_gives_the_units() {
    let mut rt = ThemeRuntime::new(graphing("t", 4), BTreeMap::new(), Language::English);
    rt.use_catalog(&[info("cpu.usage", Quantity::Celsius)]);
    // The runtime keeps the catalog it was given: a sample does not read it.
    rt.sample(&mut usage(&[1.0])).expect("sample");
    let mut r = Recorder::default();
    rt.render_with(&mut r, TIME, Default::default())
        .expect("preview");
    assert_eq!(r.seen[0].unit, Some(Quantity::Celsius));
    rt.use_catalog(&[]);
    assert!(rt.quantities().is_empty(), "the sensor is gone");
}
