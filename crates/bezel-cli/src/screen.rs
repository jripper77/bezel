//! Commands that talk to a screen.

use std::time::{Duration, Instant};

use anyhow::Context;
use bezel_core::app::open_screen;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::pattern::test_pattern as pattern;
use bezel_core::domain::screen::Brightness;
use bezel_core::ports::{DeviceBus, ScreenConnector, ScreenLink};

use crate::Target;

fn connect<B, C>(bus: &B, connector: &C, target: &Target) -> anyhow::Result<Box<dyn ScreenLink>>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    open_screen(bus, connector, target.screen.as_deref()).context("could not open the screen")
}

fn describe(link: &dyn ScreenLink) -> String {
    let id = link.identity();
    format!(
        "{} ({})",
        id.model.name,
        id.firmware.as_deref().unwrap_or("no firmware string")
    )
}

/// `bezel test-pattern`.
pub fn test_pattern<B, C>(
    bus: &B,
    connector: &C,
    target: &Target,
    seconds: u64,
    orientation: Orientation,
    release: bool,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let mut link = connect(bus, connector, target)?;
    link.set_orientation(orientation)?;
    let size = link.identity().model.panel.in_orientation(orientation);
    let started = Instant::now();
    let limit = Duration::from_secs(seconds);
    let mut frames = 0u32;
    loop {
        link.present(&pattern(size, frames))?;
        frames += 1;
        if started.elapsed() >= limit {
            break;
        }
    }
    let secs = started.elapsed().as_secs_f64().max(f64::EPSILON);
    if release {
        link.release()?;
    }
    Ok(format!(
        "{}: {frames} frames in {secs:.1} s ({:.1} fps), {}x{}{}\n",
        describe(link.as_ref()),
        f64::from(frames) / secs,
        size.width,
        size.height,
        if release { ", released" } else { "" }
    ))
}

/// `bezel brightness`.
pub fn brightness<B, C>(
    bus: &B,
    connector: &C,
    target: &Target,
    percent: u8,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let level = Brightness::new(percent).context("brightness is 0-100")?;
    let mut link = connect(bus, connector, target)?;
    link.set_brightness(level)?;
    Ok(format!(
        "{}: brightness {percent}%\n",
        describe(link.as_ref())
    ))
}

/// `bezel release`.
pub fn release<B, C>(bus: &B, connector: &C, target: &Target) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let mut link = connect(bus, connector, target)?;
    link.release()?;
    Ok(format!("{}: released\n", describe(link.as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::pattern::CORNERS;
    use bezel_devices::{FakeBus, FakeConnector};

    fn target() -> Target {
        Target { screen: None }
    }

    #[test]
    fn test_pattern_presents_frames_in_the_orientation() {
        let connector = FakeConnector::default();
        let out = test_pattern(
            &FakeBus::turing_88(),
            &connector,
            &target(),
            0,
            Orientation::Landscape,
            true,
        )
        .unwrap();
        assert!(out.contains("1920x480"), "{out}");
        assert!(out.contains("released"), "{out}");
        let log = connector.log();
        assert_eq!(log.frames.len(), 1);
        assert_eq!(
            log.frames[0].pixel(10, 10),
            Some(Rgba::opaque(255, 200, 0)),
            "strip at the start"
        );
        assert_eq!(log.frames[0].pixel(1919, 0), Some(CORNERS[1]));
        assert_eq!(log.releases, 1);
    }

    #[test]
    fn brightness_and_release() {
        let connector = FakeConnector::default();
        let out = brightness(&FakeBus::turing_88(), &connector, &target(), 40).unwrap();
        assert!(out.ends_with("brightness 40%\n"), "{out}");
        assert!(brightness(&FakeBus::turing_88(), &connector, &target(), 101).is_err());
        release(&FakeBus::turing_88(), &connector, &target()).unwrap();
        let log = connector.log();
        assert_eq!(log.brightness.len(), 1);
        assert_eq!(log.releases, 1);
        let missing = Target {
            screen: Some("COM99".into()),
        };
        let err = release(&FakeBus::turing_88(), &connector, &missing)
            .err()
            .unwrap();
        assert!(format!("{err:#}").contains("COM99"));
    }
}
