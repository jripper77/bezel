# Phase 8: Gerenciador de armazenamento — Summary  (slug: storage-manager)

**Status:** partial
**Tasks:** 7/8 complete, 0 blocked (T-8: parte da CLI validada na 8.8"; a parte do studio aguarda o usuário)

> `/jdi-do` com o `jdi-doer-bezel` em worktrees, cherry-picked para a `main` (hashes da `main`). T-4 e T-6 entraram
> juntas: a aba nova chama `manager_overview`, que só existe com o backend da T-6.

## Executed tasks
- T-1 `8c98774`: `domain::archive` (catálogo por `ScreenKey`, estados pending/stored/missing/deleted, cartão pela
  capacidade, planos de mover/copiar/renomear/restaurar com pulos e avisos, despejo de cópias de apagados) e
  `domain::cleanup` (só sinais exatos pré-marcados; boot e vídeos de tema nunca sugeridos); porta `ArchiveStore`;
  `MemoryArchive`. Nomes comparados sem caixa (FAT); `device_video_name` foi para o domínio.
- T-2 `3363b25`: `DiskArchive` em `<data>/bezel/storage` (`catalog.json` atômico, inclusive no Windows; cópias por
  SHA-256 conferidas ao ler; miniaturas de 160 px sob demanda).
- T-3 `eb233be`: `app::manager` — inventário, envio registrado (pending antes do 1º byte, stored após conferir), mover/
  renomear/restaurar na ordem preflight → envio → conferência → só então apagar, lote que para na falha ou no Cancel,
  limpeza, associar, cache.
- T-4 `17da237`: aba Armazenamento em largura total (lado a lado, miniaturas, seleção múltipla por teclado, filtros,
  arrastar para mover, renomear, restaurar, assistente de limpeza, associar, cópias locais), demo com o cartão real.
- T-5 `0a32855`: `bezel storage mv|rename|restore|cleanup [--dry-run]|catalog|cache`, `ls` com estado e cópia local;
  `put`/`rm`/`boot` registram; sem `--yes` nada muda na tela; pasta de dados do Windows = `APPDATA` (como o studio).
- T-6 `b85173e`: backend Tauri do contrato da T-4 sobre `app::manager` e `DiskArchive`; o envio existente do studio
  passa pelo `Manager`; `BEZEL_FAKE=1` usa catálogo em memória.
- T-7 `5e98c63`: guia do usuário (en e pt-BR) "Gerenciar os arquivos", README, CHANGELOG; `check-docs.sh` exige
  `bezel storage mv`, `cleanup --dry-run` e `cache clear`.
- Orquestrador: `9a7dd17` (mensagem "desatualizado" serve para planos e envios).

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/domain/{archive,cleanup,media,mod}.rs`, `crates/bezel-core/src/app/{manager,manager/**,
  storage,runtime,mod}.rs`, `crates/bezel-core/src/ports/mod.rs`, `crates/bezel-core/tests/storage_manager.rs`
- `crates/bezel-media/src/{lib.rs,archive/**}`, `crates/bezel-cli/src/{storage.rs,storage/**,theme.rs,main.rs,lib.rs}`,
  `crates/bezel-cli/tests/storage.rs`
- `apps/bezel-studio/src/{app,bridge,demo-backend,demo-data,demo-manager,storage-manager}.js`, `src/ui/{storage,
  manager}.js`, `src/i18n/*`, `src/styles.css`, `apps/bezel-studio/src-tauri/src/{manager,manager/**,storage,
  library,lib,messages,dto,commands,backend,clock,video}.rs`, `build.rs`, `capabilities/default.json`, `tests/**`
- `docs/user/{,pt-BR/}{storage-and-video,README}.md`, `scripts/ci/check-docs.sh`, `README.md`, `CHANGELOG.md`,
  `crates/bezel-media/Cargo.toml`, `Cargo.lock` (`sha2`)
- Fora do PLAN, sinalizados: `domain/media.rs`/`app/runtime.rs` (T-1), `demo-manager.js` (T-4), `ThemeLibrary::videos`,
  `MediaSetup::spare`, `clock.rs` (novo), `video.rs` e `src-tauri/tests/hardware.rs` (T-6)

## Tests
- `cargo test --workspace --locked`: 819 passando, 0 falhando, 9 ignorados (hardware e ffmpeg real)
- UI: 152 unitários; Playwright 160 (claro/escuro × pt-BR/en, axe); DoD `-g "storage manager"` 16 passando
- fmt, clippy `-D warnings` (Linux e `--target x86_64-pc-windows-msvc`), `check-docs.sh`, `check-packaging.sh`
- Coverage (`cargo llvm-cov`): 94,38% de linhas; `domain/archive.rs` 99,82%, `domain/cleanup.rs` 99,76%,
  `app/manager.rs` 98,86%, `archive/disk.rs` 90,91%, studio `manager.rs` 94,03%
- CI: verde no Linux e no Windows (gravação atômica do catálogo testada no runner Windows); releases v0.9.0 (núcleo),
  v0.10.0 (CLI) e v0.11.0 (studio)

## Hardware validation
Turing 8.8" (ROM 1.90), CLI `0.1.0-dev.238`; serviço do usuário parado e religado; só `bezel_test_*`; listagem antes e
depois idêntica (17 arquivos do usuário, mesmos tamanhos); cache local vazio no fim.
- M1: `put` na interna → `mv --to sd` (sem `--yes` só a lista; com `--yes` envia, confere e só então apaga) → volta
  para a interna → `rename` → `rm` pelo Bezel → `restore sd` a partir da cópia local, conferido; `catalog forget` das
  duas entradas removeu a cópia compartilhada.
- M2 (CLI): `cleanup --dry-run` no cartão real: nada pré-marcado, 16 só listados; as 5 reconversões do fabricante
  como `variant` ligadas ao nome que fica (`demon_open`, `demon`, `NVI`, `Rani`, `m04`); `dragon.mp4` nunca sugerido
  porque o tema "Dragon Ball" da biblioteca do studio o usa como fundo; sem `--yes` nada apagado.
- **Pendente (humano):** a aba do studio na 8.8" (mover, renomear, restaurar, abrir a limpeza e cancelar a
  confirmação, miniaturas).

## Observações
- A seção `manager` do `backend-codes.json` ainda não é lida por teste JS.
- O vídeo de tema enviado pelo studio fica catalogado com `source` = cópia temporária em `cache/sending/`.
- Nome dado à tela (D-5) existe só no domínio, sem UI nem comando.
