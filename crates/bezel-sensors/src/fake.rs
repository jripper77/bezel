//! A scripted [`SensorSource`] for tests and demo mode.

use bezel_core::Result;
use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, Wanted, keys};
use bezel_core::ports::SensorSource;

use crate::provider::{NOT_SUPPORTED_YET, WARMING_UP, describe};

/// Replays a fixed catalog and a script of snapshots: each `sample` returns
/// the next snapshot, and the last one repeats once the script runs out.
/// It records what its callers say they show ([`Self::wanted`]) and
/// measures every sensor anyway.
#[derive(Debug, Clone, Default)]
pub struct FakeSensors {
    catalog: Vec<SensorInfo>,
    script: Vec<Snapshot>,
    taken: usize,
    wanted: Option<Wanted>,
}

impl FakeSensors {
    /// A source offering `catalog` and answering with `script` in order.
    pub fn new(catalog: Vec<SensorInfo>, script: Vec<Snapshot>) -> Self {
        Self {
            catalog,
            script,
            taken: 0,
            wanted: None,
        }
    }

    /// What the last `want` said is shown (`None` before any).
    pub fn wanted(&self) -> Option<&Wanted> {
        self.wanted.as_ref()
    }

    /// A plausible desktop: two samples, the first still warming up its
    /// rates like a real source, the second with every value. It offers
    /// every key imported themes use ([`keys::IMPORTED`]).
    pub fn demo() -> Self {
        let mut catalog = Vec::new();
        let mut first = Snapshot::default();
        let mut second = Snapshot::default();
        for (key, category, label, quantity, value, is_rate) in DEMO {
            let Some(info) = describe(key, *category, *label, *quantity, "demo") else {
                continue;
            };
            let key = info.key.clone();
            catalog.push(info);
            second.insert(key.clone(), value.reading());
            let early = if *is_rate {
                Reading::Unavailable(WARMING_UP.into())
            } else {
                value.reading()
            };
            first.insert(key, early);
        }
        Self::new(catalog, vec![first, second])
    }

    /// How many samples were taken.
    pub fn samples_taken(&self) -> usize {
        self.taken
    }

    /// The snapshot the next `sample` returns.
    fn next_snapshot(&self) -> Snapshot {
        let last = self.script.len().saturating_sub(1);
        self.script
            .get(self.taken.min(last))
            .cloned()
            .unwrap_or_default()
    }
}

impl SensorSource for FakeSensors {
    fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
        Ok(self.catalog.clone())
    }

    fn sample(&mut self) -> Result<Snapshot> {
        let snapshot = self.next_snapshot();
        self.taken += 1;
        Ok(snapshot)
    }

    fn want(&mut self, wanted: &Wanted) {
        self.wanted = Some(wanted.clone());
    }
}

