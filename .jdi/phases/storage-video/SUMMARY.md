# Phase 6: Armazenamento e vídeo — Summary  (slug: storage-video)

**Status:** partial
**Tasks:** 3/8 complete, 0 blocked (T-6.3, T-6.5..T-6.8 pendentes)

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

- T-6.2: rev C — comandos de armazenamento, upload com progresso e cancelamento, boot e fake — `4dba6cc`
  - Protocolo: encoders iguais aos vetores do § 17.2 para 0x64, 0x65, 0x66, 0x6E, 0x78 (loop), 0x8C e 0x6F;
    `StorageReport` (KiB → bytes, −512 KiB da flash, cartão só com TF total > 1024 KiB); raízes por classe de
    tela (grande `/mnt/UDISK/{img,video}`, pequena `/root/...`, cartão `/mnt/SDCARD/...`).
  - Driver: `ScreenStorage` completo; upload na sequência do § 13.4 (STOP_VIDEO, STOP_MEDIA, LIST_DIR, 0x6F até
    `create_success`, blocos 249+1 agrupados por escrita, espera de `file_rev_done`), progresso por escrita,
    cancelamento entre escritas e durante a espera (HELLO + GET_FILE_SIZE → `Cancelled{partial}`, nunca apaga);
    boot reescreve 0x7D com o último brilho enviado; após tocar/parar/enviar, o próximo `present` manda
    PRE_UPDATE_BITMAP + frame cheio (§ 7.2); alfa preservado; nenhum comando de armazenamento, 0x7D, 0x81,
    0x82 ou 0x84 sai implicitamente (teste).
  - `driver/mod.rs`: `StorageRoots`, `parse_listing`, `upload_size`, `send_in_chunks` (para a T-6.5).
  - `FakeStorage` em memória no `FakeConnector` (rev C e TUR_USB), com o log das chamadas; teste de
    integração `crates/bezel-core/tests/storage.rs` com os casos de uso sobre ele.
- T-6.4: fundo em vídeo com base transparente e runtime — `3957f14`
  - Renderer: `Backdrop::OnDevice` (base A=0 fora dos elementos, alfa reto), `Backdrop::Frame` (frame do PC
    cobrindo o canvas), `Poster` como antes; os 53 goldens existentes idênticos byte a byte; goldens novos
    `bg_video_on_device`, `bg_video_host`, `bg_video_host_cover`.
  - `ThemeRuntime::start_video`: com armazenamento e reprodução, `size` do vídeo em `internal/video` e
    `sd/video` → `play_video(Loop)` e `OnDevice`, senão pôster + `VideoMissing` (com o pedido de upload pronto);
    sem reprodução, `Host` via `MediaTranscoder::stream`, `NoConverter` ou `NoPlayback`; nunca envia sozinho.
    Nome no dispositivo: `device_video_name` (asset + sufixo de giro + extensão do perfil).
  - Fora do `files_modified`: `crates/bezel-core/src/app/mod.rs` (reexportações).

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/domain/{storage,job,media,mod,error}.rs`, `crates/bezel-core/src/ports/mod.rs`,
  `crates/bezel-core/src/app/{mod,storage,runtime}.rs`
- só o campo `backdrop`: `crates/bezel-render/src/testkit.rs`, `crates/bezel-render/tests/support/showcase.rs`,
  `crates/bezel-cli/src/screen.rs`, `apps/bezel-studio/src-tauri/src/studio.rs`

## Tests
- `cargo test --workspace --locked` na `main` após T-6.1, T-6.4 e T-6.2: 416 passando, 0 falhando, 2 ignorados
- DoD: `domain::storage::tests::destructive_operations_require_confirm_yes` e
  `domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage` → OK
- DoD T-6.2: `protocol::turing_rev_c::tests::storage_packets_match_the_reference_vectors`,
  `...::storage_info_subtracts_the_reserved_flash_and_detects_the_card`,
  `driver::turing_rev_c::tests::upload_reports_progress_and_can_be_cancelled` → OK; DoD T-6.4:
  `renderer::tests::device_video_background_renders_a_transparent_base` → OK
- Coverage (`cargo llvm-cov`): `app/storage.rs` 99,42%, `domain/storage.rs` 99,31%, `domain/media.rs` 97,71%,
  `driver/turing_rev_c.rs` 97,87%, `protocol/turing_rev_c.rs` 98,49%, `app/runtime.rs` 95,31%,
  `renderer.rs` 96,10%; workspace 95,54% (medição da T-6.2)

## Hardware validation
- pendente (T-6.8, orquestrador). Pontos a observar do relatório da T-6.2: o cancelamento no meio do upload
  pode virar `Timeout` (bytes de HELLO aceitos como dados) — reconectar e conferir o arquivo; o alfa reto
  sobre vídeo; boot persistente (anotar e restaurar).

## Observações para T-6.2..T-6.7
- Testes unitários do core usam dublês gravadores em `app::storage::doubles` (`#[cfg(test)]`): o core não
  linka os fakes de `bezel-devices` em testes `--lib`. Após a T-6.2, adicionar um teste de integração em
  `crates/bezel-core/tests/` sobre o armazenamento do `FakeConnector`.
- O caminho de cada raiz no dispositivo é do adapter (rev C grande `/mnt/UDISK/...`, pequena `/root/...`,
  cartão `/mnt/SDCARD/...`; TUR_USB `/usr/data/...`, `/tmp/sdcard/mmcblk0p1/...`).
