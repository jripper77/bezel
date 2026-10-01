# Phase 7: Review  (slug: release-polish)

**Verdict:** APPROVED_PENDING_MANUAL

> Reverificação em modo `verify`, depois do BLOCKED de `bbc680d`. `HEAD` = `de64dd3`, árvore limpa. Os três commits à
> frente de `origin/main` (`7cee8ab`, `4f6f8b8`, `de64dd3`) só mexem em `.jdi/`, então o código de `HEAD` é o de
> `c6773ba`.
>
> - Rodei todos os gates. Nenhum número veio do SUMMARY; onde o meu número difere do dele, digo.
> - **Escopo julgado:**
>   - as correções dos achados B1 e W1–W4 do review anterior;
>   - as tarefas T-7.9 a T-7.12 e as correções do orquestrador `79920ad` e `c6773ba`;
>   - todos os commits `*(release-polish)*` de `bbc680d..HEAD`;
>   - as decisões D-2026-09-30-release-polish-1..13.
> - **Fora do escopo:** a fase 8 `storage-manager`. Os três commits dela (`8fd014e`, `63eabea`, `11193ad`) só tocam
>   `.jdi/`.
> - **Hardware:** nenhum comando abriu a 8.8" real. Não rodei nenhum `#[ignore]` nem `BEZEL_HW_TESTS`, e não toquei
>   em `/dev/tty*`, hidraw ou systemd. Os testes novos que chamam o `SystemConnector` real usam portas inexistentes
>   (`/dev/bezel-no-such-port`, `/dev/bezel-no-such-mcu`, `connector.rs:886-910`). O `bezel restart` da CLI só roda com
>   `--fake` (`tests/devices.rs:52-72`).
> - **Playwright:** rodou com `BEZEL_E2E_PORT=1433`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1). Nenhum `Cargo.toml`/`Cargo.lock` mudou desde `bbc680d` |
| Tests | PASS | Linux: **749 passed, 0 failed, 9 ignored**. Os ignorados são 6 de ffmpeg real, 1 de timing, 1 de hardware do studio e 1 de corpus. Bate com o SUMMARY (749/9) e subiu desde o review anterior (651). Windows, no CI de `c6773ba` (run 36805865134, job `rust-windows`): 27 resultados de teste, todos os binários, **686 passed, 0 failed, 9 ignored** |
| Coverage | PASS | **94.79%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD, sem filtro: **94.73%**, igual ao SUMMARY. Ver o detalhe por arquivo abaixo |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. O clippy cruzado para `x86_64-pc-windows-msvc` (7 crates, `CARGO_TARGET_DIR=target/wincheck`) também sai com 0. Os únicos `allow` fora de teste são os 4 de `fps/rtss.rs:247,269,302,323`, com `reason =`, sem mudança desde o review anterior |
| Hexagonal/Safety/Protocol/Hygiene | PASS (com WARN) | 5.1–5.11 limpos (detalhe abaixo). `cargo audit` e `cargo audit --deny warnings`: exit 0 (1277 advisories, 604 crates). Uma pré-condição do reinício pelo MCU não é conferida (W1) |
| Consistency | PASS (com WARN) | Os commits têm escopo `release-polish`. O código segue D-1..D-13. Há duas pendências de registro: a D-13 não diz que substitui a parte "C9/RESTART" da D-8, e a § 16 do protocolo ainda chama o `c9` de "never sent implicitly" (W2). CONTEXT e PLAN estão desatualizados (W3) |
| UI Validation | PASS | `npm ci` ok. Comando do gate: **144/144**. `storage.spec.mjs --repeat-each=10`: **240/240** (o W1 anterior falhava 11 de 40). `npm test`: **125/125** unitários (99,85% de linhas) e **144/144** e2e. Nenhum erro de console, e o axe não acusou violação séria nem crítica (os specs conferem os dois). Os textos novos estão em i18n, os filtros usam botões com `aria-pressed`, e o GIF da prévia respeita `prefers-reduced-motion` (`app.js:163-166`) |
| DoD | PASS / PENDING MANUAL | As 8 linhas Auto do CONTEXT e as 3 do PROJECT passam como escritas. Faltam 2 Manual do CONTEXT e 2 do PROJECT |

