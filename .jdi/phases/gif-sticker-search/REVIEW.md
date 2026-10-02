# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 3, iteração 4 (14ª no total; teto absoluto 15) do loop autônomo (`/jdi-issue`).
> Branch `jdi/gif-sticker-search`, `HEAD` = `edb9196`, igual ao remoto do início ao fim (`git fetch` antes da linha 7).
> Escopo: `git diff origin/main...HEAD` inteiro (112 arquivos, 76 commits), com atenção a:
> - `75f6f79`: o valor de teste com o esquema `ws:` passa a ser montado por `concat!` (Semgrep);
> - `101d53c`: `studio-starts-silent.sh` + `silent-shim.c` com 4 inícios (com/sem chave × oculto/visível), cada um num
>   `kwin_wayland --virtual` e num barramento próprios, prova de que a janela rodou e lista fechada de sockets (D-18);
> - `8f437e1`: `tests/ui/window-boundary.test.mjs` (quem pode chamar o backend, o KLIPY e o Painel) e o e2e que esconde
>   e mostra a janela 3 vezes;
> - `b2515a8`/`edb9196`: D-18, linha de TODO do PROJECT (exclui por caminho os 2 arquivos do verificador), SUMMARY.
>
> Li D-1..D-18, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 3 iter 3 (revisor W1–W5; crítico BLOCKED:
> linha 3 com início visível/sem chave e linha 10 com outras grafias). `npx -y jdi-cli render` rodou primeiro e não
> mudou arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace` antes
>   da cobertura, Playwright com `BEZEL_E2E_PORT=1442`). Os `Verify:` foram extraídos do Markdown por script, sem
>   edição, e rodados com `bash`.
> - **Studio do usuário:** PID **2070350** (`~/.local/bin/bezel-studio`) antes, durante (anotado em cada mutante) e
>   depois: inalterado. Nenhum processo de teste ficou vivo (`pgrep` e varredura de `SILENT_LOG=` em
>   `/proc/*/environ`: nada). As pastas `SILENT_KEEP` foram apagadas.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, os servidores do KLIPY nem o arquivo da chave. O script sempre roda
>   com `SILENT_BLOCK=1`. Nenhum mutante do script pede GIF (só abre o Painel, recusado pelo shim, ou usa o barramento
>   do usuário e `bezel-silent-probe.invalid`, ambos recusados).
> - **Mutações:** cópias por `git archive` de `edb9196` em `scratchpad/rev14/mut/<m>`, cada uma com `git init` próprio
>   (commit sobre `origin/main` = `e527240`) e `CARGO_TARGET_DIR` próprio (`target/rev14mut/<m>`, reflink de
>   `target/review`, apagado no fim). O repositório não foi alterado (`git status`: só o `LOOP.md` não versionado).

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **945 passed, 0 failed, 12 ignored** (hardware, ffmpeg real, KLIPY real), 39 binários; studio lib **165** no Linux. Igual ao SUMMARY e à iter anterior (as mudanças da iter são script, teste JS e um valor de teste Rust) |
| Coverage | PASS | **94.91%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do PROJECT: **94.86%**. Studio: `lib.rs` 95.52%, `diag.rs` 98.84%, `gifs.rs` 96.87%, `gifs/key.rs` 92.13%, `commands.rs` 10.72% (como antes) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy `--target x86_64-pc-windows-msvc` sem studio/klipy: exit 0. No CI, `rust-windows` passou fmt + clippy. Nenhum `allow` novo (os 4 de `rtss.rs` são anteriores) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo; o único Rust mudado na iter está em `mod tests` (`lib.rs:3793-3794`). `cargo audit`: exit 0 (620 crates). Nenhum segredo |
| Consistency | PASS | 76 commits: 75 com escopo `gif-sticker-search`, 1 `chore(jdi)`. `101d53c` e `8f437e1` implementam a D-18 como escrita (4 inícios, kwin próprio, prova de vida, sockets aceitos, regra `invoke`/`__TAURI__` só no `bridge.js`, chamadas de GIF/link só na UI de GIF/coleção, e2e de reexibição); a linha de TODO bate com a D-18 |
| UI Validation | PASS | `npm ci` ok; `npm run test:unit` **233/233** (2 novos de `window-boundary`), 99.94% de linhas; Playwright completo **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`) |
| DoD | PASS | As 10 linhas Auto dão `OK`; a 7 rodou depois do fim do run 37053261319. As 2 Manual são da release |

## Itens da rodada 3, iter 3 (reaplicados)
| Item | Estado | Evidência |
|---|---|---|
| W1: início sem chave abre o Painel por `__TAURI__` direto em `app.js` (`jsdirect`) | **CLEARED** | Linha 3 **exit 1**: o script reprova as execuções 3 e 4 (sem chave, oculta e visível) com `xdg-open`, `gio open`, `gnome-open` e `kde-open https://partner.klipy.com`, todos recusados pelo shim; 1 e 2 (com chave) passam. Linha 5 **exit 1**: `window-boundary` acusa `src/app.js:38 talks to the backend`. A forma que só o script vê (`nokeygif`, abaixo) também reprova |
| W2: página que não roda nada passa (`deadjs`, `throw` na 1ª linha de `app.js`) | **CLEARED** | Script: **4 de 4** execuções reprovam com "the window never ran: the studio never ran the video tools probe". A prova de vida não é vazia: a sonda só sai quando o JS chega a `app.js:921` |
| W3: `test.fixme ('…')` com espaço (`fixmesp`) | **CLEARED** | Linha 10 **exit 1** (controle: `OK`) |
| W4: CI vermelho pelo Semgrep | **PERSISTS (mudou de lugar)** | O achado em código sumiu: o alerta #9 (`lib.rs:3793`) está `fixed` no branch. Mas o run 37053261319 deu `failure` de novo: "SAST - Semgrep: 2 achado(s) em erro ou acima", alertas #10 e #11 da mesma regra (`detect-insecure-websocket`) em Markdown do `.jdi`. Ver W2 abaixo |
| W5: SUMMARY sem os scripts em "Files modified" | **CLEARED** | `scripts/ci/{check-docs.sh,studio-starts-silent.sh,silent-shim.c}` listados. Ver W3 (números) |
| Crítico: início visível, `if (!document.hidden) bridge.openLink('klipyPartnerPanel')` em `app.js` (`visible`) | **CLEARED** | Linha 3 **exit 1**: execuções 2 e 4 (visíveis) reprovam com as 4 tentativas de abrir o navegador; 1 e 3 (ocultas) passam. Logo a página vê `document.hidden` diferente nos dois modos, e o modo visível é real. Linha 5 **exit 1** (`src/app.js:26 asks KLIPY or opens a link`). A mesma linha em `ui/gif-search.js`, só no Tauri (`visgif`), passa a linha 5 e reprova a 3 (execuções 2 e 4) |
| Crítico: `test['fixme']` (`fixmebr`) | **CLEARED** | Linha 10 **exit 1** |

## Mutações
| Mutante | O que muda | Linha(s) | Resultado | Quem pega |
|---|---|---|---|---|
| `ctl` | nada | 3, 5, 10 | `OK` / `OK` / `OK` | controle (script: 4/4 OK) |
| `jsdirect` | `app.js:38`: `__TAURI__` pede `klipy_key` e, sem chave, `open_link` | 3, 5 | exit 1 / exit 1 | script (execuções 3, 4) e `window-boundary` |
| `visible` | `app.js:26`: `if (!document.hidden) bridge.openLink(…)` | 3, 5 | exit 1 / exit 1 | script (2, 4) e `window-boundary` |
| `visgif` | depois de `ui/gif-search.js:36`: o mesmo, só com `bridge.mode === 'tauri'` | 3, 5 | exit 1 / `OK` | só o script (2, 4) |
| `nokeygif` | depois de `ui/gif-search.js:36`: no Tauri, `klipyKey()` e, sem chave, `openLink` | script, 5 | exit 1 / `OK` | só o script (3, 4) |
| `deadjs` | `throw` na 1ª linha de `app.js` | script | exit 1 | prova de vida (4/4) |
| `reshow` | depois de `ui/gif-search.js:36`: `searchGifs` no `visibilitychange` visível | 5 | exit 1 | e2e `explicit off by default` × 4 |
| `reshowtauri` | o mesmo com `bridge.mode === 'tauri'`, abrindo o Painel | script, 5 | **`OK` / `OK`** | ninguém (W1) |
| `inline` | `index.html`: `<script type="module">` que chama `__TAURI__…invoke('open_link', …)` | script, unit | exit 1 / `window-boundary` **passa** | só o script; o Tauri roda o script inline (W1) |
| `fixmesp` | `test.fixme ('…')` | 10 | exit 1 | grep do PROJECT |
| `fixmebr` | `test['fixme']('…')` | 10 | exit 1 | grep do PROJECT |
| `userbus` | `setup`: `UnixStream::connect("/run/user/1000/bus")` | script | exit 1 | "the user's own session bus" (recusado com `EACCES`) |
| `dns` | `setup`: resolve `bezel-silent-probe.invalid` pela glibc | script | exit 1 | connect a `/run/systemd/resolve/io.systemd.Resolve` recusado; o stub na porta 53 nem é tocado |

## O script de partida silenciosa (D-18): solidez
- **Os 4 inícios são reais e diferentes.** Os mutantes `visible`/`visgif` reprovam só nas execuções visíveis e
  `jsdirect`/`nokeygif` só nas sem chave. Logo o eixo oculto/visível e o eixo com/sem chave mudam o que a página vê.
  Nada aparece na área de trabalho do usuário: o compositor é privado, `DISPLAY`, `WAYLAND_DISPLAY` e o barramento do
  usuário não são repassados.
- **A prova de vida funciona.** Ela exige o `exec` de `ffmpeg -hide_banner -version` pelo próprio studio, pedido pela
  janela em `app.js:921`. O `deadjs` reprova 4/4.
- **Sockets mais estreitos que na iter 3.** O shim agora recusa todo socket Unix fora de `SILENT_SOCKETS`
  (compositor, barramento de teste, barramento do sistema, `userdb`) e todo loopback fora da porta 53. O `userbus`
  prova a recusa do barramento do usuário. O `dns` mostra que uma resolução pela glibc reprova antes de chegar ao stub
  (via `io.systemd.Resolve`).
- **Ainda fora do alcance (observações):**
  - um resolvedor próprio falando direto com `127.0.0.53:53` passaria;
  - o barramento do sistema é aceito (por ele, `resolved` ou o NetworkManager são alcançáveis);
  - o KWin não é registrado, porque tem capacidades e a glibc não pré-carrega nada nele;
  - nada depois de 12 s é visto;
  - syscalls diretas (D-10) não passam pelo shim;
  - o script não roda no CI (precisa de KWin), só na linha 3 local.

  Todas essas são evasões deliberadas ou limites declarados na D-17/D-18.
- **Não exercitado: a janela oculta que passa a visível.** Nenhuma das 4 execuções esconde ou mostra a janela depois
  do início. Ver W1.

## Blockers
- nenhum

## Warnings
- **W1: reexibir a janela só é provado no modo demo. Um caminho só do Tauri no `visibilitychange` passa as linhas 3 e 5.**
  - **Onde:** mutante `reshowtauri`, uma linha depois de `apps/bezel-studio/src/ui/gif-search.js:36`:
    `document.addEventListener('visibilitychange', () => { if (bridge.mode === 'tauri' && !document.hidden) void bridge.openLink('klipyPartnerPanel'); });`.
    Resultado: script **OK** nas 4 execuções e linha 5 **OK** (24/24 com axe, `window-boundary` passa).
  - **Por que passa:**
    - o e2e de reexibição (`tests/e2e/gif-search.spec.mjs:144`, usado em `:286`) roda no bridge da demo
      (`mode: 'demo'`);
    - `window-boundary.test.mjs:32` permite `openLink`/`searchGifs` em `ui/gif-search.js` e `ui/collection.js`;
    - o script nunca torna visível uma janela oculta. A execução "shown" já nasce visível, sem `visibilitychange`.
  - **Mesma classe, ponto cego da regra estática:** `window-boundary.test.mjs:66-72` só lê `src/**/*.{js,mjs}`, e não o
    `index.html`. O mutante `inline` prova que o Tauri executa um `<script type="module">` inline (CSP com hash na
    build): na partida o script o pega (4/4), mas a regra estática não. Por construção, um handler de
    `visibilitychange` inline com `__TAURI__` passaria `window-boundary`, o e2e (sem `__TAURI__` na demo) e o script.
  - **Gravidade:** média-baixa. Exige um ramo explícito "só no app real" ou um script inline. Mas é a forma
    "pedido no `visibilitychange`" que o crítico citou na iter 3, agora fora do `app.js`.
  - **Correção barata:**
    - nas execuções ocultas, iniciar uma segunda instância no mesmo compositor e barramento. O
      `tauri_plugin_single_instance` mostra a janela (`src-tauri/src/lib.rs:188-190`), e o script segue observando
      alguns segundos;
    - `window-boundary` também lê o `index.html` (ou recusa `<script>` sem `src`).
- **W2: CI do `HEAD` vermelho de novo pelo Semgrep, agora em Markdown do `.jdi` (W4 da iter 3 mudou de lugar).**
  - **O que aconteceu:** no run 37053261319, `Varreduras` falhou em "Relatar - Semgrep" e, com isso, `Portao` e o run
    (`conclusion=failure`).
  - **Os achados:** a regra `javascript.lang.security.detect-insecure-websocket` casa o esquema `ws:` seguido de
    `//` em texto:
    - alerta #11: `.jdi/phases/gif-sticker-search/SUMMARY.md:48` ("Semgrep (`…`) e linha de TODO");
    - alerta #10: `.jdi/phases/gif-sticker-search/REVIEW.md:128`, que é o REVIEW da iter 3 citando o valor de teste.
      Este REVIEW o substitui e não contém essa sequência;
    - o `LOOP.md` (não versionado, linha 40) também contém a sequência e acenderia o alerta se for commitado.
  - **Por que é aviso:** a linha 7 do DoD só exige `rust-windows`, e nenhuma D-XX exige o portão verde. Mas é o
    segundo run seguido com o portão vermelho.
  - **Correção:** reescrever as duas linhas, por exemplo "esquema `ws`". Ou criar um `.semgrepignore` com `.jdi/`
    (o `.jdi/` também já fica fora da linha de TODO).
