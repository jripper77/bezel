# Phase 4: Motor de renderização — Summary  (slug: render-engine)

**Status:** complete (código); validação no hardware pendente (DoD manual)
**Tasks:** 5/5 complete, 0 blocked

> Nota de processo: T-4.5 executada pelo `jdi-doer-bezel` num worktree (`wt/render-cli`), com o commit
> cherry-picked para a `main` pelo orquestrador. T-4.1..T-4.4 já estavam concluídas (hashes do PLAN).

## Executed tasks
- T-4.1: modelo de tema no core (dado puro), `format_clock` (pt-BR/en), `Histories` com lacunas e adoção
  após edição, portas `FrameRenderer`/`ThemeStore`/`RenderContext`, use case `ThemeRuntime::{frame, show,
  replace}` — `58e2518`, `73bcd39`
- T-4.2: `bezel-render` — `SkiaRenderer` (tiny-skia + cosmic-text + image) desenha todos os tipos de
  elemento, fundos, gradientes, opacidade, fits e GIF por tempo; goldens — `96f7d4b`, `d3a4158`
- T-4.3: `bezel-themes` formato nativo — `FsThemeStore` para `.bezeltheme` (zip) e pasta, DTOs
  versionados, round-trip — `6e6cbc8`, `d3a4158`
- T-4.4: importadores YAML do Python e `.turtheme` do TURZX (parser NRBF com whitelist) com
  `ImportReport` — `504ab76`
- T-4.5: CLI `render`/`run`/`import` + temas embutidos — `cff8794`
  - `bezel render <tema> -o out.png [--fake]`: um frame via `ThemeRuntime` (amostra de aquecimento +
    250 ms, depois o frame) num PNG do tamanho do canvas; aceita `.bezeltheme`, pasta nativa, nome de
    tema embutido e também `.turtheme`, `theme.yaml` e pasta de tema Python (convertidos na hora, avisos
    no stderr).
  - `bezel run <tema> [--screen X]`: abre a tela, confere que o canvas do tema cabe no painel naquela
    orientação, aplica a orientação do tema e mostra um frame a cada `refresh_seconds` (0,25–60 s) até
    Ctrl+C/SIGTERM (`ctrlc` com `termination`); ao parar — ou se um frame falhar — devolve a tela ao modo
    standalone (`release`) e sai 0. Laço testável: trait `Pace` (espera + parada) e `--frames N` oculto.
  - `bezel import <origem> -o <destino>`: converte para `.bezeltheme` ou pasta, recusa sobrescrever e
    imprime os avisos do `ImportReport`.
  - Temas embutidos em `themes/` (pasta nativa, `theme.json` canônico gerado pelo próprio serializador +
    `assets/` com ícones próprios), família "Midnight": `turing-8.8-horizontal` (1920x480, landscape),
    `turing-8.8-vertical` (480x1920, portrait), `turing-5-horizontal` (800x480), `turing-3.5-vertical`
    (320x480), `turing-3.5-horizontal` (480x320) e `turing-2.1-round` (480x480, serve o 2.8" redondo).
    Todos mostram relógio/data, uso+temperatura de CPU e GPU, RAM e rede (os maiores também disco),
    só com chaves do catálogo demo (`bezel sensors --fake --json`).
  - Fontes uma vez só em `themes/fonts/`: Inter (Regular/Medium/SemiBold/Bold) e JetBrains Mono
    (Regular/Medium/Bold), com os textos da SIL OFL 1.1. `bezel_render::font_files(dir)` lê as fontes;
    `render`/`run` carregam `fonts/` ao lado do tema e do diretório embutido
    (`$BEZEL_THEMES_DIR` → `<exe>/../share/bezel/themes` → `~/.local/share/bezel/themes`) antes das do
    sistema.
  - `SkiaRenderer::problems()` expõe os diagnósticos (fonte/asset ausente); o `main` imprime-os como
    `bezel: warning:` e o teste dos temas embutidos exige lista vazia.

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-cli/{Cargo.toml,src/{lib,main,clock,live,theme}.rs}`,
  `crates/bezel-cli/tests/{render,runtime,bundled_themes}.rs`, `crates/bezel-cli/tests/support/mod.rs`
- `themes/**` (6 temas + `fonts/`)
- Fora do `files_modified` do PLAN (sinalizado): `crates/bezel-render/src/{fonts,diagnostics,renderer,
  text,lib,golden}.rs` (helper `font_files`, `SkiaRenderer::problems`, mensagem de fallback de fonte sem
  `Some(..)`), `Cargo.lock` (ctrlc 3.5.2, nix 0.31, cfg_aliases), `README.md`, `CHANGELOG.md`
- Observação de design: o `import_path` do `bezel-themes` é chamado direto pelo adaptador de CLI
  (importadores são conversores do adaptador de temas, não há porta de importação no core); temas
  nativos passam pela porta `ThemeStore`.

## Tests
- Workspace: 351 passando, 0 falhando, 2 ignorados (timing do renderer e corpus local de importação)
  — `cargo test --workspace --locked`
- Novos: `bezel` lib 29 (7 de `theme`, 6 de `live`, dispatch em `lib`), `tests/render.rs` 5 (inclui o
  DoD `render_writes_a_png_of_the_canvas_size`), `tests/runtime.rs` 4 (`ThemeRuntime` com
  `FakeSensors`, renderer gravador, `SkiaRenderer` e `FakeConnector`), `tests/bundled_themes.rs` 1
- Coverage (`cargo llvm-cov --workspace --locked --summary-only`): **94,95% de linhas** no total;
  `bezel-cli/src/theme.rs` 99,47%, `live.rs` 100%, `clock.rs` 100%, `lib.rs` 87,05%, `main.rs` 92,86%;
  `bezel-render/src/fonts.rs` 93,33%
- DoD auto T-4.5: `cargo test -p bezel --locked --test render -- --exact
  render_writes_a_png_of_the_canvas_size` → 1 passed
- Prévias: `BEZEL_THEME_PREVIEWS=<pasta> cargo test --release -p bezel --test bundled_themes` grava um PNG
  de cada tema (datas em en e pt-BR) após 60 amostras de sensores em movimento

## Hardware validation
- Não executada por este agente (a tela 8.8" está com o `turing-smart-screen.service` do usuário; só
  `--fake` foi usado). Pendente: `bezel run turing-8.8-horizontal` na 8.8" real por um minuto
  (comando, duração, frames, erros) — DoD manual da phase.

## Follow-ups
- `scripts/install-local.sh` ainda não copia `themes/` para `~/.local/share/bezel/themes` (onde o CLI e
  o Studio procuram os temas embutidos); até lá, usar caminho (`bezel run themes/turing-8.8-horizontal`)
  ou `BEZEL_THEMES_DIR`.
- O Studio cria o renderer só com fontes do sistema; carregar `themes/fonts` com
  `bezel_render::font_files` na phase `studio-app` para o WYSIWYG valer em máquinas sem Inter.
