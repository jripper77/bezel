# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 2, iteração 1 (6ª no total, depois do AUTO-RESET 1) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `0c6d83f`, igual ao remoto do início ao fim (conferido
> de novo antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (89 arquivos, 46 commits), com atenção à
> rodada 2:
> - `dec033f`: D-10 (o que a linha 3 do DoD prova);
> - `f6f4030`: `KlipyKey` (a chave como tipo que nada imprime, lido pelo `CommandArg` do Tauri);
> - `aea1285`: guarda de fonte com `syn` (identificadores, `r#`, tokens de macro);
> - `d63a3b7`: comentários do que o tipo e a guarda garantem;
> - `43f6b9b`: helpers da UI de GIFs e da Coleção só aceitam chaves de i18n, com varredura de chaves;
> - `32684e9`: e2e confere `keySavedNow`, `keyRemoved` e `collection.using` no idioma;
> - `0c6d83f`: SUMMARY.
>
> Li D-1..D-10, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da iter 5, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado (D-10 está em `DECISIONS.md:135`).
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Os `Verify:` foram extraídos do Markdown por script, sem edição, e
>   rodados com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** feitas em `git clone`s de `0c6d83f` no scratchpad (`rev6/mut/`), com `origin/main` = `e527240` e
>   um `CARGO_TARGET_DIR` próprio por mutante (`target/rev6mut/<mutante>`). O repositório não foi alterado. Controle
>   sem mutação: linhas 3 e 5 `OK` no `HEAD`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **939 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. A iter 5 tinha 936; os +3 são `gifs::tests::a_key_prints_nothing_of_itself`, `tests::the_source_guard_reads_identifiers_not_text` e `tests::the_window_sends_the_key_as_before`. Studio lib: **160** no Linux |
| Coverage | PASS | **94.65%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.59%**. Studio: `gifs.rs` 96.87%, `gifs/key.rs` 91.38%, `lib.rs` 87.06%, `gifs/asked.rs` 50.00%. `bezel-klipy`: `client.rs` 90.04%, `dto.rs` 99.16% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o clippy do Windows (`--all-targets --all-features -D warnings`, com o studio) passou. Nenhum `allow` novo; os 4 de `rtss.rs` são anteriores à fase |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A rodada 2 não mexe no core, em `protocol/` nem em adapters de dispositivo. Nenhum `unwrap`/`expect` novo fora de teste (`gifs/key.rs` não tem nenhum). Nenhum `unsafe` (o studio tem `#![forbid(unsafe_code)]`). Os hits de 5.9 são comentários e testes de `discovery.rs`/`sensor.rs`/`device.rs`/`reconnect.rs`, fora do diff da fase. 5.10: os comandos síncronos são anteriores; os 12 da fase são `async`. `cargo audit`: exit 0 (620 crates). Nenhum segredo. `syn`/`proc-macro2` entram só como `[dev-dependencies]` já presentes no lock (o `Cargo.lock` ganha 2 arestas, nenhum pacote) |
| Consistency | PASS | 46 commits: 45 com escopo `gif-sticker-search` e 1 `chore(jdi)`. A D-10 descreve o que foi feito: `KlipyKey` sem `Display`/`Serialize`, `Debug` mascarado, leitor único `expose_secret` `pub(crate)`; guarda com `syn`, `r#` e tokens de macro; a linha 3 com 10 testes. D-3 (a janela nunca recebe a chave) e D-8 (teste do `setup` só fora do Windows) cumpridas. Ver W1: a D-10 (1) diz que imprimir a chave "não compila", e dentro de `klipy_source` compila e passa a linha 3 |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`) |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou verde. As 2 Manual são da release |

## Itens da iter 5
| Item | Estado | Evidência |
|---|---|---|
| W1 (revisor): `window.r#eval(…)` passa a guarda textual | **CLEARED** | Mutante `m1`: thread no `setup` que espera 3 s e chama `window.r#eval("document.getElementById('gif-search-open').click(); …")`. O build de produção (`cargo build -p bezel-studio --lib --locked`) compila. A linha 3 como está escrita deu **exit 1**: `9 passed; 1 failed`, e a guarda acusa `"lib.rs: \`eval\` in production code"` |
| W1 (revisor): a mesma chamada por macro local (`call!(window, eval, …)`) | **CLEARED** | Mutante `m2`: `macro_rules! call { ($t:expr, $m:ident, $a:expr) => { $t.$m($a) }; }` no topo de `lib.rs` e `call!(window, eval, "…")` na mesma thread. Compila em produção. A linha 3 deu **exit 1**, com a guarda acusando `` `eval` in production code `` (lido nos tokens da macro) |
| W2 (revisor): a linha 3 mudou sem D-XX | **CLEARED** | `D-2026-10-01-gif-sticker-search-10` (`dec033f`) registra a guarda, o que ela recusa, o que fica para revisão de código e que a linha 3 tem 10 testes. O Verify da linha 3 na CONTEXT lista exatamente esses 10 |
| Crítico, linha 3: `eprintln!("… {key} …")` em `Gifs::save_key` | **CLEARED** | Mutante `m3a`: a linha do crítico em `gifs.rs:304`. O build de produção **não compila**: `error[E0277]: \`KlipyKey\` doesn't implement \`std::fmt::Display\``. Variantes: `m3b` (`eprintln!` com `key.expose_secret()` em `gifs.rs`) compila, e a linha 3 deu **exit 1** (`` `eprintln` prints or logs in a GIF or key module `` e `` `expose_secret` reads the KLIPY key outside `klipy_source` and `gifs/key.rs` ``). `m3c` (`tracing::info!` com `key.expose_secret()` em `commands.rs::save_klipy_key`) compila, e a linha 3 deu **exit 1** (`commands.rs: \`expose_secret\` reads the KLIPY key outside …`). O que sobra é a própria fábrica: ver W1 |
| Crítico, linha 5: `announce('Removed')` | **CLEARED** | Mutante `m4`: `announce('gifs.keyRemoved')` → `announce('Removed')` em `ui/gif-search.js:173`. A linha 5 como está escrita deu **exit 1**. `test:unit` reprova em `the GIF and collection helpers take a key, …` (`ui/gif-search.js: announce('Removed'): unknown key "Removed"`), e o e2e reprova `gif search › no key: help` nos 4 projetos. Variante `p4`: `parts.status.textContent = 'Failed'` no `silence()` da Coleção (ramo de erro que o e2e não exercita). A linha 5 deu **exit 1**: `no text is written in the UI code: everything goes through t()` pega a palavra solta (o e2e passou 24/24) |

