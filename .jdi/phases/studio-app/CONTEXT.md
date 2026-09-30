# Phase 5: Bezel Studio — Context  (slug: studio-app)

## Goal
App Tauri de janela única: dispositivos, preview ao vivo, editor drag-and-drop (paleta, camadas, inspetor, sensores arrastáveis, desfazer/refazer, guias), biblioteca de mídia, bandeja e autostart.

## Locked decisions
- D-2026-09-30-studio-app-1: uma janela, três colunas (biblioteca com abas | canvas | inspetor), barra superior e de status.
- D-2026-09-30-studio-app-2: WYSIWYG com o frame do renderer Rust; DOM só para seleção/alças/guias; ≤ 30 renders/s no arraste.
- D-2026-09-30-studio-app-3: manipulação direta com pointer events, snapping 6 px, teclado, desfazer/refazer, multi-seleção, alinhar/distribuir.
- D-2026-09-30-studio-app-4: "Ao vivo" roda o mesmo `ThemeRuntime` do `bezel run` numa thread; bandeja mantém a tela rodando; autostart.
- D-2026-09-30-studio-app-5: `has_frontend=true`; Gate 7 roda a suíte Playwright + axe do app.

## Canonical refs
- `docs/reverse-engineering/ui-inventory.md` (tudo que TURZX e Python oferecem + crítica de UX)
- Formato do tema: `crates/bezel-themes/src/dto.rs` (o JSON que a UI edita)

## Out of scope
- Cartão SD, vídeo tocado na tela, imagem de boot (phase `storage-video`)

## Definition of Done

### Auto-verifiable
- [ ] Dragging a widget from the library onto the canvas creates it and selects it in the inspector
      **Verify:** `cd apps/bezel-studio && npx playwright test -g "drag a widget onto the canvas" --reporter=line 2>&1 | grep -qE '[0-9]+ passed' && echo OK`
      **Source:** CONTEXT
- [ ] Moving an element with the keyboard and undoing restores its position
      **Verify:** `cd apps/bezel-studio && npx playwright test -g "keyboard move and undo" --reporter=line 2>&1 | grep -qE '[0-9]+ passed' && echo OK`
      **Source:** CONTEXT
- [ ] Editor store commands (add, move, resize, snap, reorder, undo, redo) are covered ≥ 80%
      **Verify:** `cd apps/bezel-studio && npm run test:unit 2>&1 | grep -E 'editor/store.js' | awk -F'|' '{ exit ($2+0 >= 80) ? 0 : 1 }' && echo OK`
      **Source:** CONTEXT
- [ ] `render_preview` returns an RGBA frame of the canvas size
      **Verify:** `cargo test -p bezel-studio --locked -- --exact commands::tests::render_preview_returns_the_canvas_size 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] Designing a theme in the studio shows it live on the real 8.8", and it keeps running from the tray after the window closes
      **Verify:** human confirmation required
      **Evidence:** screenshot of the studio + photo/description of the screen in SUMMARY.md
      **Source:** CONTEXT

## Notes
- Frontend continua sem bundler (ES modules), i18n pt-BR/en, tema claro/escuro, `prefers-reduced-motion`.
