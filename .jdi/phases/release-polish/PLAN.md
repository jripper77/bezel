# Phase 7: Polimento e release 1.0 — Plan  (slug: release-polish)

## Goal
Empacotamento deb/rpm/AppImage/msi/nsis com regras udev, i18n pt-BR/en completo, documentação de usuário, FPS de jogos e release 1.0.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-release-polish-1..9; herdadas: D-1 (hexagonal), device-protocols-2/-5/-6, sensors-1/-4, foundation-2

## Tasks

### Wave 1 (paralela)

#### T-7.1: Refatorações do core, CLI e studio (D-9) e tamanho desconhecido no TUR_USB
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{clock,theme,error,media}.rs`, `crates/bezel-core/src/app/{storage,runtime}.rs`, `crates/bezel-cli/src/{clock,live,sensors,theme,storage}.rs`, `apps/bezel-studio/src-tauri/src/{clock,backend,studio,library,storage}.rs`, `apps/bezel-studio/src-tauri/src/storage/tests.rs`, `crates/bezel-themes/src/{lib,native}.rs`, `crates/bezel-themes/src/import/mod.rs`
- **Acceptance:**
  - Um só `language_of` (core `domain::clock`), um só mínimo de 0,25 s, uma função do core "o tema cabe neste painel", um só "é tema nativo" (`bezel-themes`), e `fitting_options`/`convert_options` viram uma função do core ao lado de `cover_crop`.
  - Variante de `BezelError` para arquivo de tema (sai `Transport`/`ScreenNotFound`); `Theme::sensor_keys`/`element_mut`/`next_id` e `BoxF::translated` removidos ou usados fora de teste.
  - TUR_USB: `Unsupported` de `size` = presente com `FileEntry.size = None` em list, play e no vídeo do runtime; sobrescrever segue exigindo `Confirm::Yes`. Testes existentes passam sem mudar asserções.
- **Dependencies:** none
- **Test:** `app::storage::tests::unknown_size_counts_as_present`, `domain::media::tests::fitting_options_turn_and_cover_crop`
- **Status:** completed (`45598d6`..`0480bfa`, 8 commits)
- **Nota:** fora de `files_modified`, sinalizados: `crates/bezel-core/src/ports/mod.rs` (só doc: `ScreenStorage::size` documenta `Unsupported` = presente com tamanho desconhecido), `crates/bezel-devices/src/fake.rs` (`FakeStorage::size_unknown`/`with_file_of_unknown_size`, porque o core não implementa porta nem em teste), `crates/bezel-core/tests/{storage,runtime_video}.rs` e `crates/bezel-cli/tests/bundled_themes.rs` (usava a regra duplicada de "cabe no painel" e o `sensor_keys` removido). Asserções mudadas só onde o teste exercitava a duplicação ou o código morto: teste do locale movido para o core, testes do backend do studio comparam com `MIN_REFRESH_SECONDS`, testes do core deixam de chamar `sensor_keys`/`next_id`/`element_mut`/`translated`. Mudanças visíveis: erros de tema passam a `theme file: …` (antes `transport error:`/`screen not found:`); o studio abre um `theme.json` como a CLI (antes falhava). `MAX_REFRESH` segue por adaptador (CLI 60 s, studio 2 s). Sobras: `0.25` literal no JS (`app.js`, `ui/inspector.js`, arquivos da T-7.6); `clock::now()` segue igual na CLI e no studio (chrono fica nos adaptadores, sem crate comum); `Theme::element` só é usado em teste (fora da lista da D-9).

#### T-7.2: Sensores: chaves únicas, hwmon/amdgpu unificados, chaves importadas
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/sensor.rs`, `crates/bezel-sensors/src/{provider,system,fake,amdgpu,lhm,gpu}.rs`, `crates/bezel-sensors/src/linux/{cpu,memory,system,net,disk,hwmon}.rs`, `crates/bezel-sensors/src/windows/sys.rs`, `crates/bezel-themes/src/import/{turzx,python_yaml}.rs` (+ `tests.rs` de cada)
- **Acceptance:**
  - `keys` do core com as chaves multiplataforma e as do todo `importer-sensor-keys` em `keys::IMPORTED`; nenhum literal de chave em `linux/`, `windows/sys.rs`, `fake.rs` e importadores; um só motivo "counter reset".
  - hwmon e amdgpu dividem conversão de unidade e potência (`power1_average`, senão `power1_input`); ventoinhas, bomba, voltagens, totais de rede e memória disponível % via hwmon/sysfs e LHM; sem fonte = `Unavailable` com motivo.
  - `SensorOptions { ping_host (8.8.8.8), mangohud_dir }` + `SystemSensors::with_options` (`new()` = padrão); `with_roots`/`samples_taken` fora da API pública; importadores: `system.volume` → `Unavailable("not supported yet")`, `gpu.fps`/`net.ping` sem nota de "não medido".
