//! Sensors: what can be measured, the value of a measurement, and how it is
//! shown. Adapters measure; the core names, converts and formats.
//!
//! Keys are stable strings so themes stay portable between machines:
//! well-known keys (`cpu.usage`, `cpu.temperature`, `gpu.0.power`,
//! `memory.used`, `net.down`, …) plus open-ended ones for whatever a machine
//! exposes (`hwmon.<chip>.<label>`, `disk.<mount>.used`, `net.<iface>.up`).

use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

/// A sensor key, e.g. `cpu.temperature`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SensorKey(String);

impl SensorKey {
    /// Builds a key; `None` when empty or containing whitespace.
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into();
        (!key.is_empty() && !key.chars().any(char::is_whitespace)).then_some(Self(key))
    }

    /// Weather keys contain only a fixed prefix, numeric coordinates and suffix.
    pub(super) fn weather(latitude: f64, longitude: f64) -> [Self; 2] {
        let base = format!("weather.{latitude:.6}:{longitude:.6}");
        [
            Self(format!("{base}.temperature")),
            Self(format!("{base}.code")),
        ]
    }

    /// The key text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SensorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

/// Keys order and compare as their text, so sets of keys are looked up by
/// `&str`.
impl Borrow<str> for SensorKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// The sensors a caller shows the user, declared to the source
/// ([`crate::ports::SensorSource::want`]) so that a sensor whose measuring
/// reaches outside this machine (`net.ping` sends packets) is measured only
/// while something shown uses it (D-2026-09-30-release-polish-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wanted {
    /// Every sensor: a listing of all of them (`bezel sensors`).
    All,
    /// Only these: a theme's elements, the sensors a list shows.
    Keys(BTreeSet<SensorKey>),
}

impl Wanted {
    /// No sensor.
    pub fn nothing() -> Self {
        Self::Keys(BTreeSet::new())
    }

    /// Whether `key` is wanted.
    pub fn contains(&self, key: &str) -> bool {
        match self {
            Self::All => true,
            Self::Keys(keys) => keys.contains(key),
        }
    }

    /// What either wants.
    #[must_use]
    pub fn union(&self, other: &Wanted) -> Wanted {
        match (self, other) {
            (Self::Keys(a), Self::Keys(b)) => Self::Keys(a.union(b).cloned().collect()),
            _ => Self::All,
        }
    }
}

impl FromIterator<SensorKey> for Wanted {
    fn from_iter<I: IntoIterator<Item = SensorKey>>(keys: I) -> Self {
        Self::Keys(keys.into_iter().collect())
    }
}

