//! NVIDIA GPUs through NVML (`nvml-wrapper`), loaded at run time: without
//! the NVIDIA driver there is no library, and simply no NVIDIA GPU.
//!
//! Memory comes from `nvmlDeviceGetMemoryInfo_v2`, whose "used" excludes the
//! driver-reserved memory, like `nvidia-smi`. Power is the board draw NVML
//! reports (a 1 s average on Ampere and newer).

use std::sync::Arc;
use std::time::Instant;

use bezel_core::domain::sensor::{Quantity, Reading, SensorInfo, Snapshot};
use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::error::NvmlError;

use crate::gpu::{FoundGpu, GpuCatalog, parse_pci};
use crate::provider::{Provider, all_unavailable, percent, put};

/// Every NVIDIA GPU NVML can see; empty without the driver.
pub(crate) fn discover() -> Vec<FoundGpu> {
    let nvml = match Nvml::init() {
        Ok(nvml) => Arc::new(nvml),
        Err(e) => {
            tracing::debug!("NVML not available: {e}");
            return Vec::new();
        }
    };
    let count = nvml.device_count().unwrap_or_else(|e| {
        tracing::warn!("NVML device count failed: {e}");
        0
    });
    (0..count)
        .filter_map(|i| {
            let device = nvml
                .device_by_index(i)
                .map_err(|e| tracing::warn!("NVML device {i}: {e}"))
                .ok()?;
            let pci = device
                .pci_info()
                .ok()
                .and_then(|p| parse_pci(&p.bus_id).or(Some((p.domain, p.bus, p.device, 0))));
            let name = device.name().ok();
            let nvml = Arc::clone(&nvml);
            Some(FoundGpu {
                pci,
                integrated: false,
                make: Box::new(move |n| {
                    Box::new(NvidiaGpu::new(nvml, i, name, n)) as Box<dyn Provider>
                }),
            })
        })
        .collect()
}

/// Why an NVML query failed, in words for the user.
fn reason(e: &NvmlError) -> String {
    match e {
        NvmlError::NotSupported => "not supported by this GPU".into(),
        NvmlError::NoPermission => "NVML denied access to this value".into(),
        NvmlError::GpuLost => "the GPU fell off the bus".into(),
        other => format!("NVML: {other}"),
    }
}

/// One sample of a GPU's values, or why each is missing.
#[derive(Debug, Clone, PartialEq)]
struct Metrics {
    usage: Result<u32, String>,
    temperature: Result<u32, String>,
    /// `(used, total)` bytes.
    memory: Result<(u64, u64), String>,
    power_milliwatts: Result<u32, String>,
    clock_mhz: Result<u32, String>,
    fan_percent: Result<u32, String>,
}

/// The suffixes this provider reports, with their labels and quantities.
const METRICS: [(&str, &str, Quantity); 9] = [
    ("usage", "usage", Quantity::Percent),
    ("temperature", "temperature", Quantity::Celsius),
    ("memory.used", "memory used", Quantity::Bytes),
    ("memory.total", "memory total", Quantity::Bytes),
    ("memory.percent", "memory used (percent)", Quantity::Percent),
    ("power", "power", Quantity::Watts),
    ("frequency", "core clock", Quantity::Megahertz),
    ("fan", "fan", Quantity::Percent),
    ("name", "model", Quantity::Text),
];

