# Phase 11: Busca de GIFs e stickers (KLIPY) e coleção — Plan  (slug: gif-sticker-search)

## Goal
Buscar GIFs/stickers no KLIPY com a chave do usuário, guardá-los numa coleção local gerenciável e usá-los nos temas como as imagens de hoje.

## Locked decisions (from CONTEXT.md)
- D-2026-10-01-gif-sticker-search-1..6; herdadas: D-1, video-background-framing-7, release-polish-6

## Contrato UI ↔ backend (fixo: T-4 implementa; T-5, T-6 e `demo-gifs.js` seguem)
JSON camelCase; erros `{code,args,message}`.
- `klipy_key` → `KeyDto {configured, last4|null}`; `save_klipy_key {key}` → `KeyDto` (sem rede; `invalidInput` fora de `[A-Za-z0-9_-]{1,128}`); `remove_klipy_key` → `KeyDto`
- `search_gifs {kind:'gif'|'sticker', text, page, explicit}` → `GifPageDto {kind, text, page, hasNext, items:[{id, title, width, height}]}` (texto vazio = em alta)
- `gif_preview {id, still}` → `data:` URL|null (GIF `sm`; `still` = JPEG); `collect_gif {id}` → `CollectedDto`; ambos só com ids das páginas da última busca, senão `gifNotInResults {item}`
- `gif_collection {still}` → `CollectedDto[]`; `rename_collected {id, name}` → `CollectedDto`; `collected_users {id}` → `{themes: string[], openTheme: boolean}`; `delete_collected {id, confirmed}` → `null` (`notConfirmed` sem confirmar); `use_collected {id, target:'image'|'background'}` → `AddedMediaDto` de hoje
- `open_link {link:'klipyPartnerPanel'}`; `open_guide` aceita `gifs-and-stickers`
- `CollectedDto {id: sha256, name, kind, width, height, bytes, addedAt, source: {provider, id, url|null}, preview|null}`
- Códigos: `klipyNoKey`, `klipyKeyRejected`, `klipyRateLimited`, `klipyUnavailable {detail}`, `gifNotInResults {item}`, `notInCollection {item}`; não-GIF e nome vazio = `invalidInput`

## Tasks

### Wave 1 (paralela)

#### T-1: Core: `domain::gifs`, portas, `app::gifs`, fakes e códigos
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/{domain/gifs,domain/mod,domain/error,ports/mod,app/gifs,app/mod}.rs`, `crates/bezel-core/tests/gifs.rs`, `crates/bezel-media/src/{lib.rs,collection/{mod,memory,fake}.rs}`, `apps/bezel-studio/{src-tauri/src/messages.rs,tests/ui/fixtures/backend-codes.json,src/i18n/{en,pt-BR}.js}`
- **Acceptance:**
  - `domain::gifs`: `Explicit` → `ContentFilter {Medium, Off}`, `GifQuery {kind, text, page, explicit, language}`, `Rendition`, `GifItem`, `GifPage`, `CollectedGif {content: ContentId, name, kind, width, height, bytes, added_at, origin}`; `download_rendition` = maior GIF (área) ≤ `REV_C_MAX_UPLOAD_BYTES`; `is_gif`; `PAGE_SIZE = 24`, `PREVIEW_LIMIT = 2 MiB`.
  - `GifSource: Send + Sync` (`provider`, `page`, `download(&Rendition, limit)`, `&self`); `GifCollection` no molde de `ArchiveStore` (+ `keep_preview`/`read_preview`; `discard` leva os dois). `BezelError::Service(ServiceFailure {RateLimited, KeyRejected, Unavailable(String)})` mapeado em `messages.rs`; os 6 códigos na fixture e `error.*` en/pt-BR (429 cita 100 por hora e o Painel).
  - `app::gifs`: `search`; `add_to_collection` (teto no download, não-GIF = `InvalidInput` sem guardar, mesmo conteúdo devolve o item existente, prévia dada se GIF senão a `sm`); `rename`; `delete(.., Confirm)`. `bezel_media::collection::{MemoryCollection, FakeGifSource}` gravam chamadas. Core sem I/O nem nome do fornecedor.
  - Mutação: cada teste do DoD 1 falha sem a regra que nomeia.
- **Dependencies:** none
- **Test:** `domain::gifs::tests::{explicit_maps_to_the_filter,picks_the_largest_gif_under_25_mib}`; topo de `tests/gifs.rs`: `keeps_one_copy_per_content`, `refuses_a_download_that_is_not_a_gif`, `renames_and_deletes_items`
- **Status:** pending

#### T-7: Guia en/pt-BR, check-docs, CHANGELOG, README
- **Specialist:** jdi-doer-bezel
- **Files modified:** `docs/user/{gifs-and-stickers,README}.md`, `docs/user/pt-BR/{gifs-and-stickers,README}.md`, `scripts/ci/check-docs.sh`, `CHANGELOG.md`, `README.md`
- **Acceptance:**
  - Chave (4 passos, `partner.klipy.com`, teste = 100 por hora), busca (`Search KLIPY`), explícitos, coleção, limite e `### Privacy`/`### Privacidade` (o que vai, a quem: `api.klipy.com`/`static.klipy.com`, quando, onde a chave fica, como remover); ligadas à tradução e aos índices; nada casa `PRIVATE`.
  - `check-docs.sh`: página em `PAGES`, as 4 frases, títulos em `HEADINGS`, `KLIPY` no `[Unreleased]`; CHANGELOG `### Added` e README citam busca e coleção.
