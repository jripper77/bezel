# Phase 11: Review  (slug: gif-sticker-search)

**Verdict:** APPROVED_WITH_WARNINGS

> Revisão em modo `verify`, iteração 3 do loop autônomo (`/jdi-issue`). Branch `jdi/gif-sticker-search`, `HEAD` =
> `cc78ea1` (igual ao remoto, inalterado do início ao fim, conferido de novo antes da linha 7). Escopo:
> `git diff origin/main...HEAD` inteiro (84 arquivos), com atenção à iter 3: `8340202` (D-8 + PROJECT), `ced3217`,
> `83946df` e `b7036ee` (os patches de `8b50c09`, `7e0ab4d` e `d2e577a` do worktree `wt/gss-i3`: comparei os
> diffs e são idênticos) e `cc78ea1` (SUMMARY). Também li D-1..D-8, PROJECT, CONTEXT, PLAN, SUMMARY, LOOP e o
> REVIEW da iter 2 (B1, B2, W1, W2). `npx -y jdi-cli render` rodou antes e não mudou nenhum arquivo versionado.
>
> - **Números:** todos das minhas execuções (`CARGO_TARGET_DIR=target/review`, `cargo llvm-cov clean --workspace`
>   antes da cobertura). Nenhum veio do SUMMARY. Os `Verify:` foram extraídos do Markdown por script e rodados
>   com `bash`, como estão escritos.
> - **Rede e hardware:** nada tocou `/dev/ttyACM*`, o app instalado, os servidores do KLIPY nem o arquivo da
>   chave. Playwright com `BEZEL_E2E_PORT=1442`.
> - **Mutações:** feitas num `git clone` de `cc78ea1` no scratchpad, com `origin/main` copiado. O repositório não
>   foi alterado.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **933 passed, 0 failed, 12 ignored (hardware, ffmpeg real, KLIPY real)**, em 39 binários. Igual à iter 2: no Linux o teste do `setup` continua rodando (`tests::the_app_setup_sends_nothing_at_start ... ok`). Studio lib: 154 |
| Coverage | PASS | **94.46%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD do PROJECT: **94.40%**. Studio `lib.rs` 72.50%, `gifs.rs` 96.89%, `bezel-klipy` `client.rs` 90.04% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy local `--target x86_64-pc-windows-msvc` sem `bezel-studio`/`bezel-klipy`: exit 0. No CI, o clippy do Windows (`--all-targets --all-features -D warnings`, studio incluído) passou, então os `use` com `cfg` não deixam nada sem uso no Windows. Nenhum `allow` novo |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 sem achado novo. Os hits de 5.3b, 5.5, 5.9 e 5.10 são anteriores à fase, e a iter 3 não toca nenhum desses arquivos. Os comandos GIF são `async`. `cargo audit`: exit 0 (620 crates). Nenhum segredo |
| Consistency | WARN | 27 commits com escopo `gif-sticker-search` e 1 `chore(jdi)`. `Cargo.lock` não mudou desde a iter 2. D-1..D-7 seguem cumpridas. A parte (2) da D-8 está implementada como foi decidida. A parte (1) está implementada linha a linha, e não token a token: ver W1 |
| UI Validation | PASS | `npm ci` ok. `npm run test:unit`: **225/225**, 99.94% de linhas (+3 testes de `uncovered`). Playwright completo: **208/208** (claro/escuro × pt-BR/en, axe, `watchErrors`) |
| DoD | PASS | As 10 linhas Auto passam, incluindo a 7: o CI do `HEAD` terminou verde e rodei a linha depois disso. As 2 Manual são da release |

