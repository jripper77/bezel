# Phase 2: Review  (slug: device-protocols)

**Verdict:** APPROVED_PENDING_MANUAL

> Revisão em modo `verify` (2026-09-30), `HEAD` = `9baa5ba` (a fase vai até `87da4b3`). A fase foi executada fora
> do `/jdi-do` (declarado no SUMMARY) e foi julgada pelo código e pelos artefatos entregues contra PLAN, CONTEXT
> (DoD), PROJECT e as decisões. Nenhum comando abriu a tela real: só testes e fakes, nenhum teste `#[ignore]`
> executado (a 8.8" continua com o `turing-smart-screen.service`).

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked` ok (lock aceito) |
| Tests | PASS | 324 passed, 0 failed, 2 ignored (hardware); igual ao SUMMARY (324) |
| Coverage | PASS | 94.48% lines (TOTAL, `main.rs`/`build.rs` excluídos); 94.41% sem o filtro (comando do DoD) |
| Lint | PASS | fmt + clippy -D warnings limpos; os 6 `allow` do grep são de módulos de teste do `bezel-themes` (fora da fase, W8) |
| Hexagonal/Safety/Protocol/Hygiene | PASS (com WARN) | 5.1–5.11 sem BLOCK; adapter chamando adapter (W1), DRY/YAGNI/clean code (W2–W5), audit (W9) |
| Consistency | PASS (com WARN) | os 22 commits do PLAN existem com escopo `device-protocols`; D-2026-09-30-device-protocols-1..5 conformes; lacuna HID/AD11 (W6), processo (W7) |
| UI Validation | SKIPPED | `frontend.has_frontend` ausente no PROJECT.md; liga só a partir de `studio-app` (D-2026-09-30-studio-app-5) |
| DoD | PENDING MANUAL | 6/6 itens Auto PASS; 3 itens Manual pendentes |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O, threads, cfg de plataforma no core | PASS: sem ocorrências |
| 5.3 ports | PASS: nenhum port implementado no core. `Wire` (`wire.rs:12`), `Pause` (`driver/turing_rev_c.rs:30`) e `Clock` (`driver/turing_usb.rs:47`) são auxiliares internos do adapter; o core não precisa deles |
| 5.4 adapters só na composição | PASS: `SystemBus`/`SystemConnector`/`SkiaRenderer` só são construídos em `crates/bezel-cli/src/main.rs`; ver W1 para o uso interno de `SystemBus` |
| 5.5 `unsafe` | PASS: a única ocorrência é a string `"unsafe asset path"` (`bezel-themes/src/native.rs:39`) |
| 5.6 panics fora de teste | PASS: toda ocorrência está em `#[cfg(test)]` (incluindo `bezel-sensors/src/testing.rs` e `bezel-render/src/{golden,testkit}.rs`, que são módulos `#[cfg(test)]`) |
| 5.7 segurança de escrita no device | PASS: nenhum `Confirm::Yes`; nenhum transporte real aberto em `crates/*/tests`. Os testes unitários só abrem caminhos inexistentes (`/dev/bezel-no-such-port`, `usb:bezel-none-1.2`) |
| 5.8 fidelidade de protocolo | PASS: os 7 encoders têm testes golden. Conferi com a spec: rev C HELLO `01 ef 69 00 00 00 01 00 00 00 c5 d3`, SET_BRIGHTNESS `7b ef 69 … <L>`, cabeçalho de frame cheio do fabricante `c8ef6900384000000000` e limite ROM ≥ 1.89 (`protocol-turing-rev-c.md` §§ 4, 9.2, 17); chave WCH `41 5f d9 fa 13 42 58 b7` e as 11 cifras (`protocol-wch.md` §§ 3, 10); Turing USB: chave = IV = `slv3tuzx`, PKCS#7 `04×4`, trailer `a1 1a`, brilho máximo 102 (`protocol-turing-usb.md` §§ 3, 5) |
| 5.9 caminhos de device no core | PASS (com nota): as ocorrências são doc comments de `cd2fcbad` (foundation: `domain/device.rs:75`, `domain/discovery.rs:11`), endereços opacos de fixtures de teste (`discovery.rs:248-309`) e doc comments de sensores. Nenhuma lógica depende de caminho de plataforma |
| 5.10 comandos Tauri síncronos | PASS: `list_fonts` (clona estado em cache), `get_autostart` e `set_autostart` não chegam em device, sensor nem renderer (fase studio-app) |
| 5.11 supply chain e segredos | WARN: ver W9. Nenhum segredo encontrado |

## Blockers
Nenhum.

## Warnings
- **W1. Hexagonal, adapter chamando adapter (regra 15):** `crates/bezel-devices/src/connector.rs:97` e `:165`.
  `SystemConnector` usa o adapter concreto `SystemBus` para reenumerar durante a espera e o wake do rev C, em vez
  de receber um `DeviceBus`. Não virou BLOCK por três motivos: os dois adapters estão no mesmo crate e usam a mesma
  tecnologia (enumeração USB); o laço temporizado não pode ir para o core, porque o check 5.2 proíbe
  `Instant`/thread lá; e D-2026-09-30-foundation-3 põe o wake no caminho de connect desta fase. O problema prático é
  que o caminho de wake/retry não tem teste com fakes (`connector.rs` tem 69,23% de linhas). Sugestão: deixar
  `SystemConnector` genérico sobre `B: DeviceBus` (injetado na composição) e com um `Pause` injetado, como os
  drivers já fazem.
- **W2. DRY, RGB565:** a mesma fórmula aparece em `protocol/turing_rev_a.rs:147`, `protocol/weact.rs:162` e
  `protocol/kipye_rev_d.rs:138`. Os conversores também se repetem: `rgb565_le` em `turing_rev_a.rs:152` e
  `weact.rs:168`, `rgb565_be` em `kipye_rev_d.rs:144` e `xuanfang_rev_b.rs:136`. É o mesmo conhecimento de
  `pixel-formats.md` § 1 (regra de 3 atingida). Extrair para um módulo `protocol::pixel` junto com o teste da
  tabela de cores.
- **W3. DRY, auxiliares dos drivers:** `fn io_err` é idêntico nos 7 drivers (`driver/turing_rev_a.rs:38`,
  `turing_rev_c.rs:55`, `kipye_rev_d.rs:51`, `wch.rs:56`, `xuanfang_rev_b.rs:57`, `weact.rs:59`,
  `turing_usb.rs:76`). A checagem "frame is WxH, the screen expects …" também se repete nos 7 (ex.:
  `driver/turing_rev_c.rs:111-122`). Essa checagem é o contrato de `ScreenLink::present` e deveria ter uma única
  implementação, e o erro sai como `BezelError::Transport`, que não descreve um erro do chamador. `Pause`/`RealTime`
  moram em `driver/turing_rev_c.rs`, mas todos os drivers usam; melhor ficarem em `driver/mod.rs`.
- **W4. YAGNI:** `protocol/turing_rev_c.rs:261` `NO_CHANGE` não tem chamador nem teste. O driver não envia nada
  quando o diff é vazio, enquanto o app do fabricante manda esse dummy a cada tick (spec § 9.2). O caminho é usar a
  constante (se a tela precisar de keep-alive) ou removê-la. `Confirm` (`domain/screen.rs`) também ainda não tem
  chamador; é aceitável como contrato do D-2 para a fase `storage-video`, mas fica registrado.
- **W5. Clean code:** o literal `1024` (o "R 1024" da spec) aparece 4 vezes em `driver/turing_rev_c.rs` (ex.:
  `:101`) sem constante nomeada, ao lado de `REPLY_TIMEOUT`, que tem nome.
- **W6. Escopo CONTEXT × PLAN (HID):** o goal (ROADMAP/CONTEXT) cita "HID" e "transportes serial/USB/HID", e o Out
  of scope diz "HID Desktop mode do 1A86:AD11 … (só identificação nesta phase)". Nenhum código identifica
  `1a86:ad11` (nada no catálogo nem na discovery) e o PLAN não tem tarefa para isso. Não é item de DoD. Registrar
  como todo ou mover explicitamente para uma fase futura.
- **W7. Processo:** (a) a fase rodou fora do `/jdi-do`, com commits intercalados aos de outras fases (declarado no
  SUMMARY); (b) várias tarefas têm mais de um commit (a T-2.8 tem 7), contra a regra "1 task = 1 commit"; (c) `d74fb81`
  toca `apps/bezel-studio/src-tauri/src/dto.rs` e `apps/bezel-studio/src/demo-data.js`, fora da lista do PLAN
  (declarado no SUMMARY).
- **W8. Gate 4, fora da fase:** há `#[allow(clippy::panic)]` sem linha `// reason:` acima em
  `crates/bezel-themes/src/import/colors.rs:238`, `mod.rs:282`, `nrbf.rs:961`, `python_yaml.rs:1073`,
  `turzx.rs:1183` e `yaml.rs:251` (commit `504ab76`, render-engine). Todos ficam sobre `#[cfg(test)] mod tests`,
  que é código de teste (o grep do gate pretende isentar teste), e trazem a razão como comentário na mesma linha.
  Não conta contra esta fase. A revisão de `render-engine` deve normalizar para `// reason:` ou confirmar a isenção.
- **W9. Supply chain (5.11):** o `cargo audit` aponta `ttf-parser 0.25.1` sem manutenção (RUSTSEC-2026-0192) e
  `yoke-derive 0.8.3` com versão yanked. Nenhum dos dois é dependência direta desta fase. Reavaliar na fase
  `render-engine` ou no `.cargo/audit.toml`.

Observação fora do escopo (não conta nesta fase): `crates/bezel-core/src/app/runtime.rs` tem 0,00% de linhas
cobertas e `ThemeRuntime` só aparece no re-export `app/mod.rs:6`. Passar para as revisões de `render-engine`
(D-2026-09-30-render-engine-5) e `studio-app` (D-2026-09-30-studio-app-4).

Cobertura dos arquivos da fase: protocolos 97,9–100%, drivers 95,3–99,0%, `busy.rs` 98,2%, `fake.rs` 100%,
`cli/screen.rs` 98,4%. Abaixo de 80% ficam `wire.rs` (63,4%), `usb.rs` (63,7%) e `connector.rs` (69,2%), todos
caminhos de I/O real, e `catalog.rs` (56,3%). Em `catalog.rs` o número é artefato: as tabelas `const` são avaliadas
em tempo de compilação e as linhas de `const fn` nunca executam em runtime.

Conformidade com as decisões:
- **D-2026-09-30-device-protocols-1:** o RGBA na orientação do tema é girado pelo adapter
  (`driver/turing_rev_c.rs:123`). A run-list cobre o buffer BGRA nativo inteiro, com 4 B quando ROM ≥ 1.89 e 3 B
  comprimido nos demais casos. O fallback é um frame cheio com BE32 correto seguido de zeros. As famílias A, B, D e
  WeAct usam `dirty_rects` do core.
- **D-2026-09-30-device-protocols-2:** o tráfego automático do rev C é HELLO (mais o bloco `2c` de resync),
  STOP_VIDEO/STOP_MEDIA, 0x86, frames, QUERY_STATUS e 0x87 no release. 0x83 só sai via `bezel off`. 0x84, 0x82,
  0x7D, 0x81 e C9 nunca são enviados. No WCH, 0x58 só sai no início do tema, como o fabricante faz; 0x56 nunca é
  enviado.
- **D-2026-09-30-device-protocols-3:** `connector.rs:120-130` retorna `InUse` com `"<cmd> (PID n)"`.
- **D-2026-09-30-device-protocols-4:** USB via nusb; DES-CBC PKCS#7 e DES-ECB vêm do RustCrypto.
- **D-2026-09-30-device-protocols-5:** só `turing-8.8` está `validated(...)` (`catalog.rs:136`).

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `cargo test --workspace --locked && echo OK` → `OK` (324 passed, 0 failed, 2 ignored) |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `cargo llvm-cov --workspace --locked --fail-under-lines 80 --summary-only` → exit 0, TOTAL 94.41% lines |
| 3 | No `TODO`/`FIXME` without linked issue | PROJECT | Auto | PASS | comando `git grep` do DoD → `OK` |
| 4 | Encoder de cada família bate com os vetores de referência | CONTEXT | Auto | PASS | os 7 testes `protocol::{turing_rev_c::tests::command_packets_match_the_reference_vectors, turing_rev_a, xuanfang_rev_b, kipye_rev_d, weact, turing_usb, wch}::tests::packets_match_the_reference_vectors` → `1 passed` cada |
| 5 | Porta ocupada recusada com nome e PID | CONTEXT | Auto | PASS | `busy::tests::finds_other_processes_holding_the_device` → `OK` |
| 6 | `bezel test-pattern` pelo connector na orientação escolhida | CONTEXT | Auto | PASS | `screen::tests::test_pattern_presents_frames_in_the_orientation` → `OK` |
| 7 | Na 8.8" real, legenda dos cantos correta nas 4 orientações e faixa animando | CONTEXT | Manual | MANUAL_REQUIRED | O SUMMARY só registra checagem de protocolo. Evidência sugerida: com `systemctl --user stop turing-smart-screen.service`, rodar `bezel test-pattern --orientation <o> --seconds 5` para `portrait`, `reverse-portrait`, `landscape` e `reverse-landscape`; confirmar vermelho no canto superior esquerdo, verde no superior direito, branco no inferior direito, azul no inferior esquerdo e a faixa amarela se movendo; anotar em SUMMARY § Hardware validation e religar o serviço |
| 8 | CHANGELOG.md atualizado por release | PROJECT | Manual | MANUAL_REQUIRED | As entradas da fase estão em `## [Unreleased]` (Added/Fixed). O cabeçalho `## [version]` sai no release (versão calculada pelo CI); confirmar no `/jdi-ship` |
| 9 | README descreve o comportamento atual | PROJECT | Manual | MANUAL_REQUIRED | `README.md:23-25` diz que "every command takes `--orientation`", mas só `test-pattern` e `show` aceitam a flag (`brightness`, `off`, `release` e `devices` não). Corrigir antes de confirmar; o resto (tabela de famílias, quick start) bate com o código |

## Recommendation
Não há bloqueios. Build, testes, cobertura (94,48%), lint, as checagens hexagonais e de segurança de escrita e as
5 decisões da fase passam; os 6 itens Auto do DoD passam. Antes do `/jdi-ship`:

1. Fazer a confirmação visual na 8.8" (item 7) e registrá-la no SUMMARY.
2. Corrigir a frase de orientação do README (item 9) e confirmar CHANGELOG/README.
3. Opcional, mas recomendado: um commit `refactor(device-protocols)` para W2–W5 (módulo RGB565 único, `io_err`,
   checagem de tamanho de frame e `Pause` em `driver/mod.rs`, `NO_CHANGE` usado ou removido, constante do tamanho
   de leitura) e injeção de `DeviceBus` no `SystemConnector` (W1), que também deixa o wake testável.
4. Decidir o destino da identificação HID/`1a86:ad11` (W6): todo ou fase futura.
