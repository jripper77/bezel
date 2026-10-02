# Phase 11: Busca de GIFs e stickers (KLIPY) e coleção — Summary  (slug: gif-sticker-search)

**Status:** partial
**Tasks:** 8/9 complete, 0 blocked (T-8: hardware fica para o PR enquanto o studio do usuário segura a tela)

> `/jdi-issue` autônomo (pedido do usuário de 2026-10-01). Branch `jdi/gif-sticker-search` a partir de `main` com o
> PR #2 mergeado; um worktree por tarefa, cherry-picked. KLIPY porque o Tenor foi desligado (2026-06-30) e a produção
> do GIPHY é paga; chave do próprio usuário.

## Executed tasks
- T-1 `e570657`: core — `domain::gifs` (filtro explícito, maior GIF ≤ 25 MiB, `Collection`), portas `GifSource` e
  `GifCollection`, `app::gifs` (busca, prévia, adicionar, renomear, excluir confirmado), `BezelError::Service`, 6
  códigos novos; `MemoryCollection` e `FakeGifSource` em `bezel-media`.
- T-7 `2581df6`: guia en/pt-BR "GIFs and stickers" (chave grátis passo a passo, limites, Privacidade), índices,
  `check-docs.sh`, CHANGELOG, README.
- T-2 `e4a14a7`: `bezel-klipy` — `KlipyClient` em `ureq` 3 + rustls (sem redirect, arquivos só de `static.klipy.com`
  sem a chave e com teto, erros sem a chave), testado contra HTTP em loopback; Cargo.lock só ganha pacotes.
- T-3 `7d5f282`: `DiskCollection` — `collection.json` + `files/<sha256>.gif` + `previews/`, gravação atômica, índice
  ilegível é erro (a coleção nunca some calada).
- T-5 `ab085d7`: UI — diálogo "Buscar GIFs e stickers" (placeholder "Search KLIPY", "Powered by KLIPY", explícitos
  desligado, debounce, "Carregar mais", grade por setas, aria-live), chave com popover "?" (4 passos + Painel de
  Parceiros), demo `gifs`/`gifsNoKey`/`gifsRateLimited`.
- T-2b `6b6e79a` (do orquestrador, após a checagem real com a chave do usuário): `format_filter` por tipo (GIF
  `gif,jpg`, sticker `gif,png` — sticker não tem JPEG e voltava vazio), still PNG nos stickers, chave inválida (HTTP
  404) = `KeyRejected`, fixtures com respostas reais aparadas, teste real `#[ignore]` com `BEZEL_KLIPY_KEY`; D-7.
- T-4 `4449005`: studio — estado `Gifs`, chave em `<config>/klipy.json` (0600; a janela só vê os 4 últimos), 12
  comandos + `open_link` fixo para partner.klipy.com, cache da sessão, prévias `data:` com o MIME dos bytes, usar como
  imagem/fundo pelos caminhos de hoje, `log` limitado a debug (o trace do ureq traria a chave).
- T-6 `4094f49`: UI — Coleção na aba Mídia (Todos/GIFs/Stickers, usar, arrastar, renomear, excluir com os temas que
  usam o item), still com movimento reduzido.
- Iters 2–5 (críticos: linhas ocas e objetivas a cada rodada): o `setup` real num teste (`f3bd567`), coleção
  inutilizável dita (`6ffa470`), chave curta (`f9bbc24`), `e2e-passed.mjs` (cada teste nos 4 projetos com axe), Privacidade
  exata e exigida pelo `check-docs`, teste do mock só fora do Windows (D-8), grep de TODO e linhas 1/3/4/5 apertadas
  (D-9), token `UserAsked` de `tauri::ipc::Request`, debounce/setas/i18n provados pela UI, guarda de fonte (`ce4ade5`).
- --- AUTO-RESET 1 (teto de 5 iterações; o crítico da iter 5 achou a chave num `eprintln!` e um anúncio com texto
  solto) --- Rodada 2, iter 1: D-10; `KlipyKey` (sem `Display`, `Debug` mascarado, um só leitor: imprimir a chave não
  compila), a guarda de fonte lê identificadores com `syn` (pega `r#eval`, macros, `with_webview`), os helpers da UI só
  aceitam chaves de i18n (texto solto = chave desconhecida) e o e2e confere `keySavedNow`, `keyRemoved` e
  `collection.using` no idioma.
- Rodada 2, iter 2 (crítico: `expose_secret` num print e o token feito por outro comando): `e99c81b` — a chave só é
  lida como argumento direto do cliente e na gravação do arquivo, nunca dentro de macro; `UserAsked::of` só nos 3
  comandos de GIF (renomear ou embrulhar também falha); `navigate` e literais `javascript:` recusados.
