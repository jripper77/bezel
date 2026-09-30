# Phase 2: Protocolos de dispositivo — Summary  (slug: device-protocols)

**Status:** complete
**Tasks:** 8/8 complete, 0 blocked

> Nota de processo: esta fase foi executada fora do `/jdi-do` (orquestrador + agentes genéricos em worktrees
> paralelos, commits cherry-picked para a `main`), sem despachar o `jdi-doer-bezel` tarefa a tarefa. Os
> commits seguem as convenções da fase; este SUMMARY e os status do PLAN foram escritos ao final, a partir
> do histórico e de verificações reexecutadas agora.

## Executed tasks
- T-2.1: `Frame` RGBA com crop/fill/rotação, `dirty_rects` em tiles de 16 px, `Brightness`/`ScreenIdentity`,
  portas `ScreenConnector`/`ScreenLink`, casos de uso `choose_screen`/`open_screen`, erro `InUse` — `d126a49`,
  `8903926`, `9aac3de`
- T-2.2: rev C sobre serial (`Flow::None`), HELLO/ROM, frame cheio `C8` e parciais `CC` em run-list, espera
  de `STOP_MEDIA`, fallback para frame cheio em `needReSend`, wake pelo MCU, recusa de porta ocupada;
  reconexão que acorda uma tela em desligamento (TURNOFF de outro app) — `90ac992`, `571bfed`, `cdf3223`
- T-2.3: `bezel test-pattern`, `brightness`, `release`, `-v`; depois `show <imagem>` (orientação automática
  pelo formato, `--fit`), `off`, e nomes `vertical`/`horizontal` para as orientações — `bb02945`, `91945f2`
- T-2.4: Turing rev A (pacotes de 6 bytes, RGB565 LE) e XuanFang rev B (10 bytes, RGB565 BE) — `b21613c`,
  `29e5e04`
- T-2.5: Kipye rev D (comandos de 4 bytes, pacotes P de 64 bytes) e WeAct (LE16 + 0x0A) — `d21f08e`, `1392321`
- T-2.6: transporte USB bulk sobre nusb e protocolo Turing USB 0x1CBE (DES-CBC, PNG ≤ 1 MiB senão JPEG) —
  `8d540c0`, `a390f70`
- T-2.7: WCH 0x43A8 (DES-ECB, BGR888 em blocos de 512 bytes) — `4908926`
- T-2.8: connector roteia as 7 famílias; catálogo com 44 modelos (modelos seriais do TURZX) e regra udev
  gerada; fatos do app do fabricante consolidados nas specs; 8.8" marcada `hardware_validated`; README com
  telas suportadas e comandos; CHANGELOG — `669cc13`, `3ddd867`, `9959a0a`, `c08a52e`, `fd0032c`, `d74fb81`,
  `7755ea3`

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/domain/{frame,screen,pattern,error,catalog,device,geometry}.rs`,
  `crates/bezel-core/src/{ports,app}/**`, `crates/bezel-core/tests/screens.rs`
- `crates/bezel-devices/src/**` (wire, usb, busy, connector, discovery, fake, `protocol/*`, `driver/*`),
  `crates/bezel-devices/tests/udev_rules.rs`
- `crates/bezel-cli/src/{lib,main,screen}.rs`, `crates/bezel-cli/Cargo.toml`
- `packaging/linux/60-bezel.rules`, `docs/reverse-engineering/*.md`, `README.md`, `CHANGELOG.md`
- Fora da lista do PLAN: `apps/bezel-studio/src-tauri/src/dto.rs` e `apps/bezel-studio/src/demo-data.js`
  (teste/fixture de `hardwareValidated`, commit `d74fb81`)

## Tests
- Workspace: 324 passando, 0 falhas, 2 ignorados (`cargo test --workspace --locked`); `bezel-core` +
  `bezel-devices`: 175
- DoD auto: vetores das 7 famílias (1 teste nomeado cada), `busy::tests::finds_other_processes_holding_the_device`,
  `screen::tests::test_pattern_presents_frames_in_the_orientation` — todos `1 passed`
- Coverage: 94,48% de linhas no workspace (`cargo llvm-cov --workspace --summary-only`, `main.rs`/`build.rs`
  excluídos). Menores: `wire.rs` 63,4%, `usb.rs` 63,7%, `connector.rs` 69,2% (caminhos de I/O real);
  protocolos 97,9–100%, drivers 95,3–99,0%

## Hardware validation
Turing Smart Screen 8.8" real (`/dev/ttyACM1` 0525:a4a7 + MCU `/dev/ttyACM0` 1a86:ca88 `CT88INCH`),
ROM `chs_88inch.dev1_rom1.90`, com o `turing-smart-screen.service` do usuário parado durante os testes e
religado ao final:
- HELLO respondido; frame cheio confirmado (`full_png_sucess`, ~220 ms); parciais confirmados.
- `bezel test-pattern` nas quatro orientações sem erro; hoje `--orientation horizontal` (1920x480, 1017
  frames em 3 s) e `--orientation vertical` (480x1920, 879 frames em 3 s).
- `bezel show` com imagem 1920x480 (automático → horizontal), 480x1920 (→ vertical) e 480x1920 com
  `--orientation horizontal --fit contain`: aceitos pela tela.
- `bezel off`: o SoC sai do USB ~3 s depois (só o MCU fica); o comando retorna quando ele saiu. `show` logo
  em seguida acorda a tela em 17 s; com a tela dormindo há 5–20 s, em 11–12 s.
- Parar o serviço Python e rodar `show` imediatamente: funciona (antes da correção: "Broken pipe").
- A rotação de `horizontal` é a mesma do `landscape` do turing-smart-screen-python (90° horário a partir
  do buffer nativo REVERSE_PORTRAIT), que é como o usuário usa a tela hoje.
- **Pendente de confirmação humana (DoD manual):** que a legenda dos cantos aparece correta nas quatro
  orientações e a faixa anima — a verificação acima é de protocolo (respostas da tela), não visual.
