# Phase 6: Armazenamento e vídeo — Summary  (slug: storage-video)

**Status:** partial
**Tasks:** 6/8 complete, 0 blocked (T-6.7 em andamento; T-6.8 parcial: CLI validada, studio e itens visuais pendentes)

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

- T-6.3: crate `bezel-media` (adapter do `MediaTranscoder`, depende só de `bezel-core`) — `535e403`
  - ffmpeg/ffprobe externos: caminho configurado (programa ou pasta) e depois o PATH; exige libx264 (o
    `ffmpeg-free` do Fedora não serve); sem ferramenta utilizável, `MediaTools::Missing` com os comandos de
    instalação do SO (RPM Fusion no Fedora, apt, winget), nunca pânico.
  - `probe` nativo em Rust para MP4 (codec, tamanho, yuv420p pelo SPS, B-frames pelo `ctts`, fps, duração,
    áudio) e imagens; o resto via ffprobe JSON ou `Unsupported`.
  - `transcode`: vetor de argumentos com a cadeia do fornecedor (transpose, crop, scale+setsar, libx264 CRF 20,
    `-r` opcional, `-an -pix_fmt yuv420p`; TUR_USB `bframes=0`, `eq` opcional), entradas/saídas como URLs
    `file:`; progresso por `-progress pipe:1`; cancelar mata o processo e apaga a saída.
  - `stream`: rawvideo RGBA em cover, fila de 2 frames, loop — o fallback de vídeo pelo PC.
  - Fora do `files_modified`: `crates/bezel-media/src/process.rs` (processos comuns aos três módulos).
  - 4 testes com ffmpeg real (`#[ignore]`) rodados localmente com ffmpeg 8.1.3 + libx264: 4/4.
- T-6.5: TUR_USB — `ScreenStorage` só com vetores golden, `hardware_validated=false` — `59a4e1f`
  - Raízes `/usr/data/` e `/tmp/sdcard/mmcblk0p1/`; `info` = 100 (KiB → bytes, cartão com TF total ≠ 0);
    `list` = 99; `upload` = 111/112 → 38 → 39 por 1 MiB com progresso e cancelamento (SYNC + `size` →
    `Cancelled{partial}`); `play_*` = parar, PNG transparente, 110/113; reusa os auxiliares do `driver/mod.rs`.
  - Sem o 98 não há consulta de tamanho: `size` = presença no LIST_DIR + bytes gravados pelo link; `delete`,
    `set_start_mode` e `Repeat::Once` = `Unsupported` (D-2026-09-30-storage-video-7); 40 e 98 nunca saem.
- T-6.6: CLI `bezel storage info|ls|put|rm|play|stop|boot` — `d327cef`
  - Tamanhos legíveis, `--json` em `info`/`ls`; resumo antes de toda ação destrutiva (mesmo com `--yes`);
    sem `--yes`, `rm`/`boot` nem abrem a tela e `put` só consulta; barra de progresso em TTY; Ctrl+C cancela
    pelo `CancelToken`; `--orientation`, `--fps`, `--ffmpeg`; vídeo de outra proporção com `cover_crop`;
    sem ffmpeg, dicas por SO; tela cheia lista candidatos e não apaga nada; `Unsupported` vira uma frase clara.
  - `bezel run` com tema de vídeo chama `start_video`: vídeo na tela → loop com base transparente; ausente →
    pôster e o comando `bezel storage put …` exato; sem reprodução → decodificado no PC.
  - `--fake`: 8.8" simulada com arquivos de demonstração e cartão de 8 GiB.
- Orquestrador (achados de hardware da T-6.8): `2743b4a` — escrita serial resistente a sinais (o flush do
  serialport desistia com "timeout for retrying flush reached" quando um sinal interrompia um esvaziamento
  de mais de 10 ms, derrubando qualquer upload) e escrita que falha após o cancelamento tratada como o
  cancelamento.
