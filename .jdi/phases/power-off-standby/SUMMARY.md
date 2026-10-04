# Phase 12: Tela com o computador desligado (apagada, vídeo do cartão ou álbum) — Summary  (slug: power-off-standby)

**Status:** complete
**Tasks:** 8/8 complete, 0 blocked

> `/jdi-issue` autônomo (pedido do usuário de 2026-10-03, com a viabilidade medida no 8.8" no mesmo dia: D-1).
> Branch `jdi/power-off-standby` a partir de `main` (v0.15.0); um worktree por tarefa em 3 ondas, cherry-picked na
> ordem dos IDs. A apresentação de fotos e vídeos ficou para a fase `slideshow-builder`.

## Executed tasks
- T-1 `ca49a56`: core — `domain::standby` (`Standby`, `SleepMinutes` 1..10, `PlanB`, opções e motivos,
  `album_frame`), `ScreenRecord.standby` no `catalog.json` (ausente = `keep`), portas `set_options` (substitui
  `set_start_mode`), `restart`, `turn_off_now`; `app::standby::{show, choose, at_shutdown}`; a mídia de boot mantém o
  temporizador do `off`; `FakeScreen` grava `Options/Restart/TurnOffNow`. 8 testes em `tests/standby.rs` + 9 unitários.
- T-3 `f85a5b1`: crate `bezel-power` — `Logind` (inibidor *delay* "shutdown", `delay_max`, `wait` → `Shutdown`, erros
  tipados), `session_ending()` (`SM_SHUTTINGDOWN` no Windows, `false` fora — todo código só-Windows aqui, D-7) e a
  feature `fake` (`PrivateBus`, `FakeLogind` com fd de pipe, `BusMonitor`). 17 testes, 94,9 % de linhas; `Cargo.lock`
  só ganha `bezel-power`; `dbus` no `rust-linux` do CI.
- T-2 `e53146e`: driver rev C — `turn_off_now` = só 0x83, `restart` = só 0x84, play com no máximo um STOP_MEDIA antes
  e nada depois, keepalive de 30 s (pixel 0 + QUERY_STATUS, relógio injetável); `bezel_media::photo` (JPEG/PNG/BMP/GIF
  com EXIF → PNG nativo). 7 testes `standby_*` no simulador de firmware com as sequências exatas.
- T-4 `a0743a7`: studio — trava *delay* do logind desde o início; `PrepareForShutdown(true)` → estado final (nada mais
  alcança uma tela: `connect`, `lend_live_link`, `go_live`, `reconnect_due`, `tick`, `claim`), job cancelado e
  esperado, `at_shutdown` por tela acordada, fd fechado depois ou em `delay_max − 0,5 s`; `false` retoma; sem logind,
  `DiagCode` fixo e só o plano B; Windows: `Builder::build` + `App::run`, `RunEvent::Exit` + `session_ending()` (4 s).
  15 testes em `power::tests` (8 `linux_*`).
- T-5 `c541d27`: studio — `standby_overview`, `set_standby`, `pick_photo`, `album_preview`, `album_add` (catálogo
  relido a cada chamada, `Confirm::Yes` só com `confirmed`, tela ao vivo pelo link emprestado, `busy` no estado final,
  foto enquadrada na orientação da tela via `Manager::upload`). 9 testes `standby::tests`.
- T-6 `9abe518`: UI "Quando o computador desligar" em Tela › Ajustes (rádio por teclado, "?" em popover, motivos,
  diálogo do que será gravado, gerenciador do álbum com prévia horizontal/vertical), ponte e demo. 5 e2e × 4 projetos
  com axe; `npm test` 264/0.
- T-7 `606ea9f`: CLI `bezel standby show | set <keep|off|video|album> [--sleep] [--file] [--brightness] [--yes] |
  album add <foto> [--orientation] [--fit] [--name] [--yes]`; sem `--yes` nada enviado nem registrado. 6 testes do
  binário + 7 unitários.
- T-8 `8dd18e8`: guia en/pt-BR `power-off.md`, protocolo rev C §6.2/§16/§19 (7 linhas)/§20 com os fatos de
  2026-10-03, CHANGELOG, README, `check-docs.sh`.
- Integração (doer, depois do merge das 8): `680a978` miniaturas do álbum (lista pelo `manager_overview`, que o
  `manager_thumbnail` aceita; demo com a mesma guarda), `26a923a` texto do `bezel storage boot` (o temporizador de um
  `off` fica), `5276234` rótulos dos guias = i18n e `--brightness` (check-docs exige), `b948b68` guias de
  armazenamento e README com o que a tela mostra ao ligar (1º vídeo de `sd/video`, álbum de `sd/image`).

- Iteração 2 (fix_blockers, `d5922ee..5746fe9`, 16 commits): B1 `Inhibitor::release` (clippy msvc do studio limpo,
  conferido localmente só-check com toolchain substituto no scratchpad), B2 `album_add` com `replace` próprio
  (`notConfirmed` sem ele), prova da linha 3 pelo handler real do `App::run`, linha 5 com `FakeLog::kept` ordenado;
  W1–W7/W9: plano B gravado por último com o brilho (`stored`/`planB` no catálogo), `RecordedChoice`, prazo contado
  do sinal, diálogo do álbum preso durante o envio, guias verdadeiros sobre o plano B sem o studio. 1036 testes.

- Iteração 3 (fix_blockers, `1cdc8dd..0e588e9`, 7 commits): linha 2 do crítico provada por
  `linux_keep_opens_no_awake_screen_that_is_not_live` (falha sem `!= Keep`) e o simétrico; estado final no anúncio do
  logind; `at_shutdown(link, store, key)` lê a escolha no core; o `album` registra o plano B que grava; 0x7B do álbum
  (D-9); `createAnswers` na UI; o `set` mostra o brilho. 1041 testes, 94,90 % de linhas.

- Rodada de avisos (passo 6 do /jdi-issue, `b3f70d1..b654f8c`): catálogo travado por chamada no desligamento (`ArchivePerCall`), `Applied::AlbumNotRecorded` + `DiagCode::ShutdownAlbumNotRecorded`, a UI relê a tela depois de uma escrita concorrente; 1045 testes, 94,90 % de linhas.

## Deviations
- D-7 (orquestrador, antes do loop): contagens mínimas no DoD e Windows provado por teste + clippy msvc local.
- `bezel-power` devolve `Result` em `inhibit`/`delay_max`/`wait` (o PLAN dizia valor puro): exigido pelo próprio
  critério "sem logind = erro tipado".
- T-2: o limite de um STOP_MEDIA vale para todo play (o driver não sabe se é desligamento); o 8.8" responde na 1ª.
- Contratos mudados na iteração 2: `album_add` + `replace`, `choose(..., brightness, ...)`, `at_shutdown(link,
  store, key)` (iteração 3; `RecordedChoice` privado), `Applied::AlbumNotRecorded` (rodada de avisos), `Offer.videos: Vec<FileEntry>`.
- T-6 ajustou `tests/ui/storage.test.mjs`; T-5 ganhou `StorageState::scratch()`; T-7 tornou `pub(crate)` 4 helpers de
  `storage.rs` da CLI (fora de `files_modified`, mínimos).

## Gates
- `fmt`, `clippy -D warnings` (Linux e msvc sem studio), `cargo test --workspace`, `npm test`, `check-docs.sh` e as 8
  linhas do DoD: OK em `b654f8c` (1045 testes, 94,90 %); CI `37159714157` verde, `rust-windows` incluso.
  `studio-starts-silent.sh`: OK (o studio de teste nunca abriu `/dev/ttyACM*`).

## Notes
- Fica para o PR: desligar de verdade com o 8.8" em cada opção e religar; desligar no Windows; revisão visual.
- Studio do Windows só compila no CI (`ring` exige MSVC); o código só-Windows está no `bezel-power` (clippy local).
