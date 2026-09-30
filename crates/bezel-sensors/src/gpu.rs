//! GPU numbering across vendors and the primary `gpu.*` aliases.
//!
//! Vendor providers report the GPUs they found; every GPU is then numbered
//! by PCI address (`gpu.0.*`, `gpu.1.*`, stable across boots and vendors),
//! and the well-known `gpu.usage`, `gpu.temperature`, ... repeat the primary
//! GPU: the first discrete one, else the first integrated one.

use std::time::Instant;

use bezel_core::domain::sensor::{
    Category, Quantity, Reading, SensorInfo, SensorKey, Snapshot, keys,
};

use crate::provider::{Provider, describe, put};

/// Per-GPU metrics repeated for the primary GPU: `(suffix, alias label)`.
const PRIMARY: [(&str, &str); 9] = [
    ("usage", "GPU usage"),
    ("temperature", "GPU temperature"),
    ("memory.used", "GPU memory used"),
    ("memory.total", "GPU memory total"),
    ("memory.percent", "GPU memory used (percent)"),
    ("power", "GPU power"),
    ("frequency", "GPU core clock"),
    ("fan", "GPU fan"),
    ("name", "GPU model"),
];

/// Builds a GPU's provider once its number is known.
pub(crate) type MakeGpu = Box<dyn FnOnce(usize) -> Box<dyn Provider> + Send>;

/// A GPU found by a vendor provider, not yet numbered.
pub(crate) struct FoundGpu {
    /// `(domain, bus, device, function)`; `None` sorts last.
    pub(crate) pci: Option<(u32, u32, u32, u32)>,
    /// Integrated in the CPU package (shares system memory).
    pub(crate) integrated: bool,
    /// Builds the provider for `gpu.<n>.*`.
    pub(crate) make: MakeGpu,
}

/// `0000:6a:00.0` → `(0, 0x6a, 0, 0)`.
pub(crate) fn parse_pci(address: &str) -> Option<(u32, u32, u32, u32)> {
    let (domain, rest) = address.split_once(':')?;
    let (bus, rest) = rest.split_once(':')?;
    let (device, function) = rest.split_once('.')?;
    let hex = |s: &str| u32::from_str_radix(s, 16).ok();
    Some((hex(domain)?, hex(bus)?, hex(device)?, hex(function)?))
}

/// A key that repeats another key's reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Alias {
    pub(crate) key: SensorKey,
    pub(crate) target: SensorKey,
}

/// The numbered GPU providers, the primary aliases and their catalog entries.
pub(crate) struct Gpus {
    pub(crate) providers: Vec<Box<dyn Provider>>,
    pub(crate) aliases: Vec<Alias>,
    pub(crate) catalog: Vec<SensorInfo>,
}

/// Numbers `found` by PCI address and derives the primary aliases.
pub(crate) fn number(mut found: Vec<FoundGpu>) -> Gpus {
    found.sort_by_key(|g| (g.pci.is_none(), g.pci));
    let primary = found.iter().position(|g| !g.integrated).unwrap_or(0);
    let providers: Vec<Box<dyn Provider>> = found
        .into_iter()
        .enumerate()
        .map(|(n, g)| (g.make)(n))
        .collect();
    let Some(first) = providers.get(primary) else {
        return Gpus {
            providers: vec![Box::new(NoGpu::new())],
            aliases: Vec::new(),
            catalog: Vec::new(),
        };
    };
    let targets = first.catalog();
    let mut aliases = Vec::new();
    let mut catalog = Vec::new();
    for (suffix, label) in PRIMARY {
        let target_key = format!("gpu.{primary}.{suffix}");
        let Some(target) = targets.iter().find(|i| i.key.as_str() == target_key) else {
            continue;
        };
        let alias_key = format!("gpu.{suffix}");
        let source = format!("{} ({})", target.key, target.source);
        let Some(info) = describe(&alias_key, Category::Gpu, label, target.quantity, source) else {
            continue;
        };
        aliases.push(Alias {
            key: info.key.clone(),
            target: target.key.clone(),
        });
        catalog.push(info);
    }
    Gpus {
        providers,
        aliases,
        catalog,
    }
}

/// Stands in for the well-known GPU keys on a machine without a usable GPU.
struct NoGpu {
    catalog: Vec<SensorInfo>,
}

const NO_GPU: &str = "no GPU found (NVIDIA needs its driver's NVML; AMD needs amdgpu)";

