# Phase 12: Tela com o computador desligado (apagada, vídeo do cartão ou álbum) — Plan  (slug: power-off-standby)

## Goal
Por tela rev C: `keep`/`off`/`video`/`album` quando o PC desliga (Linux e Windows), com o plano B gravado na tela.

## Locked decisions (from CONTEXT.md)
- D-2026-10-03-power-off-standby-1..6; herdadas: D-1 (hexagonal), device-protocols-2, gif-sticker-search-10..19 (guardas)

## Contrato (dono entre parênteses)
- Core (T-1): `domain::standby::{Standby {Keep, Off(SleepMinutes 1..=10), Video(RemotePath), Album}, PlanB {start_mode, sleep_minutes}, album_frame(picture, model, orientation, VideoFit) -> Frame}` (na forma da tela, girada ao painel); `ScreenRecord.standby` (padrão `Keep`). Portas: `ScreenStorage::set_options(PlanB, Confirmed)` substitui `set_start_mode` (OPTIONS inteiro: brilho do link, modo, flip 0, temporizador); `ScreenStorage::restart(Confirmed)` (0x84; padrão `Unsupported`); `ScreenLink::turn_off_now()` (padrão `screen_off`). `app::standby::{show, choose(link, store, key, Standby, Confirm) -> PlanB, at_shutdown(link, &Standby) -> Applied}`.
- `bezel-power` (T-3): `BusAddress::{system(), new}`; `Logind::connect(&BusAddress)`, `.inhibit(reason) -> Inhibitor` (drop fecha o fd), `.delay_max()`, `.wait(timeout) -> Option<Shutdown {Starting, Cancelled}>`; `session_ending() -> bool` (Windows `SM_SHUTTINGDOWN`, `false` fora). Feature `fake` (Linux): `PrivateBus` (sobe `dbus-daemon`), `FakeLogind` (fd = ponta de um `std::io::pipe`; grava chamadas, vê o EOF, emite `PrepareForShutdown`), `BusMonitor`.
- UI ↔ studio (T-5): `standby_overview {screen}` → `StandbyDto {choice, sleepMinutes|null, file|null, options:[{choice, enabled, reason|null}], videos, card, orientation}` (motivos `notConnected|unsupported|noCard|noVideo`); `set_standby {screen, choice, sleepMinutes, file, confirmed}` → `StandbyDto`; `pick_photo` → caminho|null; `album_preview {screen, source, fit}` → `data:` PNG; `album_add {screen, source, fit, name, confirmed}` → `{path, bytes}`. Lista e remoção do álbum: `storage_overview`/`delete_stored` de hoje. Nenhum código de erro novo.
- CLI (T-7): `bezel standby show`; `bezel standby set <keep|off|video|album> [--sleep N] [--file <internal|sd>/video/<nome>] [--yes]`; `bezel standby album add <foto> [--orientation ...] [--fit cover|contain] [--name ...] [--yes]`.

## Tasks
Specialist: `jdi-doer-bezel` (todas).

### Wave 1 (paralela)

#### T-1: Core: escolha, plano B, ações e registro no catálogo
- **DoD:** 1 · **Dependencies:** none
- **Files modified:** `crates/bezel-core/src/{domain/standby,domain/mod,domain/archive,domain/storage,ports/mod,app/standby,app/storage,app/manager,app/mod}.rs`, `crates/bezel-core/tests/{standby,storage}.rs`, `crates/bezel-devices/src/{fake,driver/turing_rev_c,driver/turing_usb}.rs`, `crates/bezel-media/src/archive/{disk,tests}.rs`
- **Acceptance:**
  - Contrato do core; nos drivers só a troca para `set_options` (rev C: o `Options` de hoje; TUR_USB `Unsupported`). Boot e plano B escrevem OPTIONS inteiro a partir do registro (D-2 (4)); `keep`→`keep` nada; família ≠ rev C = `Unsupported` sem chamada; `Confirm::No` = zero chamadas e catálogo igual.
  - `at_shutdown`: `off` = `turn_off_now`; `video` = `play_video` loop; `album` = `set_options` (modo 1, 0) + `restart`; arquivo ausente ou sem cartão → `turn_off_now`; `keep` nada. A prova vem da escolha registrada.
  - `standby` no `DiskArchive` (campo opcional: catálogo antigo = `keep`). `FakeScreen` grava `Options`, `Restart`, `TurnOffNow` (mantém `FakeStorage::start_mode`).
- **Test:** `tests/standby.rs`, ≥ 6 (No/Unsupported; off/album/video; keep desfaz; boot ↔ temporizador; ações; impossível→TURNOFF); `domain::standby::tests`: `album_frame` vertical e horizontal
- **Status:** completed

