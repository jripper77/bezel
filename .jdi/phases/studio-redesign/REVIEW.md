# Phase 14: Review  (slug: studio-redesign)

**Verdict:** APPROVED_WITH_WARNINGS

Revisão em 2026-10-09 sobre HEAD 28a5fe0 (branch windows-improvements), diff 838d7eb..HEAD (115 arquivos). Ambiente Windows; os gates JS rodaram sem nenhuma outra carga na máquina.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked` exit 0 (Cargo.lock aceito) |
| Tests | PASS | Rust: 1038 passed, 0 failed, 15 ignored (hardware), igual à SUMMARY. Studio unit: 350/350. Playwright: 396/396 (light-pt, dark-pt, light-en, dark-en; `retries: 0`) |
| Coverage | WARN | Rust: `cargo llvm-cov` não está instalado, então a cobertura não foi medida (fallback). Studio (`npm run test:unit`, piso de 80%): 99.42% de linhas (all files) |
| Lint | PASS | `cargo fmt --all --check` exit 0; `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0; o diff da phase não adiciona nenhum `#[allow]` |
| Hexagonal/Safety/Protocol/Hygiene | PASS (com WARN) | 5.1, 5.2, 5.3a, 5.4, 5.8 e 5.9 sem achado na phase. 5.5, 5.6 e 5.10 só têm achados anteriores à phase, em arquivos que ela não toca. 5.11: `cargo audit` não instalado (WARN) e nenhum segredo encontrado |
| Consistency | PASS (com WARN) | 30 commits, todos com scope `studio-redesign`. T-1..T-9 têm commits e testes. Os arquivos fora de `files_modified` se justificam (ver W5). Nenhuma D-XX é violada |
| UI Validation | PASS | `npm test` exit 0. axe em light e dark × pt/en sem violações. Flake card-animation:3 não reproduzido nesta execução (ver W4) |
| DoD | PASS (com WARN) | Todos os itens Auto da CONTEXT passam. Do baseline PROJECT, a cobertura Rust não foi executada (ferramenta ausente) |

## Blockers
- nenhum

## Warnings
- **W1. Cobertura Rust não medida (Gate 3 / DoD PROJECT).** `cargo llvm-cov` responde "no such command". Para instalar: `cargo install cargo-llvm-cov --locked && rustup component add llvm-tools-preview`. O risco é baixo, porque o Rust da phase só tem refactors de `collapsible_if` para let-chains (theme.rs, renderer.rs, studio.rs) e `parse_language` com `it` (texts.rs, coberto por teste). Mesmo assim, o número precisa ser medido antes do ship.
- **W2. `cargo audit` não instalado (5.11).** Para instalar: `cargo install cargo-audit --locked`. A phase não muda `Cargo.lock`.
- **W3. Botão "×" do Connect (T-7) ainda sem aprovação do usuário. Ponto mais próximo de conflitar com D-9.**
  - O que ele faz: `apps/bezel-studio/src/ui/connect.js:99-110`, rótulo `connect.hide` = "Ocultar e editar o tema" / "Hide and edit the theme" / "Nascondi e modifica il tema" (`i18n/*.js:1189`). Ele só esconde o cartão (`hiddenFor = signature()`) até a situação mudar e põe o foco em `#zoom-fit`. Não liga o modo demo, não simula tela, não cria dados falsos e não chama o bridge.
  - Avaliação técnica: **não viola D-9.** Não há lógica nova nem placeholder falso. Editar sem tela já funcionava antes da phase. Sem o ×, o cartão cobriria o canvas o tempo todo quando não há tela, o que seria uma regressão.
  - Conflito semântico: o artboard Connect **não tem** botão de fechar (os três "×" do `Connect.dc.html` são dimensões "480×1920"). "Prova senza schermo" diz no mockup "Puoi progettare un tema ora e inviarlo dopo", que é a mesma promessa de "Ocultar e editar o tema". Na prática, o × reintroduz a intenção do item omitido com outro nome. O todo (`.jdi/todos/2026-10-09-studio-redesign.md:1`) diz que o item "só existe no modo demo web" e por isso não tem lógica no Tauri, o que deixa de ser verdade para essa leitura.
  - Recomendação (decisão do usuário):
    - (a) aprovar o × e registrar uma D-XX esclarecendo que "Prova senza schermo" omitido = trocar para o backend demo, não fechar o cartão; ou
    - (b) manter o × com rótulo neutro (`dialog.close`, "Fechar") para não prometer um modo "sem tela".
  - Em nenhum dos dois casos é bloqueio.
