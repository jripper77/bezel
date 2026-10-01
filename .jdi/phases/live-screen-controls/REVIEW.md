# Phase 10: Review  (slug: live-screen-controls)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 4 do loop autônomo (`/jdi-issue`). Branch `jdi/live-screen-controls`, `HEAD` =
> `1e82ece` (igual a `origin/jdi/live-screen-controls`). Escopo novo desde a iter 3 (`19913fa`/`8fb9ccc`): `872283e`
> (o teste das portas do host roda `SystemPorts::open` e o caminho rev C com `SystemPorts` sobre um arquivo temporário
> seguro por este processo), `05c8630` (W4: sai `busy::holders_in`) e `1e82ece` (SUMMARY). Em código só mudaram
> `crates/bezel-devices/src/{connector,busy}.rs`. O restante da fase (`git diff origin/main...HEAD`) foi relido em busca
> de regressão. Árvore limpa durante toda a revisão (só o `LOOP.md` do orquestrador como não rastreado).
>
> - **Números:** todos saíram das minhas execuções (`CARGO_TARGET_DIR=target/review`, com `cargo llvm-cov clean
>   --workspace` antes da cobertura), nenhum copiado do SUMMARY.
> - **Hardware:** nada tocou `/dev/ttyACM*`; nenhum `#[ignore]`/`BEZEL_HW_TESTS`; o studio do usuário (PID 2613288)
>   seguiu intocado.
> - **Playwright:** `BEZEL_E2E_PORT=1442`.
> - **Mutações:** numa cópia de `HEAD` no scratchpad (`git archive`, alvo `target/review/mut`); o repositório não foi
>   alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **878 passed, 0 failed, 11 ignored** (36 binários; hardware e ffmpeg real). Igual à iter 3: o `872283e` reforça um teste existente e o `05c8630` não remove teste. Bate com o SUMMARY |
| Coverage | PASS | **94.43%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.37%**. `busy.rs` 98.80% (166 linhas, 2 sem cobertura), `connector.rs` 98.63%. Nenhum `main.rs` mudou |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `--target x86_64-pc-windows-msvc` (workspace sem `bezel-studio`): exit 0 (o helper novo `through_the_host_ports` é `cfg(unix)` e não deixa código morto no Windows). Nenhum `allow` novo (os 4 de `fps/rtss.rs` são antigos, com `reason = "..."`) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos (detalhe abaixo). `cargo audit`: exit 0 (1278 advisories, 608 crates) |
| Consistency | PASS | Os 3 commits da iteração usam o escopo `live-screen-controls`, cabeçalhos ≤ 64 caracteres; código (`872283e`, `05c8630`) e `.jdi/` (`1e82ece`) separados. `connector.rs` e `busy.rs` estão em "Files modified" do PLAN. device-protocols-3, release-polish-13 e live-screen-controls-2..5 cumpridas; nenhuma D-XX contrariada |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: 186/186, **99.93%** de linhas, `live-screen.js` 100%. Playwright completo: **184/184** (claro/escuro × pt-BR/en, axe); `-g "live screen controls"`: 8 passed. A iteração não tocou a UI |
| DoD | PASS | As 6 linhas Auto do CONTEXT (a 6 com o CI **36912103461** no `HEAD` `1e82ece`) e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual; os 2 Manual do PROJECT são do corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada |
| 5.3 ports | PASS. Nenhuma impl de porta no core; as `pub trait` fora do core são as 6 auxiliares de antes; `SerialPorts` segue privada ao connector |
| 5.4 adapters na composição | PASS: nada |
| 5.5 `unsafe` | PASS: só os blocos antigos de `fps/rtss.rs` com `// SAFETY:`; `native.rs:39` é uma string |
| 5.6 panics | PASS: todo `unwrap`/`expect` acrescentado pela fase fora de `tests.rs`/`tests/` está em `mod tests` (o `872283e` só mexe no `mod tests` de `connector.rs`) |
| 5.7 escrita no dispositivo | PASS: os `Confirm::Yes` fora de teste são anteriores à fase (`storage.rs`, `manager.rs`, `screens.rs:156`, de `5e36061`/`eb233be`/`e941130`); nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS: nada em `protocol/` nem em `docs/reverse-engineering/` mudou na fase |
| 5.9 caminhos no core | PASS: as linhas acrescentadas pela fase são fixtures de teste (`discovery.rs`, `answers_to("/dev/ttyACM1")` etc.); o core não mudou desde a iter 2 |
| 5.10 comandos síncronos | PASS: `commands.rs` não mudou na fase |
| 5.11 supply chain | PASS: `cargo audit` exit 0; nenhum segredo |

