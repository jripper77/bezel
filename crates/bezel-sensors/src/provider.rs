//! What every internal provider implements, and the helpers they share to
//! describe sensors and store readings.

use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, SensorKey, Snapshot};

/// Reason shown for a rate or usage before its second sample.
pub(crate) const WARMING_UP: &str = "warming up: needs a second sample";

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