/// Well-known keys every source tries to provide, on every platform. Themes
/// should prefer them. A source that cannot measure one on this machine
/// still lists it and reads it as unavailable, with the reason.
pub mod keys {
    /// Total CPU usage, percent.
    pub const CPU_USAGE: &str = "cpu.usage";
    /// Average current CPU frequency, MHz.
    pub const CPU_FREQUENCY: &str = "cpu.frequency";
    /// Best CPU temperature (package/Tctl), °C.
    pub const CPU_TEMPERATURE: &str = "cpu.temperature";
    /// CPU package power, W.
    pub const CPU_POWER: &str = "cpu.power";
    /// 1-minute load average (run-queue length, not a percentage).
    pub const CPU_LOAD_1: &str = "cpu.load.1";
    /// 5-minute load average.
    pub const CPU_LOAD_5: &str = "cpu.load.5";
    /// 15-minute load average.
    pub const CPU_LOAD_15: &str = "cpu.load.15";
    /// Processor model name, text.
    pub const CPU_NAME: &str = "cpu.name";
    /// CPU fan speed (the fan whose label says CPU), RPM.
    pub const CPU_FAN: &str = "cpu.fan";
    /// CPU core voltage (the rail labelled Vcore or CPU core), V.
    pub const CPU_VOLTAGE: &str = "cpu.voltage";
    /// Primary GPU usage, percent.
    pub const GPU_USAGE: &str = "gpu.usage";
    /// Primary GPU temperature, °C.
    pub const GPU_TEMPERATURE: &str = "gpu.temperature";
    /// Primary GPU memory used, bytes.
    pub const GPU_MEMORY_USED: &str = "gpu.memory.used";
    /// Primary GPU memory size, bytes.
    pub const GPU_MEMORY_TOTAL: &str = "gpu.memory.total";
    /// Primary GPU memory used, percent.
    pub const GPU_MEMORY_PERCENT: &str = "gpu.memory.percent";
    /// Primary GPU power, W.
    pub const GPU_POWER: &str = "gpu.power";
    /// Primary GPU core clock, MHz.
    pub const GPU_FREQUENCY: &str = "gpu.frequency";
    /// Primary GPU fan duty, percent.
    pub const GPU_FAN: &str = "gpu.fan";
    /// Primary GPU core voltage, V.
    pub const GPU_VOLTAGE: &str = "gpu.voltage";
    /// Primary GPU model, text.
    pub const GPU_NAME: &str = "gpu.name";
    /// Frame rate of the game running now, frames per second.
    pub const GPU_FPS: &str = "gpu.fps";
    /// RAM in use, bytes.
    pub const MEMORY_USED: &str = "memory.used";
    /// Total RAM, bytes.
    pub const MEMORY_TOTAL: &str = "memory.total";
    /// RAM in use, percent.
    pub const MEMORY_PERCENT: &str = "memory.percent";
    /// RAM programs can still get without swapping, bytes.
    pub const MEMORY_AVAILABLE: &str = "memory.available";
    /// RAM programs can still get, percent of the total.
    pub const MEMORY_AVAILABLE_PERCENT: &str = "memory.available.percent";
    /// Swap (the page file on Windows) in use, bytes.
    pub const SWAP_USED: &str = "memory.swap.used";
    /// Swap size, bytes.
    pub const SWAP_TOTAL: &str = "memory.swap.total";
    /// Swap in use, percent.
    pub const SWAP_PERCENT: &str = "memory.swap.percent";
    /// Liquid-cooling pump speed (the fan whose label says pump), RPM.
    pub const FAN_PUMP: &str = "fan.pump";
    /// First case fan (labelled chassis, case or system), RPM.
    pub const FAN_CASE_1: &str = "fan.case1";
    /// Second case fan, RPM.
    pub const FAN_CASE_2: &str = "fan.case2";
    /// Download rate of the physical interfaces (no loopback, bridges, VPNs or
    /// containers, whose traffic also crosses a physical NIC), bytes per second.
    pub const NET_DOWN: &str = "net.down";
    /// Upload rate of the physical interfaces, bytes per second.
    pub const NET_UP: &str = "net.up";
    /// Bytes the physical interfaces received since they came up.
    pub const NET_DOWN_TOTAL: &str = "net.down.total";
    /// Bytes the physical interfaces sent since they came up.
    pub const NET_UP_TOTAL: &str = "net.up.total";
    /// Round trip to the configured host, milliseconds.
    pub const NET_PING: &str = "net.ping";
    /// Disk read rate of all disks, bytes per second.
    pub const DISK_READ: &str = "disk.read";
    /// Disk write rate of all disks, bytes per second.
    pub const DISK_WRITE: &str = "disk.write";
    /// Space used on the root filesystem (`disk.<mount>.used` of `/`), bytes.
    pub const ROOT_DISK_USED: &str = "disk.root.used";
    /// Size of the root filesystem, bytes.
    pub const ROOT_DISK_TOTAL: &str = "disk.root.total";
    /// Space free on the root filesystem, bytes.
    pub const ROOT_DISK_FREE: &str = "disk.root.free";
    /// Space used on the root filesystem, percent.
    pub const ROOT_DISK_PERCENT: &str = "disk.root.percent";
    /// Time since boot, seconds.
    pub const UPTIME: &str = "system.uptime";
    /// The machine's host name, text.
    pub const HOSTNAME: &str = "system.hostname";
    /// Output volume, percent. Not supported yet: always unavailable.
    pub const SYSTEM_VOLUME: &str = "system.volume";

