//! Linux providers: `/proc` and `/sys` read directly (D-2026-09-30-sensors-2).
//! Every path hangs off [`Roots`], so tests run against fake trees.

pub(crate) mod cpu;
pub(crate) mod disk;
pub(crate) mod fs;
pub(crate) mod hwmon;
pub(crate) mod memory;
pub(crate) mod net;
pub(crate) mod rapl;
pub(crate) mod system;

use std::path::PathBuf;

use crate::provider::Provider;

/// Where `/sys` and `/proc` live.
#[derive(Debug, Clone)]
pub(crate) struct Roots {
    /// `/sys`.
    pub(crate) sys: PathBuf,
    /// `/proc`.
    pub(crate) proc: PathBuf,
    /// libdrm's table of AMD GPU marketing names (`amdgpu.ids`).
    pub(crate) amdgpu_ids: PathBuf,
}

impl Roots {
    /// The running system's trees.
    pub(crate) fn host() -> Self {
        Self {
            amdgpu_ids: PathBuf::from("/usr/share/libdrm/amdgpu.ids"),
            ..Self::new("/sys", "/proc")
        }
    }

    /// Trees rooted elsewhere (tests), without a GPU name table.
    pub(crate) fn new(sys: impl Into<PathBuf>, proc: impl Into<PathBuf>) -> Self {
        Self {
            sys: sys.into(),
            proc: proc.into(),
            amdgpu_ids: PathBuf::new(),
        }
    }
}

/// Every Linux provider except the GPUs, in catalog order.
pub(crate) fn providers(roots: &Roots) -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(cpu::Cpu::new(roots)),
        Box::new(hwmon::Hwmon::new(roots)),
        Box::new(rapl::Rapl::new(roots)),
        Box::new(memory::Memory::new(roots)),
        Box::new(disk::Disks::new(roots)),
        Box::new(net::Network::new(roots)),
        Box::new(system::System::new(roots)),
    ]
}
