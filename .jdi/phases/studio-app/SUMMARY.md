# Phase 5: Bezel Studio — Summary  (slug: studio-app)

**Status:** partial
**Tasks:** 5/6 complete, 0 blocked (T-5.5 pendente: e2e finais, PROJECT.md § Frontend, validação ao vivo na 8.8")

> Nota de processo: T-5.1..T-5.4 (backend) foram executadas pelo orquestrador fora do `/jdi-do`; a T-5.6 e a
> importação da T-5.4 pelo `jdi-doer-bezel` no worktree `wt/studio-orient` (commits cherry-picked).

## Executed tasks
- T-5.1: store do editor — estado imutável no formato do `theme.json`, desfazer/refazer com arrastes agrupados
  (volta a "salvo" ao desfazer até o tema salvo), snapping, alinhar/distribuir, seleção — `790aefb`
- T-5.2: janela única em três colunas, canvas com moldura, zoom/ajuste, alças, marquee, guias, atalhos — `3e9b21d`
- T-5.3: biblioteca (widgets e sensores arrastáveis, camadas, temas, mídia, tela) e inspetor — `3e9b21d`
- T-5.4: backend — preview com o renderizador real (`render_preview`, RGBA via `ipc::Response`), laço de
  atualização (sensores + modo ao vivo, lembrado e restaurado ao iniciar, interrompido com mensagem em falha),
  biblioteca de temas (inclusos são copiados ao salvar), mídia com nomes seguros e miniaturas, bandeja (o app
  fica nela enquanto há tela ao vivo), início com o sistema (`--hidden`) — `49d3f16`; importar `.turtheme`,
  `theme.yaml`/`.yml` e pastas Python pela UI, com a lista de avisos — `67f4e85`; primeiro uso abre o tema
  incluso feito para a tela conectada, fontes inclusas carregadas no renderizador, `themes/` como recurso
  dos pacotes e no `install-local.sh` — `6fcbefd`
- T-5.6: vertical e horizontal como escolha de primeira classe (pedido do usuário) — `19b0f71`, `853a120`
  - barra superior: **Vertical | Horizontal** e **Girar 180°**; UI diz vertical/horizontal e "invertida";
  - `setOrientation`: vertical↔horizontal troca os lados do canvas e **transpõe** o layout (uma pilha vertical
    vira uma fileira horizontal), tamanhos mantidos, reduzidos só onde não cabem, tudo dentro — um passo de
    desfazer; 180° mantém o layout;
  - **Novo vertical / Novo horizontal**; padrão = última orientação usada com a tela (`settings.json`), senão
    horizontal para telas em barra (≥ 2:1, como a 8.8") e a nativa nas demais;
  - modo ao vivo segue a troca na hora; galeria com selo Vertical/Horizontal.

## Blocked tasks
- nenhuma

## Files modified
- `apps/bezel-studio/src/**` (UI, editor, i18n, demo), `apps/bezel-studio/tests/**`
- `apps/bezel-studio/src-tauri/src/{backend,studio,library,media,settings,clock,tray,commands,dto,lib}.rs`,
  `apps/bezel-studio/src-tauri/{Cargo.toml,build.rs,capabilities/default.json,tauri.conf.json}`
- `scripts/install-local.sh`; fora do PLAN (sinalizado): `crates/bezel-render` (`font_families`),
  `crates/bezel-themes` (`load_manifest`)

## Tests
- `cargo test --workspace --locked`: 352 passando, 2 ignorados; `bezel-studio`: 32
- `npm run test:unit`: 45 passando; linhas 99,79%, `src/editor/store.js` 100%
- Playwright: 16 passando (8 cenários × claro/escuro, axe sem sérias/críticas), incl. "drag a widget onto the
  canvas", "keyboard move and undo", "switch between vertical and horizontal"
- Coverage Rust: 94,43% de linhas no workspace na medição do doer (`backend.rs` 98,18%, `settings.rs` 97,67%,
  `dto.rs` 99,25%; `commands.rs`/`lib.rs` = raiz de composição)

## Hardware validation
- Studio instalado (`install-local.sh`) e aberto em modo simulado (`BEZEL_FAKE=1`) sem erros.
- Não executado ainda na 8.8" real: a tela está com o `turing-smart-screen.service` do usuário. Modo ao vivo
  e troca de orientação verificados no `FakeConnector` (orientações `[ReversePortrait, Landscape]`, frames
  480×1920 → 1920×480). Pendente (DoD manual): desenhar um tema e vê-lo ao vivo na 8.8", fechar a janela e
  seguir pela bandeja.

## Observações
- Avisos do importador vêm em inglês do `bezel-themes`; a moldura é traduzida.
- `restore` dividido em `restore_theme` (no `setup`, sem corrida com a sessão da UI) e `restore_live`.