/// A demo value.
enum Demo {
    Value(f64),
    Text(&'static str),
    Unavailable(&'static str),
}

impl Demo {
    fn reading(&self) -> Reading {
        match self {
            Demo::Value(v) => Reading::Value(*v),
            Demo::Text(t) => Reading::Text((*t).to_string()),
            Demo::Unavailable(r) => Reading::Unavailable((*r).to_string()),
        }
    }
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

type DemoRow = (&'static str, Category, &'static str, Quantity, Demo, bool);

const DEMO: &[DemoRow] = &[
    (
        keys::CPU_USAGE,
        Category::Cpu,
        "CPU usage",
        Quantity::Percent,
        Demo::Value(12.5),
        true,
    ),
    (
        keys::CPU_FREQUENCY,
        Category::Cpu,
        "CPU frequency (average)",
        Quantity::Megahertz,
        Demo::Value(4725.0),
        false,
    ),
    (
        keys::CPU_TEMPERATURE,
        Category::Cpu,
        "CPU temperature",
        Quantity::Celsius,
        Demo::Value(46.0),
        false,
    ),
    (
        keys::CPU_POWER,
        Category::Cpu,
        "CPU package power",
        Quantity::Watts,
        Demo::Unavailable("demo: the RAPL energy counter is readable by root only"),
        false,
    ),
    (
        keys::CPU_LOAD_1,
        Category::Cpu,
        "Load average (1 min)",
        Quantity::Number,
        Demo::Value(1.49),
        false,
    ),
    (
        keys::CPU_NAME,
        Category::Cpu,
        "CPU model",
        Quantity::Text,
        Demo::Text("Demo CPU 12-Core Processor"),
        false,
    ),
    (
        keys::CPU_FAN,
        Category::Cpu,
        "CPU fan",
        Quantity::Rpm,
        Demo::Value(1200.0),
        false,
    ),
    (
        keys::CPU_VOLTAGE,
        Category::Cpu,
        "CPU core voltage",
        Quantity::Volts,
        Demo::Value(1.104),
        false,
    ),
    (
        keys::GPU_USAGE,
        Category::Gpu,
        "GPU usage",
        Quantity::Percent,
        Demo::Value(38.0),
        false,
    ),
    (
        keys::GPU_TEMPERATURE,
        Category::Gpu,
        "GPU temperature",
        Quantity::Celsius,
        Demo::Value(35.0),
        false,
    ),
    (
        keys::GPU_MEMORY_USED,
        Category::Gpu,
        "GPU memory used",
        Quantity::Bytes,
        Demo::Value(2701.0 * MIB),
        false,
    ),
    (
        keys::GPU_POWER,
        Category::Gpu,
        "GPU power",
        Quantity::Watts,
        Demo::Value(25.5),
        false,
    ),
    (
        keys::GPU_VOLTAGE,
        Category::Gpu,
        "GPU core voltage",
        Quantity::Volts,
        Demo::Unavailable("demo: GPU 0 does not report its core voltage"),
        false,
    ),
    (
        keys::GPU_FPS,
        Category::Gpu,
        "Game frame rate",
        Quantity::Number,
        Demo::Value(144.0),
        false,
    ),
    (
        keys::GPU_NAME,
        Category::Gpu,
        "GPU model",
        Quantity::Text,
        Demo::Text("Demo GPU"),
        false,
    ),
    (
        keys::MEMORY_USED,
        Category::Memory,
        "RAM used",
        Quantity::Bytes,
        Demo::Value(12.0 * GIB),
        false,
    ),
    (
        keys::MEMORY_TOTAL,
        Category::Memory,
        "RAM total",
        Quantity::Bytes,
        Demo::Value(64.0 * GIB),
        false,
    ),
    (
        keys::MEMORY_PERCENT,
        Category::Memory,
        "RAM used (percent)",
        Quantity::Percent,
        Demo::Value(18.75),
        false,
    ),
    (
        keys::MEMORY_AVAILABLE_PERCENT,
        Category::Memory,
        "RAM available (percent)",
        Quantity::Percent,
        Demo::Value(81.25),
        false,
    ),
    (
        keys::DISK_READ,
        Category::Disk,
        "Disk read rate",
        Quantity::BytesPerSecond,
        Demo::Value(0.0),
        true,
    ),
    (
        keys::DISK_WRITE,
        Category::Disk,
        "Disk write rate",
        Quantity::BytesPerSecond,
        Demo::Value(4.0 * MIB),
        true,
    ),
    (
        keys::NET_DOWN,
        Category::Network,
        "Download rate",
        Quantity::BytesPerSecond,
        Demo::Value(1.5 * MIB),
        true,
    ),
    (
        keys::NET_UP,
        Category::Network,
        "Upload rate",
        Quantity::BytesPerSecond,
        Demo::Value(64.0 * 1024.0),
        true,
    ),
    (
        keys::NET_DOWN_TOTAL,
        Category::Network,
        "Downloaded since boot",
        Quantity::Bytes,
        Demo::Value(3.2 * GIB),
        false,
    ),
    (
        keys::NET_UP_TOTAL,
        Category::Network,
        "Uploaded since boot",
        Quantity::Bytes,
        Demo::Value(410.0 * MIB),
        false,
    ),
    (
        keys::NET_PING,
        Category::Network,
        "Ping",
        Quantity::Number,
        Demo::Value(12.0),
        false,
    ),
    (
        keys::FAN_PUMP,
        Category::Board,
        "Pump",
        Quantity::Rpm,
        Demo::Value(2400.0),
        false,
    ),
    (
        keys::FAN_CASE_1,
        Category::Board,
        "Case fan 1",
        Quantity::Rpm,
        Demo::Value(850.0),
        false,
    ),
    (
        keys::FAN_CASE_2,
        Category::Board,
        "Case fan 2",
        Quantity::Rpm,
        Demo::Unavailable("demo: no second hwmon fan is labelled chassis, case or system"),
        false,
    ),
    (
        keys::UPTIME,
        Category::System,
        "Uptime",
        Quantity::Seconds,
        Demo::Value(93_784.0),
        false,
    ),
    (
        keys::HOSTNAME,
        Category::System,
        "Host name",
        Quantity::Text,
        Demo::Text("bezel-demo"),
        false,
    ),
    (
        keys::SYSTEM_VOLUME,
        Category::System,
        "Output volume",
        Quantity::Percent,
        Demo::Unavailable(NOT_SUPPORTED_YET),
        false,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::sensor::SensorKey;

    fn key(k: &str) -> SensorKey {
        SensorKey::new(k).unwrap()
    }

    #[test]
    fn demo_warms_up_then_repeats_the_last_snapshot() {
        let mut fake = FakeSensors::demo();
        let catalog = fake.catalog().unwrap();
        assert_eq!(catalog.len(), DEMO.len());
        let first = fake.sample().unwrap();
        assert!(matches!(
            first.get(&key(keys::CPU_USAGE)),
            Reading::Unavailable(_)
        ));
        assert_eq!(first.get(&key(keys::CPU_TEMPERATURE)), Reading::Value(46.0));
        let second = fake.sample().unwrap();
        assert_eq!(second.get(&key(keys::CPU_USAGE)), Reading::Value(12.5));
        assert_eq!(
            second.get(&key(keys::GPU_NAME)),
            Reading::Text("Demo GPU".into())
        );
        assert_eq!(fake.sample().unwrap(), second);
        assert_eq!(fake.samples_taken(), 3);
        for info in &catalog {
            assert!(second.iter().any(|(k, _)| *k == info.key), "{}", info.key);
        }
    }

    #[test]
    fn demo_offers_every_imported_key() {
        let catalog = FakeSensors::demo().catalog().unwrap();
        for imported in keys::IMPORTED {
            assert!(
                catalog.iter().any(|i| i.key.as_str() == imported),
                "{imported}"
            );
        }
        for info in &catalog {
            assert_eq!(
                info.quantity,
                bezel_core::domain::sensor::quantity_of(&info.key),
                "{}: the demo and the key's own unit agree",
                info.key
            );
        }
    }

    #[test]
    fn an_empty_script_reads_nothing() {
        let mut fake = FakeSensors::default();
        assert!(fake.catalog().unwrap().is_empty());
        assert!(fake.sample().unwrap().is_empty());
    }

    #[test]
    fn what_is_wanted_is_recorded_and_everything_still_measured() {
        let mut fake = FakeSensors::demo();
        assert_eq!(fake.wanted(), None);
        fake.want(&Wanted::nothing());
        assert_eq!(fake.wanted(), Some(&Wanted::nothing()));
        fake.sample().unwrap();
        let all = fake.sample().unwrap();
        assert_eq!(all.get(&key(keys::NET_PING)), Reading::Value(12.0));
        fake.want(&Wanted::All);
        assert_eq!(fake.wanted(), Some(&Wanted::All));
    }
}
