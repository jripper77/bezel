# Phase 9: Vídeo de fundo que toca e se enquadra na tela — Plan  (slug: video-background-framing)

## Goal
O vídeo de fundo toca na prévia e na tela, vídeo já girado para o painel é reconhecido e o usuário enquadra o vídeo (girar, preencher/caber, zoom, posição).

## Locked decisions (from CONTEXT.md)
- D-2026-10-01-video-background-framing-1..6; herdadas: D-1 (hexagonal), storage-video-2/-4, release-polish-12, storage-manager-9

## Tasks

### Wave 1 (paralela)

#### T-1: Core: `domain::framing`, `framing` no fundo de vídeo, nomes e tipos
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{framing,mod,media,poster,theme,cleanup}.rs`, `crates/bezel-core/src/{ports/mod.rs,app/runtime.rs}`, `crates/bezel-core/tests/runtime_video.rs`; só mecânico (`framing: None`, `..`, literais): `crates/bezel-{themes,render,media,cli}/**`, `apps/bezel-studio/src-tauri/src/**`
- **Acceptance:**
  - `VideoFraming` (rotação 0..3 ou `None` = Auto, `Cover|Contain`, zoom 1–4, posição 0–1, pad preto) em `Background::Video { framing: Option<_> }` (`None` = padrão). `resolve`: tamanho sondado = painel nativo e tema a ¼ de volta ímpar → voltas que cancelam tema→painel (Dragon Ball: 270°, 0 no total); outro tamanho, desconhecido ou meia volta → 0. Painel = o da tela; sem tela, o único modelo do catálogo com o painel do canvas (480x1920 → 8.8"), senão Auto = 0.
  - `geometry(fonte, resolvido, alvo)` pura: voltas, recorte em pixels da fonte com bordas pares, tamanho escalado, deslocamento do pad; `object-position`, cover sem borda vazia; padrão = `cover_crop`. `fitting_options`, `cover_crop`, `PosterSpec::for_canvas` mantêm assinatura e resultado; `ConvertOptions`/`TranscodeTarget` ganham o pad (`None` = cadeia de hoje), `PosterSpec` a geometria; `StreamSpec` pede a fonte crua, lado maior ≤ 2× o do canvas.
  - `device_video_name` recebe o resolvido: padrão = nomes do fornecedor pelas voltas totais (`dragon.mp4`, `amd_90.mp4`); outro = `_f` + 8 hex FNV-1a do canônico (fit, zoom×100, posição×1000, pad se aparece). `Protected::theme_video` cobre também `<nome>_f<8 hex>`.
- **Dependencies:** none
- **Test:** `domain::framing::tests::{a_panel_native_video_in_a_turned_theme_counts_as_turned,geometry_covers_contains_zooms_and_positions_on_even_edges}`, `domain::media::tests::device_video_names_carry_the_framing`, `app::runtime::tests::device_video_names_follow_the_vendor` (esperado de hoje)
- **Status:** completed (41215c3)

#### T-2: Studio UI (demo): Enquadramento no inspetor e no canvas, vídeo que toca
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/{app,bridge,demo-backend,demo-data,demo-theme,demo-render}.js`, `.../src/{index.html,styles.css}`, `apps/bezel-studio/src/editor/{video-framing,background}.js`, `apps/bezel-studio/src/ui/{inspector,canvas}.js`, `apps/bezel-studio/src/i18n/{en,pt-BR}.js`, `apps/bezel-studio/tests/ui/{video-framing,demo-backend,bridge}.test.mjs`, `apps/bezel-studio/tests/e2e/video-framing.spec.mjs`
- **Acceptance:**
  - Grupo Enquadramento e "Enquadrar no canvas" exatamente como a D-6 (Auto com o ângulo detectado, faixas, passos, teclas, `aria-pressed`, aria-live, elementos não selecionáveis no modo); a imagem segue o ponteiro; cada controle e cada arrasto/rajada de roda = 1 passo pelos gestos do store.
  - Lógica pura em `editor/video-framing.js` (≥ 80% linhas): padrões, limites, `framing` omitido quando padrão, pan/zoom; adicionar vídeo não grava `framing`; i18n com paridade, nenhum literal no JS.
  - Contrato da T-6 (JSDoc no `bridge.js`): `background.framing` = JSON da D-2; `render(theme, {motion})` → `render_preview {theme, motion}` (`false` = pôster sem decodificador); `nextMs` = próxima imagem (≤ 15 fps); `videoAuto(theme)` → `video_auto` = `{rotation, size}`. Demo: tema paisagem, vídeo 480x1920 já girado, decodificador simulado; sem ffmpeg = pôster + dica com link do guia.
- **Dependencies:** none
- **Test:** `node --test tests/ui/i18n.test.mjs tests/ui/video-framing.test.mjs`; `video-framing.spec.mjs` com `expectAccessible`: ≥ 3 testes "video framing …"/"video background plays …" (Auto, controles, canvas, desfazer/refazer, toca e para com movimento reduzido) × 4 projetos; `npm test`
- **Status:** pending

### Wave 2 (paralela)

#### T-3: bezel-media: cadeias ffmpeg da geometria, prévia crua, pôster enquadrado
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-media/src/{lib,framing,transcode,poster,stream}.rs`
- **Acceptance:**
  - `framing::filter_chain` pura na ordem do fornecedor (voltas, recorte, `scale`, `pad` com a cor, `setsar`), usada por conversão e pôster; padrão numa fonte de outra forma = cadeia de hoje (teste do fornecedor sem mudar o esperado).
  - `stream`: fonte crua `scale=W:H` sem recorte nem voltas, `fps` ≤ 15, `-threads 2`, rawvideo RGBA; testes de argumentos e `Path` (Windows ok).
- **Dependencies:** T-1
- **Test:** `framing::tests::filter_chains_follow_the_geometry`, `stream::tests::preview_decoding_is_raw_at_most_15_fps`, `transcode::tests::builds_the_vendor_argument_vector_for_rev_c`
- **Status:** completed (46d32a7)

#### T-4: `.bezeltheme` com `framing`; TURZX em Auto
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-themes/src/{dto,native}.rs`, `crates/bezel-themes/src/import/{turzx.rs,turzx/tests.rs}`, `apps/bezel-studio/src-tauri/src/video.rs` (literal de teste)
- **Acceptance:**
  - `framing` opcional no fundo de vídeo, schema 1 (`rotation` 0/90/180/270, `fit`, `zoom`, `position {x,y}`, `padColor`): omitido quando padrão, chave ausente = padrão, números fora da faixa limitados, outra rotação = `DtoError`; ida e volta exata.
  - Tema antigo e importação TURZX = `framing: None` (Auto).
- **Dependencies:** T-1
- **Test:** `native::tests::video_framing_round_trips_and_older_themes_load_as_auto`, `import::turzx::tests::vendor_video_backgrounds_import_with_auto_framing`
- **Status:** completed (4e82fab)

#### T-5: Runtime e `bezel run` respeitam o enquadramento
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/app/runtime.rs`, `crates/bezel-core/src/domain/{framing.rs,framing/**}`, `crates/bezel-core/tests/runtime_video.rs`, `crates/bezel-cli/src/{live.rs,live/**}`
- **Acceptance:**
  - A runtime recebe o `MediaInfo` sondado por método aditivo (`start_video` mantém a assinatura; sem ele, Auto = 0). Identidade (0 voltas totais, cover, zoom 1) num arquivo no perfil: o nome padrão guardado vale só com `GET_FILE_SIZE` = bytes do asset, senão `VideoMissing` no mesmo caminho; nome convertido mantém a checagem de presença; `MissingVideo.options` vem da geometria do painel (identidade = envia como está).
  - Host: `StreamSpec` cru, cada imagem enquadrada por função pura do core (mesma geometria), que a prévia da runtime também aceita (studio); mudar o enquadramento não reabre o decodificador. Testes sobre fakes dos adapters; nenhuma impl de porta no core.
  - `bezel run` sonda o vídeo (cabeçalho MP4, sem ffmpeg) e o passa à runtime; a dica do `put` nomeia o arquivo procurado (identidade sem `--orientation`; outro enquadramento: "Enviar para a tela" do studio).
- **Dependencies:** T-1
- **Test:** `--test runtime_video`: `the_stored_dragon_ball_video_loops_without_an_upload`, `a_stored_file_of_another_size_is_not_reused`, `a_reframed_video_is_looked_for_under_its_own_name`, `the_host_decodes_the_video_with_its_framing`; CLI `live::tests::run_honours_the_video_framing`
- **Status:** pending

### Wave 3 (paralela)

#### T-6: Studio backend: prévia que toca, pôster enquadrado, envio como está
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/src/{studio,video,storage}{.rs,/**}`, `.../src/{backend,commands,dto,messages,lib}.rs`, `apps/bezel-studio/src-tauri/{build.rs,capabilities/default.json}`, `apps/bezel-studio/tests/ui/fixtures/backend-codes.json`, `apps/bezel-studio/src/i18n/{en,pt-BR}.js` (só códigos novos)
- **Acceptance:**
  - Contrato da T-2 (`video_auto` com `allow-*`). ≤ 1 decodificador de prévia por sessão (`stream` cru, ≤ 15 fps), enquadrado pela runtime; fecha após 2 s sem pedido (relógio injetado), volta pelo relógio; `motion=false` = pôster sem decodificador; trocar o vídeo reinicia, editar o enquadramento não. Sem ffmpeg: pôster; Auto segue.
  - Pôster com o enquadramento ao adicionar; refeito ao salvar se o enquadramento mudou (sem ffmpeg, fica). "Enviar para a tela" usa `MissingVideo.options`: vídeo nativo do painel em tema girado vai como está (teto de 25 MiB igual); outro enquadramento sem ffmpeg é recusado com as dicas.
- **Dependencies:** T-2, T-3, T-4, T-5
- **Test:** `studio::tests::{the_preview_plays_the_framed_video_at_most_15_fps,the_preview_decoder_stops_when_no_frame_is_asked,without_ffmpeg_the_preview_shows_the_poster,saving_retakes_the_poster_with_the_framing}`, `storage::tests::a_panel_native_theme_video_is_sent_as_it_is`; `npm test`
- **Status:** pending

#### T-7: Guia en/pt-BR e CHANGELOG
- **Specialist:** jdi-doer-bezel
- **Files modified:** `docs/user/storage-and-video.md`, `docs/user/pt-BR/storage-and-video.md`, `scripts/ci/check-docs.sh`, `CHANGELOG.md`
- **Acceptance:** `### Framing the video` / `### Enquadrar o vídeo` sob o vídeo de fundo: Auto, controles e teclas, cópia `_f…` (nada apagado), prévia e movimento reduzido, sem ffmpeg, CLI só respeita; `check-docs.sh` exige os títulos; CHANGELOG `[Unreleased]` cita framing.
- **Dependencies:** T-1, T-2
- **Test:** DoD 7
- **Status:** pending

### Wave 4

#### T-8: HARDWARE — Dragon Ball ao vivo na 8.8" real (orquestrador)
- **Specialist:** orquestrador na Turing 8.8" real
- **Files modified:** `.jdi/phases/video-background-framing/SUMMARY.md` (§ Hardware validation)
- **Acceptance:**
  - `scripts/install-local.sh`; liberar a tela combinado com o usuário (studio dele fora do ao vivo, serviço parado e religado no fim); `bezel storage ls` antes e depois iguais.
  - `bezel run` com o "Dragon Ball" da biblioteca acha o `dragon.mp4` guardado (`GET_FILE_SIZE` = bytes do asset), sem upload; o vídeo inteiro (10,2 s em loop) toca em pé sob o tema. SUMMARY e corpo do PR (Deferred to PR review); 2 itens visuais com o usuário.
- **Dependencies:** T-6, T-7
- **Test:** item 1 de "Deferred to PR review"
- **Status:** pending

## Execution
- 8 tasks em 4 waves (2 → 3 → 2 → 1); 1 wave por iteração do `/jdi-loop`

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| 1 Core: Auto, geometria, nomes | T-1 |
| 2 Runtime: `dragon.mp4`, tamanho, re-enquadrado, PC | T-5 |
| 3 Filtros, prévia crua, cadeia do fornecedor | T-3 |
| 4 `.bezeltheme` e TURZX em Auto | T-4 |
| 5 Studio: prévia, pôster, nativo como está | T-6 |
| 6 UI: i18n, lógica, Playwright ≥ 12 com axe | T-2 |
| 7 Guia e CHANGELOG | T-7 |
| 8 CI Windows no HEAD | todas (clippy Windows); orquestrador após o push |
| Deferred: Dragon Ball na 8.8" | T-8 |
| PROJECT: testes, cobertura, TODO | todas |

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`
- Windows (DoD 8): `cargo clippy --workspace --exclude bezel-studio --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings` por task Rust; após o push, `rust-windows` verde no HEAD do PR
- `cd apps/bezel-studio && npm test`; `bash scripts/ci/check-docs.sh`; `cargo llvm-cov --workspace --locked --fail-under-lines 80`
- `scripts/install-local.sh` a cada task

## Notes
- Donos únicos: W1 T-1 = todo `.rs` (`domain/mod.rs`, `ports/mod.rs`), T-2 = JS (`bridge.js`, `demo-backend.js`, i18n); W2 T-3 `bezel-media`, T-4 `bezel-themes`, T-5 core `app`/`domain/framing` + CLI; W3 T-6 studio Rust, i18n (só códigos), `lib.rs`/`build.rs`/`capabilities`; T-7 `docs/user/**`, `CHANGELOG.md`. Nenhuma crate nova (`Cargo.lock` intacto); cada task compila o workspace sozinha.
- Tipos do core fixos após a T-1; nenhuma variante nova em `BezelError`/`Refusal`/`VideoState`/`Backdrop`. Nomes de teste do DoD exatos (`--exact`), sem módulo a mais no caminho.
