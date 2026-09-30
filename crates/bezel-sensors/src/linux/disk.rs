//! Disk throughput from `/proc/diskstats` and space per mounted filesystem.
//!
//! Rates count whole physical disks only (those with a `device` link in
//! `/sys/block`): partitions, device-mapper, md RAID, loop and zram devices
//! would count the same bytes twice or are memory. `/proc/diskstats` sectors
//! are always 512 bytes, whatever the disk's logical block size.
//!
//! Space follows `df`: used = blocks - free blocks, free = blocks available
//! to unprivileged users, percent = used / (used + available). Only
//! block-device filesystems (and ZFS) are listed, so a hung network mount
//! can never block a sample.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys, rate};

use super::Roots;
use super::fs::{read_error, read_text};
use crate::provider::{Provider, WARMING_UP, describe, percent, put, slug};

const SECTOR: u64 = 512;

/// Counters of one `/proc/diskstats` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counters {
    read_bytes: u64,
    written_bytes: u64,
    busy_ms: u64,
}

fn parse_diskstats(text: &str) -> HashMap<String, Counters> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i)?.parse::<u64>().ok();
            let counters = Counters {
                read_bytes: num(5)?.checked_mul(SECTOR)?,
                written_bytes: num(9)?.checked_mul(SECTOR)?,
                busy_ms: num(12)?,
            };
            Some(((*f.get(2)?).to_string(), counters))
        })
        .collect()
}

/// A mounted filesystem worth listing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mount {
    source: String,
    path: PathBuf,
    fs: String,
    id: String,
}

