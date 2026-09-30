//! CPU usage (total and per logical CPU) from `/proc/stat` deltas, current
//! frequency from cpufreq, load average and the model name.
//!
//! Usage needs two samples: the first one reads "warming up" instead of an
//! average since boot, which would look like a live value but is not one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys};

use super::Roots;
use super::fs::{read_error, read_text};
use crate::provider::{Provider, WARMING_UP, describe, put};

const TOO_SOON: &str = "less than one scheduler tick since the previous sample";

/// Jiffies of one `/proc/stat` line: user, nice, system, idle, iowait, irq,
/// softirq, steal. Guest time is already part of user/nice and is not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Times([u64; 8]);

const IDLE: usize = 3;
const IOWAIT: usize = 4;

/// Busy percent between two readings of the same CPU. Each field's delta is
/// clamped at zero: iowait is known to step backwards on NO_HZ kernels.
/// `None` when no time elapsed for that CPU.
fn usage(previous: &Times, current: &Times) -> Option<f64> {
    let delta: Vec<u64> = previous
        .0
        .iter()
        .zip(current.0.iter())
        .map(|(p, c)| c.saturating_sub(*p))
        .collect();
    let total: u64 = delta.iter().sum();
    if total == 0 {
        return None;
    }
    let idle = delta[IDLE] + delta[IOWAIT];
    Some((total - idle) as f64 * 100.0 / total as f64)
}

/// `(None, total)` for the `cpu` line and `(Some(n), times)` for each `cpuN`.
fn parse_stat(text: &str) -> Vec<(Option<usize>, Times)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?.strip_prefix("cpu")?;
            let id = if name.is_empty() {
                None
            } else {
                Some(name.parse::<usize>().ok()?)
            };
            let mut times = Times::default();
            for slot in times.0.iter_mut() {
                // Old kernels lack steal: missing trailing fields stay zero.
                *slot = fields.next().map_or(Ok(0), str::parse).ok()?;
            }
            Some((id, times))
        })
        .collect()
}

/// `model name` of the first processor in `/proc/cpuinfo`.
fn model_name(cpuinfo: &str) -> Option<String> {
    cpuinfo.lines().find_map(|line| {
        let (field, value) = line.split_once(':')?;
        (field.trim() == "model name").then(|| value.trim().to_string())
    })
}

/// The CPU provider.
pub(crate) struct Cpu {
    stat: PathBuf,
    loadavg: PathBuf,
    cpufreq: PathBuf,
    cores: Vec<usize>,
    name: Option<String>,
    previous: HashMap<Option<usize>, Times>,
    catalog: Vec<SensorInfo>,
}

impl Cpu {
    /// Discovers the online CPUs listed in `/proc/stat`.
    pub(crate) fn new(roots: &Roots) -> Self {
        let stat = roots.proc.join("stat");
        let cores: Vec<usize> = read_text(&stat)
            .map(|t| {
                parse_stat(&t)
                    .into_iter()
                    .filter_map(|(id, _)| id)
                    .collect()
            })
            .unwrap_or_default();
        let name = read_text(&roots.proc.join("cpuinfo"))
            .ok()
            .and_then(|t| model_name(&t));
        let mut cpu = Self {
            stat,
            loadavg: roots.proc.join("loadavg"),
            cpufreq: roots.sys.join("devices/system/cpu"),
            cores,
            name,
            previous: HashMap::new(),
            catalog: Vec::new(),
        };
        cpu.catalog = cpu.describe_all();
        cpu
    }

    fn describe_all(&self) -> Vec<SensorInfo> {
        let freq = "cpufreq scaling_cur_freq";
        let mut all = vec![
            describe(
                keys::CPU_USAGE,
                Category::Cpu,
                "CPU usage",
                Quantity::Percent,
                "/proc/stat",
            ),
            describe(
                keys::CPU_FREQUENCY,
                Category::Cpu,
                "CPU frequency (average)",
                Quantity::Megahertz,
                freq,
            ),
            describe(
                keys::CPU_LOAD_1,
                Category::Cpu,
                "Load average (1 min)",
                Quantity::Number,
                "/proc/loadavg",
            ),
            describe(
                keys::CPU_LOAD_5,
                Category::Cpu,
                "Load average (5 min)",
                Quantity::Number,
                "/proc/loadavg",
            ),
            describe(
                keys::CPU_LOAD_15,
                Category::Cpu,
                "Load average (15 min)",
                Quantity::Number,
                "/proc/loadavg",
            ),
            describe(
                keys::CPU_NAME,
                Category::Cpu,
                "CPU model",
                Quantity::Text,
                "/proc/cpuinfo",
            ),
        ];
        for n in &self.cores {
            let key = format!("cpu.{n}.usage");
            all.push(describe(
                &key,
                Category::Cpu,
                format!("CPU {n} usage"),
                Quantity::Percent,
                "/proc/stat",
            ));
        }
        for n in &self.cores {
            let key = format!("cpu.{n}.frequency");
            all.push(describe(
                &key,
                Category::Cpu,
                format!("CPU {n} frequency"),
                Quantity::Megahertz,
                freq,
            ));
        }
        all.into_iter().flatten().collect()
    }

