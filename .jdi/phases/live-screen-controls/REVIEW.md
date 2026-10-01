# Phase 10: Review  (slug: live-screen-controls)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 3 do loop autônomo (`/jdi-issue`). Branch `jdi/live-screen-controls`, `HEAD` =
> `19913fa` (igual a `origin/jdi/live-screen-controls`). Escopo novo desde a iter 2 (`a566e80`): `0acd3f1` (Verify da
> linha 2 do DoD endurecido pelo orquestrador), `2a58bf1` (W3: saem `busy::holders` e `busy::held_here`) e `19913fa`
> (SUMMARY). Só `crates/bezel-devices/src/busy.rs` mudou em código. O restante da fase (`git diff origin/main...HEAD`)
> foi relido em busca de regressão. Árvore limpa durante toda a revisão (só o `LOOP.md` do orquestrador como não
> rastreado).
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
| Tests | PASS | **878 passed, 0 failed, 11 ignored** (36 binários; hardware e ffmpeg real). Igual à iter 2: o `2a58bf1` não removeu nem acrescentou teste. Bate com o SUMMARY |
| Coverage | PASS | **94.43%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.37%**. `busy.rs` 98.77% (162 linhas, 2 sem cobertura), `connector.rs` 98.58%. Nenhum `main.rs` mudou |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `--target x86_64-pc-windows-msvc` (workspace sem `bezel-studio`): exit 0. Nenhum `allow` novo (os 4 de `fps/rtss.rs` são antigos, com `reason = "..."` no próprio atributo; a fase não tocou `bezel-sensors`) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos (detalhe abaixo). `cargo audit`: exit 0 (1278 advisories, 608 crates) |
| Consistency | PASS | Os 3 commits da iteração usam o escopo `live-screen-controls`, cabeçalhos ≤ 70; código (`2a58bf1`) e `.jdi/` (`0acd3f1`, `19913fa`) separados. `busy.rs` está em "Files modified" do PLAN. device-protocols-3 e live-screen-controls-2..5 cumpridas; nenhuma D-XX contrariada |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: 186/186, **99.93%** de linhas, `live-screen.js` 100%. Playwright completo: **184/184** (claro/escuro × pt-BR/en, axe); `-g "live screen controls"`: 8 passed. A iteração não tocou a UI |
| DoD | PASS | As 6 linhas Auto do CONTEXT (a 2 com o Verify endurecido, `ok. 3 passed`; a 6 com o CI 36908306551 no `HEAD` `19913fa`) e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual; os 2 Manual do PROJECT são do corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada |
| 5.3 ports | PASS. Nenhuma impl de porta no core; as `pub trait` fora do core são as 6 auxiliares de antes; `SerialPorts` segue privada ao connector |
| 5.4 adapters na composição | PASS: nada |
| 5.5 `unsafe` | PASS: só os blocos antigos de `fps/rtss.rs` com `// SAFETY:`; `native.rs:39` é uma string |
| 5.6 panics | PASS: todo `unwrap`/`expect` novo da fase está em `mod tests` (o `2a58bf1` só mexe em testes e apaga duas funções) |
| 5.7 escrita no dispositivo | PASS: nenhum `Confirm::Yes` novo fora de teste/doc; nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS: nada em `protocol/` nem em `docs/reverse-engineering/` mudou |
| 5.9 caminhos no core | PASS: só doc e fixtures de teste, como antes |
| 5.10 comandos síncronos | PASS: `commands.rs` não mudou na fase |
| 5.11 supply chain | PASS: `cargo audit` exit 0; nenhum segredo |