## Itens da iter 2
| Item | Estado | Evidência |
|---|---|---|
| B1 — binário de testes do studio não subia no Windows (`0xc0000139`) | **CLEARED** | Run **36955188127** no `cc78ea1`: `conclusion=success`, com `rust-windows`, `rust-linux`, `node-ui`, CodeQL ×3, Varreduras, Versao e `Portao` = success. No log do `rust-windows`, `Running unittests src\lib.rs (…\bezel_studio-32affe4de76179bd.exe)` mostra **`running 153 tests`** e `test result: ok. 153 passed; 0 failed` (34 s). Total no Windows: **865/0/12** em 30 binários. Nenhum binário parou. Comparei com a iter 1 (run 36948045888, `8df7ffa`): 151 testes do studio e 863 no total. Agora há +2, que são os 2 testes novos da iter 2 que rodam no Windows. Comparei os nomes: Linux (154) − Windows (153) = **só** `tests::the_app_setup_sends_nothing_at_start`. Ver a análise da D-8 |
| B2 — linha TODO/FIXME do PROJECT falhava no `HEAD` | **CLEARED** | `.jdi/PROJECT.md:59` como está escrita: exit 0, `OK`. O `git grep` cru acha 8 linhas, todas com `test.fixme` ou `'fixme'` (`scripts/e2e-passed.mjs:2,6,34,35`, `tests/ui/e2e-passed.test.mjs:4,76,77,80`). Fiz um controle: `// TODO: drop the wait below` no spec de e2e dá exit 1 e a linha aparece. `# todo: remove` num `scripts/ci/*.py` novo também dá exit 1. A exclusão tem um furo: W1 |
| W1 — linha 5 não conferia os 4 projetos | **CLEARED** | No clone, cortei `playwright.config.mjs` para um só projeto (`project('light-pt', …)`). A linha 5 como está escrita deu **exit 1**: `FAIL: 3 problem(s)`: `no project runs in dark × pt-BR`, `light × en`, `dark × en`. Antes do corte ela dava `ok: 6 tests × 1 projects`. Sem a mutação, no repositório: `ok: 6 tests × 4 projects, 24/24 runs passed with axe`, `OK`. Os testes unitários cobrem 1 projeto, só os claros, pt-PT ≠ pt-BR e projeto sem `metadata`; a CLI dá exit 1 num relatório de 1 projeto |
| W2 — DoD LOCKED do PROJECT mudou sem D-XX | **CLEARED** | `D-2026-10-01-gif-sticker-search-8` (`.jdi/decisions/…-8.md`, renderizada em `.jdi/DECISIONS.md:147`) registra o alargamento do pathspec de `e2c9cb2` e a exclusão. Ela entrou no mesmo commit da edição manual de `.jdi/PROJECT.md:59` (`8340202`), como o `PROJECT.md:49` exige. O backlog do manifest do Windows está em `.jdi/todos/2026-10-01-gif-sticker-search.md` |

## As duas partes da D-8
- **(2) Teste do mock fora do Windows: correta e com escopo estreito.**
  - O `setup` (`lib.rs:289`) não tem nenhum `cfg` de plataforma. Os únicos `cfg` são o `DMABUF` do Linux, em
    `run()`. Por isso o que o teste prova vale também no Windows.
  - O `cfg(not(windows))` cobre só o teste (`lib.rs:666`), seus 4 `use` e as constantes `KEY` e `IDLE`.
  - `temp_root`, `Folders::under` e `counting` continuam compilando no Windows, porque o teste da coleção, que
    roda lá, também os usa.
  - Nenhum outro teste usa `tauri::test`.
  - O único outro `cfg` de teste no studio é o bloco `#[cfg(unix)]` dentro de um teste, em `gifs/tests.rs:402`.
    Ele confere o modo 0600, vem da iter 1 e não é um teste à parte.
  - Comparei `cargo tree -p bezel-studio --target x86_64-pc-windows-msvc -e features,normal,dev` no `cc78ea1` e
    no `f3bd567^`: são iguais (1574 linhas, só os caminhos mudam). Também não há `tauri feature "test"` no
    Windows; no Linux ela está lá.
  - O CI confirma que nenhum outro teste do studio sumiu no Windows (B1).
- **(1) Exclusão do `test.fixme`/`'fixme'`: a decisão é razoável, mas está implementada linha a linha.** A D-8
  diz que "every other spelling is still flagged". Só que o `grep -vE "test\.fixme|'fixme'"` descarta a linha
  inteira, então um marcador de verdade passa quando está na mesma linha de um desses tokens. Detalhes em W1.

## Blockers
- nenhum

