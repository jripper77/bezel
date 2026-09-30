# Phase 6: Review  (slug: storage-video)

**Verdict:** APPROVED_PENDING_MANUAL

> Re-verificação em modo `verify` (2026-09-30) depois do `BLOCKED` de `3360b3c`. `HEAD` = `f81a145`, árvore limpa
> (só o REVIEW.md antigo removido). Refiz todos os gates do zero e não reaproveitei nenhum número do SUMMARY nem do
> relatório anterior. Nenhum comando abriu a 8.8" real, que continua com o serviço do usuário: rodei só testes, fakes
> e o modo demo. Não rodei os `#[ignore]` (ffmpeg real, timing, corpus); o `bezel-media` não mudou desde `535e403`.
> A T-6.8 (hardware) continua `in_progress`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1) |
| Tests | PASS | 509 passed, 0 failed, 6 ignored (4 de ffmpeg real, 1 de timing do renderer, 1 de corpus local; nenhum de hardware). Não houve queda: eram 499 no relatório anterior. `bezel-core`: 60 `--lib`, 11 em `tests/storage.rs`, 7 em `tests/runtime_video.rs` e 3 em `tests/screens.rs` |
| Coverage | PASS | **94.94%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Com o comando do DoD, sem filtro: 94.87% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Nenhum `#[allow]` fora de teste |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 limpos. O B1 foi resolvido: nenhum port é implementado em `crates/bezel-core/src`, nem em teste (detalhe abaixo) |
| Consistency | PASS (com WARN) | Os 12 commits desde `3360b3c` têm escopo `storage-video` e todos os arquivos cabem no § Files modified do PLAN. Nenhuma D-XX é contrariada. O SUMMARY é anterior às correções (W3) |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: 57/57, 99,80% de linhas. `npx playwright test`: 24/24 em claro e escuro, com axe sem violação séria ou crítica e sem erro de console. Os textos novos do boot estão em pt-BR e en, sem string fixa, e `prefers-reduced-motion` é respeitado |
| DoD | PASS / PENDING MANUAL | Os 3 itens Auto do PROJECT e os 8 do CONTEXT passam como estão escritos. Faltam 2 itens Manual do CONTEXT (8.8") e 2 do PROJECT |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: `[dependencies]` só tem `thiserror`. `bezel-devices` e `bezel-sensors` (novo, `67d91fb`) estão só em `[dev-dependencies]`, e `crates/bezel-core/src` não cita nenhum dos dois |
| 5.2 I/O, threads, cfg de plataforma | PASS: o grep não acha nada |
| 5.3 ports | PASS: o grep do gate não acha nada. Estendi a busca a todos os 9 traits de `ports/mod.rs` (`ScreenStorage`, `MediaTranscoder`, `VideoFrames`, `ScreenConnector` etc.) e a qualquer `impl … for` em `crates/bezel-core/src`: só aparecem `Display`/`Debug`/`Default`. `app/storage.rs` não tem mais `mod doubles`, e o `mod tests` de `app/runtime.rs:405` só testa funções puras. Os dublês foram para `crates/bezel-core/tests/{storage,runtime_video}.rs`: `LocalFiles`, `Decoder`, `Recorder` e `Clip`, sobre `FakeConnector`/`FakeStorage`/`FakeSensors`. Fora do core, `MediaSetup: MediaTranscoder` (`src-tauri/src/storage.rs:88`) é um auxiliar do studio; `Wire`/`Pause`/`Clock`/`Pace` vêm de fases anteriores |
| 5.4 adapters só na composição | PASS: o grep do gate não acha nada. `FakeConnector::default/with_storage` fora de `main.rs`/`lib.rs` aparece só depois do `#[cfg(test)]` de cada arquivo (`screen.rs:206`, `live.rs:404`, `storage.rs:1004`, `backend.rs:465`, `studio.rs:457`) |
| 5.5 `unsafe` | PASS: a única ocorrência é a string `"unsafe asset path"` (`bezel-themes/src/native.rs:39`) |
| 5.6 panics fora de teste | PASS: os acertos fora de `#[cfg(test)]` estão em `bezel-sensors/src/testing.rs`, `bezel-render/src/{golden,testkit}.rs`, e os três módulos só são declarados sob `#[cfg(test)]` |
| 5.7 escrita no dispositivo | PASS: o `confirm_of` saiu de `src-tauri/src/storage.rs` e agora só existe em `commands.rs:239`. O que sobra do grep é doc, o próprio `Confirmed::require` (`domain/storage.rs:383`) e testes (`driver/turing_rev_c.rs:697`, `turing_usb.rs:877`, `fake.rs:499`, todos depois do `#[cfg(test)]`). Nenhum transporte real é aberto em `crates/*/tests` |
| 5.8 fidelidade de protocolo | PASS: todo encoder de `protocol/` tem teste, e nem `protocol/` nem `driver/` mudaram desde `3360b3c`. Conferi de novo contra `protocol-turing-rev-c.md`: `MAX_UPLOAD_PATH` = `MAX_INLINE` − 4 = 236 (§ 3: "236 for 0x6F"), `FLASH_RESERVE_KIB` = 512 e `CARD_PRESENT_ABOVE_KIB` = 1024 (§ 13.2) |
| 5.9 caminhos e plataforma no core | PASS: os acertos são doc e testes de fases anteriores (`device.rs:75`, `discovery.rs`, `sensor.rs`) |
| 5.10 comandos Tauri | PASS: os comandos síncronos são `list_fonts`, `get/set_autostart` e `cancel_job`, que só marca o token. `set_boot_media` com brilho é `async` com `blocking` |
| 5.11 supply chain e segredos | PASS: `cargo audit` (0.22.2, 1277 advisories, 601 crates) sai com 0, inclusive com `--deny warnings`. Nenhum segredo |

## Situação dos achados de `3360b3c`
| Achado | Situação | Evidência |
|---|---|---|
| B1: ports implementados no core | **Resolvido** (opção a) | `a087db3` e `67d91fb` levaram os casos de uso para `tests/`. O teste `--lib` do DoD agora prova só o puro (`Confirmed::require`) e, por coerção de tipo (`domain/storage.rs:716-717`), que `delete`/`set_start_mode` do port exigem `Confirmed`. O "nada sai antes da recusa" (inclusive brilho, sobrescrita, `load`/`transcode`) ficou em `tests/storage.rs:336` `replacing_deleting_and_the_boot_slot_need_confirmation`, que a linha 2 do DoD também roda (`f81a145`) |
| W1: `Confirm` fora de `commands.rs` | Resolvido | `ec3ddf0`: `confirm_of` em `commands.rs:239`, e o `storage.rs` do studio só recebe `Confirm` |
| W2: brilho e sleep no boot | Resolvido | `5fb4914`: `app::storage::set_boot_media(…, brightness: Option<Brightness>, …)` manda o brilho depois do `Confirmed` e do `ensure_stored` e antes do start mode (`app/storage.rs:319-340`; teste `tests/storage.rs:703`). `77f0870`: o resumo da CLI cita o sleep desligado. `a87f80b`: o studio envia o brilho ajustado nesta sessão, e o diálogo diz qual brilho vale (ou ~67% por padrão) e que a tela não entra em repouso. Resta que o brilho é lembrado só durante a sessão, não nas settings, mas o diálogo avisa isso ao usuário |
| W3: cancelamento rev C | Parcial → **W1** abaixo | O todo tem alvo `release-polish` e cita a recuperação que estourou o tempo. O § 19 registra o caso. Continua sem tratamento o erro genérico do studio |
| W4: `SerialWire` sem teste | Resolvido | `d6ff144`: `write_patiently`/`drain_patiently` puros, com 4 testes (sinal, stall, `WriteZero`, 16 flushes). `wire.rs` está com 82,97% de linhas (era 52,21%) |
| W5: DRY e fallback do studio | Registrado → W6 | `storage-core-helpers` agora tem alvo `release-polish` |
| W6: `let _ = write!` | Resolvido | `c1aab4c`: `Messages` (`bezel-cli/src/messages.rs`) guarda a primeira falha e o comando termina com erro. Na CLI, os dois `let _` que sobram fora de teste têm a razão escrita (`main.rs:198`, `live.rs:141`). Fora da CLI, `bezel-media/src/process.rs:103-106` tem a razão na doc. Já `process.rs:73` e os de janela em `src-tauri/src/lib.rs:104,147-149` não têm razão, mas são anteriores e ficam fora do escopo do W6 (ver W4) |
| W7: docs da T-6.8 | Parcial → **W2** abaixo | `d29fd6f` preencheu o § 19 e atualizou CHANGELOG e o Status do README, mas o texto afirma mais do que a evidência humana já confirmou |
| W8: orçamentos e views | Resolvido | SUMMARY com 5.055 caracteres (limite 8.192), PLAN com 11.941 (12.000), CONTEXT com 5.799 (6.000). `build.rs` está no § Files modified. `DECISIONS.md` já tem a D-7 e `todos.md` tem os três todos |

## Blockers
Nenhum.

## Warnings
- **W1. Cancelamento no studio quando o HELLO não volta (resto do W3 anterior; D-2026-09-30-storage-video-6).**
  - Na 8.8" o caso real é este: depois de cancelar um envio para o cartão, o firmware não responde o HELLO
    (§ 19, `protocol-turing-rev-c.md:714`). O driver devolve `BezelError::Timeout("… then check {path} for a partial
    file")` (`driver/turing_rev_c.rs:376-382`).
  - O studio converte isso em `timeout` (`src-tauri/src/storage.rs:74`), e a UI troca a mensagem por
    `storage.error.timeout`, "A tela não respondeu a tempo. Confira o cabo e tente de novo." (`src/ui/storage.js:121`).
    Com isso a dica do arquivo parcial se perde, e o "offers a confirmed delete of the partial file" da D-6 não
    acontece nesse caminho.
  - O todo `2026-09-30-revc-cancel-leftovers.md` não fala do studio.
  - **Ação:** registrar o caso do studio no todo, ou dar a esse erro um código ou aviso próprio que nomeie o parcial e
    sugira reconectar e apagar.
- **W2. A documentação afirma mais do que a evidência manual.**
  - `README.md:10-11` diz que armazenamento e vídeo "(`bezel storage`, the studio's Storage tab) work on the 8.8"".
  - `protocol-turing-rev-c.md:713` (§ 19, Playback) diz que "the overlay keeps its alpha".
  - Mas o SUMMARY (`:69`) e os dois itens Manual do CONTEXT ainda tratam a aba do studio na 8.8" e a transparência
    sobre o vídeo como pendentes de confirmação humana.
  - O SUMMARY também não diz se o teste no hardware usou `scripts/install-local.sh` (aceite da T-6.8).
  - **Ação:** fechar os itens manuais ou suavizar o texto até lá.
- **W3. O SUMMARY é anterior às correções.**
  - § Tests (`:53-56`) ainda diz 499 testes, 56 unitários e 94,88%. Hoje são 509, 57 e 94,94%.
  - § Files modified não lista `crates/bezel-core/tests/runtime_video.rs`, `crates/bezel-core/Cargo.toml`,
    `crates/bezel-cli/src/{messages,devices,theme}.rs` nem `apps/bezel-studio/src/ui/library.js`.
  - Os commits de correção da revisão (`a087db3`..`a87f80b`, `f81a145`) não aparecem.
  - **Ação:** atualizar o SUMMARY, que ainda cabe folgado no orçamento.
- **W4. Todo desatualizado (cosmético).** `2026-09-30-render-engine-review-followups.md` ainda pede razão ou tratamento
  para os `let _ = write!` de `live.rs`/`theme.rs`, e o `c1aab4c` já resolveu isso. Tire a cláusula e ponha no lugar os
  `let _` sem razão que restam fora da CLI: `bezel-media/src/process.rs:73`, que é desta fase, e
  `apps/bezel-studio/src-tauri/src/lib.rs:104,147-149`, que vem do studio-app.
- **W5. Cobertura baixa no adapter de ffmpeg (informativo, já existia).**
  - `crates/bezel-media/src/lib.rs` está com 57,18% de linhas. O `impl MediaTranscoder for FfmpegTranscoder` só roda
    nos 4 testes `#[ignore]` de ffmpeg real.
  - O arquivo não mudou desde `535e403`, e o TOTAL fica bem acima de 80%.
  - **Sugestão:** usar o executável falso de `transcode::tests::with_fake_ffmpeg` também em `FfmpegTranscoder`, ou
    criar um job de CI com ffmpeg.
- **W6. Dívida registrada, com alvo.** `fitting_options` (studio) e `convert_options` (CLI) duplicam a regra de girar e
  recortar, e o studio não tem o fallback de vídeo decodificado no PC (D-4 e D-2026-09-30-studio-app-4). Os dois estão
  em `2026-09-30-storage-core-helpers.md` → `release-polish`. É aceitável.

## Todos da fase (avaliação)
- `2026-09-30-storage-video.md`: aceitável. Tudo tem alvo, cita a D-XX e casa com o "Out of scope".
- `2026-09-30-tur-usb-size.md`: aceitável. É consequência direta da D-7, com proposta concreta e alvo `release-polish`.
- `2026-09-30-revc-cancel-leftovers.md`: aceitável e com alvo, mas falta o caso do studio (W1).
- `2026-09-30-storage-core-helpers.md`: aceitável e com alvo (W6).

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | 509 passed, 0 failed, 6 ignored (gate 2) |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | comando do DoD: TOTAL 94.87% lines, `OK` (gate 3: 94.94%) |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | sem release nesta fase. O Unreleased cita `bezel storage`, `bezel run` com vídeo e a aba Storage do studio. Evidência sugerida: `## [x.y.z]` no release |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | § Screen storage and video descreve a CLI, e o Status foi atualizado, mas afirma mais do que o hardware confirmou (W2). Evidência sugerida: diff do README revisado no PR |
| 6 | Rev C storage packets and storage-info reply match the vectors | CONTEXT | Auto | PASS | `storage_packets_match_the_reference_vectors` e `storage_info_subtracts_the_reserved_flash_and_detects_the_card`: `OK` |
| 7 | Delete and overwrite refused without `Confirm::Yes`, before any byte | CONTEXT | Auto | PASS | `domain::storage::tests::destructive_operations_require_confirm_yes` (1 passed) e `--test storage replacing_deleting_and_the_boot_slot_need_confirmation` (1 passed): `OK`. Nenhum dos dois depende de port implementado no core |
| 8 | Rev C upload follows the vendor sequence, progress, cancel | CONTEXT | Auto | PASS | `driver::turing_rev_c::tests::upload_reports_progress_and_can_be_cancelled`: `OK` |
| 9 | Preflight rejects names, sizes, resolution, full storage; never deletes | CONTEXT | Auto | PASS | `domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage`: `OK` |
| 10 | ffmpeg args are a vector with the vendor chain; missing ffmpeg is a state | CONTEXT | Auto | PASS | `transcode::tests::builds_the_vendor_argument_vector_for_rev_c` e `probe::tests::missing_ffmpeg_is_reported_not_fatal`: `OK` |
| 11 | Video background renders a transparent base | CONTEXT | Auto | PASS | `renderer::tests::device_video_background_renders_a_transparent_base`: `OK` |
| 12 | `bezel storage rm` without `--yes` is refused | CONTEXT | Auto | PASS | `storage::tests::rm_without_yes_is_refused` (1 passed): `OK` |
| 13 | Studio storage tab: progress, cancel, confirmed delete (light and dark) | CONTEXT | Auto | PASS | `npx playwright test -g "storage tab upload progress and confirmed delete"`: `OK` (2 passed, claro e escuro) |
| 14 | 8.8" real: info e listas, upload/verify/play/stop/delete, imagem e vídeo | CONTEXT | Manual | MANUAL_REQUIRED | a CLI já tem evidência no SUMMARY § Hardware validation; falta a aba do studio na 8.8". Evidência sugerida: "studio → Tela → Armazenamento na 8.8": enviar, tocar, parar e apagar `bezel_test_*` na flash e no cartão; cancelar um envio (ver W1)", com captura |
| 15 | 8.8" real: vídeo em loop com o tema por cima (alfa) e boot após desligar e ligar | CONTEXT | Manual | MANUAL_REQUIRED | o protocolo está verificado (`full_png_sucess` após PLAY_VIDEO). Evidência sugerida: foto ou descrição do tema sobre o vídeo; `bezel storage boot sd/video/bezel_test_*.mp4 --brightness 40 --yes`, desligar e ligar (vídeo e brilho de 40%), depois `boot default --yes` |

## Recommendation
Aprovada com pendência manual. Nenhum gate automático bloqueia:
- o B1 foi resolvido pela opção (a), e o hexágono não tem nenhum port implementado, nem em teste;
- `Confirm` só é criado nas fronteiras humanas (`--yes` da CLI e `commands.rs` do Tauri);
- o brilho do boot passa pelo caso de uso do core, atrás da mesma confirmação;
- a cobertura está em 94,9%, e a UI passa em axe, i18n e teclado.

Para fechar a fase (T-6.8):
1. Validar na 8.8" a aba de armazenamento do studio, a transparência sobre o vídeo e o boot após desligar e ligar
   (itens 14 e 15). Registrar a evidência no SUMMARY § Hardware validation, citando `scripts/install-local.sh`.
2. Resolver W1: o caso do studio no todo, ou uma mensagem própria para o parcial.
3. Ajustar W2 e W3: README e § 19 de acordo com a evidência, SUMMARY com as correções e os números atuais.

Depois disso, a fase pode ir para `APPROVED_WITH_WARNINGS`, ou para `APPROVED` se W1–W4 também forem tratados.
