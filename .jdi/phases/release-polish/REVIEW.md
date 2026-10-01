# Phase 7: Review  (slug: release-polish)

**Verdict:** BLOCKED

> Verificação em modo `verify`. `HEAD` = `e00abb8`, árvore limpa, sem REVIEW.md anterior nesta fase.
>
> - Rodei todos os gates sobre a árvore inteira. Nenhum número veio do SUMMARY; onde o meu número difere, digo.
> - Escopo julgado: T-7.1..T-7.7 (commits `*(release-polish)*` de `7e5ce7a` a `e00abb8`), o DoD do CONTEXT e do
>   PROJECT e as decisões D-2026-09-30-release-polish-1..10. A T-7.8 (8.8" + corte da 1.0.0) está pendente por desenho.
> - Nenhum comando abriu a 8.8" real, que segue travada à espera do usuário. Não rodei nenhum `#[ignore]` nem
>   `BEZEL_HW_TESTS`. Os testes que tocam o sistema usam `--fake`, porta inexistente (`/dev/bezel-no-such-port`) ou só
>   enumeram HID pelo sysfs (`enumeration_is_read_only_and_finds_no_stray_devices`), o que equivale a `bezel devices`.
> - O único bloqueio vem do CI do Windows (B1). Todos os gates locais (Linux) passam.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1) |
| Tests | **FAIL (Windows CI)** | Linux: **651 passed, 0 failed, 7 ignored** (4 de ffmpeg real, 1 de timing, 1 de corpus, 1 de hardware do studio). Bate com o SUMMARY (651/7) e não caiu (578 no review da studio-app). Windows (CI de `e00abb8`, run 36791829033): `bezel` lib com 56 passed e **1 failed** (`live::tests::a_missing_video_shows_the_poster_and_how_to_send_it`, `live.rs:761`). Os outros 23 binários de teste nunca rodaram no Windows (B1) |
| Coverage | PASS | **94.90%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD, sem filtro: **94.84%**; o SUMMARY diz 94,85% (diferença de 0,01 pp entre execuções). Arquivos da fase: `connector.rs` 96.89%, `wire.rs` 83.60%, `driver/turing_rev_c.rs` 97.07%, `hid_desktop.rs` 85.99%, `udev.rs` 100%, `rtss.rs` 99.52%, `mangohud.rs` 99.68%, `ping.rs` 94.93%, `backend.rs` 98.05%, `studio.rs` 97.26%, `messages.rs` 98.48%, `udev_rules.rs` 98.82%. Cola Tauri: `commands.rs` 1.78%, `lib.rs` 25.57%, `tray.rs` 0.00% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado para `x86_64-pc-windows-msvc` (7 crates, `CARGO_TARGET_DIR=target/wincheck`): exit 0. Os 4 `#[allow(unsafe_code, reason = "…")]` de `fps/rtss.rs:247,269,302,323` trazem o motivo no próprio atributo (ver nota no gate 5) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 limpos (detalhe abaixo). `cargo audit --deny warnings`: exit 0 (1277 advisories, 604 crates) |
| Consistency | PASS (com WARN) | Todas as D-2026-09-30-release-polish-1..10 respeitadas, D-10 inclusive (sem enchimento após cancelar). Commits com escopo `release-polish`. Defeitos de documentação no PLAN, CONTEXT, SUMMARY e todos (W4) |
| UI Validation | PASS (com WARN) | `npm ci` ok. Comando do gate, 1ª execução: **87/88** (falhou `storage.spec.mjs:42` em `dark-pt`). `npm test` logo depois: **86/86** unitários (99,85% de linhas) e **88/88** e2e. O teste é instável por corrida de tempo (W1). Sem erro de console e sem axe sério ou crítico nas execuções que passaram |
| DoD | PASS (Linux) / PENDING MANUAL | As 8 linhas Auto do CONTEXT e as 3 do PROJECT passam como escritas. No Windows, o `cargo test --workspace` do PROJECT falha (B1). Faltam 2 Manual do CONTEXT e 2 do PROJECT |

