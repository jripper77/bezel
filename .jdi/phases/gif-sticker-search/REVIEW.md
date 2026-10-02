# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** BLOCKED

> Revisão em modo `verify`, iteração 2 do loop autônomo (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` =
> `9c8295d` (igual ao remoto, inalterado do início ao fim). Escopo: `git diff origin/main...HEAD` inteiro (82
> arquivos), com atenção aos commits da iter 2 (`e2c9cb2`, `f3bd567`, `6ffa470`, `f9bbc24`, `d5fa90e`, `b524c83`,
> `e49329a`, `9c8295d`), as 7 decisões `D-2026-10-01-gif-sticker-search-{1..7}`, PROJECT, CONTEXT (linhas 1, 3 e 5
> apertadas), PLAN, SUMMARY, LOOP e o REVIEW da iter 1 (revisor W1–W3 + crítico nas linhas 1, 3, 5, 6 e 10).
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura); nenhum copiado do SUMMARY. Os `Verify:` rodaram como escritos, extraídos do Markdown por
>   script e executados com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado nem os servidores do KLIPY. Playwright com
>   `BEZEL_E2E_PORT=1442`.
> - **Mutações:** num `git clone` de `9c8295d` no scratchpad (com `origin/main` copiado do repositório), uma cópia
>   e um `CARGO_TARGET_DIR` próprio por mutante (apagados depois). Para cada uma rodei o `Verify` da iter 1 e o de
>   agora. O repositório não foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **933 passed, 0 failed, 12 ignored** (39 binários). +3 sobre os 930 da iter 1: `tests::the_app_setup_sends_nothing_at_start`, `tests::the_app_never_keeps_the_collection_in_memory` e `gifs::tests::a_collection_folder_that_cannot_be_used_is_said_not_lost`. Bate com o SUMMARY |
| Coverage | PASS | **94.46%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.40%**. Studio `lib.rs` 72.50% (o `setup` agora roda em teste), `gifs.rs` 96.89% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy `--target x86_64-pc-windows-msvc` sem `bezel-studio`/`bezel-klipy`: exit 0. Nenhum `allow` novo fora de testes |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 a 5.11 sem achado novo: os hits de 5.3b, 5.5, 5.9 e 5.10 são anteriores à fase (nenhum desses arquivos está no diff); `KlipyClient::new` e `DiskCollection::open` fora de teste só em `lib.rs:487` e `:506` (composição); nenhum `unwrap`/`expect` fora de `mod tests` em `lib.rs`/`gifs.rs`. `cargo audit`: exit 0 |
| Consistency | WARN | 22 commits com escopo `gif-sticker-search` (mais o `chore(jdi)`); `Cargo.lock` igual ao da iter 1 (o `tauri` com a feature `test` é só dev-dependency e não entra no build normal: `cargo tree -e normal,features -i tauri` sem `"test"`). D-1..D-7 cumpridas. Ver W2: o DoD LOCKED do PROJECT mudou sem a D-XX que ele exige |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **222/222**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe) |
| DoD | FAIL | Linha 7 (Windows no CI) e linha 10 (TODO/FIXME do PROJECT) falham como escritas; as outras 8 Auto passam |

## Itens da iter 1
| Item | Estado | Evidência |
|---|---|---|
| W1 — Privacidade errava os formatos dos stickers | **CLEARED** | `docs/user/gifs-and-stickers.md`: "`gif,jpg` for GIFs and `gif,png` for stickers: a GIF comes with JPEG stills, a sticker with PNG ones" e "JPEG for a GIF, PNG for a sticker"; o pt-BR diz o mesmo. Conferido contra o código: `customer_id` de 128 bits refeito só para outra chave (`gifs.rs:291-294`), `ureq` sem cabeçalho próprio, `io.github.slipalison.bezel` (`tauri.conf.json:5`), prévias e stills locais (`preview_data_url`), `last4` só com 9+ caracteres. `check-docs.sh` agora exige esses fatos dentro da seção |
| W2 — coleção que não abre virava coleção em memória | **CLEARED** | `lib.rs` não usa mais `MemoryCollection`; `Gifs` abre por `CollectionOpener` (`gifs.rs:82`) e `with_collection` (`gifs.rs:259`) reabre a cada uso; pasta inutilizável = `collectionUnavailable {folder, reason}` (en/pt-BR, fixture), `collect` recusa antes de baixar (`gifs.rs:388`); a Coleção mostra `collection.loadFailed` + o texto do erro + "Tentar de novo" (`ui/collection.js:61-70`). Provado por `a_collection_folder_that_cannot_be_used_is_said_not_lost` e `the_app_never_keeps_the_collection_in_memory` (sobre o `gifs()` do app) |
| W3 — "Saved key ending in " vazio | **CLEARED** | `keyStatus` usa `gifs.keySavedNoEnding` ("Saved key"/"Chave salva") quando `last4` é nulo ou vazio; o demo responde como o backend (`last4: null` abaixo de 9 caracteres); teste "a short saved key shows no ending" |
| Crítico, linha 1 (`http` no core) | **CLEARED** | Mutante: `http = "1.5.0"` no core + `ServiceFailure::of_status(http::StatusCode)`, `Cargo.lock` atualizado offline. `Verify` da iter 1: `OK`; o de agora: exit 1 (`cargo tree` de profundidade 1 dá `http thiserror`) |
| Crítico, linha 3 (CSP) | **CLEARED** | Mutante: `img-src 'self' https://static.klipy.com data: blob:`. Iter 1: `OK` (a substring continua lá); agora: exit 1, só a cláusula da CSP falha (testes ok) |
| Crítico, linha 3 (nada sai no início) | **CLEARED** | Mutante: busca de aquecimento (em alta) numa thread disparada no `setup` real, depois do `gifs()`. Iter 1: `OK`; agora: exit 1, `the_app_setup_sends_nothing_at_start` cai em "a GIF source was made at start" |
| Crítico, linha 3 (capabilities) | **CLEARED** | Mutante: `capabilities/links.json` com `opener:default` e `opener:allow-open-url` (compila: o `tauri-plugin-opener` é dependência). Iter 1: `OK` (só lia `default.json`); agora: exit 1 na cláusula `capabilities/*.json` |
| Crítico, linha 5 (contagem de e2e) | **CLEARED** | Mutante: `test.fixme('429: 100 per hour')` + um teste vazio + o axe tirado de "vertical and horizontal". Iter 1: `OK` (24 passed); agora: exit 1, `e2e-passed.mjs` aponta `fixme` e `no axe` nos 4 projetos (8 problemas). Resta um furo: ver W1 |
| Crítico, linha 6 (Privacidade oca) | **CLEARED** | Mutante: a seção nas duas línguas trocada por uma frase. O `check-docs.sh` da iter 1 passa ("all checks passed"); o de agora: exit 1, 60 problemas nomeados |
| Crítico, linha 10 (pathspec do TODO) | **CLEARED, mas a linha falha** | Mutante: `// TODO: …` em `tests/e2e/gif-search.spec.mjs`. Iter 1: `OK`; agora a linha é listada. Mas a linha alargada já falha no `HEAD` limpo: B2 |

## Regressões e o código novo
- **`setup(app, Start)` em produção.** Comparei com o fechamento antigo. A ordem é a mesma (compor, `restore_theme`,
  `manage` do backend, do `Gifs` e do `Unsaved`, bandeja, laço de atualização, janela salvo `--hidden`); `add_tray`
  faz o mesmo `tray::create` e os dois `manage`, e o laço chama o mesmo `LiveItem::sync` por uma closure. As pastas
  vêm de `Folders::of` com os mesmos métodos do Tauri; antes `data_dir()` já era exigido (em `gifs()`), e uma
  falha agora para o setup um pouco antes, com o mesmo resultado (o app não abre). Plugins (single-instance
  primeiro, dialog, autostart, opener), `on_window_event`, os comandos e o `run` seguem iguais.
  `show_main_window` só ficou genérica. Não achei mudança de comportamento.
- **O teste do setup não grava nas pastas do usuário.** Rodei os dois testes novos de `lib.rs` com
  `HOME`/`XDG_*`/`TMPDIR` apontando para uma pasta vazia. O teste da coleção não toca nada fora do `TMPDIR`, e a
  raiz temporária é apagada. O do setup só cria `$XDG_DATA_HOME` vazio: com `~/.local` somente leitura, o Tauri do
  mock falha com "Failed to setup app: Permission denied". No `HOME` real essa pasta já existe, então nada muda
  lá. Não sobrou `/tmp/bezel-app-setup-*` (o laço de atualização que continua vivo não recria a raiz).
- **`collectionUnavailable`.** Coerente nos 7 usos (`list`, `collect`, `rename`, `delete`, `users`,
  `use_in_theme`, `dto`); a pasta é reaberta a cada uso; o texto cita pasta e motivo, sem chave. Sem chave e com a
  pasta inutilizável, "Adicionar" diz `collectionUnavailable` antes de `klipyNoKey`: aceitável.
- **`e2e-passed.mjs`.** Correto no que promete: exige `expected`/`passed` no último resultado e a anotação `axe` em
  cada projeto; `fixme`, `skipped`, `flaky`, `missing`, `no axe` e falhas de testes não nomeados reprovam;
  relatório num temporário novo; o Playwright que cai sem relatório dá exit 1. Mas o número de projetos não é
  conferido (W1).

## Blockers
- **B1 — DoD 7: o `rust-windows` falha no `HEAD` `9c8295d`** (`.jdi/phases/gif-sticker-search/CONTEXT.md`, linha 7
  do DoD). No run **36952865167**, o passo "cargo test com cobertura" para quando sobe o binário de testes da lib do
  studio. Em `Running unittests src\lib.rs (…\bezel_studio-fb074d879e999991.exe)` o processo sai com
  **`0xc0000139` (STATUS_ENTRYPOINT_NOT_FOUND)** antes do primeiro teste. Tinham passado 663 testes em 25 binários;
  nenhum teste do studio no Windows rodou, nem os binários depois dele. `Portao` = failure. O run anterior,
  36948045888 no `8df7ffa`, estava verde com esse mesmo binário. A única mudança da iter 2 no link desse binário é a
  `f3bd567`: `tauri` com `features = ["test"]` em `apps/bezel-studio/src-tauri/Cargo.toml:52-54` e o teste do mock
  em `src/lib.rs:655`. Causa provável (hipótese, sem Windows aqui): o `tauri-build` só embute o manifest do Common
  Controls v6 nos bins (`tauri-build-2.7.0/src/lib.rs:816`), e o código do `App` puxado pelo mock passa a importar
  entradas do `comctl32` v6. O Linux passou (933/0/12, com os 3 testes novos).
  **Correção:** fazer o binário de testes subir no Windows, por exemplo embutindo o mesmo manifest nos testes pelo
  `build.rs` (`cargo:rustc-link-arg-tests=/MANIFEST:EMBED` + `/MANIFESTINPUT:<manifest>` no alvo msvc). Depois,
  provar com o `rust-windows` verde no novo `HEAD`. Tirar o teste do Windows por `cfg` esconderia a falha.
- **B2 — PROJECT DoD "No TODO/FIXME" falha no `HEAD`** (`.jdi/PROJECT.md:59`, alargada em `e2c9cb2`). Rodando como
  está escrita, sai com exit 1 e 9 linhas, todas do `d5fa90e`, que veio depois do alargamento:
  `apps/bezel-studio/scripts/e2e-passed.mjs:2`, `:5`, `:92` e `apps/bezel-studio/tests/ui/e2e-passed.test.mjs:4`,
  `:64-66`, `:68-69`. É a palavra `fixme` da API do Playwright (`test.fixme`, anotação `'fixme'`), não um
  marcador, mas o `\b(todo|fixme)\b` case-insensitive conta. O "OK no HEAD" do `e2c9cb2` valia antes do `d5fa90e`,
  e o SUMMARY não roda essa linha depois dele. **Correção:** registrar a D-XX que o PROJECT exige (ver W2) e
  ajustar o `Verify` para ignorar a anotação do Playwright (por exemplo `grep -vE "test\.fixme|'fixme'"` antes do
  filtro de issue). Outra saída é mudar as 9 linhas sem perder o sentido. Depois, rodar a linha de novo no `HEAD`.

## Warnings
- **W1 — A linha 5 não confere os "4 projetos".** `e2e-passed.mjs` exige "cada projeto que o config declara"
  (`scripts/e2e-passed.mjs:120`, `:132`), mas o `Verify` não fixa quantos são. Tirei 3 dos 4 projetos de
  `playwright.config.mjs` (linhas 23-28) numa cópia, e o `Verify` de agora deu `ok: 6 tests × 1 projects` → `OK`.
  O critério fala em 4 projetos (claro/escuro × pt-BR/en). **Correção:** o `Verify` exigir `× 4 projects` na saída,
  ou a ferramenta receber a lista esperada.
- **W2 — O DoD LOCKED do PROJECT mudou sem D-XX.** `.jdi/PROJECT.md:49` diz "Change requires a new D-XX in
  DECISIONS.md plus manual edit here". O `e2c9cb2` alargou o pathspec (correção pedida pelo crítico) sem registrar
  a decisão; `git diff origin/main...HEAD -- .jdi/decisions` só tem as 7 desta fase. Registrar junto com a
  correção de B2.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK`. Mutante `http` no core: iter 1 `OK`, agora exit 1 |
| 2 | `KlipyClient` contra servidor HTTP loopback | CONTEXT | Auto | PASS | `OK` (6 passed) |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, …; CSP igual | CONTEXT | Auto | PASS | `OK` (9 + 2 passed, CSP igual à da `origin/main`, nenhuma capability com `opener:`/`http:`). Os 3 mutantes do crítico derrubam o `Verify` |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: `gif-search.test.mjs`, i18n, Playwright 4 projetos + axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe`. Mutante `fixme`/sem axe: exit 1. Ver W1 |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK`. Mutante de Privacidade oca: exit 1 (60 problemas) |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | **FAIL** | Esperei o run 36952865167 terminar (`gh run watch --exit-status`: exit 1, `completed failure`) e rodei o comando como escrito com `HEAD` = `9c8295d`: exit 1 (`rust-windows` = `failure`). B1 |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 933/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.40% |
| 10 | No `TODO`/`FIXME` without issue | PROJECT | Auto | **FAIL** | Exit 1: 9 linhas com `fixme` em `scripts/e2e-passed.mjs` e `tests/ui/e2e-passed.test.mjs`. B2 |
| 11 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | README cita a busca e o guia; evidência sugerida: diff do README revisado no PR |

## CI (run 36952865167, `9c8295d`)
`rust-windows` **failure** (B1); `rust-linux` success (933/0/12, os 3 testes novos ok); `node-ui`, CodeQL (rust,
js, actions), Varreduras e Versao success; `Portao` **failure**; `sonar`, `imagem`, `publicar` e `lancar` skipped.

## Observações (sem aviso)
- O teste do setup espera a 1ª volta do laço e mais 1,5 s. Um aquecimento atrasado além disso escaparia. É o
  limite de um teste com janela de tempo; o mutante do crítico (imediato) cai.
- Os testes novos de `lib.rs` só apagam a raiz temporária quando chegam ao fim; uma falha no meio deixa
  `/tmp/bezel-app-*`. Há um `/tmp/bezel-app-collection-2188221` das 22:41, anterior a esta revisão.
- Ficam valendo as observações da iter 1 (timeout de 10 s, `collected_users` lendo a biblioteca, nomes reservados
  do Windows, `klipy.json` sem nova tentativa, proxy, avisos de terceiros).

## Recommendation
Os 3 avisos da iter 1 e as 5 linhas do crítico estão resolvidos. Cada mutação do crítico, refeita, passa no
`Verify` da iter 1 e derruba o de agora. A refatoração do `setup` mantém o comportamento de produção. Há dois
bloqueios novos, os dois vindos da iter 2:
(B1) o teste com o mock runtime do Tauri faz o binário de testes do studio não subir no Windows, e o CI do `HEAD`
está vermelho; (B2) a linha de TODO/FIXME do PROJECT, alargada nesta iteração, reprova a própria ferramenta
`e2e-passed.mjs`.
Corrigir os dois, registrar a D-XX da mudança do PROJECT (W2) e fixar os 4 projetos na linha 5 (W1). Depois,
fazer push e rodar de novo as linhas 7 e 10 no novo `HEAD`.
