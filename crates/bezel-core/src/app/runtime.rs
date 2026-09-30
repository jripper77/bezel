//! Running a theme: sample the sensors, keep graph histories, render a frame
//! and show it. The caller owns the cadence (`Theme::refresh_seconds`).

use std::collections::BTreeMap;

use crate::Result;
use crate::domain::clock::{Language, LocalTime};
use crate::domain::frame::Frame;
use crate::domain::history::Histories;
use crate::domain::sensor::Quantities;
use crate::domain::theme::{AssetRef, Theme};
use crate::ports::{Backdrop, FrameRenderer, RenderContext, ScreenLink, SensorSource};

/// A theme being shown.
#[derive(Debug, Clone)]
pub struct ThemeRuntime {
    theme: Theme,
    assets: BTreeMap<AssetRef, Vec<u8>>,
    histories: Histories,
    quantities: Quantities,
    language: Language,
}

impl ThemeRuntime {
    /// Starts running `theme` with its asset bytes.
    pub fn new(theme: Theme, assets: BTreeMap<AssetRef, Vec<u8>>, language: Language) -> Self {
        let histories = Histories::new(&theme.history_lengths());
        Self {
            theme,
            assets,
            histories,
            quantities: Quantities::default(),
            language,
        }
    }

    /// The theme being shown.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Swaps in an edited theme, keeping the history of sensors still graphed.
    pub fn replace(&mut self, theme: Theme, assets: BTreeMap<AssetRef, Vec<u8>>) {
        let mut histories = Histories::new(&theme.history_lengths());
        histories.adopt(&self.histories);
        self.theme = theme;
        self.assets = assets;
        self.histories = histories;
    }

    /// Samples the sensors, records histories and renders one frame.
    pub fn frame(
        &mut self,
        sensors: &mut dyn SensorSource,
        renderer: &mut dyn FrameRenderer,
        time: LocalTime,
    ) -> Result<Frame> {
        if self.quantities.is_empty() {
            // Units of sensor text; a catalog failure only costs the units.
            if let Ok(catalog) = sensors.catalog() {
                self.quantities = Quantities::from_catalog(&catalog);
            }
        }
        let snapshot = sensors.sample()?;
        self.histories.push(&snapshot);
        let context = RenderContext {
            snapshot: &snapshot,
            histories: &self.histories,
            quantities: &self.quantities,
            time,
            language: self.language,
            backdrop: Backdrop::Poster,
        };
        renderer.render(&self.theme, &self.assets, context)
    }

    /// One refresh: [`Self::frame`] then present it on `screen`.
    pub fn show(
        &mut self,
        sensors: &mut dyn SensorSource,
        renderer: &mut dyn FrameRenderer,
        screen: &mut dyn ScreenLink,
        time: LocalTime,
    ) -> Result<()> {
        let frame = self.frame(sensors, renderer, time)?;
        screen.present(&frame)
    }
}
