# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 2, iteração 4 (9ª no total, depois do AUTO-RESET 1) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `779a1ed`, igual ao remoto do início ao fim (conferido
> de novo antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (101 arquivos, 55 commits), com atenção ao
> último commit de código:
> - `1b66167` (`fix(gif-sticker-search): log only fixed diag codes; guard prints`), que implementa a D-12:
>   - módulo novo `diag` (`diag.rs`): `report(DiagCode)`, com `DiagCode` um enum fechado e sem dados (38 códigos),
>     cada um com frase fixa e canal (stderr para os 4 antigos `eprintln!`; `tracing` error/warn/info para o resto);
>   - os 41 diagnósticos do studio (37 macros de `tracing` e 4 `eprintln!`, o de `main.rs` incluído) passam por ele;
>   - a guarda de fonte ganha as regras da D-12 (print/log/stream/panic formatado/`unwrap`/`expect` fora do `diag`,
>     `Invoke`/`InvokeMessage`/`InvokeBody`/`payload` em produção, a forma do próprio `diag.rs`) e o escopo léxico
>     dos locais em `commands.rs` (W1 da iter anterior).
> - `779a1ed`: SUMMARY.
>
> Li D-1..D-12, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 2 iter 3, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos vêm das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Extraí os `Verify:` do Markdown por script, sem editar, e rodei
>   cada um com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** em cópias por `git archive` de `779a1ed` no scratchpad (`rev9/mut/<mutante>`). Cada cópia tem um
>   `git init` próprio, com `origin/main` = `e527240` buscado do repositório, para a linha 3 rodar como está escrita,
>   e um `CARGO_TARGET_DIR` próprio (`target/rev9mut/<mutante>`). O repositório não foi alterado. Controle sem
>   mutação (`ctl`): a linha 3 dá `OK`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **942 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. São 3 a mais que na iter 3 (939): os 2 testes de `diag::tests` e o doctest `compile_fail` de `diag.rs` (`report` não aceita um valor). Studio lib: **162** no Linux |
| Coverage | PASS | **94.76%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.71%**. Studio: `diag.rs` 100.00%, `lib.rs` 93.05%, `gifs.rs` 96.87%, `gifs/key.rs` 91.38%, `gifs/asked.rs` 50.00%, `commands.rs` 10.72% (invólucros finos, como antes) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. Nenhum `allow` novo: os 4 de `rtss.rs` são anteriores à fase e têm `reason =` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A iteração só mexe no studio. 5.5: `unsafe` só em `rtss.rs` (anterior, com `reason`); o studio tem `#![forbid(unsafe_code)]`. 5.6: o código de produção do studio não tem nenhum `unwrap`/`expect`/`panic!` (agora a guarda exige isso). 5.9: só comentários e testes anteriores à fase. 5.10: os comandos síncronos são anteriores; os 12 da fase são `async`. `cargo audit`: exit 0 (620 crates). Nenhum segredo. O `Cargo.lock` não mudou na iteração |
| Consistency | PASS | 55 commits: 54 com escopo `gif-sticker-search` e 1 `chore(jdi)`. O `1b66167` implementa a D-12 e não contradiz D-3, D-10, D-11 nem D-12. O SUMMARY não lista os arquivos que a migração tocou (W2) |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iteração não mudou a UI |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou. As 2 Manual são da release |

## Itens da rodada 2, iter 3
| Item | Estado | Evidência |
|---|---|---|
| W1 (revisor): local com nome de comando em `commands.rs` recusado (`fp1`, `fp2`) | **CLEARED** | Reapliquei os dois mutantes da iter 3:<br>- `fp2`: `let video_auto = blocking(…).await?; Ok(video_auto)` em `video_auto`. Linha 3: **`OK`**. `cargo clippy -p bezel-studio --all-targets -D warnings`: exit 0;<br>- `fp1`: `let preferences = state.preferences(); preferences` em `preferences`. Linha 3: **`OK`**. O clippy recusa esse por `let_and_return`, que é lint do projeto, não da guarda.<br>Os casos negativos continuam recusados (`commands_beside_locals`, 8 casos: uso antes do `let`, no próprio valor, depois do escopo, no `else` de um `if let`, com `self::`, numa função interna) |
| Crítico: wrapper de log de IPC em volta do `generate_handler!` (`logged(handler)` com `eprintln!("ipc {} {:?}", invoke.message.command(), invoke.message.payload())`) | **CLEARED** | Reapliquei o mutante (`ipc`) em `run`. O build de produção compila (`cargo build -p bezel-studio`: exit 0). A linha 3 como está escrita deu **exit 1**, e a guarda acusa:<br>- `` `Invoke` (an invocation the window sent) in production code `` (2×);<br>- `` `eprintln` prints or logs outside `diag.rs` ``;<br>- `` `payload` (an invocation the window sent) in production code `` |
| Sondas extras sobre o mesmo caminho | ver detalhes | Para não depender do nome `Invoke`, montei o wrapper como closure direto em `invoke_handler`, com um helper genérico `fn pass<T>(h: impl Fn(T) -> bool, t: T) -> bool` para tipar o `generate_handler!`:<br>- `ipcd`: `dbg!(&invoke.message)`. Compila. Linha 3: **exit 1** (`` `dbg` prints or logs outside `diag.rs` ``);<br>- `ipcf`: `std::fs::write(temp_dir().join("bezel-ipc.log"), format!("{:?}", invoke.message))`. Compila. Linha 3: **`OK`**. O `InvokeMessage` deriva `Debug` com o corpo (`tauri-2.12.0/src/ipc/mod.rs:497`), então o `{:?}` leva a chave sem nomear `payload`. A D-12 deixa isso fora do alcance de propósito ("files, events to the window … code review"). Não é engano realista (pede escrita em arquivo e o truque de inferência), então fica em Observações |