#### T-3: Adaptador `bezel-power`: logind e fim de sessão
- **DoD:** 3, 7 · **Dependencies:** none
- **Files modified:** `Cargo.toml`, `Cargo.lock`, `crates/bezel-power/{Cargo.toml,src/lib.rs,src/logind.rs,src/session.rs,src/fake.rs}`, `.github/workflows/ci.yml`
- **Acceptance:**
  - `dbus` 0.9 só Linux (feature `stdfd`: fd sem `unsafe`), `windows-sys` 0.61 só Windows; no `Cargo.lock` só `+name = "bezel-power"`. Único `unsafe`: `GetSystemMetrics`, com `// SAFETY:`.
  - Conecta só ao endereço recebido; fala só com `org.freedesktop.login1` (fora `Hello`/`AddMatch` do barramento); `Inhibit("shutdown", "Bezel", reason, "delay")`; `delay_max` = `InhibitDelayMaxUSec`; sem barramento ou logind = erro tipado, sem pânico.
  - Testes falham (não pulam) sem `dbus-daemon`; `ci.yml`: `dbus` no `rust-linux`.
- **Test:** `logind::tests::speaks_only_to_logind_on_its_bus` (`BusMonitor` vê o cliente inibir, ler o prazo e receber `true`/`false`); fechar = EOF no fake
- **Status:** completed

#### T-6: UI: "Quando o computador desligar", álbum e demo
- **DoD:** 6 · **Dependencies:** none
- **Files modified:** `apps/bezel-studio/` + `src/{standby,demo-standby,demo-backend,demo-data,bridge,app}.js`, `src/ui/{standby,library}.js`, `src/{index.html,styles.css}`, `src/i18n/{en,pt-BR}.js`, `tests/ui/{standby,demo-backend}.test.mjs`, `tests/e2e/standby.spec.mjs`
- **Acceptance:**
  - D-6 (1) em Tela › Ajustes: rádio por teclado, "?" em popover, desabilitada com motivo; escolher abre o diálogo in-app do que será gravado (`off` 1–10 min, sugestão 5; `video` interna e cartão, com o limite; `album`: gerenciador de `sd/image`, prévia na forma da tela antes de enviar, Preencher/Caber, remover confirmado com o nome).
  - Lógica pura em `src/standby.js`; `bridge.js` com o contrato; i18n com paridade, sem string fixa no JS; claro/escuro; movimento reduzido; demo simula tudo, inclusive `?demo=noCard`.
- **Test:** `tests/ui/standby.test.mjs` (lógica, demo, ponte); Playwright `standby` › os 5 títulos da DoD 6 × 4 projetos com `watchErrors` e `expectAccessible`; `npm test` (≥ 80%)
- **Status:** completed

#### T-8: Guia en/pt-BR, protocolo §19/§20, CHANGELOG, check-docs
- **DoD:** 8 · **Dependencies:** none
- **Files modified:** `docs/user/{power-off,README}.md`, `docs/user/pt-BR/{power-off,README}.md`, `docs/reverse-engineering/protocol-turing-rev-c.md`, `scripts/ci/check-docs.sh`, `CHANGELOG.md`, `README.md`
- **Acceptance:**
  - Guia: 4 opções; plano B (temporizador só com `off`; modo 2 = 1º vídeo de `sd/video`; modo 1 gira `sd/image`); vale ao desligar/reiniciar; `bezel standby`; álbum; nos índices.
  - Protocolo: §6.2/§16 com os fatos e a exceção de D-2 (5); §19 com `Start mode 1`, `Start mode 2`, `Sleep timer`, `RESTART 0x84`, `Video after the host`; §20 sem a pergunta respondida. CHANGELOG `[Unreleased]` cita "shuts down"; `"power-off.md"` em `PAGES`; README cita a escolha.
- **Test:** DoD 8
- **Status:** completed

### Wave 2 (paralela)

#### T-2: Driver rev C (ações, keepalive) e foto do álbum
- **DoD:** 1 · **Dependencies:** T-1
- **Files modified:** `crates/bezel-devices/src/{driver/turing_rev_c,driver/mod,protocol/turing_rev_c}.rs`, `crates/bezel-media/src/{lib,photo}.rs`
- **Acceptance:**
  - `set_options` = 0x7D inteiro; `turn_off_now` = só 0x83, sem ler a saída do SoC; `restart` = só 0x84, sem esperar; `play_video` loop sem 0x87 depois; nenhum 0x66/0x6F/0x81/0x82/0x87 nesses caminhos.
  - Keepalive (D-5): quadro igual e ≥ 30 s sem envio → 1 parcial mínimo (um pixel com o valor da tela) + QUERY_STATUS; antes, nada; relógio injetável (teste não dorme).
  - `bezel_media::photo`: JPEG/PNG/BMP/1º quadro de GIF com a orientação EXIF, `album_frame`, PNG nativo; sem ffmpeg.
- **Test:** ≥ 4 `driver::turing_rev_c::tests::standby_*` no simulador de firmware (responde PLAY_VIDEO): OPTIONS inteiro; as 3 ações = pacotes exatos; keepalive; nada além. `photo::tests`: JPEG com EXIF 6 (APP1 montado no teste), vertical e horizontal
- **Status:** completed

