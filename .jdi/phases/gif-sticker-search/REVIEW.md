# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 3, iteração 2 (12ª no total, depois do AUTO-RESET 2) do loop autônomo
> (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` = `ef26a83`, igual ao remoto do início ao fim (conferido de
> novo, com `git fetch`, antes da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (106 arquivos, 66 commits),
> com atenção aos commits da iteração:
> - `90db1a1` (orquestrador): D-15 e a linha 1 do DoD, que agora também recusa `TcpStream|UdpSocket|TcpListener|net::`
>   em `crates/bezel-core`;
> - `0bfa420` (`fix(gif-sticker-search): guard fences sockets, programs and opener`):
>   - a guarda de fonte do studio recusa sockets (`SOCKETS`), caminhos por `net` e crates de saída (`WAY_OUT_CRATES`);
>   - recusa também webviews feitos em Rust (`WEBVIEWS`);
>   - aceita `Command`/`CommandExt` só em `restart_without_dmabuf_renderer`, como o único
>     `std::process::Command::new` do nome ligado uma vez a `std::env::current_exe()`, sem macro;
>   - aceita o opener (`OPENER`) só em `open_fixed` de `commands.rs` e no `use tauri_plugin_opener::OpenerExt as _;`
>     do topo;
>   - `open_fixed` só pode ser citado na definição e por `open_link`/`open_guide`;
>   - `the_source_guard_reads_identifiers_not_text` ganhou 49 casos recusados (31 em `sockets_and_programs`, 18 em
>     `pages_opened`) e 3 aceitos (o re-exec, os dois comandos com o helper e `commands.rs` inteiro);
>   - em `commands.rs` e no re-exec, só mudaram comentários;
> - `ef26a83`: o SUMMARY.
>
> Li D-1..D-15, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 3 iter 1, inclusive a seção do crítico.
> `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos vêm das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Extraí os `Verify:` do Markdown por script, sem editar, e rodei
>   cada um com `bash`.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da chave.
>   Playwright com `BEZEL_E2E_PORT=1442`.
>   - A única execução do app foi a do build de revisão (`target/review/debug/bezel-studio`), com `env -i`, sem
>     `DISPLAY`/D-Bus, com `HOME`/`XDG_*` temporários e um shim `LD_PRELOAD` que só anota os `exec*`. O app cai na
>     criação do loop de eventos do GTK, antes de qualquer plugin ou pasta.
>   - O mutante `cfgwin` (abaixo) só foi compilado e testado, nunca executado.
> - **Mutações:** em cópias por `git archive` de `ef26a83` no scratchpad (`rev12/mut/<mutante>`).
>   - Cada cópia tem um `git init` próprio, com `origin/main` = `e527240` buscado do repositório, para as linhas
>     rodarem como estão escritas.
>   - Cada cópia tem um `CARGO_TARGET_DIR` próprio (`target/rev12mut/<mutante>`, semeado por cópia reflink de
>     `target/review`).
>   - O repositório não foi alterado (`git status`: só o `LOOP.md` não versionado, como no início).
>   - Controle sem mutação (`ctl`): as linhas 1 e 3 dão `OK`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **945 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. É o mesmo total da iter anterior: os 52 casos novos estão dentro de `the_source_guard_reads_identifiers_not_text`, e nenhum teste saiu. Studio lib: **165** no Linux. Bate com o SUMMARY (945) |
| Coverage | PASS | **94.88%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.83%**. Studio: `lib.rs` 95.17% (antes 94.53%), `diag.rs` 98.84%, `gifs.rs` 96.87%, `gifs/key.rs` 92.13%, `gifs/asked.rs` 50.00%, `commands.rs` 10.72% (invólucros finos, como antes) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local com `--target x86_64-pc-windows-msvc`, sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o `rust-windows` passou `clippy --all-targets --all-features`. Nenhum `allow` novo: os 4 de `rtss.rs` são anteriores à fase e têm `reason =` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. 5.2 e 5.9: as ocorrências no core são comentários e testes anteriores à fase, e nenhuma linha `+` do diff do core casa. 5.5: `unsafe` só em `rtss.rs` (anterior); studio com `#![forbid(unsafe_code)]`. 5.6: todo o código novo de `lib.rs` está dentro de `mod tests`. 5.10: nenhum comando novo. `cargo audit`: exit 0 (620 crates). Nenhum segredo. Ver W3 sobre o alcance do 5.2 |
| Consistency | PASS | 66 commits: 65 com escopo `gif-sticker-search` e 1 `chore(jdi)`. `0bfa420` só toca `lib.rs` e `commands.rs` (arquivos da T-4). Implementa a D-15 como escrita (sockets, `Command` só no re-exec, opener só no helper de `open_link`/`open_guide`) e vai além dela (`WAY_OUT_CRATES`, `WEBVIEWS`, sockets Unix). Não contradiz D-10..D-14. A D-15 emenda a D-13 por escrito ("amends D-13's list of start-failure codes"), e o `DECISIONS.md` renderizado a traz |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **231/231**, 99.94% de linhas. Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`). A iteração não mudou a UI |
| DoD | PASS | As 10 linhas Auto passam. A linha 7 rodou depois que o CI do `HEAD` terminou. As 2 Manual são da release |

## Itens da rodada 3, iter 1
| Item | Estado | Evidência |
|---|---|---|
| W1 (revisor): a D-13 listava "no window, setup" entre os `DiagCode`s de início, e a D-14 não a emendava | **CLEARED** | `D-2026-10-01-gif-sticker-search-15.md:1`: "amends D-13's list of start-failure codes, which D-14 replaced by the panic line". O `DECISIONS.md` renderizado tem a D-15. O código não mudou nesse ponto: `diag.rs` sem `NoWindow`/`SetupFailed` |
| Crítico, linha 1: um GET escrito à mão no core com `use std::{…, net::TcpStream}` passava | **CLEARED** | Reapliquei (`crit1`): `fetch_trending` em `domain/gifs.rs` com `use std::{io::{Read, Write}, net::TcpStream}` e `TcpStream::connect((host, 80))`. Compila. Resultado: linha 1 **exit 1**. O grep acha `gifs.rs:721 net::TcpStream` e `gifs.rs:723 TcpStream::connect`. O gate 5.2 continua sem achar (ver W3) |
| Crítico, linha 3: um passo de boas-vindas no `setup` que, sem chave, abre `partner.klipy.com` no navegador do sistema passava | **CLEARED** | Reapliquei as duas formas:<br>• `crit3a`: `try_state::<tauri_plugin_opener::Opener<R>>()` + `open_url`;<br>• `crit3b`: `use tauri_plugin_opener::OpenerExt as _;` + `app.opener().open_url`.<br>Ambas ficam atrás de `!folders.config.join(KEY_FILE).exists()` e `link_url("klipyPartnerPanel")`. As duas compilam, e a linha 3 dá **exit 1** nas duas. Quem pega é `nothing_in_the_app_forges_an_invocation`, com `Opener`/`OpenerExt`/`opener`/`open_url` "(the system opener) outside `open_fixed` in `commands.rs`". `the_app_setup_sends_nothing_at_start` continua passando com o mutante (o runtime mock não registra o opener): a guarda é quem prova |

## Mutações
| Mutante | O que muda | Linha | Resultado | Quem pega |
|---|---|---|---|---|
| `ctl` | nada | 1 e 3 | `OK` / `OK` | controle |
| `crit1` | GET à mão no core por `std::{…, net::TcpStream}` | 1 | exit 1 | grep `net::`/`TcpStream` da linha 1 |
| `crit3a` | `setup` sem chave abre o Painel por `try_state::<Opener>` | 3 | exit 1 | guarda (4 achados) |
| `crit3b` | `setup` sem chave abre o Painel por `OpenerExt` | 3 | exit 1 | guarda (3 achados) |
| `curl1` | core roda `curl` por `use std::{process::Command}` | 1 | **`OK`** | ninguém: nem a linha 1 nem o gate 5.2 (W3) |
| `js2` | `app.js` abre o Painel na partida quando não há chave (2 linhas) | 5 (e 3) | **`OK`** | ninguém. Um spec descartável na cópia mostra `data-demo-link="klipyPartnerPanel"` logo ao carregar `?demo=gifsNoKey`, e no controle o atributo é `null` (W1) |
| `cfgwin` | segunda janela em `tauri.conf.json` com `"url": "https://partner.klipy.com"` | 3 | **`OK`** | ninguém (W2) |

Sonda da guarda: o próprio `guard()` dos testes, numa cópia, sobre trechos soltos:
- **Recusados:**
  - `crate::commands::open_link(...)` chamado do `setup` (regra da D-11);
  - o opener em `tray.rs`;
  - `use std::os::unix::{net as u}` + `UnixStream`;
  - `open_fixed` chamado por outro comando de `commands.rs`.
- **Aceitos:**
  - `use tauri_plugin_updater::UpdaterExt as _;` + `updater().check()` no `setup`, e `.plugin(tauri_plugin_updater::Builder::new().build())`;
  - `nix::unistd::execvp`;
  - `app.restart()`;
  - `app.emit("open-partner", ())` (ver W1).

## Guarda nova: falsos positivos, re-exec e links
- **Árvore real, sem falso positivo.** `nothing_in_the_app_forges_an_invocation` passa no Linux (local e CI) e no
  Windows (CI). A guarda acha exatamente:
  - `spawns = ["lib.rs: restart_without_dmabuf_renderer"]`;
  - `opens = ["commands.rs: open_fixed"]`;
  - `helpers = ["commands.rs: open_guide", "commands.rs: open_link"]`.

  `commands.rs` inteiro (`include_str!`) está entre os casos aceitos. No Windows o re-exec é `cfg(linux)`, mas a guarda
  lê o texto, e a asserção vale lá também. Nenhum arquivo de `src/` tem hoje um identificador `Command`, `opener`,
  de socket ou um caminho por `net` fora desses pontos.
- **Falsos positivos possíveis, por identificador** (sonda): um campo chamado `opener`, uma variante de enum `Command`
  e um módulo próprio chamado `net` (`net::x()`) são recusados. Hoje nada disso existe. É o custo esperado de uma
  guarda por nome (D-10), mas um módulo `net` para sensores de rede ou um menu com `Command` vão esbarrar nela. Fica
  como observação.
- **Re-exec sem DMA-BUF, igual a antes.** O corpo da função não mudou nesta iteração (só o comentário). Contra o
  `main`, a única troca continua sendo o `eprintln!` → `diag::report(restart_failure(&error))` da D-12. No binário de
  revisão, com o shim que anota `exec*`:
  - **sem a variável:** o processo inicia com `switch=(unset)`, faz um `execvp` de `target/review/debug/bezel-studio`
    com `WEBKIT_DISABLE_DMABUF_RENDERER=1` (mesmo pid) e reinicia com `switch=1`. Não há segundo `exec`. A saída é
    `bezel-studio: the app panicked at tao-0.37.1/src/platform_impl/linux/event_loop.rs:217:53`, com exit **101**;
  - **com `WEBKIT_DISABLE_DMABUF_RENDERER=0`** (escolha do usuário): nenhum `exec` do studio;
  - **com o arquivo do programa apagado** (executado por `/proc/self/fd/3`): o `execvp` de `… (deleted)` falha, sai
    `bezel-studio: could not restart with the DMA-BUF renderer off: the app's program file is gone`
    (`DmabufRestartNoFile`) e o app segue até o mesmo panic do GTK.
- **`open_link`/`open_guide`, iguais a antes.** `open_fixed` já existia em `b90b347`; `0bfa420` só documenta.
  - `open_guide` faz `guide_url(&page, &language)?` e depois `spawn_blocking(move || app.opener().open_url(url, None))`,
    com os mesmos dois `map_err(UiError::system)`. É o mesmo corpo do `main`, só que movido para o helper.
  - `open_link` passa por `link_url` (lista fixa, `partner_panel_is_the_only_new_link` na linha 3).
  - O plugin continua com `open_js_links_on_click(false)`, e as capabilities não têm `opener:`.
  - No e2e, os dois botões do popover passam nos 4 projetos (`data-demo-link`, `data-demo-guide`).
  - Não abri o navegador real de propósito: isso iria ao KLIPY.

## Blockers
- nenhum

## Warnings
- **W1 — A UI pode abrir o Painel do KLIPY na partida, sem clique, e as linhas 3 e 5 passam.**
  - **Onde:** em `apps/bezel-studio/src/app.js:37`, duas linhas depois de `applyTranslations` (mutante `js2`):
    `bridge.klipyKey().then((k) => (k?.configured ? null : bridge.openLink(PARTNER_PANEL)))`.
  - **Por que passa:**
    - a guarda aceita, porque `open_link` é invocado por IPC e chama `open_fixed`;
    - `e2e/gif-search.spec.mjs:64-93` (`no key: help`) só confere `data-demo-link` depois do clique no botão e nunca
      confere que ele está vazio antes;
    - nenhum teste de unidade olha isso.
  - **Por que importa:** a D-15 diz "so a page opens only on the user's click", e a D-11 diz que o lado da UI é
    provado pela linha 5, mas só para chamadas de GIF. É o mesmo engano realista do crítico da iter 1 (boas-vindas
    que abrem o Painel sem chave), escrito em JS em vez de Rust. Também passa um `emit` do `setup` que a janela
    transforma em `openLink` (sonda).
  - **Correção sugerida:**
    - no e2e `no key: help` (e num cenário com chave), conferir que `data-demo-link` e `data-demo-guide` estão
      ausentes ao carregar, ao abrir a Coleção e ao abrir o diálogo, até o clique;
    - ou registrar na demo cada chamada de `openLink`/`openGuide` com o gesto que a causou.
- **W2 — Uma janela de config com URL externa abre o KLIPY na partida, e a linha 3 passa.**
  - **Onde:** em `apps/bezel-studio/src-tauri/tauri.conf.json:11-22` (`app.windows`), uma segunda janela
    `{"label": "welcome", "url": "https://partner.klipy.com"}` (mutante `cfgwin`).
  - **Por que passa:** compila, e a linha 3 dá `OK`. O Tauri cria as janelas da config na partida (`create` é `true`
    por padrão), e o CSP da config não vale para uma página externa. A linha 3 só compara a linha `"csp"`, as
    sobreposições e as capabilities. A guarda só lê Rust e recusa `WebviewUrl`/`WebviewWindowBuilder` em código, não
    em JSON.
  - **Ainda fora da cerca da D-15:** um plugin que faz HTTP sem ser nomeado em `WAY_OUT_CRATES`. Exemplo:
    `tauri_plugin_updater` com `updater().check()` no `setup`; ele usa `reqwest` por dentro, e a linha 4 só olha
    `ureq`. A D-15 diz "every way out of the computer is named and fenced" e "HTTP leaves only through
    `bezel-klipy`".
  - **Correção sugerida:**
    - na linha 3: `app.windows` com exatamente uma janela, `main`, sem `url` externa;
    - na guarda: ler o grafo resolvido (o `cargo metadata` da D-14 já está lá) e recusar `reqwest`/`hyper`/`isahc`/
      `curl` construídos para o studio em desktop, em vez de uma lista de nomes no código.
- **W3 — O core pode rodar `curl` (`use std::{process::Command}`), e a linha 1 e o gate 5.2 passam.**
  - **Onde:** mutante `curl1`, em `crates/bezel-core/src/domain/gifs.rs`.
  - **Por que passa:** a linha 1 (`CONTEXT.md:25`) só recusa `klipy|TcpStream|UdpSocket|TcpListener|net::`. O
    regex do 5.2 (`std::(fs|process|net|thread)\b`) não casa um import agrupado, a mesma brecha que o crítico usou
    com `net`.
  - **Gravidade:** baixa. A árvore real está limpa (nenhum `use std::{` no core), e "a core spawning curl" é menos
    provável que o GET do crítico.
  - **Correção sugerida:** a linha 1 também recusar `process::`, `fs::`, `thread::` e `\bCommand\b` (sensível a
    maiúsculas) em `crates/bezel-core/src`.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (`HEAD` = `ef26a83`). Controle `ctl`: `OK`; `crit1`: exit 1; `curl1`: `OK` (W3) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, itens só da última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK` (12 testes). `ctl`: `OK`; `crit3a` e `crit3b`: exit 1; `cfgwin`: `OK` (W2) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` (`# pass 4`, `test:unit` ok, `6 tests × 4 projects, 24/24 runs passed with axe`). `js2`: `OK` (W1) |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37030664822 terminar (`gh run watch --exit-status`: exit 0; `conclusion=success`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `ef26a83`: `OK` (`rust-windows` = `success`, **162** testes do studio no Windows) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 945/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.83% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37030664822, `ef26a83`)
- **Jobs:** `conclusion=success`.
  - success: `rust-windows`, `rust-linux`, `node-ui`, CodeQL (actions, rust, javascript-typescript), Varreduras,
    Versao e `Portao`;
  - skipped, como antes: `sonar`, `imagem`, `publicar` e `lancar`.
- **`rust-windows`:**
  - `cargo fmt --all -- --check` e `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes (`cargo llvm-cov`): **874 passed, 0 failed, 12 ignored**;
  - unittests do studio: **`162 passed; 0 failed`**, o mesmo número da iter anterior, porque os casos novos estão
    dentro de testes que já existiam;
  - passaram `nothing_in_the_app_forges_an_invocation`, `the_source_guard_reads_identifiers_not_text`,
    `the_studio_installs_no_logger` e `a_panic_says_where_never_what`.
- **`rust-linux`:** **944/0/12**, com 165 no studio. Localmente deram 945: o doctest `compile_fail` de `diag.rs` roda
  no `cargo test`, não no `llvm-cov` do CI. Linux (165) − Windows (162) = os 3 testes de mock runtime que só rodam
  fora do Windows (D-8).

## Observações (sem aviso)
- **`spawns_itself` exige o nome ligado por `let Ok(name) = std::env::current_exe() else { … };`.** Qualquer
  refatoração do re-exec (`match`, `?`, um helper) vai ser recusada até a guarda acompanhar. É o comportamento
  pedido pela D-15, e o comentário em `lib.rs:668-671` avisa.
- **`nix::unistd::execvp` e crates de processo fora da lista passam a guarda** (sonda). Precisam de uma dependência
  nova e de intenção. Ficam com a revisão de código (D-10).
- **Continuam valendo as observações das iters anteriores:**
  - o hook sem nome de thread nem backtrace;
  - o corte do caminho no caso teórico de `OUT_DIR`;
  - o canal "log" do `diag` sem destino;
  - o teste de metadata, que precisa de rede no primeiro cache;
  - o timeout de 10 s;
  - `collected_users` lendo a biblioteca;
  - os nomes reservados do Windows;
  - `klipy.json` sem nova tentativa;
  - o proxy;
  - a raiz temporária de teste;
  - o limite declarado da D-12;
  - o IPC do Tauri guardando o argumento como `serde_json::Value`;
  - a cobertura de `commands.rs` (10.72%).

## Recommendation
Os itens da rodada 3, iter 1 estão resolvidos:
- **W1 (D-13):** a D-15 a emenda por escrito.
- **Crítico, linha 1:** reaplicado, o GET por `std::{…, net::TcpStream}` reprova a linha 1.
- **Crítico, linha 3:** reaplicado nas duas formas (`Opener` e `OpenerExt`), o mutante reprova a linha 3 pela guarda.

A guarda nova não tem falso positivo na árvore real (Linux e Windows). O re-exec sem DMA-BUF e `open_link`/`open_guide`
se comportam como antes, e isso foi conferido no binário. Os gates 1–8 passam. O CI do `HEAD` está verde, com 162
testes do studio no Windows.

Antes de fechar a fase, para não abrir outra rodada pelo mesmo motivo:
1. **W1:** provar na UI que nenhuma página abre sem o clique. É a forma em JS do achado do crítico, e hoje passa nas
   linhas 3 e 5.
2. **W2:** fechar `app.windows` em `tauri.conf.json` na linha 3, e levar a cerca de HTTP ao grafo resolvido.
3. **W3:** estender o grep da linha 1 a `process::`/`fs::`/`thread::`/`Command`.

No PR, fazer os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e visual.

## DoD Critic (enhanced)

- DoD row «1 | Core … sem KLIPY/HTTP»: oca e objetiva — `std::{process::Command}` + `Command::new("curl")` no core passa
  (a linha só recusa sockets).
- DoD row «3 | Studio e disco … nada sai … no início»: oca e objetiva — uma segunda janela em `tauri.conf.json` com
  `url` do KLIPY e o `app.js` abrindo o Painel ao iniciar sem chave passam (a guarda só lê Rust; o e2e só confere o
  link depois do clique). O updater no `setup` não foi demonstrado.
- DoD row «10 | No TODO/FIXME …» (PROJECT): oca e objetiva — `test.fixme('…')` sem issue passa, porque o `sed` apaga
  o token antes da busca.
- Demais linhas: provam o critério; as mutações da iteração anterior falham.

**Verdict:** BLOCKED
