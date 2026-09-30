# Phase 4: Motor de renderização — Context  (slug: render-engine)

## Goal
Modelo de tema, renderer WYSIWYG (texto, imagem, GIF, barras, anéis, ponteiros, gráficos, relógios), diff de frame com atualização parcial, formato nativo e importadores (YAML do Python e .turtheme do TURZX), runtime headless `bezel run`.

## Locked decisions
- D-2026-09-30-render-engine-1: modelo de tema como dado puro no core; `ThemeRuntime` no core; pixels no adapter.
- D-2026-09-30-render-engine-2: um renderer só (tiny-skia + cosmic-text + image) para preview e tela.
- D-2026-09-30-render-engine-3: `.bezeltheme` = zip com `theme.json` (DTO serde versionado) + `assets/`; pasta equivalente vale.
- D-2026-09-30-render-engine-4: importadores convertem (YAML Python, `.turtheme` via parser NRBF com whitelist).
- D-2026-09-30-render-engine-5: `bezel run` headless + `bezel render` para PNG.

## Canonical refs
- `docs/reverse-engineering/rendering.md`, `themes-python-yaml.md`, `themes-turzx.md`
- Temas TURZX instalados do usuário: `/home/slipalison/repos/turx/TURZX-V3.07-88inchENG/theme/4801920/*.turtheme` (entrada de teste local, nunca commitada)
- Temas Python: `/home/slipalison/repos/turx/turing-smart-screen-python/res/themes/*`

## Out of scope
- Vídeo tocado na tela e upload para o SD (phase `storage-video`); aqui o fundo de vídeo usa o pôster
- Editor visual (phase `studio-app`)

## Definition of Done

### Auto-verifiable
- [ ] Every element kind renders to the expected pixels in a golden test
      **Verify:** `cargo test -p bezel-render --locked -- --exact tests::every_element_kind_matches_its_golden 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] A theme survives save → load unchanged in both zip and folder form
      **Verify:** `cargo test -p bezel-themes --locked -- --exact native::tests::round_trip_zip_and_folder 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Every installed TURZX theme and every Python theme imports without error
      **Verify:** `cargo test -p bezel-themes --locked --test import_corpus -- --ignored --exact imports_the_local_corpus 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] `bezel render` writes a PNG of the canvas size through the fake sensors
      **Verify:** `cargo test -p bezel --locked --test render -- --exact render_writes_a_png_of_the_canvas_size 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] `bezel run` shows a bundled theme with live sensors on the real 8.8" for a minute without errors
      **Verify:** human confirmation required
      **Evidence:** SUMMARY.md § Hardware validation (command, duration, frames, errors)
      **Source:** CONTEXT

## Notes
- Crates novos: `bezel-render`, `bezel-themes`. Temas embutidos próprios (sem assets de terceiros) em `themes/`.