### Achados da iter 2
| Item | Estado | Evidência |
|---|---|---|
| Crítico da iter 2 — DoD 2 oca e objetiva (`SystemPorts::holders` com `this: None` passava) | **CLEARED** | `0acd3f1`: o Verify roda também `connector::tests::the_host_ports_name_this_process_for_a_port_it_holds` e exige `ok. 3 passed`. No `HEAD`: `test result: ok. 3 passed` → `OK`. Mutação na cópia, `connector.rs:158` → `Holders { this: None, ..crate::busy::on_this_machine(&endpoint.address.0) }`: o Verify da linha 2 **como escrito** sai com exit 1, sem `OK` (`test result: FAILED. 2 passed; 1 failed`, o teste da cola em pânico em `connector.rs:1222`, o `assert!(holders.this.is_some_and(..))`). O Verify antigo (`ok. 2 passed`, sem esse teste) contra a mesma mutação ainda imprime `OK`, o que confirma que a correção é o que fecha a lacuna |
| W3 — `busy::holders` e `busy::held_here` sem chamador de produção | **CLEARED** | `2a58bf1` apaga as duas (`busy.rs`: +7/-17, só esse arquivo); nenhuma referência sobra em `crates/`, `apps/`, `docs/` ou nas decisões (`grep -RnE 'busy::\|held_here'`: só `connector.rs:158`, `busy::on_this_machine`, e o campo `held_here` do `ScriptedPorts` de teste). Build, clippy Linux e Windows ok. Ver "Nenhuma asserção perdida" abaixo. Resta uma sobra do mesmo tipo em `holders_in` (W4) |

### Nenhuma asserção perdida no `2a58bf1`
- `mod tests` de `busy.rs`: **13 asserções em 3 testes** em `a566e80` e no `HEAD` (contagem de `assert!`/`assert_eq!`).
- `this_process_is_told_apart_from_other_holders` (`busy.rs:152`): as 3 asserções sobre o `/proc` real
  (`held_here` = este PID; `holders` vazio; fechado, `held_here` = `None`) viram 3 sobre `on_this_machine(..)`
  (`.this` termina em `(PID n)`; `.others` vazio; fechado, `.this` = `None`). Antes eram duas varreduras, agora uma:
  mesma semântica.
- `missing_proc_or_device_yields_nothing` (`busy.rs:206`): `holders(..)` vazio e `held_here(..)` = `None` viram
  `.others` vazio e `.this` = `None` sobre `on_this_machine("/dev/bezel-no-such-port")`.
- Os testes ainda mordem: na cópia, `on_this_machine` com `std::process::id() + 1` derruba
  `this_process_is_told_apart_from_other_holders` (FAILED).

## Blockers
- Nenhum.

## Warnings
- **W4 (menor) — `busy::holders_in` (`crates/bezel-devices/src/busy.rs:50-54`) é a última sobra do mesmo tipo da W3.**
  Em `origin/main` ela era o miolo testável de `busy::holders` (a única chamada de produção); desde `7b147f2` o
  `holders` passou a ir por `on_this_machine`, e no `HEAD` `holders_in` só é chamada pelos testes de `busy`
  (`busy.rs:140`, `:181`, `:207`, `:209`). É uma `pub fn` de repasse (`holders_by_pid(..).others`) exposta por
  `pub mod busy` (`lib.rs:8`) sem chamador fora dos testes (kiss/yagni, mesmo critério que gerou a W3). Sugestão: os
  4 usos de teste passam a `holders_by_pid(..).others` e a função sai; ou backlog. Não muda comportamento; não estava
  na W3 e a iter 2 não a citou.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: a tela responde pelo display e pelo MCU; conectada pelo MCU volta com a chave do display | CONTEXT | Auto | PASS | `OK` (comando como escrito) |
