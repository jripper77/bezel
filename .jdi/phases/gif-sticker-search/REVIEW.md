# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 2, iteração 2 (7ª no total, depois do AUTO-RESET 1) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `acb5886`, igual ao remoto do início ao fim (conferido
> de novo antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (89 arquivos), com atenção ao último
> commit de código:
> - `e99c81b` (`test(gif-sticker-search): guard key reads, proofs and page loads`): a guarda de fonte passa a
>   - limitar `expose_secret` a dois usos;
>   - recusar qualquer macro numa função que lê a chave;
>   - aceitar `UserAsked::of` só nos 3 comandos de GIF;
>   - recusar `navigate` e literais `javascript:`.
> - `acb5886`: SUMMARY.
>
> Em produção, o `e99c81b` só muda comentários de documentação (`gifs.rs`, `gifs/asked.rs`, `gifs/key.rs` e
> `lib.rs` antes do `mod tests`). Todo o resto está no `mod tests` do `lib.rs`.
>
> Li D-1..D-10, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 2 iter 1, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Os `Verify:` foram extraídos do Markdown por script, sem edição, e
>   rodados com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** feitas em `git clone`s de `acb5886` no scratchpad (`rev7/mut/<mutante>`), com `origin/main` =
>   `e527240` e um `CARGO_TARGET_DIR` próprio por mutante (`target/rev7mut/<mutante>`). O repositório não foi
>   alterado. Controle sem mutação: a linha 3 dá `OK` no `HEAD`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **939 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. É o mesmo total da iter 1: o `e99c81b` só acrescenta casos dentro de testes que já existiam (`the_source_guard_reads_identifiers_not_text`, `nothing_in_the_app_forges_an_invocation`). Studio lib: **160** no Linux |
| Coverage | PASS | **94.67%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.61%**. Studio: `gifs.rs` 96.87%, `gifs/key.rs` 91.38%, `lib.rs` 89.75%, `gifs/asked.rs` 50.00%. `bezel-klipy`: `client.rs` 90.04%, `dto.rs` 99.16% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o clippy do Windows (`--all-targets --all-features -D warnings`, com o studio) passou. Nenhum `allow` novo: os 4 de `rtss.rs` são anteriores à fase |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A iteração não mexe no core, em `protocol/` nem em adapters. Os hits de 5.6 do diff estão em `#[cfg(test)]`/`gifs/tests.rs`. Não há `unsafe` no studio (`#![forbid(unsafe_code)]`). 5.9: só comentários e testes anteriores à fase. 5.10: os comandos síncronos são anteriores; os 12 da fase são `async`. `cargo audit`: exit 0 (620 crates). Nenhum segredo. `Cargo.lock` não mudou na iteração |
| Consistency | PASS | 49 commits: 48 com escopo `gif-sticker-search` e 1 `chore(jdi)`. As regras novas não contradizem a D-10: são mais estritas que ela. Mas a D-10 não as registra (W2) |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iteração não mudou a UI |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou verde. As 2 Manual são da release |

## Itens da rodada 2, iter 1
| Item | Estado | Evidência |
|---|---|---|
| W1 (revisor) / crítico, linha 3 (a): `eprintln!("…{}", key.expose_secret())` em `klipy_source` | **CLEARED** | Mutante `m3e`: `eprintln!("bezel-studio: KLIPY client for key {}", key.expose_secret());` antes do `KlipyClient::new`. O build de produção compila. A linha 3 como está escrita deu **exit 1**: `9 passed; 1 failed`. A guarda acusa `` `expose_secret` reads the KLIPY key inside a macro call `` e `` `eprintln!` in `klipy_source`, which reads the KLIPY key `` |
| W1, variante de log (`m3d` da iter 1) | **CLEARED** | Mutante `m3d`: `tracing::debug!(key = key.expose_secret(), "KLIPY client made");` no mesmo lugar. Compila. A linha 3 deu **exit 1**, com as mesmas duas recusas (`` `tracing::debug!` in `klipy_source` ``) |
| Crítico, linha 3 (b): `preferences` faz `UserAsked::of(&request)` e busca | **CLEARED** (como foi descrito) | Mutante `cb`: `#[tauri::command] pub fn preferences(request, gifs, state)` com `UserAsked::of(&request)` e `gifs.search(&asked, &q)` para "em alta". Compila. A linha 3 deu **exit 1** com `` commands.rs: `UserAsked::of` outside the GIF commands that take the window's `Request` ``. O mesmo efeito, por outro caminho, ainda passa: ver W1 |
| Nota da iter 1: `navigate` para `javascript:` | **CLEARED** | Mutante `nav`: thread no `setup` que espera 3 s e chama `window.navigate(tauri::Url::parse("javascript:document.getElementById('gif-search-open').click()")…)`. Compila. A linha 3 deu **exit 1**, com `` `navigate` in production code `` e `` a literal is a `javascript:` URL `` |

