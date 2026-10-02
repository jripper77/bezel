# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 2, iteração 3 (8ª no total, depois do AUTO-RESET 1) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `2d25287`, igual ao remoto do início ao fim (conferido
> de novo antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (90 arquivos, 52 commits), com atenção ao
> último commit de código:
> - `a603c45` (`test(gif-sticker-search): guard command calls, Request and prints`): implementa a D-11 na guarda de
>   fonte:
>   - um comando só é nomeado na sua definição e na lista do único `generate_handler!` de `run`;
>   - `tauri::ipc::Request` só aparece em `search_gifs`, `gif_preview`, `collect_gif` e `UserAsked::of`;
>   - `commands.rs` entra no grupo "silencioso" junto com os módulos de GIF e da chave: nenhuma macro de print,
>     panic ou log e nenhum `stdout`/`stderr`;
>   - `emit_progress` vai de `commands.rs` para `lib.rs`.
> - `2d25287`: SUMMARY.
>
> Em produção, o `a603c45` só muda duas coisas: a mudança de lugar do `emit_progress` e comentários de documentação
> (`commands.rs`, `gifs/asked.rs` e `lib.rs` antes do `mod tests`). Todo o resto está no `mod tests` do `lib.rs`.
>
> Li D-1..D-11, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 2 iter 2, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos vêm das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Extraí os `Verify:` do Markdown por script, sem editar, e rodei
>   cada um com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** feitas em cópias por `git archive` de `2d25287` no scratchpad (`rev8/mut/<mutante>`). Cada cópia
>   tem um `git init` próprio, com `origin/main` = `e527240` buscado do repositório, para a linha 3 rodar como está
>   escrita, e um `CARGO_TARGET_DIR` próprio (`target/rev8mut/<mutante>`). O repositório não foi alterado.
>   Controle sem mutação (`ctl`): a linha 3 dá `OK`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **939 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. O total é o mesmo da iter 2, porque o `a603c45` só acrescenta casos dentro de testes que já existiam (`commands_called_from_rust`, `invocations_taken_elsewhere` e `prints_in_commands`, chamados por `the_source_guard_reads_identifiers_not_text`). Studio lib: **160** no Linux |
| Coverage | PASS | **94.71%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.65%**. Studio: `gifs.rs` 96.87%, `gifs/key.rs` 91.38%, `lib.rs` 91.38%, `gifs/asked.rs` 50.00%. `bezel-klipy`: `client.rs` 90.04%, `dto.rs` 99.16% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. O clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`, deu exit 0. No CI, o clippy do Windows (`--all-targets --all-features -D warnings`, com o studio) passou. Nenhum `allow` novo: os 4 de `rtss.rs` são anteriores à fase |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A iteração não mexe no core, em `protocol/` nem nos adapters. O único `unwrap` novo está no `mod tests` (`studio_commands`). Não há `unsafe` no studio (`#![forbid(unsafe_code)]`). 5.9: só comentários e testes anteriores à fase. 5.10: os comandos síncronos são anteriores; os 12 da fase são `async`. `cargo audit`: exit 0 (620 crates). Nenhum segredo. O `Cargo.lock` não mudou na iteração |
| Consistency | PASS | 52 commits: 51 com escopo `gif-sticker-search` e 1 `chore(jdi)`. O `a603c45` implementa a D-11 e não contradiz D-3, D-10 nem D-11. A guarda tem um falso positivo que a D-11 não prevê (W1) |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iteração não mudou a UI |
| DoD | PASS | As 10 linhas Auto passam. Rodei a linha 7 depois que o CI do `HEAD` terminou verde. As 2 Manual são da release |