## Migração para `diag`: comportamento
Revisei os 41 pontos do diff do `1b66167`, um por um.

- **Nenhum `unwrap`/`expect` foi trocado.** No `1b66167^`, o código de produção do studio não tinha `unwrap`,
  `expect` nem macro de panic. Havia só o `debug_assert!` de `messages.rs:168` e o `cfg_attr(debug_assertions)` de
  `main.rs`. Portanto nenhum panic antigo virou "segue calado". A guarda só proíbe os futuros.
- **O fluxo de controle é o mesmo.**
  - Padrões trocados: `Err(e) => { log; X }` virou `Err(_) => { report; X }`; `if let Err(e) = f()` virou
    `if f().is_err()`; `inspect_err(|e| …)` virou `inspect_err(|_| …)`.
  - Os ramos e os valores de retorno não mudaram, e nenhum erro foi engolido onde antes subia. Continuam iguais:
    - o fallback `MemoryArchive` em `copies` (`lib.rs:514`);
    - os `return None` / `None` de `library.rs`, `manager.rs`, `thumbnails.rs` e `udev_help.rs`;
    - o `app.exit(0)` em `tray.rs`;
    - `video.failed(now)` e `record_probe(None)` em `studio.rs`;
    - `ExitCode::FAILURE` em `main.rs`.
  - Única diferença semântica: com `.is_err()` na condição, os temporários caem antes do corpo. Exemplos: a guarda
    de `backend.studio()` em `start_refresh_loop` (`lib.rs:595`) e o retorno de `open_at` (`backend.rs:919`).
    Isso só encurta o tempo de lock e não cria deadlock.
  - `restart_without_dmabuf_renderer` (`lib.rs:633`): `let _ = …exec();` é o mesmo, porque `exec` só volta em
    caso de falha.
- **Campos removidos:** `asset` em `VideoProbe`/`PosterRetake` servia só ao log. O código compila com clippy
  `-D warnings` e os testes de vídeo passam.
- **Os 4 antigos `eprintln!` continuam no stderr.** O canal `Terminal` usa `eprintln!("bezel-studio: {text}")`
  (`diag.rs:238`), com o mesmo prefixo. O texto de `Simulated` é idêntico ao antigo.
- **Os níveis foram preservados:** o `error!` antigo virou `Error` (`CopiesInMemory`), o `info!` virou `News`
  (`LiveScreenBack`) e os 35 `warn!` viraram `Warning`. O teste `what_the_terminal_shows_is_what_was_printed_before`
  trava isso.
- **O que se perdeu:** o texto do erro e o assunto (tema, tela, caminho, vídeo).
  - O studio não instala nenhum subscriber de `tracing` nem logger de `log` (`cargo tree -p bezel-studio -i
    tracing-subscriber`: nenhum pacote; nenhum `set_global_default`). Então os 37 diagnósticos de `tracing` já não
    apareciam em lugar nenhum em tempo de execução, antes e depois.
  - A perda que o usuário sente está no terminal, em 3 códigos (W1):
    - `NotStarted` (`main.rs:18`; antes trazia o erro do Tauri);
    - `DmabufRendererOn` (`lib.rs:637`; o erro do `exec`);
    - `RefreshLoopNotStarted` (`lib.rs:609`; o erro do `spawn`).
  - A mensagem do `debug_assert!` de `messages.rs:168` agora é fixa. Isso só afeta debug e testes.
