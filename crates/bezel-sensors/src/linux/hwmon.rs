//! Every hwmon chip under `/sys/class/hwmon`: temperatures, fans, voltages,
//! currents, power, frequencies and PWM duty, as `hwmon.<chip>.<label>`; and
//! the CPU temperature picked by priority (D-2026-09-30-sensors-2).
//!
//! Units follow the kernel's hwmon sysfs ABI: temperatures in m°C, voltages
//! and currents in mV/mA, power in µW, frequencies in Hz, PWM as 0-255.
//! Chips that share a name get distinct ids: the kernel device name when it
//! extends the chip name (`nvme0`, `nvme1`), else `<name>-<n>` in device path
//! order (`spd5118-0`, `spd5118-1`), so keys survive hwmon renumbering.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use super::Roots;
use super::fs::{read_int, read_text};
use crate::provider::{Provider, describe, put, slug};

/// Chips and labels tried, in order, for `cpu.temperature`. An empty label
/// list takes the chip's first temperature. k10temp/zenpower prefer `Tdie`
/// because `Tctl` carries a fan-control offset (+10/+20/+27 °C) on Zen 1
/// and Zen+; newer Zen reports only `Tctl`, which equals the die temperature.
const CPU_TEMPERATURE_PRIORITY: &[(&str, &[&str])] = &[
    ("k10temp", &["Tdie", "Tctl"]),
    ("zenpower", &["Tdie", "Tctl"]),
    ("coretemp", &["Package id 0", "Physical id 0"]),
    ("cpu_thermal", &[]),
    ("acpitz", &[]),
];

/// What a hwmon attribute measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Temp,
    Fan,
    Pwm,
    In,
    Curr,
    Power,
    Freq,
}

impl Kind {
    const ALL: [Kind; 7] = [
        Kind::Temp,
        Kind::Fan,
        Kind::Pwm,
        Kind::In,
        Kind::Curr,
        Kind::Power,
        Kind::Freq,
    ];

    fn prefix(self) -> &'static str {
        match self {
            Kind::Temp => "temp",
            Kind::Fan => "fan",
            Kind::Pwm => "pwm",
            Kind::In => "in",
            Kind::Curr => "curr",
            Kind::Power => "power",
            Kind::Freq => "freq",
        }
    }

    fn quantity(self) -> Quantity {
        match self {
            Kind::Temp => Quantity::Celsius,
            Kind::Fan => Quantity::Rpm,
            Kind::Pwm => Quantity::Percent,
            Kind::In => Quantity::Volts,
            Kind::Curr => Quantity::Amperes,
            Kind::Power => Quantity::Watts,
            Kind::Freq => Quantity::Megahertz,
        }
    }

    /// Raw sysfs integer to the unit of [`Kind::quantity`].
    fn convert(self, raw: i64) -> f64 {
        let raw = raw as f64;
        match self {
            Kind::Temp | Kind::In | Kind::Curr => raw / 1000.0,
            Kind::Fan => raw,
            Kind::Pwm => raw * 100.0 / 255.0,
            Kind::Power | Kind::Freq => raw / 1_000_000.0,
        }
    }
}

/// `temp1_input` → `(Temp, 1, "input")`; `pwm1` → `(Pwm, 1, "")`.
fn parse_attribute(file: &str) -> Option<(Kind, u32, &str)> {
    Kind::ALL.iter().find_map(|kind| {
        let rest = file.strip_prefix(kind.prefix())?;
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let index = rest[..digits].parse().ok()?;
        let attr = &rest[digits..];
        let attr = attr.strip_prefix('_').unwrap_or(attr);
        Some((*kind, index, attr))
    })
}

/// One readable value of a chip.
#[derive(Debug, Clone)]
struct Sensor {
    kind: Kind,
    label: Option<String>,
    /// `temp1` for `temp1_input`.
    name: String,
    input: PathBuf,
    key: String,
}

/// One hwmon chip.
#[derive(Debug, Clone)]
struct Chip {
    /// Driver-given name (`k10temp`, `nvme`).
    name: String,
    /// Unique key segment (`k10temp`, `nvme0`).
    id: String,
    device: PathBuf,
    sensors: Vec<Sensor>,
}

