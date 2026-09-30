# Phase 6: Review  (slug: storage-video)

**Verdict:** BLOCKED

> Verificação em modo `verify` (2026-09-30), `HEAD` = `4399451`, árvore limpa. Refiz todos os gates do zero, sem
> aproveitar números do SUMMARY. Nenhum comando abriu a 8.8" real, que segue com o `turing-smart-screen.service`: só
> testes, fakes e `--fake`. Fora da suíte, rodei apenas os 4 testes `#[ignore]` de ffmpeg real do `bezel-media`
> (`cargo test -p bezel-media --lib -- --ignored real_ffmpeg`, ffmpeg 8.1.3 + libx264): 4/4 passaram. Nenhum deles
> usa hardware. A T-6.8 (hardware) continua `in_progress`: a CLI foi validada, e a aba do studio, o alfa visual e o
> boot ainda dependem de confirmação humana.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1) |
| Tests | PASS | 499 passed, 0 failed, 6 ignored (4 de ffmpeg real, timing do renderer e corpus local; nenhum de hardware). Não houve queda: eram 361 no render-engine e 352 no studio-app. Os 4 `#[ignore]` de ffmpeg real também passam (4/4) |
| Coverage | PASS | 94.88% lines (TOTAL, sem `main.rs`/`build.rs`). Com o comando do DoD, sem filtro, dá 94.82% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings` limpos. Nenhum `#[allow]` fora de teste |
| Hexagonal/Safety/Protocol/Hygiene | **BLOCK** | 5.3: ports implementados dentro de `crates/bezel-core/src` (B1). 5.7 tem um acerto literal que julguei conforme (W1). O resto passa |
| Consistency | PASS (com WARN) | Todas as tasks têm commit com escopo `storage-video` e testes. Nenhuma D-XX é contrariada no que faz; há lacunas registradas (W2, W3, W5). A T-6.8 está parcial |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: 56/56, 99,80% de linhas. `npx playwright test`: 24/24 (claro e escuro), com axe sem violação séria ou crítica e sem erro de console. Foco e Esc funcionam nos diálogos, textos via i18n e `prefers-reduced-motion` respeitado |
| DoD | FAIL (gate 5) / PENDING MANUAL | Os 3 itens Auto do PROJECT e os 8 do CONTEXT passam. Ficam 2 itens Manual do CONTEXT e 2 do PROJECT |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: `[dependencies]` só com `thiserror`. `bezel-devices` aparece apenas em `[dev-dependencies]` |
| 5.2 I/O, threads e cfg de plataforma no core | PASS: nada encontrado. `CancelToken`/`Job` não usam thread nem relógio |
| 5.3 ports | **BLOCK (B1)**: o grep do gate acha `impl ScreenLink for Screen` (`app/storage.rs:423`), `impl FrameRenderer for Recorder` (`app/runtime.rs:435`) e `impl SensorSource for Quiet` (`app/runtime.rs:453`). Estendendo o grep aos ports novos, aparecem também `ScreenStorage` (`app/storage.rs:447`), `MediaTranscoder` (`app/storage.rs:528`, `app/runtime.rs:486`) e `VideoFrames` (`app/runtime.rs:523`). Traits fora do core: `MediaSetup: MediaTranscoder` (`apps/bezel-studio/src-tauri/src/storage.rs:88`) é auxiliar do studio (caminho do ffmpeg) e o core não precisa dele, então está OK. `Wire`, `Pause`, `Clock` e `Pace` vêm de fases anteriores |
| 5.4 adapters só na composição | PASS: `FfmpegTranscoder::new` e `FakeConnector::with_storage` aparecem só em `bezel-cli/src/main.rs` e `bezel-studio/src-tauri/src/lib.rs`, fora os testes |
| 5.5 `unsafe` | PASS: a única ocorrência é a string `"unsafe asset path"` (`bezel-themes/src/native.rs:39`) |
| 5.6 panics fora de teste | PASS: nenhum `unwrap`/`expect`/`panic!` antes do primeiro `#[cfg(test)]` nos arquivos da fase (core, drivers, protocolos, `bezel-media`, CLI e studio) |
| 5.7 escrita no dispositivo | PASS com WARN (W1): há um acerto literal em `apps/bezel-studio/src-tauri/src/storage.rs:247`. Os demais acertos ficam em `#[cfg(test)]` (`driver/turing_rev_c.rs:697`, `driver/turing_usb.rs:877`, `fake.rs:485`, testes do core), em documentação ou no próprio `Confirmed::require` (`domain/storage.rs:383`). Nenhum transporte real é aberto em `crates/*/tests` |
| 5.8 fidelidade de protocolo | PASS: todo encoder de `protocol/` tem teste. Conferi bytes contra `protocol-turing-rev-c.md`: UPLOAD_FILE `/mnt/UDISK/video/88.mp4` com 12.345.678 B dá `…00 17 00 00 00 <path> 4e 61 bc 00` (BE32 do caminho e **LE32** do tamanho, § 13.4/§ 17.2); PLAY_VIDEO leva o loop no byte 7 (`…00 18 01 00 00…`); 0x64 desconta 512 KiB do total e do livre da flash e só aceita cartão com TF > 1024 KiB (§ 13.2); OPTIONS `7d…05 00 00 00 aa 02 00 00 00` (§ 6.2); `MAX_UPLOAD_PATH` = 236 (§ 3). Os timeouts do driver (`create_success` 3 s, `file_rev_done` 10/240 s × 15, GET_FILE_SIZE 2 s × 3, PLAY_VIDEO 6 s × 2, STOP_MEDIA 20 × 400 ms) batem com § 13.4/§ 3/§ 7.2. TUR_USB: STORAGE_INFO lê LE32 com o TF antes da flash e considera cartão com TF ≠ 0; WRITE_CHUNK tem `[8..11]` = 1 MiB, `[12..15]` = bytes e `[16]` = último (`protocol-turing-usb.md` § 3/§ 6). 40 e 98 nunca saem, e 42 e 125 não são oferecidos (§ 11 e D-7) |
| 5.9 caminhos e plataforma no core | PASS: os acertos são os mesmos de fases anteriores (`device.rs:75`, `discovery.rs`, `sensor.rs`). `domain/storage.rs:81` é doc de `LONGEST_ROOT_BYTES`, amarrado ao adapter pelo teste `driver/mod.rs:230` |
| 5.10 comandos Tauri | PASS: o único comando síncrono novo é `cancel_job` (`commands.rs:316`), que só marca o token. Os de armazenamento são `async` com `blocking` |
| 5.11 supply chain e segredos | PASS: `cargo audit` sai com 0, inclusive com `--deny warnings` (601 crates). Nenhum segredo |