impl NoGpu {
    fn new() -> Self {
        let g = Category::Gpu;
        let catalog = [
            describe(keys::GPU_USAGE, g, "GPU usage", Quantity::Percent, "none"),
            describe(
                keys::GPU_TEMPERATURE,
                g,
                "GPU temperature",
                Quantity::Celsius,
                "none",
            ),
            describe(
                keys::GPU_MEMORY_USED,
                g,
                "GPU memory used",
                Quantity::Bytes,
                "none",
            ),
            describe(keys::GPU_POWER, g, "GPU power", Quantity::Watts, "none"),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self { catalog }
    }
}

impl Provider for NoGpu {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.catalog.clone()
    }

    fn sample(&mut self, _now: Instant, out: &mut Snapshot) {
        for info in &self.catalog {
            put(out, info.key.as_str(), Reading::Unavailable(NO_GPU.into()));
        }
    }
}

/// Catalog entries of one GPU: `gpu.<n>.<suffix>` with `GPU <n> <what>` labels.
pub(crate) struct GpuCatalog {
    index: usize,
    source: String,
    entries: Vec<SensorInfo>,
}

impl GpuCatalog {
    /// Entries for GPU `index`, with `source` naming where values come from.
    pub(crate) fn new(index: usize, source: impl Into<String>) -> Self {
        Self {
            index,
            source: source.into(),
            entries: Vec::new(),
        }
    }

    /// Adds `gpu.<n>.<suffix>`.
    pub(crate) fn add(&mut self, suffix: &str, what: &str, quantity: Quantity) {
        let key = format!("gpu.{}.{suffix}", self.index);
        let label = format!("GPU {} {what}", self.index);
        self.entries.extend(describe(
            &key,
            Category::Gpu,
            label,
            quantity,
            self.source.clone(),
        ));
    }

    /// The entries.
    pub(crate) fn into_entries(self) -> Vec<SensorInfo> {
        self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub {
        catalog: Vec<SensorInfo>,
    }

    impl Provider for Stub {
        fn catalog(&self) -> Vec<SensorInfo> {
            self.catalog.clone()
        }
        fn sample(&mut self, _now: Instant, _out: &mut Snapshot) {}
    }

    fn found(pci: Option<&str>, integrated: bool) -> FoundGpu {
        FoundGpu {
            pci: pci.and_then(parse_pci),
            integrated,
            make: Box::new(move |n| {
                let mut c = GpuCatalog::new(n, if integrated { "igpu" } else { "dgpu" });
                c.add("usage", "usage", Quantity::Percent);
                c.add("name", "model", Quantity::Text);
                Box::new(Stub {
                    catalog: c.into_entries(),
                }) as Box<dyn Provider>
            }),
        }
    }

    #[test]
    fn gpus_are_numbered_by_pci_address_and_the_discrete_one_is_primary() {
        let gpus = number(vec![
            found(Some("0000:6a:00.0"), true),
            found(None, false),
            found(Some("0000:01:00.0"), false),
        ]);
        let sources: Vec<String> = gpus
            .providers
            .iter()
            .map(|p| p.catalog()[0].source.clone())
            .collect();
        assert_eq!(sources, ["dgpu", "igpu", "dgpu"]);
        assert_eq!(gpus.providers[1].catalog()[0].label, "GPU 1 usage");
        assert_eq!(
            gpus.aliases,
            [
                Alias {
                    key: SensorKey::new("gpu.usage").unwrap(),
                    target: SensorKey::new("gpu.0.usage").unwrap()
                },
                Alias {
                    key: SensorKey::new("gpu.name").unwrap(),
                    target: SensorKey::new("gpu.0.name").unwrap()
                },
            ]
        );
        assert_eq!(gpus.catalog[0].label, "GPU usage");
        assert_eq!(gpus.catalog[0].source, "gpu.0.usage (dgpu)");
    }

    #[test]
    fn an_integrated_gpu_is_primary_when_alone() {
        let gpus = number(vec![found(Some("0000:6a:00.0"), true)]);
        assert_eq!(gpus.aliases[0].target.as_str(), "gpu.0.usage");
    }

    #[test]
    fn without_gpus_the_well_known_keys_say_why() {
        let mut gpus = number(Vec::new());
        assert!(gpus.aliases.is_empty());
        assert_eq!(gpus.providers.len(), 1);
        let mut out = Snapshot::default();
        gpus.providers[0].sample(Instant::now(), &mut out);
        assert_eq!(out.len(), 4);
        let usage = out.get(&SensorKey::new(keys::GPU_USAGE).unwrap());
        assert_eq!(usage, Reading::Unavailable(NO_GPU.into()));
    }

    #[test]
    fn pci_addresses_parse() {
        assert_eq!(parse_pci("0000:6a:00.0"), Some((0, 0x6a, 0, 0)));
        assert_eq!(parse_pci("00000000:01:00.1"), Some((0, 1, 0, 1)));
        assert_eq!(parse_pci("card1"), None);
        assert_eq!(parse_pci("0000:zz:00.0"), None);
    }
}
