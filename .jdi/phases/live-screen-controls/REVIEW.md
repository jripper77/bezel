# Phase 10: Review  (slug: live-screen-controls)

**Verdict:** APPROVED

> Revisão em modo `verify`, iteração 5 do loop autônomo (`/jdi-issue`). Branch `jdi/live-screen-controls`, `HEAD` =
> `0680be2` (igual a `origin/jdi/live-screen-controls`). Escopo novo desde a iter 4 (`1e82ece`/`52e6757`): `7883e97`
> (`SerialWire::open` passa por `settings(path, flow)` com `.exclusive(true)` explícito; teste numa pty), `2a4a84f`
> (W5: `open_serial` numa porta segura por este processo) e `0680be2` (Verify da linha 2 com 4 testes, `ok. 4
> passed`; SUMMARY). Em código só mudaram `crates/bezel-devices/src/{wire,connector}.rs`. O restante da fase (`git diff
> origin/main...HEAD`, 33 arquivos) foi relido em busca de regressão. Árvore limpa durante toda a revisão (só o
> `LOOP.md` do orquestrador como não rastreado).
>
> - **Números:** todos saíram das minhas execuções (`CARGO_TARGET_DIR=target/review`, com `cargo llvm-cov clean
>   --workspace` antes da cobertura), nenhum copiado do SUMMARY.
> - **Hardware:** nada tocou `/dev/ttyACM*`; nenhum `#[ignore]`/`BEZEL_HW_TESTS`; o studio do usuário (PID 2613288)
>   seguiu intocado. O teste novo usa uma pty (`/dev/pts/N`).
> - **Playwright:** `BEZEL_E2E_PORT=1442`.
> - **Mutações:** em cópias de `HEAD` no scratchpad (`git archive`), **cada uma com seu próprio** `CARGO_TARGET_DIR`
>   (`target/review/mut{A,B,C}`): com o alvo compartilhado o cargo reaproveitou o binário de outra mutação (mesmo
>   hash de metadados, fontes mais antigas que o build), e esse resultado foi descartado. O repositório não foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **879 passed, 0 failed, 11 ignored** (36 binários; hardware e ffmpeg real). +1 vs iter 4 (878): o `wire::tests::a_port_the_wire_holds_refuses_a_second_open`; o `2a4a84f` reforça um teste existente. Bate com o SUMMARY |
| Coverage | PASS | **94.43%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.37%**. `wire.rs` **84.56%** (298 linhas, 46 sem cobertura), `connector.rs` 98.63%, `busy.rs` 98.80% |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `--target x86_64-pc-windows-msvc` (workspace sem `bezel-studio`): exit 0 (o `exclusively` de `cfg(not(unix))` compila e não deixa código morto; o teste e o `busy` novos são `cfg(unix)`). Nenhum `allow` novo (os 4 de `fps/rtss.rs` são antigos, com `reason = "..."`) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos (detalhe abaixo). `cargo audit`: exit 0 (1278 advisories, 608 crates) |
| Consistency | PASS | Os 3 commits da iteração usam o escopo `live-screen-controls`, cabeçalhos ≤ 64 caracteres; código (`7883e97`, `2a4a84f`) e `.jdi/` (`0680be2`) separados. O Verify da linha 2 só ficou mais estrito (os 3 testes de antes + 1, `ok. 4 passed`). device-protocols-3, release-polish-13 e live-screen-controls-2..5 cumpridas; nenhuma D-XX contrariada |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: 186/186, **99.93%** de linhas. Playwright completo: **184/184** (claro/escuro × pt-BR/en, axe); `-g "live screen controls"`: 8 passed. A iteração não tocou a UI |
| DoD | PASS | As 6 linhas Auto do CONTEXT (a 6 com o CI **36918115874** no `HEAD` `0680be2`) e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual; os 2 Manual do PROJECT são do corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada (os `cfg(unix)` novos estão em `bezel-devices`, fora do core) |
| 5.3 ports | PASS. Nenhuma impl de porta no core; as `pub trait` fora do core são as 6 auxiliares de antes; `settings` e `exclusively` são `fn` privadas de `wire.rs` |
| 5.4 adapters na composição | PASS: nada |
| 5.5 `unsafe` | PASS: só os blocos antigos de `fps/rtss.rs` com `// SAFETY:`; `native.rs:39` é uma string |
| 5.6 panics | PASS: os `unwrap` novos estão no `mod tests` de `wire.rs` (`:345-351`) e de `connector.rs` (a partir de `:434`) |
| 5.7 escrita no dispositivo | PASS: os `Confirm::Yes` fora de teste são anteriores à fase; nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS: nada em `protocol/` nem em `docs/reverse-engineering/` mudou na fase |
| 5.9 caminhos no core | PASS: só fixtures de teste (`discovery.rs`); o core não mudou desde a iter 2 |
| 5.10 comandos síncronos | PASS: `commands.rs` não mudou na fase |
| 5.11 supply chain | PASS: `cargo audit` exit 0; nenhum segredo |

