//! sysinfo-backed Windows providers: CPU usage and clocks, memory and page
//! file, volumes, network interfaces, uptime and host name. Rates come from
//! the cumulative counters over the real time between samples, like on
//! Linux.

use std::collections::HashMap;
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys, rate};
use sysinfo::{
    DiskRefreshKind, Disks as SysDisks, MINIMUM_CPU_UPDATE_INTERVAL, Networks, System as SysSystem,
};

use crate::provider::{Provider, WARMING_UP, describe, percent, put, slug};

/// Why a CPU clock is missing (sysinfo returns 0 when the query failed).
const NO_CLOCK: &str = "Windows did not report the CPU clock";

const TOO_SOON: &str = "sampled again too soon for Windows to measure CPU usage";

/// CPU usage and clocks.
pub(crate) struct Cpu {
    sys: SysSystem,
    last: Instant,
    cores: usize,
    catalog: Vec<SensorInfo>,
}

impl Cpu {
    pub(crate) fn new() -> Self {
        let mut sys = SysSystem::new();
        sys.refresh_cpu_all();
        let cores = sys.cpus().len();
        let c = Category::Cpu;
        let src = "sysinfo";
        let mut catalog: Vec<Option<SensorInfo>> = vec![
            describe(keys::CPU_USAGE, c, "CPU usage", Quantity::Percent, src),
            describe(
                keys::CPU_FREQUENCY,
                c,
                "CPU frequency (average)",
                Quantity::Megahertz,
                src,
            ),
            describe(
                keys::CPU_LOAD_1,
                c,
                "Load average (1 min)",
                Quantity::Number,
                src,
            ),
            describe("cpu.name", c, "CPU model", Quantity::Text, src),
        ];
        for n in 0..cores {
            catalog.push(describe(
                &format!("cpu.{n}.usage"),
                c,
                format!("CPU {n} usage"),
                Quantity::Percent,
                src,
            ));
            catalog.push(describe(
                &format!("cpu.{n}.frequency"),
                c,
                format!("CPU {n} frequency"),
                Quantity::Megahertz,
                src,
            ));
        }
        Self {
            sys,
            last: Instant::now(),
            cores,
            catalog: catalog.into_iter().flatten().collect(),
        }
    }
}

impl Provider for Cpu {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        let too_soon = now.saturating_duration_since(self.last) < MINIMUM_CPU_UPDATE_INTERVAL;
        self.sys.refresh_cpu_usage();
        self.sys.refresh_cpu_frequency();
        self.last = now;
        let usage = |v: f32| {
            if too_soon {
                Reading::Unavailable(TOO_SOON.into())
            } else {
                Reading::Value(f64::from(v))
            }
        };
        put(out, keys::CPU_USAGE, usage(self.sys.global_cpu_usage()));
        let cpus = self.sys.cpus();
        let mut total = 0.0;
        let mut clocked = 0usize;
        for (n, cpu) in cpus.iter().enumerate().take(self.cores) {
            put(out, &format!("cpu.{n}.usage"), usage(cpu.cpu_usage()));
            // sysinfo reports 0 when the Windows query failed: never a clock.
            let reading = match cpu.frequency() {
                0 => Reading::Unavailable(NO_CLOCK.into()),
                mhz => {
                    total += mhz as f64;
                    clocked += 1;
                    Reading::Value(mhz as f64)
                }
            };
            put(out, &format!("cpu.{n}.frequency"), reading);
        }
        let average = if cpus.is_empty() {
            Reading::Unavailable("Windows reported no CPU".into())
        } else if clocked == 0 {
            Reading::Unavailable(NO_CLOCK.into())
        } else {
            Reading::Value(total / clocked as f64)
        };
        put(out, keys::CPU_FREQUENCY, average);
        put(
            out,
            keys::CPU_LOAD_1,
            Reading::Unavailable("Windows has no load average".into()),
        );
        let name = cpus
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|b| !b.is_empty());
        put(
            out,
            "cpu.name",
            name.map_or_else(
                || Reading::Unavailable("no CPU brand".into()),
                Reading::Text,
            ),
        );
    }
}

/// RAM and page file.
pub(crate) struct Memory {
    sys: SysSystem,
    catalog: Vec<SensorInfo>,
}

impl Memory {
    pub(crate) fn new() -> Self {
        let m = Category::Memory;
        let src = "sysinfo";
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
            describe("memory.available", m, "RAM available", Quantity::Bytes, src),
            describe(
                "memory.swap.used",
                m,
                "Page file used",
                Quantity::Bytes,
                src,
            ),
            describe(
                "memory.swap.total",
                m,
                "Page file total",
                Quantity::Bytes,
                src,
            ),
            describe(
                "memory.swap.percent",
                m,
                "Page file used (percent)",
                Quantity::Percent,
                src,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            sys: SysSystem::new(),
            catalog,
        }
    }
}