**Cobertura dos arquivos da fase (linhas):**
- **core:** `runtime.rs` 100%, `animation.rs` 98.80%, `poster.rs` 100%, `reconnect.rs` 100%, `screens.rs` 95.56%,
  `domain/storage.rs` 99.39%;
- **dispositivos:** `connector.rs` 97.78%, `wire.rs` 83.21%;
- **mídia:** `gif.rs` 98.46%, `poster.rs` 95.95%, `transcode.rs` 98.95%;
- **sensores:** `ping.rs` 94.76%, `system.rs` 97.92%;
- **CLI:** `live.rs` 97.89%;
- **studio:** `thumbnails.rs` 97.00%, `video.rs` 97.60%, `library.rs` 96.39%, `studio.rs` 97.85%, `backend.rs` 98.35%.
- **Abaixo de 80%:**
  - `bezel-media/src/lib.rs` 47.90%. É a cola do `FfmpegTranscoder`, incluindo o `poster()` novo, que só os testes
    `#[ignore]` de ffmpeg real alcançam.
  - A cola do Tauri: `commands.rs` 1.54% e `lib.rs` 23.93%.

### CI do código de `HEAD`
- **Run 36805865134** (`c6773ba`, `completed success`): todos os jobs verdes, inclusive `rust-linux`, `rust-windows`
  (13m23s), `node-ui` e `Portao`.
  - O Windows rodou todos os 27 binários de teste e gerou `Bezel_0.8.0_x64_en-US.msi` e `Bezel_0.8.0_x64-setup.exe`.
  - A release `v0.8.0` saiu com deb, rpm, AppImage, msi, nsis, os dois arquivos da CLI e `SHA256SUMS`.
- **Pacotes da v0.8.0:** baixei o deb e o rpm, os SHA256 conferem, e `check-packaging.sh` estrito passou nos dois
  (rule, unit, temas, postinstall e `bezel udev-rules` igual à regra empacotada).
- **Run anterior:** 36803989738 (`79920ad`) também verde, com 678 passed no Windows.
- **Diferença do SUMMARY:** `SUMMARY.md:48` cita releases até v0.7.0. A v0.8.0 saiu depois dele, pelo run de `c6773ba`.

