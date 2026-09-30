//! `gpu.fps`: the frame rate of the game being played, read from what a
//! frame-rate overlay already publishes (D-2026-09-30-release-polish-4).
//! Bezel never hooks or injects into a game: on Windows it reads the shared
//! memory of RivaTuner Statistics Server ([`rtss`]), on Linux the newest CSV
//! log of MangoHud ([`mangohud`]).
//!
//! A source that is absent, or whose newest value is older than
//! [`MAX_AGE`], reads as unavailable with a reason that says how to turn it
//! on: never 0 and never the last value seen. A live source that reports
//! 0 fps (a loading screen) reads 0.

#[cfg(target_os = "linux")]
pub(crate) mod mangohud;
#[cfg(any(windows, test))]
pub(crate) mod rtss;

use std::time::{Duration, Instant};

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use crate::SensorOptions;
use crate::provider::{Provider, describe, put};

/// False until someone reads a real game through each source; the catalog
/// says so next to the source.
pub(crate) const HARDWARE_VALIDATED: bool = false;

/// Readings older than this are stale: the game is paused, minimized or
/// closed, or logging stopped.
pub(crate) const MAX_AGE: Duration = Duration::from_secs(3);

/// Where the frame rate comes from.
pub(crate) trait FrameRateSource: Send {
    /// What the catalog shows as the sensor's source.
    fn describe(&self) -> String;
    /// The current frame rate, or why there is none (and how to get one).
    fn read(&mut self) -> Result<f64, String>;
}

/// The `gpu.fps` provider over one [`FrameRateSource`].
pub(crate) struct Fps {
    source: Box<dyn FrameRateSource>,
}

impl Fps {
    /// `gpu.fps` read from `source`.
    pub(crate) fn new(source: impl FrameRateSource + 'static) -> Self {
        Self {
            source: Box::new(source),
        }
    }
}

impl Provider for Fps {
    fn catalog(&self) -> Vec<SensorInfo> {
        let mut source = self.source.describe();
        if !HARDWARE_VALIDATED {
            source.push_str(" (not validated on hardware)");
        }
        describe(
            keys::GPU_FPS,
            Category::Gpu,
            "Game frame rate",
            Quantity::Number,
            source,
        )
        .into_iter()
        .collect()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        let reading = match self.source.read() {
            Ok(fps) => Reading::Value(fps),
            Err(why) => Reading::Unavailable(why),
        };
        put(out, keys::GPU_FPS, reading);
    }
}

/// This platform's `gpu.fps` provider.
#[cfg(target_os = "linux")]
pub(crate) fn provider(options: &SensorOptions) -> Fps {
    let dir = options.mangohud_dir.clone().or_else(mangohud::default_dir);
    Fps::new(mangohud::MangoHud::new(dir))
}

/// This platform's `gpu.fps` provider.
#[cfg(windows)]
pub(crate) fn provider(_options: &SensorOptions) -> Fps {
    Fps::new(rtss::Rtss)
}

/// This platform's `gpu.fps` provider: none is read here.
#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) fn provider(_options: &SensorOptions) -> Fps {
    Fps::new(Unsupported)
}

/// No frame-rate overlay Bezel reads on this system.
#[cfg(any(not(any(target_os = "linux", windows)), test))]
pub(crate) struct Unsupported;

#[cfg(any(not(any(target_os = "linux", windows)), test))]
impl FrameRateSource for Unsupported {
    fn describe(&self) -> String {
        "none on this system".to_string()
    }

    fn read(&mut self) -> Result<f64, String> {
        Err(
            "game FPS is read from MangoHud on Linux and from RivaTuner \
             Statistics Server on Windows"
                .to_string(),
        )
    }
}

/// Test support: decodes the `.hex` fixtures (see `fixtures/rtss/`).
#[cfg(test)]
pub(crate) fn hex_fixture(text: &str) -> Vec<u8> {
    let number = |t: &str| usize::from_str_radix(t.trim_start_matches("0x"), 16).unwrap();
    let mut image = Vec::new();
    let mut at = 0usize;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if let Some(size) = line.strip_prefix("size ") {
            image = vec![0u8; number(size.trim())];
            continue;
        }
        for token in line.split_whitespace() {
            if let Some(offset) = token.strip_prefix('@') {
                at = number(offset);
            } else {
                image[at] = u8::from_str_radix(token, 16).unwrap();
                at += 1;
            }
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::sensor::SensorKey;

    struct Scripted(Result<f64, String>);

    impl FrameRateSource for Scripted {
        fn describe(&self) -> String {
            "a script".to_string()
        }

        fn read(&mut self) -> Result<f64, String> {
            self.0.clone()
        }
    }

    fn sample(fps: &mut Fps) -> Reading {
        let mut out = Snapshot::default();
        fps.sample(Instant::now(), &mut out);
        assert_eq!(out.len(), 1);
        out.get(&SensorKey::new(keys::GPU_FPS).unwrap())
    }

    #[test]
    fn gpu_fps_is_listed_as_not_validated_on_hardware() {
        let fps = Fps::new(Scripted(Ok(60.0)));
        let catalog = fps.catalog();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].key.as_str(), keys::GPU_FPS);
        assert_eq!(catalog[0].category, Category::Gpu);
        assert_eq!(catalog[0].quantity, Quantity::Number);
        assert_eq!(catalog[0].source, "a script (not validated on hardware)");
    }

    #[test]
    fn values_and_reasons_pass_through() {
        assert_eq!(
            sample(&mut Fps::new(Scripted(Ok(0.0)))),
            Reading::Value(0.0)
        );
        assert_eq!(
            sample(&mut Fps::new(Scripted(Err("off".into())))),
            Reading::Unavailable("off".into())
        );
        let mut none = Fps::new(Unsupported);
        assert!(matches!(sample(&mut none), Reading::Unavailable(why) if why.contains("MangoHud")));
        assert_eq!(
            none.catalog()[0].source,
            "none on this system (not validated on hardware)"
        );
    }

    #[test]
    fn hex_fixtures_decode_offsets_and_bytes() {
        let image = hex_fixture("size 0x8\n@0x2 # skip\nab cd\n@6 01\n");
        assert_eq!(image, vec![0, 0, 0xab, 0xcd, 0, 0, 1, 0]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn options_choose_the_mangohud_folder() {
        let options = SensorOptions {
            mangohud_dir: Some("/games/logs".into()),
            ..SensorOptions::default()
        };
        assert_eq!(
            provider(&options).catalog()[0].source,
            "MangoHud logs in /games/logs (not validated on hardware)"
        );
    }
}
