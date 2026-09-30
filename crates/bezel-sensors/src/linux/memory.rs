//! RAM and swap from `/proc/meminfo`. "Used" is `MemTotal - MemAvailable`,
//! the memory programs cannot get back without swapping (what `free` shows
//! as used since procps-ng 4.0.1); page cache is not counted as used.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use super::Roots;
use super::fs::{read_error, read_text};
use crate::provider::{Provider, describe, percent, put};

const ALL: [&str; 8] = [
    keys::MEMORY_USED,
    keys::MEMORY_TOTAL,
    keys::MEMORY_PERCENT,
    keys::MEMORY_AVAILABLE,
    keys::MEMORY_AVAILABLE_PERCENT,
    keys::SWAP_USED,
    keys::SWAP_TOTAL,
    keys::SWAP_PERCENT,
];

/// `field → bytes` for every `Name: <n> kB` line.
fn parse(text: &str) -> HashMap<&str, u64> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let mut parts = rest.split_whitespace();
            let value: u64 = parts.next()?.parse().ok()?;
            let bytes = match parts.next() {
                Some("kB") => value.checked_mul(1024)?,
                None => value,
                Some(_) => return None,
            };
            Some((name.trim(), bytes))
        })
        .collect()
}

fn readings(info: &HashMap<&str, u64>) -> Vec<(&'static str, Reading)> {
    let field = |name: &str| {
        info.get(name)
            .map(|v| *v as f64)
            .ok_or_else(|| format!("{name} missing from /proc/meminfo"))
    };
    let value = |r: Result<f64, String>| r.map_or_else(Reading::Unavailable, Reading::Value);
    let total = field("MemTotal");
    let available = field("MemAvailable");
    let used = total
        .clone()
        .and_then(|t| available.clone().map(|a| (t - a).max(0.0)));
    let of_total = |part: &Result<f64, String>| match (part, &total) {
        (Ok(p), Ok(t)) => percent(*p, *t, "MemTotal"),
        (Err(e), _) | (_, Err(e)) => Reading::Unavailable(e.clone()),
    };
    let (ram_percent, available_percent) = (of_total(&used), of_total(&available));
    let swap_total = field("SwapTotal");
    let swap_used = swap_total
        .clone()
        .and_then(|t| field("SwapFree").map(|f| (t - f).max(0.0)));
    let swap_percent = match (&swap_used, &swap_total) {
        (Ok(_), Ok(t)) if *t == 0.0 => Reading::Unavailable("no swap configured".into()),
        (Ok(u), Ok(t)) => percent(*u, *t, "SwapTotal"),
        (Err(e), _) | (_, Err(e)) => Reading::Unavailable(e.clone()),
    };
    vec![
        (keys::MEMORY_USED, value(used)),
        (keys::MEMORY_TOTAL, value(total)),
        (keys::MEMORY_PERCENT, ram_percent),
        (keys::MEMORY_AVAILABLE, value(available)),
        (keys::MEMORY_AVAILABLE_PERCENT, available_percent),
        (keys::SWAP_USED, value(swap_used)),
        (keys::SWAP_TOTAL, value(swap_total)),
        (keys::SWAP_PERCENT, swap_percent),
    ]
}

/// The memory provider.
pub(crate) struct Memory {
    meminfo: PathBuf,
    catalog: Vec<SensorInfo>,
}

