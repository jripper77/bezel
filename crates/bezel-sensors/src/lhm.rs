//! LibreHardwareMonitor's sensors (WMI class `Sensor` in the namespace
//! `root\LibreHardwareMonitor`) mapped to keys: every sensor as
//! `lhm.<identifier>` (`/amdcpu/0/temperature/2` → `lhm.amdcpu.0.temperature.2`)
//! plus `cpu.temperature`, `cpu.temperature.ccdN` and `cpu.power` picked by
//! name from the CPU's own sensors, and `cpu.fan`, `fan.pump`, `fan.case1`,
//! `fan.case2` and `cpu.voltage` picked by name with the same label rules as
//! hwmon on Linux (D-2026-09-30-release-polish-5).
//!
//! LHM units: `Data` is GiB and `SmallData` MiB (binary, whatever the
//! "GB"/"MB" labels say), `Throughput` B/s, `Frequency` Hz, `Clock` MHz.
//! A sensor LHM reports as NaN is unavailable, not zero.
//!
//! This module is plain data mapping, compiled for tests on every platform;
//! the WMI query itself lives in `windows::wmi`.

use std::collections::HashMap;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, keys};

use crate::provider::{FanRole, cpu_core_voltage, describe, fan_role, optional_fan, slug};

/// Shown for the LHM-backed keys when the namespace is missing.
pub(crate) const HINT: &str = "configure Bezel's bundled sensor helper; alternatively run LibreHardwareMonitor \
     with Options > Remote Web Server > Run on port 8085 to read CPU temperature, fans and power";

/// CPU temperature sensor names, best first: AMD's `Core (Tctl/Tdie)`, then
/// the package sensor Intel and newer AMD report, then per-core summaries.
const CPU_TEMPERATURE: [&str; 7] = [
    "Core (Tctl/Tdie)",
    "CPU Package",
    "Core (Tdie)",
    "Core (Tctl)",
    "Package",
    "Core Max",
    "Core Average",
];

/// CPU package power names, best first.
const CPU_POWER: [&str; 2] = ["Package", "CPU Package"];

/// One `Sensor` instance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    /// `/amdcpu/0/temperature/2`.
    pub(crate) identifier: String,
    /// `Core (Tctl/Tdie)`.
    pub(crate) name: String,
    /// `Temperature`, `Load`, `Fan`, ...
    pub(crate) sensor_type: String,
    /// In LHM's unit for the type; NaN when unknown.
    pub(crate) value: f32,
    /// Identifier of the hardware (`/amdcpu/0`).
    pub(crate) parent: String,
}

/// Quantity and multiplier to it for an LHM sensor type.
fn unit(sensor_type: &str) -> (Quantity, f64) {
    match sensor_type {
        "Temperature" => (Quantity::Celsius, 1.0),
        "Load" | "Control" | "Level" | "Humidity" => (Quantity::Percent, 1.0),
        "Clock" => (Quantity::Megahertz, 1.0),
        "Frequency" => (Quantity::Megahertz, 1e-6),
        "Power" => (Quantity::Watts, 1.0),
        "Voltage" => (Quantity::Volts, 1.0),
        "Current" => (Quantity::Amperes, 1.0),
        "Fan" => (Quantity::Rpm, 1.0),
        "Data" => (Quantity::Bytes, 1024.0 * 1024.0 * 1024.0),
        "SmallData" => (Quantity::Bytes, 1024.0 * 1024.0),
        "Throughput" => (Quantity::BytesPerSecond, 1.0),
        "TimeSpan" => (Quantity::Seconds, 1.0),
        _ => (Quantity::Number, 1.0),
    }
}

/// Category from the hardware part of an identifier.
fn category(identifier: &str) -> Category {
    let hardware = identifier
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default();
    match hardware {
        "amdcpu" | "intelcpu" | "cpu" => Category::Cpu,
        h if h.starts_with("gpu") || h.ends_with("gpu") => Category::Gpu,
        "ram" | "memory" | "vram" => Category::Memory,
        "hdd" | "nvme" | "ssd" | "storage" => Category::Disk,
        "nic" => Category::Network,
        _ => Category::Board,
    }
}

