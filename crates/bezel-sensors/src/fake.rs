//! A scripted [`SensorSource`] for tests and demo mode.

use bezel_core::Result;
use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot};
use bezel_core::ports::SensorSource;

use crate::provider::{WARMING_UP, describe};

/// Replays a fixed catalog and a script of snapshots: each `sample` returns
/// the next snapshot, and the last one repeats once the script runs out.
#[derive(Debug, Clone, Default)]
pub struct FakeSensors {
    catalog: Vec<SensorInfo>,
    script: Vec<Snapshot>,
    taken: usize,
}

impl FakeSensors {
    /// A source offering `catalog` and answering with `script` in order.
    pub fn new(catalog: Vec<SensorInfo>, script: Vec<Snapshot>) -> Self {
        Self {
            catalog,
            script,
            taken: 0,
        }
    }

    /// A plausible desktop: two samples, the first still warming up its
    /// rates like a real source, the second with every value.
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
        "cpu.usage",
        Category::Cpu,
        "CPU usage",
        Quantity::Percent,
        Demo::Value(12.5),
        true,
    ),
    (
        "cpu.frequency",
        Category::Cpu,
        "CPU frequency (average)",
        Quantity::Megahertz,
        Demo::Value(4725.0),
        false,
    ),
    (
        "cpu.temperature",
        Category::Cpu,
        "CPU temperature",
        Quantity::Celsius,
        Demo::Value(46.0),
        false,
    ),
    (
        "cpu.power",
        Category::Cpu,
        "CPU package power",
        Quantity::Watts,
        Demo::Unavailable("demo: the RAPL energy counter is readable by root only"),
        false,
    ),
    (
        "cpu.load.1",
        Category::Cpu,
        "Load average (1 min)",
        Quantity::Number,
        Demo::Value(1.49),
        false,
    ),
    (
        "cpu.name",
        Category::Cpu,
        "CPU model",
        Quantity::Text,
        Demo::Text("Demo CPU 12-Core Processor"),
        false,
    ),
    (
        "gpu.usage",
        Category::Gpu,
        "GPU usage",
        Quantity::Percent,
        Demo::Value(38.0),
        false,
    ),
    (
        "gpu.temperature",
        Category::Gpu,
        "GPU temperature",
        Quantity::Celsius,
        Demo::Value(35.0),
        false,
    ),
    (
        "gpu.memory.used",
        Category::Gpu,
        "GPU memory used",
        Quantity::Bytes,
        Demo::Value(2701.0 * MIB),
        false,
    ),
    (
        "gpu.power",
        Category::Gpu,
        "GPU power",
        Quantity::Watts,
        Demo::Value(25.5),
        false,
    ),
    (
        "gpu.name",
        Category::Gpu,
        "GPU model",
        Quantity::Text,
        Demo::Text("Demo GPU"),
        false,
    ),
    (
        "memory.used",
        Category::Memory,
        "RAM used",
        Quantity::Bytes,
        Demo::Value(12.0 * GIB),
        false,
    ),
    (
        "memory.total",
        Category::Memory,
        "RAM total",
        Quantity::Bytes,
        Demo::Value(64.0 * GIB),
        false,
    ),
    (
        "memory.percent",
        Category::Memory,
        "RAM used (percent)",
        Quantity::Percent,
        Demo::Value(18.75),
        false,
    ),
    (
        "disk.read",
        Category::Disk,
        "Disk read rate",
        Quantity::BytesPerSecond,
        Demo::Value(0.0),
        true,
    ),
    (
        "disk.write",
        Category::Disk,
        "Disk write rate",
        Quantity::BytesPerSecond,
        Demo::Value(4.0 * MIB),
        true,
    ),
    (
        "net.down",
        Category::Network,
        "Download rate",
        Quantity::BytesPerSecond,
        Demo::Value(1.5 * MIB),
        true,
    ),
    (
        "net.up",
        Category::Network,
        "Upload rate",
        Quantity::BytesPerSecond,
        Demo::Value(64.0 * 1024.0),
        true,
    ),
    (
        "hwmon.demo.fan1",
        Category::Board,
        "demo fan1",
        Quantity::Rpm,
        Demo::Value(1200.0),
        false,
    ),
    (
        "system.uptime",
        Category::System,
        "Uptime",
        Quantity::Seconds,
        Demo::Value(93_784.0),
        false,
    ),
    (
        "system.hostname",
        Category::System,
        "Host name",
        Quantity::Text,
        Demo::Text("bezel-demo"),
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
            first.get(&key("cpu.usage")),
            Reading::Unavailable(_)
        ));
        assert_eq!(first.get(&key("cpu.temperature")), Reading::Value(46.0));
        let second = fake.sample().unwrap();
        assert_eq!(second.get(&key("cpu.usage")), Reading::Value(12.5));
        assert_eq!(
            second.get(&key("gpu.name")),
            Reading::Text("Demo GPU".into())
        );
        assert_eq!(fake.sample().unwrap(), second);
        assert_eq!(fake.samples_taken(), 3);
        for info in &catalog {
            assert!(second.iter().any(|(k, _)| *k == info.key), "{}", info.key);
        }
    }

    #[test]
    fn an_empty_script_reads_nothing() {
        let mut fake = FakeSensors::default();
        assert!(fake.catalog().unwrap().is_empty());
        assert!(fake.sample().unwrap().is_empty());
    }
}
