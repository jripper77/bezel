//! [`SystemSensors`]: the machine's real sensors behind the port.

use std::time::Instant;

use bezel_core::Result;
use bezel_core::domain::sensor::{SensorInfo, Snapshot};
use bezel_core::ports::SensorSource;

use crate::provider::Provider;

/// Measures this machine by composing the platform's providers. Discovery
/// happens once, in [`SystemSensors::new`]; the catalog is then fixed and
/// every `sample` returns a reading (possibly unavailable) for each entry.
pub struct SystemSensors {
    providers: Vec<Box<dyn Provider>>,
    catalog: Vec<SensorInfo>,
}

impl SystemSensors {
    /// Discovers every sensor this machine offers. Never fails: whatever
    /// cannot be read shows up as unavailable, with the reason.
    pub fn new() -> Self {
        Self::from_providers(platform_providers())
    }

    /// Linux sensors read from other `/sys` and `/proc` trees (tests, or a
    /// container that mounts the host's trees elsewhere). GPUs through NVML
    /// are not included.
    #[cfg(target_os = "linux")]
    pub fn with_roots(
        sys: impl Into<std::path::PathBuf>,
        proc: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self::from_providers(crate::linux::providers(&crate::linux::Roots::new(
            sys, proc,
        )))
    }

    fn from_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        let mut catalog: Vec<SensorInfo> = providers.iter().flat_map(|p| p.catalog()).collect();
        // Headline sensors first in each category, per-device ones after
        // (the order is presentation only; keys are what themes bind to).
        catalog.sort_by_key(|info| (info.category, rank(info.key.as_str())));
        Self { providers, catalog }
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
fn platform_providers() -> Vec<Box<dyn Provider>> {
    crate::linux::providers(&crate::linux::Roots::host())
}

#[cfg(not(target_os = "linux"))]
fn platform_providers() -> Vec<Box<dyn Provider>> {
    Vec::new()
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
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::sensor::Category;

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
        let t = crate::testing::FakeTree::new("system");
        t.file("proc/stat", "cpu  1 0 1 8 0 0 0 0\ncpu0 1 0 1 8 0 0 0 0\n")
            .file("proc/meminfo", "MemTotal: 1024 kB\nMemAvailable: 512 kB\n")
            .file("proc/uptime", "10.5 20.0\n")
            .file("sys/class/hwmon/hwmon0/name", "k10temp\n")
            .file("sys/class/hwmon/hwmon0/temp1_input", "50000\n")
            .file("sys/class/hwmon/hwmon0/temp1_label", "Tctl\n");
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