impl Chip {
    fn category(&self) -> Category {
        match self.name.as_str() {
            "k10temp" | "zenpower" | "coretemp" | "cpu_thermal" | "fam15h_power" => Category::Cpu,
            "amdgpu" | "radeon" | "nouveau" | "i915" | "xe" => Category::Gpu,
            "nvme" | "drivetemp" => Category::Disk,
            "spd5118" | "jc42" => Category::Memory,
            _ => Category::Board,
        }
    }

    fn temperature(&self, label: &str) -> Option<usize> {
        self.sensors
            .iter()
            .position(|s| s.kind == Kind::Temp && s.label.as_deref() == Some(label))
    }
}

/// The readable sensors in a chip directory, in kind then index order.
fn scan_sensors(dir: &Path) -> Vec<Sensor> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let files: HashSet<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    let mut found: BTreeMap<(Kind, u32), PathBuf> = BTreeMap::new();
    for file in &files {
        let Some((kind, index, attr)) = parse_attribute(file) else {
            continue;
        };
        let wanted = match (kind, attr) {
            (Kind::Pwm, "") => true,
            (Kind::Pwm, _) => false,
            (_, "input") => true,
            // Older amdgpu and some PMBus chips only have the average.
            (Kind::Power, "average") => !files.contains(&format!("power{index}_input")),
            _ => false,
        };
        if wanted {
            found.insert((kind, index), dir.join(file));
        }
    }
    found
        .into_iter()
        .map(|((kind, index), input)| {
            let name = format!("{}{index}", kind.prefix());
            let label = read_text(&dir.join(format!("{name}_label")))
                .ok()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty());
            Sensor {
                kind,
                label,
                name,
                input,
                key: String::new(),
            }
        })
        .collect()
}

/// Unique key segments for the chips' sensors: the slugged label, or the
/// attribute name; labels that repeat inside a chip get the attribute name
/// appended (`cpu_temp1`, `cpu_fan1`).
fn assign_sensor_keys(chip: &mut Chip) {
    let slugs: Vec<String> = chip
        .sensors
        .iter()
        .map(|s| s.label.as_deref().map(slug).unwrap_or_default())
        .collect();
    for (i, sensor) in chip.sensors.iter_mut().enumerate() {
        let base = &slugs[i];
        let repeated = slugs.iter().filter(|s| *s == base).count() > 1;
        let segment = if base.is_empty() {
            sensor.name.clone()
        } else if repeated {
            format!("{base}_{}", sensor.name)
        } else {
            base.clone()
        };
        sensor.key = format!("hwmon.{}.{segment}", chip.id);
    }
}

/// Distinct chip ids; see the module docs.
fn assign_chip_ids(chips: &mut [Chip]) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, chip) in chips.iter().enumerate() {
        groups.entry(slug(&chip.name)).or_default().push(i);
    }
    let mut taken = HashSet::new();
    for (name, mut members) in groups {
        if members.len() == 1 {
            chips[members[0]].id = name.clone();
        } else {
            members.sort_by(|a, b| chips[*a].device.cmp(&chips[*b].device));
            for (n, i) in members.iter().enumerate() {
                let device = chips[*i]
                    .device
                    .file_name()
                    .map(|f| slug(&f.to_string_lossy()));
                chips[*i].id = match device {
                    Some(d) if d.starts_with(&name) && d != name => d,
                    _ => format!("{name}-{n}"),
                };
            }
        }
    }
    for chip in chips.iter_mut() {
        let mut id = chip.id.clone();
        let mut n = 1;
        while !taken.insert(id.clone()) {
            id = format!("{}-{n}", chip.id);
            n += 1;
        }
        chip.id = id;
    }
}

/// Every chip with at least one sensor, sorted by id.
fn discover(sys: &Path) -> Vec<Chip> {
    let Ok(entries) = std::fs::read_dir(sys.join("class/hwmon")) else {
        return Vec::new();
    };
    let mut chips: Vec<Chip> = entries
        .filter_map(|e| {
            let dir = e.ok()?.path();
            let name = read_text(&dir.join("name")).ok()?;
            let device = std::fs::canonicalize(dir.join("device"))
                .or_else(|_| std::fs::canonicalize(&dir))
                .unwrap_or_else(|_| dir.clone());
            let sensors = scan_sensors(&dir);
            (!sensors.is_empty()).then(|| Chip {
                name: name.trim().to_string(),
                id: String::new(),
                device,
                sensors,
            })
        })
        .collect();
    assign_chip_ids(&mut chips);
    chips.sort_by(|a, b| a.id.cmp(&b.id));
    for chip in &mut chips {
        assign_sensor_keys(chip);
    }
    chips
}

