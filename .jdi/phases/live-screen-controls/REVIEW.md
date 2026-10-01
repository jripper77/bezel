# Phase 10: Review  (slug: live-screen-controls)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 1 do loop autônomo (`/jdi-issue`). Branch `jdi/live-screen-controls`,
> `HEAD` = `483c842` (já no remoto; 6 commits de código/docs desde `origin/main`). Árvore limpa durante toda a revisão
> (só o `LOOP.md` do orquestrador como não rastreado).
>
> - **Números:** todos saíram das minhas execuções (`CARGO_TARGET_DIR=target/review`), nenhum copiado do SUMMARY.
> - **Hardware:** nada tocou `/dev/ttyACM*`; nenhum `#[ignore]`/`BEZEL_HW_TESTS`; o studio do usuário (PID 2613288)
>   seguiu intocado.
> - **Playwright:** `BEZEL_E2E_PORT=1442`.
> - **Testes não ocos:** conferi com mutações numa cópia de `HEAD` no scratchpad (`git archive`, alvo
>   `target/review/mut`); o repositório não foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **876 passed, 0 failed, 11 ignored** (hardware e ffmpeg real). Eram 866 na fase anterior: +10 testes novos (2 core, 2 devices, 6 studio), nenhum removido |
| Coverage | PASS | **94.42%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.36%**. Arquivos da fase: `discovery.rs` 100%, `busy.rs` 98.80%, `connector.rs` 98.02%, `screens.rs` 95.77%, `backend.rs` 98.14%, `studio.rs` 97.00%, `storage.rs` 97.06%, `manager.rs` 94.42%. Nenhum `main.rs` mudou |
| Lint | PASS | `cargo fmt --all --check`: exit 0. `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0, 0 avisos. Clippy cruzado `--target x86_64-pc-windows-msvc` (workspace sem `bezel-studio`, 7 crates): exit 0. Nenhum `allow` novo; os 4 de `fps/rtss.rs` são antigos |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos (detalhe abaixo). `cargo audit`: exit 0 (1278 advisories, 608 crates) |
| Consistency | PASS | Os 6 commits da fase usam o escopo `live-screen-controls`, cabeçalhos ≤ 72. Arquivos dos commits = "Files modified" do PLAN. D-2026-10-01-live-screen-controls-2..5 cumpridas (detalhe abaixo); nenhuma D-XX contrariada |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: exit 0, **99.93%** de linhas, `live-screen.js` 100%. Playwright: **184/184** (claro/escuro × pt-BR/en, axe), 8 novos. Nenhum texto novo (i18n intacto, paridade no `i18n.test.mjs`) |
| DoD | PASS | As 6 linhas Auto do CONTEXT (a 6 com o CI 36900540628 no `HEAD` `483c842`) e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual; os 2 Manual do PROJECT são do corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. `connect_screen`/`listed_again` usam só as portas `DeviceBus`/`ScreenConnector` |
| 5.3 ports | PASS. Nenhuma impl de porta no core. As `pub trait` fora do core são as 6 auxiliares de antes; `SerialPorts` (privada) ganhou `try_open`/`held_here` e um `open` padrão, helper interno do adapter |
| 5.4 adapters na composição | PASS: nada |
| 5.5 `unsafe` | PASS: só os blocos antigos de `fps/rtss.rs` com `// SAFETY:`; `native.rs:39` é uma string |
| 5.6 panics | PASS: nenhuma linha nova fora de `#[cfg(test)]` tem `unwrap`/`expect`/`panic!` (conferido por hunk em `busy.rs`, `connector.rs`, `screens.rs`, `discovery.rs`, `studio.rs`, `backend.rs`, `storage.rs`) |
| 5.7 escrita no dispositivo | PASS: os acertos de `Confirm::Yes` são os de antes (doc, `Confirmed::require`, testes); nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS: nada em `protocol/` nem em `docs/reverse-engineering/` mudou |
| 5.9 caminhos no core | PASS: doc e testes; os novos são fixtures do teste `a_screen_answers_to_its_display_and_its_mcu_port` (`#[cfg(test)]`), como os antigos do mesmo arquivo |
| 5.10 comandos síncronos | PASS: `commands.rs` não mudou; os síncronos são os de antes e nenhum chega a dispositivo |
| 5.11 supply chain | PASS: `cargo audit` exit 0; nenhum segredo |

### Conformidade com as decisões
- **D-2 (uma chave):** `Backend::set_live` (`backend.rs:443-468`) usa `connect_screen` e `key_of(&found, asked)` para
  sessão, `liveScreen` e orientação; `toggle_live`, a retomada de `restart_screen` e `restore_live` passam por ele.
  `connect_screen` (`screens.rs:63`) relista por `find_again` como `reopen_screen` (agora com `listed_again`
  compartilhado, sem cópia).
