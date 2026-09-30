# Phase 3: Review  (slug: sensors)

**Verdict:** APPROVED_PENDING_MANUAL

> Revisão em modo `verify` (2026-09-30), `HEAD` = `6b31ad1`. Os commits da fase são `611183b`, `57f34d9`, `cdfca94`,
> `6900a37`, `5c659dd` e `06f20bd`. A fase foi executada fora do `/jdi-do` (o SUMMARY declara isso), então julguei o
> código e os artefatos entregues contra PLAN, CONTEXT (DoD), PROJECT e as decisões. Nenhum comando abriu a tela
> real. Rodei só testes, `bezel sensors` (leitura de `/proc`, `/sys` e NVML) e nenhum teste `#[ignore]`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: ok, lock aceito |
| Tests | PASS | 324 passed, 0 failed, 2 ignored (hardware). Igual ao SUMMARY (324); `bezel-sensors` sozinho tem 44 |
| Coverage | PASS | 94.48% lines (TOTAL, sem `main.rs`/`build.rs`); 94.41% sem o filtro (comando do DoD). `bezel-sensors` agregado: 98.74% (bate com o SUMMARY) |
| Lint | PASS | fmt + clippy -D warnings limpos no Linux. `cargo clippy -p bezel-sensors` e `-p bezel --target x86_64-pc-windows-gnu --all-targets -- -D warnings` também limpos. Os 6 `allow` do grep vêm de módulos de teste do `bezel-themes`, fora da fase (W11) |
| Hexagonal/Safety/Protocol/Hygiene | PASS (com WARN) | 5.1–5.11 sem BLOCK. Ver W3–W9 |
| Consistency | PASS (com WARN) | as 5 tarefas têm commit com escopo `sensors`; D-2026-09-30-sensors-1..4 conformes, com ressalvas de texto (W1, W2, W8) e processo (W10) |
| UI Validation | SKIPPED | `frontend.has_frontend` ausente no PROJECT.md; só liga a partir de `studio-app` (D-2026-09-30-studio-app-5) |
| DoD | PENDING MANUAL | 7/7 itens Auto PASS; 3 itens Manual pendentes |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O, threads, cfg de plataforma no core | PASS: nada encontrado. `domain/sensor.rs` usa só `Duration`; o `Instant` fica no adapter |
| 5.3 ports | PASS: `SensorSource` está em `bezel_core::ports` (`ports/mod.rs:50`) e é implementado só por `SystemSensors` e `FakeSensors`. `Provider` (`provider.rs`) é `pub(crate)`, auxiliar interno do adapter |
| 5.4 adapters só na composição | PASS: `SystemSensors::new()` só aparece em `crates/bezel-cli/src/main.rs:18` e `apps/bezel-studio/src-tauri/src/lib.rs:153`, que são composition roots. A lib do CLI recebe `&mut dyn SensorSource` |
| 5.5 `unsafe` | PASS: `#![forbid(unsafe_code)]` em `bezel-sensors/src/lib.rs:19`; a única ocorrência no workspace é a string `"unsafe asset path"` (`bezel-themes`) |
| 5.6 panics fora de teste | PASS: todas as ocorrências da fase estão em `#[cfg(test)]`, incluindo `testing.rs`, que só compila em teste (`lib.rs:32-34`) |
| 5.7 segurança de escrita no device | PASS: nenhum `Confirm::Yes`, nenhum transporte aberto em `crates/*/tests`. A fase não toca em device |
| 5.8 fidelidade de protocolo | PASS: a fase não mexe em `protocol/`, e o laço do gate não aponta nada |
| 5.9 caminhos de device no core | PASS (com nota): em `domain/sensor.rs:7,136,179` aparecem só doc comments com o namespace de chave `hwmon.<chip>.<label>` (D-2026-09-30-sensors-1). Nenhuma lógica do core depende de hwmon |
| 5.10 comandos Tauri síncronos | n/a para esta fase (os 3 hits são de `studio-app`, já avaliados antes) |
| 5.11 supply chain e segredos | WARN (W11): as dependências novas da fase (`nvml-wrapper 0.13.0`, `rustix 1.1.5`, `sysinfo 0.39.6`, `wmi 0.18.4`) passam limpas no audit. Os 2 avisos são de dependências de outras fases. Nenhum segredo |

