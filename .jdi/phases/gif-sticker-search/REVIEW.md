# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 1 do loop autônomo (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` =
> `8df7ffa` (igual ao remoto). Escopo: `git diff origin/main...HEAD` inteiro (76 arquivos: `crates/`, `apps/`,
> `docs/`, `scripts/`, `CHANGELOG.md`, `README.md`, `Cargo.toml`, `Cargo.lock`), as 7 decisões
> `D-2026-10-01-gif-sticker-search-{1..7}` em texto integral, CONTEXT, PLAN, SUMMARY e LOOP. Árvore limpa durante a
> revisão (só o `LOOP.md` do orquestrador como não rastreado).
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura), nenhum copiado do SUMMARY.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado nem `api.klipy.com`/`static.klipy.com` (o teste
>   real `#[ignore]` não rodou; os testes do cliente usam o servidor de loopback). Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** numa cópia de `HEAD` no scratchpad (`git archive`), alvo próprio `target/review-mut` (apagado no
>   fim). Cada mutação editou o arquivo depois do build anterior, e cada uma derrubou só os testes da sua regra, o
>   que mostra que foi recompilada. O repositório não foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 (o `Cargo.lock` passa no `--locked`) |
| Tests | PASS | **930 passed, 0 failed, 12 ignored** (39 binários). Os ignorados: 8 de ffmpeg real, 1 de timing, 1 de hardware, 1 do corpus local e o novo `client::tests::real_klipy_answers_with_the_users_key` (rede real, `BEZEL_KLIPY_KEY`). +51 sobre os 879 da fase anterior; bate com o SUMMARY |
| Coverage | PASS | **94.20%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT, sem filtro: **94.15%**. Arquivos novos: `domain/gifs.rs` e `app/gifs.rs` 100%, `bezel-klipy` `client.rs` 90.04% (o resto é o teste real ignorado) e `dto.rs` 99.16%, `collection/disk.rs` 97.32%, studio `gifs.rs` 96.87% e `gifs/key.rs` 89.89% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `--target x86_64-pc-windows-msvc --exclude bezel-studio --exclude bezel-klipy` (o `ring` não compila para MSVC aqui): exit 0. Nenhum `allow` novo fora de `tests/` (o de `crates/bezel-core/tests/gifs.rs:4` segue o padrão dos outros arquivos de teste de integração) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 limpos (detalhe abaixo). `cargo audit`: exit 0 (1279 advisories, 620 crates) |
| Consistency | PASS | 13 commits, todos com escopo `gif-sticker-search` (mais o `chore(jdi)` de abertura); arquivos batem com o PLAN (fora dele só o `Hash` em `domain/clock.rs`, o reexport de `archive/mod.rs` e uma linha de `tests/ui/video-framing.test.mjs`, todos consequência direta das tasks). D-1..D-7 cumpridas; nenhuma D-XX anterior contrariada (detalhe abaixo) |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **207/207**, 99.94% de linhas (`gif-search.js` 100%, `collection.js` 100%, `demo-gifs.js` 100%). Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe); `-g "gif search\|gif collection"`: 24 passed |
| DoD | PASS | As 7 linhas Auto do CONTEXT (a 7 com o CI **36948045888** no `HEAD` `8df7ffa`) e as 3 Auto do PROJECT passam como escritas. O CONTEXT não tem Manual; os 2 Manual do PROJECT são do corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. `app::gifs` recebe a hora do chamador (`added_at`) |
| 5.3 ports | PASS. `GifSource` e `GifCollection` estão em `bezel_core::ports` (`ports/mod.rs:218`, `:241`); nenhuma impl de porta no core; nenhuma `pub trait` nova fora do core |
| 5.4 adapters na composição | PASS. `KlipyClient::new`, `DiskCollection::open` e `MemoryCollection::new` fora de teste só em `apps/bezel-studio/src-tauri/src/lib.rs:416-428` |
| 5.5 `unsafe` | PASS: nada novo (`bezel-klipy` tem `#![forbid(unsafe_code)]`) |
| 5.6 panics | PASS: os `unwrap`/`expect` novos estão todos em `mod tests` (`backend.rs`, `lib.rs`, `domain/gifs.rs`, `client.rs`, `dto.rs`, `fake.rs`, `memory.rs`). O código de produção usa `unwrap_or_else(PoisonError::into_inner)` nos locks |
| 5.7 escrita no dispositivo | PASS: os `Confirm::Yes` novos são doc ou teste; `delete_collected` só confirma pelo `confirmed` do diálogo (`commands.rs`), e o use case recusa `Confirm::No` antes de ler |
| 5.8 protocolo | PASS: nada em `protocol/` nem em `docs/reverse-engineering/` mudou |
| 5.9 caminhos no core | PASS: nada novo; o DoD 1 confirma que `crates/bezel-core` não cita `klipy`, `ureq` nem `reqwest` |
| 5.10 comandos síncronos | PASS: os 12 comandos novos são `async` e rodam no pool de bloqueio (`with_gifs` → `blocking`); os síncronos listados são anteriores |
| 5.11 supply chain | PASS: `cargo audit` exit 0; nenhum segredo (as chaves dos testes são `test-key`, `fake-KLIPY_key-...` e `demo-demo-demo-a1b2`; os fixtures só têm URLs de `static.klipy.com` e o `blur_preview` em base64) |

