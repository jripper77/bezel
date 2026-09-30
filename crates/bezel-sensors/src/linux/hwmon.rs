//! Every hwmon chip under `/sys/class/hwmon`: temperatures, fans, voltages,
//! currents, power, frequencies and PWM duty, as `hwmon.<chip>.<label>`; the
//! CPU temperature picked by priority (D-2026-09-30-sensors-2); and the CPU
//! fan, pump, case fans and CPU core voltage picked by label
//! (D-2026-09-30-release-polish-5), unavailable with the reason when no
//! label names them.
//!
//! Units follow the kernel's hwmon sysfs ABI: temperatures in m°C, voltages
//! and currents in mV/mA, power in µW, frequencies in Hz, PWM as 0-255. The
//! amdgpu provider converts its card's hwmon files with the same [`Kind`]
//! and picks power with the same [`power_file`], so `hwmon.amdgpu.ppt` and
//! `gpu.<n>.power` never disagree.
//! Chips that share a name get distinct ids: the kernel device name when it
//! extends the chip name (`nvme0`, `nvme1`), else `<name>-<n>` in device path
//! order (`spd5118-0`, `spd5118-1`), so keys survive hwmon renumbering.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use super::Roots;
use super::fs::{read_int, read_text};
use crate::provider::{
    FanRole, Provider, cpu_core_voltage, describe, fan_role, optional_fan, put, slug,
};

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

/// Why `cpu.temperature` is unavailable.
const NO_CPU_TEMPERATURE: &str =
    "no CPU temperature sensor found (k10temp, zenpower, coretemp, cpu_thermal or acpitz)";

/// Added to the reason fan and voltage keys are unavailable when hwmon has
/// no such sensor at all.
const BOARD_DRIVER_HINT: &str = "board chips such as nct6775 or it87 need their driver loaded";

/// What a hwmon attribute measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
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

    /// What the attribute measures.
    pub(crate) fn quantity(self) -> Quantity {
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
    pub(crate) fn convert(self, raw: i64) -> f64 {
        let raw = raw as f64;
        match self {
            Kind::Temp | Kind::In | Kind::Curr => raw / 1000.0,
            Kind::Fan => raw,
            Kind::Pwm => raw * 100.0 / 255.0,
            Kind::Power | Kind::Freq => raw / 1_000_000.0,
        }
    }
}