## Warnings
- **W1 — A linha TODO/FIXME do PROJECT deixa passar marcadores reais** (`.jdi/PROJECT.md:59`, D-8 parte 1). O
  filtro `grep -vE "test\.fixme|'fixme'"` descarta a linha inteira, em vez de tirar só o token. Testei mutações no
  clone com a linha como está escrita. Todas deram `OK`, e o controle (TODO sozinho) deu exit 1:
  - `test.fixme('429 retry', async () => {}); // TODO: un-park once the demo answers` no spec de e2e;
  - `export const PARKED = 'fixme'; // FIXME: also handle 'skip' annotations` em `scripts/e2e-passed.mjs:35`;
  - `// FIXME: flaky, see test.fixme` em `crates/bezel-core/src/lib.rs`;
  - um teste estacionado sem issue: `test.fixme('drag onto a full canvas', …)`, ou `test.fixme()` dentro de um
    teste.

  Um `test.fixme(` é o próprio marcador de "corrigir depois" do Playwright. Com ele, `npx playwright test` sai com
  exit 0 (`4 skipped`, conferido no clone), e a linha 5 só o pega nos 6 testes que ela nomeia. Logo, um teste e2e
  estacionado fora desses 6 passa por todos os gates. Isso não é regressão sobre a `main`, onde `*.mjs` nem era
  varrido, e nada assim existe no `HEAD`. Mas é mais largo do que a D-8 diz.
  **Correção:** tirar só os tokens e deixar uma chamada `test.fixme(` ser marcador:
  `… | sed -E "s/test\.fixme\b([^(]|$)/\1/g; s/'fixme'//g" | grep -Ei '\b(todo|fixme)\b' | grep -vEi '(todo|fixme)[^a-z]*\(?#[0-9]+' && echo OK`.
  Testei essa versão no clone: `OK` no `HEAD`, reprova as 6 mutações e aceita `test.fixme(…); // fixme #12` (com
  issue). Se a mudança for no PROJECT, ajustar a redação da D-8 ou fazer uma D-XX nova.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core: filtro, GIF ≤ 25 MiB, cópia única, não-GIF recusado; sem KLIPY/HTTP | CONTEXT | Auto | PASS | `OK` (2 + 3 passed; core só com `thiserror`; nenhum `klipy` em `crates/bezel-core`) |
| 2 | `KlipyClient` contra servidor HTTP loopback com JSON gravado | CONTEXT | Auto | PASS | `OK` (6 passed) |
| 3 | Studio e disco: nada sai sem chave/no início, chave privada, …; CSP igual | CONTEXT | Auto | PASS | `OK` (9 + 2 passed no Linux, incluindo `tests::the_app_setup_sends_nothing_at_start`; CSP igual à da `origin/main`; nenhuma capability com `opener:`/`http:`) |
| 4 | `Cargo.lock` só ganha pacotes; `ureq` só via `bezel-klipy`; CLI sem `ureq`/`rustls` | CONTEXT | Auto | PASS | `OK` |
| 5 | UI: `gif-search.test.mjs`, i18n, Playwright 4 projetos + axe | CONTEXT | Auto | PASS | `OK`: `# pass 4`, `test:unit` ok, `ok: 6 tests × 4 projects, 24/24 runs passed with axe`. Mutante de 1 projeto: exit 1 (W1 da iter 2 resolvido) |
| 6 | Guia en/pt-BR com Privacidade, check-docs e CHANGELOG | CONTEXT | Auto | PASS | `OK` |
| 7 | CI do Windows verde no HEAD do PR | CONTEXT | Auto | PASS | Esperei o run 36955188127 terminar (`gh run watch --exit-status`: exit 0). Depois rodei a linha como está escrita, com `HEAD` = remoto = `cc78ea1`: `OK` (`rust-windows` = `success`, 153 testes do studio rodaram) |
| 8 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`; 933/0/12 |
| 9 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`; TOTAL 94.40% |
| 10 | No `TODO`/`FIXME` without linked issue reference | PROJECT | Auto | PASS | `OK` no `HEAD`; o controle com TODO dá exit 1. Furo na exclusão: W1 |
| 11 | CHANGELOG.md updated with entry per release | PROJECT | Manual | MANUAL_REQUIRED (release) | `[Unreleased]` cita KLIPY; evidência sugerida: `## [x.y.z] - <data>` no corte de release |
| 12 | README accurately describes current behavior | PROJECT | Manual | MANUAL_REQUIRED (release) | README cita a busca e o guia; evidência sugerida: diff do README revisado no PR |

