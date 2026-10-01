//! Commands that talk to a screen.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Context;
use bezel_core::app::{choose_screen, discover_screens, open_screen, restart_screen};
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::history::Histories;
use bezel_core::domain::pattern::test_pattern as pattern;
use bezel_core::domain::screen::Brightness;
use bezel_core::domain::sensor::{Quantities, Snapshot};
use bezel_core::domain::theme::{AssetRef, Background, Fit, Theme};
use bezel_core::ports::{
    Backdrop, DeviceBus, FrameRenderer, RenderContext, ScreenConnector, ScreenLink,
};

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

/// Largest picture `bezel show` reads, bytes.
const MAX_PICTURE: u64 = 64 * 1024 * 1024;

/// A time for renders that draw no clock.
const NO_TIME: LocalTime = LocalTime {
    year: 2000,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
    second: 0,
    weekday: 5,
};

/// Horizontal for a picture wider than tall, vertical otherwise.
fn orientation_for(width: u32, height: u32) -> Orientation {
    if width > height {
        Orientation::Landscape
    } else {
        Orientation::Portrait
    }
}

/// `bezel show`: draws the picture with the theme renderer (as a
/// background) so it looks exactly as it would in a theme.
pub fn show<B, C>(
    bus: &B,
    connector: &C,
    renderer: &mut dyn FrameRenderer,
    target: &Target,
    path: &Path,
    orientation: Option<Orientation>,
    fit: Fit,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let size = std::fs::metadata(path)
        .with_context(|| format!("cannot read {}", path.display()))?
        .len();
    anyhow::ensure!(
        size <= MAX_PICTURE,
        "{} is larger than 64 MiB",
        path.display()
    );
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let (width, height) = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()?
        .into_dimensions()
        .with_context(|| format!("{} is not a PNG, JPEG or GIF picture", path.display()))?;
    let orientation = orientation.unwrap_or_else(|| orientation_for(width, height));

    let mut link = connect(bus, connector, target)?;
    let panel = link.identity().model.panel;
    let asset = AssetRef("picture".into());
    let mut theme = Theme::blank("picture", panel, orientation);
    theme.background = Background::Image {
        asset: asset.clone(),
        fit,
    };
    let assets = BTreeMap::from([(asset, bytes)]);
    let (snapshot, histories, quantities) =
        (Snapshot::default(), Histories::default(), Quantities::new());
    let context = RenderContext {
        snapshot: &snapshot,
        histories: &histories,
        quantities: &quantities,
        time: NO_TIME,
        language: Language::English,
        backdrop: Backdrop::Poster,
    };
    let frame = renderer.render(&theme, &assets, context)?;
    link.set_orientation(orientation)?;
    link.present(&frame)?;
    Ok(format!(
        "{}: showing {} ({width}x{height}) on {}x{}\n",
        describe(link.as_ref()),
        path.display(),
        theme.canvas.width,
        theme.canvas.height
    ))
}

/// `bezel off`.
pub fn off<B, C>(bus: &B, connector: &C, target: &Target) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let mut link = connect(bus, connector, target)?;
    link.screen_off()?;
    Ok(format!("{}: off\n", describe(link.as_ref())))
}

/// `bezel restart`: says it restarts the screen and how long that takes
/// (only for a screen that can be restarted), then waits for it to be back
/// on the bus (D-2026-09-30-release-polish-13).
pub fn restart<B, C>(
    bus: &B,
    connector: &C,
    target: &Target,
    log: &mut dyn std::io::Write,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let mut log = crate::messages::Messages::new(log);
    let key = target.screen.as_deref();
    let screen = choose_screen(discover_screens(bus)?, key)?;
    let names: Vec<&str> = screen.candidates.iter().map(|m| m.name).collect();
    let name = names.join(" / ");
    if screen.restartable() {
        writeln!(
            log,
            "Restarting the {name} through its wake chip; it is back in about 10 s..."
        );
        log.check()?;
    }
    let back = restart_screen(bus, connector, key).context("could not restart the screen")?;
    let address = back
        .address()
        .map_or_else(|| "?".to_string(), ToString::to_string);
    Ok(format!("{name}: restarted; it is back at {address}\n"))
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
    use bezel_render::{SkiaRenderer, SystemFonts};

    fn target() -> Target {
        Target { screen: None }
    }

    fn picture(name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("bezel-show-{}-{name}.png", std::process::id()));
        image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]))
            .save(&path)
            .unwrap();
        path
    }

    #[test]
    fn show_picks_the_orientation_from_the_picture() {
        let connector = FakeConnector::default();
        let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
        let (wide, tall) = (picture("wide", 400, 100), picture("tall", 100, 400));
        let bus = FakeBus::turing_88();
        let out = show(
            &bus,
            &connector,
            &mut renderer,
            &target(),
            &wide,
            None,
            Fit::Cover,
        )
        .unwrap();
        assert!(out.contains("on 1920x480"), "{out}");
        show(
            &bus,
            &connector,
            &mut renderer,
            &target(),
            &tall,
            None,
            Fit::Cover,
        )
        .unwrap();
        show(
            &bus,
            &connector,
            &mut renderer,
            &target(),
            &tall,
            Some(Orientation::ReverseLandscape),
            Fit::Contain,
        )
        .unwrap();
        let log = connector.log();
        assert_eq!(
            log.orientations,
            vec![
                Orientation::Landscape,
                Orientation::Portrait,
                Orientation::ReverseLandscape
            ]
        );
        assert_eq!(
            log.frames[0].pixel(960, 240),
            Some(Rgba::opaque(200, 30, 30))
        );
        assert_eq!(log.frames[1].size().width, 480);
        // Contain: a tall picture on a wide screen leaves the sides empty.
        assert_ne!(log.frames[2].pixel(0, 240), Some(Rgba::opaque(200, 30, 30)));
        assert_eq!(
            log.frames[2].pixel(960, 240),
            Some(Rgba::opaque(200, 30, 30))
        );

        let text = std::env::temp_dir().join(format!("bezel-show-{}.txt", std::process::id()));
        std::fs::write(&text, b"not a picture").unwrap();
        let err = show(
            &bus,
            &connector,
            &mut renderer,
            &target(),
            &text,
            None,
            Fit::Cover,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not a PNG"), "{err}");
        for p in [wide, tall, text] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn off_turns_the_panel_off() {
        let connector = FakeConnector::default();
        let out = off(&FakeBus::turing_88(), &connector, &target()).unwrap();
        assert!(out.ends_with(": off\n"), "{out}");
        assert_eq!(connector.log().offs, 1);
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