    fn usage_key(id: Option<usize>) -> String {
        id.map_or_else(|| keys::CPU_USAGE.to_string(), |n| format!("cpu.{n}.usage"))
    }

    fn sample_usage(&mut self, out: &mut Snapshot) {
        let text = match read_text(&self.stat) {
            Ok(text) => text,
            Err(e) => {
                let reason = read_error(&self.stat, &e);
                put(out, keys::CPU_USAGE, Reading::Unavailable(reason.clone()));
                for n in &self.cores {
                    put(
                        out,
                        &Self::usage_key(Some(*n)),
                        Reading::Unavailable(reason.clone()),
                    );
                }
                return;
            }
        };
        let current = parse_stat(&text);
        for (id, times) in &current {
            let reading = match self.previous.get(id) {
                None => Reading::Unavailable(WARMING_UP.into()),
                Some(previous) => match usage(previous, times) {
                    Some(busy) => Reading::Value(busy),
                    // Keep the older baseline so the next delta spans the whole gap.
                    None => {
                        put(
                            out,
                            &Self::usage_key(*id),
                            Reading::Unavailable(TOO_SOON.into()),
                        );
                        continue;
                    }
                },
            };
            self.previous.insert(*id, *times);
            put(out, &Self::usage_key(*id), reading);
        }
        for n in &self.cores {
            if !current.iter().any(|(id, _)| *id == Some(*n)) {
                self.previous.remove(&Some(*n));
                put(
                    out,
                    &Self::usage_key(Some(*n)),
                    Reading::Unavailable(format!("CPU {n} is offline")),
                );
            }
        }
    }

    fn sample_frequency(&self, out: &mut Snapshot) {
        let mut sum = 0.0;
        let mut count = 0u32;
        let mut last_error = None;
        for n in &self.cores {
            let path = self
                .cpufreq
                .join(format!("cpu{n}/cpufreq/scaling_cur_freq"));
            let reading = super::fs::read_scaled(&path, 1000.0);
            match &reading {
                Reading::Value(mhz) => {
                    sum += mhz;
                    count += 1;
                }
                Reading::Unavailable(reason) => last_error = Some(reason.clone()),
                Reading::Text(_) => {}
            }
            put(out, &format!("cpu.{n}.frequency"), reading);
        }
        let average = if count > 0 {
            Reading::Value(sum / f64::from(count))
        } else {
            Reading::Unavailable(
                last_error.unwrap_or_else(|| "no cpufreq driver on this machine".into()),
            )
        };
        put(out, keys::CPU_FREQUENCY, average);
    }

    fn sample_load(&self, out: &mut Snapshot) {
        let loads = read_text(&self.loadavg)
            .map_err(|e| read_error(&self.loadavg, &e))
            .and_then(|t| {
                let v: Vec<f64> = t
                    .split_whitespace()
                    .take(3)
                    .filter_map(|f| f.parse().ok())
                    .collect();
                (v.len() == 3)
                    .then_some(v)
                    .ok_or_else(|| format!("unexpected content in {}", self.loadavg.display()))
            });
        for (i, key) in [keys::CPU_LOAD_1, keys::CPU_LOAD_5, keys::CPU_LOAD_15]
            .iter()
            .enumerate()
        {
            let reading = match &loads {
                Ok(v) => Reading::Value(v[i]),
                Err(reason) => Reading::Unavailable(reason.clone()),
            };
            put(out, key, reading);
        }
    }
}

impl Provider for Cpu {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        self.sample_usage(out);
        self.sample_frequency(out);
        self.sample_load(out);
        let name = match &self.name {
            Some(name) => Reading::Text(name.clone()),
            None => Reading::Unavailable("no model name in /proc/cpuinfo".into()),
        };
        put(out, keys::CPU_NAME, name);
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

    fn tree() -> FakeTree {
        let t = FakeTree::new("cpu");
        t.file(
            "proc/stat",
            "cpu  100 0 100 800 0 0 0 0 0 0\n\
             cpu0 50 0 50 400 0 0 0 0 0 0\n\
             cpu1 50 0 50 400 0 0 0 0 0 0\n\
             intr 1 2 3\n",
        )
        .file(
            "proc/cpuinfo",
            "processor\t: 0\nmodel name\t: AMD Ryzen 9 7900X3D 12-Core Processor\n",
        )
        .file("proc/loadavg", "1.49 1.64 1.92 2/2393 252525\n")
        .file(
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
            "4200000\n",
        )
        .file(
            "sys/devices/system/cpu/cpu1/cpufreq/scaling_cur_freq",
            "3000000\n",
        );
        t
    }

    fn roots(t: &FakeTree) -> Roots {
        Roots::new(t.path("sys"), t.path("proc"))
    }

