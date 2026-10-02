# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 4 do loop autônomo (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` =
> `6f8b9e1`, igual ao remoto do início ao fim (conferido de novo antes da linha 7). Escopo: `git diff
> origin/main...HEAD` inteiro (86 arquivos), com atenção à iter 4:
> - `e039cdf`: D-9, PROJECT e CONTEXT;
> - `d8d17d9`: `UserAsked`. É o patch de `a82fcc3`; comparei os diffs e são idênticos;
> - `832e861`: debounce no e2e. É o patch de `4beeaac`; também idêntico;
> - `6f8b9e1`: SUMMARY.
>
> Também li D-1..D-9, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da iter 3, incluindo a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Os `Verify:` foram extraídos do Markdown por script, conferidos
>   byte a byte com a fonte e rodados com `bash`, como estão escritos.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da
>   chave. Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** feitas em `git clone`s de `6f8b9e1` no scratchpad, com `origin/main` (`e527240`) copiado e um
>   `CARGO_TARGET_DIR` próprio por mutante. O repositório não foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **934 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. São +1 em relação à iter 3 (933): `tests::the_windows_gif_commands_reach_the_source`, que é novo. Studio lib: 155 no Linux |
| Coverage | PASS | **94.61%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.55%**. Studio: `gifs.rs` 96.89%, `lib.rs` 78.08%, `gifs/asked.rs` 50% (3/6). `UserAsked::of` roda 3×, uma vez por comando no teste de IPC. `in_a_test` aparece com 0 porque é avaliado em compilação (`const ASKED`). `bezel-klipy` `client.rs`: 90.04% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o clippy do Windows (`--all-targets --all-features -D warnings`, com o studio) passou. Nenhum `allow` novo: os 4 em `rtss.rs` são anteriores à fase |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. 5.4: `KlipyClient::new` aparece só em `lib.rs:492`, a raiz de composição. `UserAsked` é interno do studio; a porta `GifSource` do core não mudou. A iter 4 não trouxe `unwrap`/`expect` fora de teste nem `unsafe`, e não tocou em `protocol/`. `cargo audit`: exit 0 (620 crates). Nenhum segredo |
| Consistency | WARN | 33 commits: 32 com escopo `gif-sticker-search` e 1 `chore(jdi)`. A iter 4 não mexe em `Cargo.lock`, `Cargo.toml`, `build.rs`, capabilities nem `bridge.js`. A D-9 está implementada como foi decidida, e entrou no mesmo commit da edição do PROJECT (`e039cdf`), como o `PROJECT.md:49` exige. Na D-8 (2), o teste novo é `cfg(not(windows))`. A garantia que a D-3 ganhou no código é menor do que os comentários dizem: ver W1 |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **225/225**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iter 4 não muda strings nem a UI, só o spec |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou verde. As 2 Manual são da release |

