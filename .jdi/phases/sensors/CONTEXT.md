# Phase 3: Sensores — Context  (slug: sensors)

## Goal
Catálogo de sensores medido corretamente no Linux (sysinfo, hwmon, NVML, amdgpu) e no Windows (sysinfo, NVML, LibreHardwareMonitor via WMI), com `bezel sensors --watch`.

## Locked decisions
- D-2026-09-30-sensors-1: chaves estáveis (conhecidas + abertas); `Unavailable(motivo)` em vez de chute; formatação no core.
- D-2026-09-30-sensors-2: Linux lê `/proc` e `/sys` direto; prioridade de temperatura da CPU; NVML carregado em runtime.
- D-2026-09-30-sensors-3: Windows = sysinfo + NVML + WMI do LibreHardwareMonitor quando presente; HWiNFO/RTSS depois.
- D-2026-09-30-sensors-4: amostragem não bloqueante, taxas pelo tempo real, < 50 ms por amostra.

## Canonical refs
- `docs/reverse-engineering/sensors.md` (catálogo das duas referências e os bugs de medição a não repetir)
- Máquina do dev: Ryzen 9 7900X3D (k10temp Tctl/Tccd1/Tccd2), asusec (CPU, CPU Package, Motherboard, VRM), amdgpu (iGPU edge/power/freq), RTX 4090 (NVML), 2× NVMe, 2× spd5118 (DDR5), RAPL powercap.

## Out of scope
- FPS de jogos, clima, ping, dados customizados (phase `release-polish`)

## Definition of Done

### Auto-verifiable
- [ ] hwmon parsing picks the CPU temperature by priority from a fake sysfs tree
      **Verify:** `cargo test -p bezel-sensors --locked -- --exact linux::hwmon::tests::cpu_temperature_follows_the_priority 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Per-core CPU usage comes from /proc/stat deltas
      **Verify:** `cargo test -p bezel-sensors --locked -- --exact linux::cpu::tests::usage_from_proc_stat_deltas 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] `bezel sensors --json` prints the fake source's readings
      **Verify:** `cargo test -p bezel --locked --test sensors -- --exact sensors_json_lists_fake_readings 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] A full sample on this machine takes under 50 ms
      **Verify:** `cargo run -q --release -p bezel -- sensors --json --timing 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["sampleMillis"] < 50, d["sampleMillis"]; print("OK")'`
      **Source:** CONTEXT

### Manual
- [ ] Values on the dev machine match `sensors`, `nvidia-smi` and `free` within rounding
      **Verify:** human confirmation required
      **Evidence:** side-by-side output in SUMMARY.md § Hardware validation
      **Source:** CONTEXT

## Notes
- Crate novo `crates/bezel-sensors` (driven adapter), `SystemSensors` compõe provedores internos; `FakeSensors` para testes e modo demo.