### Achados do review anterior (`bbc680d`)
| Achado | Situação | Evidência |
|---|---|---|
| B1 testes falhando no Windows | **Resolvido** | `live.rs:1058` usa `quoted(&zipped.to_string_lossy())` (`cd276fd`; imports só-Unix em `6bb5ae6`). Os runs 36803989738 e 36805865134 estão verdes no `rust-windows`, com 27 resultados de teste em vez de 1 binário. O msi e o nsis saem, e o `Portao` passa |
| W1 e2e de armazenamento instável | **Resolvido** | `52f48b8`: o demo segura cada fase até o evento `bezel-demo-let-go` (`demo-backend.js` `createDemoGate`, só com `?hold` no modo demo, `bridge.js:150`). O spec libera a fase com `letGo` (`storage.spec.mjs:43-51`, usado a partir de `:53`) e roda o axe com o envio parado. Medi 240/240 com `--repeat-each=10` e os workers padrão, inclusive com o `cargo test` rodando ao mesmo tempo |
| W2 `net.ping` pingando sempre | **Resolvido** | A D-11 foi criada (`007e0bb`). O `SensorSource::want` padrão é no-op (`ports/mod.rs:252`). A thread sobe no primeiro `want` (`ping.rs:109-127`) e fica ociosa sem pacote. Sem o valor na tela, a leitura é `NOT_SHOWN` (`ping.rs:39`). O runtime declara as chaves do tema antes de cada amostra, e a lista do studio usa `show_sensors`. Há testes com probe contador. Resta uma nuance já registrada no SUMMARY: a prévia na bandeja conta como mostrada, o que é coberto pelo texto da D-11 ("preview") |
| W3 tamanho por parsing de texto | **Resolvido** | `BezelError::SizeMismatch { path, sent, stored }` (`error.rs:78`) é emitido pelo core (`app/storage.rs:312`) e mapeado sem parsing (`messages.rs:255`). O teste `a_file_stored_short_fails_its_size_check_with_its_own_code` (`storage/tests.rs:603`) atravessa o core. O todo `english-error-details` foi registrado |
| W4 documentação | **Resolvido** (com resíduo novo em W3) | `PLAN.md:52` recuperou o **Test:** da T-7.4. O CONTEXT não lista mais "vídeo no host" fora do escopo. Os três todos entregues saíram. O SUMMARY lista `devices.md`. O CHANGELOG cita a decodificação no PC. Os novos desvios do PLAN e do CONTEXT estão em W3 |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. `domain/animation.rs` e `domain/reconnect.rs` só usam `Duration`. O relógio é do chamador (`ThemeRuntime::next_due`) |
| 5.3 ports | PASS: nenhum port implementado no core. Os ports ganharam métodos com padrão (`ScreenConnector::restart`, `MediaTranscoder::poster`, `SensorSource::want`, `FrameRenderer::animation`), todos em `bezel_core::ports`. Traits públicas fora do core: `Pause`, `Wire`, `Clock`, `Pace` e `MediaSetup`, todas auxiliares de adapter e anteriores |
| 5.4 adapters na composição | PASS. Fora de `main.rs`/`lib.rs`, `SkiaRenderer`/`FakeSensors` só aparecem em módulos `#[cfg(test)]` (`video.rs:138+`, `backend.rs:846+`, `thumbnails.rs:220+`, `studio.rs:1020+`, `screen.rs:239+`, `live.rs:604+`). `Thumbnails` recebe fábricas injetadas |
| 5.5 `unsafe` | PASS: só os blocos de `fps/rtss.rs` (Windows), com `// SAFETY:`, sem mudança |
| 5.6 panics | PASS: fora de teste, só `bezel-render/src/{golden,testkit}.rs` e `bezel-sensors/src/testing.rs`, todos declarados sob `#[cfg(test)]` |
| 5.7 escrita no dispositivo | PASS. Fora de teste, `Confirm::Yes` só aparece em doc e nos `match` de `Confirmed::require`/`MonitorModeConfirmed::require`. Nenhum `*Transport::open` em `crates/*/tests`. O `c9` é disruptivo, não destrutivo (D-13), e só vai à porta do MCU |
| 5.8 protocolo | PASS: todo encoder tem teste. Conferi `MCU_RESTART = 00 00 00 00 00 c9` (`protocol/turing_rev_c.rs:77`, golden em `:570`) contra o vetor da § 17 ("MCU command … not padded") e a tabela da § 19. Conferi também os opcodes 0xC8/0xCC/0x84 contra a tabela da § 3. O `0x84 RESTART` continua nunca enviado; só aparece em testes que provam a ausência dele |
| 5.9 caminhos no core | PASS: só doc e testes (`discovery.rs`, `reconnect.rs:90-100`) |
| 5.10 comandos síncronos | PASS. O único síncrono novo é `set_theme_filter` (`commands.rs:395`), que só grava ajustes. `add_media`, `restart_screen`, `theme_thumbnail` e `show_sensors` são `async`, e os pesados passam por `blocking` |
| 5.11 supply chain | PASS: `cargo audit` 0, também com `--deny warnings`. Nenhuma dependência nova. Nenhum segredo |

## Blockers
Nenhum.

