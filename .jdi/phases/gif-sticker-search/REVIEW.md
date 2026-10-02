# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 3, iteração 1 (11ª no total, depois do AUTO-RESET 2) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `b90b347`, igual ao remoto do início ao fim (conferido de
> novo, com `git fetch`, antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (105 arquivos, 63 commits),
> com atenção aos commits da iteração:
> - `c05f5b1` (`fix(gif-sticker-search): no logger in the studio; panics say no message`):
>   - a guarda lê `cargo metadata --locked --all-features` (`studio_metadata`, `logging_problems`);
>   - recusa no código `LOG_INSTALLERS`, caminhos com raiz em `LOGGER_CRATES`, `set_hook` fora de
>     `diag::hook_panics`, `take_hook`/`update_hook` e um `main` cuja primeira instrução não é `diag::hook_panics()`;
>   - teste novo `tests::the_studio_installs_no_logger`;
>   - panic hook `diag::hook_panics` e teste `diag::tests::a_panic_says_where_never_what`, que roda a si mesmo como
>     processo filho;
>   - `NoWindow` e `SetupFailed` removidos (45 códigos);
> - `86f5c99`: tabela "The app does not open" / "O aplicativo não abre" do `troubleshooting.md` refeita;
> - `b90b347`: linha 3 do DoD com 12 testes, e o SUMMARY.
>
> Li D-1..D-14, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 2 iter 5, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos vêm das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Extraí os `Verify:` do Markdown por script, sem editar, e rodei
>   cada um com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`. A única execução do app foi a do build de revisão
>   (`target/review/debug/bezel-studio`, copiado para o scratchpad), com `env -i`, sem `DISPLAY`/D-Bus e com
>   `HOME`/`XDG_*` temporários. Ele cai na criação do loop de eventos do GTK, antes de qualquer plugin ou pasta.
> - **Mutações:** em cópias por `git archive` de `b90b347` no scratchpad (`rev11/mut/<mutante>`). Cada cópia tem um
>   `git init` próprio, com `origin/main` = `e527240` buscado do repositório, para a linha 3 rodar como está escrita,
>   e um `CARGO_TARGET_DIR` próprio (`target/rev11mut/<mutante>`). Nos mutantes de manifesto, o `Cargo.lock` foi
>   atualizado só com `cargo metadata --offline`, como faria quem comete a mudança: só ganha linhas. O repositório
>   não foi alterado. Controle sem mutação (`ctl`): a linha 3 dá `OK`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **945 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. São 2 a mais que na iter anterior (943): `tests::the_studio_installs_no_logger` e `diag::tests::a_panic_says_where_never_what`. Studio lib: **165** no Linux. Bate com o SUMMARY (945) |
| Coverage | PASS | **94.84%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.78%**. Studio: `diag.rs` 98.84% (o hook roda no processo filho e conta), `lib.rs` 94.53%, `gifs.rs` 96.87%, `gifs/key.rs` 92.13%, `gifs/asked.rs` 50.00%, `commands.rs` 10.72% (invólucros finos, como antes) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o `rust-windows` passou `clippy --all-targets --all-features`. Nenhum `allow` novo: os 4 de `rtss.rs` são anteriores à fase e têm `reason =` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. A iteração só mexe no studio e nas docs. 5.5: `unsafe` só em `rtss.rs` (anterior), e o studio tem `#![forbid(unsafe_code)]`. 5.6: os `unwrap`/`panic!` novos estão em `diag.rs:421-451`, dentro de `#[cfg(test)]`; o hook usa `unwrap_or_default`. 5.10: nenhum comando novo. `cargo audit`: exit 0 (620 crates). Nenhum segredo. `Cargo.toml` e `Cargo.lock` sem mudança desde `87d710c` |
| Consistency | PASS | 63 commits: 62 com escopo `gif-sticker-search` e 1 `chore(jdi)`. `c05f5b1` implementa a D-14 como escrita: manifesto e features resolvidas pelo `cargo metadata`, nenhum instalador no código, um panic hook primeiro no `main` com `DiagCode::Panicked` e o lugar, e o guia com o que aparece. Não contradiz D-10, D-11 nem D-12. Ver W1 sobre o texto da D-13 |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iteração não mudou a UI (`git diff 87d710c..HEAD -- apps/bezel-studio/src apps/bezel-studio/tests` vazio) |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou. As 2 Manual são da release |