Leitura do hexágono: os números saem crus dos adapters e o core cuida de chave, unidade, taxa (`rate`) e formatação
(`format_reading`, °C/°F, bytes binários/decimais, duração). O CLI formata pelo core e só traduz para tabela e JSON
(`SampleDto`/`SensorDto`). Dois pontos ficam no adapter: a prioridade da temperatura da CPU e a escolha da GPU
principal. Isso é conhecimento de plataforma (qual chip do hwmon ou do WMI é a CPU, qual placa é discreta), e o
D-2026-09-30-sensors-2 e a T-3.3 põem essas regras lá. Não é regra de negócio vazando.

## Blockers
Nenhum.

## Warnings
- **W1. D-2026-09-30-sensors-1 ("nunca um chute"), frequência da CPU no Windows:** `windows/sys.rs:98-105` publica
  `cpu.frequency()` do sysinfo como `Reading::Value`. Quando `CallNtPowerInformation` falha, o sysinfo 0.39.6 devolve
  `0` para cada CPU (`sysinfo-0.39.6/src/windows/cpu.rs:514`), e o Bezel mostraria `0 MHz` como se fosse medido. O
  `CurrentMhz` do Windows também costuma ficar preso no clock base. Classifiquei como WARN, não BLOCK, porque o zero
  vem de um caminho de falha dentro da biblioteca, o código Windows não roda aqui e a T-3.4 aceita "só compilado".
  Correção de uma linha: `0` vira `Unavailable("Windows did not report the clock")`. Validar os valores quando a fase
  rodar no Windows.
- **W2. Texto do D-2026-09-30-sensors-2 × código (k10temp):** a decisão diz "k10temp Tctl". O código prefere `Tdie`
  a `Tctl` quando o chip expõe os dois (`linux/hwmon.rs:25-31`, teste em `:548-553`). Isso acontece no Zen 1/Zen+,
  onde o `Tctl` tem offset de +10/+20/+27 °C. O código segue `docs/reverse-engineering/sensors.md` §§ 4 e 6.1
  ("`k10temp` `Tctl`/`Tdie`… document the `Tctl` offset") e, na máquina do dev (Zen 4, só `Tctl`), dá o mesmo
  resultado. Não chega a contradizer a intenção da decisão, mas o texto dela ficou impreciso. Para rastreabilidade,
  vale registrar a preferência por `Tdie` num novo D-XX ou numa nota na decisão.
- **W3. GPU AMD suspensa acordada pelo provider hwmon:** `amdgpu.rs:7-9` e `:278-280` evitam ler a placa em runtime
  suspend ("touching its files would wake it up"). Só que `linux/hwmon.rs:403-420` lê todo chip em
  `/sys/class/hwmon` a cada amostra, inclusive o `amdgpu` (e o `nouveau`) da mesma placa, sem essa checagem. Em
  notebook híbrido com kernel que faz `pm_runtime_get_sync` na leitura do hwmon, cada amostra acorda a dGPU. A
  proteção do provider amdgpu fica sem efeito. Sugestão: pular (ou dar `Unavailable`) chips cujo
  `device/power/runtime_status` seja `suspended`.
- **W4. DRY, chaves estáveis repetidas entre plataformas:** `cpu.name`, `memory.available`, `memory.swap.*`,
  `system.hostname`, `cpu.load.5/15` e `net.downloaded/uploaded` são constantes em `linux/cpu.rs:18-22`,
  `linux/memory.rs:16-22`, `linux/system.rs:13` e `linux/net.rs:20-22`, que só compilam no Linux. No Windows elas
  aparecem como literais (`windows/sys.rs:49`, `:119`, `:148`, `:150`, `:194`, `:197`, `:512`, `:536`). O `DEMO` de
  `fake.rs` usa literais até para chaves que já existem em `bezel_core::domain::sensor::keys` (ex.: `"cpu.usage"` em
  `fake.rs:101`). As chaves são o contrato dos temas (D-2026-09-30-sensors-1), e um erro de digitação num lado quebra
  temas só naquela plataforma. Levar as chaves multiplataforma para `keys` no core ou para um módulo comum do crate. A
  razão `"counter reset or no time elapsed"` também se repete 7 vezes (`linux/disk.rs:332,357`,
  `linux/net.rs:208,266`, `windows/sys.rs:347,465,494`), ao lado de `WARMING_UP`, que já é constante.
