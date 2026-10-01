# Phase 10: Review  (slug: live-screen-controls)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 2 do loop autônomo (`/jdi-issue`, Step 6: reverificação após uma rodada de
> correção dos avisos). Branch `jdi/live-screen-controls`, `HEAD` = `a566e80` (já no remoto). A rodada de correção
> são os commits `4482f13` (W1), `9f8fb1a` (W2), `9311ba4` e `43921ff` (suspeitas do crítico) e `a566e80` (SUMMARY).
> Árvore limpa durante toda a revisão (só o `LOOP.md` do orquestrador como não rastreado).
>
> - **Números:** todos saíram das minhas execuções (`CARGO_TARGET_DIR=target/review`), nenhum copiado do SUMMARY.
> - **Hardware:** nada tocou `/dev/ttyACM*`; nenhum `#[ignore]`/`BEZEL_HW_TESTS`; o studio do usuário (PID 2613288)
>   seguiu intocado.
> - **Playwright:** `BEZEL_E2E_PORT=1442`.
> - **Testes não ocos:** mutações numa cópia de `HEAD` no scratchpad (`git archive`, alvo `target/review/mut`); o
>   repositório não foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **878 passed, 0 failed, 11 ignored** (hardware e ffmpeg real). Eram 876 na iter 1: +2 em `connector::tests` (`a_port_other_programs_hold_is_refused_before_it_is_opened`, `the_host_ports_name_this_process_for_a_port_it_holds`), nenhum removido. Bate com o SUMMARY |
| Coverage | PASS | **94.43%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.37%**. Arquivos da rodada: `connector.rs` 98.58% (era 98.02%), `busy.rs` 98.80%. Nenhum `main.rs` mudou |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `--target x86_64-pc-windows-msvc` (workspace sem `bezel-studio`): exit 0. Nenhum `allow` novo (os 4 de `fps/rtss.rs` são antigos) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos (detalhe abaixo). `cargo audit`: exit 0 (1278 advisories, 608 crates) |
| Consistency | PASS | Os 5 commits da rodada usam o escopo `live-screen-controls`, cabeçalhos ≤ 67; código e `.jdi/` separados. Arquivos tocados (`busy.rs`, `connector.rs`, `check-docs.sh`, o spec e2e) estão em "Files modified" do PLAN. device-protocols-3 e live-screen-controls-2..5 cumpridas; nenhuma D-XX contrariada |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: 186/186, **99.93%** de linhas, `live-screen.js` 100%. Playwright completo: **184/184** (claro/escuro × pt-BR/en, axe); `-g "live screen controls"`: 8 passed. Nenhum texto de UI novo |
| DoD | PASS | As 6 linhas Auto do CONTEXT (a 6 com o CI 36904730390 no `HEAD` `a566e80`) e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual; os 2 Manual do PROJECT são do corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada (a rodada não tocou o core) |
| 5.3 ports | PASS. Nenhuma impl de porta no core; as `pub trait` fora do core são as 6 auxiliares de antes. `SerialPorts` (privada ao adapter) trocou `holders`/`held_here` por um `holders() -> Holders` |
| 5.4 adapters na composição | PASS: nada |
| 5.5 `unsafe` | PASS: só os blocos antigos de `fps/rtss.rs` com `// SAFETY:`; `native.rs:39` é uma string |
| 5.6 panics | PASS: as 4 linhas novas com `unwrap` (`connector.rs:1150`, `1218-1226`) estão em `mod tests` |
| 5.7 escrita no dispositivo | PASS: nenhum `Confirm::Yes` novo; nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS: nada em `protocol/` nem em `docs/reverse-engineering/` mudou |
| 5.9 caminhos no core | PASS: só doc e fixtures de teste, como na iter 1 |
| 5.10 comandos síncronos | PASS: `commands.rs` não mudou |
| 5.11 supply chain | PASS: `cargo audit` exit 0; nenhum segredo |

### Sem regressão no refactor da W2 (`9f8fb1a`)
- **device-protocols-3 (outros programas recusados antes de tocar a porta).** `SerialPorts::open`
  (`connector.rs:122-135`) faz uma única leitura `holders()` e, com `others` não vazio, devolve `InUse` antes de
  `try_open`. `try_open` só é chamado de dentro de `open` (nenhum desvio); `open_serial` (todas as famílias seriais,
  `connector.rs:412-414`), `open_rev_c` e a abertura do MCU em `restart_rev_c` passam por ele. `restart_rev_c` segue
  checando `holders(display).others` antes de qualquer envio (`connector.rs:313-321`). É a mesma semântica do
  `open_serial` de `origin/main` (outros → `InUse`, depois `SerialWire::open`); `poke` segue sem checagem, como antes
  da fase. Teste novo `a_port_other_programs_hold_is_refused_before_it_is_opened` (diário vazio, outros vencem este
  processo).
