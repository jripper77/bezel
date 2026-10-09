# Phase 14: Studio redesign — Summary  (slug: studio-redesign)

**Status:** partial
**Tasks:** 2/8 complete, 0 blocked

## Executed tasks
- T-1: tokens light/dark (`:root` + `@media (prefers-color-scheme: dark)`), `--accent: #FF9248` único, `--accent-text` #1A0E05 (8.5:1), `--accent-ink`/`--focus-ring` para ícones e foco legíveis no claro; 26 hex fora dos blocos viraram `var(--…)`; IBM Plex Sans 400/500/600 + Mono 400/500 locais (npm oficiais `@ibm/plex-sans` 1.1.0 / `@ibm/plex-mono` 2.5.0, SHA-256 no README), 700→600; OFL em README/FORK/packaging + payload no `package-evo.ps1`; `static-guards.test.mjs` (6 testes) verde; topbar fixada em 52px para não mudar a escala do canvas. Commit 8e56327.
- T-2: `i18n/it.js` novo (1146 chaves, paridade total com en/pt-BR e mesmos placeholders; texto do mockup onde se aplica; "Card" mantido; "Powered by KLIPY" mantido como atribuição). `LOCALES` com `it`, `pickLocale(['it-IT'])==='it'`, `LANGUAGES` com `it`; `guideLocale()` (it→en) nas chamadas `openGuide` de app.js e ui/gif-search.js; demo-backend aceita `it` em `setLanguage` (`DEMO_LANGUAGES`) e guias só en/pt-BR (`DEMO_GUIDE_LANGUAGES`, como `guide_url`). `texts.rs`: `parse_language` aceita `Language::Italian`, teste cobre `it`. `i18n.test.mjs`: paridade explícita (faltando/sobrando por locale + placeholders por chave, com autoteste), "Card" en/it, guideLocale, demo em it. en.js/pt-BR.js sem mudança (`language.it` já existia). Commit 5e052b4.

## Blocked tasks
- nenhuma

## Files modified
- apps/bezel-studio/src/styles.css
- apps/bezel-studio/src/assets/fonts/ibm-plex/{5 woff2, LICENSE, README.md}
- apps/bezel-studio/tests/ui/static-guards.test.mjs
- apps/bezel-studio/src/i18n/{it.js,index.js}, src/ui/{preferences,gif-search}.js, src/app.js, src/demo-backend.js, tests/ui/i18n.test.mjs, src-tauri/src/texts.rs
- README.md, FORK.md, packaging/windows/README.md, scripts/windows/package-evo.ps1

## Tests
- `npm run test:unit`: 322 testes, 321 passam, 1 falha pré-existente em HEAD f3b068d (`i18n.test.mjs` "no sentence is written in the UI code": strings de modelo em `live-screen.js`); cobertura de linhas 99.33%.
- Playwright (360): falhas determinísticas restantes = 6 testes × 4 projetos, todas também falhando em HEAD f3b068d (baseline em worktree): device-video:3, scenarios:421, storage-manager:73, storage:189, video-framing:48, video-framing:228. Demais falhas da execução paralela são flakes de carga (servidor python) e passam em série. Nenhuma violação axe `color-contrast` em light/dark.

- T-2 `npm run test:unit`: 330 testes, 329 passam, 1 falha pré-existente (a mesma de `live-screen.js`); cobertura de linhas 99.41%.
- T-2 Playwright (360, `--workers=2` + reexecução serial das falhas): restam as 24 falhas do baseline (6 × 4 projetos) + `scenarios:56` ("drag a widget onto the canvas") intermitente, que também falha em HEAD sem T-2 (4/12 em `--repeat-each=3` com as mudanças guardadas em stash) — flake pré-existente de drag, não regressão.
- T-2 Rust: `cargo test -p bezel-studio` 203 + 1 passam (texts: 2/2); `cargo fmt --check` limpo. `cargo clippy -p bezel-studio --all-targets -- -D warnings` falha em `collapsible_if` pré-existente (bezel-core theme.rs ×3, studio.rs ×8; clippy 1.98), nenhum em texts.rs; com `--no-deps -A clippy::collapsible_if` passa limpo.

## Hardware validation
- não aplicável (só CSS/fontes)
