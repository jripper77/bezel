# Phase 10: Brilho, cartão SD e filtro de temas com a tela ao vivo — Summary  (slug: live-screen-controls)

**Status:** partial
**Tasks:** 5/6 complete, 0 blocked (T-6: hardware fica para o PR enquanto o studio do usuário segura a tela)

> `/jdi-issue` autônomo (card colado: relato do usuário de 2026-10-01, studio 0.1.0-dev.287+4feaa0f). Branch
> `jdi/live-screen-controls` a partir de `main` com o PR #1 mergeado; um worktree por tarefa, cherry-picked.

## Causa
Uma tela, duas chaves: a UI conhece a 8.8" pelo SoC (`/dev/ttyACM1`), a sessão ao vivo ficou com a porta do MCU
(`/dev/ttyACM0`, gravada em `liveScreen` e repetida a cada relançamento) e toda checagem "é a tela ao vivo?" comparava
strings. Brilho e Armazenamento reabriam a porta que o próprio studio segura (busy, depois de ~5 s de espera e wake
do MCU); a UI alternava a tela escolhida entre a chave ao vivo (fora da lista) e a listada, e "Para esta tela"
piscava.

## Executed tasks
- T-1 `a7b70bf`: core — `Screen::answers_to` (display ou wake; `choose_screen` usa o mesmo predicado) e
  `connect_screen` (escolhe, conecta e relista como `reopen_screen`; sem relistar, a tela escolhida).
- T-2 `7b147f2`: devices — `busy` separa este processo dos outros detentores; `SerialPorts::open` (padrão sobre
  `try_open`) devolve `InUse` com este app na hora quando a porta que falhou é nossa: sem esperar sumir, sem wake do
  MCU, sem restart. Outros programas seguem device-protocols-3.
- T-3 `354ce0f`: studio — `Studio::is_live` por identidade em `link_of`, lend/return, `live_brightness`,
  `missing_video`, `presented`, restart, release e `refuse_while_live`; `set_live` (bandeja, retomada do restart e
  `restore_live`) chaveia sessão, `liveScreen` e orientação pelo display relistado — a `/dev/ttyACM0` salva do
  usuário vira `/dev/ttyACM1` no próximo início.
- T-4 `642cb7d`: UI — `src/live-screen.js` (`liveScreenIn`, `answersTo`); `syncLive` só escolhe tela listada; demo
  `mcuLive` reproduz o 0.1.0-dev.287 e reconhece a tela ao vivo pelas duas portas como o backend;
  `live-screen-controls.spec.mjs` com o relógio do Playwright (6 s sem espera real).
- T-5 `a2a449f`: CHANGELOG (`### Fixed`) e `check-docs.sh` exige "For this screen".
- Step 6 (avisos da iter 1 e suspeitas do crítico): `4482f13` (W1: o e2e diz que prova só a metade da UI; a do
  backend fica com os testes Rust do DoD 3), `9f8fb1a` (W2: uma varredura do `/proc` por abertura, `Holders` com
  este processo e os outros; outros programas seguem recusados antes de tocar a porta), `9311ba4` (a cola
  `SystemPorts::holders` → `/proc` real com teste), `43921ff` (`check-docs.sh` exige "Device or resource busy", só
  da entrada desta fase).
- Iter 3 (crítico da iter 2: DoD 2 oca e objetiva → BLOCKED): o Verify da linha 2 passa a rodar também
  `the_host_ports_name_this_process_for_a_port_it_holds` (`0acd3f1`; com `this: None` a linha agora falha);
  `2a58bf1` (W3: saem `busy::holders` e `busy::held_here`, sem chamador; os testes usam `on_this_machine`).
- Iter 4 (crítico da iter 3: nenhum teste da linha 2 rodava `SystemPorts::open`): `872283e` (o teste do `/proc` real
  abre a porta segura por `SystemPorts::open` e pelo caminho rev C com `SystemPorts`, sem `/dev/tty*`: `InUse`, zero
  pausas; um `open` sobrescrito ou `this: None` derrubam a linha), `05c8630` (W4: sai `busy::holders_in`).
- Iter 5 (crítico da iter 4: nada fixava a abertura exclusiva): `7883e97` (`SerialWire::open` por `settings()` com
  `.exclusive(true)` explícito, builder igual campo a campo ao de antes; numa pty segura, a 2ª abertura dá EBUSY e
  `.exclusive(false)` derruba o teste), `2a4a84f` (W5: `open_serial` numa porta segura dá `InUse`); o Verify da
  linha 2 passa a rodar `wire::tests::a_port_the_wire_holds_refuses_a_second_open` (`ok. 4 passed`).

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/{domain/discovery,app/screens,app/mod}.rs`, `crates/bezel-core/tests/screens.rs`
- `crates/bezel-devices/src/{busy,connector,wire}.rs`
- `apps/bezel-studio/src-tauri/src/{studio,backend,storage}.rs`, `apps/bezel-studio/src-tauri/src/{storage,manager}/tests.rs`
- `apps/bezel-studio/src/{live-screen,app,demo-backend,demo-data}.js`,
  `apps/bezel-studio/tests/{ui/live-screen.test.mjs,ui/demo-backend.test.mjs,e2e/live-screen-controls.spec.mjs}`
- `CHANGELOG.md`, `scripts/ci/check-docs.sh`

## Tests
- `cargo test --workspace --locked`: 879 passando, 0 falhando, 11 ignorados (hardware e ffmpeg real)
- DoD 1–5: OK a cada iteração; DoD 6: runs 36900540628 (`483c842`, Windows 811/0/11), 36904730390 (`a566e80`,
  812/0/11) e 36908306551 (`19913fa`, 812/0/11) verdes
- UI: 186 unitários (99,93% de linhas); Playwright 184/184 (claro/escuro × pt-BR/en, axe), 8 novos
- fmt, clippy `-D warnings` (Linux e `--target x86_64-pc-windows-msvc`), `check-docs.sh`, `check-packaging.sh`
- Cobertura (T-3, `cargo llvm-cov -p bezel-studio`): `backend.rs` 98,14%, `studio.rs` 97,00%, `storage.rs`
  97,06%, `manager.rs` 94,42%
- Testes não ocos: os 6 do studio falham com o comportamento antigo de volta (só a identidade: 4; só o `set_live`:
  2); os 2 e2e falhavam antes do `syncLive` novo; o do connector falhava (passava pela espera e pelo wake).

## Hardware validation
- T-6 (orquestrador): o studio do usuário (PID 2613288) segura `/dev/ttyACM1` ao vivo; nada foi feito na tela.
  `settings.json` (mtime 14:02) tem `liveScreen` `/dev/ttyACM0`; o log do studio vai para `/dev/null` (relançado com
  `setsid`) e o journal só tem a sessão das 07:34, então o caminho que gravou a chave do MCU não é visível. Os
  candidatos (bandeja com a tela dormindo, retomada do restart, ir ao vivo com a tela listada dormindo) passam todos
  por `set_live` e estão cobertos por `a_screen_put_live_by_its_mcu_port_is_keyed_by_its_display`.

## Observações
- Windows: uma COM que o próprio app segura ainda não é reconhecida (backlog); lá só D-2/D-3 evitam reabrir.
- "Ocupada" = erro `Transport` com a porta nossa (o texto do serialport pode vir traduzido); `AccessDenied` e
  `InUse` de outros programas passam intactos.
- A orientação lembrada sob a chave do MCU não migra: se acerta na primeira vez ao vivo pela chave do display.