- **Dependencies:** none
- **Test:** DoD 6
- **Status:** pending

### Wave 2 (paralela)

#### T-2: `bezel-klipy`: `KlipyClient` em `ureq` 3/rustls
- **Specialist:** jdi-doer-bezel
- **Files modified:** `Cargo.toml`, `Cargo.lock`, `crates/bezel-klipy/{Cargo.toml,src/lib.rs,src/client.rs,src/dto.rs,fixtures/*.json}`
- **Acceptance:**
  - Primeiro: `ureq` 3 (`default-features = false, features = ["rustls"]`), `getrandom` 0.3 (`new_customer_id`), sem `cargo update`; `git diff Cargo.lock | grep '^-version'` vazio (senão fixar a versão nova); `cargo audit` limpo; DoD 4 OK.
  - Base `https://api.klipy.com/api/v1`, arquivos só de `https://static.klipy.com` (troca `pub(crate)` só para teste); `max_redirects(0)`, 10 s, `http_status_as_error(false)`, `https_only`; query de D-2; JSON tolerante; corpo com `limit()`; erro do `ureq` nunca vira texto (a URL tem a chave); `Debug` manual.
  - Mutação: os 6 testes rodam o cliente real contra `TcpListener` 127.0.0.1 que grava a requisição e serve os fixtures; chave falsa óbvia.
- **Dependencies:** T-1
- **Test:** `client::tests::{searches_over_loopback_http, maps_429_and_refused_keys (+302 não seguido), stops_at_the_byte_limit, files_only_from_klipy_without_key (host alheio sem conexão), errors_hide_the_key, production_is_https_api_klipy_com}`
- **Status:** pending

#### T-3: `DiskCollection` em `bezel-media`
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-media/src/collection/{mod,disk,tests}.rs`, `crates/bezel-media/src/archive/disk.rs`
- **Acceptance:** `collection_dir(data)` = `<data>/bezel/collection`: `collection.json` (com `schema`), `files/<sha256>.gif`, `previews/<sha256>.gif`; reusa `write_atomically` (`pub(crate)`); ilegível = erro com o caminho, ausente = vazia; caminhos só juntados (Windows).
- **Dependencies:** T-1
- **Test:** `collection::tests::saves_atomically_by_content` (save que falha deixa o índice inteiro e nenhum `.tmp`; conteúdo 1 vez; discard), `collection::tests::unreadable_index_is_an_error`; mutação: sem escrita atômica, dedupe ou erro de índice, falham
- **Status:** pending

#### T-5: UI: diálogo de busca, chave e ajuda, demo
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/` + `src/{gif-search,demo-gifs,demo-backend,demo-data,bridge,app}.js`, `src/ui/{gif-search,library}.js`, `src/{index.html,styles.css}`, `src/i18n/{en,pt-BR}.js`, `tests/ui/{gif-search,demo-backend}.test.mjs`, `tests/e2e/gif-search.spec.mjs`
- **Acceptance:**
  - Mídia: subabas "Deste tema" | "Coleção" e "Buscar GIFs e stickers". Diálogo de D-4 em 960x600, chave no topo (senha, `autocomplete=off`, Salvar/Remover, `last4`); "?" = disclosure de D-3 (`aria-expanded`, Esc/clique fora, foco volta; Painel por `openLink`, guia); `bridge.js` com os 12 métodos.
  - Nada ao abrir; prévias `still` com movimento reduzido, ≤ 6 em voo; 429 = 100/h + Painel; chave recusada abre a ajuda. `src/gif-search.js` puro; `demo-gifs.js`: `gifs`, `gifsNoKey`, `gifsRateLimited`, `data-demo-gif-query`, `data-demo-link`. i18n com paridade.
- **Dependencies:** T-1
- **Test:** `gif-search.test.mjs` com exatamente 4 testes casando `debounce|grid arrows|load more|429`; Playwright `gif search` › `no key: help`, `explicit off by default`, `429: 100 per hour` × 4 projetos com `watchErrors` e `expectAccessible`; `npm test` (≥ 80%)
- **Status:** pending