## CI (run 36955188127, `cc78ea1`)
- **Jobs:** todos `success`: `rust-windows`, `rust-linux`, `node-ui`, CodeQL (rust, javascript-typescript,
  actions), Varreduras, Versao e `Portao`. `sonar`, `imagem`, `publicar` e `lancar` ficaram `skipped`, como
  antes.
- **`rust-windows`:**
  - fmt ok;
  - `cargo clippy --all-targets --all-features -- -D warnings` ok;
  - testes com cobertura: **865 passed, 0 failed, 12 ignored** em 30 binários. Studio lib = **153**, todos ok;
  - build release ok;
  - MSI, NSIS e `.zip` ok.
- **`rust-linux`:** 933/0/12, com `tests::the_app_setup_sends_nothing_at_start ... ok`.

## Observações (sem aviso)
- O `SUMMARY.md` fala em "UI: 221 unitários". Hoje são 225 (eram 222 na iter 2, e a iter 3 somou 3). O texto
  da linha 7 ("depois do push") também ficou para trás: ela agora está verde.
- A checagem de projetos lê o `metadata`, não o `use`. O helper `project()` de `playwright.config.mjs` grava os
  dois a partir dos mesmos argumentos. Uma config escrita à mão poderia fazer os dois divergirem, mas isso seria
  sabotagem, não engano.
- Continuam valendo as observações das iters 1 e 2: o timeout de 10 s; `collected_users` lendo a biblioteca; os
  nomes reservados do Windows; `klipy.json` sem nova tentativa; o proxy; a janela de tempo do teste do `setup`; a
  raiz temporária que fica para trás quando um teste do `lib.rs` falha no meio.

## Recommendation
B1, B2, W1 e W2 da iter 2 estão resolvidos.
- **B1:** o CI do `HEAD` está verde. O binário de testes do studio sobe no Windows e roda 153 testes. O único
  teste a menos que no Linux é o do mock, como a D-8 decide.
- **B2:** a linha do PROJECT passa.
- **W1:** a linha 5 reprova uma config de 1 projeto.
- **W2:** a D-8 registra a mudança do PROJECT.

A parte (2) da D-8 está correta e com escopo estreito. A parte (1) precisa do ajuste de W1: tirar só o token, não
a linha, e tratar uma chamada `test.fixme(` como marcador. A correção é uma linha no PROJECT e já foi testada no
clone. Pode seguir para o PR. Antes de fechar a fase, aplicar W1 ou registrar o motivo de não aplicar, e no PR
fazer os itens de "Deferred to PR review" (T-8 na 8.8", termos do KLIPY, visual).

## DoD Critic (enhanced)

- DoD row «3 | Studio e disco …; CSP igual»: oca e objetiva — um `tauri.linux.conf.json` com outra CSP passa (o Verify só
  compara a linha de `tauri.conf.json`); um aquecimento atrasado 3 s no `setup` passa (o teste espera 1,5 s). As
  mutações da iter 1 agora falham.
- DoD row «4 | `Cargo.lock` só ganha pacotes»: oca e objetiva — um `[patch]` que troca a origem do `getrandom` 0.3.4
  só remove `source`/`checksum`, sem `-version`.
- DoD row «5 | UI …»: oca e objetiva — `oninput` chamando `submit` (busca a cada tecla, sem pausa) passa: o debounce
  só é provado na lógica isolada.
- DoD row «10 | No TODO/FIXME …» (PROJECT): oca e objetiva — o filtro de `test.fixme` descarta a linha inteira; uma
  cor `#444` conta como issue; `packaging/` fica de fora; o plural "TODOs" passa.
- Linhas 1, 2, 6–9: provam o critério (as mutações da iter 1 nas linhas 1 e 6 agora falham).

**Verdict:** BLOCKED