- **Dependencies:** none
- **Test:** `linux::hwmon::tests::gpu_power_prefers_the_average_like_amdgpu`, `linux::hwmon::tests::fans_and_voltages_use_catalog_keys`
- **Status:** completed (`784b106`)
- **Nota:** `SensorOptions` é `pub` em `system.rs`, mas o reexport (`pub use system::{SensorOptions, SystemSensors};`) fica em `crates/bezel-sensors/src/lib.rs`, fora dos arquivos desta tarefa; falta essa linha antes de a CLI (T-7.5) e o studio (T-7.6) nomearem o tipo. `FakeSensors::samples_taken` segue público porque três testes da CLI o usam (`live.rs`, `theme.rs`, `tests/runtime.rs`), o que cumpre o "removidos ou usados" da D-9.

#### T-7.3: Rev C: connect testável e sobras do cancelamento
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-devices/src/connector.rs`, `crates/bezel-devices/src/{driver,protocol}/turing_rev_c.rs`
- **Acceptance:**
  - `connect_rev_c`/`wait_until_gone`/`wake_rev_c` com `DeviceBus`, abridor serial e `Pause` injetados; `SystemConnector` público e chamadores inalterados; `connector.rs` ≥ 80% de linhas.
  - Cancelamento com recuperação estourada: antes do HELLO completa o tamanho declarado com enchimento (249+1), mede com GET_FILE_SIZE e devolve `Cancelled { partial }`; o envio seguinte não recebe bytes soltos (fio roteirizado); nenhum comando destrutivo novo.
- **Dependencies:** none
- **Test:** `connector::tests::rev_c_wakes_and_retries_with_injected_bus`, `driver::turing_rev_c::tests::timed_out_cancel_pads_the_declared_length`
- **Status:** completed (connector testável; sem enchimento após cancelar: D-2026-09-30-release-polish-10)

#### T-7.4: HID desktop mode e `bezel udev-rules`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{catalog,device,discovery,mod}.rs`, `crates/bezel-core/src/ports/mod.rs`, `crates/bezel-core/src/app/{mod,screens}.rs`, `crates/bezel-devices/src/{hid_desktop,udev,discovery,lib,fake,usb}.rs`, `crates/bezel-devices/tests/udev_rules.rs`, `packaging/linux/60-bezel.rules`, `crates/bezel-cli/src/{udev_rules,devices,lib,main}.rs`, `crates/bezel-cli/tests/devices.rs`; `Cargo.toml`/`Cargo.lock` só com `hidapi` 2.6
- **Acceptance:**
  - `1a86:ad10–ad13` listados como "desktop mode (not validated on hardware)"; consulta de modelo e os dois relatórios `5f3759df` do § 10 (64 B, report id 0) iguais a vetores golden; nada implícito.
  - Volta ao modo monitor com `Confirm`: `Confirm::No` = zero chamadas à porta; `bezel monitor-mode` sem `--yes` sai ≠ 0 sem enviar nada.
  - Regra udev com as linhas do HID; `bezel udev-rules` imprime a regra (stdout) e o comando sudo de uma linha (stderr), sem executá-lo; ambos vêm de `bezel_devices::udev` (o studio reusa); a dica de acesso negado aponta para ele.
- **Dependencies:** none
- **Test:** `udev_rules::tests::printed_rule_matches_packaged_file_and_catalog`, `hid_desktop::` (devices), `hid_desktop_requires_confirm` (CLI)
- **Status:** completed

