//! Bezel sensors: the driven adapter behind the core's
//! [`SensorSource`](bezel_core::ports::SensorSource) port.
//!
//! [`SystemSensors`] composes internal providers. On Linux they read `/proc`
//! and `/sys` directly: CPU times, cpufreq and load, every hwmon chip (with
//! the CPU temperature picked by priority), RAPL, memory, network, disks and
//! mounts. [`FakeSensors`] replays scripted snapshots for tests and demos.
//!
//! What every provider guarantees (D-2026-09-30-sensors-1 and -4):
//! - a value that cannot be measured is `Reading::Unavailable(reason)`,
//!   never a guess;
//! - rates and usages come from counter deltas over the real time between
//!   two `sample` calls, so their first sample reads "warming up";
//! - a sample only reads local kernel files and never waits on the network.
#![forbid(unsafe_code)]

mod fake;
#[cfg(target_os = "linux")]
mod linux;
mod provider;
mod system;
#[cfg(test)]
#[cfg(target_os = "linux")]
mod testing;

pub use fake::FakeSensors;
pub use system::SystemSensors;
