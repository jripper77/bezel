shipped_at: 2026-10-09T15:46:13Z
verdict: APPROVED_WITH_WARNINGS
by: jripper77

## Learnings
- Em mudanças de layout do Studio, preserve ids/`data-*` usados pelas specs e2e e fixe as dimensões que definem a escala do canvas (sidebar/statusbar), senão `workspace-selection` quebra por 1px.
- Ao mover controles para dentro de `#stage`, confira os modos que escondem o stage (`storage-wide`): o Undo sumiu assim na T-3.
- O servidor e2e é `scripts/serve-e2e.py`; `python -m http.server` causa falhas aleatórias com vários workers no Windows.
- Testes que amostram frames por tempo (`card-animation:3`) oscilam sob carga: reexecute em série antes de julgar.
- Mockups trazem features sem lógica: verifique no código (grep no bridge) antes de omitir ou implementar — "Prova sulla faccia successiva" já existia (D-12).