    /// Keys themes imported from the vendor app (`.turtheme`) and from
    /// turing-smart-screen-python bind to that Bezel's own themes did not
    /// use before. Every source lists them, so an imported theme finds each
    /// one measured, or unavailable with the reason.
    pub const IMPORTED: [&str; 12] = [
        CPU_FAN,
        FAN_PUMP,
        FAN_CASE_1,
        FAN_CASE_2,
        CPU_VOLTAGE,
        GPU_VOLTAGE,
        GPU_FPS,
        NET_PING,
        NET_DOWN_TOTAL,
        NET_UP_TOTAL,
        MEMORY_AVAILABLE_PERCENT,
        SYSTEM_VOLUME,
    ];
}

/// What a value measures; decides units and formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quantity {
    /// 0..=100.
    Percent,
    /// Degrees Celsius.
    Celsius,
    /// Megahertz.
    Megahertz,
    /// Watts.
    Watts,
    /// Volts.
    Volts,
    /// Amperes.
    Amperes,
    /// Revolutions per minute.
    Rpm,
    /// Bytes.
    Bytes,
    /// Bytes per second.
    BytesPerSecond,
    /// Seconds.
    Seconds,
    /// A plain number (load average, frames per second).
    Number,
    /// Text (names, versions).
    Text,
}

impl Quantity {
    /// Stable lowercase name (`percent`, `bytesPerSecond`, …) for JSON and UIs.
    pub fn slug(self) -> &'static str {
        match self {
            Quantity::Percent => "percent",
            Quantity::Celsius => "celsius",
            Quantity::Megahertz => "megahertz",
            Quantity::Watts => "watts",
            Quantity::Volts => "volts",
            Quantity::Amperes => "amperes",
            Quantity::Rpm => "rpm",
            Quantity::Bytes => "bytes",
            Quantity::BytesPerSecond => "bytesPerSecond",
            Quantity::Seconds => "seconds",
            Quantity::Number => "number",
            Quantity::Text => "text",
        }
    }
}

/// Grouping for the UI's sensor browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// Processor.
    Cpu,
    /// Graphics.
    Gpu,
    /// RAM and swap.
    Memory,
    /// Storage.
    Disk,
    /// Network.
    Network,
    /// Motherboard, fans, other hwmon chips.
    Board,
    /// Uptime, host name, OS.
    System,
}

impl Category {
    /// Every category, in display order.
    pub const ALL: [Category; 7] = [
        Category::Cpu,
        Category::Gpu,
        Category::Memory,
        Category::Disk,
        Category::Network,
        Category::Board,
        Category::System,
    ];

    /// Stable lowercase name (`cpu`, `gpu`, …) for JSON and UIs.
    pub fn slug(self) -> &'static str {
        match self {
            Category::Cpu => "cpu",
            Category::Gpu => "gpu",
            Category::Memory => "memory",
            Category::Disk => "disk",
            Category::Network => "network",
            Category::Board => "board",
            Category::System => "system",
        }
    }
}

/// A sensor this machine offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorInfo {
    /// Stable key.
    pub key: SensorKey,
    /// Group in the sensor browser.
    pub category: Category,
    /// Human label in English (the UI translates the well-known ones).
    pub label: String,
    /// What it measures.
    pub quantity: Quantity,
    /// Where the value comes from (e.g. `hwmon k10temp Tctl`, `NVML`).
    pub source: String,
}

/// The quantity `key`'s name implies; plain numbers when nothing matches.
///
/// For keys a catalog does not know (a theme made on another machine):
/// well-known keys exactly, open-ended keys (`hwmon.<chip>.<label>`,
/// `disk.<mount>.used`, `net.<iface>.up`) by their segments.
pub fn quantity_of(key: &SensorKey) -> Quantity {
    let k = key.as_str().to_ascii_lowercase();
    WELL_KNOWN
        .iter()
        .find(|(known, _)| *known == k)
        .map_or_else(|| by_segments(&k), |(_, quantity)| *quantity)
}