## Regras novas da guarda: falsos positivos e D-10
- **Na árvore real, nenhum falso positivo.**
  - `nothing_in_the_app_forges_an_invocation` passa no `HEAD`, no Linux e no Windows (CI).
  - O teste fixa onde a chave é lida (`gifs/key.rs: expose_secret`, `gifs/key.rs: save`, `lib.rs: klipy_source`)
    e onde a prova nasce (`commands.rs: search_gifs`, `gif_preview`, `collect_gif`).
  - Conferi o código da guarda. Ninguém usa `navigate` em produção. Nenhum literal começa com `javascript:`.
    `gifs/asked.rs` só deriva `Debug`. `KeyFile::save` e `klipy_source` não chamam macro.
- **A chave ficou bem fechada.** O texto só sai como argumento direto de `KlipyClient::new` ou no campo `key` do
  `KeyJson`:
  - o `Debug` do `KlipyClient` (`crates/bezel-klipy/src/client.rs:196`) não mostra a chave;
  - `write_private` fica em `gifs/key.rs`, onde print e log já são recusados.
- **D-10:** nada contradiz a decisão:
  - (1) um leitor `pub(crate)`, para a fábrica e o arquivo da chave;
  - (2) fonte só com `UserAsked`;
  - (3) os identificadores listados continuam recusados.

  A guarda agora recusa mais do que a D-10 (3) enumera: `navigate`, literais `javascript:`, `UserAsked::of` fora
  dos 3 comandos, `expose_secret` fora dos 2 usos e macro em função que lê a chave. Os comentários do código citam a
  D-10 para essas regras, mas a decisão não foi emendada (W2).

## Blockers
- nenhum

## Warnings
- **W1 — A linha 3 aceita outro comando buscando com a própria invocação, sem citar `UserAsked`.**
  - **Mutante `cb2`.** O comando `preferences`, que o `app.js:30` chama ao abrir a janela, vira:
    ```rust
    #[tauri::command]
    pub async fn preferences(request: Request<'_>, gifs: State<'_, SharedGifs>, state: State<'_, Shared>)
        -> UiResult<PreferencesDto> {
        let _ = search_gifs(request, gifs, state.clone(), "gif".into(), String::new(), 1, None).await;
        Ok(state.preferences())
    }
    ```
  - **Resultado:**
    - o build de produção compila;
    - a linha 3 como está escrita dá **`OK`** (`10 passed`);
    - um teste de sonda pelo IPC do mock runtime, só no clone `cb2`, mostra que a fonte do GIF é criada na
      invocação de `preferences`, com a chave salva.
  - **Efeito:** com uma chave salva, toda abertura do app pede "em alta" ao KLIPY, o que fere a D-3.
  - **Causa:** a guarda limita onde a prova nasce (`lib.rs:1147`, `lib.rs:1328`), mas não quem chama as funções
    dos comandos de GIF (`commands.rs:874` e seguintes).
  - **Classe:** é o mesmo caso do item (b) do crítico, sem API de forja. Nenhuma linha do DoD o pega. A linha 5
    roda sobre o backend de demonstração, não sobre os comandos Rust.
  - **Correção sugerida:** recusar os nomes de `PROOF_COMMANDS` em produção fora da própria definição em
    `commands.rs` e da lista do `generate_handler!` em `lib.rs`; incluir `cb2` no teste da regra.