fn is_cpu(row: &Row) -> bool {
    category(&row.identifier) == Category::Cpu
}

/// `lhm.` and the identifier's segments, slugged.
pub(crate) fn key_of(identifier: &str) -> String {
    let segments: Vec<String> = identifier
        .split('/')
        .map(slug)
        .filter(|s| !s.is_empty())
        .collect();
    format!("lhm.{}", segments.join("."))
}

fn pick<'a>(rows: &'a [Row], sensor_type: &str, names: &[&str]) -> Option<&'a Row> {
    names.iter().find_map(|name| {
        rows.iter()
            .find(|r| is_cpu(r) && r.sensor_type == sensor_type && r.name == *name)
    })
}

/// The `nth` fan (0 = first) named for `role`, board and CPU fans only (a
/// graphics card's fan is its own); for the CPU a plainly named fan before
/// an optional header.
fn fan(rows: &[Row], role: FanRole, nth: usize) -> Option<&Row> {
    let mut fans: Vec<&Row> = rows
        .iter()
        .filter(|r| r.sensor_type == "Fan" && category(&r.identifier) != Category::Gpu)
        .filter(|r| fan_role(&r.name) == Some(role))
        .collect();
    fans.sort_by_key(|r| optional_fan(&r.name));
    fans.get(nth).copied()
}

/// The CPU core rail: the CPU's own sensor before the board's.
fn cpu_voltage(rows: &[Row]) -> Option<&Row> {
    rows.iter()
        .filter(|r| r.sensor_type == "Voltage" && category(&r.identifier) != Category::Gpu)
        .filter(|r| cpu_core_voltage(&r.name))
        .min_by_key(|r| !is_cpu(r))
}

/// `CCD1 (Tdie)` → 1.
fn ccd(row: &Row) -> Option<u32> {
    if !is_cpu(row) || row.sensor_type != "Temperature" {
        return None;
    }
    let rest = row.name.strip_prefix("CCD")?.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// A key LHM feeds and where its value comes from.
#[derive(Debug, Clone, PartialEq)]
enum Source {
    /// One sensor by identifier, times the unit multiplier.
    Sensor { identifier: String, scale: f64 },
    /// The best CPU temperature of the moment.
    CpuTemperature,
    /// The CPU package power.
    CpuPower,
    /// CCD `n` temperature.
    Ccd(u32),
    /// A board fan or rail picked by name, and why it may be missing.
    Picked(Pick, &'static str),
}

/// How a key picked by name finds its sensor among the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    /// The `nth` fan named for a role.
    Fan(FanRole, usize),
    /// The CPU core rail.
    CpuVoltage,
}

impl Pick {
    /// The row the key reads now.
    fn row(self, rows: &[Row]) -> Option<&Row> {
        match self {
            Pick::Fan(role, nth) => fan(rows, role, nth),
            Pick::CpuVoltage => cpu_voltage(rows),
        }
    }
}

/// A key picked by name: its catalog entry and why it may be missing.
struct Picked {
    key: &'static str,
    category: Category,
    label: &'static str,
    quantity: Quantity,
    pick: Pick,
    missing: &'static str,
}

const PICKED: [Picked; 5] = [
    Picked {
        key: keys::CPU_FAN,
        category: Category::Cpu,
        label: "CPU fan",
        quantity: Quantity::Rpm,
        pick: Pick::Fan(FanRole::Cpu, 0),
        missing: "no LibreHardwareMonitor fan is named CPU",
    },
    Picked {
        key: keys::CPU_VOLTAGE,
        category: Category::Cpu,
        label: "CPU core voltage",
        quantity: Quantity::Volts,
        pick: Pick::CpuVoltage,
        missing: "no LibreHardwareMonitor voltage is named as the CPU core (Vcore)",
    },
    Picked {
        key: keys::FAN_PUMP,
        category: Category::Board,
        label: "Pump",
        quantity: Quantity::Rpm,
        pick: Pick::Fan(FanRole::Pump, 0),
        missing: "no LibreHardwareMonitor fan is named pump",
    },
    Picked {
        key: keys::FAN_CASE_1,
        category: Category::Board,
        label: "Case fan 1",
        quantity: Quantity::Rpm,
        pick: Pick::Fan(FanRole::Case, 0),
        missing: "no LibreHardwareMonitor fan is named chassis, case or system",
    },
    Picked {
        key: keys::FAN_CASE_2,
        category: Category::Board,
        label: "Case fan 2",
        quantity: Quantity::Rpm,
        pick: Pick::Fan(FanRole::Case, 1),
        missing: "no second LibreHardwareMonitor fan is named chassis, case or system",
    },
];

/// Catalog entries of the keys picked by name, sourced from the sensor
/// each one reads on the first query.
fn picked_entries(rows: &[Row]) -> Vec<(SensorInfo, Source)> {
    PICKED
        .iter()
        .filter_map(|p| {
            let from = p.pick.row(rows).map_or("LibreHardwareMonitor".into(), |r| {
                format!("LibreHardwareMonitor {}", r.name)
            });
            describe(p.key, p.category, p.label, p.quantity, from)
                .map(|info| (info, Source::Picked(p.pick, p.missing)))
        })
        .collect()
}

/// The keys LHM provides on this machine, fixed at discovery.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mapping {
    entries: Vec<(SensorInfo, Source)>,
}

