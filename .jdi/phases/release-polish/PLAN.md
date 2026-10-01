# Phase 7: Polimento e release 1.0 — Plan  (slug: release-polish)

## Goal
Empacotamento deb/rpm/AppImage/msi/nsis com regras udev, i18n pt-BR/en completo, documentação de usuário, FPS de jogos e release 1.0.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-release-polish-1..13; herdadas: D-1 (hexagonal), device-protocols-2/-5/-6, sensors-1/-4, foundation-2

## Tasks

### Wave 1 (paralela)

#### T-7.1: Refatorações do core, CLI e studio (D-9) e tamanho desconhecido no TUR_USB
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/{domain,app}/**`, `crates/bezel-cli/src/**`, `apps/bezel-studio/src-tauri/src/**`, `crates/bezel-themes/src/**`
- **Acceptance:**
  - Duplicações da D-9 numa função só (idioma, mínimo de 0,25 s, "cabe no painel", "é tema nativo", opções de vídeo no core); `BezelError::ThemeFile`; código morto fora.
  - TUR_USB: tamanho desconhecido = presente (`size = None`); sobrescrever exige `Confirm::Yes`.
- **Dependencies:** none
- **Test:** `app::storage::tests::unknown_size_counts_as_present`, `domain::media::tests::fitting_options_turn_and_cover_crop`
- **Status:** completed (`41f647d`..`99e7c92`, 8 commits)

#### T-7.2: Sensores: chaves únicas, hwmon/amdgpu unificados, chaves importadas
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/sensor.rs`, `crates/bezel-sensors/src/**`, `crates/bezel-themes/src/import/**`
- **Acceptance:**
  - `keys` do core com as chaves multiplataforma e as do todo `importer-sensor-keys` em `keys::IMPORTED`; nenhum literal de chave em `linux/`, `windows/sys.rs`, `fake.rs` e importadores; um só motivo "counter reset".
  - hwmon e amdgpu dividem conversão de unidade e potência (`power1_average`, senão `power1_input`); ventoinhas, bomba, voltagens, totais de rede e memória disponível % via hwmon/sysfs e LHM; sem fonte = `Unavailable` com motivo.
  - `SensorOptions { ping_host (8.8.8.8), mangohud_dir }` + `SystemSensors::with_options` (`new()` = padrão); `with_roots`/`samples_taken` fora da API pública; importadores: `system.volume` → `Unavailable("not supported yet")`, `gpu.fps`/`net.ping` sem nota de "não medido".
- **Dependencies:** none
- **Test:** `linux::hwmon::tests::gpu_power_prefers_the_average_like_amdgpu`, `linux::hwmon::tests::fans_and_voltages_use_catalog_keys`
- **Status:** completed (`fbea521`; reexport de `SensorOptions` em `ef8e8da`)

#### T-7.3: Rev C: connect testável e sobras do cancelamento
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-devices/src/connector.rs`, `crates/bezel-devices/src/{driver,protocol}/turing_rev_c.rs`
- **Acceptance:**
  - `connect_rev_c`/`wait_until_gone`/`wake_rev_c` com `DeviceBus`, abridor serial e `Pause` injetados; `SystemConnector` público e chamadores inalterados; `connector.rs` ≥ 80% de linhas.
  - Cancelamento: a proposta de enchimento foi substituída pela D-2026-09-30-release-polish-10 (sem enchimento; HELLO e GET_FILE_SIZE → `Cancelled { partial }` ou Timeout de reconexão; o tamanho verificado pega sobras). Nenhum comando destrutivo novo.
- **Dependencies:** none
- **Test:** `connector::tests::rev_c_wakes_and_retries_with_injected_bus`, `driver::turing_rev_c::tests::a_cancel_sends_no_filler_and_the_next_upload_fails_its_size_check`
- **Status:** completed (`997c41f`, `36b195a` connector testável e enchimento; `91b9ea9` escrita que para falha em 10 s; `cb5ab6d`/`66c3001` sem enchimento após cancelar, D-2026-09-30-release-polish-10)

#### T-7.4: HID desktop mode e `bezel udev-rules`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/{domain,ports,app}/**`, `crates/bezel-devices/{src,tests}/**`, `packaging/linux/60-bezel.rules`, `crates/bezel-cli/{src,tests}/**`, `Cargo.toml`/`Cargo.lock` (`hidapi` 2.6)
- **Acceptance:**
  - `1a86:ad10–ad13` listados como "desktop mode (not validated on hardware)"; consulta de modelo e os dois relatórios `5f3759df` do § 10 (64 B, report id 0) iguais a vetores golden; nada implícito.
  - Volta ao modo monitor com `Confirm`: `Confirm::No` = zero chamadas à porta; `bezel monitor-mode` sem `--yes` sai ≠ 0 sem enviar nada.
  - Regra udev com as linhas do HID; `bezel udev-rules` imprime a regra (stdout) e o comando sudo de uma linha (stderr), sem executá-lo; ambos vêm de `bezel_devices::udev` (o studio reusa); a dica de acesso negado aponta para ele.
- **Dependencies:** none
- **Test:** `udev_rules::tests::printed_rule_matches_packaged_file_and_catalog`, `hid_desktop::` (devices), `hid_desktop_requires_confirm` (CLI)
- **Status:** completed (`e941130`)

### Wave 2 (paralela)

#### T-7.5: FPS de jogos e `net.ping`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-sensors/**`, `Cargo.lock`, `crates/bezel-cli/src/{lib,main}.rs`, `crates/bezel-cli/tests/sensors.rs`
- **Acceptance:**
  - `gpu.fps` só leitura: parser puro do RTSS testado por fixture; mapeamento só em `cfg(windows)` com `unsafe` no menor escopo e `// SAFETY:`. Linux: CSV mais novo do MangoHud em `mangohud_dir`. `hardware_validated = false`.
  - Fonte ausente ou leitura com mais de 3 s = `Unavailable` dizendo como ativar; nunca 0 nem último valor; 0 fps de fonte viva é 0.
  - `net.ping` em thread própria: ICMP sem privilégio, senão TCP; timeout = `Unavailable`; `sample()` < 50 ms com alvo mudo; catálogo ⊇ `keys::IMPORTED`. CLI: `--ping-host`, `--mangohud-dir`.
- **Dependencies:** T-7.2, T-7.4
- **Test:** `cargo test -p bezel-sensors fps::`, `-- ping:: imported_keys_are_published`; clippy `--target x86_64-pc-windows-gnu`
- **Status:** completed (`9071262`; FPS com jogo real fica para a T-7.8)

#### T-7.6: Studio: i18n completo, mensagens por código e telas novas
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/src/**`, `apps/bezel-studio/{src,tests}/**` (inclui `tests/ui/fixtures/backend-codes.json`), `apps/bezel-studio/playwright.config.mjs`, `crates/bezel-themes/src/import/**`, `crates/bezel-cli/src/theme.rs`
- **Acceptance:**
  - Erros de dispositivo/armazenamento e avisos do importador como `{code, args}` (`ImportWarning` com `Display` em inglês para a CLI); `backend-codes.json` conferido por teste Rust; `backend-messages.test.mjs` exige en/pt-BR com os mesmos `{params}`.
  - Nenhum literal visível em `src/ui/**` e `app.js` (teste em `i18n.test.mjs`); idioma do sistema com override salvo em `Settings`.
  - Porta negada mostra o comando udev copiável; modo desktop "não validado" com `Confirm`; ping e MangoHud nas configurações.
  - Modo demo emula tudo; Playwright claro/escuro × pt-BR/en, axe sem violações sérias/críticas.
- **Dependencies:** T-7.1, T-7.2, T-7.4
- **Test:** `node --test tests/ui/i18n.test.mjs tests/ui/backend-messages.test.mjs`; `npm test`
- **Status:** completed (`01f6177`..`a9efb90`, 12 commits)

### Wave 3

#### T-7.7: Empacotamento e documentação de usuário
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/tauri.conf.json`, `packaging/linux/bezel-run@.service`, `scripts/install-local.sh`, `scripts/ci/{check-packaging,check-docs}.sh`, `docs/user/**`, `README.md`, `CHANGELOG.md`
- **Acceptance:**
  - deb/rpm levam `/usr/bin/bezel`, `bezel-run@.service`, regra udev e postinstall; `install-local.sh` troca o ExecStart para `~/.local/bin`; `check-packaging.sh` confere o repositório e, com deb/rpm, o conteúdo.
  - `docs/user/` em en e pt-BR com os tópicos da D-7; `check-docs.sh`: mesmos arquivos, links válidos, sem segredos; CHANGELOG da fase.
- **Dependencies:** T-7.1..T-7.6
- **Test:** `bash scripts/ci/check-packaging.sh && bash scripts/ci/check-docs.sh`
- **Status:** completed (`0fd7f0f` empacotamento, `4762377` docs; `b063681` docs do studio após a T-7.6)

### Wave 3b (achados da T-7.8 e pedido do usuário)

#### T-7.9: Vídeo de fundo no studio
- **Specialist:** jdi-doer-bezel
- **Files modified:** studio, porta de pôster (core + `bezel-media`), docs
- **Acceptance:** adicionar ou soltar vídeo/GIF = `Background::Video` com pôster e desfazer; inspetor com o fluxo de "Enviar para a tela".
- **Dependencies:** T-7.6
- **Test:** `tests/ui/video-background.test.mjs`, e2e de vídeo/GIF de fundo, pôster no `bezel-media`
- **Status:** completed (`7d9dc5f`..`aa746c0`); validado na 8.8" (SUMMARY § Hardware)

#### T-7.10: Limite de 25 MiB (D-12) e reinício pelo MCU (D-13)
- **Specialist:** jdi-doer-bezel
- **Files modified:** core, `bezel-media`, `connector.rs`, `wire.rs`, CLI, studio, docs
- **Acceptance:** envio rev C > 25 MiB recusado antes de enviar; conversão com bitrate limitado; `00 00 00 00 00 c9` no MCU por 8 s e espera o SoC voltar; automático uma vez quando o SoC não responde.
- **Dependencies:** T-7.3
- **Test:** `each_file_is_capped_at_the_profiles_limit`, `the_bitrate_is_capped_so_the_output_fits_the_screen`, testes do `RevCHost` (bytes, 8 s, volta, uma vez)
- **Status:** completed (`777c0d7`, `2829fa8`); validado na 8.8" (SUMMARY § Hardware)

#### T-7.11: GIF com cadência própria e ao vivo que se recupera
- **Specialist:** jdi-doer-bezel
- **Files modified:** core (runtime, animação, reconexão), CLI `live.rs`, studio, docs
- **Acceptance:** GIF visível no próprio ritmo (≤ 30 fps, só o retângulo, sem atraso), sensores no `refreshSeconds`; ao vivo reconecta com recuo limitado.
- **Dependencies:** T-7.10
- **Test:** `tests/runtime_animation.rs`, `tests/animation.rs` (CLI), reconexão na CLI e no studio
- **Status:** completed (`afa6e9c`, `79920ad`); validado na 8.8" (SUMMARY § Hardware)

#### T-7.12: Aba Temas com miniaturas e filtro
- **Specialist:** jdi-doer-bezel
- **Files modified:** studio (`thumbnails.rs`, biblioteca, ajustes, UI), docs
- **Acceptance:** miniatura real de cada tema (renderizador, sensores de demonstração, cache em disco, refeita ao salvar); filtro "Para esta tela"/"Todos" e orientação, lembrado; cartão diz a tela do tema.
- **Dependencies:** T-7.6
- **Test:** `thumbnails::tests::*`, `tests/ui/theme-filter.test.mjs`, `tests/e2e/themes.spec.mjs`
- **Status:** completed (`1b4426a`, `5a966cd`)

### Wave 4

#### T-7.8: HARDWARE — 8.8" real e corte da 1.0.0 (orquestrador)
- **Specialist:** orquestrador na Turing 8.8" real (horizontal)
- **Files modified:** `.jdi/phases/*/SUMMARY.md` (§ Hardware validation), `.github/workflows/ci.yml` (se repinar), `CHANGELOG.md`
- **Acceptance:**
  - `install-local.sh`, serviço do usuário parado e religado. `/jdi-confirm-dod` humano: `bezel devices`; padrão nas 4 orientações; sensores × `sensors`/`nvidia-smi`/`free`; `bezel run` 60 s; studio ao vivo, bandeja; aba Tela, vídeo com alfa, boot; todo `revc-cancel-leftovers` (envio grande no SD, cancelar e reenviar, apagar `bezel_test_cancel.mp4`).
  - Candidato: pacotes do último run da main com `check-packaging.sh`; instalar deb/rpm/AppImage e msi/nsis, abrir o studio, FPS num jogo real.
  - `ci.yml` → `@main` só se github-workflows#15 está na main; senão fica em 71f8b07.
  - Corte só após as confirmações: `## [1.0.0] - data`, commit `feat(release-polish)!:` com `BREAKING CHANGE:`, antes em PR para ver `versao` = 1.0.0; tag `v1.0.0` com os 5 pacotes. Outra versão = parar e abrir decisão, sem tocar `Cargo.toml`.
- **Dependencies:** T-7.7
- **Test:** DoD manual (SUMMARY das fases + release v1.0.0)
- **Status:** pending

## Execution
- 12 tasks em 5 waves (4 → 2 → 1 → 4 → 1)

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| udev = catálogo = `bezel udev-rules`; HID + Confirm | T-7.4 |
| FPS; `net.ping` + chaves | T-7.5, T-7.2 |
| i18n, mensagens; studio 2 temas × 2 idiomas | T-7.6 |
| `check-packaging.sh`, `check-docs.sh` | T-7.7 |
| Manuais (8.8", fases 1–6, pacotes, FPS real) | T-7.8 |

## Test requirements
- `cargo test`, fmt, clippy (Linux e Windows), `npm test`, `check-{packaging,docs}.sh`, cobertura ≥ 80%

## Notes
- Arquivos fora de `files_modified` das T-7.1..T-7.7: SUMMARY § Files modified e as notas dos commits.
- Worktree por task + cherry-pick; um dono por arquivo compartilhado na wave.
- Ritmo e cancelamento do rev C e o HID só valem como validados na T-7.8 (D-8).