### Achados do review anterior (studio-app)
| Achado | Situação | Evidência |
|---|---|---|
| W1 mensagens do backend em inglês | **Resolvido** | `messages.rs`: `ErrorCode` com código estável, argumentos nomeados e o inglês só como `message`. `From<BezelError>` dá um código por variante. `notInLibrary` e `notPicked` substituem as duas frases citadas. `backend-codes.json` é conferido pelo Rust e `backend-messages.test.mjs` exige en/pt-BR com os mesmos `{params}`. Resta que o `{detail}` de `transport`/`invalidInput`/`themeFile`/`refused`/`system` é texto livre do core, em inglês (W3) |
| W2 Sair pela bandeja perdia edições | **Resolvido** | `tray.rs:62-73`: com `Unsaved`, mostra a janela e emite `quit-requested`. `app.js:457` pergunta com `settleUnsaved` e chama `quit_app`. Regra testada em `lib.rs:423` (`quitting_asks_over_unsaved_edits_only`) e e2e "quitting from the tray over unsaved edits shows the window and asks" |
| W3 docs após a correção | Parcial | `CONTEXT.md:7` alinhado à D-2026-09-30-studio-app-7 (`40f64ae`), mas `CONTEXT.md:24` ainda põe "vídeo no host" fora do escopo. O CHANGELOG segue sem a decodificação no PC pelo studio (W4) |
| W4 duplicação entre adapters de entrada | Parcial | "O tema cabe no painel" virou `Theme::misfit` no core, usado por `live.rs:107` e `studio.rs:647`. A cadência com vídeo no PC continua em dois lugares (`studio.rs:616-621` e `live.rs:304-331`). Não está na lista da D-9, então fica como informativo |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada |
| 5.3 ports | PASS: nenhum port implementado no core. Traits públicas fora do core: `Pause`, `Clock`, `Wire`, `Pace` e `MediaSetup`, todas auxiliares de adapter e anteriores à fase. `FrameRateSource` e `Probe` (novas) são `pub(crate)` |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs`/testes |
| 5.5 `unsafe` | PASS. Só em `bezel-sensors/src/fps/rtss.rs:256,262,278,290,310,313,330`, dentro de `mod shared` sob `#[cfg(windows)]`, cada bloco com `// SAFETY:` logo acima. O crate passou de `forbid` a `deny(unsafe_code)` (`lib.rs:27`); core, CLI e studio seguem com `forbid`. O parser do RTSS é código seguro sobre uma cópia, testado no Linux com fixtures |
| 5.6 panics | PASS. Fora de `#[cfg(test)]` só aparecem `bezel-sensors/src/testing.rs` e `bezel-render/src/{golden,testkit}.rs`, declarados sob `#[cfg(test)]`, e `fps/mod.rs:121,134` (`hex_fixture`, `#[cfg(test)]`) |
| 5.7 escrita no dispositivo | PASS. Fora de teste, `Confirm::Yes` só aparece em doc e nos `match` de `Confirmed::require`/`MonitorModeConfirmed::require` (`discovery.rs:310`). As demais ocorrências estão depois do `#[cfg(test)]` de cada arquivo (`turing_usb.rs:508`, `turing_rev_c.rs:629`, `hid_desktop.rs:262`, `fake.rs:589`, `backend.rs:686`). Nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS: todo encoder tem teste. Conferi `MODEL_QUERY = aa 55 33` e `MONITOR_MODE_MAGIC = "5f3759df"` (`hid_desktop.rs:41,44`), relatório de 64 B com report id 0, contra protocol-turing-usb.md § 10. Conferi também `GET_FILE_SIZE`/`UPLOAD_FILE`/`DELETE_FILE` contra a tabela de protocol-turing-rev-c.md. O `2c` de enchimento saiu: `git diff 7e5ce7a HEAD -- crates/bezel-devices/src/protocol/` é vazio, e `recover_after_cancel` (`turing_rev_c.rs:383-403`) só manda HELLO e GET_FILE_SIZE (D-10) |
| 5.9 caminhos no core | PASS: só doc (`device.rs:75`, `discovery.rs:16`, `sensor.rs:7,216,259,266`) e testes |
| 5.10 comandos síncronos | PASS. Os novos síncronos são `quit_app`, `preferences` e `set_language`, e nenhum toca tela, sensor ou renderer. `set_sensor_options` é `async` com `blocking` |
| 5.11 supply chain | PASS: `cargo audit` 0, também com `--deny warnings`. Dependências novas: `hidapi` 2.6.7 (backends nativos em Rust), `socket2` 0.6.5, `windows-sys` 0.61.2. Nenhum segredo |

**Nota sobre o gate 4:** a regra pede um `// reason:` acima de cada `allow`. Os quatro `allow` da fase usam `#[allow(unsafe_code, reason = "…")]`, que o compilador guarda junto do lint, e o CONTEXT (§ Notes) pede exatamente isso. Considerei cumprido. Vale atualizar o texto da regra do reviewer para aceitar `reason =`.

