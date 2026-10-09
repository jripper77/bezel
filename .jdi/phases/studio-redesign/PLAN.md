# Phase 14: Studio redesign — Plan  (slug: studio-redesign)

## Goal
Aplicar ao Bezel Studio o redesign UI/UX do canvas "Bezel Evo — proposta UI/UX" (design/project, 18 artboards), com dados reais do bridge.

## Locked decisions (from CONTEXT.md)
- D-…-1: canvas = spec visual estática; reescrever nos padrões do src (ES modules, styles.css, ui/*.js); dados reais.
- D-…-2: todos os artboards mapeados + logo; LogoAlternatives só referência.
- D-…-3: W1 tokens+fontes+shell → W2 painéis → W3 inspectors → W4 Connect+SensorStatus → W5 logo/ícones; app verde a cada wave.
- D-…-4: tokens em custom properties, sets light+dark, nenhum hex fora dos blocos de token; acento único `--accent: #FF9248`.
- D-…-5: tema claro obrigatório (`prefers-color-scheme`); axe + Playwright em light e dark.
- D-…-6: CSP inalterada; IBM Plex Sans/Mono locais em woff2 (só pesos usados); OFL nos notices.
- D-…-7: zero strings fixas; `i18n/it.js` com paridade en/pt-BR/it; "Card" fica "Card".
- D-…-8: SVG inline, controles nativos, focus ring, aria em tabs/toggles/segmented, `shortcuts.js` intacto, contraste AA.
- D-…-9: só features do mockup com lógica existente; resto em todos. D-…-10: "Reiniciar sensores" = só reader Libre. D-…-11: DoD só com gates automáticos.

## Pontos verificados no código (settled)
1. **Badge "em vista"** — derivável: `card.activeFace` é a face exibida no editor (store.js `cardFace`, canvas filtra membros por ela em `editor/cards.js:30`); `ui/library.js:235` já calcula `active` (classe `active-face` + `aria-pressed`). A rotação automática só existe no render do demo (`card-timer.js`, fora do store) e no device, logo o badge significa "face em edição/visível no editor" — texto visível, não só cor.
2. **"Prova sulla faccia successiva"** — EXISTE: `inspector.js:594` `action('card.animateNext', 'cardFace', …, { face: (activeFace+1) % n })` dispara a transição via `editor/card-motion.js` (coberto por `card-animation.spec.mjs`). Fica no artboard CardMotion (T-6, restyle com ícone play); o item 4 sai de `.jdi/todos/2026-10-09-studio-redesign.md`.
3. **Light/dark** — já roda: `playwright.config.mjs` tem `light-pt`, `dark-pt`, `light-en`, `dark-en` (colorScheme + locale em `metadata`), e `scripts/e2e-passed.mjs` falha se faltar scheme×locale. Nenhuma task de config; toda spec nova usa `expectAccessible`. Italiano fica fora dos projetos e2e (custo +50% de runtime): coberto por paridade de chaves + placeholders em unit.
4. **Testes acoplados ao DOM** — sem screenshot/aria snapshots. Acoplamento por id/classe/`data-*` nas specs e2e e em 2 unit tests (`i18n.test.mjs`: index.html só com texto "Bezel Evo", ≥20 `data-i18n` válidos; `window-boundary.test.mjs`: index.html sem script inline/`on*=`). Regra para todas as tasks: **preservar ids e `data-*` existentes**; classe renomeada → spec atualizada na mesma task (lista por task abaixo).
5. **Ícones Tauri** — não há script: os arquivos em `src-tauri/icons/` são a saída padrão do `tauri icon` (Square*Logo, StoreLogo, icns, ico) + extras manuais `icon-1024.png`, `tray.png`, `64x64.png`, `icon.svg`. O ícone do instalador NSIS e do exe vem de `icons/icon.ico` (`installer.nsi` `{{installer_icon}}`; tray usa `default_window_icon`). T-8 cria script reprodutível com `@tauri-apps/cli@2.12.1` (mesmo pin do `package-evo.ps1`).

Extra achado: UI em italiano exige `parse_language` aceitar `it` em `src-tauri/src/texts.rs` (os textos Rust de italiano já existem) e `guide_url` só aceita `en`/`pt-BR` → UI mapeia `it`→`en` para guias.

## Tasks

### Wave 1 (W1, paralelo)

#### T-1: Tokens light/dark + IBM Plex local + guardas estáticas
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/styles.css`, `apps/bezel-studio/src/assets/fonts/ibm-plex/{IBMPlexSans-Regular,IBMPlexSans-Medium,IBMPlexSans-SemiBold,IBMPlexMono-Regular,IBMPlexMono-Medium}.woff2`, `apps/bezel-studio/src/assets/fonts/ibm-plex/{LICENSE,README.md}`, `apps/bezel-studio/tests/ui/static-guards.test.mjs`, `README.md`, `FORK.md`, `packaging/windows/README.md`, `scripts/windows/package-evo.ps1`
- **Acceptance:**
  - Blocos de token: `:root { … }` (light) e `@media (prefers-color-scheme: dark) { :root { … } }`, com superfícies/texto/borda/estado/seleção da paleta do canvas; `--accent: #FF9248` definido uma única vez; texto sobre acento ≥ 4.5:1. Os 51 hex fora dos blocos (ex.: `.status-dot`, `.switch`, `.thumb`, `.object-menu #0006`) viram `var(--…)`.
  - `@font-face` só com `url('assets/fonts/ibm-plex/…woff2')` (pesos Sans 400/500/600, Mono 400/500; 700 do mockup mapeia para 600); `font-display: swap`. OFL: `LICENSE` = texto OFL-1.1 + copyright IBM; `README.md` = origem (release oficial `IBM/plex`, versão, SHA-256 por arquivo), no padrão de `assets/mdi/README.md`; linha "IBM Plex: SIL OFL 1.1" em README.md (§licenças, l.195), FORK.md e `packaging/windows/README.md`; `Add-Payload … 'LICENSE-IBM-Plex-OFL.txt'` em `package-evo.ps1` ao lado de Tabler/MDI.
  - `static-guards.test.mjs` (roda em `npm run test:unit`): (a) CSP de `tauri.conf.json` igual à string fixada no teste; (b) nenhum `fonts.googleapis`/`fonts.gstatic` em `src/**` nem em `tauri.conf.json`; (c) nenhum hex em **valores de declaração** de styles.css fora dos blocos de token (seletores `#id` não contam); (d) todo `url()` de `@font-face` existe em disco e todo `font-weight` numérico ∈ {400,500,600}.
- **Dependencies:** none
- **Test:** `node --test tests/ui/static-guards.test.mjs`; `npm test` (axe light+dark sem violação de contraste)
- **Status:** completed (commit 8e56327)

#### T-2: Locale italiano + paridade + plumbing de idioma
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/i18n/it.js` (novo), `apps/bezel-studio/src/i18n/{index.js,en.js,pt-BR.js}`, `apps/bezel-studio/src/ui/preferences.js`, `apps/bezel-studio/src/ui/gif-search.js`, `apps/bezel-studio/src/app.js`, `apps/bezel-studio/src/demo-backend.js`, `apps/bezel-studio/tests/ui/i18n.test.mjs`, `apps/bezel-studio/src-tauri/src/texts.rs`
- **Acceptance:**
  - `LOCALES = { 'pt-BR', en, it }`; `pickLocale(['it-IT'])==='it'`; `LANGUAGES` inclui `it`; novo `guideLocale(locale)` (`it`→`en`) usado nas chamadas `bridge.openGuide` de app.js e gif-search.js; demo-backend aceita `it` em `setLanguage`/guias como o backend.
  - `i18n.test.mjs`: teste explícito de paridade en/pt-BR/it que lista chaves faltando/sobrando por locale e compara `placeholders()` por chave; `languageOptions` atualizado com `['it', 'Italiano']`; "Card" mantido nos 3.
  - `texts.rs`: `parse_language` aceita `Language::Italian`; teste `languages_are_spelled_like_the_ui` cobre `it`; `cargo test` + `cargo clippy -- -D warnings` limpos em `src-tauri`.
- **Dependencies:** none
- **Test:** `npm run test:unit`; `cargo test -p bezel-studio texts`
- **Status:** completed (commit 5e052b4)

### Wave 2 (W1, sequencial)

#### T-3: Shell — topbar, coluna de ícones, barras flutuantes, statusbar
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/index.html`, `apps/bezel-studio/src/styles.css`, `apps/bezel-studio/src/app.js`, `apps/bezel-studio/src/ui/icons.js`, `apps/bezel-studio/src/i18n/{en,pt-BR,it}.js`, `apps/bezel-studio/tests/e2e/shell.spec.mjs` (novo), `apps/bezel-studio/tests/e2e/features-016.spec.mjs`
- **Acceptance:**
  - Artboard Main: tablist da biblioteca vira coluna de ícones vertical (`role=tablist`, `aria-orientation=vertical`, setas ↑/↓, nome acessível por `data-i18n-aria-label`), mantendo `#tab-*`/`#panel-*`/`data-tab`; undo/redo/copy/paste e zoom viram barras flutuantes sobre `#stage` com os mesmos ids; statusbar mantém `#status-main`, `#status-device`, `#status-libre*`.
  - Fontes Plex aplicadas; focus ring visível com `--accent`; `shortcuts.js` e `tests/ui/shortcuts.test.mjs` intocados; `i18n.test`/`window-boundary.test` verdes (index.html sem texto além de "Bezel Evo", sem script inline).
  - `shell.spec.mjs`: navegação por teclado no rail, barras flutuantes clicáveis, `expectAccessible` (4 projetos). `features-016` (overflow de `.library`) ajustado ao novo seletor.
- **Dependencies:** T-1, T-2
- **Test:** `npm test`
- **Status:** completed (commit f08ef95)

### Wave 3 (W2)

#### T-4: Painéis Sensors, Layers, Media, Themes
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/ui/{library,collection,icon-picker,gif-search}.js`, `apps/bezel-studio/src/index.html`, `apps/bezel-studio/src/styles.css`, `apps/bezel-studio/src/i18n/{en,pt-BR,it}.js`, `apps/bezel-studio/tests/e2e/{cards,layer-drag,workspace-selection,icons,themes,gif-search,alignment-019,card-animation}.spec.mjs`
- **Acceptance:**
  - Sensors/Layers/Media/Themes conforme artboards, com dados do catálogo/store; Layers: base/faces com badge "em vista" derivado de `card.activeFace` (texto + `aria-current`), sem badge "attiva ora"; Themes: botão "Importar .turtheme" = `#theme-import` existente (`bridge.importTheme`) e avisos `importWarning.*` no `#import-report`; segmented com `aria-pressed`.
  - Ids/`data-element-id`/`data-face`/`data-drop-*` preservados (drag de camadas intacto); specs listadas atualizadas só onde a classe mudou; axe light+dark verde.
- **Dependencies:** T-3
- **Test:** `npm test` (specs listadas + `tests/ui/library.test.mjs`)
- **Status:** pending

### Wave 4 (W2)

#### T-5: Painéis Screen e Storage
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/ui/{storage,standby,manager,video-preview}.js`, `apps/bezel-studio/src/index.html`, `apps/bezel-studio/src/styles.css`, `apps/bezel-studio/src/i18n/{en,pt-BR,it}.js`, `apps/bezel-studio/tests/e2e/{storage,storage-manager,standby,live-screen-controls,device-label-020}.spec.mjs`
- **Acceptance:**
  - Subtabs Impostazioni/Archivio (`#subtab-device`/`#subtab-storage`) e conteúdo conforme artboards Screen/Storage, com "Reiniciar tela" ligado a `restartScreen` existente; barra de uso via `usedFraction`/`formatBytes` (sem números do mockup).
  - `tests/ui/storage.test.mjs` verde sem mudar assinaturas exportadas; specs listadas ajustadas só onde a classe mudou (`.drop-zone`, `.album-frame`, `.album-thumb`); axe light+dark verde.
- **Dependencies:** T-4
- **Test:** `npm test`
- **Status:** pending

### Wave 5 (W3)

#### T-6: Inspectors com abas e preview ao vivo
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/ui/inspector.js`, `apps/bezel-studio/src/inspector-tabs.js` (novo, puro), `apps/bezel-studio/tests/ui/inspector-tabs.test.mjs` (novo), `apps/bezel-studio/src/styles.css`, `apps/bezel-studio/src/i18n/{en,pt-BR,it}.js`, `apps/bezel-studio/tests/e2e/{inspector-sections,card-animation,cards,ring-gradient,icons,clock,weather,video-framing,video-background,video-preview,device-video,editing-017}.spec.mjs`, `.jdi/todos/2026-10-09-studio-redesign.md`
- **Acceptance:**
  - Card: abas Facce/Movimento/Trigger (faces em chips com badge "em vista" por `activeFace`; Movimento inclui `card.animateNext` como "testar na próxima face"; Trigger = regras existentes `card.triggers`, sem "attiva ora"); Graph/Player e demais widgets: Dados/Aparência. `role=tablist/tab/tabpanel`, setas ←/→, aba escolhida por tipo de elemento sobrevive a edição e Undo (lógica em `inspector-tabs.js`, coberta por unit test).
  - Preview ao vivo no topo = recorte do frame já renderizado em `#preview` (`drawImage` na área do elemento), sem nova chamada de bridge; `role=img` com nome traduzido.
  - `inspector-sections.spec` reescrita para abas (ordem + persistência via Undo); specs que usam controles fora da aba padrão selecionam a aba por `getByRole('tab')`; item 4 removido do todo; axe light+dark verde.
- **Dependencies:** T-5
- **Test:** `npm run test:unit` (`inspector-tabs.test.mjs`); `npm test`
- **Status:** pending

### Wave 6 (W4)

#### T-7: Connect (estado vazio) + SensorStatus (pontinho + popover)
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/ui/connect.js` (novo), `apps/bezel-studio/src/ui/sensor-status.js` (novo), `apps/bezel-studio/src/ui/udev.js`, `apps/bezel-studio/src/app.js`, `apps/bezel-studio/src/index.html`, `apps/bezel-studio/src/styles.css`, `apps/bezel-studio/src/i18n/{en,pt-BR,it}.js`, `apps/bezel-studio/tests/e2e/{libre,scenarios,connect}.spec.mjs` (`connect` novo), `apps/bezel-studio/tests/ui/libre-status.test.mjs`
- **Acceptance:**
  - Sem tela (`?demo=empty`): estado vazio do artboard Connect no stage; em `?demo=denied` mostra o comando udev copiável reaproveitando `ui/udev.js`; sem "Prova senza schermo" e sem udev one-click.
  - Statusbar: pontinho clicável (`button`, `aria-expanded`, `aria-controls`, `#status-libre-dot[data-state]` preservado) abre popover com estado por hardware (`libreStatus().failed`) e "Reiniciar" = `#restart-libre` → só `bridge.restartLibre` (D-10); Escape fecha e devolve foco.
  - `libre.spec` abre o popover antes de agir; `scenarios.spec` (empty/denied) atualizado; `connect.spec` com axe; `window-boundary.test` verde.
- **Dependencies:** T-6
- **Test:** `npm test`
- **Status:** pending

### Wave 7 (W5)

#### T-8: Marca Pixel 5x7 + regeneração dos ícones Tauri
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/icon.svg`, `apps/bezel-studio/src-tauri/icons/*` (svg, png, `icon.ico`, `icon.icns`, `tray.png`, `icon-1024.png`, `64x64.png`), `apps/bezel-studio/scripts/build-icons.mjs` (novo), `apps/bezel-studio/tests/ui/app-icons.test.mjs` (novo)
- **Acceptance:**
  - `src/icon.svg` e `src-tauri/icons/icon.svg` = mesma marca Pixel 5x7 (artboard Logo) com `--accent` literal #FF9248 (arquivo SVG, fora do styles.css).
  - `node scripts/build-icons.mjs` roda `npx --yes @tauri-apps/cli@2.12.1 icon src-tauri/icons/icon.svg -o src-tauri/icons -p 1024 -p 64` e gera `tray.png`/`icon-1024.png`, descartando `android/`/`ios/`; saídas commitadas.
  - `app-icons.test.mjs`: cada PNG listado em `tauri.conf.json` e os extras têm as dimensões do nome; `icon.ico` (fonte do instalador NSIS e do exe) contém 16/24/32/48/64/256; svgs idênticos. `cargo test` + `cargo clippy -- -D warnings` limpos (build.rs reembute o ico).
- **Dependencies:** T-7
- **Test:** `npm run test:unit`; `cargo test`/`cargo clippy` em `src-tauri`
- **Status:** pending

### Wave 8 (limpeza, adicionada pelo usuário em 2026-10-09)

#### T-9: Limpeza do baseline — falhas pré-existentes + clippy
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/live-screen.js` (+ i18n se preciso), `crates/bezel-core/src/domain/theme.rs`, `apps/bezel-studio/src-tauri/src/studio.rs`, specs/código ligados às falhas e2e listadas abaixo
- **Acceptance:**
  - Unit: `i18n.test.mjs` "no sentence is written in the UI code…" passa sem enfraquecer o teste (literais de `live-screen.js` viram chaves i18n).
  - E2E: `device-video:3`, `scenarios:421`, `storage-manager:73`, `storage:189`, `video-framing:48`, `video-framing:228` passam nos 4 projetos (corrigir a causa real; spec só se ela estiver errada); flake `scenarios:56` (drag) estabilizado.
  - `cargo clippy -p bezel-studio --all-targets --locked -- -D warnings` limpo (11 `collapsible_if`), `cargo test` e `cargo fmt --check` limpos.
  - Suíte completa `npm test` verde (gate final da phase).
- **Dependencies:** T-8
- **Status:** pending

## Execution
- Total tasks: 8
- Waves: 7 (W1 = waves 1–2; W2 = 3–4; W3 = 5; W4 = 6; W5 = 7)
- Speedup paralelo estimado: 8/7 ≈ 1.1x (styles.css, index.html e i18n serializam as tasks; só T-1 ∥ T-2)
- Gate ao fim de cada wave: `npm run test:unit` e `npm test` em `apps/bezel-studio`; Rust só em T-2 e T-8.

## Files modified (all tasks)
- `apps/bezel-studio/src/{index.html,styles.css,app.js,icon.svg,demo-backend.js,inspector-tabs.js}`
- `apps/bezel-studio/src/ui/{icons,library,collection,icon-picker,gif-search,storage,standby,manager,video-preview,inspector,connect,sensor-status,udev,preferences}.js`
- `apps/bezel-studio/src/i18n/{index,en,pt-BR,it}.js`, `apps/bezel-studio/src/assets/fonts/ibm-plex/*`
- `apps/bezel-studio/tests/ui/{static-guards,i18n,inspector-tabs,app-icons,libre-status}.test.mjs`, `apps/bezel-studio/tests/e2e/*.spec.mjs` (listadas por task)
- `apps/bezel-studio/scripts/build-icons.mjs`, `apps/bezel-studio/src-tauri/{src/texts.rs,icons/*}`
- `README.md`, `FORK.md`, `packaging/windows/README.md`, `scripts/windows/package-evo.ps1`, `.jdi/todos/2026-10-09-studio-redesign.md`

## Test requirements
- Unit + guardas + paridade: `npm run test:unit` (linhas ≥ 80%; `src/ui/**` e `app.js` excluídos da cobertura → lógica nova pura fica fora de `ui/`, ex. `inspector-tabs.js`)
- E2E + axe light/dark × pt-BR/en: `npm test`
- Rust (T-2, T-8): `cargo test` e `cargo clippy -- -D warnings` em `apps/bezel-studio/src-tauri`
- Minimum coverage: 80% (PROJECT.md)