| 2 | Devices: porta presa por este app falha na hora, sem wake nem restart | CONTEXT | Auto | PASS | `OK` (`ok. 3 passed`: os 2 de antes e `the_host_ports_name_this_process_for_a_port_it_holds`). Com a mutação `this: None` o mesmo comando falha (seção acima) |
| 3 | Studio: chave do SoC; brilho, release, restart e armazenamento por qualquer porta pelo link aberto; chave MCU reescrita | CONTEXT | Auto | PASS | `OK` (`ok. 6 passed`) |
| 4 | UI: lookup puro e Playwright nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` com `BEZEL_E2E_PORT=1442`; `-g "live screen controls"`: **8 passed**; `# pass 2` no `node --test` |
| 5 | CHANGELOG (Fixed) e checagem dos guias | CONTEXT | Auto | PASS | `OK`; entrada em `CHANGELOG.md:206-212` sob `### Fixed` do `[Unreleased]`; `check-docs.sh` exit 0 (e `check-packaging.sh` exit 0) |
| 6 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | `OK` com `HEAD` = `19913fa` (o comando achou o run **36908306551**). Run `completed success` no `19913fa1…`, `jdi/live-screen-controls`. Verdes: `rust-windows` (**812 passed, 0 failed, 11 ignored**, igual à iter 2: os testes de `busy` são `cfg(unix)`), `rust-linux` (878/0/11; `the_host_ports_name_this_process_for_a_port_it_holds` e `this_process_is_told_apart_from_other_holders` ok), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` `skipped`, como nas iterações anteriores |
| 7 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Comando como escrito (gate 2) exit 0 → `OK`; 878/0/11 |
| 8 | Coverage >= 80% of lines | PROJECT | Auto | PASS | Comando como escrito exit 0 → `OK`; TOTAL 94.37% |
| 9 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 10 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` atualizado; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 11 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | Nenhum comportamento descrito no README mudou; evidência sugerida: diff do README revisado no PR |

As linhas 10 e 11 são do corte de release do projeto, como nas iterações 1 e 2. A T-6 (8.8" real) não é linha de DoD:
fica em "Deferred to PR review", com o orquestrador, enquanto o studio do usuário segura a tela.

## Observações (sem aviso)
- **PLAN T-2.** O critério "`holders()` segue 'os outros'" (`PLAN.md:27`) ficou desatualizado com o `2a58bf1`: a
  semântica que ele protegia (device-protocols-3, outros programas recusados antes de tocar a porta) segue em
  `SerialPorts::open` (`connector.rs:122-135`) via `on_this_machine(..).others`; a função em si saiu a pedido da W3.
- **SUMMARY, "Tests".** Ainda cita o CI 36900540628 (`483c842`) para o DoD 6; o run deste `HEAD` é o 36908306551.
- **M4 (iter 2).** Descartar `others` na cola `SystemPorts::holders` segue sem teste que pegue (provar exige outro
  processo segurando um arquivo); lacuna anterior à fase, sem mudança nesta iteração.

## Recommendation
O bloqueio da iter 2 caiu: o Verify da linha 2 agora inclui o teste da cola `SystemPorts` → `/proc` e falha com
`this: None`, que era a mutação que o crítico usou. A W3 também caiu, sem asserção perdida e sem mudança de
comportamento. Nenhum gate falha. As 9 linhas Auto passam, incluindo o Windows no CI do `HEAD` `19913fa`.

A W4 é a mesma limpeza da W3 numa função que a iter 2 não citou (`busy::holders_in`, só usada por testes). Tem 4
linhas de teste e uma função a apagar, sem efeito em comportamento, e pode ir junto com qualquer ajuste ou para o
backlog. Ela não impede a T-6 nem o PR.

## DoD Critic (enhanced)

- DoD row «2 | Devices: porta presa por este app falha na hora, sem wake nem restart»: oca e objetiva (brecha menor
  que a da iter 2). As mutações no corpo são pegas (`this: None` → `2 passed; 1 failed`; `open` ignorando `this`;
  `shutting_down` casando `InUse`), mas um `open` sobrescrito em `impl SerialPorts for SystemPorts`
  (`connector.rs:142`, `self.try_open(e, f)`) passa com `ok. 3 passed`; numa sonda com `RevCHost` + `SystemPorts`
  reais (display = arquivo temporário seguro por este processo, MCU inexistente, nenhum `/dev/tty*`) a abertura vira
  `Transport` depois de 5 s de espera e 1 cutucão no MCU. Nenhum dos 3 testes roda `SystemPorts::open`, a entrada de
  produção (`open_rev_c`, `open_serial`). Correção: `the_host_ports_name_this_process_for_a_port_it_holds` afirma
  `SystemPorts.open(&held, Flow::None)` == `InUse { holders: [este processo] }` com o arquivo aberto.
- Linhas 1, 3–9: provam o critério (sem mudança fora de `crates/bezel-devices` desde `a566e80`; reexecutadas OK).

**Verdict:** BLOCKED
