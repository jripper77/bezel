# Phase 10: Brilho, cartão SD e filtro de temas com a tela ao vivo — Plan  (slug: live-screen-controls)

## Goal
Com a tela ao vivo no studio, o brilho muda pelo link já aberto, o cartão SD da tela aparece e pode ser gerenciado, e o filtro "Para esta tela" da aba Temas fica estável e clicável.

## Locked decisions (from CONTEXT.md)
- D-2026-10-01-live-screen-controls-1..5 (uma tela, uma chave; identidade em vez de string; porta presa por este processo = `InUse` na hora; a UI só seleciona tela listada); herdadas: D-1 (hexagonal), device-protocols-3, release-polish-13

## Tasks

### Wave 1 (paralela)

#### T-1: Core: `Screen::answers_to` e `connect_screen` (chave do display)
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/discovery.rs`, `crates/bezel-core/src/app/{screens,mod}.rs`, `crates/bezel-core/tests/screens.rs`
- **Acceptance:**
  - `Screen::answers_to(&self, address: &str) -> bool` (display ou wake com esse endereço) é o predicado de `choose_screen`, sem cópia. `connect_screen(bus, connector, address) -> Result<(Screen, Box<dyn ScreenLink>)>` em `app::screens`, exportado: escolhe, conecta e relista por `find_again` (como `reopen_screen`); sem relistar, a tela escolhida. `open_screen`, `reopen_screen` e `restart_screen` mantêm assinatura e testes.
  - `///` em todo `pub` novo; core só com `thiserror`, sem I/O nem impl de porta em `src/`.
- **Dependencies:** none
- **Test:** `domain::discovery::tests::a_screen_answers_to_its_display_and_its_mcu_port` (display e MCU sim, outro endereço não, adormecida só pelo MCU); `--test screens`: `a_screen_connected_by_its_mcu_port_is_keyed_by_its_display` (`FakeBus::turing_88()` por `/dev/ttyACM0` → `/dev/ttyACM1`; `ScriptedBus` só com o MCU e depois acordada → display; endereço desconhecido → `ScreenNotFound`)
- **Status:** pending

#### T-2: Devices: porta presa por este processo falha na hora
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-devices/src/{busy,connector}.rs`
- **Acceptance:**
  - `busy` separa este PID dos outros detentores (mesma varredura de `/proc`; este processo descrito como os outros, `"<cmd> (PID n)"`); `holders()` segue "os outros"; sem `/proc` (Windows) = ninguém, sem `cfg` novo.
  - Abrir uma porta serial que falha como ocupada e que este processo segura → `InUse { address, holders: [este processo] }` na hora, sem texto novo: no rev C sem `wait_until_gone`, `poke`, abrir o MCU, restart nem pausa. A decisão fica em código que o teste exercita (`SerialPorts` responde "este processo segura?", `ScriptedPorts` só informa), e `open_serial` a usa em toda família serial. Outros programas: device-protocols-3 e testes de hoje intactos.
- **Dependencies:** none
- **Test:** `connector::tests::a_port_this_app_holds_fails_at_once_without_a_wake` (diário = 1 open do SoC, nenhum poke, nenhuma pausa), `busy::tests::this_process_is_told_apart_from_other_holders` (`/proc` falso com este PID e outros)
- **Status:** pending

#### T-4: UI e demo: só tela listada (`liveScreenIn`), cenário `mcuLive`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/{live-screen,app,demo-backend,demo-data}.js`, `apps/bezel-studio/tests/ui/{live-screen,demo-backend}.test.mjs`, `apps/bezel-studio/tests/e2e/live-screen-controls.spec.mjs`; só se preciso: `apps/bezel-studio/src/ui/{library,storage}.js`, `apps/bezel-studio/src/i18n/{en,pt-BR}.js`
- **Acceptance:**
  - `src/live-screen.js` puro: `liveScreenIn(screens, liveKey)` = a tela listada cuja `key`, `display.address` ou `wake.address` é a chave, senão `null`. `syncLive` seleciona a `key` listada dela; sem par, a seleção fica e o status segue "ao vivo"; a amostra de 1 s e o `refreshScreens` de 5 s não alternam mais a seleção.
  - Demo `mcuLive` (`SCENARIOS`): `setLive` grava a porta wake da tela como chave e `sample` a informa (0.1.0-dev.287). Nenhum texto novo (se surgir: i18n com paridade); nenhuma RegExp montada com chaves (Semgrep); funções curtas (Sonar).
  - Playwright `?demo=mcuLive`: ao vivo, passados > 5 s a tela escolhida segue a 8.8" (`/dev/ttyACM1`), Armazenamento mostra interna e cartão, "Para esta tela" habilitado e `aria-pressed="true"` depois de escolhido, brilho sem erro; `watchErrors` vazio e `expectAccessible`.
