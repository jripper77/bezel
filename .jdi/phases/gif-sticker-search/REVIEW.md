# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, rodada 3, iteração 5. É a 15ª iteração no total e a última que o loop permite (`/jdi-issue`).
> Branch `jdi/gif-sticker-search`, `HEAD` = `a3bfd0b`, igual ao remoto do início ao fim (`git fetch` antes da linha 7).
> Escopo: `git diff origin/main...HEAD` inteiro (114 arquivos, 82 commits), com atenção a:
> - `71357b2`: `.semgrepignore`;
> - `2a3924a`: `studio-starts-silent.sh` mostra de novo a janela oculta (segunda execução) e os dois inícios de cada
>   variante de chave usam as mesmas pastas;
> - `bde9027`: `window-boundary.test.mjs` também lê o `index.html`;
> - `e90b6a4`: o e2e exige exatamente as chamadas que cada passo pede;
> - `23ed640`/`a3bfd0b`: D-19 e SUMMARY.
>
> Li D-1..D-19, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o REVIEW da rodada 3 iter 4 (revisor W1 reexibição/`index.html`,
> W2 Semgrep em Markdown, W3 números; crítico BLOCKED: diálogo sem chave abre o Painel). `npx -y jdi-cli render` rodou
> primeiro e não mudou arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace` antes
>   da cobertura, Playwright com `BEZEL_E2E_PORT=1442`). Os `Verify:` foram extraídos do Markdown por script, sem
>   edição, e rodados com `bash`.
> - **Studio do usuário:** PID **2070350** (`~/.local/bin/bezel-studio`) antes, durante (anotado em cada mutante) e
>   depois: inalterado. Nenhum processo de teste ficou vivo (`pgrep` de `kwin_wayland --virtual`/`dbus-run-session` e
>   varredura de `SILENT_LOG=` em `/proc/*/environ`: nada).
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, os servidores do KLIPY nem o arquivo da chave. O script roda sempre
>   com `SILENT_BLOCK=1`, e cada tentativa de abrir o navegador dos mutantes foi recusada pelo shim.
> - **Mutações:** cópias por `git archive` de `a3bfd0b` em `scratchpad/rev15/mut/<m>`, cada uma com `CARGO_TARGET_DIR`
>   próprio (`target/rev15mut/<m>`, reflink de `target/review/debug`). Cópias e alvos foram apagados no fim. O
>   repositório não foi alterado (`git status`: só o `LOOP.md` não versionado).

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **945 passed, 0 failed, 12 ignored** (hardware, ffmpeg real, KLIPY real), 39 binários; studio lib **165** no Linux. Igual ao SUMMARY e à iter anterior: nenhum `.rs`, `.toml` ou `Cargo.lock` mudou desde `edb9196` |
| Coverage | PASS | **94.91%** de linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do PROJECT: **94.86%**. Studio: `lib.rs` 95.52%, `diag.rs` 98.84%, `gifs.rs` 96.87%, `gifs/key.rs` 92.13%, `commands.rs` 10.72% (como antes). `bezel-klipy`: `client.rs` 90.04%, `dto.rs` 99.16% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy `--target x86_64-pc-windows-msvc` sem studio/klipy: exit 0. No CI, `rust-windows` passou fmt + clippy. Nenhum `allow` novo (os 4 de `rtss.rs` são anteriores) |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo (nenhum Rust mudou na iter). `cargo audit`: exit 0 (620 crates). Nenhum segredo |
| Consistency | PASS | 82 commits: 81 com escopo `gif-sticker-search`, 1 `chore(jdi)`. Os commits da iter fazem o que a D-19 diz: (1) `.semgrepignore`, (2) linha de TODO, (3) reexibição e pastas compartilhadas, (4) `index.html` e e2e. Nenhuma D-XX contrariada |
| UI Validation | PASS | `npm ci` ok; `npm run test:unit` **235/235** (2 novos de `window-boundary`), 99.94% de linhas; Playwright completo **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`) |
| DoD | PASS | As 10 linhas Auto dão `OK`. A 7 rodou depois do fim do run 37062387840, que passou por inteiro. As 2 Manual são da release |

## Itens da rodada 3, iter 4 (reaplicados)
| Item | Estado | Evidência |
|---|---|---|
| W1a: reexibir só provado no demo; `visibilitychange` só no Tauri abre o Painel (`reshowtauri`) | **CLEARED** | Script **exit 1**. As execuções 1 e 3 (ocultas, depois da segunda execução que mostra a janela) reprovam com `xdg-open`, `gio open`, `gnome-open` e `kde-open https://partner.klipy.com`, todos recusados pelo shim. As 2 e 4 (já visíveis) passam. A linha 5 segue `OK`, o que é esperado: o demo não tem `__TAURI__` |
| W1b: `<script type="module">` inline no `index.html` (`inline`) | **CLEARED** | `npm run test:unit` reprova (linha 5 **exit 1**). O teste novo "the pages load only the app's own modules" acusa `src/index.html:9 an inline script` e `a script that is not one of the app's modules`. O script também reprova as **4 de 4** execuções |
| W4 (W2 da iter 4): CI vermelho pelo Semgrep em Markdown do `.jdi` | **CLEARED** | Run 37062387840: `Varreduras` `success`. Semgrep OSS 1.177.0: **0 achados**, 376 arquivos varridos, 224 pulados pelo `.semgrepignore`. Alertas #9, #10 e #11 estão `fixed` no branch. `Portao` `success`. Ver "O `.semgrepignore`" abaixo |
| W3: números do SUMMARY | **CLEARED** | O SUMMARY diz 235 unitários (medi 235) e 162 testes do studio no Windows (o CI deu 162) |
| Crítico: o diálogo aberto sem chave abre o Painel (`if (!key?.configured) openPartnerPanel();` depois de `dialog.showModal()`, `nokeydialog`) | **CLEARED** | Linha 5 **exit 1**: `gif search › no key: help` reprova nos 4 projetos em `tests/e2e/gif-search.spec.mjs:215`, porque `link` vem `["klipyPartnerPanel"]` e o esperado é `[]`. A variante só do Tauri passa (ver W1) |

## Mutações
| Mutante | O que muda | Linha(s) | Resultado | Quem pega |
|---|---|---|---|---|
| `ctl` | nada | 5 (+ 3 no `HEAD`) | `OK` | controle |
| `reshowtauri` | depois de `ui/gif-search.js:36`: no `visibilitychange` visível, só com `bridge.mode === 'tauri'`, abre o Painel | script, 5 | exit 1 / `OK` | script (execuções 1 e 3, depois da reexibição) |
| `inline` | `index.html:9`: módulo inline que chama `__TAURI__…invoke('open_link', …)` | script, 5 | exit 1 / exit 1 | `window-boundary` e o script (4/4) |
| `nokeydialog` | depois de `ui/gif-search.js:460` (`dialog.showModal()`): sem chave, abre o Painel | 5 | exit 1 | e2e `no key: help` × 4 (`:215`) |
| `nokeydialogtauri` | o mesmo, só com `bridge.mode === 'tauri'` | 5 | **`OK`** | ninguém (W1). O script nunca abre o diálogo |
| `opentrending` | depois de `:460`: com chave, pede "em alta" ao abrir | 5 | exit 1 | 20 de 24 execuções (todos menos `no key: help`), pelas chamadas exatas |
| `noshow` | `lib.rs:189`: a segunda execução não mostra a janela | script | exit 1 | execuções 1 e 3: "the running studio did not map it … within 5s" |
| `earlyshow` | `lib.rs:376`: o início oculto mostra a janela | script | exit 1 | execuções 1 e 3: "the hidden start showed its window before anything asked for it" |
| `persistls` | só no Tauri: o 1º início grava em `localStorage` e o seguinte, ao vê-lo, abre o Painel | script | exit 1 | execuções 2 e 4: o armazenamento do web view passa de um início ao outro |
| `persistcfg` | só no Tauri: o 1º início troca o idioma salvo e o seguinte, ao vê-lo, abre o Painel | script | exit 1 | execuções 2 e 4: o `settings.json` passa de um início ao outro |
| grafias de TODO | linha `// <grafia>: finish this` em `src/app.js` (cópia com `git init`) | 10 | ver Observações | `TODO_later`, `FIXME2`, `FiXmE`, `ToDos`, `ToDo`, `toDo`, `tOdO`, `TODO(#12) and TODO`: exit 1. `todos`: `OK` (permitido pela D-19) |

## O script de partida silenciosa (D-19): solidez
- **A reexibição é real.** Nas execuções ocultas, depois da prova de que a janela rodou, o script inicia o studio de
  novo com o ambiente do processo vivo (sem o `WEBKIT_DISABLE_DMABUF_RENDERER`). Essa segunda execução sai com 0 em
  ~0,2 s, e a janela é mapeada (`xdg_toplevel` visto pelo `WAYLAND_DEBUG=client`).
  - O `noshow` prova que o mapeamento é exigido.
  - O `earlyshow` prova que uma janela oculta mapeada antes da hora reprova.
  - O `reshowtauri` prova que o que a página faz ao ficar visível é observado: depois da reexibição, o script ainda
    observa por pelo menos metade da janela de 12 s.
- **O estado é de fato compartilhado.** A execução visível começa com o que a oculta deixou (26–29 arquivos). O
  `persistls` e o `persistcfg` provam que o armazenamento do web view e o `settings.json` passam de um início ao outro.
- **No `HEAD`:** 4/4 `OK`, com as execuções 1 e 3 "hidden, then shown again at 1–2s by a second launch (exit 0 in
  0.2s, then mapped)".
- **Limites que continuam (observações, declarados na D-17/D-18):**
  - um resolvedor próprio falando direto com `127.0.0.53:53` passaria;
  - o barramento do sistema é aceito;
  - nada depois de 12 s é visto;
  - syscalls diretas (D-10) não passam pelo shim;
  - o script não roda no CI;
  - o script não abre o diálogo de busca, então só os e2e do demo veem o que a UI de GIF faz depois de um clique.

## O `.semgrepignore` (pedido do orquestrador)
- **Mantém as exclusões padrão e só acrescenta `.jdi/`:** sim, no efeito.
  - Um `.semgrepignore` na raiz substitui o embutido do Semgrep (`src/targeting/Semgrepignore.ml`:
    `use_default_semgrepignore = … not root_semgrepignore_exists`). Por isso, repetir as exclusões padrão era
    necessário.
  - O arquivo repete o modelo clássico (`.gitignore`, `.git/`, `:include .gitignore`, `node_modules/`, `build/`,
    `dist/`, `vendor/`, `.env/`, `.venv/`, `.tox/`, `*.min.js`, `.npm/`, `.yarn/`, `test/`, `tests/`, `*_test.go`,
    `.semgrep`, `.semgrep_logs/`) e acrescenta `.jdi/`.
  - O embutido atual (`default.semgrepignore` no `develop`) tem também `.svn`, `.hg`, `_darcs`, `CVS`, `_opam/`,
    `_build/`, `_cargo/` e `testsuite/`, e não tem `.gitignore`, `:include .gitignore` nem `.semgrep*`. Neste
    repositório nenhuma dessas diferenças pega arquivo versionado (`git ls-files` não tem nenhum desses caminhos, e
    `git ls-files -ci --exclude-standard` é vazio). A única diferença real são os 3 arquivos `.gitignore`, que não
    são código.
- **Conta exata:** os 224 pulados no CI = 168 de `.jdi/` + 53 sob `tests/` (o padrão do Semgrep também os pula) +
  3 `.gitignore`.
- **Continua varrido:** código e scripts (rust 185, js 43, bash 9, c 1, html 1, yaml, json), além de `docs/` pelas
  regras multilinguagem (376 arquivos).

## Blockers
- nenhum

## Warnings
- **W1 (baixa): um ramo "só no app real" na UI de GIF passa todas as linhas.**
  - **Onde:** o mutante `nokeydialogtauri`, uma linha depois de `apps/bezel-studio/src/ui/gif-search.js:460`:
    `if (bridge.mode === 'tauri' && !key?.configured) openPartnerPanel();`.
  - **Resultado:** a linha 5 dá `OK` (24/24 com axe, `window-boundary` passa) e o script não vê, porque nunca abre o
    diálogo. É o mutante do crítico da iter 4, mais o ramo de modo.
  - **Gravidade:** baixa. Exige um ramo explícito para enganar o demo, o que a D-10 deixa para a revisão de código.
    Mas é a mesma classe do W1 da iter 4 (`reshowtauri`), agora depois de um clique.
  - **Correção barata:** `window-boundary.test.mjs` recusar `bridge.mode` (ou `'tauri'`) fora de `bridge.js` e
    `app.js`. Hoje nenhum módulo de UI lê `bridge.mode` (`grep`: só `press.mode` em `ui/canvas.js` e `view.failed.mode`
    em `gif-search.js`, que são outra coisa). Assim, o e2e do demo vale para o app real.

## Observações (não contam)
- **Linha de TODO:** cumpre a D-19 (todas as grafias que ela nomeia reprovam). Ainda passam `todo`/`Todo` colados a
  `_`, dígito ou camelCase (`Todo_later`, `todo_later`, `Todo2`, `TodoLater`), porque o TODO em caixa mista só conta
  como palavra inteira, para deixar passar o "todos" do português. São grafias raras.
- **`.semgrepignore`:** o comentário "Semgrep's default ignores (kept as they are)" descreve o modelo clássico, não o
  embutido do Semgrep 1.177. Neste repositório o efeito é o mesmo (ver acima).

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` |
| 3 | Studio: nada sai sem chave/no início (comportamento), chave privada, só a última busca, alpha, fundo animado, excluir nomeia temas; CSP igual | CONTEXT | Auto | PASS | `OK`: 12 testes + `studio-starts-silent: OK:`, com 4/4 execuções, 1 e 3 reexibidas e 2 e 4 nas pastas da anterior. PID do usuário intacto. Script exit 1 em `reshowtauri`, `inline`, `noshow`, `earlyshow`, `persistls` e `persistcfg`. `nokeydialogtauri` fica fora (W1) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: lógica (debounce, setas, carregar mais, 429), i18n e os 6 testes nomeados nos 4 projetos com axe | CONTEXT | Auto | PASS | `OK` (`6 tests × 4 projects, 24/24 runs passed with axe`). `ctl`: `OK`. `nokeydialog`, `opentrending` (e2e) e `inline` (`window-boundary`): exit 1. `reshowtauri` e `nokeydialogtauri`: `OK` |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 37062387840 terminar (`gh run watch --exit-status`: exit 0). Depois rodei a linha como está escrita, com `HEAD` = remoto = `a3bfd0b`: `OK`. `rust-windows` `success`: **874 passed, 0 failed, 12 ignored**, studio **162** no Windows, com `nothing_in_the_app_forges_an_invocation`, `the_source_guard_reads_identifiers_not_text` e `the_studio_installs_no_logger` ok. O run inteiro deu `success` (ver CI abaixo) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 945/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.86% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK`. As grafias da D-19 reprovam (ver Mutações). Restos em Observações |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY. Evidência sugerida: `## [x.y.z] - <data>` no corte da release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | O README cita a busca e o guia. Evidência sugerida: diff do README revisado no PR |

## CI (run 37062387840, `a3bfd0b`): `conclusion=success`
- **Passaram:**
  - `rust-windows`: 874/0/12; studio 162;
  - `rust-linux`: 944/0/12; studio 165. O doctest `compile_fail` explica o 945 local;
  - `node-ui`: 235 unitários; 208 Playwright;
  - `Varreduras`: Semgrep com 0 achados, Gitleaks, TruffleHog, Trivy e SBOM;
  - os três CodeQL (`actions`, `javascript-typescript`, `rust`), `Versao` e **`Portao`**.
- **Pulados:** `sonar`, `imagem`, `publicar` e `lancar`, como nos runs anteriores.
- **Alertas abertos no branch:** só os 7 do CodeQL (DES/chaves dos protocolos em `bezel-devices`, anteriores à fase).
  Os do Semgrep (#9, #10, #11) estão `fixed`.

## Recommendation
Os 5 itens da rodada 3, iter 4 estão resolvidos, reaplicados um a um:
- a reexibição só no Tauri;
- o script inline no `index.html`;
- o Semgrep;
- os números do SUMMARY;
- o diálogo sem chave do crítico.

As provas novas também não são ocas. A reexibição exige mapear a janela e reprova se ela aparecer antes da hora. O
estado (web view e configuração) passa de um início ao outro. O e2e pega um pedido extra ao abrir o diálogo. Os gates
1–8 passam, e o CI do `HEAD` ficou verde em todos os jobs, com o portão incluído.

Antes do PR, opcional (W1): fazer o `window-boundary` recusar `bridge.mode` fora de `bridge.js`/`app.js`, para que o
e2e do demo valha também para o app real.

No PR seguem pendentes os itens de "Deferred to PR review": T-8 na 8.8", os termos do KLIPY e o visual.

## DoD Critic (enhanced)

- DoD row «3 | Studio: nada sai sem chave/no início (comportamento) …»: oca e objetiva, por drible deliberado (código
  que só roda no app real): `if (bridge.mode === 'tauri' && !key?.configured) openPartnerPanel();` em
  `ui/gif-search.js` ou um `open_link` no `klipyKey` do adaptador Tauri do `bridge.js`; o script de início nunca abre o
  diálogo e o e2e só vê o bridge de demonstração. A mutação da iteração anterior (sem ramo exclusivo do Tauri) falha.
- DoD row «5 | UI …»: oca e objetiva, pelos mesmos dois dribles deliberados (o teste do mapeamento do bridge usa um
  `invoke` falso que sempre devolve `null`).
- Demais linhas: provam o critério.

**Verdict:** BLOCKED