### `7883e97` nas portas CDC-ACM reais (o caminho de abertura de toda tela)
Sem regressão. Conferido na fonte do `serialport 4.10.1` do `Cargo.lock` (`~/.cargo/registry/.../serialport-4.10.1`):
- **Builder igual campo a campo.** `settings` (`wire.rs:148-162`) encadeia `new(path, 115_200)`, `DataBits::Eight`,
  `Parity::None`, `StopBits::One`, `flow_control(flow)`, `dtr_on_open(true)` e `timeout(10 ms)`, exatamente o
  builder inline que saiu. O único acréscimo, `.exclusive(true)` (`wire.rs:170-173`, só Unix), repete o padrão do
  crate: `new()` monta o builder com `exclusive: true` (`src/lib.rs:1056-1057`, teste `builder_exclusive` do próprio
  crate em `:1099`). Com `exclusive`, `TTYPort::open` faz `TIOCEXCL` + `flock` exclusivo (`src/posix/tty.rs:134-136`),
  como antes; o DTR sobe dentro do `open` quando `baud_rate > 0` (`tty.rs:204-207`).
- **RTS logo após o open.** `SerialWire::open` (`wire.rs:184-190`) é `settings(path, flow).open()` seguido de
  `write_request_to_send(true)`, a mesma ordem de antes.
- **Windows inalterado.** `SerialPortBuilder::exclusive` é `#[cfg(unix)]` no crate (`lib.rs:429-431`); o
  `exclusively` de `cfg(not(unix))` (`wire.rs:176-179`) devolve o builder intacto, e o `COMPort::open` abre com
  `share_mode = 0` (`src/windows/com.rs:62`), exclusivo por natureza. Clippy cruzado para MSVC: exit 0.
- **O teste novo** (`wire.rs:338-362`) é `#[cfg(unix)]`, usa `TTYPort::pair()` (uma pty; nenhum `/dev/ttyACM*`:
  `grep ttyACM wire.rs` vazio), não dorme (0,00 s sozinho; a linha 2 inteira em 0,83 s) e limpa tudo por `Drop`: as
  duas pontas (`_master`, `slave`) vivem até o fim do escopo e o `held` é fechado antes da reabertura (`close` do
  crate faz `TIOCNXCL`). O `busy` (`:333-336`) é preciso: no crate só `EBUSY` e o `flock` recusado viram `NoDevice`
  (`src/posix/error.rs:28`, `src/posix/flock.rs`); `ENOENT` é `NotFound` e `ENOTTY` é `Unknown`. A reabertura depois
  do `drop(held)` (`:359-361`) mostra que a recusa vem da posse.
- **Cobertura de `wire.rs`.** Não caiu: em `1e82ece` era **83.21%** (274 linhas, 46 sem cobertura; `cargo llvm-cov
  -p bezel-devices` numa cópia) e no `HEAD` é **84.56%** (298 linhas, as mesmas 46). As 46 são o I/O de uma porta de
  verdade, anterior à fase: o RTS e o `Ok` depois do open (`:186-190`, uma pty não tem linhas de modem), o `Drop`
  (`:199-203`) e o `impl Wire for SerialWire` (`:213-253`). Acima do piso de 80% por arquivo e no total (PROJECT).
  Não importa para o veredito.