- **D-3 (identidade):** `grep -nE 'live_key\(\) == Some|l\.key == key'` em `studio.rs`, `backend.rs`, `storage.rs` =
  vazio. Os `live_key()` restantes são `is_some()`, o `sample` e o `push`, nenhum comparando chave.
- **D-4 (porta deste processo):** `SerialPorts::open` (`connector.rs:121-135`) é o único ponto, usado pelo rev C e por
  `open_serial` (todas as famílias seriais). `holders()` segue "os outros"; sem `/proc` = ninguém, sem `cfg` novo.
- **D-5 (UI só seleciona tela listada):** `liveScreenIn` puro, `syncLive` (`app.js:454-466`) só grava a `key`
  listada.

### Revisão crítica (além dos gates)
- **`Studio::is_live` / `Screen::answers_to`.** Compara o endereço exato de `display`/`wake`; endereço vazio não casa
  (testado em core e studio). Uma tela diferente só casaria se tivesse a mesma porta, o que não acontece; durante
  `Away` o `Live.screen` guarda as portas antigas, mas a chave já era a porta antiga antes da fase (sem regressão), e
  `reconnected` atualiza chave e `screen`. Sem `screen` (`go_live` puro), só a chave responde.
- **`connect_screen`.** Relista por identidade (`same_wake` → porta → serial → endereço), então um rev C acordado ou
  reiniciado volta pelo display novo; outra tela não é tomada. O rev C só conecta depois do HELLO, então o SoC já está
  no bus ao relistar. Se o bus falhar ao relistar, fica a tela escolhida (dormindo: chave do MCU) — caso raro em que
  `is_live(display)` seria falso até a próxima reconexão; documentado.
- **`set_live` vs o antigo `find_screen`.** Mesmos erros para a UI: `ScreenNotFound`/`NoScreenChosen`/erros do
  conector convertidos pelo mesmo `From<BezelError>`; o `stop_live` + `drop(previous)` continua antes de abrir.
- **`InUse` da porta presa por este processo.** `held_here` é consultado com o mesmo `endpoint` que falhou, só depois
  de uma recusa `Transport`. Não dispara num display morrendo: o link que falha é devolvido e solto em `deliver`
  (`backend.rs:251-253`) antes da reconexão (2 s depois); um fd de nó removido aparece como `… (deleted)` e não casa
  com o caminho; e `close(2)` tira o fd de `/proc/self/fd` antes de drenar. Custo: a varredura de `/proc` por abertura
  já existia (device-protocols-3); a nova só roda na recusa.
- **Windows.** `held_here` = `None` (sem `/proc`), comportamento de antes; COM presa pelo próprio app segue no backlog
  (`.jdi/todos/2026-10-01-live-screen-controls.md`). `busy::tests` é `cfg(unix)`; o teste novo do conector não usa
  nada de Unix. Clippy Windows limpo aqui e no CI.
- **UI `syncLive`.** A tela ao vivo não listada não mexe na seleção (D-5). Com outra tela escolhida pelo usuário
  durante o ao vivo, a amostra seguinte volta à listada ao vivo até o `setLive` da troca terminar — o mesmo que o
  código antigo fazia (`state.screen = s.live`), não é regressão; a troca move o ao vivo, então a seleção converge.
  No início, `refreshScreens` roda antes do `sampleLoop`, sem salto transitório.
- **Sonar/JS.** `syncLive` foi dividido (`syncReconnecting`, `syncLiveVideo`, `announceStop`); nenhum ternário
  aninhado, promessa solta, `charCodeAt` ou `RegExp` montada a partir de variável nas linhas novas. O ternário aninhado
  de `renderScreenSelect` (`app.js:392`) é antigo.
- **a11y/i18n.** Nenhum elemento ou chave nova; axe sem violações sérias nos 4 projetos.

### Testes não ocos (mutações em cópia de `HEAD`)
| Mutação | Resultado |
|---|---|
| `SerialPorts::open` = só `try_open` | `connector::tests::a_port_this_app_holds_fails_at_once_without_a_wake` FAILED |
| `connect_screen` sem relistar | `a_screen_connected_by_its_mcu_port_is_keyed_by_its_display` FAILED (`/dev/ttyACM0` ≠ `/dev/ttyACM1`) |
| `Live::answers_to` = só a chave | 4 dos 6 do DoD 3 FAILED (`studio::…either_of_its_ports`, `backend::either_port…`, `manager::…borrows_its_link`, `storage::…either_port…`) |
| `set_live` com a chave pedida | os outros 2 do DoD 3 FAILED (`…put_live_by_its_mcu_port…`, `…saved_mcu_key…`) |
| `syncLive` antigo (`state.screen = s.live`) | os 8 do Playwright `live screen controls` FAILED ("Nenhuma tela conectada"/"No screen connected" no lugar de "ao vivo"/"live") |

