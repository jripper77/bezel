# Phase 9: Vídeo de fundo que toca e se enquadra na tela — Summary  (slug: video-background-framing)

**Status:** partial
**Tasks:** 7/8 complete, 0 blocked (T-8: hardware na 8.8" aguarda a tela livre — o studio do usuário a usa)

> `/jdi-issue` autônomo (card colado: relato do usuário de 2026-10-01). Branch `jdi/video-background-framing`, um
> worktree por tarefa, cherry-picked para o branch; `/jdi-loop` convergiu na iter 1 (APPROVED_WITH_WARNINGS, crítico
> do DoD sem linha vazia); Step 6: uma rodada de correção dos avisos.

## Executed tasks
- T-1 `d042d1e`: `domain::framing` (Auto: vídeo no tamanho nativo do painel num tema girado conta como já girado;
  geometria pura cover/contain/zoom/posição com bordas pares), `framing` em `Background::Video`, `device_video_name`
  com o enquadramento (`dragon.mp4`, `amd_90.mp4`, `_f<8hex>`), cópias `_f` protegidas na limpeza.
- T-2 `5c96750`, `25a6c22`: studio — grupo Enquadramento (Rotação com Auto, Preencher/Caber, Zoom 100–400 %, Posição,
  Centralizar, Redefinir, cor das sobras), "Enquadrar no canvas" (arrastar, roda, setas, + −, 0, Esc), aria-live,
  pt-BR/en, demo com vídeo pré-girado.
- T-3 `ea7d6f8`: `bezel-media` — cadeia de filtros do ffmpeg da geometria (padrão = cadeia de hoje), decodificação crua
  ≤ 15 fps com `-threads 2`, pôster enquadrado.
- T-4 `367012d`: `.bezeltheme` com `framing` opcional (ausente no padrão: temas antigos byte a byte iguais), TURZX em
  Auto, rotação inválida recusada.
- T-5 `bd2fab6`: runtime com a info sondada (Auto real), procura o nome enquadrado, reusa o arquivo só com o mesmo
  tamanho, decodificação no PC enquadrada em Rust; `bezel run` lê o MP4 sem ffmpeg e aponta o "Send to the screen".
- T-6 `84d8d30`: backend do studio — prévia tocando (1 decodificador, ≤ 15 fps, fecha após 2 s, `motion=false` =
  pôster), `video_auto`, `open_guide` (D-7: `tauri-plugin-opener`, só Rust, URLs fixas), pôster refeito ao salvar,
  "Enviar para a tela" envia o vídeo nativo como está.
- T-7 `631cb5f`: guia en/pt-BR ("Framing the video" / "Enquadrar o vídeo"), CHANGELOG, `check-docs.sh`.
- Step 6 (avisos da iter 1): `3302c93` (W1: `Cargo.lock` só com as 4 crates do opener), `1790823` (W2: sonda e
  pôster fora do lock da sessão, com testes que falham se voltarem para dentro), `c0d0a04` (W3: README), `370ae78`
  (nova tentativa limitada do decodificador da prévia), `668a25e` (uma lista de rotações), `16baa2e` (envio sem
  ffmpeg pelo caminho do tema), `4feaa0f` (rótulo "Send to the screen" na CLI).

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/{domain/{framing,media,poster,theme,cleanup,mod},app/runtime,ports/mod}.rs`,
  `crates/bezel-core/tests/runtime_video.rs`
- `crates/bezel-media/src/{lib,framing,transcode,poster,stream,probe}.rs`, `crates/bezel-themes/src/{dto,native}.rs`,
  `crates/bezel-themes/src/import/{turzx.rs,turzx/tests.rs}`, `crates/bezel-render/**`, `crates/bezel-cli/src/live.rs`
- `apps/bezel-studio/src/**` (inspector, canvas, `editor/video-framing.js`, bridge, demo, i18n), `tests/**`,
  `apps/bezel-studio/src-tauri/{src/**,Cargo.toml,build.rs,capabilities/default.json}`
- `docs/user/{,pt-BR/}storage-and-video.md`, `scripts/ci/check-docs.sh`, `README.md`, `CHANGELOG.md`, `Cargo.lock`

## Tests
- `cargo test --workspace --locked`: 866 passando, 0 falhando, 11 ignorados (hardware e ffmpeg real; os 8 de ffmpeg
  real rodados à parte com o `dragon.mp4` do fabricante: 8/8)
- UI: 180 unitários; Playwright 176 (claro/escuro × pt-BR/en, axe); DoD 6 = 16 runs
- fmt, clippy `-D warnings` (Linux e `--target x86_64-pc-windows-msvc`), `check-docs.sh`, `check-packaging.sh`
- Coverage (`cargo llvm-cov`): 94,34% de linhas; `framing.rs` 100%, `studio.rs` 97,08%, `backend.rs` 98,58%
- CI: run 36861544813 verde no `cd13083` (Windows 797/0/11); run do HEAD após a rodada de correção: ver REVIEW
- Fuzz do revisor: 200 mil casos de geometria sem problema de borda, limites ou aspecto

## Hardware validation
- Tema "Dragon Ball" do usuário (1920×480, `dragon.mp4` 480×1920 pré-girado): Auto = 270° no canvas e 0 no total;
  nome `dragon.mp4`; conversão identidade; pôster em pé — conferido em testes e com o arquivo real do fabricante.
- **Pendente (T-8, orquestrador com a tela livre):** ao vivo na 8.8", o "Dragon Ball" toca o `dragon.mp4` guardado,
  inteiro, em pé, sob o tema, sem envio (ou, se o tamanho guardado diferir, "Enviar para a tela" o manda como está).

## Observações
- A prévia ainda abre o ffmpeg e lê quadros sob o lock da sessão (D-5); a sonda e o pôster saíram dele.
- Sem a sonda, a decodificação no PC estica a imagem; a cobertura de `bezel-media/src/lib.rs` cai pelos testes
  ffmpeg ignorados.