/// What the well-known keys measure, for those whose name alone would
/// mislead (`gpu.fan` is a duty in percent, `fan.pump` a speed) or say
/// nothing (`cpu.name`).
const WELL_KNOWN: [(&str, Quantity); 30] = [
    (keys::CPU_USAGE, Quantity::Percent),
    (keys::GPU_USAGE, Quantity::Percent),
    (keys::GPU_FAN, Quantity::Percent),
    (keys::MEMORY_PERCENT, Quantity::Percent),
    (keys::SYSTEM_VOLUME, Quantity::Percent),
    (keys::CPU_TEMPERATURE, Quantity::Celsius),
    (keys::GPU_TEMPERATURE, Quantity::Celsius),
    (keys::CPU_FREQUENCY, Quantity::Megahertz),
    (keys::CPU_POWER, Quantity::Watts),
    (keys::GPU_POWER, Quantity::Watts),
    (keys::CPU_VOLTAGE, Quantity::Volts),
    (keys::GPU_VOLTAGE, Quantity::Volts),
    (keys::CPU_FAN, Quantity::Rpm),
    (keys::FAN_PUMP, Quantity::Rpm),
    (keys::FAN_CASE_1, Quantity::Rpm),
    (keys::FAN_CASE_2, Quantity::Rpm),
    (keys::CPU_LOAD_1, Quantity::Number),
    (keys::GPU_FPS, Quantity::Number),
    (keys::NET_PING, Quantity::Number),
    (keys::MEMORY_USED, Quantity::Bytes),
    (keys::MEMORY_TOTAL, Quantity::Bytes),
    (keys::GPU_MEMORY_USED, Quantity::Bytes),
    (keys::NET_DOWN, Quantity::BytesPerSecond),
    (keys::NET_UP, Quantity::BytesPerSecond),
    (keys::DISK_READ, Quantity::BytesPerSecond),
    (keys::DISK_WRITE, Quantity::BytesPerSecond),
    (keys::UPTIME, Quantity::Seconds),
    (keys::CPU_NAME, Quantity::Text),
    (keys::GPU_NAME, Quantity::Text),
    (keys::HOSTNAME, Quantity::Text),
];

fn by_segments(key: &str) -> Quantity {
    let first = key.split('.').next().unwrap_or_default();
    let last = key.rsplit('.').next().unwrap_or_default();
    let rate = match first {
        "net" => matches!(last, "up" | "down" | "rx" | "tx"),
        "disk" => matches!(last, "read" | "write"),
        _ => false,
    };
    if rate {
        return Quantity::BytesPerSecond;
    }
    by_last_segment(last)
}

fn by_last_segment(last: &str) -> Quantity {
    let voltage_input =
        last.len() > 2 && last.starts_with("in") && last[2..].chars().all(|c| c.is_ascii_digit());
    match last {
        "usage" | "percent" | "utilization" | "load" => Quantity::Percent,
        "temperature" | "tctl" | "tdie" | "edge" | "junction" => Quantity::Celsius,
        l if l.contains("temp") => Quantity::Celsius,
        "frequency" | "clock" | "freq" => Quantity::Megahertz,
        "power" => Quantity::Watts,
        "voltage" | "vcore" => Quantity::Volts,
        _ if voltage_input => Quantity::Volts,
        l if l.starts_with("fan") || l == "rpm" => Quantity::Rpm,
        "used" | "total" | "free" | "available" | "size" => Quantity::Bytes,
        "uptime" => Quantity::Seconds,
        _ => Quantity::Number,
    }
}

/// What each sensor of a catalog measures, so a value bound in a theme is
/// formatted with its real unit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quantities(BTreeMap<SensorKey, Quantity>);

impl Quantities {
    /// No entries (every unit then comes from the key's well-known name).
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// The quantities of `catalog`.
    pub fn from_catalog(catalog: &[SensorInfo]) -> Self {
        Self(
            catalog
                .iter()
                .map(|s| (s.key.clone(), s.quantity))
                .collect(),
        )
    }

    /// What `key` measures, when the catalog has it.
    pub fn get(&self, key: &SensorKey) -> Option<Quantity> {
        self.0.get(key).copied()
    }

    /// What `key` measures: the catalog's word, else what its name says
    /// ([`quantity_of`]), so a theme on another machine still gets units.
    pub fn quantity(&self, key: &SensorKey) -> Quantity {
        self.get(key).unwrap_or_else(|| quantity_of(key))
    }