### Wave 3 (paralela)

#### T-4: Studio: estado `Gifs`, chave, comandos e links
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/` + `{Cargo.toml,build.rs,capabilities/default.json}`, `src/{gifs,commands,lib,backend,dto}.rs`, `src/gifs/{key,tests}.rs`; `Cargo.lock`
- **Acceptance:**
  - `Gifs` é estado próprio do Tauri (`Backend` e suas 4 construções intactos): `<config>/klipy.json` `{key, customerId}` atômico, 0600 no Unix; fábrica `Fn(key, customer) -> Arc<dyn GifSource>`; cache da sessão por (kind, texto, filtro, idioma, página) e itens da última busca por id; `DiskCollection`. Rede fora de lock; `compose` não pede nada.
  - Imagem por `add_image_bytes`; fundo por `add_media` de `<cache>/collection/<nome>.gif`; `collected_users` = temas do usuário e o aberto com o mesmo SHA-256. `LINKS = [("klipyPartnerPanel", "https://partner.klipy.com")]`, `GUIDE_PAGES += gifs-and-stickers`. 12 comandos em `build.rs`, capability e `generate_handler!`; CSP igual.
  - Mutação: cada teste falha sem a regra que nomeia.
- **Dependencies:** T-1, T-2, T-3
- **Test:** `gifs::tests::{no_request_at_start_or_without_key, key_never_reaches_the_window, only_items_of_the_last_search, sticker_alpha_shows_the_background (render real, vertical e horizontal), animated_gif_background_as_today, delete_names_themes_using_it}`, `backend::tests::partner_panel_is_the_only_new_link`, `tests::every_command_is_allowed_by_name` (exige os 12 e cada um no `generate_handler!`)
- **Status:** pending

#### T-6: UI: lista da Coleção
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/` + `src/{collection,demo-gifs,app}.js`, `src/ui/{collection,library}.js`, `src/{index.html,styles.css}`, `src/i18n/{en,pt-BR}.js`, `tests/ui/{collection,demo-backend}.test.mjs`, `tests/e2e/gif-search.spec.mjs`
- **Acceptance:** D-5: filtro, contagem e tamanho; imagem no centro, fundo (`backgroundOf`), arrastar (`makeDraggable`), renomear inline, excluir por `askChoice` com os temas de `collectedUsers`; `still` com movimento reduzido; `demo-gifs.js` completa o contrato; lógica em `src/collection.js`.
- **Dependencies:** T-5
- **Test:** `collection.test.mjs`; Playwright `gif collection` › `add, rename, delete`, `vertical and horizontal`, `reduced motion: stills` × 4 projetos com axe; `npm test`
- **Status:** pending

### Wave 4

#### T-8: HARDWARE — sticker na 8.8" (orquestrador)
- **Specialist:** orquestrador na Turing 8.8" real
- **Files modified:** `.jdi/phases/gif-sticker-search/SUMMARY.md` (§ Hardware validation)
- **Acceptance:** nada em `/dev/ttyACM*` enquanto o studio do usuário segura a tela. Liberada, após o `install-local`: tema horizontal e vertical com sticker transparente sobre cor; estado restaurado. Senão: Deferred to PR review (não `blocked`).
- **Dependencies:** T-1..T-7
- **Test:** itens de "Deferred to PR review"
- **Status:** pending

## Execution
- 8 tasks em 4 waves (2 → 3 → 2 → 1); worktree por task com `CARGO_TARGET_DIR` próprio; cherry-pick por wave. `install-local` pelo orquestrador antes da T-8.
- Speedup paralelo estimado: 2x

## DoD → task
1 T-1 · 2 T-2 · 3 T-4 e T-3 · 4 T-2 · 5 T-5 e T-6 · 6 T-7 · 7 todas (orquestrador após o push) · Deferred T-8 + PR

## Files modified (all tasks)
- Os listados em cada task (nenhum repetido dentro de uma wave).

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`
- Windows local: `cargo clippy --workspace --exclude bezel-studio --exclude bezel-klipy --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings` (ring pede `lib.exe`, ausente aqui); `bezel-klipy` e studio no `rust-windows` do CI
- `cargo audit`; `npm test`; `check-docs.sh`; `cargo llvm-cov --workspace --locked --fail-under-lines 80`

## Notes
- Nomes do DoD exatos (`--exact`) no `mod tests` do módulo (studio: `gifs/tests.rs`). `FakeGifSource` em `bezel-media`: o `grep` do DoD 1 veta `klipy` em `crates/bezel-core`.
- Commits conventional, escopo `gif-sticker-search`; `Cargo.lock` com a task que o muda.
- Fora: `.jdi/todos/2026-10-01-gif-sticker-search.md`.