## Blockers
- **B1. Os testes falham no Windows, e `e00abb8` corrigiu só metade do teste que dizia corrigir (gate 2; DoD do PROJECT "`cargo test --workspace` exits 0" no Windows).**
  - **Evidência:** CI de `e00abb8` (run 36791829033), job `qualidade / rust-windows`: fmt e clippy passam, e `cargo test com cobertura` falha em `live::tests::a_missing_video_shows_the_poster_and_how_to_send_it` (`crates/bezel-cli/src/live.rs:761`).
    - A primeira asserção (`live.rs:730-736`) agora passa pelo `quoted()`.
    - A segunda (`live.rs:762`) ainda espera `zipped.display()` sem aspas: `format!("  bezel import {} -o <FOLDER>", zipped.display())`.
    - O programa imprime `bezel import 'C:\Users\RUNNER~1\...\bezel-live-4644-missing.bezeltheme'`, porque `~` não está entre os caracteres livres de `quoted` (`live.rs:207-219`).
    - A falha é determinística. No run anterior (36790494666) o mesmo teste falhava na linha 726.
  - **Consequência:** o `cargo test` para no primeiro binário que falha. Dos 24 binários de teste, só `bezel_cli` (lib) rodou no Windows até hoje. Os outros 23 (core, devices, media, render, sensors, studio, themes, a CLI e os testes de integração) nunca rodaram lá.
    - O `Portao` da esteira falha, e os passos de build e empacotamento do Windows ficam pulados.
    - Sem esses passos, a `main` não gera msi/nsis. Isso impede o DoD Manual "instalar … msi/nsis do release candidato" e o corte da D-2026-09-30-release-polish-2.
  - **Correção:** trocar `zipped.display()` por `quoted(&zipped.to_string_lossy())` em `live.rs:762`, como na primeira asserção. Depois, esperar o `rust-windows` verde com os 24 binários rodados. É provável aparecerem outras falhas só do Windows, porque essas suítes nunca rodaram lá (o orquestrador já audita isso). Reverificar a fase depois.

## Warnings
- **W1. O e2e "storage tab upload progress and confirmed delete" é instável (gate 7).**
  - **Onde:** `tests/e2e/storage.spec.mjs:68-69` roda o axe (`expectAccessible`) com o envio demo em andamento e só então clica em Cancelar. O envio demo dura cerca de 3,75 s: `DEMO_STEP_MS = 150` com 8 + 16 passos (`src/demo-backend.js:101-103`). Sob carga, o envio termina antes do clique. O botão some ("element is not visible"), ou a região "Envio cancelado" nunca aparece.
  - **Medições locais:**
    - Execução do gate: 1 falha em 88.
    - `--repeat-each=10` com os workers padrão: 11 falhas em 40 (`dark-pt` e `light-en`).
    - Com `--workers=4`: 0 falhas em 20.
  - **Origem:** a corrida vem da storage-video. Os 4 projetos da T-7.6 (claro/escuro × pt-BR/en) aumentaram a carga paralela e a tornaram mais provável. O CI `node-ui` passou em `e00abb8`, mas a esteira do corte pode ficar vermelha por acaso.
  - **Sugestão:** o demo segura o envio até o teste liberar (um parâmetro de URL, ou uma promessa exposta ao teste), ou o axe roda depois do cancelamento. Nunca aumentar o tempo do demo como "correção".
- **W2. `net.ping` pinga 8.8.8.8 a cada segundo sempre que há sensores reais, sem opção de desligar (D-2026-09-30-release-polish-5; PROJECT "sem nuvem").**
  - **Onde:** `SystemSensors::with_options` sempre inicia o `Ping` (`crates/bezel-sensors/src/system.rs:61`), com `EVERY = 1 s` (`ping.rs:25`) e alvo padrão `8.8.8.8` (`system.rs:27`).
    - Isso vale para `bezel run` (e o serviço `bezel-run@`), `bezel sensors` e o studio, mesmo com um tema que não usa `net.ping`.
    - Onde não há socket ICMP sem privilégio, o fallback abre um TCP connect para `8.8.8.8:53` a cada segundo (`ping.rs:189-200`). No Windows é sempre assim, e também em Linux fora do `ping_group_range`. São cerca de 86 mil conexões por dia a partir de um serviço de login.
  - **Docs:** `docs/user/sensors.md:60-64` só ensina a trocar o alvo.
  - **Contexto:** a D-5 fixa o como, não o quando. Não é violação, mas é tráfego de rede contínuo num app que se declara sem nuvem, e pode disparar firewall ou IDS.
  - **Sugestão:** decidir numa D-XX entre iniciar o probe só quando `net.ping` está ligado ao tema ou visível no studio, permitir "desligado" (alvo vazio) ou pelo menos documentar o comportamento.
