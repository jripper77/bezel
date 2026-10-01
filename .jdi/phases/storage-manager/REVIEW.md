# Phase 8: Review  (slug: storage-manager)

**Verdict:** APPROVED_PENDING_MANUAL

> Re-verificação em modo `verify`, depois do BLOCKED de `13a734c`. `HEAD` = `2a4123c`, árvore limpa e igual a
> `origin/main`.
>
> - **Escopo julgado:** as correções `dae15c5`, `83d9c7f`, `bcd2216`, `4fd71d3`, `6bb8152`, `f57d8cb`, `ebb1eca` e
>   `5108f40`, mais os docs `c186cb8` e `2a4123c`. O resto da fase foi reconferido pelos gates, à procura de regressão,
>   e contra as decisões D-2026-09-30-storage-manager-1..14 (não há D-XX nova desde o review anterior).
> - **Números:** rodei todos os gates; nenhum número veio do SUMMARY. Onde o meu difere do dele, digo (W1 e
>   "Divergências do SUMMARY").
> - **Hardware:** nenhum comando abriu a 8.8" real. Não rodei `#[ignore]` nem `BEZEL_HW_TESTS`, não toquei em
>   `/dev/tty*`, hidraw ou serviços. Os comandos `bezel --fake storage …` rodaram com `XDG_DATA_HOME` num diretório
>   temporário do scratchpad (o `--fake` usa catálogo em memória e não criou nenhum arquivo nele). A pasta real
>   `~/.local/share/bezel/storage` não foi tocada: `mtime` 00:41:54, igual antes e depois desta verificação.
> - **Playwright:** rodou com `BEZEL_E2E_PORT=1443`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1). Nenhum `Cargo.toml`/`Cargo.lock` mudou desde `13a734c` |
| Tests | PASS | **825 passed, 0 failed, 9 ignored** (36 binários). Os ignorados: 6 de ffmpeg real, 1 de timing, 1 de hardware do studio, 1 de corpus. Subiu de 819 para 825 (+6, os testes das correções). Bate com o SUMMARY. Windows, no CI de `2a4123c`: 761 passed, 0 failed, 9 ignored |
| Coverage | PASS | **94.48%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD, sem filtro: **94.42%** (o SUMMARY ainda diz 94,38%; W1) |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. Clippy cruzado `x86_64-pc-windows-msvc` (7 crates, `CARGO_TARGET_DIR=target/wincheck`): exit 0. Os únicos `allow` fora de teste seguem os 4 de `fps/rtss.rs:247,269,302,323`, com `reason =` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1–5.11 limpos. O 5.7 não acusa mais `manager/plans.rs` (B1 resolvido). `cargo audit`: exit 0 (1277 advisories, 604 crates). Detalhe abaixo |
| Consistency | PASS (com WARN) | Os 10 commits desde `13a734c` têm escopo `storage-manager`, e cada correção tem teste. Nenhuma D-XX contrariada. Restam registros desatualizados no SUMMARY (W1) |
| UI Validation | PASS | `npm ci` ok. Comando do gate: **160/160** (4 projetos: claro/escuro × pt-BR/en, axe). `npm run test:unit`: **159/159**, 99.85% de linhas (`storage-manager.js` 100%). `i18n.test.mjs`: 13/13, com 742 chaves em cada idioma |
| DoD | PASS / PENDING MANUAL | As 5 linhas Auto do CONTEXT e as 3 do PROJECT passam como escritas, e nenhuma passa com filtro vazio. Faltam a parte do studio dos 2 Manual do CONTEXT e os 2 Manual do PROJECT |

**Cobertura dos arquivos tocados pelas correções (linhas):**
- **core:** `domain/cleanup.rs` 99.79%, `app/manager/report.rs` 96.15%, `app/manager/transfer.rs` 95.74%,
  `app/manager/cleanup.rs` 100%.