impl Mapping {
    /// Catalog from the first query's rows (`None` when LHM is not running:
    /// only the well-known keys, which then explain how to get them).
    pub(crate) fn discover(rows: Option<&[Row]>, hardware: &HashMap<String, String>) -> Self {
        let rows = rows.unwrap_or_default();
        let c = Category::Cpu;
        let mut entries: Vec<(SensorInfo, Source)> = Vec::new();
        let temp = pick(rows, "Temperature", &CPU_TEMPERATURE)
            .map_or("LibreHardwareMonitor".into(), |r| {
                format!("LibreHardwareMonitor {}", r.name)
            });
        entries.extend(
            describe(
                keys::CPU_TEMPERATURE,
                c,
                "CPU temperature",
                Quantity::Celsius,
                temp,
            )
            .map(|i| (i, Source::CpuTemperature)),
        );
        let mut ccds: Vec<u32> = rows.iter().filter_map(ccd).collect();
        ccds.sort_unstable();
        ccds.dedup();
        for n in ccds {
            let key = format!("{}.ccd{n}", keys::CPU_TEMPERATURE);
            let label = format!("CPU CCD{n} temperature");
            entries.extend(
                describe(&key, c, label, Quantity::Celsius, "LibreHardwareMonitor")
                    .map(|i| (i, Source::Ccd(n))),
            );
        }
        entries.extend(
            describe(
                keys::CPU_POWER,
                c,
                "CPU package power",
                Quantity::Watts,
                "LibreHardwareMonitor",
            )
            .map(|i| (i, Source::CpuPower)),
        );
        entries.extend(picked_entries(rows));
        for row in rows {
            let (quantity, scale) = unit(&row.sensor_type);
            let owner = hardware
                .get(&row.parent)
                .map_or(row.parent.as_str(), String::as_str);
            let label = format!("{owner} {}", row.name);
            let source = format!(
                "LibreHardwareMonitor {} {}",
                row.sensor_type, row.identifier
            );
            let info = describe(
                &key_of(&row.identifier),
                category(&row.identifier),
                label,
                quantity,
                source,
            );
            let identifier = row.identifier.clone();
            entries.extend(info.map(|i| (i, Source::Sensor { identifier, scale })));
        }
        Self { entries }
    }

    /// The catalog entries.
    pub(crate) fn catalog(&self) -> Vec<SensorInfo> {
        self.entries.iter().map(|(info, _)| info.clone()).collect()
    }

