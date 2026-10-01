//! What every internal provider implements, and the helpers they share to
//! describe sensors and store readings.

use std::time::Instant;

use bezel_core::domain::sensor::{
    Category, Quantity, Reading, SensorInfo, SensorKey, Snapshot, Wanted,
};

/// Reason shown for a rate or usage before its second sample.
pub(crate) const WARMING_UP: &str = "warming up: needs a second sample";

/// Reason shown for a rate whose counter went backwards or did not advance
/// in time (see [`bezel_core::domain::sensor::rate`]).
pub(crate) const COUNTER_RESET: &str = "counter reset or no time elapsed";

/// Reason shown for a key Bezel lists but does not measure yet.
pub(crate) const NOT_SUPPORTED_YET: &str = "not supported yet";

/// One family of sensors (CPU times, hwmon chips, one GPU, ...). Providers
/// discover what they offer when they are built; `sample` then reads every
/// sensor of that catalog, never more, and reports what it cannot read as
/// [`Reading::Unavailable`].
pub(crate) trait Provider: Send {
    /// The sensors this provider reports.
    fn catalog(&self) -> Vec<SensorInfo>;
    /// Reads every sensor of the catalog into `out`. `now` is the instant of
    /// this sample: rates divide counter deltas by the real time between two
    /// calls (D-2026-09-30-sensors-4).
    fn sample(&mut self, now: Instant, out: &mut Snapshot);
    /// Which sensors are shown from now on
    /// ([`bezel_core::ports::SensorSource::want`]). Only a provider whose
    /// measuring reaches outside this machine cares (the default ignores it).
    fn want(&mut self, wanted: &Wanted) {
        let _ = wanted;
    }
}

/// Describes a sensor. `None` only for an invalid key, which the slugged keys
/// built by the providers never are.
pub(crate) fn describe(
    key: &str,
    category: Category,
    label: impl Into<String>,
    quantity: Quantity,
    source: impl Into<String>,
) -> Option<SensorInfo> {
    let key = SensorKey::new(key)?;
    Some(SensorInfo {
        key,
        category,
        label: label.into(),
        quantity,
        source: source.into(),
    })
}

/// Stores `reading` under `key` (ignored when `key` is not a valid key).
pub(crate) fn put(out: &mut Snapshot, key: &str, reading: Reading) {
    if let Some(key) = SensorKey::new(key) {
        out.insert(key, reading);
    }
}

/// A key segment: lower-case ASCII letters and digits, every other run of
/// characters folded into one `_` (`CPU Package` → `cpu_package`).
pub(crate) fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// What a board fan cools, from its label (hwmon `fanN_label`,
/// LibreHardwareMonitor's sensor name). The vendor app lets the user pick
/// these fans; Bezel takes the one whose label says it, names that sensor in
/// the catalog source, and reads the key as unavailable when no label does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FanRole {
    /// `CPU Fan`, `CPU_OPT`, `Processor Fan`.
    Cpu,
    /// `AIO Pump`, `W_PUMP+`.
    Pump,
    /// `Chassis Fan 1`, `CHA_FAN2`, `SYS_FAN1`, `System Fan #3`.
    Case,
}

/// The words of a label, slugged (`CPU_OPT` → `cpu`, `opt`).
fn words(label: &str) -> Vec<String> {
    slug(label).split('_').map(str::to_string).collect()
}

/// The role `label` names, if any. A pump header on the CPU (`CPU_PUMP`) is
/// a pump.
pub(crate) fn fan_role(label: &str) -> Option<FanRole> {
    let words = words(label);
    let starts = |prefix: &str| words.iter().any(|w| w.starts_with(prefix));
    if starts("pump") {
        Some(FanRole::Pump)
    } else if starts("cpu") || starts("processor") {
        Some(FanRole::Cpu)
    } else if starts("cha") || starts("case") || starts("sys") {
        Some(FanRole::Case)
    } else {
        None
    }
}

/// True for an optional header (`CPU_OPT`, `CPU Optional`): a CPU fan
/// labelled plainly is preferred to it.
pub(crate) fn optional_fan(label: &str) -> bool {
    words(label).iter().any(|w| w.starts_with("opt"))
}

/// True for the CPU core rail's label: `Vcore`, `CPU Core`, `SVI2_Core`
/// (zenpower), `Core (SVI2 TFN)` (LibreHardwareMonitor on AMD),
/// `VDDCR_CPU`. A VID is the voltage the CPU asks for, not a measurement,
/// and SoC or northbridge rails are other rails.
pub(crate) fn cpu_core_voltage(label: &str) -> bool {
    let words = words(label);
    let has = |word: &str| words.iter().any(|w| w == word);
    if has("vid") || has("soc") || has("nb") {
        return false;
    }
    has("vcore") || (has("core") && (has("cpu") || has("svi2"))) || (has("vddcr") && has("cpu"))
}