- **CLI:** `storage/transfer.rs` 96.51%.
- **studio:** `manager/plans.rs` 97.06%, `manager.rs` 94.42%, `manager/dto.rs` 87.85%, `messages.rs` 98.71%.
- **Abaixo de 80%** (iguais ao review anterior, nenhum por causa das correções): a cola do Tauri (`commands.rs` 1.08%,
  `lib.rs` 30.22%), `bezel-media/src/lib.rs` 48.90% (ffmpeg real só nos `#[ignore]`) e `domain/catalog.rs` 66.48%.
  Os dois comandos alterados (`run_plan`, `delete_files`) continuam só cola: a lógica está em `manager/plans.rs`.

### CI
- **Run 36818984656** (`2a4123c`, `HEAD`): `completed success`.
  - Jobs verdes: `rust-linux` (05:27Z), `rust-windows` (05:30Z) com 28 resultados de teste, **761 passed, 0 failed,
    9 ignored** (eram 755 no run de `9a7dd17`), `node-ui`, CodeQL (rust, js, actions), Varreduras e `Portao`. `imagem`,
    `sonar` e `publicar` saíram como `skipped`, como esperado.
  - A release **`v0.12.0`** saiu às 05:31Z sobre `2a4123c`, com `.msi`, `-setup.exe`, `.deb`, `.rpm`, AppImage, os
    binários da CLI e `SHA256SUMS`. Esse é o build ≥ `2a4123c` indicado para a T-8 do studio.

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS: em `[dependencies]` só há `thiserror` |
| 5.2 I/O e threads no core | PASS: nada |
| 5.3 ports | PASS. Nenhuma impl de porta em `crates/bezel-core/src` (estendi a busca a `ArchiveStore`, `MediaTranscoder` e `ScreenStorage`). As traits públicas fora do core são as mesmas do review anterior (`Pause`, `Wire`, `Clock`, `Pace`, `MediaSetup`, `Pictures`), todas auxiliares de adapter |
| 5.4 adapters na composição | PASS: nada fora de `main.rs`/`lib.rs` e testes (estendi a busca a `DiskArchive`) |
| 5.5 `unsafe` | PASS: só os blocos de `fps/rtss.rs`, com `// SAFETY:`, sem mudança |
| 5.6 panics | PASS. Nos arquivos alterados, todo `unwrap`/`expect` fica em `#[cfg(test)]` (`messages.rs` a partir de `:284`, `cleanup.rs` de `:379`) |
| 5.7 escrita no dispositivo | **PASS**. Os acertos são só doc, os `match` de `Confirmed::require`/`MonitorModeConfirmed::require` e módulos `#[cfg(test)]` (`fake.rs:675`, `turing_rev_c.rs:629`, `turing_usb.rs:508`, `hid_desktop.rs:262`, `backend.rs:846`). `manager/plans.rs` saiu da lista. Nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS. Nenhum arquivo de `bezel-devices` mudou; o laço de testes por encoder não acusa nada. Conferi `STORAGE_INFO = 0x64` e `DELETE_FILE = 0x66` (`turing_rev_c.rs:29,33`) contra `protocol-turing-rev-c.md:96,98`, e `HANG_PARTIAL_BYTES = 29_577_216` (`cleanup.rs:23`) contra `:494,734` |
| 5.9 caminhos no core | PASS. Os acertos são de doc e teste anteriores (`discovery.rs`, `reconnect.rs`, `sensor.rs`, `device.rs`) |
| 5.10 comandos síncronos | PASS. `run_plan` e `delete_files` seguem `async` com `blocking`. Os comandos síncronos são os de antes (fontes, preferências, janela, `cancel_job`), e nenhum chega à tela |
| 5.11 supply chain | PASS. `cargo audit` exit 0; nenhuma crate nova; nenhum segredo |