    /// A reading for every entry from the latest rows, or `why` they are missing.
    pub(crate) fn readings(&self, rows: Result<&[Row], &str>) -> Vec<(SensorInfo, Reading)> {
        let rows = match rows {
            Ok(rows) => rows,
            Err(why) => {
                return self
                    .entries
                    .iter()
                    .map(|(info, _)| (info.clone(), Reading::Unavailable(why.to_string())))
                    .collect();
            }
        };
        let by_id: HashMap<&str, &Row> = rows.iter().map(|r| (r.identifier.as_str(), r)).collect();
        let value = |row: Option<&Row>, scale: f64, missing: &str| match row {
            Some(r) if r.value.is_finite() => Reading::Value(f64::from(r.value) * scale),
            Some(_) => Reading::Unavailable("LibreHardwareMonitor has no value for it".into()),
            None => Reading::Unavailable(missing.into()),
        };
        self.entries
            .iter()
            .map(|(info, source)| {
                let reading = match source {
                    Source::Sensor { identifier, scale } => value(
                        by_id.get(identifier.as_str()).copied(),
                        *scale,
                        "LibreHardwareMonitor no longer reports it",
                    ),
                    Source::CpuTemperature => value(
                        pick(rows, "Temperature", &CPU_TEMPERATURE),
                        1.0,
                        "LibreHardwareMonitor reports no CPU temperature",
                    ),
                    Source::CpuPower => value(
                        pick(rows, "Power", &CPU_POWER),
                        1.0,
                        "LibreHardwareMonitor reports no CPU package power",
                    ),
                    Source::Ccd(n) => value(
                        rows.iter().find(|r| ccd(r) == Some(*n)),
                        1.0,
                        "LibreHardwareMonitor no longer reports it",
                    ),
                    Source::Picked(pick, missing) => value(pick.row(rows), 1.0, missing),
                };
                (info.clone(), reading)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(identifier: &str, name: &str, sensor_type: &str, value: f32) -> Row {
        let parent = identifier
            .rsplitn(3, '/')
            .nth(2)
            .unwrap_or_default()
            .to_string();
        Row {
            identifier: identifier.into(),
            name: name.into(),
            sensor_type: sensor_type.into(),
            value,
            parent,
        }
    }

    fn rows() -> Vec<Row> {
        vec![
            row(
                "/amdcpu/0/temperature/2",
                "Core (Tctl/Tdie)",
                "Temperature",
                46.5,
            ),
            row(
                "/amdcpu/0/temperature/3",
                "CCD1 (Tdie)",
                "Temperature",
                41.75,
            ),
            row(
                "/amdcpu/0/temperature/4",
                "CCD2 (Tdie)",
                "Temperature",
                38.25,
            ),
            row("/amdcpu/0/power/0", "Package", "Power", 61.5),
            row("/amdcpu/0/load/0", "CPU Total", "Load", 12.0),
            row("/lpc/nct6798d/0/fan/1", "Fan #2", "Fan", 912.0),
            row(
                "/lpc/nct6798d/0/temperature/0",
                "CPU Package",
                "Temperature",
                99.0,
            ),
            row("/ram/data/0", "Memory Used", "Data", 12.5),
            row(
                "/gpu-nvidia/0/smalldata/1",
                "GPU Memory Used",
                "SmallData",
                2701.0,
            ),
            row(
                "/nic/{abc}/throughput/7",
                "Download Speed",
                "Throughput",
                1500.0,
            ),
            row(
                "/nvme/0/temperature/0",
                "Temperature",
                "Temperature",
                f32::NAN,
            ),
        ]
    }

    fn hardware() -> HashMap<String, String> {
        HashMap::from([("/amdcpu/0".to_string(), "AMD Ryzen 9 7900X3D".to_string())])
    }

    fn reading(m: &Mapping, rows: &[Row], key: &str) -> Reading {
        m.readings(Ok(rows))
            .into_iter()
            .find(|(i, _)| i.key.as_str() == key)
            .map(|(_, r)| r)
            .unwrap()
    }

    #[test]
    fn sensors_map_to_keys_units_and_categories() {
        let rows = rows();
        let m = Mapping::discover(Some(&rows), &hardware());
        let catalog = m.catalog();
        assert_eq!(catalog.len(), 4 + PICKED.len() + rows.len());
        let fan = catalog
            .iter()
            .find(|i| i.key.as_str() == "lhm.lpc.nct6798d.0.fan.1")
            .unwrap();
        assert_eq!(
            (fan.category, fan.quantity),
            (Category::Board, Quantity::Rpm)
        );
        let load = catalog
            .iter()
            .find(|i| i.key.as_str() == "lhm.amdcpu.0.load.0")
            .unwrap();
        assert_eq!(load.label, "AMD Ryzen 9 7900X3D CPU Total");
        assert_eq!(
            reading(&m, &rows, "lhm.ram.data.0"),
            Reading::Value(12.5 * 1024.0 * 1024.0 * 1024.0)
        );
        assert_eq!(
            reading(&m, &rows, "lhm.gpu_nvidia.0.smalldata.1"),
            Reading::Value(2701.0 * 1024.0 * 1024.0)
        );
        assert_eq!(
            reading(&m, &rows, "lhm.nic.abc.throughput.7"),
            Reading::Value(1500.0)
        );
        assert!(
            matches!(reading(&m, &rows, "lhm.nvme.0.temperature.0"), Reading::Unavailable(r) if r.contains("no value"))
        );
        let nic = catalog
            .iter()
            .find(|i| i.key.as_str() == "lhm.nic.abc.throughput.7")
            .unwrap();
        assert_eq!(nic.category, Category::Network);
    }

    #[test]
    fn cpu_keys_come_from_the_cpu_not_the_board() {
        let rows = rows();
        let m = Mapping::discover(Some(&rows), &hardware());
        assert_eq!(reading(&m, &rows, "cpu.temperature"), Reading::Value(46.5));
        assert_eq!(
            reading(&m, &rows, "cpu.temperature.ccd1"),
            Reading::Value(41.75)
        );
        assert_eq!(
            reading(&m, &rows, "cpu.temperature.ccd2"),
            Reading::Value(38.25)
        );
        assert_eq!(reading(&m, &rows, "cpu.power"), Reading::Value(61.5));
        let info = m
            .catalog()
            .into_iter()
            .find(|i| i.key.as_str() == "cpu.temperature")
            .unwrap();
        assert_eq!(info.source, "LibreHardwareMonitor Core (Tctl/Tdie)");
        // Intel: no Tctl; the CPU's package sensor wins over per-core summaries.
        let intel = vec![
            row("/intelcpu/0/temperature/9", "Core Max", "Temperature", 70.0),
            row(
                "/intelcpu/0/temperature/8",
                "CPU Package",
                "Temperature",
                66.0,
            ),
            row("/intelcpu/0/power/0", "CPU Package", "Power", 45.0),
        ];
        let m = Mapping::discover(Some(&intel), &HashMap::new());
        assert_eq!(reading(&m, &intel, "cpu.temperature"), Reading::Value(66.0));
        assert_eq!(reading(&m, &intel, "cpu.power"), Reading::Value(45.0));
        // A sensor that disappeared later.
        let later = &intel[..1];
        assert!(
            matches!(reading(&m, later, "cpu.power"), Reading::Unavailable(r) if r.contains("no CPU package power"))
        );
        assert!(
            matches!(reading(&m, later, "lhm.intelcpu.0.temperature.8"), Reading::Unavailable(r) if r.contains("no longer"))
        );
        assert_eq!(reading(&m, later, "cpu.temperature"), Reading::Value(70.0));
    }

    #[test]
    fn without_lhm_the_well_known_keys_explain_how_to_get_them() {
        let m = Mapping::discover(None, &HashMap::new());
        let listed: Vec<String> = m.catalog().iter().map(|i| i.key.to_string()).collect();
        assert_eq!(
            listed,
            [
                keys::CPU_TEMPERATURE,
                keys::CPU_POWER,
                keys::CPU_FAN,
                keys::CPU_VOLTAGE,
                keys::FAN_PUMP,
                keys::FAN_CASE_1,
                keys::FAN_CASE_2
            ]
        );
        let readings = m.readings(Err(HINT));
        assert!(
            readings
                .iter()
                .all(|(_, r)| *r == Reading::Unavailable(HINT.into()))
        );
        let empty: Vec<Row> = Vec::new();
        assert!(
            matches!(&m.readings(Ok(&empty))[0].1, Reading::Unavailable(r) if r.contains("no CPU temperature"))
        );
    }

    #[test]
    fn fans_and_the_cpu_rail_are_picked_by_name() {
        let rows = vec![
            row("/lpc/nct6798d/0/fan/0", "CPU Optional Fan", "Fan", 640.0),
            row("/lpc/nct6798d/0/fan/1", "CPU Fan", "Fan", 1180.0),
            row("/lpc/nct6798d/0/fan/2", "Chassis Fan #1", "Fan", 720.0),
            row("/lpc/nct6798d/0/fan/3", "AIO Pump", "Fan", 2350.0),
            row("/lpc/nct6798d/0/fan/4", "Chassis Fan #2", "Fan", f32::NAN),
            row("/lpc/nct6798d/0/voltage/0", "Vcore", "Voltage", 1.28),
            row("/amdcpu/0/voltage/2", "Core (SVI2 TFN)", "Voltage", 1.1),
            row("/amdcpu/0/voltage/3", "SoC (SVI2 TFN)", "Voltage", 1.2),
            row("/gpu-amd/0/fan/0", "CPU", "Fan", 1500.0),
        ];
        let m = Mapping::discover(Some(&rows), &HashMap::new());
        assert_eq!(reading(&m, &rows, keys::CPU_FAN), Reading::Value(1180.0));
        assert_eq!(reading(&m, &rows, keys::FAN_PUMP), Reading::Value(2350.0));
        assert_eq!(reading(&m, &rows, keys::FAN_CASE_1), Reading::Value(720.0));
        assert!(
            matches!(reading(&m, &rows, keys::FAN_CASE_2), Reading::Unavailable(r) if r.contains("no value")),
            "a NaN fan is unavailable, not zero"
        );
        let volts = reading(&m, &rows, keys::CPU_VOLTAGE).value().unwrap();
        assert!((volts - 1.1).abs() < 1e-6, "the CPU's own rail: {volts}");
        let info = |key: &str| {
            m.catalog()
                .into_iter()
                .find(|i| i.key.as_str() == key)
                .unwrap()
        };
        assert_eq!(info(keys::CPU_FAN).source, "LibreHardwareMonitor CPU Fan");
        assert_eq!(info(keys::CPU_FAN).category, Category::Cpu);
        assert_eq!(info(keys::FAN_PUMP).category, Category::Board);
        assert_eq!(
            info(keys::CPU_VOLTAGE).source,
            "LibreHardwareMonitor Core (SVI2 TFN)"
        );

        // A board LHM does not know: fans are "Fan #n", nothing is named.
        let bare = vec![row("/lpc/it8688e/0/fan/0", "Fan #1", "Fan", 900.0)];
        let m = Mapping::discover(Some(&bare), &HashMap::new());
        assert_eq!(info_source(&m, keys::FAN_PUMP), "LibreHardwareMonitor");
        assert_eq!(
            reading(&m, &bare, keys::FAN_PUMP),
            Reading::Unavailable("no LibreHardwareMonitor fan is named pump".into())
        );
        assert!(
            matches!(reading(&m, &bare, keys::CPU_VOLTAGE), Reading::Unavailable(r) if r.contains("Vcore"))
        );
    }

    fn info_source(m: &Mapping, key: &str) -> String {
        m.catalog()
            .into_iter()
            .find(|i| i.key.as_str() == key)
            .map(|i| i.source)
            .unwrap()
    }

    #[test]
    fn units_and_keys() {
        assert_eq!(unit("Frequency"), (Quantity::Megahertz, 1e-6));
        assert_eq!(unit("Control").0, Quantity::Percent);
        assert_eq!(unit("Current").0, Quantity::Amperes);
        assert_eq!(unit("TimeSpan").0, Quantity::Seconds);
        assert_eq!(unit("Voltage").0, Quantity::Volts);
        assert_eq!(unit("Clock").0, Quantity::Megahertz);
        assert_eq!(unit("Noise").0, Quantity::Number);
        assert_eq!(key_of("/gpu-amd/0/load/0"), "lhm.gpu_amd.0.load.0");
        assert_eq!(category("/hdd/1/load/0"), Category::Disk);
        assert_eq!(category("/ram/load/0"), Category::Memory);
        assert_eq!(category("/motherboard"), Category::Board);
        assert_eq!(
            ccd(&row("/lpc/x/temperature/1", "CCD1", "Temperature", 1.0)),
            None
        );
    }
}