## `KlipyKey`, guarda de fonte e varredura de chaves
- **`CommandArg` do `KlipyKey` (`gifs/key.rs:93-100`) mantém o contrato da UI.**
  - A janela continua mandando `{key}` como string. O `bridge.js` não mudou.
  - Uma chave fora de `[A-Za-z0-9_-]{1,128}` volta `{code: "invalidInput", args: {detail}}`, sem citá-la. O
    `keyFailure` depende disso para mostrar `gifs.keyInvalid`.
  - Argumento ausente ou que não é string vira `tauri::Error::InvalidArgs(name, key, e)`. É o mesmo texto do
    blanket impl do Tauri para `Deserialize`, ou seja, igual a antes com `key: String`.
  - `tests::the_window_sends_the_key_as_before` prova isso pelo IPC do mock runtime: recusa, gravação, `last4`,
    nenhuma fonte criada. Fica fora do Windows (D-8), mas o código não depende de SO.
- **Guarda com `syn` (`lib.rs:1094-1830`).**
  - Lê cada identificador sem o `r#`, inclusive tokens de chamadas de macro (`visit_macro` percorre os tokens) e de
    atributos (`Meta::List` passa por `visit_token_stream`, `syn` 2.0.119 `gen/visit.rs:2908`).
  - **Falsos positivos:** nenhum no tree (a guarda passa no Linux). A regra é por nome, então um campo ou variável
    legítimo chamado `eval`, `on_message` etc. seria recusado. Esse é o lado seguro, e hoje nada no studio usa
    esses nomes.
  - **Corte de `cfg(test)`:** só sai um item (ou item de `impl`) com o atributo externo exatamente `#[cfg(test)]`,
    ou um arquivo cujo `mod` só é declarado assim. Tudo o mais é lido como produção, então a falha fica do lado
    seguro: item de trait, `#[cfg(test)]` em expressão ou `let`, `cfg(any(test, …))` e `cfg_attr(test, …)`. Também
    reprovam `#[path]`, `cfg_attr(…path…)`, `include!`, arquivo que nenhum `mod` declara (inclusive `mod x;` dentro
    de um `mod` inline) e arquivo que o `syn` não lê. `the_source_guard_cuts_only_test_modules` confirma isso no
    tree e em arquivos inventados.
  - **Pontos cegos:** os declarados na D-10 (código gerado, outro crate). Há também um que a D-10 não declara: a
    fábrica imprimir a chave (W1).