- **W4. Flake conhecido `card-animation:3` (flip: amostragem de quadros por tempo).** Nesta execução passou nos 4 projetos (9.0 a 9.7 s). A SUMMARY registra 1 falha em 396 numa segunda execução com 10 workers sob carga. O teste depende de tempo de CPU e não foi corrigido, então o risco de falha intermitente continua em CI ou em máquina carregada. Isto não é um PASS garantido.
- **W5. Arquivos fora de `files_modified`.** Todos se justificam, nenhum é escopo extra:
  - `src/ui/library.js` (T-3, T-5): o keydown ↑/↓/Home/End do rail e o card de tela do painel Screen moram nesse arquivo. Mudança necessária.
  - `src/app.js` (T-5, T-6): guarda de drop em `storage-wide` (corrige a regressão video-background:30 vinda de T-3) e 1 linha `inspector.frameShown()` para o preview ao vivo. Em T-7 e T-9 o arquivo já estava listado.
  - `src/libre-status.js` (T-7): `hardware: [{name, ok}]`, com a mesma regra de `failed`, alimenta a lista por hardware do popover. Coberto por `libre-status.test.mjs:19`.
  - `tests/e2e/shell.spec.mjs` (T-7): `#restart-libre` saiu da statusbar para o popover, então a spec acompanhou.
  - `scripts/serve-e2e.py` e `playwright.config.mjs` (T-9): trocam `http.server` (backlog 5, HTTP/1.0) por um servidor com thread por conexão, backlog 256 e keep-alive. É só infra de teste e serve o mesmo `src/`. A causa dos flakes de carga é real e está bem explicada. Nota menor: ele escuta em `127.0.0.1` e o `url` do webServer é `localhost`. Funciona aqui, mas fica frágil se `localhost` resolver primeiro para `::1`.
  - Também fora da lista: `crates/bezel-render/src/renderer.rs` (T-9, 1 `collapsible_if`, exigido pelo clippy `--workspace`), `tests/ui/{library,live-screen}.test.mjs` e `tests/e2e/{storage-manager,themes}.spec.mjs`. Todos estão ligados às mudanças.
- **W6. Higiene de documentação.** `PLAN.md:142` diz "Total tasks: 8", mas são 9 com T-9. A seção "Files modified" da `SUMMARY.md` não lista vários arquivos de T-3..T-6/T-8 (ex.: `ui/inspector.js`, `ui/library.js`, `inspector-tabs.js`, `scripts/build-icons.mjs`, `src-tauri/icons/*`).
- **W7. Cores literais fora de hex (dentro da letra de D-4, contra o espírito dela).**
  - `styles.css:943` (novo, copiado de `:516`, que já existia) usa `box-shadow ... rgb(15 23 42 / 12%)` fora dos blocos de token.
  - `styles.css:33` `--marquee: rgb(255 146 72 / 14%)` está dentro do bloco de token, mas repete os componentes do acento `#FF9248` em vez de derivá-lo (`color-mix(in srgb, var(--accent) 14%, transparent)`). Se o acento mudar um dia, há dois lugares para atualizar (DRY).
  - O guard `static-guards.test.mjs` só procura hex, então passa. Sugestão: tokens de sombra.
- **Fora do escopo da phase (anteriores, não tocados):** `unsafe` em `crates/bezel-sensors/src/{fps/rtss.rs,windows/activity.rs}` e `crates/bezel-power/src/session.rs` (5.5); `#[allow]` em bezel-power/bezel-sensors/bezel-core (4b); 19 `#[tauri::command]` síncronos (5.10). Nenhum deles aparece no diff 838d7eb..HEAD.