- **D-4 (porta presa por este processo → `InUse` na hora).** `(Err(Transport), Some(this))` → `InUse` com este
  processo; outras recusas e `Ok` passam. O `this` agora vem da leitura antes da abertura (antes: depois da recusa);
  sem diferença prática — o link que morre é solto antes da reconexão (iter 1) e o `TIOCEXCL` só recusa enquanto o fd
  existe. `a_port_this_app_holds_fails_at_once_without_a_wake` intacto e passando (DoD 2).
- **Uma varredura por abertura.** `SystemPorts::holders` → `busy::on_this_machine` (um `holders_by_pid` sobre
  `/proc`); no Windows, sem `/proc`, ninguém — igual a antes.

### Testes não ocos (mutações em cópia de `HEAD`, `cargo test -p bezel-devices --lib`)
| Mutação | Resultado |
|---|---|
| M1: `open` sem a checagem de `others` | `a_port_other_programs_hold_is_refused_before_it_is_opened` FAILED |
| M2: porta própria nunca vira `InUse` | `a_port_this_app_holds_fails_at_once_without_a_wake` FAILED |
| M3: `SystemPorts::holders` com `this: None` | `the_host_ports_name_this_process_for_a_port_it_holds` FAILED |
| M4: `SystemPorts::holders` com `others: vec![]` | nenhum falha (lacuna anterior à fase; ver Observações) |
| `CHANGELOG.md` sem a entrada da fase | `check-docs.sh`: "does not mention 'Device or resource busy'", exit 1 |
| `CHANGELOG.md` só sem a frase ("resource blocked") | `check-docs.sh` exit 1 |

### Avisos e suspeitas da iter 1
| Item | Estado | Evidência |
|---|---|---|
| W1 — demo/e2e mais permissivo que o backend; passo de brilho não pode falhar | **CLEARED** | `4482f13`: o cabeçalho do spec (`live-screen-controls.spec.mjs:1-25`) diz que prova só a metade da UI, explica que o `setBrightness` do demo (`demo-backend.js:1246`) ignora o ao vivo e aponta os testes Rust do DoD 3 e o do connector para a metade backend. Títulos e o `test.step` do brilho não prometem mais "sem erro" do backend; o que prometem é real: `state.brightness[screen]` (`app.js:124`) e o diálogo lê `brightness[view.key]` (`ui/storage.js:405`), então "bootKeeps 40" prova a mesma chave listada. Nenhuma asserção perdida: 2 testes, 29 chamadas `expect`/`expectAccessible` e 26 matchers antes e depois; o diff sem comentários só acrescenta a linha do `test.step` |
| W2 — duas varreduras de `/proc` na recusa | **CLEARED** | `9f8fb1a`: uma leitura `holders()` por abertura (`connector.rs:123`); sem regressão (seção acima, M1/M2) |
| Crítico 1 — cola `SystemPorts` → `/proc` sem teste (DoD 2) | **CLEARED** | `9311ba4`: `the_host_ports_name_this_process_for_a_port_it_holds` (`connector.rs:1211-1229`, `cfg(unix)`) abre um arquivo e vê este PID em `this`; fechado ou inexistente, ninguém. M3 o derruba. A metade `others` da cola segue sem prova direta (M4), lacuna que já existia em `origin/main` (`open_serial` → `busy::holders`) |
| Crítico 2 — "For this screen" no `check-docs.sh` satisfeito por entrada antiga (DoD 5) | **CLEARED** | `43921ff`: exige "Device or resource busy" com espaços colapsados. A frase aparece uma única vez no `CHANGELOG.md` inteiro (`:208-209`, entrada desta fase); "For this screen" sai da lista sem perda (não estava em `origin/main`). Mutações acima: apagar a entrada ou só a frase falha |
| Crítico 3 — grep de TODO do PROJECT mais estreito que o critério | **PERSISTS (fora da fase)** | DoD do PROJECT é LOCKED (muda só por D-XX nova); a rodada corretamente não mexeu. Comando segue `OK` |

## Blockers
- Nenhum.

