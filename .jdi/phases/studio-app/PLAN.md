# Phase 5: Bezel Studio — Plan  (slug: studio-app)

## Goal
App de janela única com editor drag-and-drop WYSIWYG, sensores ao vivo, bandeja e autostart.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-studio-app-1..5

## Tasks

### Wave 1 (frontend, against the demo bridge)

#### T-5.1: Editor store
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/editor/{store,snap,geometry,widgets}.js` (os comandos ficaram em `store.js`), `apps/bezel-studio/tests/ui/editor-*.test.mjs`
- **Acceptance:** immutable theme state (DTO shape), command stack with undo/redo and coalescing of drags, add/remove/duplicate/reorder/move/resize/update, snapping to canvas and elements, alignment/distribution, selection model
- **Dependencies:** none
- **Test:** `npm run test:unit`
- **Status:** completed (790aefb)

#### T-5.2: Workspace layout and canvas
- **Files modified:** `apps/bezel-studio/src/{index.html,styles.css,app.js,shortcuts.js}`, `apps/bezel-studio/src/ui/{canvas,dragdrop,dom,icons}.js` (overlay, barra superior e de status ficaram em `canvas.js`/`app.js`)
- **Acceptance:** three-column layout; canvas with bezel, zoom/fit/pan; preview image from the bridge; overlay handles, marquee, guides; keyboard shortcuts
- **Dependencies:** T-5.1
- **Status:** completed (3e9b21d)

#### T-5.3: Library panels and inspector
- **Files modified:** `apps/bezel-studio/src/ui/{library,inspector,fields}.js` (o campo de cor ficou em `fields.js`), `apps/bezel-studio/src/i18n/*`
- **Acceptance:** widgets palette and sensor browser draggable onto the canvas; layers with visibility/lock/reorder; theme gallery; media; screen controls; inspector forms for every element kind and for the theme
- **Dependencies:** T-5.1
- **Status:** completed (3e9b21d)

### Wave 2 (backend)

#### T-5.4: Studio backend commands and live runtime
- **Files modified:** `apps/bezel-studio/src-tauri/src/**`, `apps/bezel-studio/src-tauri/{Cargo.toml,build.rs,capabilities/default.json,tauri.conf.json}`
- **Acceptance:** commands list/connect screens, brightness/orientation, sensor catalog + samples, theme get/set/save/load/import, bundled themes, `render_preview` (ipc::Response RGBA), live runtime thread with theme swap; tray (show/hide, live toggle, quit) and autostart
- **Dependencies:** render-engine merged (bezel-render, bezel-themes, bezel-sensors)
- **Status:** completed (49d3f16, 67f4e85, 6fcbefd: preview, live, biblioteca, mídia, bandeja, autostart, importação pela UI, temas inclusos e fontes)

#### T-5.6: Vertical e horizontal como escolha de primeira classe
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/{index.html,styles.css,app.js}`, `apps/bezel-studio/src/editor/{store,geometry}.js`, `apps/bezel-studio/src/ui/{library,inspector}.js`, `apps/bezel-studio/src/i18n/*`, `apps/bezel-studio/src-tauri/src/{backend,settings,commands}.rs`, `apps/bezel-studio/tests/**`
- **Acceptance:** (pedido do usuário: "posso usar a tela na vertical ou na horizontal")
  - barra superior com **Vertical | Horizontal** e **Girar 180°**; os nomes da UI são vertical/horizontal (e "invertida"), não retrato/paisagem;
  - trocar vertical↔horizontal troca o canvas e reposiciona os elementos proporcionalmente (tamanhos mantidos, dentro do canvas), num único passo de desfazer; girar 180° mantém o layout;
  - "Novo tema" oferece vertical ou horizontal; o padrão é a última orientação usada com aquela tela (lembrada nas configurações), senão horizontal para telas em barra (proporção ≥ 2:1, como a 8.8") e a nativa nas demais;
  - no modo ao vivo a tela segue a troca na hora;
  - a galeria de temas mostra a orientação de cada tema.
- **Dependencies:** T-5.4
- **Test:** `npm run test:unit`; Playwright "switch between vertical and horizontal"; `cargo test -p bezel-studio`
- **Status:** completed (19b0f71, 853a120)

### Wave 3

#### T-5.5: Tests, a11y, packaging
- **Files modified:** `apps/bezel-studio/tests/e2e/*.spec.mjs`, `.jdi/PROJECT.md` (§ Frontend via D-XX), `scripts/install-local.sh`
- **Acceptance:** Playwright critical paths in light/dark with axe; install-local; hardware check live on the 8.8"
- **Dependencies:** T-5.2, T-5.3, T-5.4, T-5.6
- **Status:** completed (e2e: 3e9b21d..af98ba2, 24 Playwright × claro/escuro com axe; PROJECT.md § Frontend + D-2026-09-30-studio-app-6: 1bca123; install-local: 6fcbefd; hardware: teste opt-in `tests/hardware.rs` ok na 8.8")

## Execution
- Total tasks: 6
- Waves: 3

## Notes
- DoD `render_preview`: os comandos Tauri ficaram finos (`commands.rs`) e a lógica foi para `backend.rs`,
  testável sem Tauri; o teste do critério é `backend::tests::render_preview_returns_the_canvas_size`
  (o comando de verificação no CONTEXT foi ajustado; o critério é o mesmo).