## Itens da rodada 2, iter 5
| Item | Estado | Evidência |
|---|---|---|
| Crítico: o logger `--verbose` da CLI copiado para o studio (feature `tracing` do tauri + `tracing-subscriber`, já no lock) passava a linha 3, e o Tauri gravava o corpo de cada IPC | **CLEARED** | Reapliquei o mutante (`crit`). No manifesto do studio, `tauri` com `"tracing"` e `tracing-subscriber` igual ao da CLI. Em `run()`, `tracing_subscriber::fmt().with_env_filter("trace").with_writer(std::io::stderr).init()` atrás de `BEZEL_VERBOSE`. O lock ganha 4 linhas. Resultado: linha 3 **exit 1**, com 7 achados. Entre eles: `Cargo.toml: turns tauri's tracing feature on`, `depends on tracing-subscriber`, `resolved: tauri / tauri-macros / tauri-runtime-wry / wry is built with its tracing feature on` e, no código, `tracing_subscriber::fmt is a logger installer's crate` e `stderr names an output stream outside diag.rs`. Cada parte sozinha também reprova (tabela abaixo) |
| W1 (revisor): `NoWindow`/`SetupFailed` nunca saíam no terminal, e o guia descrevia linhas que o usuário não via | **CLEARED** | Os dois códigos saíram: `git grep` não acha `NoWindow`, `SetupFailed`, `its setup failed` nem `could not be made` fora de `.jdi`. As linhas do guia agora existem. No build de revisão sem sessão gráfica, a saída inteira é `bezel-studio: the app panicked at tao-0.37.1/src/platform_impl/linux/event_loop.rs:217:53`, com exit **101**, igual à primeira linha da tabela. A falha de setup cai em `panic!("Failed to setup app: {e}")` de `tauri-2.12.0/src/app.rs:1444`, que o hook imprime como `tauri-2.12.0/src/app.rs:1444:…`, e é isso que as linhas de pastas, bandeja e janela descrevem. en e pt-BR têm as mesmas linhas, e o `check-docs.sh` passa |

## Mutações (linha 3 como está escrita)
| Mutante | O que muda | Linha 3 | Quem pega |
|---|---|---|---|
| `ctl` | nada | `OK` | controle |
| `crit` | o do crítico: feature `tracing` do tauri + `tracing-subscriber` + instalação em `run()` | exit 1 | `the_studio_installs_no_logger` e a guarda (7 achados) |
| `feat` | só `"tracing"` nas features do `tauri` do studio | exit 1 | declarado e resolvido (`tauri`, `tauri-macros`, `tauri-runtime-wry`, `wry`) |
| `dep` | só `tracing-subscriber` no `[dependencies]` do studio, sem código | exit 1 | `depends on tracing-subscriber` e `resolved: the studio is built with tracing-subscriber` |
| `ws` | o logger movido para `bezel-media` (`pub fn verbose_logs()`, dependência normal), chamado por `run()` | exit 1 | grafo resolvido: `the studio is built with tracing-subscriber` |
| `hookmsg` | o hook imprime também o payload `&str` | exit 1 | `a_panic_says_where_never_what` (a linha sai com `fake-KLIPY_key-…`) e a guarda (`payload` em código de produção) |
| `hookexit` | o hook chama `std::process::exit(0)` depois da linha (um panic deixaria de falhar o app) | exit 1 | `a_panic_says_where_never_what` (o filho sai no 1º panic: 1 linha em vez de 5) e a guarda (`std::process::exit` em `diag.rs`) |

## Panic hook: regressões
- **O app ainda sai com código diferente de zero num panic.** O hook só imprime e volta, e o unwinding segue como
  antes:
  - panic na thread principal: exit **101**, conferido no binário;
  - no Linux, o `Ready` do Tauri (onde cai o setup) é chamado pelo laço Rust de `run_return` do tao
    (`event_loop.rs:1040-1130`), e não por um callback C, então o panic sobe até o `main`;
  - nenhum mutante que troque isso passa (`hookexit`).