## Achados da iter 4
| Item | Estado | Evidência |
|---|---|---|
| Crítico da iter 4 — DoD 2 oca e objetiva (`.exclusive(false)` no builder do `SerialWire` passava a linha 2 com `ok. 3 passed` e 878/0) | **CLEARED** | `7883e97` + `0680be2`. **Mutação pedida**, na cópia `mutA` com alvo próprio: `port.exclusive(false)` em `exclusively` (`wire.rs:172`, o builder de `settings` que o `SerialWire::open` usa) → o Verify da linha 2 **como escrito** sai com **exit 1**, sem `OK` (`test result: FAILED. 3 passed; 1 failed`; pânico em `wire.rs:353`, `again` = `None`: a segunda abertura da pty segura deu certo, o segundo link na mesma porta que o crítico descreveu) |
| W5 — `open_serial` sem teste com porta presa | **CLEARED** | `2a4a84f`: `assert_eq!(open_serial(&held, Flow::Hardware).err(), Some(in_use.clone()))` (`connector.rs:1261-1264`), com o arquivo ainda aberto. **Mutação da W5**, na cópia `mutC` com alvo próprio: corpo de `open_serial` = `SystemPorts.try_open(endpoint, flow)` (`connector.rs:413`) → o Verify da linha 2 sai com **exit 1** (pânico em `connector.rs:1261`: `left: Some(Transport("…: Not a typewriter"))`, `right: Some(InUse { …, holders: ["bezel_devices-… (PID n)"] })`) |

### Nenhuma asserção perdida
- `7883e97`: nenhum teste removido nem alterado em `wire.rs`; entra 1 teste com 3 asserções. `opening_a_missing_port_fails_cleanly`
  segue passando por `SerialWire::open` (agora via `settings`).
- `2a4a84f`: o teste das portas do host ganha 1 asserção (10 verificações); só o comentário muda além disso, e passa a
  nomear a recusa real de um arquivo comum (`ENOTTY`) e apontar o `EBUSY` de uma tty para o teste do `wire`.

## Blockers
- Nenhum.

## Warnings
- Nenhum.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: a tela responde pelo display e pelo MCU; conectada pelo MCU volta com a chave do display | CONTEXT | Auto | PASS | `OK` (comando como escrito) |
| 2 | Devices: porta presa por este app falha na hora, sem wake nem restart | CONTEXT | Auto | PASS | `OK` (`ok. 4 passed`, 0,83 s). Com `.exclusive(false)` no builder (`mutA`) e com `open_serial` = `try_open` (`mutC`), o mesmo comando sai com exit 1 (seção acima). Os furos das iters 2 a 4 seguem fechados pelos mesmos testes |
| 3 | Studio: chave do SoC; brilho, release, restart e armazenamento por qualquer porta pelo link aberto; chave MCU reescrita | CONTEXT | Auto | PASS | `OK` (`ok. 6 passed`) |
| 4 | UI: lookup puro e Playwright nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` com `BEZEL_E2E_PORT=1442`; `-g "live screen controls"`: **8 passed**; `# pass 2` no `node --test`; `test:unit` 186/186 |
| 5 | CHANGELOG (Fixed) e checagem dos guias | CONTEXT | Auto | PASS | `OK`; entrada sob `### Fixed` do `[Unreleased]`; `check-docs.sh` exit 0 |
| 6 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run terminar (`gh run watch 36918115874 --exit-status`: exit 0) e rodei o comando como escrito com `HEAD` = `0680be2` → `OK` (achou o run **36918115874**, `completed success`, `headSha` `0680be2…`). Verdes: `rust-windows` (**812 passed, 0 failed, 11 ignored**, igual à iter 4: o teste da pty é `cfg(unix)`), `rust-linux` (**879/0/11**; `wire::tests::a_port_the_wire_holds_refuses_a_second_open` e `the_host_ports_name_this_process_for_a_port_it_holds` ok no runner), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` `skipped`, como nas iterações anteriores (Sonar dispensado, D-2026-09-30-foundation-2) |
| 7 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Comando como escrito exit 0 → `OK`; 879/0/11 |
| 8 | Coverage >= 80% of lines | PROJECT | Auto | PASS | Comando como escrito exit 0 → `OK`; TOTAL 94.37% |
| 9 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 10 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` atualizado; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 11 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | Nenhum comportamento descrito no README mudou; evidência sugerida: diff do README revisado no PR |