## Conformidade com as decisões locked
| Decisão | Status | Evidência |
|---|---|---|
| D-4 sem hex fora dos tokens; acento único | PASS | `static-guards.test.mjs` verde (parser de regras; os únicos blocos de token são `:root` e `@media (prefers-color-scheme: dark)`); `--accent: #FF9248` uma vez só (`styles.css:13`); nenhum hex novo em JS (diff `src/ui`, `src/*.js`). Ver W7 |
| D-5 light + dark com axe | PASS | 4 projetos Playwright e `expectAccessible` nas specs novas (shell, connect, libre) |
| D-6 CSP inalterada, sem Google Fonts, Plex local, OFL | PASS | `tauri.conf.json` sem diff; guard da CSP fixa a string; nenhum `googleapis`/`gstatic` em `src/` nem na conf; os 5 woff2 batem com o SHA-256 do `ibm-plex/README.md`; `LICENSE` = OFL-1.1 com copyright IBM; linhas de licença em `README.md:197`, `FORK.md:98`, `packaging/windows/README.md:47` e `package-evo.ps1:54` (`LICENSE-IBM-Plex-OFL.txt`) |
| D-7 paridade en/pt-BR/it, nenhuma string fixa, "Card" | PASS | `i18n.test.mjs`: paridade com faltando/sobrando e placeholders, "no text…", "no sentence…" (agora verde sem enfraquecer o teste, após 60f174a), "Card" em en/it (:63); grep por "Carta"/"Carte" em `it.js` vazio; `index.html` só tem chaves `data-i18n` (popover e `#connect`) |
| D-8 SVG inline e controles nativos | PASS | Ícones via `ICONS` / `<svg>` inline; toggle do Libre é `<button aria-haspopup=dialog aria-expanded aria-controls>`; switch de autostart é checkbox nativo; abas com `role=tablist/tab/tabpanel` e setas; nenhum `<img>`/icon-font novo; `shortcuts.js` intocado |
| D-9 só features com lógica, nenhum dado falso do mockup | PASS (W3) | Connect lista só o que vem do bridge (`screens`, `desktopMode`, `screenError`, `denied`); nenhum nome/valor do mockup em `src/` ("COM5", "Monitor di sistema", "42 °C" ausentes; `demo-data.js` é anterior à phase e é dado do modo demo); omissões registradas na SUMMARY e nos todos |
| D-10 "Riavvia" = só `bridge.restartLibre` | PASS | `app.js:536-548`: o handler de `#restart-libre` chama apenas `bridge.restartLibre()` (mais toast/`fail`); o popover não tem outros reinícios (`sensor-status.js`) |
| D-11 DoD só automática | PASS | Itens Auto da CONTEXT verificados abaixo |
| D-12 "Prova sulla faccia successiva" | PASS | `inspector.js:638`: botão com ícone play → `cardFace` `(activeFace+1) % n`, chave `card.animateNext` ("Prova sulla faccia successiva" em it); item ausente dos todos |

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Unit do Studio passa com ≥80% de linhas | CONTEXT | Auto | PASS | `npm run test:unit`: 350/350, all files 99.42% |
| 2 | Playwright + axe em light e dark | CONTEXT | Auto | PASS | `npm test` exit 0, 396 passed (1.9m), 4 projetos |
| 3 | Paridade i18n en/pt-BR/it | CONTEXT | Auto | PASS | `i18n.test.mjs` "en, pt-BR and it have the same keys, each with the same placeholders" |
| 4 | Guardas estáticas (CSP, Google Fonts, hex) | CONTEXT | Auto | PASS | `static-guards.test.mjs` (6 testes) verde em `test:unit` |
| 5 | Rust íntegro se tocado | CONTEXT | Auto | PASS | `cargo test --workspace --locked` 1038/0/15; `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0; `cargo fmt --all --check` exit 0 |
| 6 | `cargo test --workspace` exit 0 | PROJECT | Auto | PASS | idem #5 |
| 7 | Cobertura Rust ≥ 80% | PROJECT | Auto | WARN (não executado) | `cargo-llvm-cov` ausente (W1) |
| 8 | Nenhum TODO/FIXME sem issue | PROJECT | Auto | PASS | Verify do PROJECT impresso `OK`; o diff da phase não adiciona TODO/FIXME |
| 9 | CHANGELOG por release | PROJECT | Manual | N/A nesta phase | D-11 (DoD da phase só automática) e a phase não é release. Evidência sugerida no próximo release: `## [0.1.23]` no CHANGELOG.md citando o redesign do Studio e o locale it |
| 10 | README descreve o comportamento atual | PROJECT | Manual | N/A nesta phase | D-11. A phase só adicionou a linha de licença Plex (`README.md:197`). Evidência sugerida: revisão do README no PR do release |

## Recommendation
Aprovado com avisos: sem blockers, todos os gates executados passaram e todas as decisões locked (D-4..D-12) estão atendidas. Antes do `/jdi-ship`:
1. O usuário decide sobre o botão × do Connect (W3): aprovar e registrar uma D-XX que esclareça o escopo de "Prova senza schermo", ou trocar o rótulo para "Fechar" (`dialog.close`).
2. Instalar `cargo-llvm-cov` e `cargo-audit` e rodar Gate 3 e 5.11 para fechar o item 7 da DoD.
3. Opcional: corrigir `PLAN.md:142`, completar "Files modified" na SUMMARY e tokenizar as sombras e `--marquee` (W6, W7). Considerar estabilizar `card-animation:3` (W4) para não depender do tempo de CPU.

## Pós-review (correções após o verify, 2026-10-09)

Decisões do usuário e correções aplicadas depois do verdict acima:
- **W3 — "×" do Connect:** mantido com rótulo neutro "Fechar"/"Close"/"Chiudi" (`connect.hide`), commit be35503; decisão D-2026-10-09-studio-redesign-13.
- **W7 — literais de cor:** sombra das abas segmentadas virou token `--seg-shadow` (light + dark); `--marquee` agora é `color-mix(in srgb, var(--accent) 14%, transparent)`. Commit be35503. Unit 350/350; e2e connect, workspace-selection, inspector-sections, storage: 76/76.
- **W6 — docs:** PLAN.md "Total tasks: 9"; SUMMARY.md "Files modified" regenerado de `git diff --name-only f3b068d..HEAD`.
- **W1 — gates Rust que faltavam** (`cargo-llvm-cov` 0.9.1 e `cargo-audit` 0.22.2 instalados):
  - `cargo llvm-cov --workspace --locked --summary-only --fail-under-lines 80 --ignore-filename-regex '(^|/)(main|build)\.rs$'`: exit 0, **TOTAL Lines 91.84%** (regions 91.31%, functions 88.43%).
  - `cargo audit`: exit 0, 634 dependências, 1296 advisories, nenhuma vulnerabilidade.

Warnings que permanecem: W4 (`card-animation:3`, flake por amostragem temporal sob carga) e W5 (arquivos fora de `files_modified`, todos justificados). O verdict permanece APPROVED_WITH_WARNINGS.
