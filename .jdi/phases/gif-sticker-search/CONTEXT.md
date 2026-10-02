# Phase 11: Busca de GIFs e stickers (KLIPY) e coleção — Context  (slug: gif-sticker-search)

## Goal
GIFs/stickers do KLIPY, com a chave do usuário, numa coleção local gerenciável e usados nos temas como imagens.

## Locked decisions
- D-1: pedido de 2026-10-01 (Tenor desligado, GIPHY pago).
- D-2: porta `GifSource`; `bezel-klipy` (`ureq` 3 + rustls) só no studio; `content_filter` `off`/`medium`; arquivos só de `static.klipy.com`, sem chave, com teto.
- D-3: chave em `<config>/klipy.json` (0600), fora da janela e dos logs; sem chave nada sai; "?" abre popover (4 passos + Painel).
- D-4: busca na Coleção: "Search KLIPY", explícitos desligado, nada ao abrir, debounce, "Carregar mais"; prévias `data:` por id (CSP igual); 429 = 100/h.
- D-5: `GifCollection`: maior GIF ≤ 25 MiB, 1 cópia por SHA-256; Mídia: "Deste tema" | "Coleção" (usar, arrastar, renomear, excluir).
- D-6: provas pelo caminho de produção; guia com Privacidade.
- D-7: API real: `format_filter` por tipo, still PNG no sticker, 404 = chave recusada.

## Canonical refs
- Card: pedido de 2026-10-01; `D-2026-10-01-gif-sticker-search-{1..15}`; docs.klipy.com

## Out of scope
- CLI, arquivos locais, WebP/MP4, Share/Report, desfazer, keyring: `.jdi/todos/2026-10-01-gif-sticker-search.md`.

## Definition of Done

### Auto-verifiable
- [ ] Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::gifs::tests::explicit_maps_to_the_filter domain::gifs::tests::picks_the_largest_gif_under_25_mib 2>&1 | grep -q 'ok. 2 passed' && cargo test -p bezel-core --locked --test gifs -- --exact keeps_one_copy_per_content refuses_a_download_that_is_not_a_gif renames_and_deletes_items 2>&1 | grep -q 'ok. 3 passed' && ! grep -rqiE 'klipy|TcpStream|UdpSocket|TcpListener|net::' crates/bezel-core && [ "$(cargo tree -p bezel-core --locked -e normal --depth 1 --prefix none | sed 1d | cut -d' ' -f1 | sort -u)" = thiserror ] && echo OK`
      **Source:** CONTEXT
- [ ] `KlipyClient` contra servidor HTTP loopback com JSON gravado
      **Verify:** `cargo test -p bezel-klipy --locked --lib -- --exact client::tests::searches_over_loopback_http client::tests::maps_429_and_refused_keys client::tests::stops_at_the_byte_limit client::tests::files_only_from_klipy_without_key client::tests::errors_hide_the_key client::tests::production_is_https_api_klipy_com 2>&1 | grep -q 'ok. 6 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual
      **Verify:** `cargo test -p bezel-studio --locked --lib -- --exact gifs::tests::no_request_at_start_or_without_key gifs::tests::key_never_reaches_the_window gifs::tests::only_items_of_the_last_search gifs::tests::sticker_alpha_shows_the_background gifs::tests::animated_gif_background_as_today gifs::tests::delete_names_themes_using_it backend::tests::partner_panel_is_the_only_new_link tests::every_command_is_allowed_by_name tests::the_app_setup_sends_nothing_at_start tests::nothing_in_the_app_forges_an_invocation tests::the_studio_installs_no_logger diag::tests::a_panic_says_where_never_what 2>&1 | grep -q 'ok. 12 passed' && cargo test -p bezel-media --locked --lib -- --exact collection::tests::saves_atomically_by_content collection::tests::unreadable_index_is_an_error 2>&1 | grep -q 'ok. 2 passed' && cd apps/bezel-studio/src-tauri && [ "$(grep '"csp"' tauri.conf.json)" = "$(git show origin/main:./tauri.conf.json | grep '"csp"')" ] && [ -z "$(git ls-files | grep -iE '^tauri.*\.(json5?|toml)$' | grep -vx tauri.conf.json)" ] && ! grep -qE '"(opener|http):' capabilities/*.json && echo OK`
      **Source:** CONTEXT
- [ ] `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls`
      **Verify:** `! git diff "$(git merge-base HEAD origin/main)" -- Cargo.lock | grep -qE '^-(version|source|checksum)' && cargo tree -i ureq --locked --target all -e normal --prefix none --depth 1 | awk '{print $1}' | sort -u | paste -sd' ' | grep -qx 'bezel-klipy ureq' && t=$(cargo tree -p bezel --locked --target all -e normal --prefix none) && ! grep -qE '^(ureq|rustls) ' <<<"$t" && echo OK`
      **Source:** CONTEXT
- [ ] UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe
      **Verify:** `set -o pipefail; cd apps/bezel-studio && node --test --test-name-pattern='debounce|grid arrows|load more|429' --test-reporter=tap tests/ui/gif-search.test.mjs | awk '/^# pass 4$/{p=1} END{exit !p}' && npm run test:unit >/dev/null && node scripts/e2e-passed.mjs "gif search › no key: help" "gif search › explicit off by default" "gif search › 429: 100 per hour" "gif collection › add, rename, delete" "gif collection › vertical and horizontal" "gif collection › reduced motion: stills" && echo OK`
      **Source:** CONTEXT
- [ ] Guia en/pt-BR com Privacidade, check-docs e CHANGELOG
      **Verify:** `grep -q '^### Privacy' docs/user/gifs-and-stickers.md && grep -q '^### Privacidade' docs/user/pt-BR/gifs-and-stickers.md && grep -q '"gifs-and-stickers.md"' scripts/ci/check-docs.sh && sed -n '/^## \[Unreleased\]/,/^## \[[0-9]/p' CHANGELOG.md | grep -q KLIPY && bash scripts/ci/check-docs.sh >/dev/null && echo OK`
      **Source:** CONTEXT
- [ ] CI do Windows verde no HEAD do PR
      **Verify:** `s=$(git rev-parse HEAD); i=$(gh run list -w CI -b jdi/gif-sticker-search -L 20 --json databaseId,headSha -q "map(select(.headSha==\"$s\"))[0].databaseId"); gh run view "$i" --json jobs -q '.jobs[]|select(.name|endswith("rust-windows")).conclusion' | grep -qx success && echo OK`
      **Source:** CONTEXT

### Manual
- _(none)_

## Deferred to PR review
- Uso real com a chave do usuário (feito; SUMMARY).
- Termos do KLIPY sobre guardar downloads.
- Visual: Coleção, diálogo e popover.
- Sticker na 8.8" real com transparência.

## Notes
- DoD 1, 3, 4, 5 apertadas após o crítico.
