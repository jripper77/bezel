# Phase 3: Sensores — Plan  (slug: sensors)

## Goal
Catálogo de sensores medido corretamente no Linux e no Windows, com `bezel sensors --watch`.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-sensors-1..4

## Tasks

### Wave 1

#### T-3.1: Core sensor domain and port
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{sensor,mod}.rs`, `crates/bezel-core/src/ports/mod.rs`
- **Acceptance:** `SensorKey`, well-known keys, `Quantity`, `Category`, `SensorInfo`, `Reading`, `Snapshot`, `rate` (real elapsed time), `format_reading` (°C/°F, GHz, binary/decimal bytes, durations), `fraction`; `SensorSource` port
- **Dependencies:** none
- **Test:** `cargo test -p bezel-core`
- **Status:** completed (611183b, 57f34d9)

### Wave 2

#### T-3.2: bezel-sensors crate, Linux providers
- **Files modified:** `crates/bezel-sensors/**` (new), `Cargo.toml` (member), `Cargo.lock`
- **Acceptance:** `/proc/stat` total + per-core usage, cpufreq, load average, `/proc/meminfo`, `/proc/net/dev` (per interface + aggregate, loopback excluded), `/proc/diskstats` rates + mount usage, uptime, hwmon (every chip: temps, fans, voltages, power; CPU temperature priority), RAPL; sysfs root injectable for tests
- **Dependencies:** T-3.1
- **Status:** completed (cdfca94)

#### T-3.3: GPU providers
- **Files modified:** `crates/bezel-sensors/src/{nvidia,amdgpu}.rs`
- **Acceptance:** NVML (usage, temperature, memory, power, clocks, fan, name) per GPU and as `gpu.*` primary (discrete first); amdgpu sysfs (busy %, VRAM, edge/junction temps, power, sclk)
- **Dependencies:** T-3.2
- **Status:** completed (6900a37)

#### T-3.4: Windows providers
- **Files modified:** `crates/bezel-sensors/src/windows/**`
- **Acceptance:** sysinfo CPU/memory/disks/network; LibreHardwareMonitor WMI (`root\LibreHardwareMonitor`, `Sensor` class) mapped to keys when present; `cargo check --target x86_64-pc-windows-gnu` or CI Windows job green
- **Dependencies:** T-3.2
- **Status:** completed (5c659dd; só compilado/clippy para Windows aqui, o job rust-windows do CI executa)

### Wave 3

#### T-3.5: CLI `bezel sensors`
- **Files modified:** `crates/bezel-cli/**`
- **Acceptance:** table grouped by category, `--json` (with `sampleMillis` under `--timing`), `--watch <secs>`; integration test through a hidden fake source
- **Dependencies:** T-3.2
- **Status:** completed (06f20bd)

## Execution
- Total tasks: 5
- Waves: 3

## Test requirements
- `cargo test --workspace --locked`, coverage ≥ 80%
- Manual comparison with `sensors`, `nvidia-smi`, `free` on the dev machine