### Segurança do teste novo (`872283e`)
- Só caminhos em `std::env::temp_dir()`: `bezel-ports-held-<pid>` (criado e mantido aberto pelo próprio teste) e
  `bezel-ports-no-mcu-<pid>` (nunca existe). Nenhum `/dev/tty*`: o `/dev/bezel-no-such-port` do fim é o de antes
  (`/dev/b…`, inexistente). A varredura de `/proc` só lê os links `fd` (não abre dispositivos).
- Nenhuma pausa real: o `RevCHost` do helper `through_the_host_ports` (`connector.rs:1215`) usa `ScriptedBus` e
  `Pauses` (que só registram as durações); o `SerialWire::open` de um arquivo comum falha na hora (`ENOTTY`, "Not a
  typewriter"). O teste roda em **0,41 s** (binário direto, `--exact`), igual a qualquer varredura de `/proc`.
- Passou também no CI Linux (`rust-linux`, run 36912103461).

## Achados da iter 3
| Item | Estado | Evidência |
|---|---|---|
| Crítico da iter 3 — DoD 2 oca e objetiva (nenhum teste da linha rodava `SystemPorts::open`; um `open` sobrescrito em `impl SerialPorts for SystemPorts` passava com `ok. 3 passed`) | **CLEARED** | `872283e`: `the_host_ports_name_this_process_for_a_port_it_holds` afirma `SystemPorts.open(&held, Flow::None)` == `InUse { address, holders: [este processo] }` com o arquivo aberto (`connector.rs:1253-1256`), que o caminho rev C com `SystemPorts` termina no mesmo `InUse` (`:1260`) sem nenhuma pausa (`:1261`), e que, fechado, o `open` volta a ser `Transport`. **Mutação pedida**, na cópia: `fn open(&self, e: &Endpoint, f: Flow) -> Result<SerialWire> { self.try_open(e, f) }` dentro de `impl SerialPorts for SystemPorts` → o Verify da linha 2 **como escrito** sai com **exit 1**, sem `OK` (`test result: FAILED. 2 passed; 1 failed`; pânico em `connector.rs:1253` do `HEAD`: `left: Some(Transport("…: Not a typewriter"))`, `right: Some(InUse { …, holders: ["bezel_devices-… (PID n)"] })`). Mutações extras: `open_rev_c` com `self.ports.try_open(..)` (o caminho rev C sem a checagem) → exit 1, pânico em `:1260` (`ended` = `Transport`); `this: None` na cola `SystemPorts::holders` → exit 1, pânico em `:1240` (`this.expect`) |
| W4 — `busy::holders_in` (repasse `pub` sem chamador de produção) | **CLEARED** | `05c8630` apaga a função (`busy.rs`: só esse arquivo). `grep -RnE 'holders_in'` em `crates/`, `apps/`, `docs/`, `.jdi/decisions/` e `DECISIONS.md`: nada. As `pub` de `busy` agora são `Holders`, `holders_by_pid` e `on_this_machine`, e o connector usa `Holders` e `on_this_machine` (`connector.rs:10`, `:158`) |

### Nenhuma asserção perdida
- `05c8630`: o `mod tests` de `busy.rs` tem **13 asserções em 3 testes** em `19913fa` e no `HEAD` (contagem de
  `assert!`/`assert_eq!`). Os 4 usos de `holders_in(r, d, p)` viram `holders_by_pid(r, d, p).others`, que era
  literalmente o corpo da função apagada: mesma expressão, mesmo valor (`busy.rs:134`, `:175-178`, `:204-214`).
- `872283e`: o teste das portas do host passa de **4 para 9** verificações. A `assert!(holders.this.is_some_and(|h|
  h.ends_with(&me)))` vira `this.expect(..)` + `assert!(this.ends_with(&me))` (mesma condição, mensagem melhor);
  `others` vazio, os dois `Holders::default()` (fechado e porta inexistente) seguem; entram o `InUse` do `open`, o
  `InUse` do rev C, nenhuma pausa e o `Transport` depois de fechado.

## Blockers
- Nenhum.

## Warnings
- **W5 (menor) — `open_serial` (`crates/bezel-devices/src/connector.rs:412-414`) não é exercitado com porta presa
  por nenhum teste.** É a entrada de `SerialPorts::open` das famílias seriais sem MCU (rev A, rev B, rev D, WeAct;
  `connector.rs:70-85`), e o PLAN T-2 diz que "`open_serial` a usa em toda família serial". Na cópia, trocar o corpo
  por `SystemPorts.try_open(endpoint, flow)` passa o Verify da linha 2 (`OK`) e a workspace inteira (**878 passed, 0
  failed**): só `every_family_is_routed_and_missing_devices_fail_cleanly` (`:1162`) passa por ali, e com uma porta
  inexistente. Com a mutação essas famílias perdem o `InUse` com este app (D-2026-10-01-live-screen-controls-4) e o
  `InUse` de outros programas (device-protocols-3). **Não bloqueia:** o código cumpre as duas decisões; a linha 2
  continua verdadeira mesmo com a mutação (nessas famílias uma porta presa falha na hora com `Transport`, e lá não há
  espera, wake nem restart), e a tela do relato (8.8", rev C) passa por `open_rev_c`, que agora é mordido. A lacuna
  dos outros programas já existia em `origin/main` (o `busy::holders` inline de `open_serial` também não tinha teste).
  Correção de 1 a 2 linhas no mesmo teste, com o arquivo ainda aberto: `assert_eq!(open_serial(&held,
  Flow::Hardware).err(), Some(in_use.clone()))`, ou `SystemConnector.connect` de uma tela `TuringRevA` com
  `display = held`. Ou backlog.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: a tela responde pelo display e pelo MCU; conectada pelo MCU volta com a chave do display | CONTEXT | Auto | PASS | `OK` (comando como escrito) |
| 2 | Devices: porta presa por este app falha na hora, sem wake nem restart | CONTEXT | Auto | PASS | `OK` (`ok. 3 passed`, 0,39 s). Com o `open` sobrescrito do crítico, com `open_rev_c` sem a checagem e com `this: None`, o mesmo comando sai com exit 1 (seção acima) |
| 3 | Studio: chave do SoC; brilho, release, restart e armazenamento por qualquer porta pelo link aberto; chave MCU reescrita | CONTEXT | Auto | PASS | `OK` (`ok. 6 passed`) |
| 4 | UI: lookup puro e Playwright nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` com `BEZEL_E2E_PORT=1442`; `-g "live screen controls"`: **8 passed**; `# pass 2` no `node --test` |
| 5 | CHANGELOG (Fixed) e checagem dos guias | CONTEXT | Auto | PASS | `OK`; entrada sob `### Fixed` do `[Unreleased]`; `check-docs.sh` exit 0 |
| 6 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Comando como escrito → `OK` com `HEAD` = `1e82ece` (achou o run **36912103461**, `completed success`, `headSha` `1e82ece…`). Verdes: `rust-windows` (**812 passed, 0 failed, 11 ignored**, igual à iter 3: os testes de `busy` e o das portas do host são `cfg(unix)`), `rust-linux` (**878/0/11**; `the_host_ports_name_this_process_for_a_port_it_holds` e `this_process_is_told_apart_from_other_holders` ok), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` `skipped`, como nas iterações anteriores (Sonar dispensado, D-2026-09-30-foundation-2) |
| 7 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Comando como escrito exit 0 → `OK`; 878/0/11 |
| 8 | Coverage >= 80% of lines | PROJECT | Auto | PASS | Comando como escrito exit 0 → `OK`; TOTAL 94.37% |
| 9 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 10 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` atualizado; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 11 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | Nenhum comportamento descrito no README mudou; evidência sugerida: diff do README revisado no PR |

As linhas 10 e 11 são do corte de release do projeto, como nas iterações 1 a 3. A T-6 (8.8" real) não é linha de
DoD: fica em "Deferred to PR review", com o orquestrador, enquanto o studio do usuário segura a tela.

## Observações (sem aviso)
- **Comentário do teste.** "a file is no tty, so serialport's exclusive lock refuses it" (`connector.rs:1244-1245`):
  o erro real é `ENOTTY` ("Not a typewriter") do `ioctl` de exclusividade, não `EBUSY`. O teste prova a regra como o
  código a escreve (qualquer `Transport` numa porta que este processo segura vira `InUse`), que é o que a D-4 pede;
  o `EBUSY` de uma tty de verdade fica com a T-6.
- **SUMMARY, "Tests".** Lista os runs até o 36908306551 (`19913fa`); o run deste `HEAD` é o 36912103461 (verde).
- **PLAN T-2.** O critério "`holders()` segue 'os outros'" (`PLAN.md:27`) segue desatualizado desde a W3; a semântica
  (device-protocols-3) vive em `SerialPorts::open` (`connector.rs:122-135`) via `on_this_machine(..).others`.

## Recommendation
O bloqueio da iter 3 caiu: o teste das portas do host agora roda `SystemPorts::open` e o caminho rev C com
`SystemPorts` sobre uma porta segura por este processo, e o `open` sobrescrito que o crítico usou derruba a linha 2
como escrita (exit 1). O mesmo vale para `open_rev_c` sem a checagem e para `this: None`. A W4 também caiu, sem
asserção perdida. O teste novo não toca `/dev/tty*` e não dorme. Nenhum gate falha, e as 9 linhas Auto passam,
incluindo o Windows no CI do `HEAD` `1e82ece`.

A W5 é a última entrada de `SerialPorts::open` sem teste: `open_serial`, das famílias seriais sem MCU. Não afeta a
tela do relato nem o texto da linha 2. É uma asserção a mais no mesmo teste, pode ir junto com qualquer ajuste ou
para o backlog, e não impede a T-6 nem o PR.

## DoD Critic (enhanced)

- DoD row «2 | Devices: porta presa por este app falha na hora, sem wake nem restart»: oca e objetiva. As brechas
  das iters 2 e 3 estão fechadas (`open` sobrescrito, `this: None`, `open_rev_c` com `try_open`: FAIL), e a W5
  (`open_serial` com `try_open`) só troca o tipo de erro, sem falsificar o critério. Mas `.exclusive(false)` no
  builder de `SerialWire::open` (`wire.rs:157`) passa com `ok. 3 passed` e 878/0: numa pty segura por este processo,
  a reabertura dá `Ok` (segundo link na mesma porta) e, no caminho rev C, 40 pausas, 6 HELLO e um `MCU_RESTART` até
  `Timeout`. A "porta presa" do teste é um arquivo comum, que falha com ENOTTY presa ou não; nada fixa a abertura
  exclusiva de que `connector.rs:118` depende. Correção: segurar uma pty e afirmar que a reabertura com o builder do
  `SerialWire` falha como ocupada (EBUSY).
- Demais linhas: provam o critério (nada mudou fora de `crates/bezel-devices` desde `a566e80`; reexecutadas OK).

**Verdict:** BLOCKED
