//! [`SystemSensors`]: the machine's real sensors behind the port.

use std::time::Instant;

use bezel_core::Result;
use bezel_core::domain::sensor::{SensorInfo, Snapshot};
use bezel_core::ports::SensorSource;

use crate::gpu::{Alias, FoundGpu, number};
use crate::provider::Provider;

/// Measures this machine by composing the platform's providers. Discovery
/// happens once, in [`SystemSensors::new`]; the catalog is then fixed and
/// every `sample` returns a reading (possibly unavailable) for each entry.
pub struct SystemSensors {
    providers: Vec<Box<dyn Provider>>,
    aliases: Vec<Alias>,
    catalog: Vec<SensorInfo>,
}

impl SystemSensors {
    /// Discovers every sensor this machine offers. Never fails: whatever
    /// cannot be read shows up as unavailable, with the reason.
    pub fn new() -> Self {
        let (providers, gpus) = platform();
        Self::assemble(providers, gpus)
    }

    /// Linux sensors read from other `/sys` and `/proc` trees (tests, or a
    /// container that mounts the host's trees elsewhere). NVIDIA GPUs, which
    /// come from NVML rather than sysfs, are not included.
    #[cfg(target_os = "linux")]
    pub fn with_roots(
        sys: impl Into<std::path::PathBuf>,
        proc: impl Into<std::path::PathBuf>,
    ) -> Self {
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

#[cfg(target_os = "linux")]
fn platform() -> (Vec<Box<dyn Provider>>, Vec<FoundGpu>) {
    let roots = crate::linux::Roots::host();
    let mut gpus = crate::nvidia::discover();
    gpus.extend(crate::amdgpu::discover(&roots));
    (crate::linux::providers(&roots), gpus)
}

#[cfg(windows)]
fn platform() -> (Vec<Box<dyn Provider>>, Vec<FoundGpu>) {
    (crate::windows::providers(), crate::nvidia::discover())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform() -> (Vec<Box<dyn Provider>>, Vec<FoundGpu>) {
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
            let reading = out.get(&alias.target);
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

    #[cfg(target_os = "linux")]
    #[test]
    fn a_fake_machine_answers_for_every_catalog_entry() {
        use bezel_core::domain::sensor::Category;
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
        assert_eq!(catalog[0].key.as_str(), "cpu.usage");
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
        let gpu = |k: &str| snapshot.get(&bezel_core::domain::sensor::SensorKey::new(k).unwrap());
        assert_eq!(
            gpu("gpu.0.usage"),
            bezel_core::domain::sensor::Reading::Value(42.0)
        );
        assert_eq!(gpu("gpu.usage"), gpu("gpu.0.usage"));
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
