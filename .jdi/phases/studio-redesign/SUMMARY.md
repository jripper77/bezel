# Phase 14: Studio redesign — Summary  (slug: studio-redesign)

**Status:** partial
**Tasks:** 1/8 complete, 0 blocked

## Executed tasks
- T-1: tokens light/dark (`:root` + `@media (prefers-color-scheme: dark)`), `--accent: #FF9248` único, `--accent-text` #1A0E05 (8.5:1), `--accent-ink`/`--focus-ring` para ícones e foco legíveis no claro; 26 hex fora dos blocos viraram `var(--…)`; IBM Plex Sans 400/500/600 + Mono 400/500 locais (npm oficiais `@ibm/plex-sans` 1.1.0 / `@ibm/plex-mono` 2.5.0, SHA-256 no README), 700→600; OFL em README/FORK/packaging + payload no `package-evo.ps1`; `static-guards.test.mjs` (6 testes) verde; topbar fixada em 52px para não mudar a escala do canvas. Commit 8e56327.

## Blocked tasks
- nenhuma

## Files modified
- apps/bezel-studio/src/styles.css
- apps/bezel-studio/src/assets/fonts/ibm-plex/{5 woff2, LICENSE, README.md}
- apps/bezel-studio/tests/ui/static-guards.test.mjs
- README.md, FORK.md, packaging/windows/README.md, scripts/windows/package-evo.ps1

## Tests
- `npm run test:unit`: 322 testes, 321 passam, 1 falha pré-existente em HEAD f3b068d (`i18n.test.mjs` "no sentence is written in the UI code": strings de modelo em `live-screen.js`); cobertura de linhas 99.33%.
- Playwright (360): falhas determinísticas restantes = 6 testes × 4 projetos, todas também falhando em HEAD f3b068d (baseline em worktree): device-video:3, scenarios:421, storage-manager:73, storage:189, video-framing:48, video-framing:228. Demais falhas da execução paralela são flakes de carga (servidor python) e passam em série. Nenhuma violação axe `color-contrast` em light/dark.

## Hardware validation
- não aplicável (só CSS/fontes)
