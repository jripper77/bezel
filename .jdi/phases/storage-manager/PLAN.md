# Phase 8: Gerenciador de armazenamento — Plan  (slug: storage-manager)

## Goal
Gerenciar os arquivos da memória interna e do cartão SD da tela no studio e na CLI: visão lado a lado com miniaturas, seleção múltipla, ordenar e filtrar, mover e renomear por reenvio da cópia local, catálogo do que foi enviado, assistente de limpeza (duplicados, sobras, não usados) e restaurar um cartão.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-storage-manager-1..14; herdadas: D-1 (hexagonal), storage-video-1/-7, release-polish-6/-10/-12/-13

## Tasks

### Wave 1

#### T-1: Domínio do catálogo e da limpeza, porta `ArchiveStore` e fake em memória
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{archive,cleanup,mod}.rs`, `crates/bezel-core/src/ports/mod.rs`, `crates/bezel-core/Cargo.toml` (dev-dep `bezel-media`), `crates/bezel-media/{Cargo.toml,src/lib.rs,src/archive/{mod,memory}.rs}` (`sha2` 0.10), `Cargo.lock`
- **Acceptance:**
  - `domain::archive` puro (D-5/6/8/10): `ScreenKey { model, name: Option }`, `ContentId` (SHA-256 hex), entrada por meio+pasta+nome com tamanho, origem, tipo, duração/resolução, `sent_at` (valor vindo do adapter) e `EntryState::{Pending,Stored,Missing,Deleted}`; cartão pela capacidade (outra = "em outro cartão"); boot registrado; limite padrão 2 GiB; `reconcile` (listagens → stored/missing), `evict` (só cópias de apagados, mais antiga primeiro), ranking de candidatos para associar (tamanho exato + tipo, depois nome/duração/resolução); planos de mover/copiar/renomear/restaurar com pulos `Conflict`/`NoLocalCopy`/`DeleteUnsupported` (TUR_USB) e avisos `BootMedia`/`ThemeVideo`; nomes por `FileName::for_upload`, mesma extensão (`NVI.mp4` → `nvi.mp4`); restaurar recusa soma > livre ou arquivo > teto antes do 1º byte.
  - `domain::cleanup::findings` puro (D-9): códigos estáveis `duplicate`/`hangPartial`/`pending` (pré-marcados) e `variant`/`sizeDiffers`/`sameSize`/`unused` (só listados); protegidos (boot + `device_video_name` nas 4 voltas) nunca aparecem; teste com a listagem real da 8.8" (nomes e tamanhos de `bezel storage ls --json`, dados pelo orquestrador): pares do fornecedor e o parcial sim, `8.8APEX_2.mp4` não.
  - Porta `ArchiveStore` (`load`, `save` atômico, `keep(bytes) -> ContentId` com bytes iguais guardados uma vez, `read`, `discard`), só tipos do core; `bezel_media::archive::{content_id, MemoryArchive}` = fake do adapter para os testes de core, CLI e studio. Nenhuma impl de porta em `crates/bezel-core/src`.
- **Dependencies:** none
- **Test:** `domain::archive::tests::{entries_match_listings_per_screen_and_card,eviction_drops_only_copies_of_deleted_files}`, `domain::cleanup::tests::{finds_the_users_vendor_duplicates_and_the_hang_partial,never_suggests_the_boot_media_or_a_theme_video}`, `archive::memory::tests::same_bytes_are_kept_once`
- **Status:** completed

### Wave 2 (paralela)

#### T-2: Store em disco `<data>/bezel/storage/` e miniaturas
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-media/src/{lib.rs,archive/{mod,disk,thumbs,tests}.rs}`
- **Acceptance:**
  - `DiskArchive::open(root)`: `catalog.json` versionado (DTOs serde no adapter) gravado por temporário + rename, `files/<sha256>.<ext>`, `thumbs/<sha256>.png`; recarregar devolve catálogo e bytes idênticos; arquivo corrompido = erro com o caminho, nunca pânico nem catálogo zerado.
  - Miniaturas sob demanda: imagem cabendo em 160 px; vídeo = quadro em 1 s pelo ffmpeg (reusa `poster`); sem ffmpeg = `None`. Testes só com `Path`/tempdir (passam no Windows).
- **Dependencies:** T-1
- **Test:** `archive::tests::copies_are_the_exact_bytes_sent_and_survive_a_reload`, `archive::tests::thumbnails_fit_160_px_and_videos_need_ffmpeg`
- **Status:** completed