    #[test]
    fn usage_from_proc_stat_deltas() {
        let t = tree();
        let mut cpu = Cpu::new(&roots(&t));
        let now = Instant::now();
        let mut first = Snapshot::default();
        cpu.sample(now, &mut first);
        assert_eq!(
            get(&first, "cpu.usage"),
            Reading::Unavailable(WARMING_UP.into())
        );
        assert_eq!(
            get(&first, "cpu.0.usage"),
            Reading::Unavailable(WARMING_UP.into())
        );

        // cpu0: +300 busy (user 100, system 100, irq 50, steal 50) against
        // +100 idle and +100 iowait => 60 %. cpu1: +200 busy, +300 idle =>
        // 40 %. Total: +500 busy of +1000 => 50 %. The trailing guest fields
        // are already inside user/nice and must not count twice.
        t.file(
            "proc/stat",
            "cpu  300 0 300 1200 100 50 0 50 999 999\n\
             cpu0 150 0 150 500 100 50 0 50 999 0\n\
             cpu1 150 0 150 700 0 0 0 0 0 999\n",
        );
        let mut second = Snapshot::default();
        cpu.sample(now + std::time::Duration::from_millis(250), &mut second);
        assert_eq!(get(&second, "cpu.usage"), Reading::Value(50.0));
        assert_eq!(get(&second, "cpu.0.usage"), Reading::Value(60.0));
        assert_eq!(get(&second, "cpu.1.usage"), Reading::Value(40.0));
    }

    #[test]
    fn a_sample_without_new_ticks_keeps_the_older_baseline() {
        let t = tree();
        let mut cpu = Cpu::new(&roots(&t));
        let now = Instant::now();
        cpu.sample(now, &mut Snapshot::default());
        let mut same = Snapshot::default();
        cpu.sample(now, &mut same);
        assert_eq!(
            get(&same, "cpu.usage"),
            Reading::Unavailable(TOO_SOON.into())
        );
        t.file(
            "proc/stat",
            "cpu  150 0 100 850 0 0 0 0 0 0\ncpu0 100 0 50 450 0 0 0 0 0 0\n",
        );
        let mut later = Snapshot::default();
        cpu.sample(now, &mut later);
        assert_eq!(get(&later, "cpu.usage"), Reading::Value(50.0));
        assert_eq!(
            get(&later, "cpu.1.usage"),
            Reading::Unavailable("CPU 1 is offline".into())
        );
    }

    #[test]
    fn iowait_stepping_back_does_not_inflate_usage() {
        let before = Times([10, 0, 10, 100, 50, 0, 0, 0]);
        let after = Times([20, 0, 20, 180, 40, 0, 0, 0]);
        assert_eq!(usage(&before, &after), Some(20.0));
        assert_eq!(usage(&after, &after), None);
    }

    #[test]
    fn frequency_load_and_name() {
        let t = tree();
        let mut cpu = Cpu::new(&roots(&t));
        let mut s = Snapshot::default();
        cpu.sample(Instant::now(), &mut s);
        assert_eq!(get(&s, "cpu.frequency"), Reading::Value(3600.0));
        assert_eq!(get(&s, "cpu.0.frequency"), Reading::Value(4200.0));
        assert_eq!(get(&s, "cpu.load.1"), Reading::Value(1.49));
        assert_eq!(get(&s, "cpu.load.15"), Reading::Value(1.92));
        assert_eq!(
            get(&s, "cpu.name"),
            Reading::Text("AMD Ryzen 9 7900X3D 12-Core Processor".into())
        );
        let keys: Vec<String> = cpu.catalog().iter().map(|i| i.key.to_string()).collect();
        assert_eq!(keys.len(), 6 + 2 * 2);
        for (key, _) in s.iter() {
            assert!(keys.contains(&key.to_string()), "{key} not in catalog");
        }
        assert_eq!(s.len(), keys.len());
    }

    #[test]
    fn missing_files_are_explained() {
        let t = FakeTree::new("cpu-empty");
        t.file("proc/stat", "cpu  1 0 1 1 0 0 0 0\ncpu0 1 0 1 1 0 0 0 0\n");
        t.file("proc/loadavg", "garbage\n");
        let mut cpu = Cpu::new(&roots(&t));
        std::fs::remove_file(t.path("proc/stat")).unwrap();
        let mut s = Snapshot::default();
        cpu.sample(Instant::now(), &mut s);
        assert!(
            matches!(get(&s, "cpu.usage"), Reading::Unavailable(r) if r.contains("does not exist"))
        );
        assert!(matches!(get(&s, "cpu.0.usage"), Reading::Unavailable(_)));
        assert!(
            matches!(get(&s, "cpu.frequency"), Reading::Unavailable(r) if r.contains("scaling_cur_freq"))
        );
        assert!(
            matches!(get(&s, "cpu.load.5"), Reading::Unavailable(r) if r.starts_with("unexpected"))
        );
        assert!(matches!(get(&s, "cpu.name"), Reading::Unavailable(_)));
        assert_eq!(parse_stat("cpux 1 2\ncpu0 a b\n"), vec![]);
        assert_eq!(model_name("processor : 0\n"), None);
    }
}