- **W2 — A D-10 não registra as regras do `e99c81b`.**
  - A linha 3 continua com os mesmos 10 testes, mas o que ela prova mudou:
    - `navigate` e `javascript:`;
    - lugar da prova;
    - dois usos de `expose_secret`;
    - nenhuma macro nas funções que leem a chave.
  - A D-10 (3) ainda lista só os 8 identificadores antigos. Emendar a D-10 ou abrir uma D-XX nova, para o crítico
    julgar a linha 3 pelo escopo declarado.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (`HEAD` = `acb5886`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK`. Mutantes `m3e`, `m3d`, `cb` e `nav`: exit 1. `cb2` passa (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe` |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37003453567 terminar (`gh run watch --exit-status`: exit 0). Depois rodei a linha como está escrita, com `HEAD` = remoto = `acb5886`: `OK` (`rust-windows` = `success`, **157** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 939/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.61% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37003453567, `acb5886`)
- **Jobs:** `conclusion=success`.
  - success: `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust, javascript-typescript), Varreduras,
    Versao e `Portao`;
  - skipped, como antes: `sonar`, `imagem`, `publicar` e `lancar`.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` ok;
  - `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes: **869 passed, 0 failed, 12 ignored**;
  - unittests do studio (`bezel_studio-*.exe`): **`157 passed; 0 failed`**;
  - `tests::nothing_in_the_app_forges_an_invocation` e `tests::the_source_guard_reads_identifiers_not_text` rodaram
    ok: as regras novas não dependem de SO.
- **`rust-linux`:** 939/0/12, com 160 testes no studio. Linux (160) − Windows (157) = os 3 testes de mock runtime que
  só rodam fora do Windows (D-8).

## Observações (sem aviso)
- **Sonda `pf`:** uma segunda busca dentro do próprio `search_gifs` (pré-carregar `page + 1`) compila e passa a
  linha 3.
  - Nenhum teste da linha 3 roda `search_gifs` pelo IPC.
  - O mesmo erro em `Gifs::search` seria pego: `no_request_at_start_or_without_key` exige exatamente uma chamada.
  - Fica dentro do que a guarda aceita; é assunto para revisão de código.
- **Sonda `rb`:** `tracing::debug!("{:?}", request.body())` num `save_klipy_key` que recebe `Request` compila e passa
  a linha 3, porque `commands.rs` não é módulo de GIF. Isso fica fora do que a D-10 (1) afirma ("from the command
  boundary on"), e o studio não instala subscriber de `tracing` hoje. Vale lembrar no PR.
- Continuam valendo as observações das iters anteriores:
  - o timeout de 10 s;
  - `collected_users` lendo a biblioteca;
  - os nomes reservados do Windows;
  - `klipy.json` sem nova tentativa;
  - o proxy;
  - a raiz temporária que fica para trás quando um teste do `lib.rs` falha no meio;
  - o "bezel-klipy 97%" do SUMMARY (medi `client.rs` 90.04%, `dto.rs` 99.16%).

## Recommendation
Os itens da rodada 2, iter 1 estão resolvidos:
- **W1 / crítico (a):** `eprintln!`/`tracing::debug!` de `key.expose_secret()` em `klipy_source` compilam, mas
  reprovam pela guarda (`m3e`, `m3d`).
- **Crítico (b):** `UserAsked::of(&request)` em `preferences` reprova (`cb`).
- **Nota da iter 1:** `navigate` para `javascript:` reprova pelo identificador e pelo literal (`nav`).

Os gates 1–8 passam. O CI do `HEAD` está verde, com 157 testes do studio no Windows.

Antes de fechar a fase:
1. **W1:** recusar em produção os nomes `search_gifs`, `gif_preview` e `collect_gif` fora da definição em
   `commands.rs` e do `generate_handler!` do `lib.rs`, e pôr o `cb2` no teste da regra. Pelo critério do crítico da
   iter 1, o `cb2` é da mesma classe do item (b).
2. **W2:** emendar a D-10 (ou abrir uma D-XX nova) com as regras do `e99c81b`.

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … nada sai … no início, chave privada»: oca e objetiva — (1) engano realista:
  `preferences` (chamado ao abrir a janela) chama `search_gifs(request, …)` diretamente; compila, a guarda passa e o
  app pede o "em alta" ao KLIPY a cada início com chave salva; (2) engano realista: `save_klipy_key` ganha `request:
  Request` e `eprintln!("{:?}", request.body())` — a chave inteira vai ao stderr pelo caminho real de IPC. As mutações
  da iteração anterior falham. A prefetch de página+1 dentro de `search_gifs` segue uma ação do usuário (não conta).
- DoD row «5 | UI …»: prova o critério (uma chamada de GIF no carregamento, em JS, falha no e2e e no teste do bridge).
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