/// `part / whole` in percent; unavailable for an empty whole.
pub(crate) fn percent(part: f64, whole: f64, what: &str) -> Reading {
    if whole > 0.0 {
        Reading::Value(part / whole * 100.0)
    } else {
        Reading::Unavailable(format!("{what} is zero"))
    }
}

/// Every catalog entry of `catalog` as unavailable for `reason`.
pub(crate) fn all_unavailable(catalog: &[SensorInfo], reason: &str, out: &mut Snapshot) {
    for info in catalog {
        out.insert(info.key.clone(), Reading::Unavailable(reason.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_fold_everything_but_letters_and_digits() {
        assert_eq!(slug("CPU Package"), "cpu_package");
        assert_eq!(slug("Package id 0"), "package_id_0");
        assert_eq!(slug("  +3.3V  "), "3_3v");
        assert_eq!(slug("/boot/efi"), "boot_efi");
        assert_eq!(slug("+"), "");
    }

    #[test]
    fn describe_and_put_skip_invalid_keys() {
        assert!(describe("a b", Category::Cpu, "x", Quantity::Percent, "y").is_none());
        let info = describe(
            "cpu.usage",
            Category::Cpu,
            "CPU usage",
            Quantity::Percent,
            "s",
        );
        assert_eq!(info.map(|i| i.label), Some("CPU usage".to_string()));
        let mut out = Snapshot::default();
        put(&mut out, "bad key", Reading::Value(1.0));
        put(&mut out, "good.key", Reading::Value(1.0));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn fan_roles_come_from_the_label() {
        for (label, role) in [
            ("CPU Fan", Some(FanRole::Cpu)),
            ("CPU_OPT", Some(FanRole::Cpu)),
            ("cpufan", Some(FanRole::Cpu)),
            ("Processor Fan", Some(FanRole::Cpu)),
            ("AIO Pump", Some(FanRole::Pump)),
            ("W_PUMP+", Some(FanRole::Pump)),
            ("CPU_PUMP", Some(FanRole::Pump)),
            ("Chassis Fan 1", Some(FanRole::Case)),
            ("CHA_FAN2", Some(FanRole::Case)),
            ("SYS_FAN1", Some(FanRole::Case)),
            ("System Fan #3", Some(FanRole::Case)),
            ("Case", Some(FanRole::Case)),
            ("Fan #2", None),
            ("fan1", None),
            ("Water Flow", None),
            ("Chipset", None),
        ] {
            assert_eq!(fan_role(label), role, "{label}");
        }
        assert!(optional_fan("CPU Optional"));
        assert!(optional_fan("CPU_OPT"));
        assert!(!optional_fan("CPU Fan"));
    }

    #[test]
    fn the_cpu_core_rail_is_found_by_label() {
        for label in [
            "Vcore",
            "CPU VCORE",
            "CPU Core",
            "CPU Core Voltage",
            "SVI2_Core",
            "Core (SVI2 TFN)",
            "VDDCR_CPU",
        ] {
            assert!(cpu_core_voltage(label), "{label}");
        }
        for label in [
            "SVI2_SoC",
            "SoC (SVI2 TFN)",
            "Core #1 VID",
            "vddgfx",
            "vddnb",
            "+3.3V",
            "in0",
            "Core",
        ] {
            assert!(!cpu_core_voltage(label), "{label}");
        }
    }

    #[test]
    fn percent_rejects_an_empty_whole() {
        assert_eq!(percent(1.0, 4.0, "total"), Reading::Value(25.0));
        assert_eq!(
            percent(1.0, 0.0, "total"),
            Reading::Unavailable("total is zero".into())
        );
    }

    #[test]
    fn all_unavailable_covers_the_catalog() {
        let catalog: Vec<SensorInfo> = ["a.b", "c.d"]
            .iter()
            .filter_map(|k| describe(k, Category::Board, "l", Quantity::Number, "s"))
            .collect();
        let mut out = Snapshot::default();
        all_unavailable(&catalog, "gone", &mut out);
        assert_eq!(out.len(), 2);
        assert!(
            out.iter()
                .all(|(_, r)| matches!(r, Reading::Unavailable(m) if m == "gone"))
        );
    }
}
