//! Sensors: what can be measured, the value of a measurement, and how it is
//! shown. Adapters measure; the core names, converts and formats.
//!
//! Keys are stable strings so themes stay portable between machines:
//! well-known keys (`cpu.usage`, `cpu.temperature`, `gpu.0.power`,
//! `memory.used`, `net.down`, …) plus open-ended ones for whatever a machine
//! exposes (`hwmon.<chip>.<label>`, `disk.<mount>.used`, `net.<iface>.up`).

use std::collections::BTreeMap;
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

/// Well-known keys every source tries to provide. Themes should prefer them.
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
    /// Primary GPU usage, percent.
    pub const GPU_USAGE: &str = "gpu.usage";
    /// Primary GPU temperature, °C.
    pub const GPU_TEMPERATURE: &str = "gpu.temperature";
    /// Primary GPU memory used, bytes.
    pub const GPU_MEMORY_USED: &str = "gpu.memory.used";
    /// Primary GPU power, W.
    pub const GPU_POWER: &str = "gpu.power";
    /// RAM in use, bytes.
    pub const MEMORY_USED: &str = "memory.used";
    /// Total RAM, bytes.
    pub const MEMORY_TOTAL: &str = "memory.total";
    /// RAM in use, percent.
    pub const MEMORY_PERCENT: &str = "memory.percent";
    /// Download rate of all non-loopback interfaces, bytes per second.
    pub const NET_DOWN: &str = "net.down";
    /// Upload rate of all non-loopback interfaces, bytes per second.
    pub const NET_UP: &str = "net.up";
    /// Disk read rate of all disks, bytes per second.
    pub const DISK_READ: &str = "disk.read";
    /// Disk write rate of all disks, bytes per second.
    pub const DISK_WRITE: &str = "disk.write";
    /// Time since boot, seconds.
    pub const UPTIME: &str = "system.uptime";
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
    if !value.is_finite() || !(max > min) {
        return None;
    }
    Some(((value - min) / (max - min)).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

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
