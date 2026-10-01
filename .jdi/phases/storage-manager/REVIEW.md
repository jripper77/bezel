# Phase 8: Review  (slug: storage-manager)

**Verdict:** BLOCKED

> Verificação em modo `verify`. `HEAD` = `9070217`, árvore limpa (já está em `origin/main`). Os três commits depois de
> `9a7dd17` (`5e98c63`, `e6fed06`, `9070217`) só mexem em `.jdi/`, `CHANGELOG.md`, `README.md`, `docs/user/` e
> `scripts/ci/check-docs.sh`. O código de `crates/` e `apps/` em `HEAD` é o de `9a7dd17`.
>
> - Rodei todos os gates. Nenhum número veio do SUMMARY; onde o meu número difere do dele, digo (seção "Divergências
>   do SUMMARY").
> - **Escopo julgado:** os commits `*(storage-manager)*` de `8c98774` a `9070217` (T-1..T-7 e a correção `9a7dd17`) e
>   as decisões D-2026-09-30-storage-manager-1..14. A T-8 (hardware) entra só como DoD Manual.
> - **Hardware:** nenhum comando abriu a 8.8" real. Não rodei `#[ignore]` nem `BEZEL_HW_TESTS`, não toquei em
>   `/dev/tty*`, hidraw ou systemd. Os comandos `bezel --fake storage …` rodaram com `XDG_DATA_HOME` num diretório
>   temporário (o `--fake` usa catálogo em memória e não criou nada nele). A pasta real
>   `~/.local/share/bezel/storage` não foi tocada: o `mtime` dela é 00:41, antes desta verificação.
> - **Playwright:** rodou com `BEZEL_E2E_PORT=1440`.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked`: exit 0, lock aceito (rustc 1.98.1). O `Cargo.lock` só ganhou `bezel-media` nas dev-deps do core e `sha2` no `bezel-media` (o `sha2` 0.10.9 já estava no lock) |
| Tests | PASS | Linux: **819 passed, 0 failed, 9 ignored**. Os ignorados são 6 de ffmpeg real, 1 de timing, 1 de hardware do studio e 1 de corpus. Bate com o SUMMARY (819/9) e subiu desde o review anterior (749). Windows, no CI de `9a7dd17` (run 36813246114, job `rust-windows`): 28 resultados de teste, **755 passed, 0 failed, 9 ignored**, inclusive os testes do `DiskArchive` (`a_save_replaces_a_catalog_another_reader_holds_open`) |
| Coverage | PASS | **94.45%** lines (TOTAL, sem `main.rs`/`build.rs`), exit 0. Pelo comando do DoD, sem filtro: **94.38%**, igual ao SUMMARY. Detalhe por arquivo abaixo |
| Lint | PASS | `cargo fmt --all --check` e `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0. O clippy cruzado para `x86_64-pc-windows-msvc` (7 crates, `CARGO_TARGET_DIR=target/wincheck`) também sai com 0. Os únicos `allow` fora de teste seguem os 4 de `fps/rtss.rs:247,269,302,323`, com `reason =` |
| Hexagonal/Safety/Protocol/Hygiene | **FAIL** | 5.7 acusa `Confirm::Yes` fora da fronteira humana: `apps/bezel-studio/src-tauri/src/manager/plans.rs:247` (B1). O resto de 5.1–5.11 está limpo (detalhe abaixo). `cargo audit` e `cargo audit --deny warnings`: exit 0 (1277 advisories, 604 crates) |
| Consistency | PASS (com WARN) | Todos os commits têm escopo `storage-manager`. Todos os **Test:** do PLAN existem e passam. O código segue D-1..D-14 (o B1 é regra do gate 5, não de uma D-XX). Há lacunas de paridade e de códigos (W2, W3) e registros do SUMMARY desatualizados (W5) |
| UI Validation | PASS | `npm ci` ok. Comando do gate: **160/160** (4 projetos: claro/escuro × pt-BR/en). `npm run test:unit`: **152/152**, 99.84% de linhas. `i18n.test.mjs`: 13/13, com 732 chaves em cada idioma. Os specs conferem erro de console e axe sério/crítico. As listas são `listbox` com `aria-multiselectable` (setas, Espaço, Shift+setas, Ctrl+A, Delete, F2; `storage-manager.js:283-297`). Há regiões `aria-live` (`ui/storage.js:157,160,532`). As transições novas ficam sob `prefers-reduced-motion: no-preference` (`styles.css:425`) |
| DoD | PASS / PENDING MANUAL | As 5 linhas Auto do CONTEXT e as 3 do PROJECT passam como escritas, e nenhuma passa com filtro vazio (cada uma exige o número exato de testes com `--exact`, ou ≥ 8 no Playwright com `pipefail`). Faltam 2 Manual do CONTEXT (a parte do studio) e 2 do PROJECT. O veredito é BLOCKED só por causa do gate 5 |

**Cobertura dos arquivos da fase (linhas):**
- **core:**
  - `domain/archive.rs` 99.82%, `domain/cleanup.rs` 99.76%, `app/storage.rs` 99.32%;
  - `app/manager.rs` 98.86%, `manager/cleanup.rs` 100%, `manager/ledger.rs` 98.98%, `manager/transfer.rs` 95.80%,
    `manager/copies.rs` 92.74%, `manager/report.rs` 90.54%.
- **mídia:** `archive/disk.rs` 90.91%, `archive/memory.rs` 100%, `archive/mod.rs` 100%, `archive/thumbs.rs` 87.65%.
- **CLI:** `storage.rs` 98.07%, `storage/catalog.rs` 97.95%, `storage/demo.rs` 97.30%, `storage/transfer.rs` 96.21%,
  `storage/cleanup.rs` 90.68%.
- **studio:** `storage.rs` 96.46%, `manager/originals.rs` 95.37%, `manager/plans.rs` 94.65%, `manager.rs` 94.03%,
  `manager/dto.rs` 88.33%.
- **Abaixo de 80%** (nenhum é arquivo novo da fase):
  - a cola do Tauri: `commands.rs` 1.08% e `lib.rs` 30.22%;
  - `bezel-media/src/lib.rs` 48.90%, cuja parte de ffmpeg real só os `#[ignore]` alcançam;
  - `domain/catalog.rs` 66.48%, uma tabela não tocada nesta fase (último commit: `e941130`, release-polish).

### CI
- **Run 36813246114** (`9a7dd17`, o código de `HEAD`): `completed success`. Todos os jobs estão verdes:
  - `rust-linux`, `rust-windows` (15m20s, com o msi e o nsis `Bezel_0.11.0_*`), `node-ui`, CodeQL e `Portao`;
  - a release `v0.11.0` saiu às 04:18Z.
- **Run 36814576833** (`9070217`, só docs): em andamento durante este review. O `node-ui` e as varreduras já estão
  verdes; o `rust-linux` e o `rust-windows` ainda rodavam.

### Detalhe do gate 5
| Check | Resultado |
|---|---|
| 5.1 dependências do core | PASS. Em `[dependencies]` só há `thiserror`. O `bezel-media` entrou só em `[dev-dependencies]` (`crates/bezel-core/Cargo.toml:18`), para os testes de integração |
| 5.2 I/O e threads no core | PASS: nada. O relógio vem do chamador (`now: u64` em `Manager::upload`/`run`/`associate`) |
| 5.3 ports | PASS. Nenhuma impl de porta em `crates/bezel-core/src`; estendi a busca a `ArchiveStore`, `MediaTranscoder` e `ScreenStorage`, também sem nada. A porta nova `ArchiveStore` está em `ports/mod.rs:182-201`, só com tipos do core. A única trait pública nova fora do core é `Pictures` (`src-tauri/src/manager.rs:53`), auxiliar de miniaturas do adapter, sem uso no core |
| 5.4 adapters na composição | PASS. O `DiskArchive::open` só aparece em `main.rs:189` (CLI) e `lib.rs:371` (studio). Fora deles, `MemoryArchive::new()` aparece em `cli/src/storage/demo.rs:119` (o catálogo do `--fake`, escolhido em `main.rs:176-184`) e em `backend.rs:906`/`video.rs:219`, os dois em módulos `#[cfg(test)]` |
| 5.5 `unsafe` | PASS: só os blocos de `fps/rtss.rs`, com `// SAFETY:`, sem mudança |
| 5.6 panics | PASS. Nos arquivos da fase, todo `unwrap`/`expect` fica em `#[cfg(test)]`: `archive.rs` a partir de `:1127`, `cleanup.rs` de `:360`, `memory.rs` de `:85`. Os `doubles.rs` da CLI também são `#[cfg(test)]` (`storage.rs:1264`) |
| 5.7 escrita no dispositivo | **FAIL**. Fora de teste, de doc e dos `match` de `Confirmed::require`, há um `Confirm::Yes` em `manager/plans.rs:247` (B1). Nenhum `*Transport::open` em `crates/*/tests` |
| 5.8 protocolo | PASS. Nenhum arquivo de `bezel-devices` mudou nesta fase, e o laço de testes por encoder não acusa nada |
| 5.9 caminhos no core | PASS. Os acertos são de doc e teste anteriores (`discovery.rs`, `reconnect.rs`, `sensor.rs`, `device.rs`); nenhum em arquivo da fase |
| 5.10 comandos síncronos | PASS. Os 14 comandos novos são `async` e passam por `blocking`. `pick_originals` usa o diálogo bloqueante dentro de um comando `async`, fora da thread principal |
| 5.11 supply chain | PASS. `cargo audit` sai com 0, também com `--deny warnings`. Nenhuma crate nova e nenhum segredo |

### Pontos pedidos pelo orquestrador
- **Segurança dos dados na tela: PASS.**
  - **Ordem dos passos.** `Runner::step` (`app/manager/transfer.rs:219-238`) faz preflight → registro `pending` →
    envio → conferência de tamanho → `settle`. Só então chama `delete_source`.
  - **Falhas.** Uma falha ao gravar o catálogo depois da conferência também para antes de apagar (`:232`).
  - **Cancel.** O cancelamento entre a conferência e o apagamento mantém a origem (`:338`; teste
    `Cancel::AfterVerify`).
  - **Origem mudada.** Uma origem que mudou desde o plano para o lote (`source_unchanged`, `:268-274`).
  - **Restaurar.** Confere de novo espaço e teto antes do 1º byte (`fits`, `:150-172`) e nunca apaga
    (`Transfer::deletes_source`, `archive.rs:658-660`).
  - **Renomear** só a caixa (`NVI.mp4` → `nvi.mp4`), o que num cartão FAT apagaria o próprio arquivo, é recusado
    (`PlanRefusal::SameName`, `archive.rs:1072-1074`). Conferi também com `--fake`.
- **Nada apagado sem confirmar:**
  - **CLI: PASS.** `mv`, `rename`, `restore`, `cleanup`, `rm`, `boot`, `catalog associate|forget` e `cache clear` sem
    `--yes` só consultam e saem com erro. `cleanup --dry-run` conflita com `--yes`.
  - **Core: PASS.** `Manager::run` e `delete_files` recusam `Confirm::No` sem chamar a tela (testado).
  - **Studio: B1.** O `run_plan` não recebe a confirmação. A UI, porém, sempre pergunta antes.
- **Limpeza × D-9: PASS.**
  - **Pré-marcados.** Só `duplicate`, `hangPartial` e `pending` (`domain/cleanup.rs:120`).
  - **Protegidos.** Boot e `device_video_name` nas 4 voltas, nos dois meios, são filtrados antes de qualquer achado
    (`domain/cleanup.rs:344`).
  - **Arquivos do próprio Bezel.** Os verificados não geram achado.
  - **Cartão real.** Na listagem real, os 5 grupos do fornecedor saem como `variant` (tamanhos diferentes), e
    `8.8APEX_2.mp4` não é artefato.
- **Atomicidade e integridade do catálogo: PASS.**
  - **Gravação atômica.** `write_atomically` (`archive/disk.rs`) grava num temporário, faz `sync_all`, fecha e só
    então renomeia. Repete enquanto o Windows disser "ocupado".
  - **SHA-256.** `read` confere o SHA-256 e recusa uma cópia danificada.
  - **Dedup.** `keep` compara os bytes de uma cópia já guardada e regrava se ela estiver danificada.
  - **Catálogo ilegível.** É erro com o caminho, nunca um catálogo vazio.
  - **Ledger.** Guarda a cópia antes de o catálogo nomeá-la e descarta só depois de salvar
    (`app/manager/ledger.rs:1-6`).
- **Windows: PASS.**
  - **CLI.** `data_home_on(Windows)` usa `%APPDATA%` (senão `%USERPROFILE%\AppData\Roaming`), mesmo com `HOME`
    definido (`cli/src/theme.rs:83-91`). O teste roda no runner Windows.
  - **Studio.** Usa o `data_dir()` do Tauri (Roaming AppData), a mesma pasta.

## Blockers
- **B1. O `run_plan` do studio confirma sozinho (gate 5.7: só a fronteira humana confirma uma operação destrutiva).**
  - **Onde:**
    - `apps/bezel-studio/src-tauri/src/manager/plans.rs:247` passa `Confirm::Yes` fixo para `Manager::run`.
    - O comando `run_plan` (`commands.rs:664-674`) só recebe `ticket`, e `bridge.js:226` só envia `{ ticket }`.
  - **Por que é destrutivo:** mover e renomear terminam apagando a origem. Mover e restaurar com `overwrite` substituem
    um arquivo da tela.
  - **Contraste com o resto do projeto:**
    - Os outros comandos destrutivos desta fase recebem `confirmed` e passam por `confirm_of` (`commands.rs:440`):
      `delete_files` (`:678`), `associate_original` (`:738`) e `clear_cache` (`:760`).
    - O envio que já existia faz o mesmo: `Backend::run_upload(ticket, overwrite: Confirm, …)`
      (`storage.rs:605-611`).
    - O próprio módulo diz que o `Confirm` é algo "which only the Tauri commands make" (`storage.rs:17-20`).
    - Com o `Confirm::Yes` fixo, o caminho `Confirm::No` de `Manager::run` (testado no core) fica inalcançável pelo
      studio. Nenhum teste do backend prova que um `run_plan` sem confirmação deixa a tela intacta.
  - **Risco hoje:** baixo.
    - A UI só chama `runPlan` depois do OK do diálogo que lista origem → destino (`ui/manager.js:514`).
    - Um ticket só existe depois de um `plan_*`.
    - O bloqueio vem da regra do gate e da invariante do projeto, não de um apagamento observado.
  - **Correção (pequena):**
    - `run_plan(ticket, confirmed: bool)` em `commands.rs`, com `confirm_of(confirmed)`.
    - `Backend::run_plan(ticket, confirm, …)` repassa o `Confirm` a `Manager::run`.
    - `bridge.runPlan(ticket, confirmed)`, com `true` vindo do diálogo.
    - O demo recusa sem confirmação, como já faz em `deleteFiles` (`demo-backend.js:721`).
    - Um teste em `manager/tests.rs`: com `Confirm::No`, nenhuma chamada de escrita à tela e o erro `notConfirmed`.

## Warnings
- **W1. A exclusão em lote do studio não leva os tamanhos que o usuário confirmou (D-9, D-7).**
  - **O que acontece:**
    - Os diálogos da limpeza e de "Apagar" com seleção múltipla mostram cada arquivo com o tamanho
      (`ui/manager.js:612-623`, `:668-681`). Mas `runDeletes` envia só os caminhos (`ui/storage.js:355`,
      `bridge.js:228`).
    - O backend lista a tela de novo e usa o tamanho de agora como "o confirmado" (`manager/plans.rs:315-325`).
    - Assim a guarda do core contra "mudou desde a confirmação" (`app/manager/cleanup.rs:96`, `sourceChanged`) nunca
      dispara no studio.
  - **CLI:** está certa, porque lista, mostra e apaga no mesmo processo.
  - **Correção:** enviar `{ path, size }` da lista confirmada.
- **W2. O studio não restaura um arquivo apagado pelo Bezel; a CLI restaura (paridade D-4/D-12, motivo da D-6).**
  - **No studio:**
    - `chosen_entries` descarta entradas `Deleted` (`manager/plans.rs:75-78`).
    - A lista "Restaurar…" só tem `missing` e "em outro cartão" (`src-tauri/src/manager.rs:242-243`;
      `archive.rs:363-381`).
  - **Na CLI:** `restore internal NOME` aceita "also ones deleted through Bezel" (`cli/src/storage/transfer.rs:80`,
    `docs/user/storage-and-video.md:235`).
  - **Por que importa:** a D-6 guarda as cópias de apagados justamente para isso.
  - **O que não viola:** a D-8 não exige isso do studio, e o guia descreve o studio como está (`:228-230`).
  - **Para a T-8:** no studio, teste "restaurar" com um arquivo sumido (apagado fora do Bezel ou de outro cartão), não
    com um apagado pelo próprio Bezel.
- **W3. Motivos novos com código estável chegam ao studio como texto em inglês (D-13; D-2026-09-30-release-polish-6).**
  - **Onde:**
    - `halt_error` (`src-tauri/src/manager/dto.rs:463-473`) manda `sourceChanged` e `noLocalCopy` como
      `invalidInput`. O `detail` é a frase em inglês do core, então o pt-BR mostra "O Bezel não pôde usar isto: the
      file is gone or changed since the list was made".
    - A recusa de um restaurar que deixou de caber vai como `refused` com `detail` = `PlanRefusal` em inglês
      (`manager/plans.rs:260`).
  - **O que já existe:** o core já dá os códigos (`Halt::code`, `report.rs:122`; `PlanRefusal::code`,
    `archive.rs:807`), e a UI já tem `storage.plan.refused.*` (por exemplo `en.js:557`).
  - **Relação com o backlog:** o todo `english-error-details` cobre o `detail` genérico. Estes casos são novos e têm
    código pronto.
- **W4. O fixture de códigos do gerenciador não é lido por teste JS.**
  - **O que está fora:**
    - A seção `manager` do `backend-codes.json` (`:251-293`) é gerada e conferida só pelo Rust (`messages.rs:369-378`).
      Nenhum `*.test.mjs` a lê; o próprio SUMMARY registra isso (`SUMMARY.md:66`).
    - Faltam no fixture os motivos de parada (`cancelled`, `sourceChanged`, `conflict`, `noLocalCopy`, `refused`,
      `failed`) e os estágios (`preflight`, `upload`, `verify`, `delete`).
  - **O que conferi à mão:** toda chave `storage.finding*`, `storage.skip.*`, `storage.plan.*`,
    `storage.report.done.*`, `storage.job.*`, `storage.warning.*` e `storage.state.*` existe nos dois idiomas. A
    exceção é `storage.state.deleted`, que hoje é inalcançável, porque nenhuma entrada apagada chega à UI.
- **W5. Registros do SUMMARY desatualizados (gate 6).**
  - **CI:** `SUMMARY.md:51` cita a v0.9.0. Já saíram a v0.10.0 e a v0.11.0, e o run 36813246114 de `9a7dd17` está
    verde, com 755 testes no Windows.
  - **Arquivos:** `SUMMARY.md:37-39` lista `studio.rs`, que não mudou nesta fase. Omite `src-tauri/src/clock.rs`
    (novo), `video.rs` e `src-tauri/tests/hardware.rs` (T-6, fora do PLAN e não sinalizados em `:42-43`). Também cita
    `Cargo.toml` (`:41`), mas só `crates/bezel-{core,media}/Cargo.toml` mudaram; o da raiz não.
- **Menores (informativo).**
  - **Limpeza:** o `kept` de um grupo de duplicatas pode ser ele mesmo pré-marcado por outro motivo (`pending` ou
    parcial de 29.577.216 B; `domain/cleanup.rs:279-302`). Nesse caso, a lista padrão apagaria as duas cópias dos
    mesmos bytes. É raro, e a confirmação lista os dois.
  - **Texto da CLI:** se o catálogo não puder ser gravado logo depois de apagar a origem
    (`app/manager/transfer.rs:340-341`), a CLI diz que a origem "is still there too"
    (`cli/src/storage/transfer.rs:338`), mas ela já foi apagada.
  - **YAGNI:** `Manager::named` (`app/manager.rs:146`) não tem uso fora de teste. A D-5 pede a chave, e o PLAN registra
    isso.
  - **Concorrência:** CLI e studio editam o mesmo `catalog.json` com carregar-mudar-salvar sem trava entre processos. A
    porta da tela serializa as operações nela; `cache` e `catalog` não passam por essa porta. A janela é curta.

## Divergências do SUMMARY
- **Números:** os meus batem com os do SUMMARY.
  - 819/0/9 testes, 152 unitários, 160 e2e e 16 no `-g "storage manager"`.
  - 94,38% pelo comando do DoD.
  - Por arquivo: `archive.rs` 99,82%, `cleanup.rs` 99,76%, `app/manager.rs` 98,86%, `disk.rs` 90,91% e studio
    `manager.rs` 94,03%.
- **O que difere:** a linha de CI e a lista de arquivos (W5).

## DoD Checklist (gate 8)
| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | Gate 2: 819 passed, 0 failed, 9 ignored. Windows no CI de `9a7dd17`: 755 passed, 0 failed |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | Comando do DoD: TOTAL 94.38%, `OK` |
| 3 | No `TODO`/`FIXME` without issue | PROJECT | Auto | PASS | `OK` |
| 4 | CHANGELOG.md updated per release | PROJECT | Manual | MANUAL_REQUIRED | O `[Unreleased]` cobre o gerenciador, e o `check-docs.sh` exige `bezel storage mv` nele. Evidência sugerida: `## [1.0.0] - <data>` no commit de corte (D-2026-09-30-release-polish-2) |
| 5 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | O README ganhou o gerenciador no studio e na CLI (`5e98c63`). Evidência sugerida: revisão humana do diff do README no PR do corte |
| 6 | Mover/renomear só apaga após cópia verificada; falha/Cancel mantém a origem; restaurar checa antes | CONTEXT | Auto | PASS | `OK` (3 passed, 7 filtered out) |
| 7 | Limpeza na listagem real; nunca boot nem vídeo de tema | CONTEXT | Auto | PASS | `OK` (2 passed, 100 filtered out) |
| 8 | Catálogo × listagens; despejo; bytes exatos ao recarregar | CONTEXT | Auto | PASS | `OK` (core 2 passed; `bezel-media` 1 passed, 62 filtered out) |
| 9 | CLI sem `--yes` não muda a tela; `cleanup --dry-run` | CONTEXT | Auto | PASS | `OK` (2 passed, 84 filtered out). Reproduzi com `--fake`: `mv` sem `--yes` só lista e sai com "moving 1 file needs --yes" |
| 10 | Studio (demo, claro/escuro × pt-BR/en, axe): mover, limpeza | CONTEXT | Auto | PASS | `OK`: i18n 13/13 e "16 passed" (4 specs × 4 projetos), com `BEZEL_E2E_PORT=1440` |
| 11 | 8.8" real: `bezel_test_*` interna → cartão → interna, renomear, restaurar após apagar pelo Bezel; arquivos do usuário intactos | CONTEXT | Manual | MANUAL_REQUIRED | SUMMARY § Hardware validation: passe pela CLI na ROM 1.90 (`0.1.0-dev.238`), com 17 arquivos do usuário iguais antes e depois. O código bate com a ordem relatada (preflight → envio → conferência → apagar). Falta o humano confirmar e fazer a parte do studio. Evidência sugerida: `/jdi-confirm-dod` com o `ls` antes e depois e a captura da aba (ver W2 para o restaurar no studio) |
| 12 | 8.8" real: studio e `cleanup --dry-run` listam `demon_open`, `demon`, `NVI`, `Rani`, `m04`; nada apagado sem confirmar | CONTEXT | Manual | MANUAL_REQUIRED | A parte da CLI está no SUMMARY e bate com o teste da listagem real (5 `variant`, nada pré-marcado). Falta a captura do studio com a confirmação cancelada e o `ls` igual antes e depois |

## Recommendation
**Bloquear até corrigir o B1; o resto está pronto.**
- **O que passa:**
  - build;
  - 819 testes no Linux e 755 no Windows (CI de `9a7dd17` verde, v0.11.0 com msi e nsis);
  - 94,45% de cobertura (94,38% pelo DoD);
  - fmt e clippy, inclusive o cruzado para Windows, e `cargo audit`;
  - 160/160 e2e, 152/152 unitários e as 8 linhas Auto do DoD.
- **O código da fase segue D-1..D-14.** O núcleo de segurança está certo e testado: copiar, conferir e só então apagar;
  o lote para; restaurar confere antes do 1º byte; a limpeza só pré-marca sinais exatos.
- **O único bloqueio:** o `Confirm::Yes` fixo no backend do studio (gate 5.7).

**Para destravar (uma tarefa curta, depois `/jdi-verify storage-manager`):**
1. **B1:** levar `confirmed` do diálogo até `Manager::run` (comando, `Backend::run_plan`, `bridge.runPlan`, demo) e
   cobrir `Confirm::No` com um teste do backend.

**Recomendado na mesma passada (sem novo bloqueio):**
2. **W1:** enviar os tamanhos confirmados no `delete_files`.
3. **W3:** mandar `sourceChanged`, `noLocalCopy` e `PlanRefusal` como códigos que a UI traduz.
4. **W4:** ler a seção `manager` do fixture num teste JS, com motivos de parada e estágios.
5. **W5:** atualizar CI e arquivos no SUMMARY.
6. **W2:** decidir se o studio também restaura apagados pelo Bezel (ou registrar a diferença numa D-XX).

**Depois:** a parte do studio da T-8 com o usuário (M1 e M2 na 8.8"), via `/jdi-confirm-dod`.