### Dependências novas (`Cargo.lock`)
Só entradas: `bezel-klipy`, `ureq 3.4.2`, `ureq-proto 0.6.4`, `rustls 0.23.45`, `rustls-pki-types 1.15.1`,
`rustls-webpki 0.103.15`, `ring 0.17.14`, `untrusted 0.9.0`, `webpki-roots 1.0.9`, `subtle 2.6.1`, `zeroize 1.9.0`,
`utf8-zero 0.8.1`; nenhuma linha `-version`. Licenças (do `Cargo.toml` de cada crate): MIT/Apache-2.0 (ureq,
ureq-proto, pki-types, utf8-zero, zeroize), Apache-2.0/ISC/MIT (rustls), ISC (rustls-webpki, untrusted),
Apache-2.0 AND ISC (ring), BSD-3-Clause (subtle) e **CDLA-Permissive-2.0** (webpki-roots, dados das raízes da
Mozilla): todas compatíveis com GPL-3.0-or-later. `cargo tree` confirma `ureq` só via `bezel-klipy`, e nem `ureq`,
`rustls`, `ring` nem `webpki-roots` na árvore da CLI `bezel`.

## Segurança da chave e da rede (revisão além dos gates)
Conferido na fonte, e onde dava, por mutação:
- **A chave nunca volta à janela.** `KeyDto` só tem `configured` e `last4` (e `last4` só para chaves de 9+
  caracteres, `gifs/key.rs:69-72`); `SavedKey`, `KlipyClient` e `Gifs` têm `Debug` manual sem a chave; o erro de um
  `klipy.json` ilegível descarta o texto do serde, que poderia citar o valor (`key.rs:108-122`); a chave inválida
  recebida não é ecoada (`invalid_key`). O teste `key_never_reaches_the_window` serializa toda resposta e erro que a
  janela recebe, incluindo as 3 falhas do serviço, e procura a chave.
- **Logs.** O studio não instala nenhum logger (`log` ou `tracing`), e mesmo assim o `log` vem com
  `max_level_debug`/`release_max_level_debug` (`src-tauri/Cargo.toml:36`): `cargo tree -e features -i log` mostra as
  duas features no `log` do alvo do studio, e o teste `the_http_client_cannot_log_request_paths` prova
  `STATIC_MAX_LEVEL <= Debug` no próprio build. Na fonte do `ureq 3.4.2`, o único ponto que imprime o caminho é o
  `DebugUri`, que só escreve `path_and_query` com `log_enabled!(Trace)` (`util.rs:272-277`) e senão `/******`; o
  `ureq-proto 0.6.4` só despeja os bytes da requisição em `trace!` (`util.rs:73-76`). Com o teto em debug, os dois
  ficam fora do binário.
- **Erros.** `failure()` (`client.rs:279-299`) troca todo `ureq::Error` por um texto fixo; o `RequireHttpsOnly(uri)`
  do ureq, que carrega a URL inteira, nunca vira texto. **Mutação M1** (`unavailable(&error.to_string())`): caem
  `errors_hide_the_key` e `stops_at_the_byte_limit`.
- **Arquivo.** `<config>/klipy.json` ao lado do `settings.json` (`lib.rs:354` e `:430`), gravado num temporário
  `create_new` com `0o600` e renomeado por cima (`key.rs:153-184`); um `klipy.json` antigo com outro modo é
  substituído por um inode novo `0600`. **M3** (`0o644`): cai `key_never_reaches_the_window`. No Windows não há ACL
  própria: vale a do perfil (`%APPDATA%`), como D-3 e o guia dizem.
- **Coleção e DTOs.** `collection.json` guarda provedor, id e a página `https://klipy.com/{gifs|stickers}/{slug}`
  (slug validado em `dto.rs:159-164`), nunca uma URL de arquivo nem da API; `CollectedDto.source.url` é essa página.
