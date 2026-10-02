# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 3, iteração 3 (13ª no total; teto absoluto 15) do loop autônomo (`/jdi-issue`).
> Branch `jdi/gif-sticker-search`, `HEAD` = `d1e6d6f`, igual ao remoto do início ao fim (conferido com `git fetch` antes
> da linha 7). Escopo: `git diff origin/main...HEAD` inteiro (110 arquivos, 71 commits), com atenção a:
> - `70b926b`: a guarda passa a ler o `tauri.conf.json` (janelas só locais, sem `devUrl`, sem overlay) e o grafo do
>   `cargo metadata` (nenhuma crate de rede além do `ureq` do `bezel-klipy`), e recusa a API do updater;
> - `3f71b78`: `scripts/ci/studio-starts-silent.sh` + `scripts/ci/silent-shim.c` (D-16/D-17);
> - `944cfc3`: e2e "nada sem clique, nem após 1 h ociosa" (`recordOutside`/`startIdle`);
> - `d1e6d6f`: CONTEXT (linha 3 roda o script), D-17, SUMMARY.
>
> Li D-1..D-17, PROJECT (linha de TODO sem isenção de `test.fixme(`), CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada
> 3 iter 2 (crítico: BLOCKED nas linhas 1, 3 e 10). `npx -y jdi-cli render` rodou primeiro e não mudou arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace` antes
>   da cobertura). Os `Verify:` foram extraídos do Markdown por script, sem edição, e rodados com `bash`.
> - **Studio do usuário:** PID **2070350** (`~/.local/bin/bezel-studio`) antes, durante e depois — inalterado.
>   Nenhuma instância de teste ficou viva (`pgrep` e varredura de `SILENT_LOG` em `/proc/*/environ`: nada); as pastas
>   `SILENT_KEEP` que criei para inspeção foram apagadas.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, os servidores do KLIPY nem o arquivo da chave. Os mutantes que
>   chegam à rede usam `192.0.2.1` (TEST-NET-1, sem DNS) e o script sempre roda com `SILENT_BLOCK=1`; os mutantes com
>   URL do KLIPY só rodaram o trecho da linha que para antes do script (a guarda reprova primeiro).
> - **Mutações:** cópias por `git archive` de `d1e6d6f` em `scratchpad/rev13/mut/<m>`, cada uma com `git init`
>   próprio sobre `origin/main` = `e527240` e `CARGO_TARGET_DIR` próprio (`target/rev13mut/<m>`, reflink de
>   `target/review`, apagado no fim). O repositório não foi alterado (`git status`: só o `LOOP.md` não versionado).

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **945 passed, 0 failed, 12 ignored** (hardware, ffmpeg real, KLIPY real), 39 binários; studio lib **165** no Linux. Igual ao SUMMARY e à iter anterior (os casos novos estão dentro de testes existentes) |
| Coverage | PASS | **94.91%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do PROJECT: **94.86%**. Studio: `lib.rs` 95.52%, `diag.rs` 98.84%, `gifs.rs` 96.87%, `gifs/key.rs` 92.13%, `commands.rs` 10.72% (como antes) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy `--target x86_64-pc-windows-msvc` sem studio/klipy: exit 0. No CI, `rust-windows` passou fmt + clippy. Nenhum `allow` novo (os 4 de `rtss.rs` são anteriores, com `reason =`). `silent-shim.c` compila com `-Wall -Wextra -Werror` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo; todo o código Rust novo da iteração está em `mod tests` de `lib.rs` (5.6 limpo). `cargo audit`: exit 0 (620 crates). Nenhum segredo |
| Consistency | PASS | 71 commits: 70 com escopo `gif-sticker-search`, 1 `chore(jdi)`. `70b926b`/`3f71b78`/`944cfc3` implementam a D-16 como escrita; a D-17 descreve exatamente o que o script aceita. Ver W4 (SUMMARY) |
| UI Validation | PASS | `npm ci` ok; `npm run test:unit` **231/231**, 99.94% de linhas; Playwright completo **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`) |
| DoD | PASS | As 10 linhas Auto dão `OK`; a 7 rodou depois do fim do run 37042812434. As 2 Manual são da release |

## Itens da rodada 3, iter 2 (reaplicados)
| Item | Estado | Evidência |
|---|---|---|
| Crítico/W3: core roda `curl` por `use std::{process::Command}` | **CLEARED** | Mutante `curl1` (`fetch_trending` em `domain/gifs.rs`) compila e os testes passam (`ok. 2 passed`), mas a linha 1 dá **exit 1**: o grep acha `gifs.rs:719 process::Command` e `:720 Command::new`. Controle `ctl`: `OK` |
| Crítico/W2: segunda janela no `tauri.conf.json` com `"url": "https://partner.klipy.com"` | **CLEARED** | Mutante `cfgwin`: `nothing_in_the_app_forges_an_invocation` falha com "the window \"welcome\" loads \"https://partner.klipy.com\", not one of the app's own pages"; linha 3 **exit 1** antes do script. Além disso, o script sozinho pega a mesma forma (mutante `remote`, janela oculta em `https://192.0.2.1/`): `FAIL: connection: connect by WebKitNetworkProcess … to inet 192.0.2.1 port 443` |
| Crítico/W1: `app.js` abre o Painel na partida sem chave (pelo bridge) | **CLEARED** | Mutante `js2` (1 linha após `app.js:37`): linha 5 **exit 1** — `gif search › no key: help` falha nos 4 projetos (`startIdle` vê `data-demo-link`). Ver W1 para a forma que ainda passa |
| Crítico/W2: updater no `setup` | **CLEARED** | Mutante `updater`: `tauri-plugin-updater = "2"` no manifesto (lock atualizado só com pacotes novos), `.plugin(tauri_plugin_updater::Builder::new().build())` e `handle.updater()?.check()` no `setup`. Compila; a guarda dá 12+ achados (manifesto, grafo resolvido com `hyper` via `hyper-rustls`/`hyper-util`, `UpdaterExt`, `updater`); linha 3 **exit 1** antes do script |
| Crítico: `test.fixme('…')` sem issue | **CLEARED** | Mutante `fixme` (`test.fixme('parked without an issue', …)` em `gif-search.spec.mjs`): linha 10 **exit 1**. Controle: `OK`. Ver W3 para a grafia com espaço |

## Mutações
| Mutante | O que muda | Linha(s) | Resultado | Quem pega |
|---|---|---|---|---|
| `ctl` | nada | 1, 3, 10 | `OK` / `OK` / `OK` | controle |
| `curl1` | core: `use std::{process::Command}` + `Command::new("curl")` | 1 | exit 1 | grep `process::`/`Command` |
| `cfgwin` | janela `welcome` em `https://partner.klipy.com` | 3 | exit 1 | guarda (`config_problems`) |
| `remote` | janela oculta em `https://192.0.2.1/` (script sozinho) | script | exit 1 | shim: connect do `WebKitNetworkProcess` |
| `updater` | plugin updater + `check()` no `setup` | 3 | exit 1 | guarda (manifesto, grafo, API) |
| `js2` | `app.js` abre o Painel sem chave via `bridge.openLink` | 5 | exit 1 | e2e `no key: help` × 4 |
| `fixme` | `test.fixme('…')` | 10 | exit 1 | grep do PROJECT |
| `jsdirect` | `app.js:37+`: `globalThis.__TAURI__?.core.invoke('klipy_key').then((k) => k.configured \|\| globalThis.__TAURI__.core.invoke('open_link', { link: 'klipyPartnerPanel' }))` | 3 e 5 | **`OK` / `OK`** | ninguém (W1). O mesmo script com a única troca "não gravar a chave" reprova: `xdg-open`, `gio open`, `gnome-open`, `kde-open https://partner.klipy.com` (todos recusados pelo shim) |
| `fixmesp` | `test.fixme ('…')` (espaço antes do parêntese) | 10 | **`OK`** | ninguém (W3) |
| `deadjs` | `throw` na 1ª linha de `app.js` (a página não roda nada) | script | **`OK`** | só a linha 5 (W2) |

## O script de partida silenciosa (D-16/D-17): solidez
- **Vê o que diz ver, na árvore real.** No `HEAD`, com `SILENT_KEEP=1`, o log tem `load` de todo processo da árvore
  (`dbus-run-session`, `dbus-daemon`, o studio duas vezes pelo re-exec DMA-BUF, `WebKitWebProcess`,
  `WebKitNetworkProcess`, `ffmpeg`×2, `ffprobe`) e o `exec` de cada um pelo pai. O `WebKitNetworkProcess` carrega o
  shim, então o tráfego do web view é visto (provado pelo mutante `remote`). Execs por `execvp`/`posix_spawn[p]` do
  Rust e do GLib passam pelos ganchos; um programa iniciado por outro caminho é pego pelo `load` (construtor).
- **Exceções da D-17 — estreitas na prática:**
  1. `ffmpeg`/`ffprobe`: o veredito exige os argumentos exatos. O shim deixa qualquer `ffmpeg` rodar (lista por nome),
     mas o `connect` dele também passa pelo shim e é recusado.
  2. glycin: só é aceito como tentativa **recusada** (`bwrap` não está em `SILENT_ALLOW`, então nunca roda). O casamento
     do argv é frouxo (o caminho do loader pode estar em qualquer argumento), mas isso não importa, porque é recusado.
  3. Portal: é um ponto cego declarado, não uma exceção.
- **Mais largo que "exactly three … nothing else" (D-17), mas dentro da D-16:**
  - todo socket Unix é aceito. Na árvore real, o studio e os dois processos do WebKit já conectam ao barramento do
    sistema (`/run/dbus/system_bus_socket`);
  - `XDG_RUNTIME_DIR` é repassado, então o barramento de sessão real do usuário (`$XDG_RUNTIME_DIR/bus`, com o portal
    `OpenURI`) fica a um `connect` de distância e passaria como `local`;
  - o loopback inteiro é aceito (um proxy local passaria);
  - consultas DNS não aparecem: o resolvedor da glibc usa símbolos internos, e o `systemd-resolved` atende por socket
    Unix ou por `127.0.0.53`.
  - **Observações:** syscalls diretas (rustix `linux_raw`, io_uring) e `bind`/`listen` não passam pelo shim. São
    evasões deliberadas (D-10) e ficam como observação.
- **Pode passar no vazio:**
  - se o studio morre cedo, não: saída dentro da janela, `panicked`, studio que não iniciou e `WebKitWebProcess`
    ausente reprovam. Um SKIP também reprova, porque não imprime `OK:`;
  - a partida **sem chave** nunca é exercitada (W1);
  - uma página que não roda nada passa (W2);
  - o que acontece depois de 12 s não é visto. No JS, o e2e cobre 1 h, mas só pelo bridge;
  - o script não roda no CI (sem display). A prova de comportamento fica só na linha 3, local.

## Blockers
- nenhum

## Warnings
- **W1 — Partida sem chave: o JS abre o Painel do KLIPY por `__TAURI__` direto, e as linhas 3 e 5 passam.**
  - **Onde:** mutante `jsdirect`, uma linha depois de `apps/bezel-studio/src/app.js:37`.
  - **Por que passa:**
    - `scripts/ci/studio-starts-silent.sh:154-158` sempre grava uma chave antes da partida, então o caminho sem chave
      nunca roda;
    - o e2e (`gif-search.spec.mjs:80-130`) só vê o que passa pelos ganchos da demo do bridge, e na demo `__TAURI__` não
      existe;
    - a guarda só lê Rust, e `open_link` é IPC legítimo;
    - `withGlobalTauri: true` (`tauri.conf.json:10`) expõe `invoke` a qualquer módulo.
  - **Por que importa:** é o cenário do crítico da rodada 3 iter 1 (boas-vindas sem chave abre o navegador), agora em
    JS. Toda partida de uma instalação nova abriria `partner.klipy.com`. Provei o efeito: o mesmo script, só sem gravar
    a chave, reprova com 4 tentativas de abrir o navegador, todas recusadas pelo shim.
  - **Correção barata:** rodar o script duas vezes, sem chave e com chave; e/ou um teste de unidade que só aceite
    `__TAURI__` em `src/bridge.js`.
- **W2 — O script aceita uma página que não rodou nada.**
  - **Onde:** `studio-starts-silent.sh:273-275` toma o início do `WebKitWebProcess` como "a página rodou". Mutante
    `deadjs`: `OK`, sem a sonda do ffmpeg no log.
  - **Por que importa:** numa máquina lenta, ou com a página quebrada, o JS da janela não é observado. Hoje isso é
    compensado pela linha 5.
  - **Correção:** exigir a sonda `media_tools` (o `exec` do `ffmpeg -hide_banner -version` é registrado mesmo sem
    ffmpeg instalado) ou outro marcador de que a página chegou ao backend.
- **W3 — `test.fixme ('…')`, com espaço, passa a linha 10.**
  - **Onde:** `PROJECT.md:59`. O `sed` `s/test\.fixme\b([^(]|$)/\1/g` apaga o token quando o próximo caractere é um
    espaço. Mutante `fixmesp`: `OK`.
  - **Gravidade:** baixa. É grafia incomum e o repositório não tem formatador de JS.
  - **Correção:** no `sed`, usar `test\.fixme\b\s*([^(\s]|$)`, ou procurar `test\.fixme\s*\(`.
- **W4 — CI do `HEAD` vermelho fora do Windows: o Semgrep reprova um dado de teste.**
  - **O que aconteceu:** no run 37042812434, `Varreduras` falhou em "Relatar - Semgrep" ("1 achado(s) em erro ou
    acima"), e com isso falharam `Portao` e o run (`conclusion=failure`). O run anterior (`ef26a83`) passou.
  - **O achado:** `javascript.lang.security.detect-insecure-websocket` em
    `apps/bezel-studio/src-tauri/src/lib.rs:3793`, a string `"ws://partner.klipy.com"`. É dado de
    `remote_pages_are_refused` (`mod tests`), vindo de `70b926b`. É falso positivo, mas deixa o PR com o portão
    vermelho.
  - **Por que é aviso e não bloqueio:** a linha 7 do DoD só exige `rust-windows` verde. Mesmo assim, corrigir antes do
    PR: montar a URL em tempo de execução (`format!("{}://…", "ws")`) ou `// nosemgrep: …` com motivo.
- **W5 (menor) — SUMMARY desatualizado.** "Files modified" não lista `scripts/ci/studio-starts-silent.sh` nem
  `scripts/ci/silent-shim.c`.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP (sem I/O) | CONTEXT | Auto | PASS | `OK` no `HEAD`. `ctl`: `OK`; `curl1`: exit 1 |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio: nada sai sem chave/no início (comportamento), chave privada, só a última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK` (12 testes + `studio-starts-silent: OK:`; PID do usuário intacto). `ctl`: `OK`; `cfgwin`, `updater`: exit 1; `jsdirect`: `OK` (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` (`6 tests × 4 projects, 24/24 runs passed with axe`). `js2`: exit 1; `jsdirect`: `OK` (W1) |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37042812434 terminar (`status=completed`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `d1e6d6f`: `OK`. `rust-windows` = `success`: **874 passed, 0 failed, 12 ignored**, studio **162** no Windows, com a guarda, `the_source_guard_reads_identifiers_not_text` e `the_studio_installs_no_logger` ok. O run como um todo deu `failure` por `Varreduras`/`Portao` (W4) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 945/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.86% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK`. `fixme`: exit 1; `fixmesp`: `OK` (W3) |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte da release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37042812434, `d1e6d6f`)
- **Passaram:** `rust-windows` (874/0/12; studio 162), `rust-linux` (944/0/12; studio 165; o doctest `compile_fail`
  explica o 945 local), `node-ui`, os três CodeQL e `Versao`.
- **Falharam:** `Varreduras` (Semgrep, W4) e, por causa dele, `Portao`.
- **Pulados:** `sonar`, `imagem`, `publicar` e `lancar`.
- **Alertas abertos:** os 7 do CodeQL são DES/chaves dos protocolos, anteriores à fase. O alerta Semgrep #9 é o da W4.

## Recommendation
Todos os itens da rodada 3, iter 2 estão resolvidos, reaplicados um a um: o `curl` no core, a janela do KLIPY na
config, o `app.js` que abria o Painel pelo bridge, o updater e o `test.fixme(`. Os gates 1–8 passam, e a guarda nova
não dá falso positivo no Linux nem no Windows.

O script de partida silenciosa é sólido para o que observa: árvore inteira, WebKit incluído, e exceções que na prática
só aceitam o que é recusado ou inofensivo. Faltam a partida sem chave e uma prova de que a página rodou.

Antes do PR:
1. **W4:** tirar o falso positivo do Semgrep. Sem isso o portão do CI fica vermelho.
2. **W1:** rodar o script também sem chave (ou proibir `__TAURI__` fora de `bridge.js`). É a forma do achado do
   crítico da iter 1 que ainda passa nas linhas 3 e 5.
3. **W2 e W3:** exigir a sonda `media_tools` no script e fechar o `test.fixme (` no grep.

No PR, seguem pendentes os itens de "Deferred to PR review": T-8 na 8.8", termos do KLIPY e o visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio: nada sai sem chave/no início (comportamento) …»: oca e objetiva — o script só testa o início
  oculto com chave salva: `if (!document.hidden) bridge.openLink('klipyPartnerPanel')` no `app.js` passa (um início
  visível abre o navegador); o revisor mostrou o mesmo sem chave via `__TAURI__ … invoke('open_link')`. Fora do
  "início": um pedido de GIF no `visibilitychange` passa as linhas 3 e 5.
- DoD row «10 | No TODO/FIXME …» (PROJECT): oca e objetiva — `test.fixme ('…')` com espaço e `test['fixme']` passam.
- Demais linhas: provam o critério; as mutações da iteração anterior falham.

**Verdict:** BLOCKED