## Blockers
- **B1. Ports implementados dentro do hexágono (gate 5.3, hexagonal regras 8 e 20).**
  - **Onde:** `crates/bezel-core/src/app/storage.rs:331-572` (`pub(crate) mod doubles`, `#[cfg(test)]`) implementa
    `ScreenLink` (`:423`), `ScreenStorage` (`:447`) e `MediaTranscoder` (`:528`). O `mod tests` de
    `crates/bezel-core/src/app/runtime.rs:405+` implementa `FrameRenderer` (`:435`), `SensorSource` (`:453`),
    `MediaTranscoder` (`:486`) e `VideoFrames` (`:523`). Entraram em `5e36061` (T-6.1) e `3957f14` (T-6.4).
  - **Por que bloqueia:**
    - O primeiro grep do 5.3 tem de sair vazio, e sai com 3 linhas.
    - O projeto já decidiu que `cfg(test)` não é exceção. O commit `8903926` (device-protocols) tirou dublês de
      `DeviceBus`/`ScreenConnector` de dentro do core porque "the reviewer's rule 5.3 forbids [it] even under
      cfg(test)".
    - `crates/bezel-core/Cargo.toml:15-16` registra a mesma política: "Use-case tests run through the adapters'
      fakes (tests/), never through a port implemented inside the hexagon". O doer diz o mesmo em
      `.jdi/agents/jdi-doer-bezel.md:66,163`.
    - O SUMMARY (§ Observações) admite o desvio: "o core não linka os fakes de `bezel-devices` em testes `--lib`".
      Isso explica a causa, mas nenhuma D-XX autoriza a exceção.
  - **Efeito no DoD:** os testes Auto do CONTEXT `domain::storage::tests::destructive_operations_require_confirm_yes`
    (`domain/storage.rs:683`) e os de `app::runtime`/`app::storage` dependem desses dublês.
  - **Correção, uma das duas:**
    - **(a)** Levar os testes de caso de uso para `crates/bezel-core/tests/{storage,runtime}.rs`, sobre
      `FakeConnector`/`FakeStorage`. O `StorageCall` já registra as chamadas, e há adapters locais de teste em
      `tests/` como `LocalFiles`, `impl MediaTranscoder` em `tests/storage.rs:61`.
      - No teste `--lib` do DoD fica só a parte pura: `Confirmed::require` recusa `Confirm::No` para Delete,
        Overwrite e Boot. O port só aceita `delete`/`set_start_mode` com `Confirmed`, então o tipo já garante que
        nada recusado chega a ele.
      - O "nada sai antes da recusa" do overwrite passa a ser provado no teste de integração. Se preciso, ajuste o
        `Verify:` do DoD com uma D-XX.
    - **(b)** Se o usuário preferir manter os dublês, registrar uma D-XX que permita dublês gravadores `#[cfg(test)]`
      de ports dentro do `bezel-core`. Ela revoga a leitura de `8903926` e exige atualizar o comentário do
      `Cargo.toml` e a regra 5.3. O revisor não pode dispensar isso sozinho.

