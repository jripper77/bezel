//! The core's `ThemeRuntime` use case driven through adapters: the demo
//! sensors, a renderer that records what it is given, the real renderer and
//! the simulated screen (D-1: the fakes live in the adapter crates, never in
//! the core).
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

mod support;

use std::collections::BTreeMap;

use bezel_core::app::{ThemeRuntime, open_screen};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::sensor::{Quantity, Reading, SensorInfo, SensorKey, Snapshot, keys};
use bezel_core::domain::theme::{
    AssetRef, Binding, BoxF, Element, ElementId, ElementKind, GraphStyle, Theme,
};
use bezel_core::ports::{FrameRenderer, RenderContext, SensorSource, ThemeLocation, ThemeStore};
use bezel_core::{BezelError, Result};
use bezel_devices::{FakeBus, FakeConnector};
use bezel_sensors::FakeSensors;
use bezel_themes::FsThemeStore;

fn key(k: &str) -> SensorKey {
    SensorKey::new(k).expect("key")
}

/// What one render call was given.
#[derive(Debug, Clone, PartialEq)]
struct Call {
    theme: String,
    time: LocalTime,
    language: Language,
    cpu: Reading,
    cpu_history: Vec<Option<f64>>,
    cpu_quantity: Option<Quantity>,
    assets: usize,
}

/// A renderer adapter that records its inputs and returns a blank frame.
#[derive(Debug, Default)]
struct RecordingRenderer {
    calls: Vec<Call>,
}

impl FrameRenderer for RecordingRenderer {
    fn render(
        &mut self,
        theme: &Theme,
        assets: &BTreeMap<AssetRef, Vec<u8>>,
        context: RenderContext<'_>,
    ) -> Result<Frame> {
        let cpu = key(keys::CPU_USAGE);
        self.calls.push(Call {
            theme: theme.name.clone(),
            time: context.time,
            language: context.language,
            cpu: context.snapshot.get(&cpu),
            cpu_history: context.histories.get(&cpu),
            cpu_quantity: context.quantities.get(&cpu),
            assets: assets.len(),
        });
        Ok(Frame::filled(theme.canvas, Rgba::BLACK))
    }
}

/// Sensors whose catalog fails (samples still work).
struct NoCatalog;

impl SensorSource for NoCatalog {
    fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
        Err(BezelError::Transport("no catalog".into()))
    }

    fn sample(&mut self) -> Result<Snapshot> {
        let mut s = Snapshot::default();
        s.insert(key(keys::CPU_USAGE), Reading::Value(7.0));
        Ok(s)
    }
}

/// A portrait 8.8" theme graphing CPU usage over `history` samples.
fn graphing(name: &str, history: u16) -> Theme {
    let mut theme = Theme::blank(name, Size::new(480, 1920), Orientation::Portrait);
    theme.elements.push(Element {
        card: None,
        is_group: false,
        group_parent: None,
        card_member: None,
        id: ElementId(1),
        name: "cpu graph".into(),
        frame: BoxF::new(0.0, 0.0, 480.0, 200.0),
        opacity: 1.0,
        visible: true,
        locked: false,
        kind: ElementKind::Graph {
            binding: Binding {
                key: key(keys::CPU_USAGE),
                min: 0.0,
                max: 100.0,
            },
            history,
            style: GraphStyle::Line,
            color: Rgba::WHITE,
            fill: None,
            line_width: 2.0,
            autoscale: false,
        },
    });
    theme
}

#[test]
fn frames_sample_the_sensors_and_keep_graph_histories() {
    let mut runtime = ThemeRuntime::new(graphing("a", 3), BTreeMap::new(), Language::PortugueseBr);
    let mut sensors = FakeSensors::demo();
    let mut renderer = RecordingRenderer::default();
    for _ in 0..4 {
        let frame = runtime
            .frame(&mut sensors, &mut renderer, support::TIME)
            .expect("frame");
        assert_eq!(frame.size(), Size::new(480, 1920));
    }
    assert_eq!(sensors.samples_taken(), 4);
    let first = &renderer.calls[0];
    assert_eq!(first.time, support::TIME);
    assert_eq!(first.language, Language::PortugueseBr);
    // The demo's first sample is still warming its rates up.
    assert!(matches!(first.cpu, Reading::Unavailable(_)));
    assert_eq!(first.cpu_history, vec![None]);
    assert_eq!(
        first.cpu_quantity,
        Some(Quantity::Percent),
        "units from the catalog"
    );
    let last = renderer.calls.last().expect("calls");
    assert_eq!(last.cpu, Reading::Value(12.5));
    assert_eq!(
        last.cpu_history,
        vec![Some(12.5); 3],
        "the last three samples"
    );
}

#[test]
fn replacing_the_theme_keeps_the_history_of_graphed_sensors() {
    let mut runtime = ThemeRuntime::new(graphing("a", 4), BTreeMap::new(), Language::English);
    let mut sensors = support::busy_sensors(6);
    let mut renderer = RecordingRenderer::default();
    for _ in 0..3 {
        runtime
            .frame(&mut sensors, &mut renderer, support::TIME)
            .expect("frame");
    }
    let before = renderer.calls.last().expect("calls").cpu_history.clone();
    let assets = BTreeMap::from([(AssetRef("assets/x.png".into()), vec![1, 2, 3])]);
    runtime.replace(graphing("b", 2), assets);
    assert_eq!(runtime.theme().name, "b");
    runtime
        .frame(&mut sensors, &mut renderer, support::TIME)
        .expect("frame");
    let after = renderer.calls.last().expect("calls");
    assert_eq!(after.theme, "b");
    assert_eq!(after.assets, 1);
    assert_eq!(after.cpu_history.len(), 2);
    assert_eq!(after.cpu_history[0], before[2], "kept the newest sample");
}

#[test]
fn a_missing_catalog_only_costs_the_units() {
    let mut runtime = ThemeRuntime::new(graphing("a", 2), BTreeMap::new(), Language::English);
    let mut renderer = RecordingRenderer::default();
    runtime
        .frame(&mut NoCatalog, &mut renderer, support::TIME)
        .expect("frame");
    assert_eq!(renderer.calls[0].cpu, Reading::Value(7.0));
    assert_eq!(renderer.calls[0].cpu_quantity, None);
}

#[test]
fn show_presents_a_bundled_theme_on_the_screen() {
    let dir = support::themes_dir().join("turing-8.8-horizontal");
    let (theme, assets) = FsThemeStore
        .load(&ThemeLocation(dir.display().to_string()))
        .expect("bundled theme");
    let connector = FakeConnector::default();
    let mut link = open_screen(&FakeBus::turing_88(), &connector, None).expect("screen");
    link.set_orientation(theme.orientation)
        .expect("orientation");
    let mut runtime = ThemeRuntime::new(theme, assets, Language::English);
    let mut sensors = support::busy_sensors(2);
    let mut renderer = support::renderer();
    for _ in 0..2 {
        runtime
            .show(&mut sensors, &mut renderer, link.as_mut(), support::TIME)
            .expect("shown");
    }
    let log = connector.log();
    assert_eq!(log.frames.len(), 2);
    assert_eq!(log.frames[0].size(), Size::new(1920, 480));
    assert_ne!(log.frames[0], log.frames[1], "the sensors moved");
    assert!(renderer.problems().is_empty(), "{:?}", renderer.problems());
}
