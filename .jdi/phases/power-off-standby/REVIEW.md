# Phase 12: Review  (slug: power-off-standby)

**Verdict:** APPROVED

> Revisão em modo `verify`: a re-verificação única depois da rodada que corrigiu os avisos (passo 6 do `/jdi-issue`).
> O loop tinha convergido na iteração 3 (`APPROVED_WITH_WARNINGS`, W1–W3). Branch `jdi/power-off-standby`, `HEAD` =
> `b654f8c` (48 commits desde `origin/main`), que também é o último commit de código. A árvore ficou limpa durante toda
> a revisão.
>
> - **Escopo julgado:**
>   - o diff `f317fda..b654f8c` (3 commits; 10 arquivos, +280/−19), lido por inteiro;
>   - comparado com CONTEXT, PLAN, SUMMARY, PROJECT e as decisões D-2026-10-03-power-off-standby-1..9;
>   - cada aviso da iteração 3 (W1–W3) conferido no código, por execução e por mutação.
> - **Números:** todos vêm das minhas execuções nesta sessão; nenhum foi copiado do SUMMARY. Os `Verify:` do CONTEXT
>   rodaram exatamente como escritos: extraídos do arquivo e passados a `bash`, sem edição.
> - **CI (D-8):** o run `37159714157` (workflow_dispatch, `headSha` `b654f8c`) terminou `success`, e
>   `gh run watch 37159714157 --exit-status --interval 60` deu exit 0.
>   - `rust-windows` (job `111310514131`): `success`, com os passos `cargo clippy` e `cargo test com cobertura` verdes.
>   - No log do Windows: 944 passed, 0 failed, 12 ignored. Entre eles estão
>     `power::tests::a_session_end_applies_the_choice_and_a_quit_does_not`, os dois testes novos de `power::tests` e
>     `storage::tests::the_archive_per_call_is_locked_only_while_it_is_called`, todos `ok`.
>   - `rust-linux`, `node-ui`, CodeQL (rust, js/ts, actions), `Varreduras` e `Portao`: `success`.
> - **Hardware e studio do usuário:** nada abriu `/dev/ttyACM*`; nenhum teste `#[ignore]`/`BEZEL_HW_TESTS` rodou.
>   - O studio instalado (PID 570720) seguiu vivo e intocado.
>   - O `studio-starts-silent.sh` isolou cada execução (compositor, barramento e pastas próprios) e passou 2 de 2: uma
>     dentro da linha 7 e outra avulsa (`OK: in 4 runs of 12s`). O `kwin_wayland --virtual` não caiu.
> - **Prova por mutação:** numa cópia do `b654f8c` no scratchpad (`git archive`, `CARGO_TARGET_DIR` próprio, apagada no
>   fim; o repositório não foi tocado), desfiz cada correção, uma por vez.
>   - **W1:** `archive_per_call()` → `&mut **self.storage.archive()` em `power.rs:353`.
>     `a_hung_screen_holds_no_command_on_the_catalog` **falhou** com `left: Err(Timeout)`, "the catalog waited for the
>     hung screen" (`power/tests.rs:620`).
>   - **W2:** o `match` de `app/standby.rs:307` → `record_stored(...)?; Ok(Applied::Album)`.
>     `the_album_at_shutdown_restarts_though_its_record_is_not_saved` **falhou**: `Err(Transport("no space left on the
>     disk"))` em vez de `Ok(AlbumNotRecorded(..))`.
>   - **W3:** `'read'` → `'none'` em `src/standby.js:256`. O teste `a write whose screen is shown again before its
>     answer reads that screen again` **falhou** ("A keeps its choice from before the write"); o arquivo deu 28/1.
>   - Os testes vizinhos (`the_archive_per_call_is_locked_only_while_it_is_called`,
>     `the_album_at_shutdown_records_the_plan_b_it_stores`) continuaram passando.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0 |
| Tests | PASS | **1045 passed, 0 failed, 12 ignored** (hardware/ffmpeg real/KLIPY real). Na iteração 3 eram 1041: +4, nenhum removido (core `tests/standby.rs` +1, studio `power::tests` +2, `storage::tests` +1) |
| Coverage | PASS | **94.95%** linhas (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do PROJECT, sem filtro: **94.90%**. Arquivos tocados: `power.rs` (studio) 85.56%, `storage.rs` (studio) 96.77%, `diag.rs` 98.92%, core `app/standby.rs` 99.39% |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy `x86_64-pc-windows-msvc` dos 8 crates sem studio (`bezel-core`, `-devices`, `-sensors`, `-render`, `bezel`, `-themes`, `-media`, `-power`): exit 0. Clippy do studio no Windows: verde no CI (`rust-windows` do `b654f8c`). Nenhum `allow` novo no diff |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 limpos (detalhe abaixo). `cargo audit`: nenhuma vulnerabilidade (1290 advisories, 621 crates) |
| Consistency | PASS | Escopos: 46 commits `power-off-standby` + 2 `chore(jdi)`. Os 10 arquivos do diff estão no `files_modified` do PLAN (T-1: core `app/standby.rs` e `tests/standby.rs`; T-4: `power.rs`, `power/tests.rs`, `storage.rs`, `diag.rs`; T-6: `standby.js`, `ui/standby.js`, `standby.test.mjs`). Nenhuma D-XX quebrada. Nota N2 (SUMMARY) |
| UI Validation | PASS | `npm test`: **266/266** unit (99.95% de linhas; +1, o teste do W3) e **228/228** Playwright (claro/escuro × pt-BR/en, axe). Os 5 e2e de standby passam nos 4 projetos. Nenhum texto novo na UI |
| DoD | PASS | As 8 linhas Auto do CONTEXT passam como escritas, inclusive a 3 com o CI do `b654f8c`. As 3 Auto do PROJECT também. As 2 Manual do PROJECT ficam para o corte de release |

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: só `thiserror` |
| 5.2 I/O e threads no core | PASS: nada. O `AlbumNotRecorded` só embrulha o erro da porta `ArchiveStore` |
| 5.3 ports | PASS. Nenhuma impl de porta no core. Traits públicas fora do core: as mesmas de antes (`Clock`, `Pause`, `Monotonic`, `Wire`, `Pace`, `Pictures`, `MediaSetup`). O `ArchivePerCall` é um adapter do studio que implementa a porta do core, `pub(crate)` |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs` e testes |
| 5.5 `unsafe` | PASS: nenhum novo. O `bezel-power/src/session.rs:23` continua com `// SAFETY:` |
| 5.6 panics | PASS: os 13 `unwrap`/`expect` novos estão todos em código de teste (`power/tests.rs`, `storage/tests.rs`, core `tests/standby.rs`). Nenhum no código de produção do diff |
| 5.7 escrita no dispositivo | PASS. `Confirm::Yes` fora de teste: só em `Confirmed::require`/`MonitorModeConfirmed::require` e doc, como antes. A ação do `album` continua sob `Confirmed::recorded` (`app/standby.rs:303-304`); o diff não muda nenhum pacote |
| 5.8 protocolo | PASS: o diff não toca o protocolo. Conferi de novo `SET_BRIGHTNESS` 0x7B (`protocol/turing_rev_c.rs:43`) e `RESTART` 0x84 (`:51`) contra o §19 (`protocol-turing-rev-c.md:786`) |
| 5.9 caminhos no core | PASS: nada novo; só doc e fixtures de teste antigos |
| 5.10 comandos síncronos | PASS: os não-async são os de antes (`list_fonts`, `preferences`, `cancel_job`…), e nenhum toca a tela nem o catálogo |
| 5.11 supply chain | PASS: `cargo audit` limpo; nenhum segredo; o diff não toca `Cargo.toml`/`Cargo.lock` |

### Avisos da iteração 3
| Aviso | Situação | Evidência |
|---|---|---|
| W1 guard do catálogo preso durante o I/O da tela | **Resolvido** | O `apply` (`power.rs:351-357`) entrega ao core um `ArchivePerCall` (`storage.rs:129-151`), que trava o mesmo mutex só dentro de cada `load`/`save`/`keep`/`read`/`discard`. O guard temporário de `apply_choices` (`power.rs:312`) cai no fim do `let`, antes do `apply`, então não há autotrava. O teste `a_hung_screen_holds_no_command_on_the_catalog` (`power/tests.rs:596`) percorre o fluxo real: tela travada no `turn_off_now`, prazo vencido, desligamento cancelado; o `cache_info` responde em menos de 2 s. A mutação prova que o teste falha sem a correção |
| W2 `album` reiniciado com o registro não salvo dito como falha | **Resolvido** | O core devolve `Applied::AlbumNotRecorded(erro)` (`app/standby.rs:63`, `:307-310`); erros antes do RESTART continuam `Err`. O teste do core exige INFO, OPTIONS e RESTART (nada mais), 1 `save` tentado e o catálogo igual; com o registro já certo, nada é salvo e vem `Applied::Album`. O studio diz `DiagCode::ShutdownAlbumNotRecorded` (`power.rs:255-260`) e guarda `ShutdownChoiceFailed` para falhas de verdade (`an_album_not_recorded_is_not_said_as_a_failed_choice`, `power/tests.rs:628`) |
| W3 escrita que correu com uma leitura da mesma tela | **Resolvido** | `createAnswers().written(ticket, key, shown)` (`src/standby.js:254-257`) devolve `draw`, `read` ou `none`. No caso `read`, o painel lê a tela de novo se a aba está à mostra (`ui/standby.js:376-380`); a leitura que pode ter visto o catálogo antigo perde a ficha para a nova. Conferi os casos: volta para A antes da resposta → `read`; resposta antes da volta → `none`, e a leitura da volta já vê o salvo; escrita para a tela mostrada sem leitura no meio → `draw`. A mutação prova o teste |

### O diff contra o resto da fase (regressões)
- **`ArchivePerCall` e a ordem de travas:** enquanto chama a porta, a thread `bezel-shutdown` não segura nenhuma outra
  trava. No estado final, todo comando que grava o catálogo pede antes o `claim()`, que é recusado: o `with_screen` do
  armazenamento (`storage.rs:540`), o gerenciador (`manager.rs:311`, `:354`, `:372`, `:388`; `manager/plans.rs:247`,
  `:295`) e a escolha e o álbum (`standby.rs:358`). Então, num desligamento em curso, nada mais grava o
  catálogo; o efeito do travamento por chamada só aparece depois de um cancelamento (nota N1).
- **`Applied::AlbumNotRecorded`:** é variante pública nova de um enum sem `#[non_exhaustive]`.
  - O único `match` fora de testes é o `said_of`, que a trata. Os testes do driver rev C não mudaram e passam.
  - A doc do `at_shutdown` (`app/standby.rs:244-249`) diz o contrato novo.
  - A D-9 continua valendo: com sucesso, o OPTIONS gravado vira o plano B registrado. Com falha, a resposta diz que o
    registro ficou com o plano B anterior.
  - D-2 (5) e D-3 (1): os pacotes são os mesmos.
- **`DiagCode` novo:**
  - O enum tem 58 variantes, e `ALL: [Self; 58]` lista as 58 na ordem da declaração (conferi por script). O `text()`
    tem 58 braços.
  - A frase é fixa, sem dado e única (`each_code_has_its_own_fixed_sentence` ok).
  - O canal é `Warning` pelo `_`, como o `ShutdownChoiceFailed`. A lista do terminal em
    `what_the_terminal_shows_is_what_was_printed_before` não muda, e o teste reporta todos os códigos.
  - As guardas `nothing_in_the_app_forges_an_invocation` (códigos sem dado) e `the_studio_installs_no_logger`
    passam.
- **UI:** só lógica pura nova e uma chamada a `load()`. Nenhuma string, nenhum elemento novo; i18n, claro/escuro e axe
  seguem verdes.

## Blockers
_(nenhum)_

## Warnings
_(nenhum)_

### Notas (não contam como aviso)
- **N1** `crates/bezel-core/src/app/standby.rs:318-328` (com `apps/bezel-studio/src-tauri/src/power.rs:353`): o
  `record_stored` lê e salva o catálogo em duas travas separadas. Antes da correção do W1, isso era atômico diante das
  outras threads do studio.
  - Para perder uma gravação, três coisas precisam acontecer juntas: um desligamento cancelado com uma tela travada
    no `album`; essa tela destravar e terminar o RESTART depois; e um comando do studio tomar o mutex exatamente entre
    o `load` e o `save`, um intervalo de microssegundos só em memória.
  - É a mesma classe de corrida que já existe entre processos com a CLI (o `DiskArchive` não tem trava entre
    processos). Não pede correção.
- **N2** `.jdi/phases/power-off-standby/SUMMARY.md:49-60`: o SUMMARY para na iteração 3 (1041 testes). Falta registrar
  a rodada `f317fda..b654f8c` (1045 testes, 94.90%) e a variante pública nova `Applied::AlbumNotRecorded` em
  "Contratos mudados". Registrar antes do PR.
- **N3** `apps/bezel-studio/src/ui/standby.js:380`: a releitura no caso `read` é uma linha de ligação. Só a decisão pura
  tem teste: `src/ui/**` fica fora da cobertura unitária por configuração, e nenhum e2e reproduz a corrida. A mutação
  mostra que a decisão está provada; a ligação fica para a revisão de código.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | Core e driver rev C: plano B e ações = pacotes exatos; OPTIONS inteiro; keepalive; impossível→TURNOFF | CONTEXT | Auto | PASS | `OK`, exit 0. `standby_` **8 passed** (≥4); core `--test standby` **11 passed** (≥6; novo `the_album_at_shutdown_restarts_though_its_record_is_not_saved`) |
| 2 | Linux: 1 inibidor *delay*; `true` aplica e só então fecha o fd; `keep`/ticks/reconexão sem chamadas; `liveScreen` fica; prazo; `false` retoma | CONTEXT | Auto | PASS | `OK`, exit 0. `power::tests::linux_` **12 passed** (≥6). Todo o `power::tests`: 21 passed |
| 3 | Windows: o `Exit` de uma sessão que acaba aplica, sair e outros eventos não; `rust-windows` verde no último commit de código (D-8) | CONTEXT | Auto | PASS | `OK`, exit 0. Os 2 `--exact` dão `ok. 2 passed`. `git log -1 -- . ':!.jdi'` = `b654f8c` → run `37159714157` → job `rust-windows` `success` (clippy e testes do Windows verdes; 944/0/12) |
| 4 | Studio: `Confirm::No` sem chamadas; `keep` desfaz; catálogo compartilhado; álbum lista/adiciona; substituir e remover só confirmados | CONTEXT | Auto | PASS | `OK`, exit 0. `standby::tests` **10 passed** (≥5) |
| 5 | CLI: sem `--yes` nada na tela nem no registro; com `--yes` o brilho e depois o plano B; `album add` horizontal e vertical | CONTEXT | Auto | PASS | `OK`, exit 0. `bezel --test standby` **6 passed** (≥5); os 2 `--exact` dão `ok. 2 passed` |
| 6 | UI: lógica pura, i18n, 5 e2e nomeados × 4 projetos com axe | CONTEXT | Auto | PASS | `OK`, exit 0. `standby.test.mjs` tap `# pass 29`, `# fail 0`; `test:unit` 266/0; `e2e-passed`: "5 tests × 4 projects, 20/20 runs passed with axe" |
| 7 | Guardas: comandos, fonte, sem logger, início silencioso; monitor vê só logind; nenhum pacote novo | CONTEXT | Auto | PASS | `OK`, exit 0, na 1ª execução. Os 4 `--exact` dão `ok. 4 passed`; `speaks_only_to_logind_on_its_bus` 1 passed; no lock só `+name = "bezel-power"`. O script deu `studio-starts-silent: OK: in 4 runs of 12s …` nas 2 execuções |
| 8 | Guia en/pt-BR, §19/§20, CHANGELOG, check-docs | CONTEXT | Auto | PASS | `OK`, exit 0; `check-docs: 16 pages in English and Portuguese, links and privacy checked; all checks passed` (exit 0) |
| P1 | `cargo test --workspace` | PROJECT | Auto | PASS | 1045/0/12, exit 0 |
| P2 | Cobertura ≥ 80% | PROJECT | Auto | PASS | 94.90% (comando do PROJECT, sem filtro), exit 0 |
| P3 | Sem TODO/FIXME sem issue | PROJECT | Auto | PASS | `OK`, exit 0 |
| P4 | CHANGELOG por release | PROJECT | Manual | MANUAL_REQUIRED (release) | O `[Unreleased]` tem a entrada. Evidência sugerida: `## [x.y.z] - <data>` no corte |
| P5 | README descreve o comportamento atual | PROJECT | Manual | MANUAL_REQUIRED (release) | Evidência sugerida: diff do README revisado no PR |

P4 e P5 são do corte de release, não desta fase (o CONTEXT não tem linha Manual), como nas iterações anteriores. Por
isso o veredito não é `APPROVED_PENDING_MANUAL`.

Fica para o PR (CONTEXT, não é blocker):
- desligar de verdade com o 8.8" em cada opção e religar;
- desligar no Windows;
- revisão visual;
- fotos em pé nas duas orientações no 8.8";
- tema parado com temporizador de 1 min não dorme (D-5).

## Recommendation
A fase pode seguir para `/jdi-ship power-off-standby`.

Os três avisos da iteração 3 estão resolvidos, e cada correção tem um teste que falha quando ela é desfeita:
- o desligamento não segura mais o catálogo enquanto a tela trabalha;
- um álbum reiniciado cujo registro não foi salvo é dito com um código próprio, não como falha;
- a UI relê a tela quando uma escrita correu com uma leitura dela.

O diff não quebra nenhuma decisão, e os gates, as 8 linhas do DoD e o CI do Windows estão verdes.

Antes do PR, registre a rodada no SUMMARY (N2). As notas N1 e N3 não pedem mudança.

## DoD Critic (enhanced)

_(nenhuma linha oca: os 3 commits da rodada de avisos não abrem lacuna em nenhuma das 8 linhas Auto)_

**Verdict:** APPROVED