/// `(chip, sensor)` indexes of the CPU temperature, by priority.
fn cpu_temperature(chips: &[Chip]) -> Option<(usize, usize)> {
    CPU_TEMPERATURE_PRIORITY.iter().find_map(|(name, labels)| {
        chips.iter().enumerate().find_map(|(c, chip)| {
            if chip.name != *name {
                return None;
            }
            let sensor = if labels.is_empty() {
                chip.sensors.iter().position(|s| s.kind == Kind::Temp)
            } else {
                labels.iter().find_map(|label| chip.temperature(label))
            };
            sensor.map(|s| (c, s))
        })
    })
}

/// Every sensor of `chip`, in order.
/// Why a runtime-suspended device is not read.
const SUSPENDED: &str = "the device is powered down (runtime suspend)";

/// True when the chip's device is runtime-suspended. Reading its sensors
/// would wake it (a hybrid laptop's discrete GPU, every second); the status
/// file itself is safe to read.
fn suspended(chip: &Chip) -> bool {
    std::fs::read_to_string(chip.device.join("power/runtime_status"))
        .is_ok_and(|s| s.trim() == "suspended")
}

fn read_chip(chip: &Chip) -> Vec<Reading> {
    if suspended(chip) {
        return chip
            .sensors
            .iter()
            .map(|_| Reading::Unavailable(SUSPENDED.into()))
            .collect();
    }
    chip.sensors
        .iter()
        .map(|s| match read_int(&s.input) {
            Ok(raw) => Reading::Value(s.kind.convert(raw)),
            Err(reason) => Reading::Unavailable(reason),
        })
        .collect()
}

/// A derived key that repeats one chip sensor (`cpu.temperature` = k10temp Tctl).
#[derive(Debug, Clone)]
struct Alias {
    key: String,
    chip: usize,
    sensor: usize,
}

/// The hwmon provider.
pub(crate) struct Hwmon {
    chips: Vec<Chip>,
    aliases: Vec<Alias>,
    catalog: Vec<SensorInfo>,
}

impl Hwmon {
    /// Discovers every chip under `<sys>/class/hwmon`.
    pub(crate) fn new(roots: &Roots) -> Self {
        let chips = discover(&roots.sys);
        let mut aliases = Vec::new();
        let mut catalog = Vec::new();
        match cpu_temperature(&chips) {
            Some((c, s)) => {
                let chip = &chips[c];
                let sensor = &chip.sensors[s];
                let label = sensor.label.as_deref().unwrap_or(&sensor.name);
                catalog.extend(describe(
                    keys::CPU_TEMPERATURE,
                    Category::Cpu,
                    "CPU temperature",
                    Quantity::Celsius,
                    format!("hwmon {} {label}", chip.name),
                ));
                aliases.push(Alias {
                    key: keys::CPU_TEMPERATURE.into(),
                    chip: c,
                    sensor: s,
                });
                if chip.name == "k10temp" || chip.name == "zenpower" {
                    for (i, ccd) in chip.sensors.iter().enumerate() {
                        let Some(n) = ccd.label.as_deref().and_then(|l| l.strip_prefix("Tccd"))
                        else {
                            continue;
                        };
                        let key = format!("{}.ccd{n}", keys::CPU_TEMPERATURE);
                        catalog.extend(describe(
                            &key,
                            Category::Cpu,
                            format!("CPU CCD{n} temperature"),
                            Quantity::Celsius,
                            format!("hwmon {} Tccd{n}", chip.name),
                        ));
                        aliases.push(Alias {
                            key,
                            chip: c,
                            sensor: i,
                        });
                    }
                }
            }
            None => catalog.extend(describe(
                keys::CPU_TEMPERATURE,
                Category::Cpu,
                "CPU temperature",
                Quantity::Celsius,
                "hwmon",
            )),
        }
        for chip in &chips {
            for sensor in &chip.sensors {
                let label = sensor.label.as_deref().unwrap_or(&sensor.name);
                let file = sensor
                    .input
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned());
                catalog.extend(describe(
                    &sensor.key,
                    chip.category(),
                    format!("{} {label}", chip.id),
                    sensor.kind.quantity(),
                    format!("hwmon {} {}", chip.name, file.unwrap_or_default()),
                ));
            }
        }
        Self {
            chips,
            aliases,
            catalog,
        }
    }
}