- **Aceito pela D-12?** Sim, por construção: as funções do `diag` aceitam só `DiagCode`/`&'static str`.
- **Está documentado?**
  - Para quem desenvolve: sim. Está na doc do módulo e do enum (`diag.rs:24-27`: "What it is about … and the error's
    own text are not said"), no corpo do commit ("Lost: the error's text and the subject") e no SUMMARY.
  - No texto da D-12: não explicitamente.
  - Para o usuário: não. O `troubleshooting.md` › "Reporting a problem" só pede os logs da CLI e do serviço (W1).

## Guarda: falsos positivos
- **Na árvore real, nenhum falso positivo.**
  - `nothing_in_the_app_forges_an_invocation` passa no `HEAD`, no Linux (local e CI) e no Windows (CI, abaixo).
  - O W1 sumiu: o escopo é léxico (parâmetros, `let`, `let … else`, closures, braços de `match`, `if let`,
    `while let`, `for`), e cada `fn` começa com escopo vazio.
- **Falsos positivos teóricos que ficam.** Todos vêm de decisão ou são mais estritos que o texto. Nenhum pesa hoje:
  - `payload` e `Invoke` recusados como qualquer identificador, até um local ou um campo (`event.payload()`). A D-12
    nomeia os dois.
  - `stdout`/`stderr` como prefixo de qualquer identificador, inclusive `.stderr(Stdio::null())`, que silencia um
    filho (observação (a) da iter 3).
  - `unwrap`/`expect`/`unwrap_err`/`expect_err`/`panic_any` em qualquer lugar fora do `diag`, inclusive
    `Option::unwrap()` e `expect("texto fixo")`, que não imprimem valor, e um método próprio chamado `expect`.
    Isso vai além do texto da D-12, que fala em macros, mas é coerente com o gate 5.6.
  - Nomes ligados dentro dos tokens de uma macro (`matches!(x, Some(preferences))`) não entram no escopo do W1.

## Blockers
- nenhum

## Warnings
- **W1 — Os 3 diagnósticos de terminal que tinham detalhe perderam o texto do erro, sem registro para o usuário.**
  - **Onde:**
    - `main.rs:18` (`NotStarted`): antes `bezel-studio: {error}` com o `tauri::Error`; agora `bezel-studio: the app
      did not start`;
    - `lib.rs:637` (`DmabufRendererOn`, o erro do `exec`);
    - `lib.rs:609` (`RefreshLoopNotStarted`, o erro do `spawn`).
  - **Por que pesa:** como o studio não tem subscriber de `tracing`, esses 3 eram o único detalhe de falha visível,
    num terminal no Linux. Se a janela não abre (WebKitGTK, GTK, configuração), o relato de bug fica sem causa.
  - **O que dizem a decisão e as docs:** a D-12 aceita a perda por construção, mas não a diz com todas as letras.
    As docs do usuário também não dizem.
  - **Gravidade:** baixa, porque é uma troca de segurança decidida.
  - **Correção sugerida**, uma das duas, ambas dentro da D-12:
    - (a) separar `NotStarted` em códigos fixos por variante do `tauri::Error` (por exemplo setup, runtime/webview,
      outro), ainda sem valor;
    - (b) dizer a troca na D-12 e numa linha do `troubleshooting.md` (en/pt-BR): o studio imprime só frases fixas, e
      a causa de uma falha ao abrir não aparece.
- **W2 — O SUMMARY não lista os arquivos da migração.**
  - **O que falta:** a seção "Files modified" para em `src/{gifs,gifs/*,commands,lib,backend,dto,messages}.rs` e
    omite os 10 arquivos que o `1b66167` tocou fora do escopo do PLAN (T-4): `diag.rs`, `library.rs`, `main.rs`,
    `manager.rs`, `settings.rs`, `storage.rs`, `studio.rs`, `thumbnails.rs`, `tray.rs` e `udev_help.rs`.
  - **Contagem errada no commit:** ele diz "36 `tracing` macros". Contei 37 em `1b66167^`: 35 `warn!`, 1 `error!`
    e 1 `info!`. O total de 41 está certo.
  - **Gravidade:** baixa, só de rastreabilidade.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (`HEAD` = `779a1ed`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK`. Controle `ctl`: `OK`. Mutantes `ipc` (o do crítico) e `ipcd`: exit 1. `fp1`/`fp2`: `OK`, porque o falso positivo do W1 anterior saiu. `ipcf` (arquivo): `OK`, fora do alcance pela D-12 |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe` |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37013978498 terminar (`gh run watch --exit-status`: exit 0; `conclusion=success`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `779a1ed`: `OK` (`rust-windows` = `success`, **159** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 942/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.71% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37013978498, `779a1ed`)
- **Jobs:** `conclusion=success`.
  - success: `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust, javascript-typescript), Varreduras,
    Versao e `Portao`;
  - skipped, como antes: `sonar`, `imagem`, `publicar` e `lancar`.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` ok;
  - `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes (`cargo llvm-cov`): **871 passed, 0 failed, 12 ignored**;
  - unittests do studio: **`159 passed; 0 failed`**. São 157 da iter 3 mais os 2 de `diag::tests`;
  - `tests::nothing_in_the_app_forges_an_invocation`, `tests::the_source_guard_reads_identifiers_not_text` e os
    dois `diag::tests` rodaram ok.
- **`rust-linux`:** verde, com 941/0/12 e 162 no studio. Localmente deram 942: o doctest `compile_fail` de `diag.rs`
  roda no `cargo test`, não no `llvm-cov` do CI. Linux (162) − Windows (159) = os 3 testes de mock runtime que só
  rodam fora do Windows (D-8).

## Observações (sem aviso)
- **Limite declarado da D-12, confirmado pelo `ipcf`:** um wrapper que grava `format!("{:?}", invoke.message)` num
  arquivo passa a linha 3. Ele não nomeia `Invoke` nem `payload`, porque o `Debug` derivado do `InvokeMessage` leva
  o corpo. Arquivos e eventos para a janela ficam com a revisão de código (D-10/D-12). Outras rotas fora do alcance
  da guarda, todas deliberadas:
  - literais `/dev/tty` ou `/dev/pts/*` (o `STREAM_FILES` cobre stdout/stderr/fd e `CONOUT$`);
  - um processo filho que herda o stdio (`Command::new("echo").arg(…)`);
  - um `&'static str` montado por `String::leak` passado a uma futura função do `diag` que aceite texto.
- **`diag.rs` fala em "the log"**, mas o studio não tem log. Os `tracing::*` vão para o despachante nulo, o que já
  acontecia antes da fase. O código `code = ?code` só passa a servir se um dia houver um subscriber.
- **A guarda vai além do texto da D-12** sem contradizê-la:
  - `unwrap`/`expect`/`unwrap_err`/`expect_err`/`panic_any`;
  - `main` devolvendo `Result`;
  - `assert_eq!` e afins mesmo sem mensagem;
  - arquivos de stream em literais.

  Tudo isso está documentado nos `const` da guarda (`lib.rs:1163-1212`) e no commit.
- Continuam valendo as observações das iters anteriores:
  - o timeout de 10 s;
  - `collected_users` lendo a biblioteca;
  - os nomes reservados do Windows;
  - `klipy.json` sem nova tentativa;
  - o proxy;
  - a raiz temporária de teste;
  - o "bezel-klipy 97%" do SUMMARY (medi `client.rs` 90.04% e `dto.rs` 99.16% na iter 3; o crate não mudou);
  - a cobertura de `commands.rs` (10.72%).

## Recommendation
Os itens da rodada 2, iter 3 estão resolvidos:
- **Crítico (wrapper de log de IPC):** reaplicado, compila e reprova pela guarda por `Invoke`, `eprintln` e
  `payload`. A variante sem nomear `Invoke`/`payload` também reprova se imprime (`dbg!`).
- **W1 (local com nome de comando):** `fp1` e `fp2` passam; os casos negativos seguem recusados.

A migração para o `diag` não muda o fluxo de controle. Nenhum `unwrap`/`expect` existia para ser trocado, e os 4
avisos de terminal continuam no stderr. Só se perde o texto do erro, como a D-12 escolheu.

Os gates 1–8 passam. O CI do `HEAD` está verde, com 159 testes do studio no Windows.

Antes de fechar a fase:
1. **W1:** códigos fixos por variante para a falha ao abrir, ou registrar a troca na D-12 e no `troubleshooting.md`.
2. **W2:** atualizar a lista de arquivos do SUMMARY (e o "36" do commit, se o SUMMARY o repetir).

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco … chave privada»: oca e objetiva — engano realista dentro do que a D-10 afirma:
  `KeyJson.key` (`gifs/key.rs:150`) é uma `String` com a chave; um erro de `KeyFile::load` que cita o valor lido
  (`format!("… (found {found:?})")`) passa a linha e, com um `klipy.json` danificado (customerId inválido, chave com
  espaço no fim), o comando `klipy_key` devolve a chave inteira à janela. O wrapper de log de IPC da iteração anterior
  agora falha.
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