- Rodada 2, iter 3 (D-11): `a603c45` — comandos só via IPC, `Request` só nos 3 comandos de GIF, sem print em `commands.rs`.
- Rodada 2, iter 4 (D-12): `1b66167` — logs só pelo módulo `diag` (38 códigos fixos; os textos de erro saem dos logs),
  sem `payload`/`Invoke`, sem panic com valor; variável local com nome de comando aceita.
- Rodada 2, iter 5 (D-13): `6f4afe0` — o arquivo da chave guarda `KlipyKey` (nenhuma `String` com a chave), 12 arquivos
  danificados testados; `2d1272d` — códigos fixos dizem por que o app não abriu (guia de problemas).
- --- AUTO-RESET 2 --- Rodada 3, iter 1 (D-14): `c05f5b1` — sem logger nem feature `tracing` do Tauri no studio (manifesto e
  features resolvidas pelo `cargo metadata`), panic hook que diz só onde; `86f5c99` — guia de problemas com o que o usuário vê.
- Rodada 3, iter 2 (D-15): `0bfa420` — sem sockets no core (linha 1) nem no studio; `Command` só na reabertura do próprio
  app; o navegador do sistema só abre por `open_link`/`open_guide` (clique do usuário).

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/{domain/{gifs,clock,mod},ports/mod,app/{gifs,mod},error}.rs`, `crates/bezel-core/tests/gifs.rs`
- `crates/bezel-klipy/**` (novo, com `fixtures/`), `Cargo.toml`, `Cargo.lock`
- `crates/bezel-media/src/{lib.rs,collection/**,archive/{mod,disk}.rs}`
- `apps/bezel-studio/src-tauri/{Cargo.toml,build.rs,capabilities/default.json}`, `src-tauri/src/**` (gifs, `gifs/{key,asked,tests}`,
  `diag`, commands, lib, backend, dto, messages e os módulos que passaram a logar pelo `diag`)
- `apps/bezel-studio/src/{gif-search,collection,demo-gifs,demo-backend,demo-data,bridge,app}.js`,
  `src/ui/{gif-search,collection,library}.js`, `index.html`, `styles.css`, `i18n/{en,pt-BR}.js`, `tests/**`
- `docs/user/{,pt-BR/}{gifs-and-stickers,README,troubleshooting}.md`, `scripts/ci/check-docs.sh`, `CHANGELOG.md`, `README.md`

## Tests
- `cargo test --workspace --locked`: 945 passando, 0 falhando, 12 ignorados (hardware, ffmpeg real e o KLIPY real)
- DoD 1–6: OK no branch combinado a cada iteração; DoD 7: `rust-windows` verde (iter 3: 153 testes do studio no Windows)
- UI: 231 unitários; Playwright 208/208 (claro/escuro × pt-BR/en, axe), 24 novos
- fmt, clippy `-D warnings` (Linux e `--target x86_64-pc-windows-msvc` sem studio e klipy: o `ring` não compila
  para MSVC aqui; o `rust-windows` do CI cobre), `check-docs.sh`, `check-packaging.sh`, `cargo audit`
- Cobertura: `bezel-klipy` 97%, studio `gifs.rs` 96,87%, `collection.js` 100%
- Testes não ocos: cada teste do DoD caiu com a regra removida (mutações por tarefa: 6 no core, 8 no cliente, 3 no
  disco, 10 no studio, 12 + 14 na UI)

## Integração real (KLIPY)
- Com a chave de teste do usuário (guardada só no scratchpad, 0600; nenhum commit a contém — `git log -p` conferido):
  busca e em alta de GIFs e stickers, download de `static.klipy.com` (GIF, `image/gif`, sem redirect) e chave
  inválida (404). Achou 2 bugs que os fixtures inventados escondiam (T-2b). `real_klipy_answers_with_the_users_key`
  passou (5 requisições). ~16 requisições no total, do limite de 100/h.

## Hardware validation
- T-8 (orquestrador): sticker da coleção na 8.8" — pendente enquanto o studio do usuário segura a tela; a
  transparência já funciona no caminho de hoje (o usuário usou um sticker transparente) e
  `sticker_alpha_shows_the_background` a prova no render.

## Observações
- Termos de API do KLIPY sobre guardar downloads não são públicos: cada item guarda a origem (id e URL).
- Windows: `bezel-klipy` (ring) e o arquivo da chave só são compilados e testados no CI.