impl Provider for Hwmon {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    /// Reads the chips concurrently: some answer in microseconds (k10temp),
    /// others take milliseconds per value (NVMe SMART log, SMBus, Wi-Fi
    /// firmware), and a sample should cost the slowest chip, not the sum.
    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        let readings: Vec<Vec<Reading>> = std::thread::scope(|scope| {
            let running: Vec<_> = self
                .chips
                .iter()
                .map(|chip| scope.spawn(move || read_chip(chip)))
                .collect();
            running
                .into_iter()
                .zip(&self.chips)
                .map(|(thread, chip)| {
                    thread.join().unwrap_or_else(|_| {
                        let reason = || Reading::Unavailable("reading the chip failed".into());
                        chip.sensors.iter().map(|_| reason()).collect()
                    })
                })
                .collect()
        });
        for (chip, values) in self.chips.iter().zip(&readings) {
            for (sensor, value) in chip.sensors.iter().zip(values) {
                put(out, &sensor.key, value.clone());
            }
        }
        if self.aliases.is_empty() {
            let reason = "no CPU temperature sensor found (k10temp, zenpower, coretemp, cpu_thermal or acpitz)";
            put(
                out,
                keys::CPU_TEMPERATURE,
                Reading::Unavailable(reason.into()),
            );
        }
        for alias in &self.aliases {
            put(out, &alias.key, readings[alias.chip][alias.sensor].clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;

    fn get(s: &Snapshot, key: &str) -> Reading {
        s.get(&SensorKey::new(key).unwrap())
    }

    fn chip(t: &FakeTree, n: u32, name: &str, device: &str, files: &[(&str, &str)]) {
        let dir = format!("sys/class/hwmon/hwmon{n}");
        t.file(&format!("{dir}/name"), &format!("{name}\n"));
        t.dir(&format!("sys/devices/{device}"));
        t.link(&format!("{dir}/device"), &format!("sys/devices/{device}"));
        for (file, value) in files {
            t.file(&format!("{dir}/{file}"), &format!("{value}\n"));
        }
    }

    fn sample(t: &FakeTree) -> (Hwmon, Snapshot) {
        let mut hwmon = Hwmon::new(&Roots::new(t.path("sys"), t.path("proc")));
        let mut s = Snapshot::default();
        hwmon.sample(Instant::now(), &mut s);
        (hwmon, s)
    }

    fn cpu_source(hwmon: &Hwmon) -> String {
        hwmon
            .catalog()
            .into_iter()
            .find(|i| i.key.as_str() == keys::CPU_TEMPERATURE)
            .map(|i| i.source)
            .unwrap()
    }

    #[test]
    fn a_suspended_device_is_not_read() {
        let t = FakeTree::new("hwmon-suspended");
        chip(
            &t,
            0,
            "amdgpu",
            "pci0000:00/0000:03:00.0",
            &[("temp1_input", "45000")],
        );
        let (_, awake) = sample(&t);
        let values: Vec<Reading> = awake
            .iter()
            .filter(|(k, _)| k.as_str().starts_with("hwmon.amdgpu"))
            .map(|(_, r)| r.clone())
            .collect();
        assert_eq!(values, vec![Reading::Value(45.0)]);

        t.file(
            "sys/devices/pci0000:00/0000:03:00.0/power/runtime_status",
            "suspended\n",
        );
        let (_, asleep) = sample(&t);
        let values: Vec<Reading> = asleep
            .iter()
            .filter(|(k, _)| k.as_str().starts_with("hwmon.amdgpu"))
            .map(|(_, r)| r.clone())
            .collect();
        assert_eq!(values, vec![Reading::Unavailable(SUSPENDED.into())]);
    }

    #[test]
    fn cpu_temperature_follows_the_priority() {
        let t = FakeTree::new("hwmon-priority");
        chip(&t, 0, "acpitz", "LNXTHERM:00", &[("temp1_input", "27800")]);
        let (hwmon, s) = sample(&t);
        assert_eq!(get(&s, "cpu.temperature"), Reading::Value(27.8));
        assert_eq!(cpu_source(&hwmon), "hwmon acpitz temp1");

        chip(
            &t,
            1,
            "cpu_thermal",
            "cpu-thermal",
            &[("temp1_input", "51000")],
        );
        let (_, s) = sample(&t);
        assert_eq!(get(&s, "cpu.temperature"), Reading::Value(51.0));

        chip(
            &t,
            2,
            "coretemp",
            "coretemp.0",
            &[
                ("temp1_input", "61000"),
                ("temp1_label", "Core 0"),
                ("temp2_input", "64000"),
                ("temp2_label", "Package id 0"),
            ],
        );
        let (hwmon, s) = sample(&t);
        assert_eq!(get(&s, "cpu.temperature"), Reading::Value(64.0));
        assert_eq!(cpu_source(&hwmon), "hwmon coretemp Package id 0");

        chip(
            &t,
            3,
            "zenpower",
            "zen",
            &[("temp1_input", "70000"), ("temp1_label", "Tdie")],
        );
        let (_, s) = sample(&t);
        assert_eq!(get(&s, "cpu.temperature"), Reading::Value(70.0));

        // A board chip that also says "CPU Package" never beats the CPU's own sensor.
        chip(
            &t,
            4,
            "asusec",
            "asus-ec-sensors",
            &[("temp2_input", "46000"), ("temp2_label", "CPU Package")],
        );
        chip(
            &t,
            5,
            "k10temp",
            "0000:00:18.3",
            &[
                ("temp1_input", "46625"),
                ("temp1_label", "Tctl"),
                ("temp3_input", "41750"),
                ("temp3_label", "Tccd1"),
                ("temp4_input", "38375"),
                ("temp4_label", "Tccd2"),
            ],
        );
        let (hwmon, s) = sample(&t);
        assert_eq!(get(&s, "cpu.temperature"), Reading::Value(46.625));
        assert_eq!(cpu_source(&hwmon), "hwmon k10temp Tctl");
        assert_eq!(get(&s, "cpu.temperature.ccd1"), Reading::Value(41.75));
        assert_eq!(get(&s, "cpu.temperature.ccd2"), Reading::Value(38.375));

        // Zen 1/Zen+: Tctl carries an offset, Tdie is the real die temperature.
        t.file("sys/class/hwmon/hwmon5/temp2_input", "36625\n")
            .file("sys/class/hwmon/hwmon5/temp2_label", "Tdie\n");
        let (hwmon, s) = sample(&t);
        assert_eq!(get(&s, "cpu.temperature"), Reading::Value(36.625));
        assert_eq!(cpu_source(&hwmon), "hwmon k10temp Tdie");
    }

    #[test]
    fn without_a_cpu_chip_the_temperature_is_unavailable() {
        let t = FakeTree::new("hwmon-none");
        chip(&t, 0, "nct6798", "nct6775.656", &[("fan1_input", "1200")]);
        let (hwmon, s) = sample(&t);
        assert!(
            matches!(get(&s, "cpu.temperature"), Reading::Unavailable(r) if r.starts_with("no CPU temperature"))
        );
        assert_eq!(cpu_source(&hwmon), "hwmon");
        assert_eq!(get(&s, "hwmon.nct6798.fan1"), Reading::Value(1200.0));
    }

    #[test]
    fn every_kind_is_converted_to_its_unit() {
        let t = FakeTree::new("hwmon-units");
        chip(
            &t,
            0,
            "amdgpu",
            "0000:03:00.0",
            &[
                ("temp1_input", "-5500"),
                ("temp1_label", "edge"),
                ("fan1_input", "1450"),
                ("pwm1", "51"),
                ("pwm1_enable", "2"),
                ("in0_input", "905"),
                ("in0_label", "vddgfx"),
                ("curr1_input", "12500"),
                ("power1_average", "39660000"),
                ("power1_label", "PPT"),
                ("freq1_input", "2100000000"),
                ("freq1_label", "sclk"),
                ("temp1_crit", "100000"),
                ("uevent", ""),
            ],
        );
        let (hwmon, s) = sample(&t);
        assert_eq!(get(&s, "hwmon.amdgpu.edge"), Reading::Value(-5.5));
        assert_eq!(get(&s, "hwmon.amdgpu.fan1"), Reading::Value(1450.0));
        assert_eq!(get(&s, "hwmon.amdgpu.pwm1"), Reading::Value(20.0));
        assert_eq!(get(&s, "hwmon.amdgpu.vddgfx"), Reading::Value(0.905));
        assert_eq!(get(&s, "hwmon.amdgpu.curr1"), Reading::Value(12.5));
        assert_eq!(get(&s, "hwmon.amdgpu.ppt"), Reading::Value(39.66));
        assert_eq!(get(&s, "hwmon.amdgpu.sclk"), Reading::Value(2100.0));
        let catalog = hwmon.catalog();
        let edge = catalog
            .iter()
            .find(|i| i.key.as_str() == "hwmon.amdgpu.edge")
            .unwrap();
        assert_eq!(edge.category, Category::Gpu);
        assert_eq!(edge.label, "amdgpu edge");
        assert_eq!(edge.source, "hwmon amdgpu temp1_input");
        let quantities: Vec<Quantity> = catalog.iter().skip(1).map(|i| i.quantity).collect();
        assert_eq!(
            quantities,
            [
                Quantity::Celsius,
                Quantity::Rpm,
                Quantity::Percent,
                Quantity::Volts,
                Quantity::Amperes,
                Quantity::Watts,
                Quantity::Megahertz
            ]
        );
        // power1_input wins over power1_average when both exist.
        t.file("sys/class/hwmon/hwmon0/power1_input", "41000000\n");
        let (_, s) = sample(&t);
        assert_eq!(get(&s, "hwmon.amdgpu.ppt"), Reading::Value(41.0));
    }

    #[test]
    fn twin_chips_get_stable_distinct_ids() {
        let t = FakeTree::new("hwmon-twins");
        let composite = [("temp1_input", "39850"), ("temp1_label", "Composite")];
        chip(&t, 0, "nvme", "pci/0000:65:00.0/nvme/nvme1", &composite);
        chip(&t, 1, "nvme", "pci/0000:02:00.0/nvme/nvme0", &composite);
        chip(
            &t,
            2,
            "spd5118",
            "i2c-6/6-0053",
            &[("temp1_input", "40250")],
        );
        chip(
            &t,
            3,
            "spd5118",
            "i2c-6/6-0051",
            &[("temp1_input", "40500")],
        );
        chip(&t, 4, "asus", "eeepc-wmi", &[]);
        let (hwmon, s) = sample(&t);
        let keys: Vec<String> = hwmon.catalog().iter().map(|i| i.key.to_string()).collect();
        assert_eq!(
            keys,
            [
                "cpu.temperature",
                "hwmon.nvme0.composite",
                "hwmon.nvme1.composite",
                "hwmon.spd5118-0.temp1",
                "hwmon.spd5118-1.temp1"
            ]
        );
        assert_eq!(get(&s, "hwmon.spd5118-0.temp1"), Reading::Value(40.5));
        assert_eq!(get(&s, "hwmon.spd5118-1.temp1"), Reading::Value(40.25));
        let dimm = &hwmon.catalog()[3];
        assert_eq!(dimm.category, Category::Memory);
    }

    #[test]
    fn repeated_labels_and_read_errors() {
        let t = FakeTree::new("hwmon-repeat");
        chip(
            &t,
            0,
            "asusec",
            "asus-ec-sensors",
            &[
                ("temp1_input", "37000"),
                ("temp1_label", "CPU"),
                ("fan1_input", "900"),
                ("fan1_label", "CPU"),
                ("in0_input", "1200"),
                ("in0_label", "+"),
                ("temp2_input", "garbage"),
            ],
        );
        let (_, s) = sample(&t);
        assert_eq!(get(&s, "hwmon.asusec.cpu_temp1"), Reading::Value(37.0));
        assert_eq!(get(&s, "hwmon.asusec.cpu_fan1"), Reading::Value(900.0));
        assert_eq!(get(&s, "hwmon.asusec.in0"), Reading::Value(1.2));
        assert!(
            matches!(get(&s, "hwmon.asusec.temp2"), Reading::Unavailable(r) if r.starts_with("unexpected"))
        );
    }

    #[test]
    fn attribute_names_parse() {
        assert_eq!(
            parse_attribute("temp12_input"),
            Some((Kind::Temp, 12, "input"))
        );
        assert_eq!(parse_attribute("pwm1"), Some((Kind::Pwm, 1, "")));
        assert_eq!(
            parse_attribute("power1_average"),
            Some((Kind::Power, 1, "average"))
        );
        assert_eq!(parse_attribute("name"), None);
        assert_eq!(parse_attribute("temp_input"), None);
        assert!(discover(Path::new("/nonexistent")).is_empty());
        assert!(scan_sensors(Path::new("/nonexistent")).is_empty());
    }
}