- **SSRF.** A janela só nomeia resultados por id (`gif_preview`, `collect_gif`) e só os das páginas da última busca
  (`Searches::item`, `gifs.rs:152-157`; **M4**, sem zerar os itens a cada busca nova: cai
  `only_items_of_the_last_search`). Os endereços vêm só da resposta da API, filtrados duas vezes para
  `https://static.klipy.com/` (`dto.rs:142-144` e `file_address`, `client.rs:144-155`, que recusa `@`, `:porta`,
  sufixo de host e `http`). `open_link` e `open_guide` só abrem endereços fixos; a capability não ganhou `opener:` nem
  `http:` e a CSP é a mesma (DoD 3).
- **Transporte.** HTTPS only, rustls com raízes webpki, `max_redirects(0)` (**M2**, 5 redirects: caem
  `production_is_https_api_klipy_com` e `maps_429_and_refused_keys`, que também prova que o host do `Location` não
  recebeu conexão), `timeout_global` de 10 s, corpo limitado (4 MiB na API, 2 MiB na prévia, 25 MiB no item) e um
  `Content-Length` maior recusado antes de ler. A chave é um único segmento percent-encoded do caminho.
- **D-7.** `format_filter` por tipo (`gif,jpg` / `gif,png`; **M7**, sticker com `gif,jpg`: caem
  `searches_over_loopback_http` e `locale_and_filter_follow_the_query`), still PNG dos stickers com o MIME dos bytes
  (`stills_take_the_media_type_of_their_bytes`, `a_sticker_still_is_its_png`) e 404 da API = `KeyRejected` (**M8**,
  sem o 404: cai `maps_429_and_refused_keys`). Os fixtures são respostas reais aparadas, sem chave.

## Privacidade, coleção e UI
- **Nada sai sem ação.** A composição só monta o estado (`lib.rs:413-436`); a fonte é criada na primeira busca,
  prévia ou adição com chave; salvar a chave não pede nada; abrir o diálogo só chama `klipy_key` (local); a
  Coleção só lê o disco. Provado em `no_request_at_start_or_without_key` e, na UI, por `data-demo-gif-query` nulo
  após abrir e após salvar a chave (e2e).
- **Coleção.** Conteúdo por SHA-256 (`keep` não regrava bytes iguais), índice e arquivos por `write_atomically`,
  índice ilegível = erro com o caminho e nada é salvo por cima (`add`, `rename` e `delete` carregam antes; **M5**,
  índice ilegível virando coleção vazia: cai `unreadable_index_is_an_error`). Caminhos só de `ContentId` (64 hex) e
  do `file_stem` (letras e dígitos, até 64): nenhuma travessia. Excluir salva o índice antes de apagar os bytes e o
  diálogo nomeia os temas do usuário e o aberto que têm os mesmos bytes (`delete_names_themes_using_it`). Não-GIF é
  recusado sem guardar (**M6**: cai `refuses_a_download_that_is_not_a_gif`).
- **UI.** Placeholder e nome acessível "Search KLIPY" nas duas línguas, "Powered by KLIPY" visível, explícitos
  desligados a cada início e mantidos na sessão, popover "?" como disclosure (`aria-expanded`/`aria-controls`, Esc
  fecha só ele, clique fora, foco volta ao "?"), 429 com 100/h e o Painel, chave recusada abre a ajuda, grade com
  roving tabindex, `aria-live`, stills com movimento reduzido e animações só em `prefers-reduced-motion:
  no-preference`. Tudo coberto pelos 6 testes e2e × 4 projetos com axe e `watchErrors`. Paridade i18n pelo teste
  existente. Nada de ternário aninhado, `force: true`, `RegExp` de variável ou promessa solta nos arquivos novos
  (as chamadas `void` têm `catch` dentro).

## Blockers
- Nenhum.

## Warnings
- **W1 — A seção Privacidade do guia erra os formatos pedidos aos stickers.** `docs/user/gifs-and-stickers.md:162` e
  `docs/user/pt-BR/gifs-and-stickers.md:169` dizem que o Bezel pede "GIF, and JPEG stills" / "GIF, e imagens paradas
  em JPEG"; desde D-7 os stickers pedem `gif,png` (`client.rs:334-339`) e o still de um sticker é PNG. É a seção
  que D-6 manda listar exatamente o que é enviado; trocar por "GIF, with JPEG stills for GIFs and PNG stills for
  stickers" nas duas línguas.