### Wave 2 (paralela)

#### T-7.5: FPS de jogos e `net.ping`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-sensors/src/{fps/mod,fps/rtss,fps/mangohud,ping,system,lib}.rs`, `crates/bezel-sensors/src/fps/fixtures/**`, `crates/bezel-sensors/Cargo.toml`, `Cargo.lock`, `crates/bezel-cli/src/{lib,main}.rs`, `crates/bezel-cli/tests/sensors.rs`
- **Acceptance:**
  - `gpu.fps` só leitura: parser do RTSS (`RTSSSharedMemoryV2`, assinatura `RTSS`, entrada ativa mais nova) puro, testado no Linux com fixtures; o mapeamento (`windows-sys`) só em `cfg(windows)`, com `#[allow(unsafe_code, reason = "…")]` no menor escopo e `// SAFETY:`, nenhum outro `unsafe`. Linux: CSV mais novo do MangoHud em `mangohud_dir` (padrão: `output_folder` do `MangoHud.conf`). `hardware_validated = false`.
  - Fonte ausente ou leitura com mais de 3 s = `Unavailable` dizendo como ativar; nunca 0 nem último valor; 0 fps de fonte viva é 0.
  - `net.ping` em thread própria: ICMP datagrama sem privilégio (`ping_group_range`, `socket2`), senão TCP 53/443; timeout = `Unavailable`; `sample()` < 50 ms com alvo mudo; `imported_keys_are_published` (catálogo de fixture ⊇ `keys::IMPORTED`). CLI: `--ping-host`, `--mangohud-dir`.
- **Dependencies:** T-7.2, T-7.4
- **Test:** `cargo test -p bezel-sensors fps::`, `-- ping:: imported_keys_are_published`; clippy `--target x86_64-pc-windows-gnu`
- **Status:** pending

