# Phase 6: Armazenamento e vídeo — Plan  (slug: storage-video)

## Goal
Cartão SD/armazenamento da tela (listar, enviar, apagar, espaço), fundo em vídeo tocado na tela com tema sobreposto, imagem de boot, transcodificação ffmpeg e vídeo/GIF transmitido pelo PC.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-storage-video-1..6; herdadas: D-1 (hexagonal), D-2026-09-30-device-protocols-2 (nada implícito), -5 (golden)

## Tasks

### Wave 1 (núcleo: todas as outras dependem dele)

#### T-6.1: Domínio, portas e casos de uso de armazenamento, jobs e mídia
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{storage,job,media,mod,error}.rs`, `crates/bezel-core/src/ports/mod.rs`, `crates/bezel-core/src/app/{mod,storage,runtime}.rs`; só o campo novo de `RenderContext` em `crates/bezel-render/src/testkit.rs`, `crates/bezel-render/tests/support/showcase.rs`, `crates/bezel-cli/src/screen.rs`, `apps/bezel-studio/src-tauri/src/studio.rs`
- **Acceptance:**
  - Porta `ScreenStorage` (info, list, size, upload, delete, play_video, play_image, stop, set_start_mode) via `ScreenLink::storage() -> Option<&mut dyn ScreenStorage>` com default `None`: famílias sem armazenamento não mudam e o caso de uso devolve `BezelError::Unsupported`; erros novos `Unsupported`, `Cancelled`, `NotConfirmed`.
  - `StorageLocation` só representa as quatro raízes (interna/SD × img/video); nomes normalizados para `[a-z0-9_.-]`, caminho ≤ 236 bytes; `preflight` recusa com motivo nome inválido, > 120 MB ou ≥ 2 GiB, perfil errado (rev C: MP4 H.264 yuv420p sem áudio na resolução nativa, 480x1920 na 8.8"; imagens jpg/jpeg/bmp/png/gif) e falta de espaço (0x64 menos 512 KiB); quando não cabe, devolve a lista de candidatos a apagar com confirmação e nunca apaga nada.
  - `Confirm` na assinatura de delete, upload sobre arquivo existente e boot; `Confirm::No` recusa antes de qualquer chamada à porta (fake gravador com zero chamadas). `app::storage`: upload = preflight → converter (se preciso) → enviar → verificar; boot = tocar + startMode sob um único `Confirm`.
  - Jobs: `Progress { phase: Convert|Upload|Verify, done, total }` e `CancelToken` (sem thread nem I/O no core); porta `MediaTranscoder` (disponibilidade, probe, transcode, stream de frames); `RenderContext.backdrop` (`Poster` | `OnDevice` | `Frame(&Frame)`), construtores atuais com `Poster` e nenhum pixel muda.
- **Dependencies:** none
- **Test:** `domain::storage::tests::destructive_operations_require_confirm_yes`, `domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage`; `cargo test -p bezel-core`
- **Status:** completed (5e36061)

### Wave 2 (paralela: adapters, arquivos disjuntos)

#### T-6.2: Rev C: comandos de armazenamento, upload com progresso, boot e fake
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-devices/src/protocol/turing_rev_c.rs`, `crates/bezel-devices/src/driver/{turing_rev_c,mod}.rs`, `crates/bezel-devices/src/fake.rs`
- **Acceptance:**
  - Pacotes 0x64/0x65/0x66/0x6E/0x6F (BE32 do caminho, LE32 do tamanho)/0x78 (loop)/0x8C iguais aos vetores do § 17.2; `a-b-c-d-e-f` desconta 512 KiB da flash e detecta o cartão (TF total > 1024 KiB); respostas `file:`, `nodir-createdone`, `create_success`, `file_rev_done`, `play_video_success`, `play_img_ok`.
  - `ScreenStorage` na sequência do § 13.4 (STOP_VIDEO → STOP_MEDIA → LIST_DIR → 0x6F → blocos 249+1 → `file_rev_done` → GET_FILE_SIZE), progresso por bloco; cancelar entre blocos refaz HELLO, mede com GET_FILE_SIZE e devolve `Cancelled` com o tamanho parcial; timeouts e tentativas em constantes nomeadas (sem literal repetido).
  - Boot reescreve 0x7D com o último brilho/sleep enviados, mudando só o startMode; `present` mantém o alfa por pixel (A=0 mostra o vídeo) em BGRA e no formato de 3 bytes; nenhum comando de armazenamento, 0x7D, 0x82 ou 0x84 sai implicitamente (teste). `FakeConnector` ganha armazenamento em memória da 8.8" para CLI e studio.