- **Panics em threads de fundo continuam aparecendo**, agora como uma linha com o lugar e sem a mensagem. O hook é do
  processo. O teste novo faz 5 panics em threads próprias, e saem 5 linhas no Linux e no Windows (CI). Como antes, a
  thread morre e o app segue.
- **O `cargo metadata` do teste falha fechado** (rodei o binário de teste direto):
  - sem `CARGO`, usa o `cargo` do `PATH`: passa;
  - com `CARGO_NET_OFFLINE=true` e o cache local: passa (1,3 s);
  - com `CARGO=/bin/false`: falha (`assert` em `lib.rs:3103`);
  - com `CARGO` inexistente: falha (`unwrap` em `lib.rs:3101`);
  - com `CARGO_HOME` vazio e offline: falha (`no matching package named thiserror`);
  - um grafo sem o studio é um problema (testado).

  No CI, `the_studio_installs_no_logger` e `nothing_in_the_app_forges_an_invocation` passaram no Linux e no Windows,
  sob `cargo llvm-cov`.
- **Nenhum texto do erro vaza.** O hook não lê o payload. O lugar sai cortado da pasta do crate em diante
  (`tao-0.37.1/src/…`, `src-tauri/src/diag.rs`), com `/` e `\` tratados. O teste confere isso no Windows também.

## Blockers
- nenhum

## Warnings
- **W1 — A D-13 ainda lista "no window, setup" entre os `DiagCode`s de falha de início, e a D-14 não a emenda.**
  - **Onde:** `.jdi/decisions/D-2026-10-01-gif-sticker-search-13.md:1` ("Start failures are told apart by fixed
    `DiagCode`s (no window, setup, plugin, folders, tray, …)") contra `apps/bezel-studio/src-tauri/src/diag.rs:41-56`
    e `lib.rs:140-145`. Nesses pontos, `NoWindow`/`SetupFailed` não existem mais, e essas falhas saem como
    `Panicked` + lugar.
  - **Por quê:** a mudança está certa (era o W1 anterior, opção (b)) e está no commit e no guia. Mas a D-14 diz só
    "completes D-12", e o registro travado da D-13 continua descrevendo códigos que o código não tem. É o mesmo tipo
    de deriva do W2 da rodada 2, iter 2 (D-10 desatualizada).
  - **Gravidade:** baixa. Não há comportamento errado: as falhas de janela e de setup continuam se distinguindo, por
    `Panicked` e pelo lugar, sem texto do erro.
  - **Correção:** uma linha na D-14 (ou numa D-15): "amends D-13: Tauri 2.12 panics on window and setup failures, so
    `NoWindow`/`SetupFailed` are removed and those failures are told by `DiagCode::Panicked` and the place".

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (`HEAD` = `b90b347`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK` (12 testes). Controle `ctl`: `OK`. `crit`, `feat`, `dep`, `ws`, `hookmsg` e `hookexit`: todos exit 1 |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` (`# pass 4`, `test:unit` ok, `6 tests × 4 projects, 24/24 runs passed with axe`) |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` (o `check-docs.sh` passa com a tabela nova do `troubleshooting.md`) |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37025240721 terminar (`gh run watch --exit-status`: exit 0; `conclusion=success`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `b90b347`: `OK` (`rust-windows` = `success`, **162** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 945/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.78% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37025240721, `b90b347`)
- **Jobs:** `conclusion=success`.
  - success: `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust, javascript-typescript), Varreduras,
    Versao e `Portao`;
  - skipped, como antes: `sonar`, `imagem`, `publicar` e `lancar`.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` e `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes (`cargo llvm-cov`): **874 passed, 0 failed, 12 ignored**;
  - unittests do studio: **`162 passed; 0 failed`**. São os 160 da iter anterior mais `the_studio_installs_no_logger`
    e `a_panic_says_where_never_what`;
  - nos dois testes novos, o `cargo metadata` rodou dentro do teste no Windows, e o processo filho do panic também
    (com `current_exe`, sob `llvm-cov`, com caminhos `\` cortados para `src-tauri/src/diag.rs`). Também passaram
    `nothing_in_the_app_forges_an_invocation` e `key_never_reaches_the_window`.
- **`rust-linux`:** **944/0/12**, com 165 no studio. Localmente deram 945: o doctest `compile_fail` de `diag.rs` roda
  no `cargo test`, não no `llvm-cov` do CI. Linux (165) − Windows (162) = os 3 testes de mock runtime que só rodam
  fora do Windows (D-8).

## Observações (sem aviso)
- **O hook tira o nome da thread e o backtrace.** `RUST_BACKTRACE` deixa de valer, porque o hook padrão é
  substituído. É de propósito (D-14), e nenhuma doc manda usar `RUST_BACKTRACE`. Para depurar, ainda dá para ler o
  lugar exato do panic.
- **Corte do caminho, caso teórico.** Um arquivo fora de qualquer `src/` cujo caminho tenha uma pasta-mãe chamada
  `src` sairia com as pastas acima. Exemplo: código incluído de `OUT_DIR` num build feito em `~/src/…`, que imprimiria
  `<usuário>/src/…/out/x.rs`. O studio usa `generate_context!` no próprio `lib.rs` (sem `include!` de `OUT_DIR`; a
  guarda recusa `include!`), e builds de release vêm do runner. Não achei caminho real.
- **O canal "log" do `diag` agora nunca tem destino.** 34 dos 45 códigos (32 avisos, 1 erro, 1 notícia) vão para o
  `tracing`. A D-14 proíbe instalar um subscriber, então ninguém os lê, e os docs de `diag.rs:1,33,293` ainda falam em
  "the log". Já era assim antes da fase, sem subscriber, mas agora é permanente. Sugestão de backlog: mover os que
  importam para o terminal ou registrar a escolha.
- **O teste de metadata resolve todos os alvos.** Na primeira execução sem cache completo, ele precisa de rede para
  baixar os manifestos. Sem rede, falha fechado, como deve.
- **Continuam valendo as observações das iters anteriores:**
  - o timeout de 10 s;
  - `collected_users` lendo a biblioteca;
  - os nomes reservados do Windows;
  - `klipy.json` sem nova tentativa;
  - o proxy;
  - a raiz temporária de teste;
  - o limite declarado da D-12 (arquivo/evento);
  - o IPC do Tauri guardando o argumento como `serde_json::Value`;
  - a cobertura de `commands.rs` (10.72%).

## Recommendation
Os itens da rodada 2, iter 5 estão resolvidos:
- **Crítico (logger da CLI no studio):** reaplicado, ele reprova a linha 3. Também reprovam cada uma das partes
  sozinha (feature, dependência) e o logger movido para outro crate do workspace.
- **W1:** os dois códigos inalcançáveis saíram, e as linhas do guia agora existem. A do tao foi conferida no binário,
  e a do setup confere com `tauri-2.12.0/src/app.rs:1444`.

O panic hook não regride:
- o app sai com 101;
- panics em threads de fundo aparecem;
- o payload nunca sai;
- o `cargo metadata` falha fechado e passa no Linux e no Windows.

Os gates 1–8 passam. O CI do `HEAD` está verde, com 162 testes do studio no Windows.

Antes de fechar a fase:
1. **W1:** registrar na D-14 (ou numa D-15) que ela emenda a D-13, com `NoWindow`/`SetupFailed` removidos e essas
   falhas ditas por `Panicked` + lugar.

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «1 | Core … sem KLIPY/HTTP»: oca e objetiva — um GET escrito à mão com `std::{…, net::TcpStream}` no core
  passa (o Verify prova "nenhuma crate HTTP", não "nenhum HTTP").
- DoD row «3 | Studio e disco … nada sai … no início»: oca e objetiva — engano realista: um passo de boas-vindas no
  `setup` que, sem chave, abre `partner.klipy.com` no navegador do sistema (`try_state::<Opener>` + `open_url`) passa;
  o teste do `setup` só roda com chave salva e a guarda só conhece as APIs da janela. O logger da iteração anterior
  agora falha.
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