## Situação dos achados do review anterior
| Achado | Situação | Evidência |
|---|---|---|
| **B1** `run_plan` confirmava sozinho | **RESOLVIDO** (`bcd2216`) | `run_plan(ticket, confirmed: bool)` faz `confirm_of(confirmed)` (`commands.rs:665-672`) e passa o `Confirm` por `Backend::run_plan` (`manager/plans.rs:239-263`) até `Manager::run`. Este recusa `Confirm::No` antes de chamar a tela (`app/manager/transfer.rs:111-113`). `bridge.runPlan(ticket, confirmed)` (`bridge.js:248`); a UI só manda `true` depois do OK do diálogo (`ui/storage.js:345`); o demo recusa sem ele (`demo-backend.js:739`). Teste `a_plan_runs_only_with_the_dialogs_confirmation` (`manager/tests.rs:416`): `notConfirmed`, nenhuma chamada à tela, arquivos e catálogo iguais |
| **W1** lote não levava os tamanhos confirmados | **RESOLVIDO** (`6bb8152`) | `delete_files(files: Vec<ConfirmedFileDto>)` com `{path, size}` (`commands.rs:682-690`, `dto.rs:566`). Os tamanhos vão ao core (`plans.rs:69-77`), que só apaga se a tela ainda listar aquele tamanho (`app/manager/cleanup.rs:95-98`). Os dois diálogos passam a lista que mostraram (`ui/manager.js:638,696` → `ui/storage.js:366`). Teste `a_file_changed_since_the_confirmation_is_not_deleted` (`manager/tests.rs:757`): confirmado com 700 B e com 900 B agora → `sourceChanged`, nada apagado, o resto não iniciado; tamanho desconhecido também não apaga. O demo faz igual |
| **W2** studio não restaurava apagados pelo Bezel | **RESOLVIDO** (`f57d8cb`) | A visão geral lista, depois dos sumidos, os apagados que têm cópia (`src-tauri/src/manager.rs:246,273-282`), e `chosen_entries` os aceita (`plans.rs:82-93`). O diálogo os mostra em "Apagados pelo Bezel", desmarcados (`restoreDefaults`, `storage-manager.js:504`; `ui/manager.js:580`). Assim a seleção padrão da D-8 (sumidos e outro cartão) não muda. Teste `files_deleted_through_bezel_are_restorable_on_request` (`manager/tests.rs:444`) e o e2e estendido (`storage-manager.spec.mjs:262-284`, 4 projetos, com axe). Guia en/pt-BR e CHANGELOG atualizados |
| **W3** texto em inglês em vez de códigos | **RESOLVIDO** (`4fd71d3`) | `HaltDto` leva o código do core e o que ele precisa (`dto.rs:440`). Falha e cancelamento levam `stage`. `RunDto` responde `{status: ran \| refused}`, com o `PlanRefusal` por código (`dto.rs:425`). O `halt_error` com `detail` em inglês saiu. A UI traduz por `haltText` (`storage-manager.js:430`), `storage.halt.*` e `storage.report.stage.*` nos dois idiomas. Teste Rust: restaurar que deixou de caber → `refused`/`noSpace` com `{needed, free}` (`manager/tests.rs:646-680`) |
| **W4** fixture `manager` sem teste JS | **RESOLVIDO** (`ebb1eca`) | O fixture ganhou `halts` e `stages` (`messages.rs:389`, `backend-codes.json`). `backend-messages.test.mjs:113` lê a seção inteira: as constantes de `storage-manager.js` são exatamente as do backend; toda chave existe em pt-BR e en com os mesmos `{params}`; recusas e paradas viram frase sem inglês do core nem `{param}` sobrando |
| **W5** SUMMARY desatualizado | **PARCIAL** | CI e lista de arquivos corrigidos (`SUMMARY.md:41-47,55-56`). Restam três pontos (W1 abaixo) |
| Menor: o `kept` de duplicatas podia vir marcado | **RESOLVIDO** (`dae15c5`) | `judge_group` só mantém um arquivo que não começa marcado (`domain/cleanup.rs:289-296`). Arquivos protegidos contam como `Own` desde o início (`:353`) e nunca aparecem. Teste `the_file_a_group_keeps_is_never_prechecked` (`:674`). O porte do demo segue o core (`demo-manager.js`), com teste JS |
| Menor: a CLI dizia que a origem continuava lá | **RESOLVIDO** (`83d9c7f`) | O registro no catálogo depois do apagar virou o estágio `catalog` (`app/manager/transfer.rs:235-237`). A CLI diz que a origem foi apagada e sugere `catalog forget … --yes` (`cli/src/storage/transfer.rs:318-330`). Testes: `a_move_whose_catalog_cannot_follow_says_the_source_is_gone` (`cli/src/storage/tests.rs:1303`) e `a_catalog_that_cannot_follow_the_delete_says_the_source_is_gone` (`storage_manager.rs:677`). O studio diz o mesmo por `storage.report.stage.catalog` |
| Menores informativos (`Manager::named`, concorrência entre CLI e studio) | inalterados | Seguem informativos |