- **W5. DRY, unidades do hwmon em dois lugares e em conflito:** `linux/hwmon.rs:81-89` (`Kind::convert`) e
  `amdgpu.rs:36-45` (`Unit::convert`) codificam a mesma ABI (m°C, µW, Hz→MHz, PWM/255). E divergem na potência: o
  hwmon prefere `power1_input` (`hwmon.rs:162`, teste em `:622`) e o amdgpu prefere `power1_average`
  (`amdgpu.rs:188-190`). Na mesma placa, `hwmon.amdgpu.ppt` e `gpu.<n>.power` podem mostrar números diferentes.
  Escolher uma regra só.
- **W6. Clean code, funções longas:** `linux/net.rs:187` `sample` (94 linhas), `linux/disk.rs:282` `sample_io` (81),
  `amdgpu.rs:179` `new` (98), `linux/net.rs:74` `new` (92), `linux/hwmon.rs:320` `new` (73) e `lhm.rs:143`
  `discover` (61). Os samplers de rede e disco misturam taxa por dispositivo, agregado e "sumiu/aqueceu" na mesma
  função. Há também `let _ =` sem motivo escrito em `windows/wmi.rs:102` e `:130`.
- **W7. YAGNI:** `SystemSensors::with_roots` (`system.rs:33`) é `pub`, mas só o teste do próprio crate usa
  (`system.rs:175`). O mesmo vale para `FakeSensors::samples_taken` (`fake.rs:52`), usado só pelo teste dele. Deixar
  `pub(crate)`/`#[cfg(test)]` ou remover.
- **W8. Documentação × comportamento:** o doc de `keys::NET_DOWN`/`NET_UP` diz "all non-loopback interfaces"
  (`bezel-core/src/domain/sensor.rs:62-65`). O adapter soma só interfaces físicas, sem bridges, veth, VPN e
  Tailscale (`linux/net.rs:4-7`). O comportamento é o melhor dos dois; o doc do core é que precisa mudar. No Windows
  surgiu o namespace aberto `lhm.<identifier>` (`lhm.rs:1-5`), que não está na lista do D-2026-09-30-sensors-1, e um
  tema que usa `hwmon.*` não funciona no Windows. Registrar.
- **W9. Diagnóstico invisível:** o filtro do `-v` em `crates/bezel-cli/src/main.rs:34`
  (`bezel=debug,bezel_devices=debug,bezel_core=debug`) deixa `bezel_sensors` de fora. Com isso, as mensagens de NVML
  e LHM e o `tracing::error!` de provider que entrou em pânico (`system.rs:125`) nunca aparecem, nem com `-v`.
- **W10. Windows nunca executado, e processo:** `windows/sys.rs` (538 linhas) e `windows/wmi.rs` (163) não têm teste
  unitário. Quem os exercita é só `this_machine_answers_for_every_catalog_entry`, no job `rust-windows` do CI, e esse
  job ainda não rodou nestes commits: a `origin/main` está em `80270f2` e a fase não foi enviada. O critério da
  T-3.4 está cumprido pelo clippy para `x86_64-pc-windows-gnu`. Ainda no Windows, a mensagem `NO_GPU`
  (`gpu.rs:112`) fala em "amdgpu" numa máquina onde a GPU AMD só aparece como `lhm.*`. Processo: a fase rodou fora do
  `/jdi-do` (declarado); `cdfca94` mexe em `bezel-core/src/domain/sensor.rs` (`Quantity::Amperes`), fora da lista da
  T-3.2 (declarado); `gpu.rs`, `system.rs` e `lhm.rs` ficam fora das listas da T-3.3 e da T-3.4 (`lhm.rs` está fora
  de `windows/**` de propósito, para testar no Linux).
- **W11. Fora da fase (gates 4 e 5.11), igual ao W8/W9 de `device-protocols`:** os 6 `#[allow(clippy::panic)]` sem
  `// reason:` acima deles em `crates/bezel-themes/src/import/*.rs` (render-engine), sobre `#[cfg(test)] mod tests`.
  E o `cargo audit` segue apontando `ttf-parser 0.25.1` sem manutenção (RUSTSEC-2026-0192) e `yoke-derive 0.8.3`
  yanked. Nenhum dos dois conta contra esta fase.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `cargo test --workspace --locked`: exit 0, 324 passed, 0 failed, 2 ignored |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `cargo llvm-cov --workspace --locked --fail-under-lines 80 --summary-only`: exit 0, TOTAL 94.41% lines |