- **Dependencies:** none
- **Test:** `tests/ui/live-screen.test.mjs` com exatamente 2 testes casando `MCU port|unlisted` (outros nomes à vontade); `demo-backend.test.mjs` cobre `mcuLive`; ≥ 2 testes "live screen controls …" × 4 projetos; `npm test` (≥ 80% linhas)
- **Status:** pending

#### T-5: CHANGELOG (Fixed) e checagem dos guias
- **Specialist:** jdi-doer-bezel
- **Files modified:** `CHANGELOG.md`, `scripts/ci/check-docs.sh`
- **Acceptance:** `## [Unreleased]` → `### Fixed`, em inglês: com a tela ao vivo no studio (8.8" ligada pela porta do MCU), brightness muda sem "Device or resource busy", o SD card aparece e é gerenciável na aba Storage e o filtro "For this screen" fica estável; `check-docs.sh` passa a exigir "For this screen" no `[Unreleased]` (como "framing").
- **Dependencies:** none
- **Test:** DoD 5
- **Status:** pending

### Wave 2

#### T-3: Studio: uma chave ao vivo, todas as checagens por identidade
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/src/{studio,backend,storage}.rs`, `apps/bezel-studio/src-tauri/src/{storage,manager}/tests.rs`
- **Acceptance:**
  - `Studio::is_live(key)`: a chave ao vivo ou qualquer porta da tela ao vivo (`Screen::answers_to`); usado por `link_of`, `lend_live_link`, `return_live_link`, `live_brightness`, `missing_video`, o match de `presented`, `Backend::{restart_screen,release}` e `refuse_while_live`. `grep -nE 'live_key\(\) == Some|l\.key == key'` nos 3 arquivos = vazio.
  - `set_live` (bandeja e retomada do restart incluídas) usa `connect_screen`: sessão, `liveScreen` e orientação lembrada com o `address()` relistado (display); a chave passada só quando não relista. `restore_live` de `/dev/ttyACM0` regrava `/dev/ttyACM1`; `reconnected` segue re-chaveando.
  - Por qualquer porta, brilho, release e Armazenamento usam o link aberto: `f.connector.clone().refusing_after(1, vec![Transport("… Device or resource busy")])` (roteiro compartilhado) e `connects == 1`. Restart pelo MCU: para o ao vivo antes de `restart` (nenhum connect com o link aberto) e retoma com a chave do display.
- **Dependencies:** T-1
- **Test:** `backend::tests::{a_screen_put_live_by_its_mcu_port_is_keyed_by_its_display,either_port_of_the_live_screen_reaches_its_live_link,a_saved_mcu_key_is_restored_under_the_display_key}`, `studio::tests::the_live_screen_is_known_by_either_of_its_ports`, `manager::tests::the_overview_of_the_live_screen_borrows_its_link`, `storage::tests::playing_a_file_is_refused_on_either_port_of_the_live_screen`
- **Status:** pending

### Wave 3

#### T-6: HARDWARE — a 8.8" ao vivo pelo studio (orquestrador)
- **Specialist:** orquestrador na Turing 8.8" real
- **Files modified:** `.jdi/phases/live-screen-controls/SUMMARY.md` (§ Hardware validation)
- **Acceptance:**
  - A qualquer hora, sem tocar na tela: `liveScreen` do `settings.json` e o log do studio dizem qual caminho gravou `/dev/ttyACM0` (bandeja com a tela dormindo, retomada do restart, reconexão).
  - Nada em `/dev/ttyACM*` enquanto o studio do usuário segura a tela (`bezel storage ls` → "in use by bezel-studio"). Só com a tela liberada por ele, após o `install-local`: `bezel storage ls` antes e depois iguais; o studio instalado relançado com `--hidden` retoma o ao vivo, grava `/dev/ttyACM1` e o log não tem "busy"; estado encontrado restaurado (studio dele, serviço). Brilho, SD com `bezel_test_*` e "Para esta tela" ficam com o usuário no PR.
  - Tela não liberada → registrado no SUMMARY e no PR como Deferred to PR review (não `blocked`).
- **Dependencies:** T-1, T-2, T-3, T-4, T-5
- **Test:** itens de "Deferred to PR review"
- **Status:** pending

## Execution
- 6 tasks em 3 waves (4 → 1 → 1); 1 worktree por task em `~/repos/bezel-wt/` com `CARGO_TARGET_DIR` próprio. IDs na ordem do CONTEXT (core → devices → studio → UI → docs): cherry-pick de T-1, T-2 após a W1 e de T-3, T-4, T-5 após a W2. `scripts/install-local.sh` pelo orquestrador antes da T-6 (não é task).
- Speedup paralelo estimado: 2x

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| 1 Core: display e MCU, chave do display | T-1 |
| 2 Devices: porta deste app falha na hora | T-2 |
| 3 Studio: chave do SoC, link aberto por qualquer porta, chave MCU reescrita | T-3 |
| 4 UI: lookup e Playwright ≥ 8 com axe | T-4 |
| 5 CHANGELOG e guias | T-5 |
| 6 CI Windows no HEAD | todas (clippy Windows); orquestrador após o push |
| Deferred: 8.8" ao vivo pelo studio | T-6 |
| PROJECT: testes, cobertura, TODO | todas |

## Files modified (all tasks)
- `crates/bezel-core/src/{domain/discovery,app/screens,app/mod}.rs`, `crates/bezel-core/tests/screens.rs`
- `crates/bezel-devices/src/{busy,connector}.rs`
- `apps/bezel-studio/src-tauri/src/{studio,backend,storage}.rs`, `.../src/{storage,manager}/tests.rs`
- `apps/bezel-studio/src/{live-screen,app,demo-backend,demo-data}.js`, `apps/bezel-studio/tests/{ui/live-screen.test.mjs,ui/demo-backend.test.mjs,e2e/live-screen-controls.spec.mjs}` (+ `ui/{library,storage}.js`, i18n se preciso)
- `CHANGELOG.md`, `scripts/ci/check-docs.sh`, `.jdi/phases/live-screen-controls/SUMMARY.md`

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`
- Windows (DoD 6): `cargo clippy --workspace --exclude bezel-studio --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings` por task Rust; teste novo de `connector` sem import só de Unix (só `busy::tests` é `cfg(unix)`); após o push, `rust-windows` verde no HEAD do PR
- `cd apps/bezel-studio && npm test`; `bash scripts/ci/check-docs.sh`; `cargo llvm-cov --workspace --locked --fail-under-lines 80` (mínimo 80%, PROJECT)

## Notes
- Donos únicos: W1 T-1 core, T-2 `bezel-devices`, T-4 JS/e2e (e i18n, se houver texto novo), T-5 `CHANGELOG.md`/`check-docs.sh`; W2 T-3 studio Rust. Nenhuma variante nova em `BezelError`/`ErrorCode`; cada task compila o workspace sozinha.
- Só fakes (`FakeBus::turing_88()`, `FakeConnector::refusing_after(1, [Transport busy])`); nenhuma task toca a 8.8" (ao vivo no studio do usuário).
- Nomes de teste do DoD exatos (`--exact`): direto no `mod tests` de cada arquivo, sem submódulo; o do core no topo de `tests/screens.rs`.
- Commits: conventional, escopo `live-screen-controls`, cabeçalho ≤ 72; código e `.jdi/` em commits separados.
- Fora: COM presa pelo próprio app no Windows, wake do MCU com SoC acordado, brilho em fila durante job (`.jdi/todos/2026-10-01-live-screen-controls.md`).