## Blockers
Nenhum.

## Warnings
- **W1. O SUMMARY ainda tem registros desatualizados (resto da W5; gate 6).** Não afeta o código.
  - **Observações:** `SUMMARY.md:71` diz que a seção `manager` do `backend-codes.json` "ainda não é lida por teste
    JS". Desde `ebb1eca` ela é lida (`tests/ui/backend-messages.test.mjs:113`).
  - **Cobertura:** `SUMMARY.md:53-54` mantém 94,38%, `domain/cleanup.rs` 99,76% e studio `manager.rs` 94,03%, de
    antes das correções. Agora são 94,42% (comando do DoD), 99,79% e 94,42%.
  - **Arquivos:** `SUMMARY.md:44-45` cita só `crates/bezel-media/Cargo.toml`. O `crates/bezel-core/Cargo.toml` também
    mudou nesta fase: ganhou a dev-dep `bezel-media` em `8c98774`.
- **Menores (informativo).**
  - **"Restaurar… (N)" conta os apagados.** O contador agora soma os apagados pelo Bezel com cópia
    (`ui/manager.js:362-366`). Por isso o botão aparece em toda coluna onde o Bezel apagou algo, até a cópia sair pelo
    limite de 2 GiB (D-6). A seleção padrão respeita a D-8, e o guia descreve isso. Vale observar na T-8 se incomoda.
  - **Estágio `catalog` no studio.** A frase explica o que houve, mas o studio não tem "esquecer". Na visão seguinte,
    a origem antiga aparece como `missing`, marcada por padrão no Restaurar. Restaurar reenviaria uma duplicata, mas
    nunca apaga nada. É raro: exige falhar a gravação do catálogo logo depois do apagar.
  - **YAGNI:** `Stopped::copied()` e `Stopped::source_deleted()` (`app/manager/report.rs:195,201`) agora só têm uso
    em teste (`storage_manager.rs:703`), porque a CLI passou a casar o `stage`.
  - **Fixture das paradas.** A lista `halts` do teste é escrita à mão (`messages.rs:389-396`), ao contrário de
    `Stage::ALL`. Uma variante nova de `Halt` obriga a mexer no `HaltDto::from` (`match` exaustivo), mas não no
    fixture.
  - **D-9 refinada, não contrariada.** "Todos menos o nome mais curto" virou "todos menos o nome mais curto que não
    começa marcado". Isso pré-marca menos e evita marcar todas as cópias dos mesmos bytes, como pede o motivo da D-9
    ("só sinais exatos"). Não exige D-XX nova.