/// The file `powerN` is read from: `powerN_average` when the chip has it,
/// else `powerN_input` (D-2026-09-30-release-polish-5). amdgpu exposes both
/// on recent kernels and its own tools report the average; older amdgpu
/// and many PMBus chips have only one of them.
pub(crate) fn power_file(index: u32, exists: impl Fn(&str) -> bool) -> Option<String> {
    ["average", "input"]
        .into_iter()
        .map(|attribute| format!("power{index}_{attribute}"))
        .find(|file| exists(file))
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

    /// A graphics card's chip: its fans and rails belong to `gpu.<n>.*`.
    fn is_gpu(&self) -> bool {
        self.category() == Category::Gpu
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
            (Kind::Power, _) => {
                power_file(index, |f| files.contains(f)).as_deref() == Some(file.as_str())
            }
            (_, "input") => true,
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

/// Why a runtime-suspended device is not read.
const SUSPENDED: &str = "the device is powered down (runtime suspend)";

/// True when the chip's device is runtime-suspended. Reading its sensors
/// would wake it (a hybrid laptop's discrete GPU, every second); the status
/// file itself is safe to read.
fn suspended(chip: &Chip) -> bool {
    std::fs::read_to_string(chip.device.join("power/runtime_status"))
        .is_ok_and(|s| s.trim() == "suspended")
}

/// Every sensor of `chip`, in order.
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

/// A well-known key that repeats one chip sensor (`cpu.temperature` =
/// k10temp Tctl, `cpu.fan` = the fan labelled CPU), or why there is none.
#[derive(Debug, Clone)]
struct Alias {
    key: String,
    /// `(chip, sensor)`.
    target: Result<(usize, usize), String>,
}

/// A well-known key's catalog description: key, category, label, quantity.
type Wanted<'a> = (&'a str, Category, &'a str, Quantity);

/// The well-known keys picked from the chips, with their catalog entries.
#[derive(Default)]
struct Picks {
    aliases: Vec<Alias>,
    catalog: Vec<SensorInfo>,
}

impl Picks {
    /// Adds `wanted`, read from `found` or unavailable for the reason given.
    fn add(&mut self, chips: &[Chip], wanted: Wanted<'_>, found: Result<(usize, usize), String>) {
        let (key, category, label, quantity) = wanted;
        let source = match found {
            Ok((c, s)) => {
                let (chip, sensor) = (&chips[c], &chips[c].sensors[s]);
                let named = sensor.label.as_deref().unwrap_or(&sensor.name);
                format!("hwmon {} {named}", chip.name)
            }
            Err(_) => "hwmon".to_string(),
        };
        self.catalog
            .extend(describe(key, category, label, quantity, source));
        self.aliases.push(Alias {
            key: key.to_string(),
            target: found,
        });
    }
}

/// `cpu.temperature` by priority, and each CCD's on AMD.
fn pick_cpu_temperatures(chips: &[Chip], picks: &mut Picks) {
    let found = cpu_temperature(chips);
    let wanted = (
        keys::CPU_TEMPERATURE,
        Category::Cpu,
        "CPU temperature",
        Quantity::Celsius,
    );
    picks.add(
        chips,
        wanted,
        found.ok_or_else(|| NO_CPU_TEMPERATURE.into()),
    );
    let Some((c, _)) = found else {
        return;
    };
    if chips[c].name != "k10temp" && chips[c].name != "zenpower" {
        return;
    }
    for (i, sensor) in chips[c].sensors.iter().enumerate() {
        let Some(n) = sensor.label.as_deref().and_then(|l| l.strip_prefix("Tccd")) else {
            continue;
        };
        let key = format!("{}.ccd{n}", keys::CPU_TEMPERATURE);
        let label = format!("CPU CCD{n} temperature");
        let wanted = (
            key.as_str(),
            Category::Cpu,
            label.as_str(),
            Quantity::Celsius,
        );
        picks.add(chips, wanted, Ok((c, i)));
    }
}

/// `(chip, sensor)` and label of every `kind` sensor on chips that are not
/// GPUs (a graphics card's fan and rails are `gpu.<n>.*`), in chip order.
fn board_sensors(chips: &[Chip], kind: Kind) -> Vec<((usize, usize), &str)> {
    chips
        .iter()
        .enumerate()
        .filter(|(_, chip)| !chip.is_gpu())
        .flat_map(|(c, chip)| {
            chip.sensors
                .iter()
                .enumerate()
                .filter(move |(_, s)| s.kind == kind)
                .map(move |(i, s)| ((c, i), s.label.as_deref().unwrap_or_default()))
        })
        .collect()
}

/// Why a fan or rail picked by label is unavailable: hwmon has no `sensor`
/// at all, or `unlabelled` says none carries the label.
fn why_missing(found: &[((usize, usize), &str)], sensor: &str, unlabelled: &str) -> String {
    if found.is_empty() {
        format!("hwmon has no {sensor} sensor ({BOARD_DRIVER_HINT})")
    } else {
        format!("{unlabelled}; bind the theme to its hwmon sensor")
    }
}

/// `cpu.fan`, `fan.pump`, `fan.case1` and `fan.case2` by label.
fn pick_fans(chips: &[Chip], picks: &mut Picks) {
    let fans = board_sensors(chips, Kind::Fan);
    let labelled = |role| {
        fans.iter()
            .filter(move |(_, label)| fan_role(label) == Some(role))
    };
    let cpu = labelled(FanRole::Cpu)
        .find(|(_, label)| !optional_fan(label))
        .or_else(|| labelled(FanRole::Cpu).next());
    let pump = labelled(FanRole::Pump).next();
    let mut cases = labelled(FanRole::Case);
    let (case1, case2) = (cases.next(), cases.next());
    let found = |fan: Option<&((usize, usize), &str)>, unlabelled: &str| {
        fan.map(|(at, _)| *at)
            .ok_or_else(|| why_missing(&fans, "fan", unlabelled))
    };
    let rpm = Quantity::Rpm;
    picks.add(
        chips,
        (keys::CPU_FAN, Category::Cpu, "CPU fan", rpm),
        found(cpu, "no hwmon fan is labelled CPU"),
    );
    picks.add(
        chips,
        (keys::FAN_PUMP, Category::Board, "Pump", rpm),
        found(pump, "no hwmon fan is labelled pump"),
    );
    picks.add(
        chips,
        (keys::FAN_CASE_1, Category::Board, "Case fan 1", rpm),
        found(case1, "no hwmon fan is labelled chassis, case or system"),
    );
    picks.add(
        chips,
        (keys::FAN_CASE_2, Category::Board, "Case fan 2", rpm),
        found(
            case2,
            "no second hwmon fan is labelled chassis, case or system",
        ),
    );
}

/// `cpu.voltage`: the rail labelled as the CPU core.
fn pick_cpu_voltage(chips: &[Chip], picks: &mut Picks) {
    let rails = board_sensors(chips, Kind::In);
    let found = rails
        .iter()
        .find(|(_, label)| cpu_core_voltage(label))
        .map(|(at, _)| *at)
        .ok_or_else(|| {
            let unlabelled = "no hwmon voltage is labelled as the CPU core (Vcore)";
            why_missing(&rails, "voltage", unlabelled)
        });
    let wanted = (
        keys::CPU_VOLTAGE,
        Category::Cpu,
        "CPU core voltage",
        Quantity::Volts,
    );
    picks.add(chips, wanted, found);
}

/// The catalog entries of a chip's own sensors.
fn chip_entries(chip: &Chip) -> impl Iterator<Item = SensorInfo> + '_ {
    chip.sensors.iter().filter_map(move |sensor| {
        let label = sensor.label.as_deref().unwrap_or(&sensor.name);
        let file = sensor
            .input
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        describe(
            &sensor.key,
            chip.category(),
            format!("{} {label}", chip.id),
            sensor.kind.quantity(),
            format!("hwmon {} {file}", chip.name),
        )
    })
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
        let mut picks = Picks::default();
        pick_cpu_temperatures(&chips, &mut picks);
        pick_fans(&chips, &mut picks);
        pick_cpu_voltage(&chips, &mut picks);
        let mut catalog = picks.catalog;
        catalog.extend(chips.iter().flat_map(chip_entries));
        Self {
            chips,
            aliases: picks.aliases,
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
        for alias in &self.aliases {
            let reading = match &alias.target {
                Ok((c, s)) => readings[*c][*s].clone(),
                Err(why) => Reading::Unavailable(why.clone()),
            };
            put(out, &alias.key, reading);
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
        let quantities: Vec<Quantity> = catalog
            .iter()
            .filter(|i| i.key.as_str().starts_with("hwmon."))
            .map(|i| i.quantity)
            .collect();
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
    }

    #[test]
    fn gpu_power_prefers_the_average_like_amdgpu() {
        let t = FakeTree::new("hwmon-power");
        let card = "pci0000:00/0000:03:00.0";
        chip(
            &t,
            0,
            "amdgpu",
            card,
            &[
                ("power1_input", "310000000"),
                ("power1_label", "PPT"),
                ("in0_input", "905"),
                ("in0_label", "vddgfx"),
            ],
        );
        let (hwmon, s) = sample(&t);
        assert_eq!(
            get(&s, "hwmon.amdgpu.ppt"),
            Reading::Value(310.0),
            "the input when it is the only one"
        );
        t.file("sys/class/hwmon/hwmon0/power1_average", "300000000\n");
        let (hwmon_both, s) = sample(&t);
        assert_eq!(get(&s, "hwmon.amdgpu.ppt"), Reading::Value(300.0));
        let source = |h: &Hwmon| {
            h.catalog()
                .into_iter()
                .find(|i| i.key.as_str() == "hwmon.amdgpu.ppt")
                .map(|i| i.source)
        };
        assert_eq!(source(&hwmon).as_deref(), Some("hwmon amdgpu power1_input"));
        assert_eq!(
            source(&hwmon_both).as_deref(),
            Some("hwmon amdgpu power1_average")
        );

        // The amdgpu provider reads the same card's power with the same rule.
        t.dir("sys/drivers/amdgpu")
            .link(&format!("sys/devices/{card}/driver"), "sys/drivers/amdgpu")
            .link("sys/class/drm/card0/device", &format!("sys/devices/{card}"))
            .link(
                &format!("sys/devices/{card}/hwmon/hwmon0"),
                "sys/class/hwmon/hwmon0",
            );
        let roots = Roots::new(t.path("sys"), t.path("proc"));
        let mut gpus = crate::gpu::number(crate::amdgpu::discover(&roots));
        let mut out = Snapshot::default();
        gpus.providers[0].sample(Instant::now(), &mut out);
        assert_eq!(get(&out, "gpu.0.power"), get(&s, "hwmon.amdgpu.ppt"));
        assert_eq!(get(&out, "gpu.0.voltage"), get(&s, "hwmon.amdgpu.vddgfx"));
        assert_eq!(get(&out, "gpu.0.voltage"), Reading::Value(0.905));
        assert_eq!(power_file(1, |_| false), None);
    }

    #[test]
    fn fans_and_voltages_use_catalog_keys() {
        let t = FakeTree::new("hwmon-board");
        chip(
            &t,
            0,
            "nct6798",
            "nct6775.656",
            &[
                ("fan1_input", "850"),
                ("fan1_label", "CPU Optional"),
                ("fan2_input", "1200"),
                ("fan2_label", "CPU Fan"),
                ("fan3_input", "2400"),
                ("fan3_label", "AIO Pump"),
                ("fan4_input", "700"),
                ("fan4_label", "Chassis Fan 1"),
                ("fan5_input", "0"),
                ("fan5_label", "SYS_FAN2"),
                ("fan6_input", "999"),
                ("in0_input", "3344"),
                ("in0_label", "+3.3V"),
                ("in1_input", "1104"),
                ("in1_label", "Vcore"),
            ],
        );
        // A graphics card's fan and rail never stand in for the board's.
        chip(
            &t,
            1,
            "amdgpu",
            "0000:03:00.0",
            &[
                ("fan1_input", "1650"),
                ("fan1_label", "CPU"),
                ("in0_input", "905"),
                ("in0_label", "Vcore"),
            ],
        );
        let (hwmon, s) = sample(&t);
        assert_eq!(get(&s, keys::CPU_FAN), Reading::Value(1200.0));
        assert_eq!(get(&s, keys::FAN_PUMP), Reading::Value(2400.0));
        assert_eq!(get(&s, keys::FAN_CASE_1), Reading::Value(700.0));
        assert_eq!(
            get(&s, keys::FAN_CASE_2),
            Reading::Value(0.0),
            "a stopped fan is a real 0"
        );
        assert_eq!(get(&s, keys::CPU_VOLTAGE), Reading::Value(1.104));
        let catalog = hwmon.catalog();
        let info = |key: &str| catalog.iter().find(|i| i.key.as_str() == key).unwrap();
        assert_eq!(info(keys::CPU_FAN).source, "hwmon nct6798 CPU Fan");
        assert_eq!(info(keys::CPU_FAN).quantity, Quantity::Rpm);
        assert_eq!(info(keys::FAN_CASE_2).category, Category::Board);
        assert_eq!(info(keys::CPU_VOLTAGE).source, "hwmon nct6798 Vcore");
        assert_eq!(info(keys::CPU_VOLTAGE).quantity, Quantity::Volts);
        assert_eq!(get(&s, "hwmon.nct6798.fan6"), Reading::Value(999.0));

        // Only an optional CPU header: it is the CPU fan. Unlabelled fans and
        // rails say how to bind them.
        let t = FakeTree::new("hwmon-unlabelled");
        chip(
            &t,
            0,
            "it8688",
            "it87.2624",
            &[
                ("fan1_input", "900"),
                ("fan2_input", "600"),
                ("fan2_label", "CPU_OPT"),
                ("in0_input", "1200"),
            ],
        );
        let (_, s) = sample(&t);
        assert_eq!(get(&s, keys::CPU_FAN), Reading::Value(600.0));
        for key in [keys::FAN_PUMP, keys::FAN_CASE_1, keys::CPU_VOLTAGE] {
            assert!(
                matches!(get(&s, key), Reading::Unavailable(r) if r.starts_with("no hwmon") && r.ends_with("bind the theme to its hwmon sensor")),
                "{key}: {:?}",
                get(&s, key)
            );
        }

        // No board chip at all: every key is listed and says why.
        let t = FakeTree::new("hwmon-no-board");
        chip(
            &t,
            0,
            "k10temp",
            "0000:00:18.3",
            &[("temp1_input", "46000")],
        );
        let (hwmon, s) = sample(&t);
        let listed: Vec<String> = hwmon.catalog().iter().map(|i| i.key.to_string()).collect();
        for key in [
            keys::CPU_FAN,
            keys::FAN_PUMP,
            keys::FAN_CASE_1,
            keys::FAN_CASE_2,
            keys::CPU_VOLTAGE,
        ] {
            assert!(listed.iter().any(|k| k == key), "{key} listed");
            assert!(
                matches!(get(&s, key), Reading::Unavailable(r) if r.starts_with("hwmon has no") && r.contains("nct6775")),
                "{key}: {:?}",
                get(&s, key)
            );
        }
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
        let chips: Vec<SensorInfo> = hwmon
            .catalog()
            .into_iter()
            .filter(|i| i.key.as_str().starts_with("hwmon."))
            .collect();
        let keys: Vec<String> = chips.iter().map(|i| i.key.to_string()).collect();
        assert_eq!(
            keys,
            [
                "hwmon.nvme0.composite",
                "hwmon.nvme1.composite",
                "hwmon.spd5118-0.temp1",
                "hwmon.spd5118-1.temp1"
            ]
        );
        assert_eq!(get(&s, "hwmon.spd5118-0.temp1"), Reading::Value(40.5));
        assert_eq!(get(&s, "hwmon.spd5118-1.temp1"), Reading::Value(40.25));
        assert_eq!(chips[2].category, Category::Memory);
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
