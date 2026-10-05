//! Windows providers (D-2026-09-30-sensors-3): sysinfo for CPU usage and
//! clocks, memory, disks, network and system; LibreHardwareMonitor's WMI
//! namespace or local web server for temperatures, fans and power. NVIDIA GPUs
//! come from NVML like on Linux.

mod embedded;
mod lhm_http;
mod sys;
mod wmi;

use crate::provider::Provider;

/// Every Windows provider except the GPUs, in catalog order.
pub(crate) fn providers() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(sys::Cpu::new()),
        Box::new(wmi::Lhm::new()),
        Box::new(sys::Memory::new()),
        Box::new(sys::Disks::new()),
        Box::new(sys::Network::new()),
        Box::new(sys::System::new()),
    ]
}
