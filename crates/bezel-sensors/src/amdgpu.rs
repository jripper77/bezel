//! AMD GPUs through the amdgpu driver's sysfs files: `gpu_busy_percent`,
//! `mem_info_vram_used/total`, and the card's hwmon chip (edge, junction and
//! memory temperatures, power, shader clock, fan).
//!
//! A card is integrated when the driver does not expose `mem_busy_percent`,
//! which amdgpu hides on APUs. On an APU, `PPT` is the whole package's power
//! (CPU cores included) and "VRAM" is the BIOS carve-out of system RAM; the
//! labels say so. A runtime-suspended card is not read at all: touching its
//! files would wake it up just to report that it sleeps.

use std::path::{Path, PathBuf};
use std::time::Instant;

use bezel_core::domain::sensor::{Quantity, Reading, SensorInfo, Snapshot};

use crate::gpu::{FoundGpu, GpuCatalog, parse_pci};
use crate::linux::Roots;
use crate::linux::fs::{read_int, read_text};
use crate::provider::{Provider, percent, put};

const SLEEPING: &str = "the GPU is powered down (runtime suspend)";

/// How a raw sysfs integer becomes the metric's unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    /// Already in the unit (percent, bytes, RPM).
    Same,
    /// m°C → °C.
    Milli,
    /// µW → W, Hz → MHz.
    Micro,
    /// PWM 0-255 → percent.
    Pwm,
}

impl Unit {
    fn convert(self, raw: i64) -> f64 {
        let raw = raw as f64;
        match self {
            Unit::Same => raw,
            Unit::Milli => raw / 1000.0,
            Unit::Micro => raw / 1_000_000.0,
            Unit::Pwm => raw * 100.0 / 255.0,
        }
    }
}

/// A value of the card and the file it comes from.
#[derive(Debug, Clone)]
struct Metric {
    suffix: &'static str,
    path: PathBuf,
    unit: Unit,
}

/// Every amdgpu card under `<sys>/class/drm`.
pub(crate) fn discover(roots: &Roots) -> Vec<FoundGpu> {
    let Ok(entries) = std::fs::read_dir(roots.sys.join("class/drm")) else {
        return Vec::new();
    };
    let mut cards: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix("card"))
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    cards.sort();
    cards
        .into_iter()
        .filter_map(|card| {
            let device = std::fs::canonicalize(card.join("device")).ok()?;
            let driver = std::fs::canonicalize(device.join("driver")).ok()?;
            if driver.file_name()?.to_str()? != "amdgpu" {
                return None;
            }
            let pci = device
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(parse_pci);
            let integrated = !device.join("mem_busy_percent").exists();
            let name = model_name(&device, &roots.amdgpu_ids);
            Some(FoundGpu {
                pci,
                integrated,
                make: Box::new(move |n| {
                    Box::new(AmdGpu::new(device, name, integrated, n)) as Box<dyn Provider>
                }),
            })
        })
        .collect()
}

/// Marketing name from libdrm's `amdgpu.ids` (device and revision), else the
/// card's `product_name`, else the PCI id.
fn model_name(device: &Path, ids: &Path) -> String {
    let hex = |file: &str| {
        read_text(&device.join(file))
            .ok()
            .map(|t| t.trim().trim_start_matches("0x").to_ascii_lowercase())
    };
    let (id, revision) = (
        hex("device").unwrap_or_default(),
        hex("revision").unwrap_or_default(),
    );
    let listed = read_text(ids).ok().and_then(|table| {
        table.lines().find_map(|line| {
            let mut f = line.split(',').map(str::trim);
            let (d, r, name) = (f.next()?, f.next()?, f.next()?);
            (d.eq_ignore_ascii_case(&id) && r.eq_ignore_ascii_case(&revision))
                .then(|| name.to_string())
        })
    });
    listed
        .or_else(|| {
            read_text(&device.join("product_name"))
                .ok()
                .filter(|n| !n.trim().is_empty())
        })
        .unwrap_or_else(|| format!("AMD GPU 1002:{id}"))
}