- **W3. A verificação de tamanho chega ao studio por parsing de texto, sem teste entre crates (DRY; nota da T-7.6).**
  - **Onde:** `messages.rs:216-229` reconhece `sizeMismatch` pelo sufixo e pelo formato do `BezelError::Transport` que o core monta em `crates/bezel-core/src/app/storage.rs:305-310`.
  - **Teste:** `a_failed_size_check_has_its_own_code` (`messages.rs:439`) usa uma cópia escrita à mão da frase, então uma mudança no core passa nos testes e quebra o botão Apagar no studio.
  - **Backlog:** o PLAN e o SUMMARY dizem que "a variante no core fica no backlog", mas não há todo registrado em `.jdi/todos/`.
  - **Sugestão:** registrar o todo (`BezelError::SizeMismatch`) e, até lá, um teste no studio que passe pelo `app::storage::upload` real com um `FakeConnector` que guarda o tamanho errado.
  - **Mesma família (informativo):** o `{detail}` dos códigos `transport`, `invalidInput`, `themeFile`, `invalidTheme`, `refused` e `system` continua em inglês dentro da frase pt-BR.
- **W4. Defeitos de documentação da fase (gate 6).**
  - **PLAN:** a linha `PLAN.md:55` está corrompida (`\1- **Status:** completed (...)`, resto de um `sed` em `475fc06`) e apagou a linha `**Test:**` da T-7.4. Era `udev_rules::tests::printed_rule_matches_packaged_file_and_catalog`, `hid_desktop::` e `hid_desktop_requires_confirm`, como em `475fc06^`.
  - **CONTEXT:** `CONTEXT.md:24` ainda lista "vídeo no host" fora do escopo, contra a D-2026-09-30-studio-app-7 e a própria linha 7. `CONTEXT.md:18` e `PLAN.md:7` citam só D-…-1..9; a D-10 fica de fora dos refs canônicos.
  - **Todos:** três seguem abertos com o trabalho entregue: `.jdi/todos/2026-09-30-hid-desktop-mode.md:1` (T-7.4), `2026-09-30-importer-sensor-keys.md:1` (T-7.2) e `2026-09-30-sensors-review-followups.md:1`. Deste último, o que sobrou já está em `2026-09-30-release-polish.md:3`.
  - **SUMMARY:** o § Files modified (`SUMMARY.md:52`) omite `docs/reverse-engineering/devices.md` (`83800f2`).
  - **CHANGELOG:** o `[Unreleased]` ainda não diz que o studio decodifica o vídeo no PC para telas sem reprodução (D-2026-09-30-studio-app-7). Isso sobrou do W3 da studio-app.
