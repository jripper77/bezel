//! Network rates and totals from `/proc/net/dev`, per interface
//! (`net.<iface>.down`, `net.<iface>.down.total`) and summed (`net.down`,
//! `net.down.total`).
//!
//! The sum only counts physical interfaces (those with a `device` link in
//! `/sys/class/net`): loopback, bridges, veth pairs, Docker, libvirt, VPN
//! tunnels and Tailscale carry traffic that also crosses a physical NIC, or
//! never leaves the machine, and would be counted twice.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use bezel_core::domain::sensor::{Category, Quantity, Reading, SensorInfo, Snapshot, keys, rate};

use super::Roots;
use super::fs::{read_error, read_text};
use crate::provider::{COUNTER_RESET, Provider, WARMING_UP, describe, put};

/// Per-interface key suffixes: rates, then totals since the interface came up.
const SUFFIXES: [&str; 4] = ["down", "up", "down.total", "up.total"];

/// Name prefixes of virtual interfaces, never summed even with a device link.
const VIRTUAL: [&str; 10] = [
    "lo",
    "docker",
    "veth",
    "virbr",
    "br-",
    "vnet",
    "tun",
    "tap",
    "wg",
    "tailscale",
];

/// `(interface, received bytes, sent bytes)` for every line of `/proc/net/dev`.
fn parse(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let fields: Vec<u64> = rest
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            Some((name.trim().to_string(), *fields.first()?, *fields.get(8)?))
        })
        .collect()
}

/// Received and sent bytes per interface.
type Counters = HashMap<String, (u64, u64)>;

/// Interfaces worth their own keys: all but loopback and container veths,
/// which come and go with every container.
fn listed(name: &str) -> bool {
    name != "lo" && !name.starts_with("veth")
}

/// The network provider.
pub(crate) struct Network {
    dev: PathBuf,
    class: PathBuf,
    listed: Vec<String>,
    physical: HashMap<String, bool>,
    previous: Option<(Counters, Instant)>,
    catalog: Vec<SensorInfo>,
}