- **W3 (menor): números desatualizados no SUMMARY.**
  - `SUMMARY.md` diz "UI: 231 unitários". Hoje são **233**.
  - Diz também "DoD 7: … (iter 3: 153 testes do studio no Windows)". Este run tem **162**.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio: nada sai sem chave/no início (comportamento), chave privada, só a última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK` (12 testes + `studio-starts-silent: OK:` com 4/4 execuções; PID do usuário intacto). `ctl`: `OK`. `jsdirect`, `visible`, `visgif`: exit 1. `nokeygif`, `deadjs`, `userbus`, `dns`, `inline`: script exit 1. `reshowtauri`: `OK` (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` (`6 tests × 4 projects, 24/24 runs passed with axe`). `jsdirect`, `visible` (`window-boundary`) e `reshow` (e2e): exit 1. `visgif`, `nokeygif`, `reshowtauri`: `OK` |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37053261319 terminar (`gh run watch`: `completed`). Depois rodei a linha como está escrita, com `HEAD` = remoto = `edb9196`: `OK`. `rust-windows` = `success`: **874 passed, 0 failed, 12 ignored**, studio **162** no Windows, com `nothing_in_the_app_forges_an_invocation`, `the_source_guard_reads_identifiers_not_text` e `the_studio_installs_no_logger` ok. O run como um todo deu `failure` por `Varreduras`/`Portao` (W2) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 945/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.86% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK`. `fixmesp`, `fixmebr`: exit 1. Observação: os 2 arquivos do verificador (`scripts/e2e-passed.mjs` e seu teste) ficam fora por inteiro, como manda a D-18 |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte da release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37053261319, `edb9196`)
- **Passaram:**
  - `rust-windows`: 874/0/12; studio 162;
  - `rust-linux`: 944/0/12; studio 165. O doctest `compile_fail` explica o 945 local;
  - `node-ui`: 233 unitários; 208 Playwright;
  - os três CodeQL (`actions`, `javascript-typescript`, `rust`) e `Versao`.
- **Falharam:** `Varreduras` (Semgrep, 2 achados: W2) e, por causa dele, `Portao`. Gitleaks, TruffleHog, Trivy e
  SBOM passaram dentro de `Varreduras`.
- **Pulados:** `sonar`, `imagem`, `publicar` e `lancar`.
- **Alertas abertos no branch:**
  - os 7 do CodeQL são DES/chaves dos protocolos, anteriores à fase;
  - os 2 do Semgrep são os da W2;
  - o #9 (`lib.rs`) está `fixed`.

## Recommendation
Todos os itens da rodada 3, iter 3 foram reaplicados, e 6 de 7 estão resolvidos:
- o início sem chave por `__TAURI__`;
- a página morta;
- `test.fixme (`;
- o SUMMARY;
- o início visível do crítico;
- `test['fixme']`.

O script agora prova o que diz. Os 4 inícios mudam de fato o que a página vê, a prova de vida reprova uma página
morta, e o barramento do usuário e as resoluções de nome pela glibc são recusados. Os gates 1–8 passam, sem bloqueio.

Antes do PR:
1. **W2:** tirar a sequência do esquema `ws:` do `SUMMARY.md:48` (e do `LOOP.md`, se for commitado), ou ignorar
   `.jdi/` no Semgrep. Sem isso o portão do CI fica vermelho pela segunda vez.
2. **W1:** exercitar a reexibição real (segunda instância nas execuções ocultas) e fazer `window-boundary` ler o
   `index.html`.
3. **W3:** atualizar os números do SUMMARY.

No PR, seguem pendentes os itens de "Deferred to PR review": T-8 na 8.8", os termos do KLIPY e o visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio: nada sai sem chave/no início (comportamento) …»: oca e objetiva — engano realista: abrir o
  Painel do KLIPY ao abrir o diálogo sem chave (`if (!key?.configured) openPartnerPanel();` depois de
  `dialog.showModal()` em `ui/gif-search.js`) passa todas as linhas; o e2e `no key: help` só confere a saída antes do
  primeiro clique. Confirmado o W1 do revisor (reexibir a janela só no demo; `index.html` fora da regra).
- Demais linhas: provam o critério (grafias raras de TODO listadas, não contadas).

**Verdict:** BLOCKED