fn readings(m: &Metrics) -> [(&'static str, Reading); 8] {
    let value = |r: &Result<u32, String>, scale: f64| match r {
        Ok(v) => Reading::Value(f64::from(*v) / scale),
        Err(e) => Reading::Unavailable(e.clone()),
    };
    let memory = |pick: fn(&(u64, u64)) -> Reading| match &m.memory {
        Ok(mem) => pick(mem),
        Err(e) => Reading::Unavailable(e.clone()),
    };
    [
        ("usage", value(&m.usage, 1.0)),
        ("temperature", value(&m.temperature, 1.0)),
        (
            "memory.used",
            memory(|(used, _)| Reading::Value(*used as f64)),
        ),
        (
            "memory.total",
            memory(|(_, total)| Reading::Value(*total as f64)),
        ),
        (
            "memory.percent",
            memory(|(used, total)| percent(*used as f64, *total as f64, "GPU memory")),
        ),
        ("power", value(&m.power_milliwatts, 1000.0)),
        ("frequency", value(&m.clock_mhz, 1.0)),
        ("fan", value(&m.fan_percent, 1.0)),
    ]
}

/// One NVIDIA GPU as `gpu.<n>.*`.
struct NvidiaGpu {
    nvml: Arc<Nvml>,
    device_index: u32,
    index: usize,
    name: Option<String>,
    catalog: Vec<SensorInfo>,
}

impl NvidiaGpu {
    fn new(nvml: Arc<Nvml>, device_index: u32, name: Option<String>, index: usize) -> Self {
        let mut catalog = GpuCatalog::new(index, "NVML");
        for (suffix, what, quantity) in METRICS {
            catalog.add(suffix, what, quantity);
        }
        Self {
            nvml,
            device_index,
            index,
            name,
            catalog: catalog.into_entries(),
        }
    }

    fn metrics(&self) -> Result<Metrics, String> {
        let d = self
            .nvml
            .device_by_index(self.device_index)
            .map_err(|e| reason(&e))?;
        let r = |e: NvmlError| reason(&e);
        Ok(Metrics {
            usage: d.utilization_rates().map(|u| u.gpu).map_err(r),
            temperature: d.temperature(TemperatureSensor::Gpu).map_err(r),
            memory: d.memory_info().map(|m| (m.used, m.total)).map_err(r),
            power_milliwatts: d.power_usage().map_err(r),
            clock_mhz: d.clock_info(Clock::Graphics).map_err(r),
            fan_percent: d.fan_speed(0).map_err(r),
        })
    }

    fn put_all(&self, readings: &[(&str, Reading)], out: &mut Snapshot) {
        let n = self.index;
        for (suffix, reading) in readings {
            put(out, &format!("gpu.{n}.{suffix}"), reading.clone());
        }
        let name = match &self.name {
            Some(name) => Reading::Text(name.clone()),
            None => Reading::Unavailable("NVML did not name this GPU".into()),
        };
        put(out, &format!("gpu.{n}.name"), name);
    }
}

impl Provider for NvidiaGpu {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        match self.metrics() {
            Ok(metrics) => self.put_all(&readings(&metrics), out),
            Err(why) => {
                all_unavailable(&self.catalog, &why, out);
                self.put_all(&[], out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Metrics {
        Metrics {
            usage: Ok(38),
            temperature: Ok(35),
            memory: Ok((2701 * 1024 * 1024, 24564 * 1024 * 1024)),
            power_milliwatts: Ok(25_510),
            clock_mhz: Ok(675),
            fan_percent: Ok(0),
        }
    }

    #[test]
    fn values_are_converted_to_their_units() {
        let r = readings(&good());
        assert_eq!(r[0], ("usage", Reading::Value(38.0)));
        assert_eq!(r[2], ("memory.used", Reading::Value(2_832_203_776.0)));
        assert_eq!(r[3], ("memory.total", Reading::Value(25_757_220_864.0)));
        let pct = r[4].1.value().unwrap();
        assert!((pct - 10.9958).abs() < 1e-3, "{pct}");
        assert_eq!(r[5], ("power", Reading::Value(25.51)));
        assert_eq!(r[6], ("frequency", Reading::Value(675.0)));
        assert_eq!(r[7], ("fan", Reading::Value(0.0)));
        let suffixes: Vec<&str> = r.iter().map(|(s, _)| *s).collect();
        let listed: Vec<&str> = METRICS.iter().map(|(s, _, _)| *s).take(8).collect();
        assert_eq!(suffixes, listed);
    }

    #[test]
    fn unsupported_values_say_so() {
        let m = Metrics {
            fan_percent: Err(reason(&NvmlError::NotSupported)),
            memory: Err(reason(&NvmlError::GpuLost)),
            ..good()
        };
        let r = readings(&m);
        assert_eq!(
            r[7].1,
            Reading::Unavailable("not supported by this GPU".into())
        );
        assert_eq!(
            r[2].1,
            Reading::Unavailable("the GPU fell off the bus".into())
        );
        assert_eq!(
            r[4].1,
            Reading::Unavailable("the GPU fell off the bus".into())
        );
        assert_eq!(
            reason(&NvmlError::NoPermission),
            "NVML denied access to this value"
        );
        assert!(reason(&NvmlError::Unknown).starts_with("NVML: "));
    }
}
