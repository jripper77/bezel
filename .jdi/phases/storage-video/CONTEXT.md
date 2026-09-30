# Phase 6: Armazenamento e vídeo — Context  (slug: storage-video)

## Goal
Cartão SD/armazenamento da tela (listar, enviar, apagar, espaço), fundo em vídeo tocado na tela com tema sobreposto, imagem de boot, transcodificação ffmpeg e vídeo/GIF transmitido pelo PC.

## Locked decisions
- D-2026-09-30-storage-video-1: porta `ScreenStorage` no core via `ScreenLink::storage()`; SD só pelo protocolo (0x64/0x65/0x66/0x6E/0x6F); apagar/sobrescrever exigem `Confirm`; nunca formatar nem limpar automático; rev C validada, TUR_USB só golden.
- D-2026-09-30-storage-video-2: ffmpeg/ffprobe externos (config, depois PATH), nunca embutidos; porta `MediaTranscoder` + crate `bezel-media`; sem ffmpeg o app degrada com aviso claro.
- D-2026-09-30-storage-video-3: formatos por família validados antes de converter/enviar (rev C: MP4 H.264 yuv420p na resolução nativa, 480x1920 na 8.8"); nomes `[a-z0-9_.-]`, ≤ 120 MB, espaço livre checado, sem limpeza automática.
- D-2026-09-30-storage-video-4: vídeo de fundo tocado pela tela (loop) com o tema por cima em alpha; envio do vídeo só por ação explícita; vídeo/GIF transmitido pelo PC é fallback (WCH, telas sem armazenamento).
- D-2026-09-30-storage-video-5: mídia de boot = startMode 0x7D (imagem/vídeo) atrás de `Confirm`; logo de boot TUR_USB só golden; firmware, C9 e RESTART fora.
- D-2026-09-30-storage-video-6: operações longas são jobs com progresso e `CancelToken`; painel "Tela" com aba de armazenamento; CLI `bezel storage ...` com `--yes`; validação real na 8.8" listada na decisão.

## Canonical refs
- `docs/reverse-engineering/protocol-turing-rev-c.md` (§ 6.2, 13, 16), `protocol-turing-usb.md` (§ 6, 7, 11), `video.md`, `devices.md`, `ui-inventory.md` (§ 2.5), `pixel-formats.md` § 8
- Políticas herdadas: D-1, D-2026-09-30-device-protocols-2, -5

## Out of scope
- Formatar o SD, firmware, boot logo real, ffmpeg embutido, baixar/renomear arquivos (ver `.jdi/todos/2026-09-30-storage-video.md`)

## Definition of Done

### Auto-verifiable
- [ ] Rev C storage packets and the storage-info reply match the reference vectors
      **Verify:** `export LC_ALL=C.UTF-8; for t in storage_packets_match_the_reference_vectors storage_info_subtracts_the_reserved_flash_and_detects_the_card; do cargo test -p bezel-devices --locked --lib -- --exact "protocol::turing_rev_c::tests::$t" 2>&1 | grep -q '1 passed' || { printf 'missing %s\n' "$t"; exit 1; }; done && echo OK`
      **Source:** CONTEXT
- [ ] Delete and overwrite are refused without `Confirm::Yes`, before any byte reaches the link
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::storage::tests::destructive_operations_require_confirm_yes 2>&1 | grep -q '1 passed' && cargo test -p bezel-core --locked --test storage -- --exact replacing_deleting_and_the_boot_slot_need_confirmation 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Rev C upload follows the vendor sequence, reports progress and can be cancelled
      **Verify:** `cargo test -p bezel-devices --locked --lib -- --exact driver::turing_rev_c::tests::upload_reports_progress_and_can_be_cancelled 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Preflight rejects bad names, oversize files, wrong resolution and full storage, and never deletes anything
      **Verify:** `cargo test -p bezel-core --locked --lib -- --exact domain::storage::tests::preflight_rejects_bad_names_sizes_and_full_storage 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] ffmpeg arguments are a vector with the vendor chain, and a missing ffmpeg is a reported state, not a crash
      **Verify:** `export LC_ALL=C.UTF-8; for t in transcode::tests::builds_the_vendor_argument_vector_for_rev_c probe::tests::missing_ffmpeg_is_reported_not_fatal; do cargo test -p bezel-media --locked --lib -- --exact "$t" 2>&1 | grep -q '1 passed' || { printf 'missing %s\n' "$t"; exit 1; }; done && echo OK`
      **Source:** CONTEXT
- [ ] A theme with a video background renders a transparent base for device-played video
      **Verify:** `cargo test -p bezel-render --locked --lib -- --exact renderer::tests::device_video_background_renders_a_transparent_base 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] `bezel storage rm` without `--yes` is refused
      **Verify:** `cargo test -p bezel --locked --lib -- --exact storage::tests::rm_without_yes_is_refused 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Studio storage tab: upload shows progress, cancel works, delete asks for confirmation (demo mode, light and dark)
      **Verify:** `cd apps/bezel-studio && npx playwright test -g "storage tab upload progress and confirmed delete" --reporter=line 2>&1 | grep -qE '[0-9]+ passed' && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] On the real 8.8": storage info and file lists (internal and SD when present), upload/verify/play/stop/delete of test files Bezel created, image and video playback
      **Verify:** human confirmation required
      **Evidence:** `bezel storage info|ls|put|play|stop|rm --yes` output and the studio storage tab; SUMMARY.md § Hardware validation
      **Source:** CONTEXT
- [ ] On the real 8.8": a looping device-played video with the live theme over it (overlay alpha correct), and the boot slot set and confirmed after a power cycle
      **Verify:** human confirmation required
      **Evidence:** photo/description of the screen; SUMMARY.md § Hardware validation
      **Source:** CONTEXT

## Notes
- Ordem sugerida: core (porta, `Confirm`, preflight) → rev C (protocolo + driver + vetores) → bezel-media → render/overlay → CLI → studio. Fase grande: se estourar, dividir em storage e video.
- Preferir arquivos de teste pequenos no hardware; sem apagar arquivos que o Bezel não criou.