    /// True without entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One measurement.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    /// A number in the sensor's [`Quantity`].
    Value(f64),
    /// A text value.
    Text(String),
    /// Not measurable right now, with the reason shown to the user.
    Unavailable(String),
}

impl Reading {
    /// The number, if any.
    pub fn value(&self) -> Option<f64> {
        match self {
            Reading::Value(v) if v.is_finite() => Some(*v),
            _ => None,
        }
    }
}

/// Readings taken together.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    readings: BTreeMap<SensorKey, Reading>,
}

impl Snapshot {
    /// Stores a reading.
    pub fn insert(&mut self, key: SensorKey, reading: Reading) {
        self.readings.insert(key, reading);
    }

    /// The reading of `key`; missing keys read as unavailable.
    pub fn get(&self, key: &SensorKey) -> Reading {
        self.readings.get(key).cloned().unwrap_or_else(|| {
            Reading::Unavailable(format!("{key} is not provided on this machine"))
        })
    }

    /// Number of readings.
    pub fn len(&self) -> usize {
        self.readings.len()
    }

    /// True without readings.
    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
    }

    /// Readings in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&SensorKey, &Reading)> {
        self.readings.iter()
    }
}

/// The rate of a monotonically increasing counter between two samples, per
/// second of **actual** elapsed time. A counter that went backwards (reset,
/// interface re-created) yields `None`, as does a zero interval.
pub fn rate(previous: u64, current: u64, elapsed: Duration) -> Option<f64> {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 || current < previous {
        return None;
    }
    Some((current - previous) as f64 / secs)
}

/// Temperature scale for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemperatureUnit {
    /// °C.
    #[default]
    Celsius,
    /// °F.
    Fahrenheit,
}

/// Byte multiples for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ByteUnits {
    /// 1 KiB = 1024 B, shown as `KiB`, `MiB`, `GiB`.
    #[default]
    Binary,
    /// 1 kB = 1000 B, shown as `kB`, `MB`, `GB`.
    Decimal,
}

/// How a value is turned into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayFormat {
    /// Decimal places; `None` picks a sensible default per quantity.
    pub decimals: Option<u8>,
    /// Append the unit.
    pub show_unit: bool,
    /// Temperature scale.
    pub temperature: TemperatureUnit,
    /// Byte multiples.
    pub bytes: ByteUnits,
}

impl Default for DisplayFormat {
    fn default() -> Self {
        Self {
            decimals: None,
            show_unit: true,
            temperature: TemperatureUnit::Celsius,
            bytes: ByteUnits::Binary,
        }
    }
}

/// Formats a reading, e.g. `63°C`, `4.72 GHz`, `12.4 MiB/s`, `—` when unavailable.
pub fn format_reading(reading: &Reading, quantity: Quantity, format: DisplayFormat) -> String {
    match reading {
        Reading::Text(t) => t.clone(),
        Reading::Unavailable(_) => "—".to_string(),
        Reading::Value(v) if !v.is_finite() => "—".to_string(),
        Reading::Value(v) => format_value(*v, quantity, format),
    }
}

fn format_value(v: f64, quantity: Quantity, f: DisplayFormat) -> String {
    let (number, unit, default_decimals) = match quantity {
        Quantity::Percent => (v, "%", 0),
        Quantity::Celsius => match f.temperature {
            TemperatureUnit::Celsius => (v, "°C", 0),
            TemperatureUnit::Fahrenheit => (v * 9.0 / 5.0 + 32.0, "°F", 0),
        },
        Quantity::Megahertz if v >= 1000.0 => (v / 1000.0, " GHz", 2),
        Quantity::Megahertz => (v, " MHz", 0),
        Quantity::Watts => (v, " W", 0),
        Quantity::Volts => (v, " V", 2),
        Quantity::Amperes => (v, " A", 2),
        Quantity::Rpm => (v, " RPM", 0),
        Quantity::Bytes => return scaled_bytes(v, "", f),
        Quantity::BytesPerSecond => return scaled_bytes(v, "/s", f),
        Quantity::Seconds => return duration_text(v),
        Quantity::Number | Quantity::Text => (v, "", 1),
    };
    let decimals = usize::from(f.decimals.unwrap_or(default_decimals));
    let text = format!("{number:.decimals$}");
    if f.show_unit {
        format!("{text}{unit}")
    } else {
        text
    }
}

