//! [`SystemSensors`]: the machine's real sensors behind the port, and the
//! [`SensorOptions`] of the sources that take settings.

use std::path::PathBuf;
use std::time::Instant;

use bezel_core::Result;
use bezel_core::domain::sensor::{SensorInfo, Snapshot, Wanted};
use bezel_core::ports::SensorSource;

use crate::gpu::{Alias, FoundGpu, number};
use crate::provider::Provider;

/// Settings of the sources that take them (the studio's settings and the
/// CLI's flags fill them in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorOptions {
    /// Host `net.ping` measures the round trip to, only while a shown
    /// value uses it ([`SensorSource::want`]).
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
/// starts when it is built; providers can add sensors when hardware becomes
/// ready later. Previously discovered keys remain available in the catalog.
/// `net.ping` sends packets only while [`SensorSource::want`] says it is
/// shown; until the first `want`, nothing is.
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
        let (mut providers, gpus) = platform();
        providers.push(Box::new(crate::fps::provider(&options)));
        providers.push(Box::new(crate::ping::Ping::new(&options.ping_host)));
        providers.push(Box::new(crate::weather::WeatherProvider::default()));
        Self::assemble(providers, gpus)
    }

    /// Linux sensors read from fake `/sys` and `/proc` trees, game FPS from
    /// MangoHud logs in `sys/../mangohud` and `ping`. NVIDIA GPUs, which
    /// come from NVML rather than sysfs, are not included.
    #[cfg(all(test, target_os = "linux"))]
    fn with_roots(
        sys: impl Into<PathBuf>,
        proc: impl Into<PathBuf>,
        ping: crate::ping::Ping,
    ) -> Self {
        let sys = sys.into();
        let logs = sys.with_file_name("mangohud");
        let roots = crate::linux::Roots::new(sys, proc);
        let mut providers = crate::linux::providers(&roots);
        providers.push(Box::new(crate::fps::Fps::new(
            crate::fps::mangohud::MangoHud::new(Some(logs)),
        )));
        providers.push(Box::new(ping));
        Self::assemble(providers, crate::amdgpu::discover(&roots))
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

/// The platform's providers and GPUs (game FPS and ping, which take
/// options, are added by [`SystemSensors::with_options`]).
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
        let mut known: std::collections::HashSet<_> =
            self.catalog.iter().map(|info| info.key.clone()).collect();
        for info in self
            .providers
            .iter()
            .flat_map(|provider| provider.catalog())
        {
            if known.insert(info.key.clone()) {
                self.catalog.push(info);
            }
        }
        self.catalog
            .sort_by_key(|info| (info.category, rank(info.key.as_str())));
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

    /// Passes what is shown to every provider: the ping starts or stops
    /// its probes (D-2026-09-30-release-polish-11).
    fn want(&mut self, wanted: &Wanted) {
        for provider in &mut self.providers {
            provider.want(wanted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ping whose probe answers in 12 ms at once, every `every`, and the
    /// count of its probes.
    #[cfg(target_os = "linux")]
    fn answering_ping(
        every: std::time::Duration,
    ) -> (
        crate::ping::Ping,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Answers(Arc<AtomicUsize>);
        impl crate::ping::Probe for Answers {
            fn round_trip(&mut self) -> std::result::Result<std::time::Duration, String> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(std::time::Duration::from_millis(12))
            }
        }
        let probes = Arc::new(AtomicUsize::new(0));
        let ping = crate::ping::Ping::with_probe(
            "192.0.2.7",
            Answers(Arc::clone(&probes)),
            every,
            std::time::Duration::from_secs(1),
        );
        (ping, probes)
    }

    /// Samples until `net.ping` is past "measuring" (at most 3 s).
    #[cfg(target_os = "linux")]
    fn settled(sensors: &mut SystemSensors) -> Snapshot {
        use bezel_core::domain::sensor::{Reading, SensorKey, keys};
        let ping = SensorKey::new(keys::NET_PING).unwrap();
        let start = Instant::now();
        loop {
            let snapshot = sensors.sample().unwrap();
            let measuring = matches!(
                snapshot.get(&ping),
                Reading::Unavailable(why) if why.starts_with("measuring")
            );
            if !measuring || start.elapsed() > std::time::Duration::from_secs(3) {
                return snapshot;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn catalog_includes_hardware_ready_after_the_first_sample() {
        use bezel_core::domain::sensor::{Category, Quantity};
        struct Late(bool);
        impl Provider for Late {
            fn catalog(&self) -> Vec<SensorInfo> {
                if !self.0 {
                    return vec![];
                }
                crate::provider::describe(
                    "lhm.psu.corsair.0.power.14",
                    Category::Board,
                    "Total Output",
                    Quantity::Watts,
                    "LibreHardwareMonitor",
                )
                .into_iter()
                .collect()
            }
            fn sample(&mut self, _: Instant, _: &mut Snapshot) {
                self.0 = true;
            }
        }
        let mut sensors = SystemSensors::assemble(vec![Box::new(Late(false))], vec![]);
        let initial = sensors.catalog().unwrap();
        assert!(
            !initial
                .iter()
                .any(|info| info.key.as_str() == "lhm.psu.corsair.0.power.14")
        );
        sensors.sample().unwrap();
        assert_eq!(sensors.catalog().unwrap().len(), initial.len() + 1);
        assert_eq!(sensors.catalog().unwrap().len(), initial.len() + 1);
    }

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
        let (ping, _) = answering_ping(std::time::Duration::from_secs(1));
        let mut sensors = SystemSensors::with_roots(t.path("sys"), t.path("proc"), ping);
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
        assert_eq!(get(keys::MEMORY_AVAILABLE_PERCENT), Reading::Value(50.0));
        assert!(matches!(get(keys::GPU_VOLTAGE), Reading::Unavailable(r) if r.contains("voltage")));
    }

    /// Themes imported from the vendor app and from the Python project bind
    /// to [`keys::IMPORTED`]: each is listed and answered, measured or with
    /// the reason, game FPS and ping included.
    #[cfg(target_os = "linux")]
    #[test]
    fn imported_keys_are_published() {
        use bezel_core::domain::sensor::{Reading, SensorKey, keys};
        use std::time::Duration;
        let t = crate::testing::FakeTree::new("imported");
        t.file("proc/stat", "cpu  1 0 1 8 0 0 0 0\n")
            .file("proc/meminfo", "MemTotal: 1024 kB\nMemAvailable: 256 kB\n")
            .file(
                "mangohud/witcher3_2026-09-30_21-05-00.csv",
                include_str!("fps/fixtures/mangohud/witcher3_2026-09-30_21-05-00.csv"),
            )
            .dir("sys");
        let (ping, _) = answering_ping(Duration::from_secs(1));
        let mut sensors = SystemSensors::with_roots(t.path("sys"), t.path("proc"), ping);
        let catalog = sensors.catalog().unwrap();
        let listed: Vec<&str> = catalog.iter().map(|i| i.key.as_str()).collect();
        for key in keys::IMPORTED {
            assert!(listed.contains(&key), "{key} not listed");
        }
        // Listed like `bezel sensors` does: every sensor shown.
        sensors.want(&bezel_core::domain::sensor::Wanted::All);
        let snapshot = settled(&mut sensors);
        for key in keys::IMPORTED {
            assert!(
                snapshot.iter().any(|(k, _)| k.as_str() == key),
                "{key} not sampled"
            );
        }
        let get = |k: &str| snapshot.get(&SensorKey::new(k).unwrap());
        assert_eq!(get(keys::GPU_FPS), Reading::Value(139.874));
        assert_eq!(get(keys::NET_PING), Reading::Value(12.0));
        assert_eq!(get(keys::MEMORY_AVAILABLE_PERCENT), Reading::Value(25.0));
        assert!(matches!(get(keys::SYSTEM_VOLUME), Reading::Unavailable(_)));
    }

    /// A theme showing `shown` as text values.
    #[cfg(target_os = "linux")]
    fn theme_showing(shown: &[&str]) -> bezel_core::domain::theme::Theme {
        use bezel_core::domain::geometry::{Orientation, Size};
        use bezel_core::domain::sensor::{DisplayFormat, SensorKey};
        use bezel_core::domain::theme::{
            BoxF, Element, ElementId, ElementKind, TextContent, TextStyle, Theme,
        };
        let mut theme = Theme::blank("t", Size::new(480, 1920), Orientation::Portrait);
        for (id, key) in (1..).zip(shown) {
            theme.elements.push(Element {
                id: ElementId(id),
                name: (*key).to_string(),
                frame: BoxF::new(0.0, 0.0, 100.0, 40.0),
                opacity: 1.0,
                visible: true,
                locked: false,
                kind: ElementKind::Text {
                    content: TextContent::Sensor {
                        key: SensorKey::new(*key).unwrap(),
                        format: DisplayFormat::default(),
                        prefix: String::new(),
                        suffix: String::new(),
                    },
                    style: TextStyle::default(),
                },
            });
        }
        theme
    }

    /// The whole way from a theme to the probe (D-2026-09-30-release-polish-11):
    /// the core's runtime says what its theme shows, and only a theme with a
    /// `net.ping` value makes the probe run.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_theme_pings_only_while_it_shows_net_ping() {
        use bezel_core::app::ThemeRuntime;
        use bezel_core::domain::clock::Language;
        use bezel_core::domain::sensor::{Reading, SensorKey, keys};
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        let t = crate::testing::FakeTree::new("on-demand");
        t.file("proc/stat", "cpu  1 0 1 8 0 0 0 0\n")
            .file("proc/meminfo", "MemTotal: 1024 kB\nMemAvailable: 256 kB\n")
            .dir("sys");
        let (ping, probes) = answering_ping(Duration::from_millis(5));
        let mut sensors = SystemSensors::with_roots(t.path("sys"), t.path("proc"), ping);
        let without = theme_showing(&[keys::CPU_USAGE, keys::MEMORY_PERCENT]);
        let mut runtime = ThemeRuntime::new(without.clone(), Default::default(), Language::English);
        let net_ping = SensorKey::new(keys::NET_PING).unwrap();
        let not_shown = Reading::Unavailable(crate::ping::NOT_SHOWN.into());
        for _ in 0..5 {
            runtime.sample(&mut sensors).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(probes.load(Ordering::SeqCst), 0, "no probe at all");
        assert_eq!(runtime.snapshot().get(&net_ping), not_shown);

        // Another theme shows the ping: it is measured.
        runtime.replace_theme(theme_showing(&[keys::CPU_USAGE, keys::NET_PING]));
        let start = std::time::Instant::now();
        while runtime.snapshot().get(&net_ping) != Reading::Value(12.0)
            && start.elapsed() < Duration::from_secs(3)
        {
            runtime.sample(&mut sensors).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(runtime.snapshot().get(&net_ping), Reading::Value(12.0));
        assert!(probes.load(Ordering::SeqCst) > 0);

        // Back to a theme without it: the probes stop.
        runtime.replace_theme(without);
        runtime.sample(&mut sensors).unwrap();
        assert_eq!(runtime.snapshot().get(&net_ping), not_shown);
        std::thread::sleep(Duration::from_millis(50));
        let idle = probes.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(probes.load(Ordering::SeqCst), idle, "no probe while hidden");
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