## Itens do crítico da iter 3
| Item | Estado | Evidência |
|---|---|---|
| Linha 3: overlay de config do Tauri com outra CSP | **CLEARED** | No clone `m3a`, adicionei e deixei em stage um `src-tauri/tauri.linux.conf.json` com `"csp": "default-src * 'unsafe-inline' 'unsafe-eval'; connect-src *"`. A linha 3 como está escrita deu **exit 1**, sem `OK`. A causa é a checagem nova: os 9 testes passam (`ok. 9 passed`), a comparação da linha `"csp"` com a `main` dá exit 0 e a checagem de overlay dá exit 1. O padrão `^tauri.*\.(json5?\|toml)$` com `-i` também pega `Tauri.toml`, `tauri.conf.json5` e `tauri.windows.conf.json`. Nenhum script nem workflow usa `--config` ou `TAURI_CONFIG` |
| Linha 3: aquecimento atrasado no `setup` | **CLEARED** | No clone `m3b`, o `setup` abre uma thread que espera 3 s e chama `Gifs::search` em alta. Testei 3 variantes, cada uma em build normal (`--lib`) e de teste (`--lib --tests`), e **nenhuma compila**: (A) a chamada antiga `warm.search(&query)` dá `E0061` (o método pede 2 argumentos); (B) `warm.search(&UserAsked::in_a_test(), …)` dá `E0599` no build normal e `E0624` (privado) no de teste; (C) `UserAsked { _invoked: () }` dá `E0451` (campo privado). O controle sem a mutação compila. Uma função de comando também não serve, porque pede um `tauri::ipc::Request`, que não tem construtor público. Há dois caminhos deliberados que ainda passam: ver W1 |
| Linha 4: `[patch]` que troca a origem do `getrandom` 0.3.4 | **CLEARED** | No clone `m4`, copiei `getrandom-0.3.4` para `vendor/`, adicionei `[patch.crates-io] getrandom = { path = "vendor/getrandom" }` e regerei o lock offline. O diff do lock só tem `-source`/`-checksum`, sem `-version`. A linha 4 como está escrita deu **exit 1**, e a falha vem do primeiro termo (`^-(version\|source\|checksum)`). Controle sem a mutação: `OK` |
| Linha 5: `oninput` chamando `submit` (busca a cada tecla) | **CLEARED** | No clone `m5`, em `src/ui/gif-search.js:204`, troquei `ui.trigger.typed` por `ui.trigger.submit`. A linha 5 como está escrita deu **exit 1**: `gif search › explicit off by default` falhou nos 4 projetos (`FAIL: 4 problem(s)`) e os outros 5 testes ficaram `ok`. Fiz uma mutação extra, `createSearchTrigger({ …, delay: 150 })`, e ela também deu **exit 1** nos 4 projetos. O teste prende o relógio da página, e um `MutationObserver` grava cada consulta, não só a última |
| Linha 10 (PROJECT): filtro, `#444`, `packaging/`, plural | **CLEARED** | No clone `m10`, rodei a linha como está escrita contra 10 mutações, e todas deram **exit 1**: `test.fixme(…); // TODO: un-park` no spec de e2e; `'fixme'; // FIXME: …` em `scripts/e2e-passed.mjs`; `color: #444; /* FIXME: #444 … */` e `/* TODO #444 */` no CSS; `# TODO` em `packaging/linux/postinstall.sh`; `# TODO` em `.github/workflows/`; `# TODOs:`; `# FIXMEs`; `# fixme later`; `todo!()` em Rust. Os 3 controles deram `OK`: `TODO(#12): …`, `todos os arquivos` e `test.fixme('parked', …)` sozinho, que a D-9 aceita de propósito |

## Desenho do `UserAsked` (`d8d17d9`): regressões
- **O contrato da UI não mudou.**
  - `bridge.js:333-337` continua mandando `search_gifs {kind, text, page, explicit}`, `gif_preview {id, still}`
    e `collect_gif {id}`. `build.rs:71-73`, `generate_handler!` e as capabilities não foram tocados.
  - O `Request<'_>` não lê argumento por nome. O `CommandArg` do tauri 2.12 (`ipc/mod.rs:165-172`) só guarda
    referências ao payload e aos headers, e nunca falha.
  - `tests::the_windows_gif_commands_reach_the_source` passa pelo IPC do mock runtime com os mesmos JSON do
    `bridge.js`. Ele confere 1 fonte criada no primeiro comando, nenhuma no início, e as chamadas `Page` →
    `Download` → `Download`.
  - Os 6 e2e de GIF passam nos 4 projetos.
- **O build do Windows segue a D-8.**
  - O teste novo e o que ele usa (`context_with_the_window`, `invoke`, os `use` de `tauri::test`/`serde_json`)
    são `cfg(not(windows))` (`lib.rs:748-786`).
  - O `UserAsked` do topo de `lib.rs` é usado fora de teste (`klipy_source`), e `counting` continua compilando no
    Windows.
  - No CI, `rust-windows` teve o clippy `--all-targets --all-features -D warnings` ok e **153** testes do studio,
    os mesmos da iter 3. Nenhum dos 2 testes fora do Windows aparece no log.