## Warnings
- **W1. 5.7: acerto literal fora das fronteiras listadas (julgado conforme).**
  - **Onde:** `apps/bezel-studio/src-tauri/src/storage.rs:246-247` (`confirm_of(bool)`).
  - **Por que é conforme:** o `bool` vem só dos parâmetros dos comandos Tauri (`commands.rs:294-361`: `overwrite` e
    `confirmed`). A UI só manda `true` depois do `<dialog>` que nomeia o arquivo (`src/ui/storage.js:329,364,373,382`).
    Nenhum caminho interno do backend liga a confirmação.
  - **Sugestão:** converter para `Confirm` dentro de `commands.rs`, para o grep do gate voltar a sair vazio.
- **W2. D-2026-09-30-storage-video-5: brilho e sleep do OPTIONS num link novo.**
  - O driver lembra o último brilho **por link** e usa 170 e sleep 0 quando não houve nenhum
    (`driver/turing_rev_c.rs:92,179-185,588-597`).
  - Na CLI, `storage boot` abre um link novo. O resumo avisa do brilho e `--brightness` escolhe o valor
    (`bezel-cli/src/storage.rs:954-960`), mas o resumo não fala do sleep.
  - No studio, o boot numa tela que não está ao vivo abre um link novo (`src-tauri/src/storage.rs:635-648`). Ele grava
    170 mesmo que o usuário tenha ajustado o brilho no painel nesta sessão (`backend.rs:160-173` não guarda o valor),
    e o diálogo (`storage.confirmBoot`) não menciona brilho nem sleep.
  - Não altera arquivos e está atrás de `Confirm`, mas contraria o "without changing them" da decisão.
  - **Ação:** lembrar o brilho por tela nas settings do studio, passá-lo ao boot e citá-lo no diálogo. Na CLI, citar
    também o sleep.
- **W3. Recuperação do cancelamento rev C no hardware (D-2026-09-30-storage-video-6).**
  - Na 8.8", ao cancelar um upload para o cartão, o HELLO não respondeu, porque o firmware ainda esperava os bytes
    declarados. A sequência "HELLO → GET_FILE_SIZE → oferecer apagar" não aconteceu na mesma sessão.
  - O upload seguinte recebeu ~191 KB de restos. A verificação de tamanho pegou, mas o cartão ficou com 24,3 MiB a
    mais de uso.
  - O registro em `.jdi/todos/2026-09-30-revc-cancel-leftovers.md` é aceitável e acionável, com duas ressalvas:
    - a tag `[storage-video follow-up]` não aponta para nenhuma fase futura;
    - nesse caso o studio mostra só o erro genérico `timeout`, sem o "apagar o parcial", o que não está escrito em
      lugar nenhum.
  - **Ação:** dar fase-alvo ao todo, citar o caso de timeout no todo e registrar o achado em
    `protocol-turing-rev-c.md` § 19 (parte da T-6.8).
