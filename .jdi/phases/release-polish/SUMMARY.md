# Phase 7: Polimento e release 1.0 — Summary  (slug: release-polish)

**Status:** partial
**Tasks:** 7/8 complete, 0 blocked (T-7.8 = passagem na 8.8" com o usuário e corte da 1.0.0)

> Execução: `/jdi-do` com o `jdi-doer-bezel` por tarefa, em worktrees `~/repos/bezel-wt/rp-*`, cherry-picked
> para a `main`. Hashes abaixo são os da `main`.

## Executed tasks
- T-7.1 — `41f647d`..`99e7c92`: um `language_of`, um mínimo de atualização, "o tema cabe no painel", "é tema
  nativo" e as opções de vídeo no core; `BezelError::ThemeFile`; código morto removido; TUR_USB com tamanho
  desconhecido = presente.
- T-7.2 — `fbea521`, `ef8e8da`: catálogo único de chaves (`keys::IMPORTED`), hwmon/amdgpu com a mesma
  conversão, ventoinhas, bomba, voltagens, totais de rede, memória disponível; `SensorOptions` +
  `SystemSensors::with_options`.
- T-7.3 — `997c41f`, `36b195a`, `91b9ea9`, `cb5ab6d`, `66c3001`: `connect_rev_c` testável (bus, portas e
  `Pause` injetados); escrita que a tela para de ler falha em 10 s (`drain_watching`); após cancelar, sem
  enchimento (D-2026-09-30-release-polish-10): HELLO + GET_FILE_SIZE → `Cancelled { partial }`; um envio que
  pega sobras falha na verificação de tamanho dizendo para apagar e reenviar.
- T-7.4 — `e941130`: painéis `1a86:ad10–ad13` em modo desktop listados como "not validated on hardware",
  consulta de modelo e volta ao modo monitor (vetores golden, `Confirm`); `bezel udev-rules`; regra udev com
  `hidraw`.
- T-7.5 — `9071262`: `gpu.fps` pelo RTSS (Windows, `unsafe` no menor escopo) e pelo MangoHud (Linux), só
  leitura, ausente ou velho = `Unavailable` com o jeito de ativar; `net.ping` em thread própria (ICMP sem
  privilégio, senão TCP); `--ping-host`, `--mangohud-dir`.
- T-7.6 — `01f6177`..`a9efb90`: avisos do importador e erros do studio como `{code, args}` traduzidos (fixture
  conferida pelo Rust); idioma do sistema ou escolhido em Preferências, trocado na hora (bandeja e diálogos
  nativos juntos); ping e pasta do MangoHud nas Preferências; `sizeMismatch` com Apagar; comando udev
  copiável na porta negada (nunca executado); painéis em modo desktop na aba Tela com diálogo de
  confirmação; "Sair" da bandeja pergunta por edições não salvas (W2 do studio-app); nenhum literal visível
  no JS (teste); Playwright em 4 projetos (claro/escuro × pt-BR/en).
- T-7.7 — `0fd7f0f`, `4762377`: deb/rpm com `/usr/bin/bezel`, `bezel-run@.service`, regra udev e temas
  (`check-packaging.sh`); guia do usuário com 14 páginas em en e pt-BR (`check-docs.sh`); README/CHANGELOG.
- Orquestrador: `b063681` (docs do studio após a T-7.6: modo desktop na aba Tela, Preferências → Idioma);
  `6bb5ae6` (imports de teste só-Unix do `bezel-media` quebravam o clippy do Windows desde a storage-video;
  CI do Windows voltou a compilar — conferido com `cargo clippy --target x86_64-pc-windows-msvc`).

## Blocked tasks
- nenhuma (T-7.8 aguarda o usuário na tela real)

## Files modified
- `crates/bezel-core/src/{domain,ports,app}/**`, `crates/bezel-core/tests/**`
- `crates/bezel-devices/src/{connector,wire,hid_desktop,udev,discovery,usb,fake,lib}.rs`,
  `crates/bezel-devices/src/{driver,protocol}/turing_rev_c.rs`, `crates/bezel-devices/tests/udev_rules.rs`
- `crates/bezel-sensors/**` (fps, ping, hwmon, amdgpu, LHM), `crates/bezel-themes/src/import/**`,
  `crates/bezel-media/src/{lib,transcode}.rs` (só testes)
- `crates/bezel-cli/src/**`, `crates/bezel-cli/tests/**`
- `apps/bezel-studio/src-tauri/{src/**,build.rs,capabilities/default.json,tauri.conf.json,tests/hardware.rs}`,
  `apps/bezel-studio/{src,tests}/**`, `apps/bezel-studio/playwright.config.mjs`
- `packaging/linux/{60-bezel.rules,bezel-run@.service,postinstall.sh}`, `scripts/install-local.sh`,
  `scripts/ci/{check-packaging,check-docs}.sh`
- `docs/user/**`, `docs/reverse-engineering/{protocol-turing-rev-c,protocol-turing-usb,sensors,devices}.md`,
  `README.md`, `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`

## Tests
- `cargo test --workspace --locked`: 651 passando, 0 falhando, 7 ignorados; fmt e clippy `-D warnings` limpos
  (Linux; e `--target x86_64-pc-windows-msvc` nas crates fora do studio)
- UI: 86 unitários; Playwright 88 (22 cenários × claro/escuro × pt-BR/en, axe sem violações sérias/críticas)
- `check-packaging.sh` (repositório; deb/rpm locais na T-7.7) e `check-docs.sh`: passam
- DoD Auto: as 8 linhas do CONTEXT → OK
- Coverage (`cargo llvm-cov`): 94,85% de linhas no workspace; `connector.rs` 96,89%, `wire.rs` 83,60%,
  `driver/turing_rev_c.rs` 97,07%, `hid_desktop.rs` 85,99%, `udev.rs` 100%, `rtss.rs` 99,52%, `mangohud.rs`
  99,68%, `ping.rs` 94,93%, `backend.rs` 98,05%, `studio.rs` 97,26%

## Hardware validation
Turing 8.8" (ROM 1.90), serviço do usuário parado e religado a cada teste; só `bezel_test_*` na tela.
- O `connect_rev_c` testável (T-7.3) abriu a 8.8" nos envios de teste desta fase (build com `36b195a`).
- Enchimento após cancelar (proposta da T-7.3): na 8.8" o firmware parou de ler após ~30,8 MB no SD e o
  `tcdrain` travou o processo por mais de 10 min; sem recuperação sem religar o cabo. Evidência em
  protocol-turing-rev-c.md § 19; daí a D-10 (sem enchimento) e o `drain_watching` (falha em 10 s).
- A tela ficou travada depois disso e segue esperando o usuário religar o cabo (o serviço Python está parado
  em "Display reset").
- `bezel devices` com a build instalada (`0.1.0-dev.188+b063681`) lista MCU e SoC.
- **Pendente (T-7.8, humano):** todo `revc-cancel-leftovers` (envio grande no SD, cancelar sem enchimento e
  reenviar, apagar `sd/video/bezel_test_cancel.mp4`), os Manual das fases 1–7, pacotes do candidato (deb, rpm,
  AppImage, msi, nsis) e FPS num jogo real; HID em modo desktop fica "não validado" (sem painel).

## Observações
- `sizeMismatch` é reconhecido pelo texto do `Transport`; uma variante no core fica no backlog.
- Leituras formatadas no core usam ponto decimal também em pt-BR (`4.72 GHz`); rótulos que a máquina dá aos
  sensores (hwmon, discos, redes) seguem em inglês.
- A esteira segue em `71f8b07` (github-workflows#15 fora da `main`); Sonar dispensado.
