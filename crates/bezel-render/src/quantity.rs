//! What a sensor key measures, for formatting sensor text.
//!
//! Themes bind text to a key, not to a [`Quantity`]; the well-known keys are
//! mapped exactly and open-ended keys (`hwmon.<chip>.<label>`,
//! `disk.<mount>.used`, `net.<iface>.up`) by their last segment.

use bezel_core::domain::sensor::{Quantity, SensorKey, keys};

/// The quantity of `key`; plain numbers when nothing matches.
pub(crate) fn quantity_of(key: &SensorKey) -> Quantity {
    let k = key.as_str().to_ascii_lowercase();
    match k.as_str() {
        keys::CPU_USAGE | keys::GPU_USAGE | keys::MEMORY_PERCENT => Quantity::Percent,
        keys::CPU_TEMPERATURE | keys::GPU_TEMPERATURE => Quantity::Celsius,
        keys::CPU_FREQUENCY => Quantity::Megahertz,
        keys::CPU_POWER | keys::GPU_POWER => Quantity::Watts,
        keys::CPU_LOAD_1 => Quantity::Number,
        keys::MEMORY_USED | keys::MEMORY_TOTAL | keys::GPU_MEMORY_USED => Quantity::Bytes,
        keys::NET_DOWN | keys::NET_UP | keys::DISK_READ | keys::DISK_WRITE => {
            Quantity::BytesPerSecond
        }
        keys::UPTIME => Quantity::Seconds,
        _ => by_segments(&k),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn q(k: &str) -> Quantity {
        quantity_of(&SensorKey::new(k).expect("key"))
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
}