## Blockers
- Nenhum.

## Warnings
- **W1 — o cenário demo `mcuLive` é mais permissivo que os backends reais, e o passo de brilho do e2e não pode
  falhar.** `demo-backend.js:1246` (`setBrightness`) ignora o modo ao vivo, e o Armazenamento do demo reconhece a tela
  pelas duas portas mesmo com a chave ao vivo no MCU. No 0.1.0-dev.287 isso dava "busy"; no backend novo só chegaria a
  esse estado no fallback raro (display não relistado), em que `is_live(display)` seria falso. Então
  `live-screen-controls.spec.mjs:86-94` ("the brightness changes with no error", toast oculto, "bootKeeps 40") e as
  asserções de Armazenamento verificam só a seleção da UI, não a metade do bug no backend (coberta pelos testes Rust
  do DoD 3). Sugestão: dizer isso no cabeçalho do spec ou no título do teste, sem prometer o brilho do backend.
- **W2 (menor) — duas varreduras de `/proc` na recusa.** `SystemPorts::try_open` varre para os outros
  (`connector.rs:149`), e `SerialPorts::open` varre de novo para `held_here` (`connector.rs:128` → `busy.rs:99-104`);
  `holders_by_pid` já devolve os dois numa passada. Custo só no caminho de falha; informativo.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: a tela responde pelo display e pelo MCU; conectada pelo MCU volta com a chave do display | CONTEXT | Auto | PASS | `OK` (comando como escrito) |
| 2 | Devices: porta presa por este app falha na hora, sem wake nem restart | CONTEXT | Auto | PASS | `OK` (`ok. 2 passed`) |
| 3 | Studio: chave do SoC; brilho, release, restart e armazenamento por qualquer porta pelo link aberto; chave MCU reescrita | CONTEXT | Auto | PASS | `OK` (`ok. 6 passed`) |
| 4 | UI: lookup puro e Playwright nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` com `BEZEL_E2E_PORT=1442`; `-g "live screen controls"`: **8 passed** (2 × 4 projetos); `# pass 2` no `node --test` |
| 5 | CHANGELOG (Fixed) e checagem dos guias | CONTEXT | Auto | PASS | `OK`; entrada em `CHANGELOG.md:206-212` sob `### Fixed` do `[Unreleased]` |
| 6 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | `OK` com `HEAD` = `483c842`. Run **36900540628** (CI, `483c8422736d…`, `jdi/live-screen-controls`): `completed success`. Verdes: `rust-linux`, `rust-windows` (**811 passed, 0 failed, 11 ignored**; eram 802: +9, o teste de `busy` é `cfg(unix)`), `node-ui`, CodeQL (rust, js, actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` `skipped`, como esperado |
| 7 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Gate 2: 876/0/11 |
| 8 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.36% (comando como escrito) |
| 9 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 10 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` atualizado; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 11 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | Nenhum comportamento descrito no README mudou; evidência sugerida: diff do README revisado no PR |

As linhas 10 e 11 são do corte de release do projeto. A T-6 (8.8" real) não é linha de DoD: fica em "Deferred to PR
review", com o orquestrador, enquanto o studio do usuário segura a tela.

## Recommendation
Pode seguir para o PR e para a T-6 no hardware quando o usuário liberar a 8.8". Nenhum gate falha, as 9 linhas Auto
passam (incluindo o Windows no CI do `HEAD`), e cada teste do DoD falha sem a correção. Os dois avisos são menores e
não pedem mudança de comportamento: W1 é a honestidade do e2e/demo sobre o que ele prova (a metade backend do bug é
coberta em Rust), W2 é eficiência no caminho de falha. Corrigir W1 com um comentário no spec basta; W2 pode ir para
o backlog.

## DoD Critic (enhanced)

- Nenhuma linha oca: as 9 linhas Auto com PASS provam o critério (mutações numa cópia descartável: `syncLive` antigo
  derruba os 8 Playwright; checagem exata da chave em cada ponto derruba um teste do DoD 3; `connect_screen` sem
  relistar derruba o caso acordado do core). Suspeitas não objetivas, levadas à rodada de correção: a cola
  `SystemPorts::held_here` → `busy::held_here` sem teste (DoD 2); "For this screen" no `check-docs.sh` já satisfeito
  por `CHANGELOG.md:118` de uma fase anterior (DoD 5, a prova fica no grep da própria linha); escopo do grep de TODO
  do PROJECT mais estreito que o critério (fora da fase).

**Verdict:** APPROVED
