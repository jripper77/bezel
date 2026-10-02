# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 5 do loop autônomo (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` =
> `56aa726`, igual ao remoto do início ao fim (conferido de novo antes da linha 7). Escopo: `git diff
> origin/main...HEAD` inteiro (87 arquivos, 38 commits), com atenção à iter 5:
> - `ce4ade5`: guarda de fonte (`tests::nothing_in_the_app_forges_an_invocation`) e comentários do W1;
> - `8793841`: varredura de frases fixas na UI e chave vazia no e2e;
> - `3e1d1ae`: setas da grade pelas colunas reais no e2e;
> - `56aa726`: linha 3 do DoD com 10 testes e SUMMARY.
>
> O despacho e o SUMMARY citam `310c987`, `c85de43` e `e280b14`. São os commits de antes do rebase; comparei os
> patches com os do branch e são idênticos.
>
> Também li D-1..D-9, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da iter 4, incluindo a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Os `Verify:` foram extraídos do Markdown por script, sem edição, e
>   rodados com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** feitas em `git clone`s de `56aa726` no scratchpad (`r5/mut/`), com `origin/main` (`e527240`) e um
>   `CARGO_TARGET_DIR` próprio por mutante. O repositório não foi alterado. Controle sem mutação: linha 3 `OK`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **936 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. A iter 4 tinha 934; os +2 são `tests::nothing_in_the_app_forges_an_invocation` e `tests::the_source_guard_cuts_only_test_modules`. Studio lib: 157 no Linux |
| Coverage | PASS | **94.62%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.56%**. Studio: `gifs.rs` 96.89%, `lib.rs` 85.33% (sobe de 78.08% porque o código de teste da guarda está no `mod tests` do arquivo), `gifs/asked.rs` 50%. `bezel-klipy`: `client.rs` 90.04%, `dto.rs` 99.16% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o clippy do Windows (`--all-targets --all-features -D warnings`, com o studio) passou. Nenhum `allow` novo; os 4 de `rtss.rs` são anteriores à fase |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A iter 5 só mexe em código de teste e em comentários: nenhum `unwrap`/`expect` novo fora de teste, nenhum `unsafe`, nada em `protocol/` ou no core. `cargo audit`: exit 0 (620 crates). Nenhum segredo |
| Consistency | WARN | 38 commits: 37 com escopo `gif-sticker-search` e 1 `chore(jdi)`. Os arquivos da iter 5 estão na lista do PLAN. A D-3 é cumprida no `HEAD`. Na D-8, os testes da guarda usam só `std` e não dependem de SO, então rodam no Windows (155 = 153 + 2). A linha 3 do DoD mudou sem D-XX (W2) |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **228/228**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`) |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou verde. As 2 Manual são da release |

## Itens da iter 4
| Item | Estado | Evidência |
|---|---|---|
| W1 (revisor): comentários afirmam que um aquecimento no início "não compila" | **CLEARED** | `gifs/asked.rs:1-22`, `gifs.rs:11-17` e `lib.rs:287-296` agora limitam o que o tipo garante ao estado GIF, à fábrica de fontes e às funções de comando. Para a invocação forjada e o segundo cliente, eles remetem à guarda de fonte. Esses comentários ainda dizem que a guarda "recusa" qualquer script rodado na janela, e isso não é verdade: ver W1 abaixo |
| Crítico, linha 3: invocação forjada via `on_message` + `invoke_key` | **CLEARED** | Mutante `m3a`: uma thread no `setup` espera 3 s e chama `WebviewWindow::on_message(InvokeRequest { cmd: "search_gifs", …, invoke_key: handle.invoke_key().into() }, …)`. O `cargo build --lib` de produção compila. A linha 3 como está escrita deu **exit 1**: `9 passed; 1 failed`, e a falha é da guarda (`on_message`, `invoke_key`, `InvokeRequest` "in production code") |
| Crítico, linha 3: `window.eval` de um `invoke` | **CLEARED** | Mutante `m3b`: thread de 3 s que roda `window.eval("window.__TAURI__.core.invoke('search_gifs', …)")`. Compila em produção. A linha 3 deu **exit 1**, com a guarda acusando `.eval(` e `__TAURI`. Variantes com `r#eval` ou macro ainda passam: ver W1 |
| W1 (revisor) e crítico: segundo `KlipyClient` | **CLEARED** | Mutante `m3c`: `KlipyClient::new("k", "c").page(&query)` no `setup`. Ficou atrás de uma variável de ambiente que nunca é definida, para não chegar ao KLIPY. Compila em produção. A linha 3 deu **exit 1**: `KLIPY's client is made once in production` (`left == right`). Também vi no código que `use … as K`, `<KlipyClient>::new` e `type K = …` caem na regra "named outside its import", e que um segundo `KlipyClient::new(` muda a contagem |
| Crítico, linha 5: frase fixa em inglês via helper (`keyProblem('…')`) | **CLEARED** | Mutante `m5a`: `keyProblem(t('gifs.keyEmpty'))` trocado por `keyProblem('Paste the key in the field first.')`. A linha 5 como está escrita deu **exit 1**: `npm run test:unit` reprova em `no sentence is written in the UI code` (`ui/gif-search.js: Paste the key in the field first.`). Rodei o e2e à parte e ele também reprova: `no key: help` falha em light-pt e dark-pt e passa nos projetos en |
| Crítico, linha 5: `columnsOf` no eixo errado | **CLEARED** | Mutante `m5b`: `offsetTop` trocado por `offsetLeft` em `columnsOf` (`ui/gif-search.js:26`), o que faz a seta para baixo andar 1. A lógica unitária passa (`# pass 4`) e `test:unit` passa. A linha 5 deu **exit 1**: `gif search › explicit off by default` falha nos 4 projetos (`FAIL: 4 problem(s)`), porque a seta para baixo deveria focar `nth(6)` (5 colunas + 1) e não focou |

## Guarda de fonte e varredura de i18n
- **Guarda no tree real:** passa nos dois SOs (Linux 157, Windows 155). Ela não acusa código legítimo: os
  `InvokeRequest` e `get_ipc_response` dos testes de `lib.rs` e os arquivos `*/tests.rs` ficam de fora pela regra.
  O teste confirma o papel de `gifs/tests.rs`, `manager/tests.rs`, `storage/tests.rs`, `gifs.rs`, `gifs/asked.rs`
  e `main.rs`.
- **Regra de corte:** é correta. Ela corta só um bloco `mod` logo abaixo de uma linha exatamente `#[cfg(test)]`,
  até a chave que o fecha, e conta as chaves com comentários e literais apagados. O lexer trata comentários
  aninhados, escapes, `'{'`, `b'…'`, strings raw com `#`, lifetimes, labels e identificadores raw.
  - Um código Rust só esconde chaves desbalanceadas dentro de comentários ou literais, e esses são apagados.
  - Qualquer outra forma (atributo extra, `cfg(any(test, …))`, `#![cfg(test)]`) é lida como produção, ou seja,
    falha do lado seguro.
  - `#[path`, `include!(`, `cfg_attr(…path…)` e arquivo sem `mod` que o declare reprovam.
  - CRLF no checkout do Windows não muda o resultado: `trim` e `split_whitespace` absorvem o `\r`, e o CI passou.
- **Falsos negativos:** existem, por grafia. Ver W1.
- **Varredura de i18n:**
  - Lê todo `.js` de `src/`, exceto `i18n/` e `demo-*.js`. Isso inclui `gif-search.js`, `collection.js`,
    `ui/gif-search.js`, `ui/collection.js`, `ui/library.js`, `bridge.js` e `app.js`. O CSS não tem texto
    (`content:`), e o `index.html` tem teste próprio.
  - Testei o extrator inserindo uma frase em cada linha desses arquivos. Ela só escapou nas linhas dentro de
    comentário de bloco, então o extrator não perde o passo.
  - Uma palavra só, duas palavras minúsculas ou uma concatenação (`'Paste ' + 'the key'`) passam pela varredura de
    propósito. Nos helpers da UI de GIFs, o e2e pega esse caso. Mutante `p5c`: `refuseName('Required')`
    (`ui/collection.js:193`) passou em `test:unit`, mas a linha 5 deu **exit 1** por `gif collection › add,
    rename, delete` nos 4 projetos. `keyProblem` também é coberto (`m5a`).

## Blockers
- nenhum

## Warnings
- **W1: a guarda de fonte compara texto e deixa passar o mesmo `eval` com outra grafia.** O problema está em
  `apps/bezel-studio/src-tauri/src/lib.rs:1016-1032` (`FORGERIES`: `".eval("`, `"::eval"`) e em `forgeries()`
  (`lib.rs:1355`). Ela procura substrings no código sem espaços, então duas formas passam:
  - **`p1`:** `window.r#eval("…")`. Um identificador raw é o mesmo método, mas o texto vira `.r#eval(` e não casa
    com `.eval(`.
  - **`p2`:** uma macro local `macro_rules! call { ($t:expr, $m:ident, $a:expr) => { $t.$m($a) } }` chamada como
    `call!(window, eval, "…")`.

  Nas duas, uma thread do `setup` espera 3 s e roda o script
  `document.getElementById('gif-search-open').click(); setTimeout(() => document.querySelector('.gif-start button').click(), 500)`,
  que não tem `__TAURI`. As duas compilam no build de produção (`cargo build --lib`) e a linha 3 como está escrita
  dá **`OK`** (`10 passed`). Rodei o mesmo script no modo demo, pelo Playwright, sem ação do usuário. Ele pediu ao
  KLIPY do demo `{"kind":"gif","text":"","page":1,"explicit":false}`. No app real esse clique vira um
  `invoke('search_gifs')` com `Request` de verdade, e o `UserAsked` não tem como barrar isso.

  Para chegar aqui é preciso escrever o código de propósito, e nada disso existe no `HEAD`, então a D-3 está
  cumprida. Mas é o mesmo tipo de drible que o crítico da iter 4 considerou suficiente para dizer que a linha 3
  não prova o critério, e ele deve repetir esse juízo.

  Os comentários em `lib.rs:293-296`, `gifs.rs:14-17` e `gifs/asked.rs:15-22` afirmam que a guarda recusa "um
  script rodado na janela", sem ressalva.

  **Correção:**
  1. Casar os nomes proibidos como identificadores inteiros (`\b…\b`) no `skeleton`, com literais apagados e o
     prefixo `r#` removido. Assim `eval` pega `.eval(`, `::eval`, `r#eval` e `call!(w, eval, …)`.
  2. Acrescentar `with_webview` à lista.
  3. Pôr `p1` e `p2` em `the_source_guard_cuts_only_test_modules`.

  No `HEAD`, a palavra `eval` só aparece em produção num comentário (`gifs/asked.rs:17`), que é apagado antes da
  busca, e o único `macro_rules!` de produção (`messages.rs:20`) não usa nenhum desses nomes. Por isso a correção
  não deve criar falso positivo. A alternativa é analisar com `syn`, que já está no `Cargo.lock`; o `Ident` sem o
  `r#` resolve os dois casos.
- **W2: a linha 3 do DoD da CONTEXT mudou sem D-XX.** O commit `56aa726` passou o Verify de 9 para 10 testes.
  A mudança só aperta o gate e não contradiz nenhuma decisão. Mesmo assim:
  - o item 3 da correção do W1 da iter 4 pediu registrá-la numa D-XX;
  - a iter 4 registrou o aperto equivalente na D-9.

  Hoje a mudança só aparece na mensagem do commit e em `CONTEXT.md` ("Notes"). **Correção:** criar
  `D-2026-10-01-gif-sticker-search-10` para a guarda e a linha 3, junto com a correção do W1, se ela mudar o
  Verify.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (2 + 3 passed; core só com `thiserror`; nenhum `klipy` em `crates/bezel-core`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` (6 passed) |
| 3 | Studio e disco: nada sai sem chave/no início, …; CSP igual | CONTEXT | Auto | PASS | `OK`: 10 + 2 passed, CSP igual à da `origin/main`, nenhum overlay de config, nenhuma capability `opener:`/`http:`. Mutantes `m3a`, `m3b` e `m3c`: exit 1 pela guarda. Variantes por grafia (`p1` `r#eval`, `p2` macro): `OK` (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica, i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe`. Mutantes `m5a`, `m5b` e `p5c`: exit 1 |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 36994141707 terminar (`gh run watch --exit-status`: exit 0). Depois rodei a linha como está escrita, com `HEAD` = remoto = `56aa726`: `OK` (`rust-windows` = `success`, 155 testes do studio rodaram, inclusive os 2 da guarda) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 936/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.56% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 36994141707, `56aa726`)
- **Jobs:** `conclusion=success`. `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust,
  javascript-typescript), Varreduras, Versao e `Portao` = success. `sonar`, `imagem`, `publicar` e `lancar` =
  skipped, como antes.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` ok;
  - `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes: **867 passed, 0 failed, 12 ignored** em 30 binários;
  - unittests do studio: **`155 passed; 0 failed`**, com `tests::nothing_in_the_app_forges_an_invocation ... ok` e
    `tests::the_source_guard_cuts_only_test_modules ... ok`. A guarda não depende de SO e rodou no Windows.
- **`rust-linux`:** 936/0/12, com 157 testes no studio. Linux (157) − Windows (155) = os 2 testes de mock runtime
  que só rodam fora do Windows (D-8).

## Observações (sem aviso)
- O SUMMARY cita `310c987`, `c85de43` e `e280b14`, que são os commits de antes do rebase. No branch, eles são
  `ce4ade5`, `8793841` e `3e1d1ae`, com patches idênticos.
- O SUMMARY diz "bezel-klipy 97%". Medi `client.rs` 90.04% e `dto.rs` 99.16%. Não muda nenhum gate.
- A guarda tem ~500 linhas de lexer escrito à mão dentro do `mod tests` de `lib.rs`. Se o W1 for corrigido com
  `syn` (já no lock), ela fica menor e mais robusta.
- A guarda só lê o Rust do studio. A JS da UI tem outras proteções:
  - um aquecimento na UI aparece em `explicit off by default` (`lastQuery` nulo ao abrir);
  - no bridge Tauri, aparece no teste de chamadas exatas de `bridge.test.mjs`.
- Continuam valendo as observações das iters 1–4: o timeout de 10 s; `collected_users` lendo a biblioteca; os
  nomes reservados do Windows; `klipy.json` sem nova tentativa; o proxy; a raiz temporária que fica para trás
  quando um teste do `lib.rs` falha no meio.

## Recommendation
Os itens da iter 4 estão resolvidos:
- **W1:** os comentários agora dizem o que o tipo garante.
- **Crítico, linha 3:** a forja via `on_message` + `invoke_key`, o `window.eval` e o segundo `KlipyClient`
  reprovam pela guarda, e os mutantes compilam em produção.
- **Crítico, linha 5:** a frase via helper reprova pela varredura e pelo e2e em pt-BR, e `columnsOf` no eixo errado
  reprova pelo e2e nos 4 projetos.

Os gates 1–8 passam, e o CI do `HEAD` está verde com 155 testes do studio no Windows.

Antes de fechar a fase:
1. Aplicar W1: casar identificadores inteiros sem `r#`, acrescentar `with_webview` e incluir `p1`/`p2` no teste da
   regra, ou registrar por que esse drible fica fora do escopo. Pelo critério do crítico da iter 4, a linha 3 deve
   voltar a ser julgada oca enquanto `r#eval` passar.
2. Registrar a guarda e a linha 3 numa D-XX (W2).

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … chave privada …»: oca e objetiva — engano realista: `eprintln!("… {key} …")` em
  `Gifs::save_key` (`gifs.rs:303`) compila e a linha passa; a chave vai ao stderr, o canal de log do studio. Drible
  deliberado: `window.r#eval(…)` passa a guarda textual.
- DoD row «5 | UI …»: oca e objetiva — engano realista: `announce('Removed')` (`ui/gif-search.js:152`) passa; nenhum
  e2e confere `gifs.keyRemoved`, `gifs.keySavedNow` nem `collection.using` no idioma.
- Linhas 1, 2, 4, 6–10: provam o critério (as mutações da iter 4 falham).

**Verdict:** BLOCKED