impl Provider for Memory {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        self.sys.refresh_memory();
        let total = self.sys.total_memory() as f64;
        let available = self.sys.available_memory() as f64;
        let used = (total - available).max(0.0);
        put(out, keys::MEMORY_USED, Reading::Value(used));
        put(out, keys::MEMORY_TOTAL, Reading::Value(total));
        put(out, keys::MEMORY_PERCENT, percent(used, total, "RAM total"));
        put(out, "memory.available", Reading::Value(available));
        let swap_total = self.sys.total_swap() as f64;
        let swap_used = self.sys.used_swap() as f64;
        put(out, "memory.swap.used", Reading::Value(swap_used));
        put(out, "memory.swap.total", Reading::Value(swap_total));
        put(
            out,
            "memory.swap.percent",
            percent(swap_used, swap_total, "page file"),
        );
    }
}

/// Volumes: space per drive and summed throughput.
pub(crate) struct Disks {
    disks: SysDisks,
    ids: Vec<(String, String)>,
    previous: Option<(u64, u64, Instant)>,
    catalog: Vec<SensorInfo>,
}

/// `C:\` → `c`.
fn volume_id(mount: &str) -> String {
    match slug(mount) {
        s if s.is_empty() => "root".into(),
        s => s,
    }
}

impl Disks {
    pub(crate) fn new() -> Self {
        let disks = SysDisks::new_with_refreshed_list();
        let d = Category::Disk;
        let mut catalog = vec![
            describe(
                keys::DISK_READ,
                d,
                "Disk read rate",
                Quantity::BytesPerSecond,
                "sysinfo",
            ),
            describe(
                keys::DISK_WRITE,
                d,
                "Disk write rate",
                Quantity::BytesPerSecond,
                "sysinfo",
            ),
        ];
        let mut ids: Vec<(String, String)> = Vec::new();
        for disk in disks.list() {
            let mount = disk.mount_point().display().to_string();
            let id = volume_id(&mount);
            if ids.iter().any(|(i, _)| *i == id) {
                continue;
            }
            let src = format!("sysinfo {mount}");
            catalog.push(describe(
                &format!("disk.{id}.used"),
                d,
                format!("{mount} used"),
                Quantity::Bytes,
                &src,
            ));
            catalog.push(describe(
                &format!("disk.{id}.total"),
                d,
                format!("{mount} size"),
                Quantity::Bytes,
                &src,
            ));
            catalog.push(describe(
                &format!("disk.{id}.free"),
                d,
                format!("{mount} free"),
                Quantity::Bytes,
                &src,
            ));
            catalog.push(describe(
                &format!("disk.{id}.percent"),
                d,
                format!("{mount} used (percent)"),
                Quantity::Percent,
                src,
            ));
            ids.push((id, mount));
        }
        Self {
            disks,
            ids,
            previous: None,
            catalog: catalog.into_iter().flatten().collect(),
        }
    }
}

impl Provider for Disks {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        self.disks.refresh_specifics(
            false,
            DiskRefreshKind::nothing().with_storage().with_io_usage(),
        );
        let (mut read, mut written) = (0u64, 0u64);
        let mut seen = HashMap::new();
        for disk in self.disks.list() {
            let usage = disk.usage();
            read = read.saturating_add(usage.total_read_bytes);
            written = written.saturating_add(usage.total_written_bytes);
            seen.insert(
                disk.mount_point().display().to_string(),
                (disk.total_space(), disk.available_space()),
            );
        }
        for (id, mount) in &self.ids {
            let Some(&(total, available)) = seen.get(mount) else {
                for s in ["used", "total", "free", "percent"] {
                    put(
                        out,
                        &format!("disk.{id}.{s}"),
                        Reading::Unavailable(format!("{mount} is gone")),
                    );
                }
                continue;
            };
            let used = total.saturating_sub(available) as f64;
            put(out, &format!("disk.{id}.used"), Reading::Value(used));
            put(
                out,
                &format!("disk.{id}.total"),
                Reading::Value(total as f64),
            );
            put(
                out,
                &format!("disk.{id}.free"),
                Reading::Value(available as f64),
            );
            put(
                out,
                &format!("disk.{id}.percent"),
                percent(used, total as f64, "volume size"),
            );
        }
        let rates = self.previous.map(|(r, w, then)| {
            let elapsed = now.saturating_duration_since(then);
            (rate(r, read, elapsed), rate(w, written, elapsed))
        });
        let value = |v: Option<Option<f64>>| match v {
            None => Reading::Unavailable(WARMING_UP.into()),
            Some(Some(v)) => Reading::Value(v),
            Some(None) => Reading::Unavailable("counter reset or no time elapsed".into()),
        };
        put(out, keys::DISK_READ, value(rates.map(|r| r.0)));
        put(out, keys::DISK_WRITE, value(rates.map(|r| r.1)));
        self.previous = Some((read, written, now));
    }
}