- **W2 — Uma coleção que não abre vira, calada, uma coleção em memória.** `lib.rs:415-422`: se
  `DiskCollection::open` falha (pasta sem permissão, disco cheio), o studio usa `MemoryCollection` e só registra
  `tracing::error!`, que nenhum subscriber do studio mostra. A janela exibe uma coleção vazia mesmo havendo
  `collection.json`, e o que o usuário adicionar some ao fechar, sem aviso: o oposto do "a collection never
  vanishes" de D-5 (que o caso do índice ilegível cumpre). Repete o padrão de `copies()` (`lib.rs:398-405`), por
  isso não bloqueia; o mínimo é a Coleção dizer que está guardando só nesta sessão.
- **W3 — Chave curta salva mostra "Saved key ending in " sem nada.** `gif-search.js:267` passa `last4: ''` quando o
  backend devolve `last4: null` (chaves de 1 a 8 caracteres passam no `is_valid_key`), e `gifs.keySaved` (en e
  pt-BR) termina vazio. Uma chave do KLIPY é longa, mas uma colagem errada curta chega aqui; usar um texto
  "Key saved" sem final para `last4 === null`.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (comando como escrito): `ok. 2 passed` na lib e `ok. 3 passed` em `tests/gifs.rs`; `grep` vazio. M6 derruba `refuses_a_download_that_is_not_a_gif` |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` (`ok. 6 passed`). M1, M2, M7 e M8 derrubam testes desta linha |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, só itens da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK` (`ok. 8 passed` no studio, `ok. 2 passed` no `bezel-media`, CSP e capability conferidas). M3, M4 e M5 derrubam testes desta linha |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK`. Nenhuma linha `-version`; também sem `ring`/`webpki-roots` na CLI |
| 5 | UI: `gif-search.test.mjs`, i18n, Playwright 4 projetos + axe | CONTEXT | Auto | PASS | `OK` com `BEZEL_E2E_PORT=1442` (rodado em `bash -c` com as aspas trocadas, por causa do zsh; mesma semântica): `# pass 4`, `test:unit` 207/207, `-g "gif search\|gif collection"` 24 passed |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK`; `check-docs.sh` exit 0. Ver W1 sobre o conteúdo da seção |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run terminar (`gh run watch 36948045888 --exit-status`: exit 0) e rodei o comando como escrito com `HEAD` = `8df7ffa` → `OK` (achou o run **36948045888**, `completed success`). `rust-windows`: fmt, clippy e `cargo test` com cobertura verdes, **863 passed, 0 failed, 12 ignored**, com `bezel-klipy` (ring/rustls para MSVC), `gifs::tests::key_never_reaches_the_window` (o `create_private` de `cfg(not(unix))`) e `collection::tests::saves_atomically_by_content` (o bloqueio por `share_mode`) ok; empacotamento ok. Verdes também `rust-linux`, `node-ui`, CodeQL (rust, js, actions), Varreduras (Gitleaks/TruffleHog), Versao e `Portao`; `sonar`, `imagem`, `publicar` e `lancar` `skipped`, como nas fases anteriores (D-2026-09-30-foundation-2) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Exit 0 → `OK`; 930/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | Exit 0 → `OK`; TOTAL 94.15% |
| 10 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` → `### Added` com a busca KLIPY e a coleção; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | README cita a busca de GIFs e stickers e o guia novo; evidência sugerida: diff do README revisado no PR |

As linhas 11 e 12 são do corte de release, como nas fases anteriores. Os itens de "Deferred to PR review" (busca real
com a chave do usuário, já feita pelo orquestrador e registrada no SUMMARY; termos do KLIPY; visual; sticker na 8.8"
real, T-8) não são linhas de DoD.

## Observações (sem aviso)
- **Tempo de 10 s para o arquivo inteiro.** `timeout_global` (`client.rs:98`) cobre do DNS ao último byte, como D-2
  e a doc do crate dizem. Os GIFs reais dos fixtures têm até 3,7 MB (a 10 s, ~3 Mbit/s bastam), mas um GIF grande numa
  conexão lenta falha com "no answer within 10 s", o que soa como servidor mudo. Um texto "took longer than 10 s"
  seria mais honesto.
- **`collected_users` lê a biblioteca inteira.** `gifs.rs:399-417` carrega cada tema do usuário com todos os assets
  (vídeos de até 25 MiB) a cada "Excluir…", só para comparar tamanhos e hashes. Funciona fora da thread da UI; com
  uma biblioteca grande o diálogo demora a abrir.