#### T-7.6: Studio: i18n completo, mensagens por código e telas novas
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/src/**`, `apps/bezel-studio/{src,tests}/**` (inclui `tests/ui/fixtures/backend-codes.json`), `apps/bezel-studio/playwright.config.mjs`, `crates/bezel-themes/src/import/**`, `crates/bezel-cli/src/theme.rs`
- **Acceptance:**
  - Erros de dispositivo/armazenamento e avisos do importador como `{code, args}` (`ImportWarning` com `Display` em inglês para a CLI); `backend-codes.json` conferido por teste Rust; `backend-messages.test.mjs` exige en/pt-BR com os mesmos `{params}`.
  - Nenhum literal visível em `src/ui/**` e `app.js` (teste em `i18n.test.mjs`); idioma do sistema com override salvo em `Settings`.
  - Porta negada (Linux) mostra o comando de `bezel_devices::udev`, copiável, sem elevar; "desktop mode" rotulado "não validado no hardware", volta atrás de diálogo `Confirm`; host do ping e pasta do MangoHud nas configurações.
  - Modo demo emula tudo; Playwright claro/escuro × pt-BR/en, axe sem violações sérias/críticas.
- **Dependencies:** T-7.1, T-7.2, T-7.4
- **Test:** `node --test tests/ui/i18n.test.mjs tests/ui/backend-messages.test.mjs`; `npm test`
- **Status:** pending

### Wave 3

#### T-7.7: Empacotamento e documentação de usuário
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/tauri.conf.json`, `packaging/linux/bezel-run@.service`, `scripts/install-local.sh`, `scripts/ci/{check-packaging,check-docs}.sh`, `docs/user/**`, `README.md`, `CHANGELOG.md`
- **Acceptance:**
  - deb/rpm levam `/usr/bin/bezel` (`target/release/bezel`, compilado pelo `binarios_extra` antes do bundle), `/usr/lib/systemd/user/bezel-run@.service` com ExecStart `/usr/bin/bezel`, regra udev e postinstall; `install-local.sh` troca o ExecStart para `~/.local/bin`.
  - `check-packaging.sh` confere `tauri.conf.json`, unit, regra e temas e, com deb/rpm construídos (ou passados), exige `usr/bin/bezel`, regra e unit (`dpkg-deb -c`, `rpm -qlp`).
  - `docs/user/` e `docs/user/pt-BR/`: tópicos da D-7, Windows (usbser, WinUSB/Zadig, LHM elevado), sem assinatura/SmartScreen, `bezel udev-rules`, FPS, HID "não validado"; `check-docs.sh`: mesmos arquivos nos dois idiomas, links relativos válidos, sem segredos/caminhos privados, README sem "early development"; CHANGELOG `## [Unreleased]` da fase.
- **Dependencies:** T-7.1..T-7.6
- **Test:** `bash scripts/ci/check-packaging.sh && bash scripts/ci/check-docs.sh`
- **Status:** pending

### Wave 4

#### T-7.8: HARDWARE — 8.8" real e corte da 1.0.0 (orquestrador)
- **Specialist:** orquestrador na Turing 8.8" real (horizontal)
- **Files modified:** `.jdi/phases/*/SUMMARY.md` (§ Hardware validation), `.github/workflows/ci.yml` (se repinar), `CHANGELOG.md`
- **Acceptance:**
  - `install-local.sh`, serviço `turing-smart-screen` parado e religado. `/jdi-confirm-dod` humano com evidência: `bezel devices` (MCU+SoC); padrão nas 4 orientações; sensores × `sensors`/`nvidia-smi`/`free`; `bezel run` 60 s; studio ao vivo e bandeja; aba Tela, vídeo com alfa, boot; cancelar envio sem sobra (espaço após reboot).
  - Candidato: pacotes do último run da main, `check-packaging.sh` nos deb/rpm; instalar deb/rpm/AppImage e msi/nsis, abrir o studio, FPS num jogo real.
  - Se slipalison/github-workflows#15 está na main, `ci.yml` → `@main`; senão fica em 71f8b07 (Sonar segue dispensado).
  - Corte, só após todas as confirmações: `## [Unreleased]` → `## [1.0.0] - data`, commit `feat(release-polish)!:` com `BREAKING CHANGE:` (schema 1 do `.bezeltheme` e CLI), antes em PR para ver o `versao` = 1.0.0; depois a tag `v1.0.0` com os 5 pacotes. `bin/versao.py` @71f8b07 não trata 0.x à parte (quebra vira 1.0.0); o `pipeline.yml` só tem `versao_inicial` (sem tag): outra versão = parar e abrir decisão, sem tocar `Cargo.toml`.
- **Dependencies:** T-7.7
- **Test:** DoD manual (SUMMARY das fases + release v1.0.0)
- **Status:** pending

## Execution
- Total tasks: 8
- Waves: 4 (4 → 2 → 1 → 1)
- Estimated parallel speedup: 2x

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| udev = catálogo = `bezel udev-rules`; HID + Confirm | T-7.4 |
| FPS por fixture; `net.ping` + chaves importadas | T-7.5 (chaves: T-7.2) |
| i18n + mensagens do backend; studio 2 temas × 2 idiomas | T-7.6 |
| `check-packaging.sh`, `check-docs.sh` | T-7.7 |
| Manuais (8.8", fases 1–6, pacotes, FPS real) | T-7.8 |

## Files modified (all tasks)
- `crates/**`, `apps/bezel-studio/**`, `Cargo.lock`, `packaging/linux/**`, `scripts/**`, `docs/user/**`, `README.md`, `CHANGELOG.md`, `ci.yml`

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --check`; clippy `-D warnings`; `npm test`; `scripts/ci/check-{packaging,docs}.sh`
- Coverage ≥ 80% de linhas (`cargo llvm-cov`); nenhum `TODO`/`FIXME` sem issue

## Notes
- Worktree por task + cherry-pick, um dono por wave: `Cargo.lock` (T-7.4 só com `hidapi`; T-7.5), `backend.rs` (T-7.1 → T-7.6), i18n (T-7.6), `README.md`/`CHANGELOG.md` (só T-7.7/T-7.8). Nenhuma variante de enum que force `match` em arquivo de outra task da wave; o studio mostra o modo desktop só na T-7.6.
- `install-local.sh` após cada task. Backlog da D-9 fica fora; enchimento do rev C e HID só valem como validados na T-7.8 (D-8).