- **Varredura de chaves (`tests/ui/i18n.test.mjs`).**
  - Em `gif-search.js`, `collection.js` e nos módulos `ui/` deles, o 1º argumento de
    `t`/`announce`/`keyProblem`/`refuseName` precisa ser um destes:
    - chave entre aspas presente em en e pt-BR;
    - ternário de chaves;
    - fonte listada em `KEY_SOURCES` (as chaves são conferidas, e uma entrada que sai do tree reprova);
    - `.key` de uma função de mensagem (`errorMessage`, `keyFailure`, `resultsMessage`, provadas à parte);
    - o próprio `key` do helper.
  - Os helpers precisam ter `key` como 1º parâmetro. Uma volta a `announce(text)` em arrow function também reprova,
    porque o conjunto de helpers definidos muda.
  - **Falsos positivos:** nenhum no tree.
  - **Pontos cegos:** um helper novo com outro nome, ou um apelido (`const say = announce`), fica fora da varredura
    de chaves. Mesmo assim, texto direto no DOM em `ui/*.js` é pego por `no text is written in the UI code` (`p4`), e
    frases por `no sentence is written …`. Não achei um engano realista que passe as três varreduras e o e2e.

## Blockers
- nenhum

## Warnings
- **W1: a fábrica do cliente pode imprimir a chave, e a linha 3 passa.** O problema está em
  `apps/bezel-studio/src-tauri/src/lib.rs:501-507` (`klipy_source`) e nas regras da guarda em `lib.rs:1264-1270` e
  `lib.rs:1290`:
  - a regra de `expose_secret` aceita qualquer uso dentro de `klipy_source`;
  - a regra de print/log só vale em `gifs.rs` e `gifs/`.

  Dois mutantes provam o furo. Os dois compilam em produção, e a linha 3 como está escrita deu **`OK`** (`10
  passed`):
  - **`m3e`:** `eprintln!("bezel-studio: KLIPY client for key {}", key.expose_secret());` antes do
    `KlipyClient::new`. A chave inteira vai para o stderr na primeira busca: no Linux e nos builds de debug, já que o
    release do Windows não tem console.
  - **`m3d`:** a mesma coisa com `tracing::debug!`. Hoje isso não aparece, porque nenhum subscriber de `tracing` é
    instalado, mas aparece no dia em que um for.

  Nenhum teste chama `klipy_source`: os testes usam a fábrica `counting`.

  A D-10 (1) diz que "printing or logging the key does not compile", e isso só vale fora da fábrica. Para chegar
  aqui é preciso escrever `expose_secret()` dentro de um print. Isso é menos realista que o `{key}` do crítico,
  que agora não compila. Por isso fica como aviso, e nada disso existe no `HEAD`.

  **Correção:**
  1. Na guarda, recusar `expose_secret` dentro dos tokens de qualquer macro. Hoje os dois usos legítimos, em
     `lib.rs:504` e `gifs/key.rs:198`, são argumentos de chamada comum, então a regra não cria falso positivo. Isso
     fecha `format!`, print e log de uma vez.
  2. Recusar macros de print/log também dentro de `klipy_source`.
  3. Pôr `m3e` em `the_source_guard_reads_identifiers_not_text`.
  4. Ajustar a frase da D-10 (1) se a regra não mudar.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (2 + 3 passed; core só com `thiserror`; nenhum `klipy` em `crates/bezel-core`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` (6 passed) |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, …; CSP igual | CONTEXT | Auto | PASS | `OK`: 10 + 2 passed, CSP igual à da `origin/main`, nenhum overlay de config, nenhuma capability `opener:`/`http:`. Mutantes: `m1`, `m2`, `m3b` e `m3c` deram exit 1 pela guarda, e `m3a` não compila. `m3d` e `m3e` (print na fábrica) deram `OK` (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica, i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe`. Mutantes `m4` e `p4`: exit 1 |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 36998982608 terminar (`gh run watch --exit-status`: exit 0). Depois rodei a linha como está escrita, com `HEAD` = remoto = `0c6d83f`: `OK` (`rust-windows` = `success`, **157** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 939/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.59% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 36998982608, `0c6d83f`)
- **Jobs:** `conclusion=success`. `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust,
  javascript-typescript), Varreduras, Versao e `Portao` = success. `sonar`, `imagem`, `publicar` e `lancar` =
  skipped, como antes.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` ok;
  - `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes: **869 passed, 0 failed, 12 ignored**;
  - unittests do studio: **`157 passed; 0 failed`** (155 da iter 5 + `gifs::tests::a_key_prints_nothing_of_itself`
    e `tests::the_source_guard_reads_identifiers_not_text`). `tests::nothing_in_the_app_forges_an_invocation` e
    `tests::the_source_guard_cuts_only_test_modules` também rodaram ok: a guarda com `syn` não depende de SO.
- **`rust-linux`:** 939/0/12, com 160 testes no studio. Linux (160) − Windows (157) = os 3 testes de mock runtime
  que só rodam fora do Windows (D-8), incluindo o novo `tests::the_window_sends_the_key_as_before`.

## Observações (sem aviso)
- O "Files modified" do SUMMARY não lista `apps/bezel-studio/src/messages.js`, que o `43f6b9b` mudou: `errorText`
  virou `errorMessage` + `errorText`, com a mesma saída para os outros módulos.
- O SUMMARY diz "bezel-klipy 97%". Medi `client.rs` 90.04% e `dto.rs` 99.16% (já anotado na iter 5).
- Não verificado: `Webview::navigate` (Tauri 2.12 → `wry` `load_uri`, sem filtro de esquema) com uma URL
  `javascript:` pode rodar script na janela no WebKitGTK. Não está na lista `FORGERIES`. Seria um drible deliberado,
  como os que a D-10 deixa para a revisão de código. Se for barato, vale acrescentar `navigate` à lista ou recusar
  literais `javascript:`.
- Continuam valendo as observações das iters 1–5: o timeout de 10 s; `collected_users` lendo a biblioteca; os
  nomes reservados do Windows; `klipy.json` sem nova tentativa; o proxy; a raiz temporária que fica para trás
  quando um teste do `lib.rs` falha no meio.

## Recommendation
Os itens da iter 5 estão resolvidos:
- **W1 (revisor):** `r#eval` e a macro local reprovam pela guarda com `syn` (`m1`, `m2`), e os mutantes compilam em
  produção.
- **W2 (revisor):** a D-10 registra a guarda e a linha 3 com 10 testes.
- **Crítico, linha 3:** o `eprintln!("… {key} …")` não compila mais (`m3a`). As variantes com `expose_secret` em
  `gifs.rs` e `commands.rs` reprovam pela guarda (`m3b`, `m3c`).
- **Crítico, linha 5:** `announce('Removed')` reprova pela varredura de chaves e pelo e2e nos 4 projetos (`m4`). A
  palavra solta direto no DOM reprova pela varredura de literais visíveis (`p4`).

Os gates 1–8 passam. O CI do `HEAD` está verde, com 157 testes do studio no Windows.

Antes de fechar a fase, aplicar o W1 ou registrar por que fica fora:
1. recusar `expose_secret` dentro de tokens de macro;
2. recusar print/log em `klipy_source`;
3. incluir `m3e` no teste da regra.

Pelo critério do crítico, `m3e` é um engano menos realista que o da iter 5, porque exige escrever
`expose_secret()` dentro de um print, mas a linha 3 o deixa passar.

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … chave privada … nada sai … no início»: oca e objetiva — (a) engano plausível, dentro
  do que a D-10 afirma: `eprintln!("…{}", key.expose_secret())` em `klipy_source` (`lib.rs:501-507`) compila e passa;
  (b) drible deliberado sem API de forja: outro comando (`preferences`, chamado ao abrir a janela) cria
  `UserAsked::of(&request)` e busca; a guarda não limita onde o token nasce. Não demonstrado: `navigate` para
  `javascript:`. As mutações da iter 5 falham.
- DoD row «5 | UI …»: prova o critério (`announce('Removed')` falha); um valor pt-BR igual ao inglês é conteúdo de
  tradução (fica para o PR).
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
