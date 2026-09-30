//! CPU package power from the RAPL energy counter
//! (`/sys/class/powercap/intel-rapl:0/energy_uj`, also on AMD Zen): the
//! energy delta between two samples over the real elapsed time.
//!
//! Since CVE-2020-8694 the counter is readable by root only; the reading then
//! says so instead of guessing. The counter wraps at `max_energy_range_uj`;
//! one wrap between two samples is corrected (at 200 W a wrap takes minutes,
//! far longer than any sampling interval).

use std::io;
use std::path::PathBuf;
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use super::Roots;
use super::fs::{read_error, read_text};
use crate::provider::{Provider, WARMING_UP, describe, put};

const PERMISSION_HINT: &str = "the RAPL energy counter is readable by root only \
     (kernel hardening, CVE-2020-8694); grant read access to \
     /sys/class/powercap/intel-rapl:0/energy_uj to show CPU power";

/// The RAPL provider.
pub(crate) struct Rapl {
    domain: PathBuf,
    previous: Option<(u64, Instant)>,
    catalog: Vec<SensorInfo>,
}

impl Rapl {
    /// Uses the package domain `intel-rapl:0`.
    pub(crate) fn new(roots: &Roots) -> Self {
        Self {
            domain: roots.sys.join("class/powercap/intel-rapl:0"),
            previous: None,
            catalog: describe(
                keys::CPU_POWER,
                Category::Cpu,
                "CPU package power",
                Quantity::Watts,
                "RAPL powercap intel-rapl:0",
            )
            .into_iter()
            .collect(),
        }
    }

    fn read(&mut self, now: Instant) -> Result<f64, String> {
        let path = self.domain.join("energy_uj");
        let energy = match read_text(&path) {
            Ok(text) => text
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("unexpected content in {}", path.display()))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(
                    "no RAPL package domain (powercap intel-rapl:0) on this machine".into(),
                );
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                return Err(PERMISSION_HINT.into());
            }
            Err(e) => return Err(read_error(&path, &e)),
        };
        let Some((before, then)) = self.previous.replace((energy, now)) else {
            return Err(WARMING_UP.into());
        };
        let secs = now.saturating_duration_since(then).as_secs_f64();
        if secs <= 0.0 {
            self.previous = Some((before, then));
            return Err("no time elapsed since the previous sample".into());
        }
        let delta = if energy >= before {
            energy - before
        } else {
            let range = read_text(&self.domain.join("max_energy_range_uj"))
                .ok()
                .and_then(|t| t.trim().parse::<u64>().ok())
                .filter(|range| *range >= before)
                .ok_or("the RAPL counter wrapped and its range is unknown")?;
            range - before + energy
        };
        Ok(delta as f64 / 1_000_000.0 / secs)
    }
}

impl Provider for Rapl {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        let reading = match self.read(now) {
            Ok(watts) => Reading::Value(watts),
            Err(reason) => Reading::Unavailable(reason),
        };
        put(out, keys::CPU_POWER, reading);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    fn power(rapl: &mut Rapl, at: Instant) -> Reading {
        let mut s = Snapshot::default();
        rapl.sample(at, &mut s);
        s.get(&SensorKey::new(keys::CPU_POWER).unwrap())
    }

    fn setup(t: &FakeTree) -> Rapl {
        Rapl::new(&Roots::new(t.path("sys"), t.path("proc")))
    }

    const DOMAIN: &str = "sys/class/powercap/intel-rapl:0";

    #[test]
    fn power_is_energy_over_real_time_and_survives_a_wrap() {
        let t = FakeTree::new("rapl");
        t.file(&format!("{DOMAIN}/energy_uj"), "1000000\n")
            .file(&format!("{DOMAIN}/max_energy_range_uj"), "65532610987\n");
        let mut rapl = setup(&t);
        assert_eq!(rapl.catalog().len(), 1);
        let t0 = Instant::now();
        assert_eq!(
            power(&mut rapl, t0),
            Reading::Unavailable(WARMING_UP.into())
        );
        // 30 J in 0.5 s = 60 W.
        t.file(&format!("{DOMAIN}/energy_uj"), "31000000\n");
        assert_eq!(
            power(&mut rapl, t0 + Duration::from_millis(500)),
            Reading::Value(60.0)
        );
        // Same instant again: no division by zero, baseline kept.
        assert!(matches!(
            power(&mut rapl, t0 + Duration::from_millis(500)),
            Reading::Unavailable(_)
        ));
        // Wrap, with a small range to keep the numbers readable.
        t.file(&format!("{DOMAIN}/max_energy_range_uj"), "40000000\n")
            .file(&format!("{DOMAIN}/energy_uj"), "11000000\n");
        // (40 - 31) + 11 = 20 J in 1 s = 20 W.
        assert_eq!(
            power(&mut rapl, t0 + Duration::from_millis(1500)),
            Reading::Value(20.0)
        );
    }

    #[test]
    fn a_wrap_without_a_known_range_is_not_guessed() {
        let t = FakeTree::new("rapl-norange");
        t.file(&format!("{DOMAIN}/energy_uj"), "5000\n");
        let mut rapl = setup(&t);
        let t0 = Instant::now();
        power(&mut rapl, t0);
        t.file(&format!("{DOMAIN}/energy_uj"), "10\n");
        assert!(
            matches!(power(&mut rapl, t0 + Duration::from_secs(1)), Reading::Unavailable(r) if r.contains("wrapped"))
        );
        t.file(&format!("{DOMAIN}/energy_uj"), "x\n");
        assert!(
            matches!(power(&mut rapl, t0 + Duration::from_secs(2)), Reading::Unavailable(r) if r.starts_with("unexpected"))
        );
    }

    #[test]
    fn missing_and_forbidden_counters_explain_why() {
        let t = FakeTree::new("rapl-missing");
        let mut rapl = setup(&t);
        assert!(
            matches!(power(&mut rapl, Instant::now()), Reading::Unavailable(r) if r.starts_with("no RAPL"))
        );
        t.file(&format!("{DOMAIN}/energy_uj"), "1\n");
        let path = t.path(&format!("{DOMAIN}/energy_uj"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root can read anything; the hint only shows for ordinary users.
        if std::fs::read(&path).is_err() {
            assert_eq!(
                power(&mut rapl, Instant::now()),
                Reading::Unavailable(PERMISSION_HINT.into())
            );
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            matches!(power(&mut rapl, Instant::now()), Reading::Unavailable(r) if r.starts_with("cannot read"))
        );
    }
}