As linhas 10 e 11 são do corte de release do projeto, como nas iterações 1 a 4. A T-6 (8.8" real) não é linha de
DoD: fica em "Deferred to PR review", com o orquestrador, enquanto o studio do usuário segura a tela.

## Observações (sem aviso)
- **Abertura fora de `settings` (mutação `mutB`).** O teste segura a pty por `settings`, não por `SerialWire::open`
  (numa pty o RTS falha; comentário em `wire.rs:347-350`). Logo, um `SerialWire::open` que se afastasse do builder
  comum, `settings(path, flow).exclusive(false).open()`, passa a linha 2 no Linux (`OK`, cópia `mutB` com alvo
  próprio): a abertura recusada continua recusada porque quem segura é exclusivo. Mas essa forma **não compila no
  Windows** (`E0599: no method named exclusive`, clippy `--target x86_64-pc-windows-msvc`), então a linha 6 cai; só
  um desvio deliberado com `cfg(unix)` passaria tudo. Hoje `SerialWire::open` é uma linha sobre `settings`
  (`wire.rs:185`) e a doc diz isso; um `open` que deixe de usar `settings` deixa `settings` sem chamador fora de teste
  (`dead_code` → clippy `-D warnings`). Registro, sem aviso.
- **Arquivos temporários de testes que falham.** `the_host_ports_name_this_process_for_a_port_it_holds` só apaga
  `bezel-ports-held-<pid>` no fim (`connector.rs:1277`): quando uma asserção cai antes, o arquivo vazio fica em
  `/tmp`. Há 24 deles de 15:05 a 16:55 (mutações das iterações anteriores); os 2 das minhas mutações foram apagados.
  Só acontece com o teste falhando; nada a fazer na fase.
- **PLAN T-2.** "Files modified" lista `crates/bezel-devices/src/{busy,connector}.rs`; o `wire.rs` (`7883e97`, correção
  pedida pelo crítico) está no SUMMARY, não no PLAN. O critério "`holders()` segue 'os outros'" (`PLAN.md:27`) segue
  desatualizado desde a W3, como registrado na iter 4.
- **SUMMARY, "Tests".** Lista os runs até o 36908306551 (`19913fa`); o run deste `HEAD` é o 36918115874.

## Recommendation
O bloqueio da iter 4 caiu: `SerialWire::open` abre pelo builder único `settings`, que pede `.exclusive(true)`
explicitamente, e o teste da pty prova que, segura por esse builder, a porta recusa uma segunda abertura (pelo
builder e por `SerialWire::open`) como ocupada (`EBUSY` → `NoDevice`) e volta a abrir depois de solta. O
`.exclusive(false)` do crítico derruba a linha 2 como escrita (exit 1). A W5 também caiu: `open_serial` com
`try_open` derruba a mesma linha. A mudança não regride as portas CDC-ACM reais: o builder é igual campo a campo
(o `exclusive: true` já era o padrão do `serialport 4.10.1`), o RTS segue logo após o open e o Windows recebe o
builder intacto (lá a COM já abre com `share_mode = 0`). A queda de cobertura de `wire.rs` não existe: subiu de
83.21% para 84.56%, com as mesmas 46 linhas de I/O de porta real descobertas desde antes da fase.

Nenhum gate falha e as 9 linhas Auto passam, incluindo o Windows no CI do `HEAD` `0680be2`. Sem blockers nem
avisos; as 2 linhas Manual do PROJECT ficam para o corte de release, como nas iterações anteriores e na fase
`video-background-framing`. A fase pode seguir para a T-6 (8.8" real, quando o studio do usuário liberar a tela) e
para o PR.

## DoD Critic (enhanced)

- Nenhuma linha oca. Linha 2: os 4 testes cobrem detecção (`/proc` real), mapeamento (`SystemPorts::open`,
  `open_serial`, rev C sem pausas), ausência de espera/wake/restart e abertura exclusiva; nenhuma mutação de um ponto
  realista passa — as que passam só trocam o tipo de erro (`access_error` casando "busy": falha na hora como
  `AccessDenied`). Brecha residual não objetiva: um desvio deliberado de duas linhas com `cfg(unix)` em
  `SerialWire::open` (o teste fixa o builder compartilhado, não o wire como detentor, porque RTS falha numa pty).
  Linhas 1, 3–9: nada mudou fora de `crates/bezel-devices` desde `a566e80`; reexecutadas OK.

**Verdict:** APPROVED