/// `\040` and friends in `/proc/self/mounts` fields.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes.get(i + 1..i + 4).and_then(|d| {
            let text = std::str::from_utf8(d).ok()?;
            u8::from_str_radix(text, 8).ok()
        });
        match (bytes[i], octal) {
            (b'\\', Some(byte)) => {
                out.push(byte);
                i += 4;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Key segment of a mount point: `/` is `root`, `/boot/efi` is `boot_efi`.
fn mount_id(path: &str) -> String {
    match slug(path) {
        s if s.is_empty() => "root".to_string(),
        s => s,
    }
}

fn parse_mounts(text: &str) -> Vec<Mount> {
    let mut seen = HashSet::new();
    let mut mounts = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (Some(source), Some(path), Some(fs)) = (f.first(), f.get(1), f.get(2)) else {
            continue;
        };
        let block = source.starts_with("/dev/") && !source.starts_with("/dev/loop");
        let image = matches!(*fs, "squashfs" | "iso9660");
        if !((block && !image) || *fs == "zfs") {
            continue;
        }
        let path = unescape(path);
        let id = mount_id(&path);
        // An over-mounted path or a colliding slug is listed once.
        if seen.insert(id.clone()) {
            mounts.push(Mount {
                source: unescape(source),
                path: PathBuf::from(path),
                fs: (*fs).to_string(),
                id,
            });
        }
    }
    mounts
}

/// `(used, total, free, percent)` of a filesystem, like `df`.
fn space(mount: &Mount) -> Result<[Reading; 4], String> {
    let st = rustix::fs::statvfs(&mount.path)
        .map_err(|e| format!("statvfs {} failed: {e}", mount.path.display()))?;
    let bytes = |blocks: u64| blocks as f64 * st.f_frsize as f64;
    let used = bytes(st.f_blocks.saturating_sub(st.f_bfree));
    let free = bytes(st.f_bavail);
    Ok([
        Reading::Value(used),
        Reading::Value(bytes(st.f_blocks)),
        Reading::Value(free),
        percent(used, used + free, "filesystem size"),
    ])
}

/// The disk provider.
pub(crate) struct Disks {
    diskstats: PathBuf,
    block: PathBuf,
    disks: Vec<String>,
    physical: HashMap<String, bool>,
    mounts: Vec<Mount>,
    previous: Option<(HashMap<String, Counters>, Instant)>,
    catalog: Vec<SensorInfo>,
}

const SPACE: [&str; 4] = ["used", "total", "free", "percent"];

impl Disks {
    /// Lists the physical disks of `<proc>/diskstats` and the mounts of `<proc>/self/mounts`.
    pub(crate) fn new(roots: &Roots) -> Self {
        let diskstats = roots.proc.join("diskstats");
        let mut disks = Self {
            block: roots.sys.join("block"),
            disks: Vec::new(),
            physical: HashMap::new(),
            mounts: read_text(&roots.proc.join("self/mounts"))
                .map(|t| parse_mounts(&t))
                .unwrap_or_default(),
            previous: None,
            catalog: Vec::new(),
            diskstats,
        };
        let mut names: Vec<String> = read_text(&disks.diskstats)
            .map(|t| parse_diskstats(&t).into_keys().collect())
            .unwrap_or_default();
        names.sort();
        disks.disks = names.into_iter().filter(|n| disks.is_physical(n)).collect();
        disks.catalog = disks.describe_all();
        disks
    }

    fn describe_all(&self) -> Vec<SensorInfo> {
        let d = Category::Disk;
        let all = "/proc/diskstats (physical disks)";
        let mut catalog = vec![
            describe(
                keys::DISK_READ,
                d,
                "Disk read rate",
                Quantity::BytesPerSecond,
                all,
            ),
            describe(
                keys::DISK_WRITE,
                d,
                "Disk write rate",
                Quantity::BytesPerSecond,
                all,
            ),
        ];
        for m in &self.mounts {
            let src = format!("statvfs {} ({} on {})", m.path.display(), m.fs, m.source);
            let at = m.path.display();
            catalog.push(describe(
                &format!("disk.{}.used", m.id),
                d,
                format!("{at} used"),
                Quantity::Bytes,
                &src,
            ));
            catalog.push(describe(
                &format!("disk.{}.total", m.id),
                d,
                format!("{at} size"),
                Quantity::Bytes,
                &src,
            ));
            catalog.push(describe(
                &format!("disk.{}.free", m.id),
                d,
                format!("{at} free"),
                Quantity::Bytes,
                &src,
            ));
            catalog.push(describe(
                &format!("disk.{}.percent", m.id),
                d,
                format!("{at} used (percent)"),
                Quantity::Percent,
                src,
            ));
        }
        for disk in &self.disks {
            let src = "/proc/diskstats";
            catalog.push(describe(
                &format!("disk.{disk}.read"),
                d,
                format!("{disk} read rate"),
                Quantity::BytesPerSecond,
                src,
            ));
            catalog.push(describe(
                &format!("disk.{disk}.write"),
                d,
                format!("{disk} write rate"),
                Quantity::BytesPerSecond,
                src,
            ));
            catalog.push(describe(
                &format!("disk.{disk}.activity"),
                d,
                format!("{disk} activity"),
                Quantity::Percent,
                src,
            ));
        }
        catalog.into_iter().flatten().collect()
    }

    fn is_physical(&mut self, name: &str) -> bool {
        if let Some(known) = self.physical.get(name) {
            return *known;
        }
        // Names with '/' (cciss/c0d0) appear with '!' in /sys/block.
        let physical = self
            .block
            .join(name.replace('/', "!"))
            .join("device")
            .exists();
        self.physical.insert(name.to_string(), physical);
        physical
    }

    fn sample_space(&self, out: &mut Snapshot) {
        for m in &self.mounts {
            match space(m) {
                Ok(values) => {
                    for (suffix, value) in SPACE.iter().zip(values) {
                        put(out, &format!("disk.{}.{suffix}", m.id), value);
                    }
                }
                Err(reason) => {
                    for suffix in SPACE {
                        put(
                            out,
                            &format!("disk.{}.{suffix}", m.id),
                            Reading::Unavailable(reason.clone()),
                        );
                    }
                }
            }
        }
    }

    fn sample_io(&mut self, now: Instant, out: &mut Snapshot) {
        let current = match read_text(&self.diskstats) {
            Ok(text) => parse_diskstats(&text),
            Err(e) => {
                let reason = read_error(&self.diskstats, &e);
                for info in self
                    .catalog
                    .iter()
                    .filter(|i| !SPACE.iter().any(|s| i.key.as_str().ends_with(s)))
                {
                    out.insert(info.key.clone(), Reading::Unavailable(reason.clone()));
                }
                return;
            }
        };
        let previous = self.previous.take();
        let elapsed = previous
            .as_ref()
            .map(|(_, then)| now.saturating_duration_since(*then));
        let mut names: Vec<&String> = current.keys().collect();
        names.sort();
        let members: Vec<String> = names
            .into_iter()
            .filter(|n| self.is_physical(n))
            .cloned()
            .collect();
        let (mut read, mut write) = (Some(0.0), Some(0.0));
        for name in &members {
            let now_c = current[name];
            let before = previous.as_ref().and_then(|(b, _)| b.get(name).copied());
            let listed = self.disks.contains(name);
            let Some((before, elapsed)) = before.zip(elapsed) else {
                if previous.is_none() && listed {
                    for s in ["read", "write", "activity"] {
                        put(
                            out,
                            &format!("disk.{name}.{s}"),
                            Reading::Unavailable(WARMING_UP.into()),
                        );
                    }
                }
                continue;
            };
            let r = rate(before.read_bytes, now_c.read_bytes, elapsed);
            let w = rate(before.written_bytes, now_c.written_bytes, elapsed);
            read = read.zip(r).map(|(a, b)| a + b);
            write = write.zip(w).map(|(a, b)| a + b);
            if listed {
                let value = |v: Option<f64>| {
                    v.map_or_else(
                        || Reading::Unavailable("counter reset or no time elapsed".into()),
                        Reading::Value,
                    )
                };
                put(out, &format!("disk.{name}.read"), value(r));
                put(out, &format!("disk.{name}.write"), value(w));
                put(
                    out,
                    &format!("disk.{name}.activity"),
                    value(activity(before.busy_ms, now_c.busy_ms, elapsed)),
                );
            }
        }
        for name in self.disks.iter().filter(|n| !current.contains_key(*n)) {
            for s in ["read", "write", "activity"] {
                put(
                    out,
                    &format!("disk.{name}.{s}"),
                    Reading::Unavailable(format!("disk {name} is gone")),
                );
            }
        }
        let total = |v: Option<f64>| match (&previous, v) {
            (None, _) => Reading::Unavailable(WARMING_UP.into()),
            (_, Some(v)) => Reading::Value(v),
            (_, None) => Reading::Unavailable("counter reset or no time elapsed".into()),
        };
        put(out, keys::DISK_READ, total(read));
        put(out, keys::DISK_WRITE, total(write));
        self.previous = Some((current, now));
    }
}

/// Percent of the elapsed time the disk had I/O in flight.
fn activity(before_ms: u64, now_ms: u64, elapsed: Duration) -> Option<f64> {
    let busy = rate(before_ms, now_ms, elapsed)?;
    // ms of busy time per second of real time, as a percent; timer
    // granularity can overshoot by a tick.
    Some((busy / 10.0).min(100.0))
}

impl Provider for Disks {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        self.sample_space(out);
        self.sample_io(now, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;

    fn line(name: &str, read_sectors: u64, written_sectors: u64, busy_ms: u64) -> String {
        format!(
            " 259 0 {name} 100 0 {read_sectors} 50 200 0 {written_sectors} 70 0 {busy_ms} 120 0 0 0 0 0 0\n"
        )
    }

    fn get(s: &Snapshot, key: &str) -> Reading {
        s.get(&SensorKey::new(key).unwrap())
    }

    fn setup() -> FakeTree {
        let t = FakeTree::new("disk");
        for disk in ["nvme0n1", "sda"] {
            t.dir(&format!("sys/devices/{disk}")).link(
                &format!("sys/block/{disk}/device"),
                &format!("sys/devices/{disk}"),
            );
        }
        t.dir("sys/block/zram0").dir("sys/block/dm-0");
        let tmp = t.root().display().to_string();
        t.file(
            "proc/self/mounts",
            &format!(
                "/dev/sda3 / btrfs rw 0 0\n\
                 /dev/sda3 /home btrfs rw 0 0\n\
                 /dev/sda1 /boot/efi vfat rw 0 0\n\
                 /dev/sdb1 {tmp}/My\\040Disk ext4 rw 0 0\n\
                 tmpfs /tmp tmpfs rw 0 0\n\
                 /dev/loop3 /snap/core squashfs ro 0 0\n\
                 server:/x /mnt/nfs nfs4 rw 0 0\n\
                 tank/data /tank zfs rw 0 0\n\
                 /dev/sda3 / btrfs rw 0 0\n"
            ),
        );
        t.dir("My Disk");
        t
    }

    fn stats(t: &FakeTree, lines: &[String]) {
        t.file("proc/diskstats", &lines.concat());
    }

    #[test]
    fn whole_physical_disks_only_and_sectors_are_512_bytes() {
        let t = setup();
        stats(
            &t,
            &[
                line("nvme0n1", 0, 0, 0),
                line("nvme0n1p1", 0, 0, 0),
                line("sda", 0, 0, 0),
                line("zram0", 0, 0, 0),
                line("dm-0", 0, 0, 0),
            ],
        );
        let mut disks = Disks::new(&Roots::new(t.path("sys"), t.path("proc")));
        assert_eq!(disks.disks, ["nvme0n1", "sda"]);
        let t0 = Instant::now();
        let mut first = Snapshot::default();
        disks.sample(t0, &mut first);
        assert_eq!(
            get(&first, "disk.read"),
            Reading::Unavailable(WARMING_UP.into())
        );
        assert_eq!(
            get(&first, "disk.sda.activity"),
            Reading::Unavailable(WARMING_UP.into())
        );
        // 2 s later: nvme read 4096 sectors (2 MiB), wrote 2048; sda wrote 1024;
        // partitions, zram and dm move too and must not be added.
        stats(
            &t,
            &[
                line("nvme0n1", 4096, 2048, 500),
                line("nvme0n1p1", 4096, 2048, 500),
                line("sda", 0, 1024, 2400),
                line("zram0", 9999, 9999, 0),
                line("dm-0", 9999, 9999, 0),
            ],
        );
        let mut second = Snapshot::default();
        disks.sample(t0 + Duration::from_secs(2), &mut second);
        assert_eq!(
            get(&second, "disk.nvme0n1.read"),
            Reading::Value(1_048_576.0)
        );
        assert_eq!(get(&second, "disk.read"), Reading::Value(1_048_576.0));
        assert_eq!(get(&second, "disk.write"), Reading::Value(786_432.0));
        assert_eq!(get(&second, "disk.nvme0n1.activity"), Reading::Value(25.0));
        assert_eq!(get(&second, "disk.sda.activity"), Reading::Value(100.0));
        let keys: Vec<String> = disks.catalog().iter().map(|i| i.key.to_string()).collect();
        assert_eq!(second.len(), keys.len());
        // A disk that vanished (USB unplugged).
        stats(&t, &[line("nvme0n1", 4096, 2048, 500)]);
        let mut third = Snapshot::default();
        disks.sample(t0 + Duration::from_secs(3), &mut third);
        assert!(
            matches!(get(&third, "disk.sda.read"), Reading::Unavailable(r) if r.contains("gone"))
        );
        assert_eq!(get(&third, "disk.read"), Reading::Value(0.0));
        std::fs::remove_file(t.path("proc/diskstats")).unwrap();
        let mut fourth = Snapshot::default();
        disks.sample(t0 + Duration::from_secs(4), &mut fourth);
        assert!(
            matches!(get(&fourth, "disk.write"), Reading::Unavailable(r) if r.contains("does not exist"))
        );
        assert!(get(&fourth, "disk.root.total").value().is_some());
    }

    #[test]
    fn mounts_are_block_filesystems_listed_once() {
        let t = setup();
        let text = std::fs::read_to_string(t.path("proc/self/mounts")).unwrap();
        let ids: Vec<String> = parse_mounts(&text).into_iter().map(|m| m.id).collect();
        let disk_id = mount_id(&format!("{}/My Disk", t.root().display()));
        assert_eq!(
            ids,
            [
                "root".to_string(),
                "home".into(),
                "boot_efi".into(),
                disk_id.clone(),
                "tank".into()
            ]
        );
        assert_eq!(unescape("a\\040b\\\\c\\9"), "a b\\\\c\\9");
        stats(&t, &[]);
        let mut disks = Disks::new(&Roots::new(t.path("sys"), t.path("proc")));
        let mut s = Snapshot::default();
        disks.sample(Instant::now(), &mut s);
        let used = get(&s, &format!("disk.{disk_id}.used")).value().unwrap();
        let free = get(&s, &format!("disk.{disk_id}.free")).value().unwrap();
        let total = get(&s, &format!("disk.{disk_id}.total")).value().unwrap();
        let pct = get(&s, &format!("disk.{disk_id}.percent")).value().unwrap();
        assert!(used + free <= total, "{used} + {free} > {total}");
        assert!((pct - used / (used + free) * 100.0).abs() < 1e-9);
        // /tank does not exist here: statvfs fails and says so.
        assert!(
            matches!(get(&s, "disk.tank.used"), Reading::Unavailable(r) if r.starts_with("statvfs /tank"))
        );
        let info = disks
            .catalog()
            .into_iter()
            .find(|i| i.key.as_str() == "disk.boot_efi.used")
            .unwrap();
        assert_eq!(info.source, "statvfs /boot/efi (vfat on /dev/sda1)");
    }
}