| 3 | No `TODO`/`FIXME` without linked issue | PROJECT | Auto | PASS | comando `git grep` do DoD: `OK` |
| 4 | hwmon parsing picks the CPU temperature by priority from a fake sysfs tree | CONTEXT | Auto | PASS | `cargo test -p bezel-sensors --locked -- --exact linux::hwmon::tests::cpu_temperature_follows_the_priority`: `OK` |
| 5 | Per-core CPU usage comes from /proc/stat deltas | CONTEXT | Auto | PASS | `cargo test -p bezel-sensors --locked -- --exact linux::cpu::tests::usage_from_proc_stat_deltas`: `OK` |
| 6 | `bezel sensors --json` prints the fake source's readings | CONTEXT | Auto | PASS | `cargo test -p bezel --locked --test sensors -- --exact sensors_json_lists_fake_readings`: `OK` |
| 7 | A full sample on this machine takes under 50 ms | CONTEXT | Auto | PASS | comando do DoD (`cargo run -q --release -p bezel -- sensors --json --timing` + asserção em Python) 5 vezes: `OK` com 11, 15, 12, 12 e 11 ms para 167 sensores. `--watch 0.5 --count 3`: 12, 16 e 6 ms |
| 8 | Values on the dev machine match `sensors`, `nvidia-smi` and `free` within rounding | CONTEXT | Manual | MANUAL_REQUIRED | A evidência está em SUMMARY § Hardware validation. Nesta revisão, conferi as leituras lado a lado, só lendo: `cpu.temperature` 47.0 × Tctl 47.0; ccd1/ccd2 42.25/38.375 × 41.8/38.4 (variam entre leituras); asusec CPU Package/VRM 47/40 × 47/40; nvme 35.85/39.85 × 35.9/39.9; spd5118 41.25/41.0 × 41.2/41.0; RTX 4090 35 °C/37%/2816 MiB/25.8 W × `nvidia-smi` 35/37/2816/25.70; `memory.total` 66507341824 × `free -b` idêntico. Falta a confirmação humana, com uma linha "conferido por <nome> em <data>" no SUMMARY |
| 9 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED | `## [Unreleased]` não tem entrada para a fase. Acrescentar em Added: "`bezel sensors` (tabela, `--json`, `--watch`, `--timing`) com o catálogo de sensores de Linux (hwmon, RAPL, amdgpu, NVML) e Windows (sysinfo, NVML, LibreHardwareMonitor)". Confirmar no `/jdi-ship` |
| 10 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | `README.md:7-9` diz que "sensors … are landing", e o Quick start não tem `bezel sensors`. Atualizar o status e acrescentar `bezel sensors --watch 1`; citar o LibreHardwareMonitor como requisito de temperatura e potência no Windows e o RAPL restrito ao root no Linux |

## Recommendation
Nenhum bloqueio. Build, testes (324), cobertura (94,48%), lint (inclusive o clippy para Windows), as checagens
hexagonais e as quatro decisões da fase passam. Os 7 itens Auto do DoD também passam, e a amostra completa leva de 6
a 16 ms, contra o limite de 50 ms. Antes do `/jdi-ship`:

1. Confirmar os valores (item 8) e registrar a confirmação no SUMMARY.
2. Acrescentar a entrada no CHANGELOG e o `bezel sensors` no README (itens 9 e 10).
3. Recomendado, num commit `fix(sensors)`: MHz zero no Windows vira `Unavailable` (W1), o hwmon deixa de ler chip de
   GPU suspensa (W3) e o `-v` passa a incluir `bezel_sensors` (W9).
4. Recomendado, num `refactor(sensors)`: chaves multiplataforma e razões repetidas em constantes únicas (W4), uma
   regra só de unidades e de potência do hwmon (W5), samplers de rede e disco quebrados em funções menores (W6) e
   visibilidade de `with_roots`/`samples_taken` (W7).
5. Ajustar o texto do D-2026-09-30-sensors-2 (preferência por `Tdie`, W2) e o doc de `NET_DOWN`/`NET_UP` e o
   namespace `lhm.*` (W8).
6. Enviar os commits para o CI rodar o job `rust-windows` (W10).
