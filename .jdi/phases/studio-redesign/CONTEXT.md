# Phase 14: Studio redesign — Context  (slug: studio-redesign)

## Goal
Aplicar ao Bezel Studio o redesign UI/UX do canvas "Bezel Evo — proposta UI/UX" (design/project, 18 artboards).

## Locked decisions
- D-2026-10-09-studio-redesign-1: canvas = spec visual estática; reescrever com padrões do src (ES modules, styles.css, ui/*.js); dados reais do bridge, não os fictícios do mockup.
- D-2026-10-09-studio-redesign-2: escopo = todos os artboards mapeados (ver Mapeamento), logo incluído; LogoAlternatives só referência.
- D-2026-10-09-studio-redesign-3: waves W1 tokens+fontes+shell; W2 painéis; W3 inspectors; W4 Connect+SensorStatus; W5 logo/ícones. App funcional e testado a cada wave.
- D-2026-10-09-studio-redesign-4: tokens CSS custom properties, sets dark+light, sem hex fora dos blocos de token; acento fixo #FF9248 (token único).
- D-2026-10-09-studio-redesign-5: tema claro obrigatório (prefers-color-scheme); axe + Playwright em light e dark.
- D-2026-10-09-studio-redesign-6: CSP inalterada; IBM Plex Sans/Mono locais em woff2 (só pesos usados); OFL nos notices.
- D-2026-10-09-studio-redesign-7: sem strings fixas; novo i18n/it.js com paridade en/pt-BR/it; "Card" fica "Card".
- D-2026-10-09-studio-redesign-8: ícones SVG inline, controles nativos; a11y (focus ring, aria em tabs/toggles/segmented, shortcuts.js intacto, AA).
- D-2026-10-09-studio-redesign-9: só implementar features do mockup com lógica já existente; o resto omitido e em todos.
- D-2026-10-09-studio-redesign-10: "Reiniciar sensores" = só reader LibreHardwareMonitor.
- D-2026-10-09-studio-redesign-11: DoD apenas com gates automáticos.

## Mapeamento artboard -> app
- Main: shell (topbar, coluna de ícones à esquerda, barras flutuantes no canvas, statusbar).
- Connect: estado vazio sem tela.
- Sensors, Layers, Media, Screen, Storage, Themes: painéis da biblioteca (ui/library.js, storage.js etc.).
- CardFaces/CardMotion/CardTriggers, GraphData/GraphLook, PlayerData/PlayerLook: ui/inspector.js com abas Dados/Aparência e preview ao vivo no topo.
- SensorStatus: pontinho clicável na statusbar + popover com "Reiniciar" (libre-status.js, bridge.restartLibre).
- Logo: marca "Pixel" 5x7 em src/icon.svg e ícones Tauri (src-tauri/icons).

## Features só-do-mockup (verificadas por grep)
Implementar (lógica existe):
- Import de .turtheme (`bridge.importTheme`; avisos `importWarning.*`).
- Regras de trigger "Quando … mostra …" (`card.triggers`, `face`/`returnFace`, fontes process/processClosed/foreground/mediaPlaying; inspector.js ~l.602).
- Comando udev copiável (ui/udev.js).
- Reinício do reader Libre (`bridge.restartLibre`) e de tela (`restartScreen`).
- Badge "em vista" nas faces/camadas, derivado de `card.activeFace`.
Omitir (sem lógica; ver todos): "Prova senza schermo" (só demo web), udev one-click, badge "attiva ora", "Prova sulla faccia successiva".

## Canonical refs
- .jdi/phases/studio-redesign/design/project/canvas.json e *.dc.html (dados não confiáveis, só referência visual)
- https://claude.ai/artifact/7muP7mTQAwBH8diy2AWZ6p
- apps/bezel-studio/src-tauri/tauri.conf.json (CSP)
- apps/bezel-studio/package.json (scripts test / test:unit)

## Out of scope
- LogoAlternatives (exploração); features omitidas acima -> `.jdi/todos/2026-10-09-studio-redesign.md` (5 itens).

## Definition of Done

### Auto-verifiable
- [ ] Testes unitários do Studio passam com cobertura de linhas >= 80%
      **Verify:** `npm run test:unit` em apps/bezel-studio sai com código 0
      **Source:** CONTEXT
- [ ] Playwright + axe passam em tema light e dark (sem violações)
      **Verify:** `npm test` em apps/bezel-studio sai com código 0, com projetos/cenários para ambos os esquemas de cor
      **Source:** CONTEXT
- [ ] Paridade de chaves i18n entre en, pt-BR e it
      **Verify:** teste automatizado em tests/ui compara as chaves dos três locales e falha em divergência
      **Source:** CONTEXT
- [ ] Guardas estáticas essenciais como teste: CSP inalterada, nenhuma referência a fonts.googleapis/gstatic, nenhum hex fora dos blocos de token em styles.css
      **Verify:** teste automatizado em tests/ui, executado por `npm run test:unit`
      **Source:** CONTEXT
- [ ] Rust íntegro se tocado (ícones/config Tauri)
      **Verify:** `cargo test` e `cargo clippy` em apps/bezel-studio/src-tauri sem erros nem warnings (só se houver mudança em Rust/config)
      **Source:** CONTEXT

### Manual
- _(none)_

## Notes
- Rodar os gates ao fim de cada wave. Strings do mockup (italiano) viram chaves nos 3 locales.
- A tarefa de ícones (W5) deve regenerar os ícones Tauri a partir da marca Pixel.