#### T-4: Studio: desligamento (logind, Windows) e estado final
- **DoD:** 2, 3, 7 · **Dependencies:** T-1, T-3
- **Files modified:** `apps/bezel-studio/src-tauri/` + `Cargo.toml`, `src/{power,lib,studio,backend,storage,diag}.rs`, `src/power/tests.rs`; `Cargo.lock`
- **Acceptance:**
  - `Start` recebe o `BusAddress` (teste: privado). No início 1 inibidor *delay*; sem logind, `DiagCode` fixo e só o plano B. `true`: relê o catálogo, estado final, `at_shutdown` em cada rev C acordada com escolha ≠ `keep` (a ao vivo pelo link aberto; a que dorme, nada), fecha o fd depois ou no prazo (`delay_max − 0,5 s`); `keep` solta já; `liveScreen` fica; `false` sai do estado, re-inibe e retoma o ao vivo.
  - Estado final onde todo link nasce ou é emprestado (`Backend::connect`, `lend_live_link`, `go_live`, `reconnect_due`, `tick`, `StorageState::claim`); job em curso cancelado e esperado no prazo.
  - `run`: `Builder::build` + `App::run`; `RunEvent::Exit` com `session_ending()` → mesma sequência, síncrona, 4 s; sair pelo app não aplica. Guardas da DoD 7 sem relaxar regra; `the_app_setup_sends_nothing_at_start` com o `Start` novo (bus privado).
- **Test:** ≥ 6 `power::tests::linux_*` = as cláusulas da DoD 2, com `setup` real (runtime mock), `PrivateBus`, `FakeLogind`, `FakeConnector` e um conector que trava. `power::tests::a_session_end_applies_the_choice_and_a_quit_does_not` sem runtime mock (roda no Windows)
- **Status:** completed

### Wave 3 (paralela)

#### T-5: Studio: comandos da escolha e do álbum
- **DoD:** 4, 7 · **Dependencies:** T-1, T-2, T-4
- **Files modified:** `apps/bezel-studio/src-tauri/` + `{build.rs,capabilities/default.json}`, `src/{standby,commands,lib,dto}.rs`, `src/standby/tests.rs`
- **Acceptance:**
  - 5 comandos em `build.rs`, capability e `generate_handler!`. `set_standby`: tela conectada (a ao vivo pelo link aberto), `Confirm::Yes` só com `confirmed`; catálogo relido a cada chamada.
  - `album_add`: `photo` + `album_frame` na `screenOrientations` da tela (senão a do modelo), PNG em `<cache>`, `Manager::upload` para `sd/image` (cópia e catálogo); nome existente pede confirmação; sem cartão recusa antes de enviar.
- **Test:** ≥ 5 `standby::tests` = a DoD 4 (catálogo em disco gravado por outro escritor; álbum vertical e horizontal); `tests::every_command_is_allowed_by_name`
- **Status:** completed

#### T-7: CLI `bezel standby`
- **DoD:** 5 · **Dependencies:** T-1, T-2
- **Files modified:** `crates/bezel-cli/src/{lib,main,standby}.rs`, `crates/bezel-cli/tests/standby.rs`
- **Acceptance:** contrato; `show` por tela conectada; `set` sem `--yes`: nada enviado nem registrado, e a saída diz isso; com `--yes`, plano B e escolha no catálogo em disco `<data>/bezel/storage` (também com `--fake`); família ≠ rev C = `Unsupported`; `album add`: `--orientation` (padrão do modelo), `--fit` (cover), nome sugerido, `--yes` para substituir.
- **Test:** `--test standby`, ≥ 5 do binário = a DoD 5 (`--fake`, `XDG_DATA_HOME`/`APPDATA` temporários; `DiskArchive` lê a escolha; JPEG com EXIF 6 → cópia local 480x1920 enquadrada)
- **Status:** completed

## Execution
- 3 waves (4 → 2 → 2), worktree e `CARGO_TARGET_DIR` por task. Commits na ordem dos IDs (CONTEXT): T-6 após T-5, T-8 por último. `install-local.sh` pelo orquestrador ao fim.
- Speedup paralelo estimado: 2,7x

## DoD → task
1 T-1, T-2 · 2 T-4 · 3 T-3, T-4 · 4 T-5 · 5 T-7 · 6 T-6 · 7 T-3, T-4, T-5 (script pelo orquestrador) · 8 T-8 · PROJECT: todas · Deferred: PR

## Files modified (all tasks)
- Os de cada task (disjuntos por wave).

## Test requirements
- Workspace `--locked`: `cargo test`, `fmt --check`, `clippy -D warnings`, `llvm-cov --fail-under-lines 80`; clippy Windows (msvc) sem studio/klipy; studio no `rust-windows` do CI
- `npm test` (studio); `check-docs.sh`; `studio-starts-silent.sh`

## Notes
- Contagens do DoD mínimas (D-7: ≥ N, 0 falhas); `--exact` só nos nomes citados.
- DoD pela entrada de produção (driver no simulador, `setup` real, `Logind` real no bus privado, binário); runtime mock só fora do Windows, D-Bus só Linux.
- Commits escopo `power-off-standby`; `Cargo.lock` com a task que o muda. Fora: `.jdi/todos/2026-10-03-power-off-standby.md`.