- **W4. Correção de hardware sem teste.** `SerialWire::send` (`crates/bezel-devices/src/wire.rs:69-105`, commit
  `2743b4a`) ganhou retentativas de escrita e de flush. `wire.rs` está com 52,21% de linhas. A parte de
  `driver/mod.rs` tem teste, mas a do wire não. Como `port` é `Box<dyn SerialPort>`, um `SerialPort` falso cobriria
  `Interrupted`/`TimedOut` e o limite de 16 flushes.
- **W5. Dívida conhecida e registrada (DRY e D-4 no studio).**
  - `fitting_options` (`src-tauri/src/storage.rs:262`) e `convert_options` (`bezel-cli/src/storage.rs:599`) aplicam
    duas regras de "girar e recortar o vídeo" nos adapters de entrada.
  - O studio não tem o fallback de vídeo decodificado no PC (D-2026-09-30-storage-video-4) que o `bezel run` tem:
    telas WCH mostram só o pôster no Ao vivo. D-2026-09-30-studio-app-4 promete o mesmo `ThemeRuntime` do
    `bezel run`.
  - Ambos estão em `.jdi/todos/2026-09-30-storage-core-helpers.md`. É aceitável, mas falta fase-alvo.
- **W6. `Result` ignorado sem razão (clean-code).**
  - São 27 `let _ = write!/writeln!` em `crates/bezel-cli/src/storage.rs` (a partir de `:286`, `:288`, `:402`) e 6
    novos em `live.rs`.
  - É o mesmo padrão que o W2 do render-engine pediu para comentar, e o todo
    `2026-09-30-render-engine-review-followups.md` só cita `live.rs`/`theme.rs`.
  - `bezel-media/src/process.rs:103-107` tem a razão na doc, então está OK.
  - **Ação:** incluir `storage.rs` no todo ou pôr `// reason:`.
- **W7. T-6.8 com documentação pendente.**
  - `protocol-turing-rev-c.md` § 19 não tem as observações desta fase: `file_rev_done`, `full_png_sucess` após
    PLAY_VIDEO, restos após transferência abortada, HELLO mudo após cancelamento e o flush interrompido por sinal.
  - O `CHANGELOG.md` não fala da aba de armazenamento do studio (`af98ba2`).
  - O Status do `README.md:9-11` ainda diz "being validated".
  - O SUMMARY não diz se o teste no hardware usou `scripts/install-local.sh`.
- **W8. Processo.**
  - O SUMMARY tem 14.186 caracteres, acima do orçamento de 8.192 (`.jdi/config.json`). O PLAN tem 12.557, acima de
    12.000.
  - O § Files modified do SUMMARY lista só a T-6.1.
  - `apps/bezel-studio/src-tauri/build.rs` (`3d1842f`, lista de comandos) não aparece entre os arquivos fora do
    `files_modified` da T-6.7.
  - As views geradas `.jdi/DECISIONS.md` e `.jdi/todos.md` estão velhas: faltam D-2026-09-30-storage-video-7 e os
    todos `tur-usb-size`, `revc-cancel-leftovers` e `storage-core-helpers`. Rode `npx -y jdi-cli render`.
  - A nota do PLAN ("se T-6.7 passar de um commit, dividir a fase") não foi seguida. É cosmético.

## Decisões e todos da fase (avaliação pedida)
- **D-2026-09-30-storage-video-1..7:** estão registradas e são cumpridas no código:
  - `Confirm`/`Confirmed` protegem delete, overwrite e boot;
  - LIST_DIR só roda nas quatro raízes e nunca num cartão ausente (`app/storage.rs:39-41`,
    `driver/turing_usb.rs:383-385`);
  - não há limpeza automática nem formatação;
  - ffmpeg é externo, chamado com vetor de argumentos e sem shell;
  - o vídeo só é enviado por ação explícita.

  A D-7 refina a D-1 para o TUR_USB de forma coerente. Ressalvas: W2 (D-5) e W3 (D-6 no hardware).
- **`2026-09-30-storage-video.md`:** aceitável. Casa com o "Out of scope" do CONTEXT e cada item cita a D-XX.
- **`2026-09-30-tur-usb-size.md`:** aceitável. É consequência direta da D-7, tem proposta concreta
  (`FileEntry.size: Option<u64>` ou `exists`) e fase-alvo `release-polish`. Vale saber que hoje um único arquivo
  alheio faz o `ls` inteiro falhar numa tela TUR_USB, que é só golden.