/// Name fragments of virtual adapters, left out of `net.down`/`net.up`.
const VIRTUAL: [&str; 10] = [
    "loopback",
    "vethernet",
    "virtualbox",
    "vmware",
    "hyper-v",
    "tailscale",
    "zerotier",
    "wireguard",
    "tap-",
    "bluetooth",
];

fn physical(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !VIRTUAL.iter().any(|v| lower.contains(v))
}

/// Received and sent bytes per interface.
type Counters = HashMap<String, (u64, u64)>;

/// Network interfaces.
pub(crate) struct Network {
    networks: Networks,
    listed: Vec<(String, String)>,
    previous: Option<(Counters, Instant)>,
    catalog: Vec<SensorInfo>,
}

impl Network {
    pub(crate) fn new() -> Self {
        let networks = Networks::new_with_refreshed_list();
        let n = Category::Network;
        let mut names: Vec<String> = networks.list().keys().cloned().collect();
        names.sort();
        let mut catalog = vec![
            describe(
                keys::NET_DOWN,
                n,
                "Download rate",
                Quantity::BytesPerSecond,
                "sysinfo",
            ),
            describe(
                keys::NET_UP,
                n,
                "Upload rate",
                Quantity::BytesPerSecond,
                "sysinfo",
            ),
        ];
        let mut listed = Vec::new();
        for name in names {
            let id = slug(&name);
            if id.is_empty() || listed.iter().any(|(i, _)| *i == id) {
                continue;
            }
            catalog.push(describe(
                &format!("net.{id}.down"),
                n,
                format!("{name} download rate"),
                Quantity::BytesPerSecond,
                "sysinfo",
            ));
            catalog.push(describe(
                &format!("net.{id}.up"),
                n,
                format!("{name} upload rate"),
                Quantity::BytesPerSecond,
                "sysinfo",
            ));
            listed.push((id, name));
        }
        Self {
            networks,
            listed,
            previous: None,
            catalog: catalog.into_iter().flatten().collect(),
        }
    }
}

impl Provider for Network {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        self.networks.refresh(true);
        let current: Counters = self
            .networks
            .list()
            .iter()
            .map(|(name, data)| {
                (
                    name.clone(),
                    (data.total_received(), data.total_transmitted()),
                )
            })
            .collect();
        let previous = self.previous.take();
        let rates = |name: &str| -> Option<(Option<f64>, Option<f64>)> {
            let (before, then) = previous.as_ref()?;
            let (b, c) = (before.get(name)?, current.get(name)?);
            let elapsed = now.saturating_duration_since(*then);
            Some((rate(b.0, c.0, elapsed), rate(b.1, c.1, elapsed)))
        };
        let reading = |v: Option<f64>| {
            v.map_or_else(
                || Reading::Unavailable("counter reset or no time elapsed".into()),
                Reading::Value,
            )
        };
        for (id, name) in &self.listed {
            let (down, up) = match rates(name) {
                Some((d, u)) => (reading(d), reading(u)),
                None if previous.is_none() => (
                    Reading::Unavailable(WARMING_UP.into()),
                    Reading::Unavailable(WARMING_UP.into()),
                ),
                None => (
                    Reading::Unavailable(format!("{name} is gone")),
                    Reading::Unavailable(format!("{name} is gone")),
                ),
            };
            put(out, &format!("net.{id}.down"), down);
            put(out, &format!("net.{id}.up"), up);
        }
        let (mut down, mut up) = (Some(0.0), Some(0.0));
        for name in current.keys().filter(|n| physical(n)) {
            if let Some((d, u)) = rates(name) {
                down = down.zip(d).map(|(a, b)| a + b);
                up = up.zip(u).map(|(a, b)| a + b);
            }
        }
        let summed = |v: Option<f64>| match (&previous, v) {
            (None, _) => Reading::Unavailable(WARMING_UP.into()),
            (_, Some(v)) => Reading::Value(v),
            (_, None) => Reading::Unavailable("counter reset or no time elapsed".into()),
        };
        put(out, keys::NET_DOWN, summed(down));
        put(out, keys::NET_UP, summed(up));
        self.previous = Some((current, now));
    }
}

/// Uptime and host name.
pub(crate) struct System {
    catalog: Vec<SensorInfo>,
}

impl System {
    pub(crate) fn new() -> Self {
        let s = Category::System;
        let catalog = [
            describe(keys::UPTIME, s, "Uptime", Quantity::Seconds, "sysinfo"),
            describe("system.hostname", s, "Host name", Quantity::Text, "sysinfo"),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self { catalog }
    }
}

impl Provider for System {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        put(
            out,
            keys::UPTIME,
            Reading::Value(SysSystem::uptime() as f64),
        );
        let host = SysSystem::host_name().map_or_else(
            || Reading::Unavailable("no host name".into()),
            Reading::Text,
        );
        put(out, "system.hostname", host);
    }
}