impl Network {
    /// Lists the interfaces of `<proc>/net/dev`.
    pub(crate) fn new(roots: &Roots) -> Self {
        let dev = roots.proc.join("net/dev");
        let listed: Vec<String> = read_text(&dev)
            .map(|t| {
                parse(&t)
                    .into_iter()
                    .map(|(n, _, _)| n)
                    .filter(|n| listed(n))
                    .collect()
            })
            .unwrap_or_default();
        let n = Category::Network;
        let src = "/proc/net/dev";
        let summed = "/proc/net/dev (physical interfaces)";
        let mut catalog: Vec<SensorInfo> = [
            describe(
                keys::NET_DOWN,
                n,
                "Download rate",
                Quantity::BytesPerSecond,
                summed,
            ),
            describe(
                keys::NET_UP,
                n,
                "Upload rate",
                Quantity::BytesPerSecond,
                summed,
            ),
            describe(
                keys::NET_DOWN_TOTAL,
                n,
                "Downloaded since boot",
                Quantity::Bytes,
                summed,
            ),
            describe(
                keys::NET_UP_TOTAL,
                n,
                "Uploaded since boot",
                Quantity::Bytes,
                summed,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        for iface in &listed {
            catalog.extend(
                [
                    describe(
                        &format!("net.{iface}.down"),
                        n,
                        format!("{iface} download rate"),
                        Quantity::BytesPerSecond,
                        src,
                    ),
                    describe(
                        &format!("net.{iface}.up"),
                        n,
                        format!("{iface} upload rate"),
                        Quantity::BytesPerSecond,
                        src,
                    ),
                    describe(
                        &format!("net.{iface}.down.total"),
                        n,
                        format!("{iface} downloaded"),
                        Quantity::Bytes,
                        src,
                    ),
                    describe(
                        &format!("net.{iface}.up.total"),
                        n,
                        format!("{iface} uploaded"),
                        Quantity::Bytes,
                        src,
                    ),
                ]
                .into_iter()
                .flatten(),
            );
        }
        Self {
            dev,
            class: roots.sys.join("class/net"),
            listed,
            physical: HashMap::new(),
            previous: None,
            catalog,
        }
    }

    fn is_physical(&mut self, name: &str) -> bool {
        if let Some(known) = self.physical.get(name) {
            return *known;
        }
        let physical = !VIRTUAL.iter().any(|p| name.starts_with(p))
            && self.class.join(name).join("device").exists();
        self.physical.insert(name.to_string(), physical);
        physical
    }

    fn all_unavailable(&self, reason: &str, out: &mut Snapshot) {
        crate::provider::all_unavailable(&self.catalog, reason, out);
    }
}

impl Provider for Network {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        let counters = match read_text(&self.dev) {
            Ok(text) => parse(&text),
            Err(e) => return self.all_unavailable(&read_error(&self.dev, &e), out),
        };
        let current: Counters = counters
            .into_iter()
            .map(|(n, rx, tx)| (n, (rx, tx)))
            .collect();
        let previous = self.previous.take();
        let elapsed = previous
            .as_ref()
            .map(|(_, then)| now.saturating_duration_since(*then));
        let rates = |name: &str, (rx, tx): (u64, u64)| -> (Reading, Reading) {
            let Some((before, _)) = &previous else {
                let warm = || Reading::Unavailable(WARMING_UP.into());
                return (warm(), warm());
            };
            let as_rate =
                |b: Option<u64>, c: u64| match b.zip(elapsed).and_then(|(b, e)| rate(b, c, e)) {
                    Some(r) => Reading::Value(r),
                    None => Reading::Unavailable(COUNTER_RESET.into()),
                };
            let old = before.get(name);
            (as_rate(old.map(|o| o.0), rx), as_rate(old.map(|o| o.1), tx))
        };
        for iface in &self.listed {
            let Some(&(rx, tx)) = current.get(iface) else {
                let gone = format!("interface {iface} is gone");
                for suffix in SUFFIXES {
                    put(
                        out,
                        &format!("net.{iface}.{suffix}"),
                        Reading::Unavailable(gone.clone()),
                    );
                }
                continue;
            };
            let (down, up) = rates(iface, (rx, tx));
            let totals = (Reading::Value(rx as f64), Reading::Value(tx as f64));
            for (suffix, reading) in SUFFIXES.into_iter().zip([down, up, totals.0, totals.1]) {
                put(out, &format!("net.{iface}.{suffix}"), reading);
            }
        }
        let mut names: Vec<&String> = current.keys().collect();
        names.sort();
        let members: Vec<String> = names
            .into_iter()
            .filter(|n| self.is_physical(n))
            .cloned()
            .collect();
        let (mut down, mut up, mut rx_total, mut tx_total) = (Some(0.0), Some(0.0), 0.0, 0.0);
        for name in &members {
            let (rx, tx) = current[name];
            rx_total += rx as f64;
            tx_total += tx as f64;
            // An interface that appeared since the last sample has no delta yet.
            if previous
                .as_ref()
                .is_some_and(|(b, _)| !b.contains_key(name))
            {
                continue;
            }
            let (d, u) = rates(name, (rx, tx));
            down = down.zip(d.value()).map(|(a, b)| a + b);
            up = up.zip(u.value()).map(|(a, b)| a + b);
        }
        let summed = |v: Option<f64>| match (&previous, members.is_empty(), v) {
            (_, true, _) => Reading::Unavailable("no physical network interface".into()),
            (None, _, _) => Reading::Unavailable(WARMING_UP.into()),
            (_, _, Some(v)) => Reading::Value(v),
            (_, _, None) => Reading::Unavailable(COUNTER_RESET.into()),
        };
        put(out, keys::NET_DOWN, summed(down));
        put(out, keys::NET_UP, summed(up));
        let total = |v: f64| {
            if members.is_empty() {
                Reading::Unavailable("no physical network interface".into())
            } else {
                Reading::Value(v)
            }
        };
        put(out, keys::NET_DOWN_TOTAL, total(rx_total));
        put(out, keys::NET_UP_TOTAL, total(tx_total));
        self.previous = Some((current, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;
    use std::time::Duration;

    const HEADER: &str = "Inter-|   Receive |  Transmit\n face |bytes    packets|bytes\n";

    fn dev(lines: &[(&str, u64, u64)]) -> String {
        let mut text = HEADER.to_string();
        for (name, rx, tx) in lines {
            text.push_str(&format!(
                "{name:>6}: {rx} 10 0 0 0 0 0 0 {tx} 20 0 0 0 0 0 0\n"
            ));
        }
        text
    }

    fn get(s: &Snapshot, key: &str) -> Reading {
        s.get(&SensorKey::new(key).unwrap())
    }

    fn sample(net: &mut Network, at: Instant) -> Snapshot {
        let mut s = Snapshot::default();
        net.sample(at, &mut s);
        s
    }

    #[test]
    fn rates_per_interface_and_summed_over_physical_ones() {
        let t = FakeTree::new("net");
        t.dir("sys/devices/eno1")
            .link("sys/class/net/eno1/device", "sys/devices/eno1");
        t.dir("sys/devices/wlp96s0")
            .link("sys/class/net/wlp96s0/device", "sys/devices/wlp96s0");
        t.dir("sys/class/net/docker0")
            .dir("sys/class/net/tailscale0")
            .dir("sys/class/net/lo");
        t.file(
            "proc/net/dev",
            &dev(&[
                ("lo", 5000, 5000),
                ("eno1", 1000, 100),
                ("wlp96s0", 0, 0),
                ("docker0", 0, 0),
                ("tailscale0", 10, 10),
                ("veth12ab", 1, 1),
            ]),
        );
        let mut net = Network::new(&Roots::new(t.path("sys"), t.path("proc")));
        let keys: Vec<String> = net.catalog().iter().map(|i| i.key.to_string()).collect();
        assert!(keys.contains(&"net.eno1.down".to_string()));
        assert!(keys.contains(&"net.docker0.up".to_string()));
        assert!(
            !keys
                .iter()
                .any(|k| k.starts_with("net.lo.") || k.starts_with("net.veth"))
        );
        let t0 = Instant::now();
        let first = sample(&mut net, t0);
        assert_eq!(
            get(&first, "net.down"),
            Reading::Unavailable(WARMING_UP.into())
        );
        assert_eq!(
            get(&first, "net.eno1.down"),
            Reading::Unavailable(WARMING_UP.into())
        );
        assert_eq!(get(&first, keys::NET_DOWN_TOTAL), Reading::Value(1000.0));
        assert_eq!(first.len(), keys.len());

        // Half a second later: eno1 +2000/+500, wlp +1000/0, loopback and
        // tunnels move a lot but are not summed.
        t.file(
            "proc/net/dev",
            &dev(&[
                ("lo", 90000, 90000),
                ("eno1", 3000, 600),
                ("wlp96s0", 1000, 0),
                ("docker0", 0, 0),
                ("tailscale0", 9999, 9999),
            ]),
        );
        let second = sample(&mut net, t0 + Duration::from_millis(500));
        assert_eq!(get(&second, "net.eno1.down"), Reading::Value(4000.0));
        assert_eq!(get(&second, "net.eno1.up"), Reading::Value(1000.0));
        assert_eq!(get(&second, "net.down"), Reading::Value(6000.0));
        assert_eq!(get(&second, "net.up"), Reading::Value(1000.0));
        assert_eq!(get(&second, keys::NET_UP_TOTAL), Reading::Value(600.0));
        assert_eq!(
            get(&second, "net.tailscale0.down.total"),
            Reading::Value(9999.0)
        );
        assert_eq!(get(&second, "net.eno1.up.total"), Reading::Value(600.0));

        // eno1's counter reset (driver reload): no negative or huge rate.
        t.file(
            "proc/net/dev",
            &dev(&[("eno1", 10, 10), ("wlp96s0", 1000, 0)]),
        );
        let third = sample(&mut net, t0 + Duration::from_millis(1000));
        assert!(matches!(
            get(&third, "net.eno1.down"),
            Reading::Unavailable(_)
        ));
        assert_eq!(
            get(&third, "net.down"),
            Reading::Unavailable(COUNTER_RESET.into())
        );
        assert!(
            matches!(get(&third, "net.docker0.down"), Reading::Unavailable(r) if r.contains("gone"))
        );
    }

    #[test]
    fn new_interfaces_and_missing_files() {
        let t = FakeTree::new("net-new");
        t.file("proc/net/dev", &dev(&[("lo", 1, 1)]));
        let mut net = Network::new(&Roots::new(t.path("sys"), t.path("proc")));
        let t0 = Instant::now();
        let s = sample(&mut net, t0);
        assert!(
            matches!(get(&s, "net.down"), Reading::Unavailable(r) if r.starts_with("no physical"))
        );
        assert!(matches!(
            get(&s, keys::NET_DOWN_TOTAL),
            Reading::Unavailable(_)
        ));
        // A USB NIC plugged in: summed from its second sample on.
        t.dir("sys/devices/usb0")
            .link("sys/class/net/enp0s1/device", "sys/devices/usb0");
        t.file("proc/net/dev", &dev(&[("lo", 1, 1), ("enp0s1", 100, 100)]));
        let s = sample(&mut net, t0 + Duration::from_secs(1));
        assert_eq!(get(&s, "net.down"), Reading::Value(0.0));
        std::fs::remove_file(t.path("proc/net/dev")).unwrap();
        let s = sample(&mut net, t0 + Duration::from_secs(2));
        assert!(
            matches!(get(&s, "net.up"), Reading::Unavailable(r) if r.contains("does not exist"))
        );
        assert_eq!(parse("garbage\nx: 1 2\n"), vec![]);
    }
}
