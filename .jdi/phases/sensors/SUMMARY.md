# Phase 3: Sensores — Summary  (slug: sensors)

**Status:** complete
**Tasks:** 5/5 complete, 0 blocked

> Nota de processo: executada fora do `/jdi-do` — T-3.1 pelo orquestrador; T-3.2..T-3.5 por um agente
> genérico num worktree, com commits cherry-picked para a `main`. SUMMARY escrito ao final a partir do
> relatório do agente e de medições refeitas agora.

## Executed tasks
- T-3.1: domínio de sensores no core — `SensorKey` (chaves estáveis + abertas `hwmon.<chip>.<label>`,
  `disk.<mount>.*`, `net.<iface>.*`), `Reading::{Value,Text,Unavailable(motivo)}`, `rate` sobre o tempo real
  decorrido, `format_reading` (°C/°F, bytes binários/decimais), `fraction`; porta `SensorSource` —
  `611183b`, `57f34d9`
- T-3.2: crate `bezel-sensors` com provedores Linux lendo `/proc` e `/sys` direto: CPU (uso total e por
  núcleo por delta de `/proc/stat`, cpufreq, load), todos os chips hwmon (temperatura da CPU por
  prioridade), RAPL, memória/swap, rede, discos (taxas e espaço via `statvfs`, igual ao `df`), sistema —
  `cdfca94`
- T-3.3: GPUs NVIDIA (NVML carregada em tempo de execução) e AMD (sysfs do amdgpu), com apelidos da GPU
  principal (`gpu.*`) — `6900a37`
- T-3.4: Windows sobre sysinfo + WMI do LibreHardwareMonitor (temperaturas, ventoinhas, potência) —
  `5c659dd`
- T-3.5: `bezel sensors` (tabela por categoria, `--json`, `--watch`, `--count`, `--timing`) — `06f20bd`

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/domain/sensor.rs` (+ `Quantity::Amperes`, fora da lista do PLAN, para correntes do hwmon)
- `crates/bezel-sensors/**`, `crates/bezel-cli/src/{lib,main,sensors}.rs`, `crates/bezel-cli/tests/sensors.rs`
- `Cargo.toml`, `Cargo.lock`

## Tests
- `bezel-sensors`: 44 testes (árvores sysfs/proc falsas, respostas WMI/NVML simuladas), todos passando;
  workspace inteiro: 324 passando
- Coverage: `bezel-sensors` 98,74% de linhas; `bezel-cli/src/sensors.rs` 98,41%;
  `bezel-core/src/domain/sensor.rs` 98,28% (`cargo llvm-cov --workspace --summary-only`)
- DoD auto: `linux::hwmon::tests::cpu_temperature_follows_the_priority`,
  `linux::cpu::tests::usage_from_proc_stat_deltas`, `sensors_json_lists_fake_readings` e amostra < 50 ms
  (6–16 ms para 167 sensores nesta máquina)

## Hardware validation
Máquina do dev (Ryzen com iGPU Raphael + RTX 4090, 2× DDR5, 2 NVMe), comparado com as ferramentas de
referência amostradas ao mesmo tempo:

| Sensor | Bezel | Referência |
|---|---|---|
| CPU k10temp Tctl | 47,125 °C | `sensors` 47,125 |
| Tccd1 / Tccd2 | 42,6 / 38,9 | 42,75 / 39,25 (variam entre leituras) |
| asusec CPU Package / VRM | 47 / 41 | 47 / 41 |
| nvme0 / nvme1 Composite | 34,85 / 39,85 | 34,85 / 39,85 |
| DDR5 spd5118 ×2 | 42,25 / 42,0 | 42,25 / 42,0 |
| RTX 4090 temp/uso/mem/potência/clock/fan | 34 °C / 38% / 2764 MiB / 25,6 W / 870 MHz / 0% | `nvidia-smi` 34 / 38 / 2765 / 25,58 / 870 / 0 |
| RAM total | 66 507 341 824 B | `free -b` idêntico (usada/disponível diferem 17 MB pelo intervalo) |
| Swap, `/`, `/boot`, `/boot/efi` | — | idênticos a `free`/`df -B1` |
| Load average | 8,18 5,95 3,59 | idêntico |

Indisponíveis nesta máquina (com o motivo na leitura, nunca um chute): `cpu.power` (RAPL `energy_uj` só
root), ventoinhas (nenhum chip as expõe; o 0% da 4090 é real, ventoinha parada). O PPT do amdgpu no
Raphael é potência do pacote inteiro e está rotulado assim.

**Pendente de confirmação humana (DoD manual):** conferir os valores contra `sensors`, `nvidia-smi` e
`free` — a tabela acima é a evidência.

## Follow-ups
- O importador de temas (render-engine) usa chaves que os provedores ainda não publicam: `cpu.fan`,
  `fan.pump`, `fan.case*`, `gpu.fps`, `net.ping`, `net.*.total`, `memory.available.percent`,
  `system.volume`, `cpu.voltage`/`gpu.voltage`. Em temas importados aparecem como indisponíveis; alinhar
  na phase `release-polish` (FPS/ping já estão no roadmap dela).