- **Dependencies:** T-6.1
- **Test:** `protocol::turing_rev_c::tests::storage_packets_match_the_reference_vectors`, `protocol::turing_rev_c::tests::storage_info_subtracts_the_reserved_flash_and_detects_the_card`, `driver::turing_rev_c::tests::upload_reports_progress_and_can_be_cancelled`
- **Status:** pending

#### T-6.3: Crate `bezel-media` (ffmpeg/ffprobe externos)
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-media/{Cargo.toml,src/lib.rs,src/probe.rs,src/transcode.rs,src/mp4.rs,src/stream.rs}`, `Cargo.toml` (membro + `workspace.dependencies`), `Cargo.lock`, `crates/bezel-cli/Cargo.toml` e `apps/bezel-studio/src-tauri/Cargo.toml` (só a dependência, para a wave 3 não tocar o lock)
- **Acceptance:**
  - `probe`: caminho configurado, depois PATH; ausente = estado `Missing` com dicas dnf/apt/winget, nunca pânico; nada embutido.
  - `transcode`: argumentos em vetor (sem shell) com a cadeia do fornecedor (rotação/crop + `scale=W:H,setsar=1:1`, `libx264 -crf 20 -an -pix_fmt yuv420p -f mp4`, `-r 24` opcional) na resolução nativa; perfil TUR_USB Annex-B `bframes=0` com `eq` opcional desligado; progresso por `-progress pipe:1`; cancelar mata o filho e apaga a saída parcial.
  - `mp4`: parser mínimo de caixas decide se o MP4 já está no perfil (sobe sem converter); `stream`: pipe rawvideo RGBA → `Frame`s em loop, limitado ao fps pedido. Depende só de `bezel-core` (adapter não chama adapter); processos testados com um executável falso, sem exigir ffmpeg no CI.
- **Dependencies:** T-6.1
- **Test:** `transcode::tests::builds_the_vendor_argument_vector_for_rev_c`, `probe::tests::missing_ffmpeg_is_reported_not_fatal`; `cargo test -p bezel-media`
- **Status:** pending

#### T-6.4: Fundo em vídeo: base transparente e runtime
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-render/src/{renderer,golden,testkit}.rs`, `crates/bezel-core/src/app/runtime.rs`
- **Acceptance:**
  - `Background::Video` + `Backdrop::OnDevice` → base transparente (A=0 fora dos elementos, alfa reto neles); `Backdrop::Frame` desenha o frame do PC como fundo (Cover); `Poster` mantém os golden atuais.
  - `ThemeRuntime`: tela com reprodução e o arquivo presente (GET_FILE_SIZE nas raízes, sem LIST_DIR nem upload) → PLAY_VIDEO loop=1 e `OnDevice`; ausente → pôster + estado `VideoMissing` (chamada "Enviar para a tela"); tela sem reprodução (WCH) → frames do `MediaTranscoder` ou pôster sem ffmpeg.
- **Dependencies:** T-6.1
- **Test:** `renderer::tests::device_video_background_renders_a_transparent_base`; `cargo test -p bezel-core -p bezel-render`
- **Status:** completed (0756ce4; `crates/bezel-core/src/app/mod.rs` também mudou: só a reexportação dos tipos novos do runtime)

### Wave 3 (paralela: entradas e TUR_USB)

#### T-6.5: TUR_USB: armazenamento só com vetores golden
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-devices/src/{protocol,driver}/turing_usb.rs`
- **Acceptance:** `ScreenStorage` com os comandos 100/99/38/39/110/113 (nunca 40/98), info em LE32 KiB, raízes TF do § 6; vetores golden; `hardware_validated=false`; boot logo `/usr/data/boot.jpg` (JPEG q95 ≤ 307200 B) só como vetor, sem comando exposto; reusa os auxiliares do `driver/mod.rs` (T-6.2), sem duplicar
- **Dependencies:** T-6.2
- **Test:** `cargo test -p bezel-devices --lib turing_usb`
- **Status:** pending

#### T-6.6: CLI `bezel storage`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-cli/src/{storage,lib,main,live}.rs`, `crates/bezel-cli/tests/storage.rs`
- **Acceptance:**
  - `bezel storage info|ls|put|rm|play|stop|boot` sobre `app::storage`; `--yes` = `Confirm::Yes`; sem `--yes`, `rm`, `put` sobre existente e `boot` recusam (código ≠ 0) sem enviar nada à tela.
  - `put`: barra de progresso (convert/upload/verify) em stderr; Ctrl+C cancela e sugere apagar o parcial com `rm --yes`; `--ffmpeg PATH` antes do PATH; sem ffmpeg, aviso com dica por SO e só sobe o que já está no perfil.
  - `bezel run` com tema de vídeo segue o `ThemeRuntime` (na tela / pôster com aviso / stream do PC).