- **Menores (informativo).**
  - O teste corrigido em `e00abb8` monta a linha esperada com o próprio `quoted()` (`live.rs:733`). Isso só confere a ligação, e a regra de aspas fica presa por `paths_are_quoted_for_the_shell`, que é suficiente.
  - `set_language` (síncrono) pega o lock da sessão na thread principal (`backend.rs:306`). É curto desde a correção do W7 da studio-app.
  - `udev_help.rs` grava a regra no cache do usuário e o comando instala esse arquivo com `sudo install`. Algo rodando como o usuário poderia trocá-lo antes do sudo. O risco é baixo e a CLI (`bezel udev-rules | sudo tee`) não tem esse caso.
  - `"cpu.fan.percent"` em `import/python_yaml.rs:37` é uma chave que o Bezel propositalmente não publica, e está documentada. Não é resto da T-7.2.
  - O `bezel-studio` não é compilado no clippy cruzado (o build script precisa do Windows SDK). O studio no Windows só é exercitado pelo CI, que hoje para antes (B1).
  - O "não validado no hardware" (D-8) aparece na CLI (`devices.rs:66`), no studio (`desktop.notValidated`, `desktop.confirmRisk`) e nas docs (`check-docs.sh` exige a frase).

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS (Linux) | 651 passed, 0 failed, 7 ignored: `OK`. No Windows o mesmo suite falha (B1) |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | comando do DoD: TOTAL 94.84%, `OK` |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | Não há release ainda. Evidência sugerida: `## [1.0.0] - <data>` no commit de corte da T-7.8, incluindo o vídeo no PC do studio (W4) |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | Sem "early development" e com links para `docs/user` (conferido pelo `check-docs.sh`). Evidência sugerida: revisão humana do diff do README no PR do corte |
| 6 | Regra udev do pacote = catálogo = `bezel udev-rules` | CONTEXT | Auto | PASS | `OK` (`udev_rules::tests::printed_rule_matches_packaged_file_and_catalog`, 1 passed) |
| 7 | FPS por fixture, ausente/velho = Unavailable | CONTEXT | Auto | PASS | `OK` (`fps::`: 27 passed) |
| 8 | `net.ping` não bloqueia (< 50 ms) + chaves importadas | CONTEXT | Auto | PASS | `OK` (`ping::` + `imported_keys_are_published`: 14 passed) |
| 9 | Paridade i18n e mensagens do backend por código | CONTEXT | Auto | PASS | `OK` (`node --test`: 22/22) |
| 10 | Studio claro/escuro × pt-BR/en (Playwright + axe) | CONTEXT | Auto | PASS | `npm test`: exit 0, 86/86 unitários e 88/88 e2e. Na execução anterior (gate 7) houve 1 falha instável (W1) |
| 11 | HID desktop mode listado; volta exige Confirm | CONTEXT | Auto | PASS | `OK` (`hid_desktop::` 11 passed; `hid_desktop_requires_confirm` 2 passed, unitário e binário) |
| 12 | Empacotamento com udev, unit e temas | CONTEXT | Auto | PASS | `OK`. Só o repositório foi conferido: não há deb/rpm em `target/release/bundle` nem em `dist/` |
| 13 | Docs de usuário en/pt-BR, links e segredos | CONTEXT | Auto | PASS | `OK` (14 páginas nos dois idiomas) |
| 14 | 8.8" real: studio ao vivo, aba Tela, vídeo com alfa, boot, cancelamento sem sobra + Manual das fases 1-6 | CONTEXT | Manual | MANUAL_REQUIRED | A tela está travada desde o teste de enchimento e precisa ser religada antes. Evidência sugerida: `/jdi-confirm-dod` por fase e o todo `revc-cancel-leftovers` (envio grande sem cancelar, cancelar e reenviar com a verificação de tamanho, apagar `sd/video/bezel_test_cancel.mp4`), registrados no SUMMARY § Hardware validation com fotos ou descrição |
| 15 | Instalar deb/rpm/AppImage e msi/nsis do candidato; FPS num jogo real | CONTEXT | Manual | MANUAL_REQUIRED | **Depende de B1:** com o `rust-windows` vermelho, a `main` não gera msi/nsis. Evidência sugerida: `check-packaging.sh` sobre os deb/rpm do run, relato de instalação dos 5 pacotes e `gpu.fps` lido com RTSS e MangoHud |

## Recommendation
**Bloquear até o Windows ficar verde.** Todos os gates locais passam:
- build, 651 testes, 94,9% de cobertura, fmt e clippy (inclusive cruzado para Windows), `cargo audit`, gate 5 inteiro e as 11 linhas Auto do DoD;
- a D-10 foi seguida à risca e os achados W1 e W2 da studio-app foram fechados com teste.

A fase, porém, é a do release. O único binário de teste que já rodou no Windows falha, o que para a esteira e impede os pacotes do Windows.

Antes de reverificar:
1. **B1:** corrigir `crates/bezel-cli/src/live.rs:762` (usar `quoted(&zipped.to_string_lossy())`). Depois, levar o `rust-windows` a verde com os 24 binários rodados, tratando as falhas que surgirem nas suítes que nunca rodaram no Windows.
2. **W1:** tirar a corrida do e2e de armazenamento, com o envio demo esperando o teste em vez de mais tempo.
3. **W2:** decidir numa D-XX quando o `net.ping` sonda a rede (só se usado, ou desligável) e documentar.
4. **W3 e W4:** registrar o todo do `SizeMismatch` e um teste que passe pelo core. Consertar `PLAN.md:55`, `CONTEXT.md:18,24`, os três todos já entregues, o SUMMARY (`devices.md`) e o CHANGELOG (vídeo no PC do studio).

Depois disso, `/jdi-verify release-polish` de novo. Com tudo verde, o veredito esperado é APPROVED_PENDING_MANUAL, com a T-7.8 (8.8" religada, pacotes do candidato, FPS real) e o corte da 1.0.0 pela D-2.