- **Nada muda em produção.**
  - `UserAsked` é um tipo de tamanho zero e `of` ignora o request. `klipy_source` ignora o token e cria o mesmo
    `KlipyClient`.
  - Os comandos, os nomes e as respostas são os mesmos.
  - Os testes de `gifs/tests.rs` só ganharam `&ASKED` nas chamadas. Nenhuma asserção foi removida nem
    enfraquecida.
- **Limite:** o tipo protege o estado `Gifs` e as funções de comando. Ele não protege uma invocação forjada em
  Rust nem o adapter usado direto. Ver W1.

## Blockers
- nenhum

## Warnings
- **W1: a garantia "um aquecimento no início não compila" é maior do que o tipo entrega.** Ela aparece em
  `apps/bezel-studio/src-tauri/src/gifs/asked.rs:8-12`, em `gifs.rs:12-14` e em `lib.rs:290-292`. Com a API
  pública do tauri 2.12, dois caminhos compilam no build de produção e passam pela linha 3:
  - **(a) Invocação forjada.** O `setup` pode chamar `WebviewWindow::on_message(InvokeRequest { cmd: "search_gifs",
    …, invoke_key: handle.invoke_key().into() }, …)`. Tanto `on_message` (`webview/webview_window.rs:2572`) quanto
    `AppHandle::invoke_key` (`app.rs:1147`) e os campos de `InvokeRequest` (`webview/mod.rs:135`) são `pub`. Testei
    no clone `m3c`, com uma thread do `setup` que espera 3 s e forja a busca em alta:
    - o build `--lib` normal compila;
    - a linha 3 como está escrita dá **`OK`**;
    - um teste de rascunho com o handler real registrado, como o `run()` faz, mostra a fonte recebendo
      `Page(GifQuery { text: "", page: 1, … })` 3 s depois do `setup` (`at_ready=0 later=true`).
  - **(b) Adapter direto.** No `setup`, `KeyFile::load()` seguido de
    `KlipyClient::new(saved.key(), saved.customer_id()).page(&query)` numa thread atrasada também compila. Só rodei
    `cargo check`; executar isso iria ao KLIPY.

  As duas formas exigem código escrito de propósito; a documentação do tauri diz que `InvokeRequest` "NOT part of
  the public stable API". Nada disso existe no `HEAD`, então a D-3 está cumprida. Também não é regressão: na
  iter 3, qualquer aquecimento atrasado passava. O problema é que os comentários afirmam algo falso, e quem vier
  depois vai confiar neles.
  **Correção:**
  1. Ajustar os 3 comentários ao alcance real: o estado GIF e as funções de comando.
  2. Acrescentar à linha 3, antes do `cd`, uma checagem estática objetiva:
     `! grep -rnE '\.on_message\(|\.invoke_key\(\)' apps/bezel-studio/src-tauri/src && [ "$(grep -rn 'KlipyClient::new' apps/bezel-studio/src-tauri/src | wc -l)" = 1 ]`.
     Testei: no `HEAD` passa (o único `KlipyClient::new` é `lib.rs:492`) e no mutante (a) dá exit 1.
  3. Como a linha 3 é DoD da CONTEXT, registrar a mudança numa D-XX.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (2 + 3 passed; core só com `thiserror`; nenhum `klipy` em `crates/bezel-core`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` (6 passed) |
| 3 | Studio e disco: nada sai sem chave/no início, …; CSP igual | CONTEXT | Auto | PASS | `OK`: 9 + 2 passed, CSP igual à da `origin/main`, nenhum overlay de config, nenhuma capability `opener:`/`http:`. Mutante de overlay: exit 1. Aquecimento atrasado via `Gifs`: não compila. Furo deliberado: W1 |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK`. Mutante com `[patch]` de origem: exit 1 |
| 5 | UI: lógica, i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe`. Mutantes `oninput → submit` e `delay: 150`: exit 1 |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 36959630092 terminar (`gh run watch --exit-status`: exit 0). Depois rodei a linha como está escrita, com `HEAD` = remoto = `6f8b9e1`: `OK` (`rust-windows` = `success`, 153 testes do studio rodaram) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 934/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.55% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` no `HEAD`. As 10 mutações do crítico e as extras dão exit 1, e os 3 controles dão `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 36959630092, `6f8b9e1`)
- **Jobs:** `conclusion=success`. `rust-windows`, `rust-linux`, `node-ui`, CodeQL (rust,
  javascript-typescript, actions), Varreduras, Versao e `Portao` = success. `sonar`, `imagem`, `publicar` e
  `lancar` = skipped, como antes.
- **`rust-windows`:**
  - fmt ok;
  - clippy `--all-targets --all-features -D warnings` ok;
  - testes com cobertura: **865 passed, 0 failed, 12 ignored** em 30 binários;
  - `Running unittests src\lib.rs (…bezel_studio-32affe4de76179bd.exe)`: **`153 passed; 0 failed`** (15 s);
  - build release, binários extras e empacotamento Tauri ok.
- **`rust-linux`:** 934/0/12. Studio lib **155**, com `tests::the_windows_gif_commands_reach_the_source ... ok` e
  `tests::the_app_setup_sends_nothing_at_start ... ok`. Linux (155) − Windows (153) = esses 2 testes, como a D-8
  decide.

## Observações (sem aviso)
- O SUMMARY cita `a82fcc3` e `4beeaac`, que são os commits dos worktrees. No branch, eles são `d8d17d9` e `832e861`;
  os patches são idênticos.
- Um `test.fixme('…')` sozinho, ou seja, um teste estacionado sem marcador, passa a linha 10. Isso foi decidido
  na D-9. Só os 6 testes nomeados na linha 5 são exigidos como aprovados.
- `the_app_setup_sends_nothing_at_start` não registra o handler de comandos, então não enxerga caminhos por IPC.
  O teste de IPC novo cobre o lado positivo. A checagem estática de W1 fecharia o negativo sem depender de tempo.
- Continuam valendo as observações das iters 1–3: o timeout de 10 s; `collected_users` lendo a biblioteca; os
  nomes reservados do Windows; `klipy.json` sem nova tentativa; o proxy; a raiz temporária que fica para trás
  quando um teste do `lib.rs` falha no meio.

## Recommendation
Os 4 itens do crítico da iter 3 estão resolvidos:
- **Linha 3:** um overlay de config reprova, e um aquecimento atrasado via `Gifs` ou função de comando não compila.
- **Linha 4:** um `[patch]` de origem reprova.
- **Linha 5:** busca a cada tecla, ou com pausa curta, reprova pela UI nos 4 projetos.
- **Linha 10:** o filtro tira só os tokens; `#444`, `packaging/`, `.github/`, o plural e `fixme` minúsculo
  reprovam.

O `UserAsked` não muda o contrato da UI nem o comportamento em produção, e o build do Windows segue a D-8 (CI
verde, 153 testes do studio). Pode seguir para o PR.

Antes de fechar a fase, aplicar W1 (comentários e checagem estática na linha 3, via D-XX) ou registrar o motivo
de não aplicar. No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … nada sai … no início»: oca e objetiva, por drible deliberado — o `setup` forja uma
  invocação (`WebviewWindow::on_message` + `AppHandle::invoke_key()`, ou `window.eval` de um `invoke`) 3 s depois do
  início; compila, a linha passa e uma busca chega à fonte. Os enganos realistas já não compilam (`UserAsked`).
- DoD row «5 | UI …»: oca e objetiva, por enganos realistas — (a) um texto fixo em inglês passado por um helper local
  (`keyProblem('…')`) passa na varredura de i18n; (b) `columnsOf` no eixo errado (seta para baixo anda 1 em vez de
  uma linha) passa: as setas só são provadas na lógica isolada.
- Linhas 1, 2, 4, 6–10: provam o critério (as mutações da iter 3 falham).

**Verdict:** BLOCKED