- **Dependencies:** T-6.2, T-6.3, T-6.4
- **Test:** `storage::tests::rm_without_yes_is_refused`; `tests/storage.rs` com `--fake`
- **Status:** pending

#### T-6.7: Studio: aba de armazenamento no painel "Tela"
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/src/{storage,backend,commands,lib,dto,settings}.rs`, `apps/bezel-studio/src-tauri/capabilities/default.json`, `apps/bezel-studio/src/ui/{storage,library}.js`, `apps/bezel-studio/src/{index.html,styles.css,app.js,bridge.js,demo-backend.js,demo-data.js}`, `apps/bezel-studio/src/i18n/{en,pt-BR}.js`, `apps/bezel-studio/tests/e2e/storage.spec.mjs`, `apps/bezel-studio/tests/ui/storage.test.mjs`
- **Acceptance:**
  - Backend: info, lista, upload, cancelar, apagar, play/stop, boot e "Enviar para a tela" do vídeo do tema (resumo no `Confirm`: arquivo, raiz, tamanho, sobrescrita); progresso por evento Tauri; com Live ligado usa o link do Live pausando os frames; caminho do ffmpeg nas configurações (botão Localizar).
  - UI: barras de uso da flash e do SD, lista por local, arrastar arquivo para um slot, Play/Stop; Apagar e Boot atrás de diálogo que nomeia o arquivo; barra com Cancelar; ffmpeg ausente e vídeo ausente no Live explicados em linha; SD explica FAT32/MBR, sem formatar; textos em pt-BR e en.
  - Modo demo emula armazenamento e progresso; Playwright em claro e escuro com axe sem violações sérias/críticas.
- **Dependencies:** T-6.2, T-6.3, T-6.4
- **Test:** Playwright "storage tab upload progress and confirmed delete"; `npm run test:unit`; `cargo test -p bezel-studio`
- **Status:** pending

### Wave 4

#### T-6.8: HARDWARE — validação na 8.8" real (orquestrador) e documentação
- **Specialist:** orquestrador na Turing 8.8" real (docs: jdi-doer-bezel)
- **Files modified:** `.jdi/phases/storage-video/SUMMARY.md` (§ Hardware validation), `docs/reverse-engineering/protocol-turing-rev-c.md` (§ 19), `CHANGELOG.md`, `README.md`
- **Acceptance:**
  - `scripts/install-local.sh`; `turing-smart-screen.service` parado no teste e religado no fim; só arquivos pequenos criados pelo Bezel (`bezel_test_*`) são apagados.
  - `bezel storage info|ls|put|play|stop|rm --yes` na flash e no SD (se houver): upload/verify/play/stop/delete de imagem e vídeo; a aba de armazenamento do studio.
  - Vídeo em loop tocado pela tela com o tema ao vivo por cima (alfa correto) e slot de boot confirmado após desligar/ligar; SUMMARY separa o verificado por protocolo do que depende de confirmação visual humana.
- **Dependencies:** T-6.5, T-6.6, T-6.7
- **Test:** DoD manual (evidência em SUMMARY.md § Hardware validation)
- **Status:** pending

## Execution
- Total tasks: 8
- Waves: 4 (1 → 3 → 3 → 1)
- Estimated parallel speedup: 2x

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| vetores de armazenamento rev C + storage-info | T-6.2 |
| delete/sobrescrita exigem `Confirm::Yes`; preflight | T-6.1 |
| upload rev C com progresso e cancelamento | T-6.2 |
| argumentos do ffmpeg; ffmpeg ausente não é fatal | T-6.3 |
| base transparente para vídeo tocado na tela | T-6.4 |
| `bezel storage rm` sem `--yes` recusado | T-6.6 |
| aba de armazenamento do studio (Playwright) | T-6.7 |
| manuais na 8.8" | T-6.8 |

## Files modified (all tasks)
- `crates/bezel-{core,devices,render,cli}/**`, `crates/bezel-media/**` (novo), `Cargo.toml`, `Cargo.lock`
- `apps/bezel-studio/{src,src-tauri,tests}/**`; docs, `CHANGELOG.md`, `README.md`, SUMMARY.md

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`
- `cd apps/bezel-studio && npm run test:unit && npx playwright test`
- Coverage ≥ 80% de linhas (`cargo llvm-cov`), inclusive `bezel-media`

## Notes
- Só T-6.3 mexe no `Cargo.toml` raiz e no `Cargo.lock`; outra task que precisar de dependência nova depende de T-6.3.
- Se T-6.7 passar de um commit, dividir em backend e UI; a fase então vira duas (storage e video), como o CONTEXT prevê.