## Itens da rodada 2, iter 2
| Item | Estado | Evidência |
|---|---|---|
| W1 (revisor) / crítico, mutante 1: `preferences` chama `search_gifs(request, …)` (`cb2`) | **CLEARED** | Reapliquei o `cb2`: `#[tauri::command] pub async fn preferences(request: Request<'_>, gifs, state) -> UiResult<PreferencesDto>`, com `let _ = search_gifs(request, gifs, state.clone(), "gif".into(), String::new(), 1, None).await;`. O build de produção compila. A linha 3 como está escrita deu **exit 1**, e a guarda acusa:<br>- `` `Request` (the window's invocation) named outside the GIF commands ``;<br>- `` `search_gifs` names a command function outside its definition and the list of `generate_handler!` in `run` `` |
| Crítico, mutante 2: `save_klipy_key` ganha `request: Request` e `eprintln!("{:?}", request.body())` (`rb`) | **CLEARED** | Reapliquei o `rb`. Compila. A linha 3 deu **exit 1**, e a guarda acusa:<br>- `` `Request` … named outside the GIF commands ``;<br>- `` `eprintln` prints or logs in a GIF, key or command module `` |
| W2 (revisor): a D-10 não registrava as regras do `e99c81b` | **CLEARED** | A D-11 (`.jdi/decisions/D-2026-10-01-gif-sticker-search-11.md`, já renderizada no `DECISIONS.md:137`) emenda a D-10 (3). Ela lista:<br>- as 9 APIs recusadas, `navigate` incluída;<br>- os literais `__TAURI`, os hosts do KLIPY e `javascript:`;<br>- nenhum print/log nos módulos de GIF e da chave, em `commands.rs` e nas funções que leem a chave;<br>- os dois usos de `expose_secret`;<br>- `UserAsked::of` só nos 3 comandos;<br>- `Request` só neles;<br>- comandos nomeados só na definição e no `generate_handler!`.<br>A guarda faz isso. Ela ainda é mais estrita em alguns pontos que a D-11 não descreve (Observações) e tem um falso positivo (W1 desta rodada) |

## `emit_progress` em `lib.rs`: comportamento
Pelo diff, o comportamento é o mesmo. Só muda o texto do aviso do upload.
- **Antes:** em `run_upload`, `if throttle.pass(progress) && let Err(e) = app.emit(PROGRESS_EVENT, ProgressDto::from(progress)) { tracing::warn!("upload progress not sent: {e}") }`. O `emit_progress` privado de `commands.rs` fazia o mesmo para `run_plan` e `delete_files`, com `"storage progress not sent"`.
- **Agora:**
  - `run_upload` chama `if throttle.pass(progress) { emit_progress(&app, ProgressDto::from(progress)); }` (`commands.rs:567`);
  - `run_plan` (`:721`) e `delete_files` (`:740`) chamam o `crate::emit_progress` (`lib.rs:357`).
- **O que não mudou:** o `emit_progress` novo emite o mesmo evento (`commands::PROGRESS_EVENT` = `"storage-progress"`, que o `bridge.js:41` escuta). Também ficam iguais o payload, o throttle e a mesma ordem de chamada. Quando o envio falha, ele só registra o erro e o trabalho continua.
- **Única diferença:** o upload registra `"storage progress not sent: {e}"` em vez de `"upload progress not sent: {e}"`, como diz o commit.
- **Escopo:** `lib.rs` não é módulo "silencioso", e a função não recebe chave nem invocação.
- **Testes:** os testes de upload e do gerenciador continuam passando (939/0/12). No Playwright, `storage manager …` e `video background …` passaram nos 4 projetos.

## A guarda nova contra a D-11: falsos positivos
- **Na árvore real, nenhum falso positivo.**
  - `nothing_in_the_app_forges_an_invocation` passa no `HEAD`, no Linux e no Windows (CI, `rust-windows`).
  - O teste confere que existe um só `generate_handler!` (`lib.rs: run`) e que a lista dele tem exatamente os
    comandos encontrados nas árvores de sintaxe.
  - Ele aceita `Backend::cache_info` como caminho (o pai não é `commands`), além de métodos e campos com o nome de um
    comando, inclusive nos tokens de macro (`is_member`).
- **Fora da árvore real, um falso positivo** (W1): uma variável local, um parâmetro ou um campo abreviado em
  `commands.rs` com o nome de um comando é recusado, embora não nomeie a função do comando.
- **Aderência à D-11:** nenhuma regra a contradiz. Onde a guarda vai além do texto, ela é mais estrita (Observações).

## Blockers
- nenhum

## Warnings
- **W1 — A regra "comando só via IPC" recusa uma variável local com o nome de um comando em `commands.rs`.**
  - **Mutante `fp2`.** Uma refatoração que não muda o comportamento, em `commands.rs:164`:
    ```rust
    pub async fn video_auto(state: State<'_, Shared>, theme: ThemeDto) -> UiResult<VideoAutoDto> {
        let video_auto = blocking(&state, move |b| b.video_auto(&theme)).await?;
        Ok(video_auto)
    }
    ```
  - **Resultado:**
    - compila;
    - `cargo clippy -p bezel-studio --all-targets -- -D warnings` dá exit 0;
    - a linha 3 dá **exit 1**, com `` `video_auto` names a command function outside its definition and the list of `generate_handler!` in `run` ``.
  - **Outro caso:** `fp1` (`let preferences = state.preferences(); preferences` em `preferences`) dá o mesmo.
  - **Causa:** em `lib.rs:1491`, `command_named` trata qualquer caminho de um segmento em `commands.rs` como o
    comando, porque `parent` vazio torna `all(…)` verdadeiro. Uma variável, um parâmetro ou um campo abreviado
    (`Dto { video_auto }`) com o nome de um comando também é um `syn::Path`.
  - **Por que é falso positivo:** a D-11 só proíbe "name any Tauri command function". O próprio teste
    `the_source_guard_reads_identifiers_not_text` (`lib.rs:2799`) aceita método e campo com o nome de um comando.
  - **Gravidade:** baixa. A falha aparece com mensagem clara e não deixa nada passar. Mas a linha 3 cai numa edição
    legítima, e o caso pode reaparecer, porque `commands.rs` tem 72 comandos com nomes como `preferences`,
    `video_auto` e `cache_info`.
  - **Correção sugerida**, uma das duas:
    - (a) ignorar em `commands.rs` os nomes de um segmento ligados na própria função (padrões de `let`, parâmetros
      e closures) e pôr o `fp2` entre os casos aceitos;
    - (b) emendar a D-11 para dizer que, em `commands.rs`, nenhum caminho pode usar o nome de um comando, locais
      incluídos, e pôr o `fp2` entre os casos recusados.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (`HEAD` = `2d25287`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK`. Controle `ctl`: `OK`. Mutantes `cb2` e `rb`: exit 1. `fp1`/`fp2` também dão exit 1, mas são falsos positivos (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe` |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37008240730 terminar (`gh run watch --exit-status`; `conclusion=success`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `2d25287`: `OK` (`rust-windows` = `success`, **157** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 939/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.65% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK`. O `concat!("to", "do")` do `PRINTS` não dispara o grep |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37008240730, `2d25287`)
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
    ok: as regras novas não dependem de SO (os nomes dos arquivos são montados por componentes unidos com `/`,
    `lib.rs:2143`).
- **`rust-linux`:** verde. Localmente deu 939/0/12, com 160 testes no studio. Linux (160) − Windows (157) = os 3
  testes de mock runtime que só rodam fora do Windows (D-8).

## Observações (sem aviso)
- **A guarda vai além do texto da D-11, sem contradizê-la.** Para ajustar no mesmo passo que o W1, se quiserem:
  - (a) no grupo "silencioso", a guarda trata como print as macros de panic e assert (`PRINTS`, `lib.rs:1145`, 15
    nomes) e os identificadores `stdout`/`stderr` (`lib.rs:1173`), inclusive como campo (`out.stderr`). A D-11 fala
    só em "print/log macro";
  - (b) `Request` é recusado por identificador em qualquer ponto do studio, não só como `tauri::ipc::Request`. Um
    tipo homônimo de outra crate também cairia. Hoje não existe nenhum;
  - (c) `UserAsked::of` recebe `&Request` e é aceito (`lib.rs:1425`), mas a D-11 só exclui os três comandos da
    regra de `Request`;
  - (d) a guarda recusa `__TAURI` também nos testes, e os hosts do KLIPY também em comentários. A D-11 diz "the last
    three in tests too".
- **Rotas que ficam com a revisão de código, segundo a D-10/D-11:**
  - `#[path = "commands.rs"]` com outro nome de módulo já é recusado;
  - um `macro_rules!` que monta o nome de um comando, não;
  - uma mudança de `save_klipy_key(key: KlipyKey)` para `String` tiraria a proteção de tipo da D-10 (1). A guarda
    não fixa o tipo do parâmetro, mas isso é mudança deliberada, não engano realista.
- **Cobertura de `commands.rs`: 10.72%.** Ele tem só invólucros finos dos métodos do `Backend`, e a iteração não
  mudou isso.
- Continuam valendo as observações das iters anteriores:
  - o timeout de 10 s;
  - `collected_users` lendo a biblioteca;
  - os nomes reservados do Windows;
  - `klipy.json` sem nova tentativa;
  - o proxy;
  - a raiz temporária que fica para trás quando um teste do `lib.rs` falha no meio;
  - o "bezel-klipy 97%" do SUMMARY (medi `client.rs` 90.04%, `dto.rs` 99.16%);
  - a sonda `pf` (pré-carregar `page + 1` dentro de `search_gifs`), que vem de uma ação do usuário.

## Recommendation
Os itens da rodada 2, iter 2 estão resolvidos:
- **Crítico, mutante 1 / W1 (`cb2`):** `preferences` chamando `search_gifs` com a própria invocação compila, mas
  reprova pela guarda (`Request` fora dos 3 comandos e comando nomeado fora do `generate_handler!`).
- **Crítico, mutante 2 (`rb`):** `save_klipy_key` com `Request` e `eprintln!` do corpo reprova pelas regras de
  `Request` e de print em `commands.rs`.
- **W2:** a D-11 registra o conjunto de regras.

A mudança do `emit_progress` não altera o comportamento; só muda o texto do aviso do upload.

Os gates 1–8 passam. O CI do `HEAD` está verde, com 157 testes do studio no Windows.

Antes de fechar a fase:
1. **W1:** tirar o falso positivo das variáveis locais com nome de comando em `commands.rs`, ou declará-lo na D-11,
   com o `fp2` no teste da regra.
2. Opcional: alinhar o texto da D-11 às Observações (a)–(d).

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … chave privada»: oca e objetiva — engano realista: um log de depuração em volta do
  `generate_handler!` (`logged(handler)` com `eprintln!("ipc {} {:?}", invoke.message.command(),
  invoke.message.payload())`) compila, passa a linha e põe a chave inteira no stderr a cada "Salvar" (antes da fronteira
  do comando). As mutações da iteração anterior falham.
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