impl Memory {
    /// Reads `<proc>/meminfo`.
    pub(crate) fn new(roots: &Roots) -> Self {
        let src = "/proc/meminfo";
        let m = Category::Memory;
        let catalog = [
            describe(keys::MEMORY_USED, m, "RAM used", Quantity::Bytes, src),
            describe(keys::MEMORY_TOTAL, m, "RAM total", Quantity::Bytes, src),
            describe(
                keys::MEMORY_PERCENT,
                m,
                "RAM used (percent)",
                Quantity::Percent,
                src,
            ),
            describe(
                keys::MEMORY_AVAILABLE,
                m,
                "RAM available",
                Quantity::Bytes,
                src,
            ),
            describe(
                keys::MEMORY_AVAILABLE_PERCENT,
                m,
                "RAM available (percent)",
                Quantity::Percent,
                src,
            ),
            describe(keys::SWAP_USED, m, "Swap used", Quantity::Bytes, src),
            describe(keys::SWAP_TOTAL, m, "Swap total", Quantity::Bytes, src),
            describe(
                keys::SWAP_PERCENT,
                m,
                "Swap used (percent)",
                Quantity::Percent,
                src,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            meminfo: roots.proc.join("meminfo"),
            catalog,
        }
    }
}

impl Provider for Memory {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        match read_text(&self.meminfo) {
            Ok(text) => {
                for (key, reading) in readings(&parse(&text)) {
                    put(out, key, reading);
                }
            }
            Err(e) => {
                let reason = read_error(&self.meminfo, &e);
                for key in ALL {
                    put(out, key, Reading::Unavailable(reason.clone()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;

    fn sample(t: &FakeTree) -> Snapshot {
        let mut m = Memory::new(&Roots::new(t.path("sys"), t.path("proc")));
        assert_eq!(m.catalog().len(), ALL.len());
        let mut s = Snapshot::default();
        m.sample(Instant::now(), &mut s);
        s
    }

    fn get(s: &Snapshot, key: &str) -> Reading {
        s.get(&SensorKey::new(key).unwrap())
    }

    #[test]
    fn used_is_total_minus_available() {
        let t = FakeTree::new("mem");
        t.file(
            "proc/meminfo",
            "MemTotal:       64948576 kB\nMemFree:        14963996 kB\n\
             MemAvailable:   53091044 kB\nSwapTotal:       8388604 kB\n\
             SwapFree:        6291452 kB\nHugePages_Total:       0\n",
        );
        let s = sample(&t);
        assert_eq!(get(&s, "memory.total"), Reading::Value(66_507_341_824.0));
        assert_eq!(
            get(&s, "memory.available"),
            Reading::Value(54_365_229_056.0)
        );
        assert_eq!(get(&s, "memory.used"), Reading::Value(12_142_112_768.0));
        let pct = get(&s, "memory.percent").value().unwrap();
        assert!((pct - 18.2567).abs() < 1e-3, "{pct}");
        // `free`'s "available" column over its total.
        let available = get(&s, keys::MEMORY_AVAILABLE_PERCENT).value().unwrap();
        assert!((available - 81.7433).abs() < 1e-3, "{available}");
        assert!((pct + available - 100.0).abs() < 1e-9);
        assert_eq!(get(&s, "memory.swap.used"), Reading::Value(2_147_483_648.0));
        let swap = get(&s, "memory.swap.percent").value().unwrap();
        assert!((swap - 25.0).abs() < 1e-3, "{swap}");
    }

    #[test]
    fn missing_fields_and_no_swap() {
        let t = FakeTree::new("mem-old");
        t.file(
            "proc/meminfo",
            "MemTotal: 1000 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\nWeird: 1 MB\n",
        );
        let s = sample(&t);
        assert!(
            matches!(get(&s, "memory.used"), Reading::Unavailable(r) if r.starts_with("MemAvailable"))
        );
        assert!(matches!(get(&s, "memory.percent"), Reading::Unavailable(_)));
        assert!(
            matches!(get(&s, keys::MEMORY_AVAILABLE_PERCENT), Reading::Unavailable(r) if r.starts_with("MemAvailable"))
        );
        assert_eq!(
            get(&s, "memory.swap.percent"),
            Reading::Unavailable("no swap configured".into())
        );
        let gone = FakeTree::new("mem-gone");
        let s = sample(&gone);
        assert!(
            matches!(get(&s, "memory.total"), Reading::Unavailable(r) if r.contains("meminfo"))
        );
        assert_eq!(s.len(), ALL.len());
    }
}
