# Phase 7: Polimento e release 1.0 — Plan  (slug: release-polish)

## Goal
Empacotamento deb/rpm/AppImage/msi/nsis com regras udev, i18n pt-BR/en completo, documentação de usuário, FPS de jogos e release 1.0.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-release-polish-1..11; herdadas: D-1 (hexagonal), device-protocols-2/-5/-6, sensors-1/-4, foundation-2

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
- **Nota:** fora de `files_modified`, sinalizados: `ports/mod.rs` (só doc), `bezel-devices/src/fake.rs` (arquivo de tamanho desconhecido), testes de integração do core e da CLI que usavam a duplicação removida. Erros de tema viram `theme file: …`.

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
- **Nota:** `FakeSensors::samples_taken` segue público porque testes da CLI o usam ("usados" da D-9).

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
- **Nota:** fora de `files_modified`, permitido: `docs/reverse-engineering/sensors.md` § 8. O `unsafe` do RTSS inclui `GetTickCount` (mesmo módulo `cfg(windows)`, `allow` + `// SAFETY:`); `lib.rs` passa de `forbid` a `deny(unsafe_code)`.

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
- **Nota:** fora de `files_modified`, sinalizados: `build.rs`/`capabilities/default.json` (`allow-*` dos comandos novos), `tests/hardware.rs` (campos novos do `Backend`), `bezel-themes/tests/import_corpus.rs` (ignorado). `sizeMismatch` é reconhecido pelo texto do `Transport` (variante no core fica no backlog).

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
- **Nota:** fora de `files_modified`, permitido: `packaging/linux/postinstall.sh` (gatilho `hidraw`). Os deb/rpm também levam `/usr/share/bezel/themes`; provado com deb e rpm construídos localmente.

### Wave 3b (achados da T-7.8 e pedido do usuário)

#### T-7.9: Vídeo de fundo no studio
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/**`, porta de pôster no core + `bezel-media`, `docs/user/**`, `CHANGELOG.md`
- **Acceptance:** "Adicionar vídeo…" e soltar um vídeo na tela copiam o vídeo para o tema com pôster (ffmpeg); "Usar como fundo" = `Background::Video` com desfazer; inspetor com vídeo, pôster e o fluxo existente de "Enviar para a tela"; demo e e2e nos 4 projetos.
- **Dependencies:** T-7.6
- **Status:** in progress

#### T-7.10: Limite de 25 MiB (D-12) e reinício pelo MCU (D-13)
- **Specialist:** jdi-doer-bezel
- **Files modified:** core (perfil, pré-voo, porta de conexão), `bezel-media`, `connector.rs`, CLI (`bezel restart`), studio (aba Tela), docs, `CHANGELOG.md`
- **Acceptance:** envio rev C > 25 MiB recusado antes de enviar e conversão com bitrate limitado; `00 00 00 00 00 c9` no MCU por 8 s e espera o SoC voltar; automático uma vez quando o SoC não responde; `bezel restart` e ação no studio.
- **Dependencies:** T-7.3
- **Test:** `domain::storage::tests::each_file_is_capped_at_the_profiles_limit`, `transcode::tests::the_bitrate_is_capped_so_the_output_fits_the_screen`, `a_converted_video_over_the_limit_is_refused_before_a_byte_is_sent` (core), `connector::tests::the_mcu_restart_sends_six_bytes_holds_8_s_and_waits_for_the_soc`, `connector::tests::a_soc_on_the_bus_without_hello_is_restarted_once_then_connected`, `connector::tests::a_screen_that_answers_is_never_restarted`, e2e `a hung screen offers the restart…`
- **Status:** completed (`d207730` D-12, `b6a2f7b` D-13; validação no hardware com o orquestrador)
- **Nota:** fora do escopo natural, sinalizados: `wire.rs` (`Stalled` tipado → `BezelError::Hung`, e a porta serial descarta ao fechar a saída que um dispositivo travado não leu, para o fechamento não esperar até 30 s), `driver/mod.rs` (`io_err`), `README.md` (`bezel restart` no início rápido), `demo-data.js` (`chuva.mp4` de 64 MiB → 22 MiB, coerente com o limite; cenário `noffmpeg` com 32 MB internos para o `noSpace` continuar testável).

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
- 8 tasks em 4 waves (4 → 2 → 1 → 1)

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| udev = catálogo = `bezel udev-rules`; HID + Confirm | T-7.4 |
| FPS por fixture; `net.ping` + chaves importadas | T-7.5 (chaves: T-7.2) |
| i18n + mensagens do backend; studio 2 temas × 2 idiomas | T-7.6 |
| `check-packaging.sh`, `check-docs.sh` | T-7.7 |
| Manuais (8.8", fases 1–6, pacotes, FPS real) | T-7.8 |

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --check`; clippy `-D warnings`; `npm test`; `scripts/ci/check-{packaging,docs}.sh`; cobertura ≥ 80% (`cargo llvm-cov`)

## Notes
- Worktree por task + cherry-pick; um dono por arquivo compartilhado na wave.
- Ritmo e cancelamento do rev C e o HID só valem como validados na T-7.8 (D-8).