fn scaled_bytes(v: f64, suffix: &str, f: DisplayFormat) -> String {
    let (base, names): (f64, [&str; 5]) = match f.bytes {
        ByteUnits::Binary => (1024.0, ["B", "KiB", "MiB", "GiB", "TiB"]),
        ByteUnits::Decimal => (1000.0, ["B", "kB", "MB", "GB", "TB"]),
    };
    let mut value = v.max(0.0);
    let mut index = 0;
    while value >= base && index < names.len() - 1 {
        value /= base;
        index += 1;
    }
    let default_decimals = if index == 0 || value >= 100.0 { 0 } else { 1 };
    let decimals = usize::from(f.decimals.unwrap_or(default_decimals));
    let text = format!("{value:.decimals$}");
    if f.show_unit {
        format!("{text} {}{suffix}", names[index])
    } else {
        text
    }
}

fn duration_text(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (days, rest) = (total / 86_400, total % 86_400);
    let (hours, minutes) = (rest / 3600, rest % 3600 / 60);
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else {
        format!("{hours:02}:{minutes:02}")
    }
}

/// The fraction (0..=1) a value represents between `min` and `max`, clamped.
/// Bars, rings and needles use it; `None` when the range is empty.
pub fn fraction(value: f64, min: f64, max: f64) -> Option<f64> {
    if !value.is_finite() || max.partial_cmp(&min) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    Some(((value - min) / (max - min)).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(k: &str) -> Quantity {
        quantity_of(&SensorKey::new(k).expect("key"))
    }

    fn wanted(list: &[&str]) -> Wanted {
        list.iter().filter_map(|k| SensorKey::new(*k)).collect()
    }

    #[test]
    fn wanted_sensors_combine() {
        let theme = wanted(&[keys::CPU_USAGE, keys::NET_PING]);
        assert!(theme.contains(keys::NET_PING));
        assert!(!theme.contains(keys::GPU_USAGE));
        assert!(!Wanted::nothing().contains(keys::NET_PING));
        assert!(Wanted::All.contains("anything.at.all"));
        let list = wanted(&[keys::GPU_USAGE, keys::CPU_USAGE]);
        assert_eq!(
            theme.union(&list),
            wanted(&[keys::CPU_USAGE, keys::GPU_USAGE, keys::NET_PING])
        );
        assert_eq!(theme.union(&Wanted::nothing()), theme);
        assert_eq!(Wanted::All.union(&list), Wanted::All);
        assert_eq!(list.union(&Wanted::All), Wanted::All);
    }

    #[test]
    fn well_known_keys() {
        assert_eq!(q(keys::CPU_USAGE), Quantity::Percent);
        assert_eq!(q(keys::GPU_TEMPERATURE), Quantity::Celsius);
        assert_eq!(q(keys::CPU_FREQUENCY), Quantity::Megahertz);
        assert_eq!(q(keys::GPU_POWER), Quantity::Watts);
        assert_eq!(q(keys::CPU_LOAD_1), Quantity::Number);
        assert_eq!(q(keys::MEMORY_USED), Quantity::Bytes);
        assert_eq!(q(keys::NET_DOWN), Quantity::BytesPerSecond);
        assert_eq!(q(keys::UPTIME), Quantity::Seconds);
    }

    #[test]
    fn imported_keys_have_their_units() {
        assert_eq!(q(keys::CPU_FAN), Quantity::Rpm);
        assert_eq!(q(keys::FAN_PUMP), Quantity::Rpm);
        assert_eq!(q(keys::FAN_CASE_2), Quantity::Rpm);
        assert_eq!(q(keys::GPU_FAN), Quantity::Percent, "a duty, not a speed");
        assert_eq!(q(keys::CPU_VOLTAGE), Quantity::Volts);
        assert_eq!(q(keys::GPU_VOLTAGE), Quantity::Volts);
        assert_eq!(q(keys::GPU_FPS), Quantity::Number);
        assert_eq!(q(keys::NET_PING), Quantity::Number);
        assert_eq!(q(keys::NET_DOWN_TOTAL), Quantity::Bytes);
        assert_eq!(q(keys::NET_UP_TOTAL), Quantity::Bytes);
        assert_eq!(q(keys::MEMORY_AVAILABLE_PERCENT), Quantity::Percent);
        assert_eq!(q(keys::SYSTEM_VOLUME), Quantity::Percent);
        assert_eq!(q(keys::ROOT_DISK_USED), Quantity::Bytes);
        assert_eq!(q(keys::ROOT_DISK_PERCENT), Quantity::Percent);
        assert_eq!(q(keys::CPU_NAME), Quantity::Text);
        assert_eq!(q(keys::HOSTNAME), Quantity::Text);
        assert_eq!(
            q("CPU.FAN"),
            Quantity::Rpm,
            "keys compare case-insensitively"
        );
        for key in keys::IMPORTED {
            assert!(SensorKey::new(key).is_some(), "{key}");
        }
    }

    #[test]
    fn open_ended_keys() {
        assert_eq!(q("net.eth0.up"), Quantity::BytesPerSecond);
        assert_eq!(q("disk.nvme0n1.read"), Quantity::BytesPerSecond);
        assert_eq!(q("disk./home.used"), Quantity::Bytes);
        assert_eq!(q("hwmon.k10temp.Tctl"), Quantity::Celsius);
        assert_eq!(q("hwmon.nct6798.cputin_temp"), Quantity::Celsius);
        assert_eq!(q("hwmon.nct6798.fan2"), Quantity::Rpm);
        assert_eq!(q("hwmon.nct6798.in0"), Quantity::Volts);
        assert_eq!(q("hwmon.nct6798.vcore"), Quantity::Volts);
        assert_eq!(q("gpu.1.usage"), Quantity::Percent);
        assert_eq!(q("gpu.1.clock"), Quantity::Megahertz);
        assert_eq!(q("gpu.1.power"), Quantity::Watts);
        assert_eq!(q("system.host.uptime"), Quantity::Seconds);
        assert_eq!(q("cpu.load.5"), Quantity::Number);
        assert_eq!(q("net.eth0.name"), Quantity::Number);
        assert_eq!(q("in"), Quantity::Number);
    }

    #[test]
    fn quantities_fall_back_to_the_key_name() {
        let q = Quantities::new();
        assert_eq!(
            q.quantity(&SensorKey::new("hwmon.nvme0.composite_temp").unwrap()),
            Quantity::Celsius
        );
        let catalog = [SensorInfo {
            key: SensorKey::new("hwmon.nvme0.composite").unwrap(),
            category: Category::Disk,
            label: "NVMe".into(),
            quantity: Quantity::Celsius,
            source: "hwmon".into(),
        }];
        let known = Quantities::from_catalog(&catalog);
        assert_eq!(
            known.quantity(&catalog[0].key),
            Quantity::Celsius,
            "the catalog wins"
        );
        assert_eq!(
            known.quantity(&SensorKey::new("x.y").unwrap()),
            Quantity::Number
        );
    }

    fn key(k: &str) -> SensorKey {
        SensorKey::new(k).unwrap()
    }

    #[test]
    fn keys_reject_blank_and_spaces() {
        assert!(SensorKey::new("").is_none());
        assert!(SensorKey::new("cpu temp").is_none());
        assert_eq!(key(keys::CPU_TEMPERATURE).as_str(), "cpu.temperature");
        assert_eq!(key("a.b").to_string(), "a.b");
    }

    #[test]
    fn snapshot_reports_missing_keys_as_unavailable() {
        let mut s = Snapshot::default();
        assert!(s.is_empty());
        s.insert(key("cpu.usage"), Reading::Value(12.5));
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(&key("cpu.usage")).value(), Some(12.5));
        assert!(matches!(s.get(&key("gpu.usage")), Reading::Unavailable(_)));
        assert_eq!(s.iter().count(), 1);
        assert_eq!(Reading::Value(f64::NAN).value(), None);
        assert_eq!(Reading::Text("x".into()).value(), None);
    }

    #[test]
    fn rates_use_the_real_elapsed_time() {
        assert_eq!(rate(1000, 3000, Duration::from_millis(500)), Some(4000.0));
        assert_eq!(
            rate(3000, 1000, Duration::from_secs(1)),
            None,
            "counter reset"
        );
        assert_eq!(rate(1, 2, Duration::ZERO), None);
    }

    #[test]
    fn formats_each_quantity() {
        let f = DisplayFormat::default();
        let v = |x| Reading::Value(x);
        assert_eq!(format_reading(&v(42.6), Quantity::Percent, f), "43%");
        assert_eq!(format_reading(&v(46.625), Quantity::Celsius, f), "47°C");
        let fahrenheit = DisplayFormat {
            temperature: TemperatureUnit::Fahrenheit,
            ..f
        };
        assert_eq!(
            format_reading(&v(100.0), Quantity::Celsius, fahrenheit),
            "212°F"
        );
        assert_eq!(
            format_reading(&v(4725.0), Quantity::Megahertz, f),
            "4.72 GHz"
        );
        assert_eq!(format_reading(&v(555.0), Quantity::Megahertz, f), "555 MHz");
        assert_eq!(format_reading(&v(31.58), Quantity::Watts, f), "32 W");
        assert_eq!(format_reading(&v(1.2345), Quantity::Volts, f), "1.23 V");
        assert_eq!(format_reading(&v(12.5), Quantity::Amperes, f), "12.50 A");
        assert_eq!(format_reading(&v(1200.0), Quantity::Rpm, f), "1200 RPM");
        assert_eq!(
            format_reading(&v(2730.0 * 1048576.0), Quantity::Bytes, f),
            "2.7 GiB"
        );
        assert_eq!(format_reading(&v(512.0), Quantity::Bytes, f), "512 B");
        let decimal = DisplayFormat {
            bytes: ByteUnits::Decimal,
            ..f
        };
        assert_eq!(
            format_reading(&v(12_400_000.0), Quantity::BytesPerSecond, decimal),
            "12.4 MB/s"
        );
        assert_eq!(
            format_reading(&v(150.0 * 1024.0 * 1024.0), Quantity::BytesPerSecond, f),
            "150 MiB/s"
        );
        assert_eq!(
            format_reading(&v(93_784.0), Quantity::Seconds, f),
            "1d 02:03"
        );
        assert_eq!(format_reading(&v(3_700.0), Quantity::Seconds, f), "01:01");
        assert_eq!(format_reading(&v(0.85), Quantity::Number, f), "0.8");
        assert_eq!(
            format_reading(&Reading::Text("RTX 4090".into()), Quantity::Text, f),
            "RTX 4090"
        );
        assert_eq!(
            format_reading(&Reading::Unavailable("n/a".into()), Quantity::Percent, f),
            "—"
        );
        assert_eq!(format_reading(&v(f64::NAN), Quantity::Percent, f), "—");
    }

    #[test]
    fn decimals_and_hidden_units() {
        let f = DisplayFormat {
            decimals: Some(1),
            show_unit: false,
            ..DisplayFormat::default()
        };
        assert_eq!(
            format_reading(&Reading::Value(42.66), Quantity::Percent, f),
            "42.7"
        );
        assert_eq!(
            format_reading(&Reading::Value(2048.0), Quantity::Bytes, f),
            "2.0"
        );
    }

    #[test]
    fn fractions_clamp_and_reject_empty_ranges() {
        assert_eq!(fraction(50.0, 0.0, 100.0), Some(0.5));
        assert_eq!(fraction(150.0, 0.0, 100.0), Some(1.0));
        assert_eq!(fraction(-5.0, 0.0, 100.0), Some(0.0));
        assert_eq!(fraction(5.0, 10.0, 10.0), None);
        assert_eq!(fraction(f64::NAN, 0.0, 1.0), None);
    }
}