- Orquestrador: `cover_crop` no core (`d4130d1`) — um vídeo de outra proporção guarda o centro em vez de
  esticar; usado pela CLI e pelo studio.

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/domain/{storage,job,media,mod,error}.rs`, `crates/bezel-core/src/ports/mod.rs`,
  `crates/bezel-core/src/app/{mod,storage,runtime}.rs`
- só o campo `backdrop`: `crates/bezel-render/src/testkit.rs`, `crates/bezel-render/tests/support/showcase.rs`,
  `crates/bezel-cli/src/screen.rs`, `apps/bezel-studio/src-tauri/src/studio.rs`

## Tests
- `cargo test --workspace --locked` na `main` após T-6.1..T-6.6 e as correções de hardware: 486 passando, 0 falhando, 6 ignorados (4 de ffmpeg real)
- DoD: `domain::storage::tests::destructive_operations_require_confirm_yes` e
  `domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage` → OK
- DoD T-6.2: `protocol::turing_rev_c::tests::storage_packets_match_the_reference_vectors`,
  `...::storage_info_subtracts_the_reserved_flash_and_detects_the_card`,
  `driver::turing_rev_c::tests::upload_reports_progress_and_can_be_cancelled` → OK; DoD T-6.4:
  `renderer::tests::device_video_background_renders_a_transparent_base` → OK
- DoD T-6.6: `storage::tests::rm_without_yes_is_refused` → OK
- DoD T-6.3: `transcode::tests::builds_the_vendor_argument_vector_for_rev_c` e
  `probe::tests::missing_ffmpeg_is_reported_not_fatal` → OK
- Coverage (`cargo llvm-cov`): `bezel-media` 90,82% (97,4% com os ignorados); `app/storage.rs` 99,42%, `domain/storage.rs` 99,31%, `domain/media.rs` 97,71%,
  `driver/turing_rev_c.rs` 97,87%, `protocol/turing_rev_c.rs` 98,49%, `app/runtime.rs` 95,31%,
  `renderer.rs` 96,10%; workspace 95,54% (medição da T-6.2)

## Hardware validation
Turing 8.8" real (ROM `chs_88inch.dev1_rom1.90`) com cartão SD de 29,7 GiB, `bezel` da `main` (CLI), o
`turing-smart-screen.service` do usuário parado durante os testes e religado ao final; só arquivos
`bezel_test_*` foram criados e todos foram apagados (os 5 vídeos internos e 12 do cartão do usuário não
foram tocados).
- `storage info` / `--json`: interna 65,9 MiB (18,6 usados), cartão 29,7 GiB detectado; `ls` e `ls --json`
  listam as raízes com tamanhos.
- Recusas: `rm` e `boot` sem `--yes` → resumo, "Nothing was sent", código 1; `put` sobre arquivo existente
  sem `--yes` → código 1.
- `put` PNG 480x1920 em `internal/image` → enviado e verificado; `play`/`stop` ok.
- `put` MP4 480x1920 nativo em `internal/video` → "as is", verificado; `play` em loop, `stop` ok.
- `put` MP4 1920x1080 com áudio `--orientation horizontal --fps 24` → convertido pelo ffmpeg (90°, centro
  1920x480, 24 fps, sem áudio), verificado, tocado.
- Espaço: um MP4 de 79 MiB para a interna (44 MiB livres) foi recusado no preflight com a lista de
  candidatos, nada apagado.
- Cancelamento no cartão (Ctrl+C a ~30%): antes da correção `2743b4a`, "transport error: timeout for
  retrying flush reached"; depois, "upload cancelled accepted=25688832" e, como o firmware ainda esperava o
  resto do arquivo e não respondeu ao HELLO, a mensagem "the next command reconnects it, then check
  sd/video/… for a partial file". A conexão seguinte reconectou (wake + retry, ~10 s) e listou o parcial de
  24,3 MiB, apagado com `rm --yes`.
- Achado: o **primeiro** upload depois de uma transferência abortada gravou 198 925 bytes para um PNG de
  7 444 (o firmware anexou restos); a verificação de tamanho acusou e o arquivo foi apagado. Repetido com o
  estado limpo: exato, duas vezes. Após a limpeza o cartão reporta 24,3 MiB a mais de uso do que antes (a
  listagem está limpa): contabilidade do FAT ou resto da transferência abortada — conferir após reiniciar
  a tela.
- Cartão: PNG em `sd/image` e MP4 em `sd/video` enviados, tocados e apagados.
- Tema com vídeo de fundo (8.8" horizontal): `run` sem o vídeo → pôster + o comando `put` exato; após
  `storage put … sd/video/bezel_test_loop_90.mp4 --orientation horizontal` (convertido, 3,3 MiB), `run` →
  "the screen plays … under the theme", 25 frames em 24,4 s, frame cheio aceito (`full_png_sucess`) depois
  do PLAY_VIDEO, sem erros.
- **Pendente de confirmação humana (DoD manual):** a transparência do tema sobre o vídeo (visual); o boot
  (`storage boot … --yes` + desligar/ligar a tela; não executado: persistente e exige ação física); a aba
  de armazenamento do studio (T-6.7).

## Observações para T-6.2..T-6.7
- Testes unitários do core usam dublês gravadores em `app::storage::doubles` (`#[cfg(test)]`): o core não
  linka os fakes de `bezel-devices` em testes `--lib`. Após a T-6.2, adicionar um teste de integração em
  `crates/bezel-core/tests/` sobre o armazenamento do `FakeConnector`.
- O caminho de cada raiz no dispositivo é do adapter (rev C grande `/mnt/UDISK/...`, pequena `/root/...`,
  cartão `/mnt/SDCARD/...`; TUR_USB `/usr/data/...`, `/tmp/sdcard/mmcblk0p1/...`).