- **`2026-09-30-revc-cancel-leftovers.md` e `2026-09-30-storage-core-helpers.md`:** registro acionável, mas sem
  fase-alvo (W3, W5).

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | 499 passed, 0 failed, 6 ignored |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | comando do DoD: TOTAL 94.82% lines, `OK` (gate 3: 94.88%) |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | sem release nesta fase. O Unreleased tem `bezel storage` e `bezel run` com vídeo, mas falta a aba do studio (W7) |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | `README.md` § Screen storage and video (`:79-121`) descreve a CLI. Faltam o Status e o studio (W7) |
| 6 | Rev C storage packets and storage-info reply match the vectors | CONTEXT | Auto | PASS | `storage_packets_match_the_reference_vectors` e `storage_info_subtracts_the_reserved_flash_and_detects_the_card`: `OK` |
| 7 | Delete and overwrite refused without `Confirm::Yes`, before any byte | CONTEXT | Auto | PASS* | `domain::storage::tests::destructive_operations_require_confirm_yes`: `OK`. *Depende dos dublês do B1 |
| 8 | Rev C upload follows the vendor sequence, progress, cancel | CONTEXT | Auto | PASS | `driver::turing_rev_c::tests::upload_reports_progress_and_can_be_cancelled`: `OK` |
| 9 | Preflight rejects names, sizes, resolution, full storage; never deletes | CONTEXT | Auto | PASS | `domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage`: `OK` |
| 10 | ffmpeg args are a vector with the vendor chain; missing ffmpeg is a state | CONTEXT | Auto | PASS | `transcode::tests::builds_the_vendor_argument_vector_for_rev_c` e `probe::tests::missing_ffmpeg_is_reported_not_fatal`: `OK`. ffmpeg real também confere (4/4 ignorados) |
| 11 | Video background renders a transparent base | CONTEXT | Auto | PASS | `renderer::tests::device_video_background_renders_a_transparent_base`: `OK` |
| 12 | `bezel storage rm` without `--yes` is refused | CONTEXT | Auto | PASS | `storage::tests::rm_without_yes_is_refused`: `OK` |
| 13 | Studio storage tab: progress, cancel, confirmed delete (light and dark) | CONTEXT | Auto | PASS | `npx playwright test -g "storage tab upload progress and confirmed delete"`: `OK` (2 passed) |
| 14 | 8.8" real: info e listas, upload/verify/play/stop/delete, imagem e vídeo | CONTEXT | Manual | MANUAL_REQUIRED | a CLI já tem evidência (SUMMARY § Hardware validation: flash e cartão, PNG e MP4 nativo e convertido, recusa por espaço, cancelamento). Falta a aba de armazenamento do studio na 8.8". Evidência sugerida: "studio → Tela → Armazenamento na 8.8": enviar, tocar, parar e apagar `bezel_test_*`, com captura" |
| 15 | 8.8" real: vídeo em loop com o tema por cima (alfa) e boot após desligar e ligar | CONTEXT | Manual | MANUAL_REQUIRED | o protocolo está verificado (`run` com vídeo, `full_png_sucess`). Faltam a foto ou descrição do alfa e `bezel storage boot sd/video/bezel_test_*.mp4 --yes` + desligar e ligar, depois `boot default --yes` |

## Recommendation
Resolver o B1 pela opção (a) ou (b) e rodar `/jdi-verify storage-video` de novo. O resto do trabalho está sólido:
- os protocolos rev C e TUR_USB conferem byte a byte com as specs;
- delete, overwrite e boot só passam com `Confirmed`;
- nada de armazenamento, 0x7D, 0x81, 0x82 ou 0x84 sai implicitamente (`nothing_storage_related_is_sent_implicitly`);
- a cobertura está em 94,9%;
- a UI passa em axe, i18n e teclado.

Na mesma rodada vale tratar W1 (mover `confirm_of` para `commands.rs`) e W2 (brilho no boot do studio), e dar
fase-alvo aos todos de W3/W5. A T-6.8 fecha com a aba do studio, o alfa e o boot na 8.8" e com § 19, CHANGELOG e
README (W7). Depois disso, a expectativa é `APPROVED_PENDING_MANUAL` ou `APPROVED_WITH_WARNINGS`.
