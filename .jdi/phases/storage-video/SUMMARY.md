# Phase 6: Armazenamento e vídeo — Summary  (slug: storage-video)

**Status:** partial
**Tasks:** 1/8 complete, 0 blocked (T-6.2..T-6.8 pendentes)

> Nota de processo: T-6.1 executada pelo `jdi-doer-bezel` no worktree `wt/sv-core` (a partir de `main`),
> cherry-picked para a `main`.

## Executed tasks
- T-6.1: domínio, portas e casos de uso de armazenamento, jobs e mídia no core — `5e36061`
  - Porta `ScreenStorage` (`info`, `list`, `size`, `upload`, `delete`, `play_video`, `play_image`, `stop`,
    `set_start_mode`) via `ScreenLink::storage() -> Option<&mut dyn ScreenStorage>`, default `None`; famílias
    sem armazenamento não mudaram e `app::storage` devolve `BezelError::Unsupported`.
  - Domínio (`domain/storage.rs`): `Medium` (`internal`/`sd`), `StorageLocation` (só as 4 raízes), `FileName`
    (listado: leniente; upload: `[a-z0-9_.-]` minúsculo, sem ponto inicial, ≤ 208 bytes = 236 − 28 da raiz
    mais longa), `RemotePath` (`internal/video/88.mp4`), `StorageInfo`/`Capacity` em bytes (reserva já
    descontada pelo adapter), `FileEntry`, `Repeat`, `StartMode`, `BootMedia`, `Operation` e a prova
    `Confirmed` (só nasce de `Confirm::Yes`; `delete` e `set_start_mode` da porta exigem-na).
  - `preflight` (puro): nome → tipo da pasta → perfil (converte vídeo se houver conversor, senão
    `NeedsConverter`) → extensão → 120 MB (decimal) / teto de 2 GiB → cartão → espaço. Sem espaço:
    `Refusal::NoSpace` com candidatos do mesmo meio, maiores primeiro; nunca apaga nada.
  - Mídia (`domain/media.rs`): `MediaInfo`/`VideoTrack`, `UploadProfile::for_model` (rev C: MP4 H.264
    yuv420p sem áudio na resolução nativa — 480x1920 na 8.8"; TUR_USB: H.264 Annex-B sem B-frames; imagens
    jpg/jpeg/png/bmp/gif), `Mismatch`, `ConvertOptions`, `TranscodeTarget`, `StreamSpec`, `MediaTools`.
  - Jobs (`domain/job.rs`): `Progress { phase: Convert|Upload|Verify, done, total }`, `CancelToken`, `Job`.
  - Casos de uso (`app/storage.rs`): `info`, `list`, `prepare_upload` (só consultas), `upload` (converter →
    reconferir → enviar → verificar; sobrescrever exige `Confirm::Yes` antes de qualquer chamada), `delete`,
    `play`, `stop`, `set_boot_media` (tocar + startMode sob um único `Confirm`), `suggest_name`.
  - Porta `MediaTranscoder` (`tools`, `probe`, `transcode`, `load`, `stream`) e `VideoFrames::frame_at`;
    `RenderContext.backdrop` (`Poster` | `OnDevice` | `Frame(&Frame)`), `Poster` em todos os construtores.
  - `BezelError`: `Unsupported`, `Cancelled { partial }`, `NotConfirmed`, `Refused(Refusal)`.

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/domain/{storage,job,media,mod,error}.rs`, `crates/bezel-core/src/ports/mod.rs`,
  `crates/bezel-core/src/app/{mod,storage,runtime}.rs`
- só o campo `backdrop`: `crates/bezel-render/src/testkit.rs`, `crates/bezel-render/tests/support/showcase.rs`,
  `crates/bezel-cli/src/screen.rs`, `apps/bezel-studio/src-tauri/src/studio.rs`

## Tests
- `cargo test --workspace --locked` na `main` após o cherry-pick: 385 passando, 0 falhando, 2 ignorados;
  `bezel-core --lib`: 62 (24 novos)
- DoD: `domain::storage::tests::destructive_operations_require_confirm_yes` e
  `domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage` → OK
- Coverage (`cargo llvm-cov -p bezel-core`): `app/storage.rs` 99,42%, `domain/storage.rs` 99,31%,
  `domain/media.rs` 97,71%, `domain/job.rs` e `domain/error.rs` 100%; bezel-core 94,19%

## Hardware validation
- não aplicável (T-6.1 é só core)

## Observações para T-6.2..T-6.7
- Testes unitários do core usam dublês gravadores em `app::storage::doubles` (`#[cfg(test)]`): o core não
  linka os fakes de `bezel-devices` em testes `--lib`. Após a T-6.2, adicionar um teste de integração em
  `crates/bezel-core/tests/` sobre o armazenamento do `FakeConnector`.
- O caminho de cada raiz no dispositivo é do adapter (rev C grande `/mnt/UDISK/...`, pequena `/root/...`,
  cartão `/mnt/SDCARD/...`; TUR_USB `/usr/data/...`, `/tmp/sdcard/mmcblk0p1/...`).
