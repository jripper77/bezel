//! Bezel sensors: the driven adapter behind the core's
//! [`SensorSource`](bezel_core::ports::SensorSource) port.
//!
//! [`SystemSensors`] composes internal providers. On Linux they read `/proc`
//! and `/sys` directly: CPU times, cpufreq and load, every hwmon chip (with
//! the CPU temperature picked by priority), RAPL, memory, network, disks and
//! mounts, and AMD GPUs through amdgpu's sysfs files. NVIDIA GPUs come from
//! NVML, loaded at run time. On Windows, sysinfo measures CPU, memory,
//! disks and network, and LibreHardwareMonitor's WMI namespace or local
//! web server the temperatures, fans and power. On both, `gpu.fps` comes from a
//! frame-rate overlay already running (MangoHud's logs on Linux, RivaTuner
//! Statistics Server's shared memory on Windows) and `net.ping` from a probe
//! thread of its own, which sends packets only while a shown value uses the
//! ping (`SensorSource::want`, D-2026-09-30-release-polish-11;
//! [`SensorOptions`] chooses the host and the MangoHud folder).
//! [`FakeSensors`] replays scripted snapshots for tests and demos.
//!
//! What every provider guarantees (D-2026-09-30-sensors-1 and -4):
//! - a value that cannot be measured is `Reading::Unavailable(reason)`,
//!   never a guess;
//! - rates and usages come from counter deltas over the real time between
//!   two `sample` calls, so their first sample reads "warming up";
//! - a sample only reads local files (kernel, logs) and memory, never waits on
//!   the network (the ping thread stores its last result; a sample reads it).
//!
//! `unsafe` is denied (the workspace lint); the one exception is the Win32
//! code that maps RTSS's shared memory read-only (`fps::rtss`, Windows only),
//! each call allowed in the smallest scope with its `// SAFETY:` reasoning.
#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod amdgpu;
mod fake;
mod fps;
mod gpu;
#[cfg(any(windows, test))]
mod lhm;
mod libre_control;
#[cfg(target_os = "linux")]
mod linux;
mod nvidia;
mod ping;
mod provider;
#[cfg(windows)]
mod sensor_log;
mod shared;
mod system;
#[cfg(test)]
#[cfg(target_os = "linux")]
mod testing;
pub mod weather;

#[cfg(windows)]
mod windows;

pub use fake::FakeSensors;
pub use libre_control::restart_libre_reader;
pub use shared::SharedSensors;
pub use system::{SensorOptions, SystemSensors};