/// The first hwmon directory of a sysfs device (`<device>/hwmon/hwmonN`).
fn hwmon_dir(device: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(device.join("hwmon"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}

/// The hwmon temperature input labelled `label`.
fn labelled_temperature(hwmon: &Path, label: &str) -> Option<PathBuf> {
    (1..=8).find_map(|n| {
        let text = read_text(&hwmon.join(format!("temp{n}_label"))).ok()?;
        let input = hwmon.join(format!("temp{n}_input"));
        (text.trim() == label && input.exists()).then_some(input)
    })
}

/// Collects the metrics whose files exist, with their catalog entries.
struct Builder {
    catalog: GpuCatalog,
    metrics: Vec<Metric>,
}

impl Builder {
    /// Adds `gpu.<n>.<suffix>` when `path` exists; tells whether it did.
    fn add(
        &mut self,
        suffix: &'static str,
        what: &str,
        quantity: Quantity,
        path: Option<PathBuf>,
        unit: Unit,
    ) -> bool {
        let Some(path) = path.filter(|p| p.exists()) else {
            return false;
        };
        self.catalog.add(suffix, what, quantity);
        self.metrics.push(Metric { suffix, path, unit });
        true
    }
}

/// One amdgpu card as `gpu.<n>.*`.
struct AmdGpu {
    index: usize,
    runtime_status: PathBuf,
    name: String,
    metrics: Vec<Metric>,
    catalog: Vec<SensorInfo>,
}

impl AmdGpu {
    fn new(device: PathBuf, name: String, integrated: bool, index: usize) -> Self {
        let hwmon = hwmon_dir(&device);
        let on_hwmon = |file: &str| hwmon.as_ref().map(|h| h.join(file));
        let temp = |label: &str| hwmon.as_ref().and_then(|h| labelled_temperature(h, label));
        let vram = if integrated {
            "VRAM carve-out"
        } else {
            "memory"
        };
        let power = on_hwmon("power1_average")
            .filter(|p| p.exists())
            .or_else(|| on_hwmon("power1_input"));
        let power_what = if integrated {
            "package power (PPT, CPU included)"
        } else {
            "power"
        };
        let mut b = Builder {
            catalog: GpuCatalog::new(index, "amdgpu sysfs"),
            metrics: Vec::new(),
        };
        let (c, m) = (Quantity::Celsius, Unit::Milli);
        b.add(
            "usage",
            "usage",
            Quantity::Percent,
            Some(device.join("gpu_busy_percent")),
            Unit::Same,
        );
        b.add(
            "temperature",
            "temperature (edge)",
            c,
            temp("edge").or_else(|| on_hwmon("temp1_input")),
            m,
        );
        b.add(
            "temperature.junction",
            "junction temperature",
            c,
            temp("junction"),
            m,
        );
        b.add(
            "temperature.memory",
            "memory temperature",
            c,
            temp("mem"),
            m,
        );
        let used = format!("{vram} used");
        let used = b.add(
            "memory.used",
            &used,
            Quantity::Bytes,
            Some(device.join("mem_info_vram_used")),
            Unit::Same,
        );
        let total = format!("{vram} total");
        let total = b.add(
            "memory.total",
            &total,
            Quantity::Bytes,
            Some(device.join("mem_info_vram_total")),
            Unit::Same,
        );
        if used && total {
            b.catalog.add(
                "memory.percent",
                &format!("{vram} used (percent)"),
                Quantity::Percent,
            );
        }
        b.add("power", power_what, Quantity::Watts, power, Unit::Micro);
        b.add(
            "frequency",
            "core clock",
            Quantity::Megahertz,
            on_hwmon("freq1_input"),
            Unit::Micro,
        );
        b.add("fan", "fan", Quantity::Percent, on_hwmon("pwm1"), Unit::Pwm);
        b.add(
            "fan.rpm",
            "fan speed",
            Quantity::Rpm,
            on_hwmon("fan1_input"),
            Unit::Same,
        );
        b.catalog.add("name", "model", Quantity::Text);
        Self {
            index,
            runtime_status: device.join("power/runtime_status"),
            name,
            metrics: b.metrics,
            catalog: b.catalog.into_entries(),
        }
    }

    fn asleep(&self) -> bool {
        read_text(&self.runtime_status).is_ok_and(|s| s.trim() == "suspended")
    }
}

impl Provider for AmdGpu {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        let n = self.index;
        let asleep = self.asleep();
        let mut memory = (None, None);
        for m in &self.metrics {
            let reading = if asleep {
                Reading::Unavailable(SLEEPING.into())
            } else {
                match read_int(&m.path) {
                    Ok(raw) => Reading::Value(m.unit.convert(raw)),
                    Err(reason) => Reading::Unavailable(reason),
                }
            };
            match m.suffix {
                "memory.used" => memory.0 = Some(reading.clone()),
                "memory.total" => memory.1 = Some(reading.clone()),
                _ => {}
            }
            put(out, &format!("gpu.{n}.{}", m.suffix), reading);
        }
        if let (Some(used), Some(total)) = memory {
            let pct = match (used.value(), total.value()) {
                (Some(u), Some(t)) => percent(u, t, "VRAM total"),
                _ => Reading::Unavailable(if asleep {
                    SLEEPING.into()
                } else {
                    "VRAM unreadable".into()
                }),
            };
            put(out, &format!("gpu.{n}.memory.percent"), pct);
        }
        put(
            out,
            &format!("gpu.{n}.name"),
            Reading::Text(self.name.clone()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::number;
    use crate::testing::FakeTree;
    use bezel_core::domain::sensor::SensorKey;

    fn get(s: &Snapshot, key: &str) -> Reading {
        s.get(&SensorKey::new(key).unwrap())
    }

    /// An APU at 6a:00.0 and a discrete card at 03:00.0.
    fn tree() -> FakeTree {
        let t = FakeTree::new("amdgpu");
        t.dir("sys/bus/pci/drivers/amdgpu")
            .dir("sys/bus/pci/drivers/nvidia");
        let igpu = "sys/devices/pci0000:00/0000:6a:00.0";
        t.file(&format!("{igpu}/gpu_busy_percent"), "3\n")
            .file(&format!("{igpu}/mem_info_vram_used"), "82292736\n")
            .file(&format!("{igpu}/mem_info_vram_total"), "536870912\n")
            .file(&format!("{igpu}/device"), "0x164e\n")
            .file(&format!("{igpu}/revision"), "0xc1\n")
            .file(&format!("{igpu}/power/runtime_status"), "active\n")
            .file(&format!("{igpu}/hwmon/hwmon2/temp1_input"), "43000\n")
            .file(&format!("{igpu}/hwmon/hwmon2/temp1_label"), "edge\n")
            .file(&format!("{igpu}/hwmon/hwmon2/power1_input"), "39660000\n")
            .file(&format!("{igpu}/hwmon/hwmon2/freq1_input"), "600000000\n")
            .link(&format!("{igpu}/driver"), "sys/bus/pci/drivers/amdgpu")
            .link("sys/class/drm/card1/device", igpu);
        let dgpu = "sys/devices/pci0000:00/0000:03:00.0";
        t.file(&format!("{dgpu}/gpu_busy_percent"), "97\n")
            .file(&format!("{dgpu}/mem_busy_percent"), "40\n")
            .file(&format!("{dgpu}/mem_info_vram_used"), "1073741824\n")
            .file(&format!("{dgpu}/mem_info_vram_total"), "25753026560\n")
            .file(&format!("{dgpu}/device"), "0x744c\n")
            .file(&format!("{dgpu}/revision"), "0xc8\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/temp1_input"), "61000\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/temp1_label"), "edge\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/temp2_input"), "75000\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/temp2_label"), "junction\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/temp3_input"), "68000\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/temp3_label"), "mem\n")
            .file(
                &format!("{dgpu}/hwmon/hwmon5/power1_average"),
                "300000000\n",
            )
            .file(&format!("{dgpu}/hwmon/hwmon5/power1_input"), "310000000\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/freq1_input"), "2500000000\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/pwm1"), "102\n")
            .file(&format!("{dgpu}/hwmon/hwmon5/fan1_input"), "1650\n")
            .link(&format!("{dgpu}/driver"), "sys/bus/pci/drivers/amdgpu")
            .link("sys/class/drm/card0/device", dgpu);
        let nv = "sys/devices/pci0000:00/0000:01:00.0";
        t.dir(nv)
            .link(&format!("{nv}/driver"), "sys/bus/pci/drivers/nvidia")
            .link("sys/class/drm/card2/device", nv);
        t.dir("sys/class/drm/card1-DP-1")
            .dir("sys/class/drm/renderD128");
        t.file(
            "amdgpu.ids",
            "# List\n1.0.0\n744C,\tC8,\tAMD Radeon RX 7900 XTX\n164E,\tD8,\tAMD Radeon 610M\n",
        );
        t
    }

    fn roots(t: &FakeTree) -> Roots {
        let mut roots = Roots::new(t.path("sys"), t.path("proc"));
        roots.amdgpu_ids = t.path("amdgpu.ids");
        roots
    }

    #[test]
    fn discrete_and_integrated_cards() {
        let t = tree();
        let found = discover(&roots(&t));
        assert_eq!(found.len(), 2, "the NVIDIA card and connectors are skipped");
        let gpus = number(found);
        let mut out = Snapshot::default();
        let mut providers = gpus.providers;
        for p in &mut providers {
            p.sample(Instant::now(), &mut out);
        }
        // 03:00.0 sorts first and is discrete: gpu.0, and the primary.
        assert_eq!(gpus.aliases[0].target.as_str(), "gpu.0.usage");
        assert_eq!(get(&out, "gpu.0.usage"), Reading::Value(97.0));
        assert_eq!(get(&out, "gpu.0.temperature"), Reading::Value(61.0));
        assert_eq!(
            get(&out, "gpu.0.temperature.junction"),
            Reading::Value(75.0)
        );
        assert_eq!(get(&out, "gpu.0.temperature.memory"), Reading::Value(68.0));
        assert_eq!(
            get(&out, "gpu.0.power"),
            Reading::Value(300.0),
            "the average wins"
        );
        assert_eq!(get(&out, "gpu.0.frequency"), Reading::Value(2500.0));
        assert_eq!(get(&out, "gpu.0.fan"), Reading::Value(40.0));
        assert_eq!(get(&out, "gpu.0.fan.rpm"), Reading::Value(1650.0));
        assert_eq!(
            get(&out, "gpu.0.name"),
            Reading::Text("AMD Radeon RX 7900 XTX".into())
        );
        let vram = get(&out, "gpu.0.memory.percent").value().unwrap();
        assert!((vram - 4.1694).abs() < 1e-3, "{vram}");
        // The APU: no junction or fan, PPT labelled as the package's.
        assert_eq!(get(&out, "gpu.1.usage"), Reading::Value(3.0));
        assert_eq!(get(&out, "gpu.1.power"), Reading::Value(39.66));
        assert_eq!(get(&out, "gpu.1.frequency"), Reading::Value(600.0));
        assert_eq!(
            get(&out, "gpu.1.name"),
            Reading::Text("AMD GPU 1002:164e".into())
        );
        let catalog: Vec<SensorInfo> = providers.iter().flat_map(|p| p.catalog()).collect();
        let igpu_power = catalog
            .iter()
            .find(|i| i.key.as_str() == "gpu.1.power")
            .unwrap();
        assert_eq!(igpu_power.label, "GPU 1 package power (PPT, CPU included)");
        assert!(
            !catalog
                .iter()
                .any(|i| i.key.as_str() == "gpu.1.temperature.junction")
        );
        assert_eq!(out.len(), catalog.len());
    }

    #[test]
    fn a_sleeping_card_is_not_woken() {
        let t = tree();
        let dgpu = "sys/devices/pci0000:00/0000:03:00.0";
        t.file(&format!("{dgpu}/power/runtime_status"), "suspended\n");
        let mut gpus = number(discover(&roots(&t)));
        let mut out = Snapshot::default();
        gpus.providers[0].sample(Instant::now(), &mut out);
        assert_eq!(
            get(&out, "gpu.0.usage"),
            Reading::Unavailable(SLEEPING.into())
        );
        assert_eq!(
            get(&out, "gpu.0.memory.percent"),
            Reading::Unavailable(SLEEPING.into())
        );
        assert_eq!(
            get(&out, "gpu.0.name"),
            Reading::Text("AMD Radeon RX 7900 XTX".into())
        );
    }

    #[test]
    fn names_fall_back_and_errors_are_explained() {
        let t = tree();
        let igpu = "sys/devices/pci0000:00/0000:6a:00.0";
        t.file(&format!("{igpu}/product_name"), "Radeon Graphics\n");
        t.file(&format!("{igpu}/mem_info_vram_used"), "busy\n");
        let mut gpus = number(discover(&roots(&t)));
        let mut out = Snapshot::default();
        gpus.providers[1].sample(Instant::now(), &mut out);
        assert_eq!(
            get(&out, "gpu.1.name"),
            Reading::Text("Radeon Graphics".into())
        );
        assert!(
            matches!(get(&out, "gpu.1.memory.used"), Reading::Unavailable(r) if r.starts_with("unexpected"))
        );
        assert_eq!(
            get(&out, "gpu.1.memory.percent"),
            Reading::Unavailable("VRAM unreadable".into())
        );
        assert!(discover(&Roots::new("/nonexistent", "/nonexistent")).is_empty());
        assert_eq!(Unit::Pwm.convert(255), 100.0);
    }
}