## Warnings
- **W1. O reinício pelo MCU não confere se outro programa segura a porta do SoC (gate 5; espírito da
  D-2026-09-30-device-protocols-3).**
  - **Onde:**
    - `restart_rev_c` (`crates/bezel-devices/src/connector.rs:275-286`) só abre o MCU. Por isso só confere quem segura
      o MCU, via `open_serial`.
    - O SoC não é conferido, embora o próprio doc diga "Nothing may hold the SoC's port meanwhile" (`connector.rs:274`)
      e o port diga "Nobody may hold the screen's link meanwhile" (`ports/mod.rs:66`).
    - `bezel restart` (`crates/bezel-cli/src/screen.rs:199-225`) e o "Reiniciar a tela" do studio
      (`apps/bezel-studio/src-tauri/src/backend.rs:360-384`, que só para o próprio ao vivo) reiniciam o SoC por baixo
      de quem o estiver usando: o app do fabricante, o turing-smart-screen-python ou o `bezel-run@` do usuário.
  - **Contraste:** o caminho automático respeita a porta ocupada. `connect_rev_c` devolve `InUse` antes de qualquer
    reinício, e o teste `rev_c_failures_without_a_wake_or_of_another_kind_are_returned` confere isso.
  - **Gravidade:** é disruptivo e não destrutivo. Os arquivos ficam, e o `bezel run` volta sozinho (validado na 8.8").
    Mesmo assim, contraria a regra do projeto de recusar com `InUse` em vez de atropelar outro programa.
  - **Sugestão:** antes de mandar o `c9`, chamar `busy::holders` sobre o display listado. Se houver alguém, falhar com
    `InUse` nomeando o programa, ou exigir uma confirmação explícita. Cobrir com um teste do `RevCHost`.
- **W2. A D-13 contraria o texto da D-8 e da § 16 do protocolo sem dizer que os substitui (gate 6).**
  - **D-8:** a D-2026-09-30-release-polish-8 (`.jdi/DECISIONS.md:51`) diz "Firmware update, C9/RESTART and boot logo
    stay out". A D-13 (`.jdi/DECISIONS.md:37`) põe o `c9` no 1.0, inclusive automático, mas só declara que substitui a
    D-2026-09-30-device-protocols-2.
  - **Protocolo:** `docs/reverse-engineering/protocol-turing-rev-c.md:580-582` ainda diz que tudo na tabela da § 16 é
    "**never sent implicitly**", e a tabela inclui o `c9` (`:593`). O `devices.md:256-258` já foi corrigido.
  - **Por que não bloqueia:** o código segue a D-13, que é posterior, específica, validada no hardware e trata o `c9`
    como disruptivo. A contradição está nos registros, não no código.
  - **Correção:** acrescentar à D-13 (e ao índice) que ela substitui a parte "C9" da D-8, e pôr na § 16 a exceção do
    reinício automático uma vez por conexão (D-13).
- **W3. Os documentos da fase não acompanharam a Wave 3b (gate 6).**
  - **Decisões:** `CONTEXT.md:20` e `PLAN.md:7` citam D-…-1..11, e os "Locked decisions" do CONTEXT terminam na D-11
    (`CONTEXT.md:17`). A D-12 e a D-13 regem a T-7.10 e não aparecem.
  - **Testes:** T-7.9, T-7.11 e T-7.12 não têm a linha **Test:** (`PLAN.md:94-99`, `109-114`, `116-121`). Os testes
    existem:
    - T-7.9: `video-background.spec.mjs` e `video-background.test.mjs`;
    - T-7.11: `tests/runtime_animation.rs`, `crates/bezel-cli/tests/animation.rs` e os testes de reconexão de
      `live.rs`/`backend.rs`;
    - T-7.12: `thumbnails.rs`, `themes.spec.mjs` e `theme-filter.test.mjs`.
  - **Execução:** `PLAN.md:138` ainda diz "8 tasks em 4 waves". São 12, com a Wave 3b.
  - **SUMMARY:** `SUMMARY.md:48` para na v0.7.0, mas já existe a v0.8.0.
- **Menores (informativo).**
  - **Reinícios por falha:** a reconexão ao vivo faz até 3 conexões (2, 5 e 10 s; `live.rs:396`, `backend.rs:819`), e
    cada uma pode reiniciar pelo MCU uma vez (`connector.rs:183-205`). Uma tela que segue muda depois do primeiro
    reinício leva até 3 reinícios por queda. É defensável como "uma vez por conexão", e o guia diz isso
    (`docs/user/troubleshooting.md:56-58`). Se "once per failure" da D-13 for literal, cabe limitar a um reinício por queda.
  - **Cobertura do `bezel-media`:** `bezel-media/src/lib.rs` está com 47.90% de linhas, porque o `poster()` real só
    roda nos `#[ignore]` de ffmpeg. O total está bem acima do piso.
  - **Clippy cruzado:** o `bezel-studio` segue fora do clippy cruzado local (precisa do Windows SDK). O CI do Windows
    agora compila e testa o studio e está verde.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | 749 passed, 0 failed, 9 ignored (gate 2). Windows no CI de `c6773ba`: 686 passed, 0 failed |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | comando do DoD: TOTAL 94.73%, `OK` |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | O `[Unreleased]` cobre T-7.9..T-7.12, o teto de 25 MiB e o reinício. Evidência sugerida: `## [1.0.0] - <data>` no commit de corte (D-2) |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | O README já traz `bezel restart`. Evidência sugerida: revisão humana do diff do README no PR do corte |
| 6 | Regra udev do pacote = catálogo = `bezel udev-rules` | CONTEXT | Auto | PASS | `OK` (1 passed) |
| 7 | FPS por fixture, ausente/velho = Unavailable | CONTEXT | Auto | PASS | `OK` (`fps::`: 27 passed) |
| 8 | `net.ping` não bloqueia + chaves importadas | CONTEXT | Auto | PASS | `OK` (16 passed) |
| 9 | Paridade i18n e mensagens do backend por código | CONTEXT | Auto | PASS | `OK` (`node --test`: 22/22) |
| 10 | Studio claro/escuro × pt-BR/en (Playwright + axe) | CONTEXT | Auto | PASS | `npm test` (com `BEZEL_E2E_PORT=1433`): exit 0, 125/125 unitários e 144/144 e2e |
| 11 | HID desktop mode listado; volta exige Confirm | CONTEXT | Auto | PASS | `OK` (`hid_desktop::` 11 passed; `hid_desktop_requires_confirm` 1 + 1 passed) |
| 12 | Empacotamento com udev, unit e temas | CONTEXT | Auto | PASS | `OK` no repositório. Além do comando, os deb/rpm da v0.8.0 passaram no modo estrito |
| 13 | Docs de usuário en/pt-BR, links e segredos | CONTEXT | Auto | PASS | `OK` (14 páginas nos dois idiomas) |
| 14 | 8.8" real: studio ao vivo, aba Tela, vídeo com alfa, boot, cancelamento sem sobra + Manual das fases 1-6 | CONTEXT | Manual | MANUAL_REQUIRED | O SUMMARY § Hardware validation traz medições do orquestrador, e o código bate com elas. O que confere: o teto de 25 MiB (26.214.400 < 29.577.216, `domain/storage.rs:401`), a recusa antes de enviar (`check_size`, `:591`), o bitrate limitado (`transcode.rs:54`), o `c9` mantido 8 s e a espera de 30 s (`connector.rs:35-45`, `:275`), o GIF que só manda o retângulo (`tests/animation.rs`) e a reconexão 2/5/10 s. Falta o humano confirmar o que a tela mostrou (legenda nas 4 orientações, tema, vídeo com alfa), o boot, o studio ao vivo e a bandeja, "Reiniciar a tela" e as miniaturas reais. Evidência sugerida: `/jdi-confirm-dod` por fase, registrado no SUMMARY |
| 15 | Instalar deb/rpm/AppImage e msi/nsis do candidato; FPS num jogo real | CONTEXT | Manual | MANUAL_REQUIRED | O desbloqueio veio com o B1 resolvido: a v0.8.0 tem os 5 pacotes, e deb e rpm passam no `check-packaging.sh` estrito. Evidência sugerida: relato da instalação dos 5 pacotes do candidato e `gpu.fps` lido com RTSS e com MangoHud |

## Recommendation
**Aprovar com o DoD Manual pendente.**
- Todos os gates passam:
  - build;
  - 749 testes no Linux e 686 no Windows, com o CI de `c6773ba` inteiro verde, inclusive o msi e o nsis;
  - 94,79% de cobertura (94,73% pelo DoD);
  - fmt e clippy, inclusive o cruzado para Windows;
  - `cargo audit`, o gate 5 inteiro, 144/144 e2e (240/240 no spec antes instável) e as 11 linhas Auto do DoD.
- O B1 e os W1–W4 do review anterior estão resolvidos, com teste.
- As tarefas T-7.9..T-7.12 seguem D-11, D-12 e D-13 no código.

**Antes do corte da 1.0.0 (D-2), sem precisar de nova verificação:**
1. **W1:** recusar com `InUse` o `bezel restart` e o "Reiniciar a tela" quando outro programa segura a porta do SoC.
2. **W2:** registrar na D-13 que ela substitui a parte "C9" da D-8 e corrigir a § 16 de `protocol-turing-rev-c.md`.
3. **W3:** pôr D-12 e D-13 no CONTEXT e no PLAN, acrescentar as linhas **Test:** de T-7.9, T-7.11 e T-7.12 e
   atualizar a contagem de tarefas e o SUMMARY.

**Depois:** a T-7.8 com o usuário (confirmações na 8.8", os 5 pacotes do candidato e FPS num jogo real) e então o
commit `feat(release-polish)!:` com `BREAKING CHANGE:`, conferindo que a tag sai `v1.0.0`.