## Divergências do SUMMARY
- **Números que batem:** 825/0/9 testes, 159 unitários, 160 e2e e 16 no `-g "storage manager"`.
- **O que difere:** cobertura e a linha de Observações (W1).
- **Hardware:** a evidência da CLI na 8.8" (`0.1.0-dev.238`) é anterior às correções.
  - `83d9c7f` mudou onde o catálogo esquece a origem, mas o caminho de sucesso continua preflight → envio →
    conferência → apagar → esquecer. Por isso a evidência segue válida para o M1 da CLI.
  - A parte do studio deve usar um build ≥ `2a4123c`.

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | `OK`. Gate 2: 825 passed, 0 failed, 9 ignored. Windows no CI de `2a4123c`: 761 passed, 0 failed |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | `OK`, TOTAL 94.42% |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | O `[Unreleased]` cobre o gerenciador, e `5108f40` acrescentou o restaurar de apagados e o lote que não apaga arquivo mudado. Evidência sugerida: `## [1.0.0] - <data>` no commit de corte (D-2026-09-30-release-polish-2) |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | README sem mudança desde `5e98c63`; o guia do usuário acompanha o restaurar de apagados (`f57d8cb`). Evidência sugerida: revisão humana do diff do README no PR do corte |
| 6 | Mover/renomear só apaga após cópia verificada; falha/Cancel mantém a origem; restaurar checa antes | CONTEXT | Auto | PASS | `OK` (3 passed, 8 filtered out) |
| 7 | Limpeza na listagem real; nunca boot nem vídeo de tema | CONTEXT | Auto | PASS | `OK` (2 passed, 101 filtered out) |
| 8 | Catálogo × listagens; despejo; bytes exatos ao recarregar | CONTEXT | Auto | PASS | `OK` (core 2 passed, 101 filtered out; `bezel-media` 1 passed, 62 filtered out) |
| 9 | CLI sem `--yes` não muda a tela; `cleanup --dry-run` | CONTEXT | Auto | PASS | `OK` (2 passed, 85 filtered out). Reproduzi com `--fake`: `mv` e `restore sd` sem `--yes` só listam e saem com "… needs --yes"; `cleanup --dry-run` pré-marca só o `pending` e termina com "Dry run: nothing was deleted." |
| 10 | Studio (demo, claro/escuro × pt-BR/en, axe): mover, limpeza | CONTEXT | Auto | PASS | `OK`: i18n 13/13 e "16 passed" (4 specs × 4 projetos), com `BEZEL_E2E_PORT=1443` |
| 11 | 8.8" real: `bezel_test_*` interna → cartão → interna, renomear, restaurar após apagar pelo Bezel; arquivos do usuário intactos | CONTEXT | Manual | MANUAL_REQUIRED | A parte da CLI está no SUMMARY § Hardware validation (ROM 1.90, `0.1.0-dev.238`, 17 arquivos do usuário iguais antes e depois). Falta a parte do studio com um build ≥ `2a4123c`. Agora o studio também restaura um arquivo apagado pelo Bezel, na seção "Apagados pelo Bezel", desmarcado. Evidência sugerida: `/jdi-confirm-dod` com o `ls` antes e depois e a captura da aba |
| 12 | 8.8" real: studio e `cleanup --dry-run` listam `demon_open`, `demon`, `NVI`, `Rani`, `m04`; nada apagado sem confirmar | CONTEXT | Manual | MANUAL_REQUIRED | A parte da CLI está no SUMMARY e bate com o teste da listagem real (5 `variant`, nada pré-marcado). Falta a captura do studio com a confirmação cancelada e o `ls` igual antes e depois |

## Recommendation
**Aprovar, pendente da validação manual do studio na 8.8".**
- **O B1 e as W1–W4 estão resolvidos, com testes.** O studio só roda um plano com a confirmação do diálogo, apaga só no
  tamanho confirmado, restaura apagados pelo Bezel e manda códigos em vez de inglês. O fixture `manager` é lido pelo JS.
  Os dois menores também foram corrigidos.
- **Nenhuma regressão nos gates:**
  - build;
  - 825 testes;
  - 94,48% de cobertura (94,42% pelo DoD);
  - fmt, clippy (inclusive o cruzado para Windows) e `cargo audit`;
  - 160/160 e2e, 159/159 unitários e as 8 linhas Auto do DoD;
  - CI de `2a4123c` verde, inclusive no Windows (761 testes), e release `v0.12.0` publicada.

**Antes do `/jdi-ship`:**
1. **T-8 do studio (M1 e M2):** com o usuário, na 8.8", com a `v0.12.0` (build de `2a4123c`), via `/jdi-confirm-dod`.
2. **W1:** acertar as três linhas do SUMMARY (Observações, cobertura e `crates/bezel-core/Cargo.toml`). É só doc e
   pode ir junto com a evidência da T-8.
