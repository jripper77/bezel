//! [`SystemSensors`]: the machine's real sensors behind the port, and the
//! [`SensorOptions`] of the sources that take settings.

use std::path::PathBuf;
use std::time::Instant;

use bezel_core::Result;
use bezel_core::domain::sensor::{SensorInfo, Snapshot};
use bezel_core::ports::SensorSource;

use crate::gpu::{Alias, FoundGpu, number};
use crate::provider::Provider;

/// Settings of the sources that take them (the studio's settings and the
/// CLI's flags fill them in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorOptions {
    /// Host `net.ping` measures the round trip to.
    pub ping_host: String,
    /// Folder of MangoHud's CSV logs read for `gpu.fps` on Linux; `None`
    /// means MangoHud's own `output_folder`.
    pub mangohud_dir: Option<PathBuf>,
}

impl SensorOptions {
    /// The default `ping_host`: a public resolver that answers ICMP and TCP.
    pub const DEFAULT_PING_HOST: &str = "8.8.8.8";
}

impl Default for SensorOptions {
    fn default() -> Self {
        Self {
            ping_host: Self::DEFAULT_PING_HOST.to_string(),
            mangohud_dir: None,
        }
    }
}

/// Measures this machine by composing the platform's providers. Discovery
/// happens once, when it is built; the catalog is then fixed and every
/// `sample` returns a reading (possibly unavailable) for each entry.
pub struct SystemSensors {
    providers: Vec<Box<dyn Provider>>,
    aliases: Vec<Alias>,
    catalog: Vec<SensorInfo>,
}

impl SystemSensors {
    /// Discovers every sensor this machine offers, with the default
    /// [`SensorOptions`]. Never fails: whatever cannot be read shows up as
    /// unavailable, with the reason.
    pub fn new() -> Self {
        Self::with_options(SensorOptions::default())
    }

    /// Like [`SystemSensors::new`], with `options` for the sources that
    /// take settings.
    pub fn with_options(options: SensorOptions) -> Self {
        let (providers, gpus) = platform(&options);
        Self::assemble(providers, gpus)
    }

    /// Linux sensors read from fake `/sys` and `/proc` trees. NVIDIA GPUs,
    /// which come from NVML rather than sysfs, are not included.
    #[cfg(all(test, target_os = "linux"))]
    fn with_roots(sys: impl Into<PathBuf>, proc: impl Into<PathBuf>) -> Self {
        let roots = crate::linux::Roots::new(sys, proc);
        Self::assemble(
            crate::linux::providers(&roots),
            crate::amdgpu::discover(&roots),
        )
    }

    fn assemble(mut providers: Vec<Box<dyn Provider>>, gpus: Vec<FoundGpu>) -> Self {
        let gpus = number(gpus);
        providers.extend(gpus.providers);
        let mut catalog: Vec<SensorInfo> = providers
            .iter()
            .flat_map(|p| p.catalog())
            .chain(gpus.catalog)
            .collect();
        // Headline sensors first in each category, per-device ones after
        // (the order is presentation only; keys are what themes bind to).
        catalog.sort_by_key(|info| (info.category, rank(info.key.as_str())));
        Self {
            providers,
            aliases: gpus.aliases,
            catalog,
        }
    }
}

impl Default for SystemSensors {
    fn default() -> Self {
        Self::new()
    }
}

/// 0 for summary keys (`cpu.usage`, `disk.root.used`), 1 for per-device keys
/// (`cpu.3.usage`, `net.eno1.down`), 2 for raw chip sensors (`hwmon.*`).
fn rank(key: &str) -> u8 {
    if key.starts_with("hwmon.") || key.starts_with("lhm.") {
        return 2;
    }
    let second = key.split('.').nth(1).unwrap_or_default();
    u8::from(second.chars().any(|c| c.is_ascii_digit()))
}

/// The platform's providers and GPUs. No provider takes `_options` yet;
/// the ping and game-FPS sources will (D-2026-09-30-release-polish-4, -5).
#[cfg(target_os = "linux")]
fn platform(_options: &SensorOptions) -> (Vec<Box<dyn Provider>>, Vec<FoundGpu>) {
    let roots = crate::linux::Roots::host();
    let mut gpus = crate::nvidia::discover();
    gpus.extend(crate::amdgpu::discover(&roots));
    (crate::linux::providers(&roots), gpus)
}

