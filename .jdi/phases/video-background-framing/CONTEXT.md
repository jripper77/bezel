# Phase 9: Vídeo de fundo que toca e se enquadra na tela — Context  (slug: video-background-framing)

## Goal
O vídeo de fundo toca na prévia e na tela, vídeo já girado para o painel é reconhecido e o usuário enquadra o vídeo (girar, preencher/caber, zoom, posição).

## Locked decisions
- D-1: fase do relato de 2026-10-01 (Dragon Ball parado e 4x maior que a tela).
- D-2: `framing` opcional no fundo de vídeo (schema 1): `rotation` 0/90/180/270 ou ausente = Auto, `fit` cover|contain, `zoom` 1–4, `position` {x,y} 0–1, `padColor`; omitido no padrão. Auto: tamanho nativo do painel em tema a ¼ de volta = já girado (Dragon Ball 270°). Importar/adicionar não gravam `framing`.
- D-3: geometria pura no core; conversão e pôster por cadeia ffmpeg pura (padrão = a de hoje); prévia/PC: ffmpeg decodifica a fonte crua e o Rust enquadra. Identidade = envia como está. Pôster refeito ao salvar.
- D-4: `device_video_name` com enquadramento: padrão = `dragon.mp4`/`amd_90.mp4`, outro soma `_f`+8 hex. Reuso só com mesmo nome e tamanho; teto de 25 MiB igual; nada apagado sozinho.
- D-5: prévia ≤ 15 fps via `nextMs`, 1 ffmpeg, fecha após 2 s sem pedido; oculta/movimento reduzido não decodifica. Sem ffmpeg: pôster + dica; Auto e envio da identidade funcionam.
- D-6: Enquadramento no inspetor e no canvas (arrastar, roda, teclado), 1 desfazer por gesto, aria-live, pt-BR/en; CLI só respeita.

## Canonical refs
- Card: relato do usuário de 2026-10-01 (colado), `.jdi/decisions/D-2026-10-01-video-background-framing-{1..6}.md`
- `docs/reverse-engineering/video.md` § 3–4, `protocol-turing-rev-c.md` § 13.5, `themes-turzx.md` § 5.6–5.7
- `domain/{media,poster}.rs`, `app/runtime.rs`, bezel-media `{transcode,stream,poster}.rs`, `dto.rs`, studio `preview-animation.js`, `inspector.js`

## Out of scope
- Framing pela CLI, cortar/animar vídeo, reconverter nativo acima do teto, limpar cópias antigas: `.jdi/todos/2026-10-01-video-background-framing.md`.

## Definition of Done

### Auto-verifiable
- [ ] Core: Auto, geometria e nomes (os do fornecedor seguem)
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::framing::tests::a_panel_native_video_in_a_turned_theme_counts_as_turned domain::framing::tests::geometry_covers_contains_zooms_and_positions_on_even_edges domain::media::tests::device_video_names_carry_the_framing app::runtime::tests::device_video_names_follow_the_vendor 2>&1 | grep -q 'ok. 4 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Runtime: `dragon.mp4` guardado toca sem envio; outro tamanho não; re-enquadrado tem nome próprio; PC enquadra
      **Verify:** `cargo test -p bezel-core --locked --test runtime_video -- --exact the_stored_dragon_ball_video_loops_without_an_upload a_stored_file_of_another_size_is_not_reused a_reframed_video_is_looked_for_under_its_own_name the_host_decodes_the_video_with_its_framing 2>&1 | grep -q 'ok. 4 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Filtros ffmpeg puros; prévia crua ≤ 15 fps; cadeia do fornecedor intacta
      **Verify:** `cargo test -p bezel-media --locked --lib -- --exact framing::tests::filter_chains_follow_the_geometry stream::tests::preview_decoding_is_raw_at_most_15_fps transcode::tests::builds_the_vendor_argument_vector_for_rev_c 2>&1 | grep -q 'ok. 3 passed' && echo OK`
      **Source:** CONTEXT
- [ ] `.bezeltheme`: `framing` ida e volta; temas antigos e TURZX carregam em Auto
      **Verify:** `cargo test -p bezel-themes --locked --lib -- --exact native::tests::video_framing_round_trips_and_older_themes_load_as_auto import::turzx::tests::vendor_video_backgrounds_import_with_auto_framing 2>&1 | grep -q 'ok. 2 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Studio: prévia ≤ 15 fps, para sem pedidos, pôster sem ffmpeg e ao salvar; nativo vai como está
      **Verify:** `cargo test -p bezel-studio --locked --lib -- --exact studio::tests::the_preview_plays_the_framed_video_at_most_15_fps studio::tests::the_preview_decoder_stops_when_no_frame_is_asked studio::tests::without_ffmpeg_the_preview_shows_the_poster studio::tests::saving_retakes_the_poster_with_the_framing storage::tests::a_panel_native_theme_video_is_sent_as_it_is backend::tests::the_session_goes_on_while_the_poster_is_taken backend::tests::the_session_goes_on_while_the_video_is_probed 2>&1 | grep -q 'ok. 7 passed' && echo OK`
      **Source:** CONTEXT
- [ ] UI: i18n, lógica do enquadramento e Playwright nos 4 projetos com axe
      **Verify:** `set -o pipefail; cd apps/bezel-studio && grep -qE 'await expectAccessible\(' tests/e2e/video-framing.spec.mjs && node --test tests/ui/i18n.test.mjs tests/ui/video-framing.test.mjs >/dev/null && npx playwright test -g "video framing|video background plays" --reporter=line 2>&1 | awk '{for(i=1;i<NF;i++) if($(i+1)=="passed") n=$i} END{exit !(n>=16)}' && echo OK`
      **Source:** CONTEXT
- [ ] Guia en/pt-BR e CHANGELOG
      **Verify:** `grep -q '^### Framing the video' docs/user/storage-and-video.md && grep -q '^### Enquadrar o vídeo' docs/user/pt-BR/storage-and-video.md && sed -n '/^## \[Unreleased\]/,/^## \[[0-9]/p' CHANGELOG.md | grep -qi framing && bash scripts/ci/check-docs.sh >/dev/null && echo OK`
      **Source:** CONTEXT
- [ ] CI do Windows verde no HEAD do PR
      **Verify:** `s=$(git rev-parse HEAD); i=$(gh run list -w CI -b jdi/video-background-framing -L 20 --json databaseId,headSha -q "map(select(.headSha==\"$s\"))[0].databaseId"); gh run view "$i" --json jobs -q '.jobs[]|select(.name|endswith("rust-windows")).conclusion' | grep -qx success && echo OK`
      **Source:** CONTEXT

### Manual
- _(none)_

## Deferred to PR review
- 8.8" real, ao vivo: "Dragon Ball" toca o vídeo inteiro (10,2 s em loop), em pé, sob o tema, com o `dragon.mp4` já guardado (orquestrador valida; SUMMARY).
- Visual: prévia do Dragon Ball em pé, no tamanho do canvas; arrastar e zoom naturais.
- Vídeo com Caber + zoom enviado à 8.8" bate com a prévia.

## Notes
- Ordem: core → media → themes → runtime/CLI → studio → docs; install-local ao fim.
