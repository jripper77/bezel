//! Uptime and host name.

use std::path::PathBuf;
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use super::Roots;
use super::fs::{read_error, read_text};
use crate::provider::{Provider, describe, put};

/// The machine's host name.
pub(crate) const HOSTNAME: &str = "system.hostname";

/// The system provider.
pub(crate) struct System {
    uptime: PathBuf,
    hostname: PathBuf,
    catalog: Vec<SensorInfo>,
}

impl System {
    /// Reads `<proc>/uptime` and `<proc>/sys/kernel/hostname`.
    pub(crate) fn new(roots: &Roots) -> Self {
        let s = Category::System;
        Self {
            uptime: roots.proc.join("uptime"),
            hostname: roots.proc.join("sys/kernel/hostname"),
            catalog: [
                describe(keys::UPTIME, s, "Uptime", Quantity::Seconds, "/proc/uptime"),
                describe(
                    HOSTNAME,
                    s,
                    "Host name",
                    Quantity::Text,
                    "/proc/sys/kernel/hostname",
                ),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }
    }

    fn uptime(&self) -> Reading {
        let text = match read_text(&self.uptime) {
            Ok(text) => text,
            Err(e) => return Reading::Unavailable(read_error(&self.uptime, &e)),
        };
        match text.split_whitespace().next().map(str::parse::<f64>) {
            Some(Ok(secs)) => Reading::Value(secs),
            _ => Reading::Unavailable(format!("unexpected content in {}", self.uptime.display())),
        }
    }
}

impl Provider for System {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        put(out, keys::UPTIME, self.uptime());
        let host = match read_text(&self.hostname) {
            Ok(name) => Reading::Text(name.trim().to_string()),
            Err(e) => Reading::Unavailable(read_error(&self.hostname, &e)),
        };
        put(out, HOSTNAME, host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;

    #[test]
    fn uptime_and_host_name() {
        let t = FakeTree::new("system");
        t.file("proc/uptime", "6900.09 156461.31\n")
            .file("proc/sys/kernel/hostname", "fedorakde\n");
        let mut sys = System::new(&Roots::new(t.path("sys"), t.path("proc")));
        assert_eq!(sys.catalog().len(), 2);
        let mut s = Snapshot::default();
        sys.sample(Instant::now(), &mut s);
        let get = |s: &Snapshot, k: &str| s.get(&SensorKey::new(k).unwrap());
        assert_eq!(get(&s, "system.uptime"), Reading::Value(6900.09));
        assert_eq!(
            get(&s, "system.hostname"),
            Reading::Text("fedorakde".into())
        );
        t.file("proc/uptime", "soon\n");
        std::fs::remove_file(t.path("proc/sys/kernel/hostname")).unwrap();
        let mut s = Snapshot::default();
        sys.sample(Instant::now(), &mut s);
        assert!(
            matches!(get(&s, "system.uptime"), Reading::Unavailable(r) if r.starts_with("unexpected"))
        );
        assert!(matches!(
            get(&s, "system.hostname"),
            Reading::Unavailable(_)
        ));
        std::fs::remove_file(t.path("proc/uptime")).unwrap();
        let mut s = Snapshot::default();
        sys.sample(Instant::now(), &mut s);
        assert!(
            matches!(get(&s, "system.uptime"), Reading::Unavailable(r) if r.contains("does not exist"))
        );
    }
}
