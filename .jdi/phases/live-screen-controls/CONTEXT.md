# Phase 10: Brilho, cartão SD e filtro de temas com a tela ao vivo — Context  (slug: live-screen-controls)

## Goal
Com a tela ao vivo no studio, o brilho muda pelo link aberto, o cartão SD aparece e é gerenciável e "Para esta tela" fica estável e clicável.

## Locked decisions
- D-1: relato de 2026-10-01 (0.1.0-dev.287, 8.8" ao vivo): brilho "Device or resource busy", SD invisível, "Para esta tela" pisca. Causa comum: duas chaves para uma tela (UI = SoC `/dev/ttyACM1`, sessão ao vivo = MCU `/dev/ttyACM0`).
- D-2: uma tela, uma chave: a chave ao vivo é o endereço do display relistado logo após conectar (caso de uso no core, como `reopen_screen`); `set_live` (bandeja e retomada do restart) grava `liveScreen` e orientação com ela; `restore_live` reescreve uma chave MCU salva.
- D-3: "é a tela ao vivo?" por identidade: `Screen::answers_to` (display ou wake) e `Studio::is_live` em `link_of`/lend/return, `live_brightness`, `missing_video`, restart, release e `refuse_while_live`; qualquer porta usa o link aberto, nunca reabre.
- D-4: porta presa por este processo → `InUse` na hora, sem esperar sumir, acordar o MCU nem reiniciar; outros programas seguem device-protocols-3.
- D-5: a UI só seleciona tela listada: `src/live-screen.js` (`liveScreenIn`) no `syncLive`; Armazenamento, brilho e filtro usam a chave listada; cenário demo `mcuLive` reproduz o bug.

## Canonical refs
- Card: relato do usuário de 2026-10-01 (colado), `.jdi/decisions/D-2026-10-01-live-screen-controls-{1..5}.md`, release-polish-13, device-protocols-3
- core `domain/discovery.rs`, `app/screens.rs`; devices `connector.rs`, `busy.rs`; studio `backend.rs`, `studio.rs`, `storage.rs`, `manager.rs`; UI `app.js`, `ui/{library,storage}.js`, `demo-backend.js`

## Out of scope
- COM presa pelo próprio app no Windows, wake do MCU com SoC acordado (hardware), brilho em fila durante job: `.jdi/todos/2026-10-01-live-screen-controls.md`.

## Definition of Done

### Auto-verifiable
- [ ] Core: a tela responde pelo display e pelo MCU; conectada pela porta MCU, volta com a chave do display
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::discovery::tests::a_screen_answers_to_its_display_and_its_mcu_port 2>&1 | grep -q 'ok. 1 passed' && cargo test -p bezel-core --locked --test screens -- --exact a_screen_connected_by_its_mcu_port_is_keyed_by_its_display 2>&1 | grep -q 'ok. 1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Devices: porta presa por este app falha na hora, sem wake nem restart
      **Verify:** `cargo test -p bezel-devices --locked --lib -- --exact connector::tests::a_port_this_app_holds_fails_at_once_without_a_wake busy::tests::this_process_is_told_apart_from_other_holders 2>&1 | grep -q 'ok. 2 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Studio: ao vivo pela porta MCU fica com a chave do SoC; brilho, release, restart e armazenamento por qualquer porta usam o link aberto (1 connect, reabrir daria busy); chave MCU salva é reescrita
      **Verify:** `cargo test -p bezel-studio --locked --lib -- --exact backend::tests::a_screen_put_live_by_its_mcu_port_is_keyed_by_its_display backend::tests::either_port_of_the_live_screen_reaches_its_live_link backend::tests::a_saved_mcu_key_is_restored_under_the_display_key studio::tests::the_live_screen_is_known_by_either_of_its_ports manager::tests::the_overview_of_the_live_screen_borrows_its_link storage::tests::playing_a_file_is_refused_on_either_port_of_the_live_screen 2>&1 | grep -q 'ok. 6 passed' && echo OK`
      **Source:** CONTEXT
- [ ] UI: lookup puro (alias MCU) e Playwright nos 4 projetos com axe: ao vivo pela porta MCU, por > 5 s, Armazenamento mostra interna e cartão e "Para esta tela" fica habilitado e `aria-pressed="true"`
      **Verify:** `set -o pipefail; cd apps/bezel-studio && grep -q 'await expectAccessible(' tests/e2e/live-screen-controls.spec.mjs && node --test --test-name-pattern='MCU port|unlisted' --test-reporter=tap tests/ui/live-screen.test.mjs | awk '/^# pass 2$/{ok=1} END{exit !ok}' && npm run test:unit >/dev/null && npx playwright test -g "live screen controls" --reporter=line 2>&1 | awk '{for(i=1;i<NF;i++) if($(i+1)=="passed") n=$i} END{exit !(n>=8)}' && echo OK`
      **Source:** CONTEXT
- [ ] CHANGELOG (Fixed) e checagem dos guias
      **Verify:** `f=$(sed -n '/^## \[Unreleased\]/,/^## \[[0-9]/p' CHANGELOG.md | sed -n '/^### Fixed/,$p'); grep -qi brightness <<<"$f" && grep -qi 'SD card' <<<"$f" && grep -q 'For this screen' <<<"$f" && bash scripts/ci/check-docs.sh >/dev/null && echo OK`
      **Source:** CONTEXT
- [ ] CI do Windows verde no HEAD do PR
      **Verify:** `s=$(git rev-parse HEAD); i=$(gh run list -w CI -b jdi/live-screen-controls -L 20 --json databaseId,headSha -q "map(select(.headSha==\"$s\"))[0].databaseId"); gh run view "$i" --json jobs -q '.jobs[]|select(.name|endswith("rust-windows")).conclusion' | grep -qx success && echo OK`
      **Source:** CONTEXT

### Manual
- _(none)_

## Deferred to PR review
- 8.8" real ao vivo pelo studio: o brilho muda sem erro (orquestrador valida quando o studio do usuário liberar a tela; SUMMARY).
- Armazenamento mostra interna e cartão SD; uma operação com `bezel_test_*` funciona; arquivos do usuário intactos.
- "Para esta tela" segue habilitado e usável ao vivo.
- Relançar o studio (`--hidden`) retoma o ao vivo e `settings.json` guarda a chave do SoC.
- Pelos logs: qual caminho gravou `/dev/ttyACM0`.

## Notes
- Ordem: core → devices → studio → UI/demo → docs; install-local ao fim.
- Nada na 8.8" durante a fase (ao vivo no studio do usuário): só fakes (`FakeBus::turing_88()`, `FakeConnector::refusing_after(1, [Transport busy])`).