- **Nomes reservados do Windows.** `file_stem` (`gifs.rs:582`) deixa passar `con`, `nul`, `aux`, `com1`…: "Usar como
  fundo" grava `<cache>/collection/nul.gif`, que no Windows 10 é o dispositivo NUL, e a adição falha. Só o caminho do
  fundo toca o disco com esse nome; o usuário contorna renomeando.
- **`klipy.json` no Windows sem nova tentativa.** `write_private` (`key.rs:153-167`) renomeia uma vez; o
  `write_atomically` da coleção tenta de novo enquanto um antivírus ou indexador segura o arquivo (`patiently`).
- **Proxy.** O `ureq 3` lê `HTTP(S)_PROXY`/`ALL_PROXY` do ambiente por padrão (`config.rs:948`). Para HTTPS ele abre
  um túnel `CONNECT`, então a chave segue dentro do TLS; o guia já fala de firewall/proxy.
- **Avisos de terceiros.** O projeto não distribui avisos de licença dos crates (anterior à fase). O
  `webpki-roots` (CDLA-Permissive-2.0) entra nessa mesma lacuna; vale para um todo de empacotamento, não para esta fase.
- **PLAN.** As tasks seguem `Status: pending` no PLAN; o SUMMARY registra 8/9 concluídas.

## Recommendation
Nenhum gate falha e as 10 linhas Auto passam, incluindo o Windows no CI do `HEAD` `8df7ffa` (run 36948045888,
`rust-windows` 863/0/12, `Portao` verde). A chave do usuário não chega à janela, a DTO, erro, `Debug`, log,
temporário nem índice; a rede só sai por ação do usuário, para `api.klipy.com` (chave no caminho, sem redirect) e
`static.klipy.com` (sem chave, com teto), e a janela não consegue apontar o backend para outro endereço. As regras
de D-7 estão no código e nos testes. As 8 mutações que fiz (vazamento no erro, redirects, modo do arquivo, itens só da
última busca, índice ilegível, não-GIF, filtro dos stickers, 404) derrubaram os testes do DoD que nomeiam a regra.

Ficam 3 avisos sem bloqueio: a seção Privacidade do guia diz que os stickers são pedidos com stills JPEG (W1), a
queda silenciosa para uma coleção em memória quando a pasta não abre (W2) e o "Saved key ending in " vazio de uma
chave curta (W3). Vale corrigir W1 antes do PR, porque é a seção que o usuário lê para saber o que sai do
computador. W2 e W3 podem ir para o PR ou para um todo. As 2 linhas Manual do PROJECT ficam para o corte de release;
a T-8 (sticker na 8.8" real) e o visual seguem em "Deferred to PR review", com o orquestrador.

## DoD Critic (enhanced)

- DoD row «1 | Core … sem KLIPY/HTTP»: oca e objetiva — o grep só procura nomes; `http = "1.5.0"` no core e um
  `ServiceFailure::of_status(http::StatusCode)` passam com OK. Correção: o Verify exige que o core só dependa de
  `thiserror` (`cargo tree -p bezel-core -e normal --depth 1`).
- DoD row «3 | Studio e disco … CSP igual»: oca e objetiva — (a) a CSP é checada por substring
  (`img-src 'self' https://static.klipy.com data: blob:` passa); (b) "nada sai no início" só é provado num `Gifs` de
  teste: uma busca de aquecimento no `setup` real (`lib.rs:153-160`) com a chave salva passa. Suspeita: só
  `capabilities/default.json` é lido. Correção: CSP igual à da `main` byte a byte; teste do `setup` real com a fonte
  injetada; todas as capabilities.
- DoD row «5 | UI … Playwright 4 projetos + axe»: oca e objetiva — só conta `>= 24 passed`; `test.fixme('429: 100 per
  hour')` + outro teste qualquer passa. Correção: cada um dos 6 testes nomeados aprovado nos 4 projetos, com o axe
  registrado em cada execução.
- DoD row «6 | Guia … Privacidade»: oca e objetiva — só o título é exigido; trocar a seção por uma frase passa (por
  isso o W1 passou). Correção: o `check-docs.sh` exige na seção o que sai, para onde, quando, onde fica a chave e como
  removê-la.
- DoD row «10 | No TODO/FIXME without issue» (PROJECT): oca e objetiva — o pathspec ignora `.mjs`, `.css`, `.html`,
  `.sh`; um TODO em `gif-search.spec.mjs` passa. Correção: ampliar o pathspec.
- Linhas 2, 4, 7, 8, 9: provam o critério.

**Verdict:** BLOCKED