#[cfg(windows)]
fn platform(_options: &SensorOptions) -> (Vec<Box<dyn Provider>>, Vec<FoundGpu>) {
    (crate::windows::providers(), crate::nvidia::discover())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform(_options: &SensorOptions) -> (Vec<Box<dyn Provider>>, Vec<FoundGpu>) {
    (Vec::new(), crate::nvidia::discover())
}

impl SensorSource for SystemSensors {
    fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
        Ok(self.catalog.clone())
    }

    /// Samples every provider at once, each on its own thread: a slow
    /// kernel read (a Wi-Fi chip's firmware query, an NVMe SMART log, an
    /// SMBus DIMM sensor) then costs its own latency, not the sum of all.
    fn sample(&mut self) -> Result<Snapshot> {
        let now = Instant::now();
        let parts: Vec<Snapshot> = std::thread::scope(|scope| {
            let running: Vec<_> = self
                .providers
                .iter_mut()
                .map(|provider| {
                    scope.spawn(move || {
                        let mut part = Snapshot::default();
                        provider.sample(now, &mut part);
                        part
                    })
                })
                .collect();
            running
                .into_iter()
                .filter_map(|thread| {
                    thread
                        .join()
                        .map_err(|_| {
                            tracing::error!("a sensor provider panicked; its sensors are missing")
                        })
                        .ok()
                })
                .collect()
        });
        let mut out = Snapshot::default();
        for part in &parts {
            for (key, reading) in part.iter() {
                out.insert(key.clone(), reading.clone());
            }
        }
        for alias in &self.aliases {
            let reading = alias.reading(&out);
            out.insert(alias.key.clone(), reading);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_keys_rank_before_devices_and_chips() {
        assert_eq!(rank("cpu.usage"), 0);
        assert_eq!(rank("cpu.load.15"), 0);
        assert_eq!(rank("disk.root.used"), 0);
        assert_eq!(rank("cpu.3.usage"), 1);
        assert_eq!(rank("net.eno1.down"), 1);
        assert_eq!(rank("hwmon.k10temp.tctl"), 2);
        assert_eq!(rank("nodots"), 0);
    }

    #[test]
    fn options_default_to_the_public_resolver() {
        let options = SensorOptions::default();
        assert_eq!(options.ping_host, "8.8.8.8");
        assert_eq!(options.mangohud_dir, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_fake_machine_answers_for_every_catalog_entry() {
        use bezel_core::domain::sensor::{Category, Reading, keys};
        let t = crate::testing::FakeTree::new("system");
        t.file("proc/stat", "cpu  1 0 1 8 0 0 0 0\ncpu0 1 0 1 8 0 0 0 0\n")
            .file("proc/meminfo", "MemTotal: 1024 kB\nMemAvailable: 512 kB\n")
            .file("proc/uptime", "10.5 20.0\n")
            .file("sys/class/hwmon/hwmon0/name", "k10temp\n")
            .file("sys/class/hwmon/hwmon0/temp1_input", "50000\n")
            .file("sys/class/hwmon/hwmon0/temp1_label", "Tctl\n")
            .file("sys/devices/card/gpu_busy_percent", "42\n")
            .dir("sys/drivers/amdgpu")
            .link("sys/devices/card/driver", "sys/drivers/amdgpu")
            .link("sys/class/drm/card0/device", "sys/devices/card");
        let mut sensors = SystemSensors::with_roots(t.path("sys"), t.path("proc"));
        let catalog = sensors.catalog().unwrap();
        assert_eq!(catalog[0].key.as_str(), keys::CPU_USAGE);
        let categories: Vec<Category> = catalog.iter().map(|i| i.category).collect();
        let mut sorted = categories.clone();
        sorted.sort();
        assert_eq!(categories, sorted);
        let snapshot = sensors.sample().unwrap();
        for info in &catalog {
            assert!(
                snapshot.iter().any(|(k, _)| *k == info.key),
                "{} not sampled",
                info.key
            );
        }
        assert_eq!(snapshot.len(), catalog.len());
        let get = |k: &str| snapshot.get(&bezel_core::domain::sensor::SensorKey::new(k).unwrap());
        assert_eq!(get("gpu.0.usage"), Reading::Value(42.0));
        assert_eq!(get(keys::GPU_USAGE), get("gpu.0.usage"));
        // Imported themes find their keys, measured or explained. Ping and
        // game FPS come from their own sources.
        let listed: Vec<&str> = catalog.iter().map(|i| i.key.as_str()).collect();
        for key in keys::IMPORTED {
            if key != keys::GPU_FPS && key != keys::NET_PING {
                assert!(listed.contains(&key), "{key} not listed");
            }
        }
        assert_eq!(get(keys::MEMORY_AVAILABLE_PERCENT), Reading::Value(50.0));
        assert!(matches!(get(keys::GPU_VOLTAGE), Reading::Unavailable(r) if r.contains("voltage")));
    }

    /// The real machine, read-only: whatever it has, every catalog entry is
    /// answered and nothing outside the catalog is.
    #[test]
    fn this_machine_answers_for_every_catalog_entry() {
        let mut sensors = SystemSensors::default();
        let catalog = sensors.catalog().unwrap();
        sensors.sample().unwrap();
        let snapshot = sensors.sample().unwrap();
        assert_eq!(snapshot.len(), catalog.len());
        for info in &catalog {
            assert!(
                snapshot.iter().any(|(k, _)| *k == info.key),
                "{} not sampled",
                info.key
            );
        }
    }
}