## Warnings
- **W3 (menor) — sobras do refactor da W2: `busy::holders` e `busy::held_here` ficaram sem chamador de produção.**
  Desde `9f8fb1a` o connector chama só `busy::on_this_machine` (`connector.rs:158`); `busy::holders`
  (`busy.rs:91-95`, de antes da fase) e `busy::held_here` (`busy.rs:97-101`, criada nesta fase em `7b147f2`)
  sobrevivem só por serem `pub` em `pub mod busy` (`lib.rs:8`) e por servirem aos testes de `busy`
  (`busy.rs:209-222`). Código morto público (yagni); nenhum outro crate os usa (`grep busy::` em `crates/`/`apps/`).
  Sugestão: os testes de `busy` passam a usar `on_this_machine(..).this/.others` e as duas funções saem; ou backlog.
  Não muda comportamento.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: a tela responde pelo display e pelo MCU; conectada pelo MCU volta com a chave do display | CONTEXT | Auto | PASS | `OK` (comando como escrito) |
| 2 | Devices: porta presa por este app falha na hora, sem wake nem restart | CONTEXT | Auto | PASS | `OK` (`ok. 2 passed`) |
| 3 | Studio: chave do SoC; brilho, release, restart e armazenamento por qualquer porta pelo link aberto; chave MCU reescrita | CONTEXT | Auto | PASS | `OK` (`ok. 6 passed`) |
| 4 | UI: lookup puro e Playwright nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` com `BEZEL_E2E_PORT=1442`; `-g "live screen controls"`: **8 passed**; `# pass 2` no `node --test` |
| 5 | CHANGELOG (Fixed) e checagem dos guias | CONTEXT | Auto | PASS | `OK`; entrada em `CHANGELOG.md:206-212` sob `### Fixed` do `[Unreleased]`; `check-docs.sh` exige a frase única da entrada |
| 6 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | `OK` com `HEAD` = `a566e80`. Run **36904730390** (CI, `a566e803…`, `jdi/live-screen-controls`): `completed success`. Verdes: `rust-linux`, `rust-windows` (**812 passed, 0 failed, 11 ignored**; eram 811: +1, o teste do `/proc` real é `cfg(unix)`), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` `skipped`, como na iter 1 |
| 7 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; gate 2: 878/0/11 |
| 8 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.37% (comando como escrito) |
| 9 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 10 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` atualizado; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 11 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | Nenhum comportamento descrito no README mudou; evidência sugerida: diff do README revisado no PR |

As linhas 10 e 11 são do corte de release do projeto, como na iter 1. A T-6 (8.8" real) não é linha de DoD: fica em
"Deferred to PR review", com o orquestrador, enquanto o studio do usuário segura a tela.

## Observações (sem aviso)
- **M4.** Descartar `others` na cola `SystemPorts::holders` não derruba nenhum teste: provar isso no `/proc` real
  pediria outro processo segurando um arquivo. A cola é uma expressão sem lógica e a lacuna é anterior à fase.
- **`rev_c_failures_without_a_wake_or_of_another_kind_are_returned`** (`connector.rs:1065-1092`) roteiriza um `InUse`
  vindo de `try_open`, que o host real não devolve mais (o `InUse` de outros agora sai de `open`, sem abrir). O teste
  segue válido para o laço do rev C (erro que não é `Transport` volta sem wake); o comportamento real está no teste
  novo.

## Recommendation
Os dois avisos da iter 1 e as duas suspeitas objetivas do crítico estão resolvidos, com testes que falham sem a
correção; o refactor da W2 mantém device-protocols-3 (outros programas recusados antes de tocar a porta) e o `InUse`
da porta própria. Nenhum gate falha e as 9 linhas Auto passam, incluindo o Windows no CI do `HEAD` `a566e80`. W3 é limpeza menor de API morta, sem efeito em
comportamento: pode ir junto com qualquer ajuste ou para o backlog.

## DoD Critic (enhanced)

- DoD row «2 | Devices: porta presa por este app falha na hora, sem wake nem restart»: oca e objetiva. Numa cópia
  descartável, `SystemPorts::holders` com `this: None` (`connector.rs:158`, o host real nunca nomeia este processo)
  ainda dá `ok. 2 passed` no Verify; com a mutação, uma porta que o studio segura volta como `Transport` e o rev C
  espera, cutuca e acorda o MCU — o que o critério proíbe. Só
  `connector::tests::the_host_ports_name_this_process_for_a_port_it_holds` (9311ba4) pega, e ele não está no
  `--exact` da linha (o `cargo test --workspace` da linha 7 pega). Correção: o Verify inclui esse teste e espera
  `ok. 3 passed`.
- Demais linhas: provam o critério (DoD 4: `syncLive` antigo → `8 failed`; DoD 5: entrada removida ou movida para
  Added → FAIL).

**Verdict:** BLOCKED