#### T-3: Casos de uso do gerenciador no core
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/app/{mod,storage}.rs`, `crates/bezel-core/src/app/manager{.rs,/**}`, `crates/bezel-core/tests/storage_manager.rs`
- **Acceptance:**
  - `app::manager` sobre `ScreenLink` + `ArchiveStore` + `MediaTranscoder`: envio registrado (`pending` antes do 1º byte com os bytes exatos enviados, `stored` após conferir); apagar, boot e visão geral (reconcile, tamanho do catálogo ou desconhecido no TUR_USB) atualizam o catálogo, recarregado e gravado a cada passo; `app::storage::upload` mantém a assinatura.
  - Mover/copiar/renomear um a um: preflight no destino → envia a cópia → tamanho = catálogo → só então apaga a origem; lote para na 1ª falha ou Cancel (origem fica; relatório movidos/falhou com motivo/não iniciados). Restaurar: espaço e teto antes do 1º byte, mesmo nome+tamanho pula, conflito só confirmado, nunca apaga. Limpeza apaga só a lista confirmada, um a um; associar, esquecer, cache (info; limpar só cópias de apagados, todas com `--all`, mantém entradas e miniaturas como "sem cópia local"; limite).
  - `Confirm::No` = zero escritas na porta; testes com `FakeStorage` + `MemoryArchive`.
- **Dependencies:** T-1
- **Test:** `--test storage_manager`: `move_deletes_the_source_only_after_the_copy_is_verified`, `a_failed_or_cancelled_move_keeps_the_source_and_stops_the_batch`, `restore_checks_space_and_the_cap_before_sending_anything`, `uploads_are_recorded_pending_then_stored`
- **Status:** completed

#### T-4: Studio UI: aba Armazenamento em largura total (demo)
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src/{index.html,styles.css,app.js,bridge.js,demo-backend.js,demo-data.js,storage-manager.js}`, `apps/bezel-studio/src/ui/{storage,manager}.js`, `apps/bezel-studio/src/i18n/{en,pt-BR}.js`, `apps/bezel-studio/tests/e2e/{storage,storage-manager}.spec.mjs`, `apps/bezel-studio/tests/ui/{storage-manager,demo-backend,bridge}.test.mjs`
- **Acceptance:**
  - Interna × cartão como listboxes multi-seleção (setas, Espaço, Shift+setas, Ctrl+A), miniatura ou ícone + "Tocar na tela", ordenar/filtrar (D-13), "Mover para o outro lado" = arrastar, renomear, restaurar, limpeza (pré-marcados, confirmação com lista exata e espaço liberado), associar, cache; TUR_USB desabilitado com motivo; aria-live; lógica pura em `storage-manager.js` (cobertura ≥ 80%).
  - Contrato: comandos novos em `bridge.js` com JSDoc dos DTOs, códigos de motivo da T-1; cenário demo `vendorCard` com a listagem do teste da T-1 ao lado de uma interna enviada pelo Bezel; o demo segura o job até o teste liberar (sem aumentar tempos); i18n com paridade, nenhum literal no JS.
- **Dependencies:** T-1
- **Test:** Playwright "storage manager moves files by drag and by keyboard after confirming" e "storage manager cleanup lists the vendor duplicates and deletes only what was confirmed" (4 projetos, axe); `storage.spec.mjs` verde; `node --test tests/ui/i18n.test.mjs tests/ui/storage-manager.test.mjs`
- **Status:** completed

### Wave 3 (paralela)

#### T-5: CLI `bezel storage` do gerenciador
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-cli/src/{storage.rs,storage/**,main.rs,lib.rs,theme.rs}`, `crates/bezel-cli/tests/storage.rs`
- **Acceptance:**
  - `mv PATH... --to internal|sd`, `rename`, `restore internal|sd [NAME...]`, `cleanup [--dry-run]`, `catalog [ls|associate|forget]`, `cache [info|clear [--all]|--limit SIZE]`; `ls` com cópia local e estado; `put`/`rm`/`boot` registram (D-12).
  - Imprime a lista exata (origem -> destino, tamanhos, o que é apagado); sem `--yes` só consulta a tela; `--dry-run` nunca apaga; Ctrl+C cancela como `put`; textos em inglês. `DiskArchive` em `data_home()/bezel/storage`; `--fake` com `MemoryArchive`; protegidos = boot + vídeos dos temas em `bezel/themes`; testes com `XDG_DATA_HOME` temporário.
- **Dependencies:** T-2, T-3
- **Test:** `storage::tests::mv_rename_restore_and_cleanup_without_yes_change_nothing`, `storage::tests::cleanup_dry_run_lists_without_deleting`; `tests/storage.rs` com `--fake`
- **Status:** completed

#### T-6: Studio backend (Tauri) do gerenciador
- **Specialist:** jdi-doer-bezel
- **Files modified:** `apps/bezel-studio/src-tauri/src/{storage.rs,storage/**,manager.rs,manager/**,dto.rs,commands.rs,lib.rs,messages.rs,backend.rs,library.rs,studio.rs}`, `apps/bezel-studio/src-tauri/{build.rs,capabilities/default.json}`, `apps/bezel-studio/tests/ui/fixtures/backend-codes.json`, `apps/bezel-studio/src/i18n/{en,pt-BR}.js` (só códigos novos)
- **Acceptance:**
  - Comandos com os nomes e DTOs do `bridge.js` da T-4 sobre `app::manager`; `DiskArchive` em `data_dir()/bezel/storage`; enviar, apagar, boot e vídeo do tema registram; vídeos da biblioteca e do tema ao vivo protegidos; miniaturas fora da thread da UI como data URL; `allow-*` por comando.
  - Resultados e erros por código com args (sem parsing de texto); fixture e traduções em dia.
- **Dependencies:** T-2, T-3, T-4
- **Test:** `storage::tests::uploads_deletes_and_the_boot_media_are_recorded`, `manager::tests::a_move_reports_by_code`; teste do fixture em `messages.rs`; `npm test`
- **Status:** pending

### Wave 4

#### T-7: Documentação de usuário, README e CHANGELOG
- **Specialist:** jdi-doer-bezel
- **Files modified:** `docs/user/{storage-and-video,README}.md`, `docs/user/pt-BR/{storage-and-video,README}.md`, `scripts/ci/check-docs.sh`, `README.md`, `CHANGELOG.md`
- **Acceptance:** en e pt-BR: cópias locais e onde ficam, limite e "Limpar cache", mover/renomear/restaurar (só apaga após conferir), limpeza (nada sem confirmar), TUR_USB; `check-docs.sh` exige `bezel storage mv`, `cleanup --dry-run`, `cache clear`; CHANGELOG `[Unreleased]`.
- **Dependencies:** T-5, T-6
- **Test:** `bash scripts/ci/check-docs.sh`
- **Status:** pending

### Wave 5

#### T-8: HARDWARE — 8.8" real (orquestrador)
- **Specialist:** orquestrador na Turing 8.8" real
- **Files modified:** `.jdi/phases/storage-manager/SUMMARY.md` (§ Hardware validation)
- **Acceptance:**
  - `scripts/install-local.sh`; serviço do usuário parado e religado no fim; só `bezel_test_*` é criado, movido ou apagado; `ls` antes/depois iguais fora deles.
  - M1: `bezel_test_*` interna → cartão → interna, `rename`, `rm --yes` no cartão e `restore sd --yes`, tamanhos conferidos. M2: studio e `cleanup --dry-run` listam `demon_open`, `demon`, `NVI`, `Rani`, `m04`; confirmação cancelada; nada apagado.
- **Dependencies:** T-7
- **Test:** DoD Manual M1 e M2 (`/jdi-confirm-dod`)
- **Status:** pending

## Execution
- 8 tasks em 5 waves (1 → 3 → 2 → 1 → 1)

## DoD → task
| DoD (CONTEXT) | Task |
|---|---|
| mover só apaga após cópia verificada; falha/Cancel; restaurar checa antes | T-3 |
| limpeza na listagem real; nunca boot nem vídeo de tema | T-1 |
| catálogo × listagens, despejo; store com bytes exatos | T-1 (`domain::archive`), T-2 (`archive::tests`) |
| CLI sem `--yes` não muda a tela; `cleanup --dry-run` | T-5 |
| studio `-g "storage manager"` ≥ 8 + i18n | T-4 (backend real: T-6) |
| Manual M1, M2 | T-8 |
| PROJECT: testes, cobertura, TODO, CHANGELOG/README | todas; T-7 |

## Test requirements
- `cargo test --workspace --locked`; `cargo fmt --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`
- Windows: `cargo clippy --workspace --exclude bezel-studio --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings`
- `cd apps/bezel-studio && npm test`; `bash scripts/ci/check-docs.sh`; `cargo llvm-cov --workspace --locked --fail-under-lines 80`
- `scripts/install-local.sh` antes da T-8

## Notes
- Nenhuma variante nova em `BezelError`/`Refusal` (studio e CLI casam esses enums): motivos do gerenciador são tipos novos. Nomes de teste do DoD exatos (`--exact`), sem módulo a mais no caminho.
- Donos únicos: `Cargo.lock`, `ports/mod.rs`, `domain/mod.rs` = T-1 (W1); i18n e `demo-backend.js` = T-4 (W2), depois T-6 (W3); docs/README/CHANGELOG = T-7. Porta fixa após a T-1.
- Nome do usuário para a tela: chave pronta no domínio, sem comando nem UI (D-12/D-13 não pedem); todo se o usuário quiser.
- No hardware o parcial apagado segue contando como usado até reiniciar (§ 19).
