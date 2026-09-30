# Phase 6: Armazenamento e vídeo — Summary  (slug: storage-video)

**Status:** partial
**Tasks:** 7/8 complete, 0 blocked (T-6.8: CLI validada na 8.8"; aba do studio, alfa e boot aguardam confirmação humana)

> Execução: `/jdi-do` com o `jdi-doer-bezel` por tarefa, em worktrees, cherry-picked para a `main`.

## Executed tasks
- T-6.1 — `5e36061`: porta `ScreenStorage` via `ScreenLink::storage()` (default `None`); domínio de
  armazenamento (4 raízes, nomes `[a-z0-9_.-]` ≤ 208 B, `RemotePath`, capacidade em bytes, prova `Confirmed`
  só de `Confirm::Yes`); `preflight` (nome, perfil, 120 MB/2 GiB, cartão, espaço; sem espaço lista candidatos
  e nunca apaga); `domain::media` (perfis por família, `ConvertOptions`, `TranscodeTarget`); jobs
  (`Progress`, `CancelToken`); casos de uso `app::storage`; porta `MediaTranscoder`; `Backdrop`; erros
  `Unsupported`/`Cancelled`/`NotConfirmed`/`Refused`.
- T-6.2 — `4dba6cc`: rev C — encoders do § 17.2 (0x64/65/66/6E/6F/78/8C), `StorageReport` (−512 KiB, cartão
  com TF > 1024 KiB), upload do § 13.4 com progresso e cancelamento (HELLO + GET_FILE_SIZE → parcial, nunca
  apaga), boot pelo 0x7D com o último brilho, quadro cheio após tocar/enviar (§ 7.2), nada disruptivo
  implícito; `FakeStorage` no `FakeConnector`.
- T-6.3 — `535e403`: `bezel-media` — ffmpeg/ffprobe externos (config, depois PATH; exige libx264; dicas por
  SO), `probe` nativo de MP4 e imagens, `transcode` com a cadeia do fornecedor (vetor de argumentos,
  progresso, cancelar apaga a saída), `stream` rawvideo em cover. Só depende do core.
- T-6.4 — `3957f14`: renderer com base transparente (`OnDevice`) e frame do PC (`Frame`), goldens antigos
  idênticos; `ThemeRuntime::start_video` (tocar se o vídeo está na tela, senão pôster + `VideoMissing`;
  `Host` via `stream`); `device_video_name`.
- T-6.5 — `59a4e1f`: TUR_USB só com vetores golden; sem o 98, tamanho = presença + bytes gravados pelo link;
  delete/boot/tocar-uma-vez `Unsupported` (D-2026-09-30-storage-video-7).
- T-6.6 — `d327cef`: CLI `bezel storage info|ls|put|rm|play|stop|boot` (`--json`, resumo antes de ação
  destrutiva, `--yes`, barra de progresso, Ctrl+C cancela, `--orientation`/`--fps`/`--ffmpeg`, `cover_crop`);
  `bezel run` com vídeo de fundo.
- T-6.7 — `3d1842f`, `af98ba2`: aba Armazenamento do studio (uso flash/SD, listas, arrastar para enviar,
  progresso e Cancelar, Tocar/Parar, Apagar e "Ao ligar" atrás de `<dialog>`; o job pega o link do Ao vivo
  emprestado sem travar o preview; "Enviar para a tela" do vídeo do tema; ffmpeg ausente em linha; TUR_USB
  sem Apagar/Boot; demo e i18n).
- Orquestrador: `cover_crop` no core (`d4130d1`); correções achadas no hardware (`2743b4a`): escrita serial
  resistente a sinais e cancelamento no meio de uma escrita.

## Blocked tasks
- nenhuma

## Files modified
- `crates/bezel-core/src/{domain/{storage,job,media,mod,error},ports/mod,app/{mod,storage,runtime}}.rs`,
  `crates/bezel-core/tests/storage.rs`
- `crates/bezel-devices/src/{protocol,driver}/{turing_rev_c,turing_usb}.rs`, `driver/mod.rs`, `fake.rs`,
  `wire.rs`
- `crates/bezel-media/**` (novo), `crates/bezel-render/src/{renderer,golden,testkit}.rs`
- `crates/bezel-cli/src/{storage,lib,main,live,screen}.rs`, `crates/bezel-cli/tests/storage.rs`
- `apps/bezel-studio/src-tauri/src/{storage,studio,media,commands,dto,lib,settings}.rs`, `build.rs`,
  `capabilities/default.json`; `apps/bezel-studio/src/{ui/storage,ui/icons,app,bridge,demo-backend}.js`,
  `src/i18n/*`, `src/{index.html,styles.css}`, `tests/**`
- `Cargo.toml`, `Cargo.lock`, `README.md`, `CHANGELOG.md`, `docs/reverse-engineering/protocol-turing-rev-c.md`

## Tests
- `cargo test --workspace --locked`: 499 passando, 0 falhando, 6 ignorados (4 de ffmpeg real, rodados à parte:
  4/4); UI: 56 unitários, 24 Playwright (claro/escuro, axe)
- DoD Auto: as 8 linhas do CONTEXT → OK (verificadas pelo revisor)
- Coverage (`cargo llvm-cov`): 94,88% de linhas no workspace

## Hardware validation
Turing 8.8" (ROM 1.90) com cartão de 29,7 GiB; serviço do usuário parado e religado; só `bezel_test_*`
criados e todos apagados (arquivos do usuário intocados). Detalhes em protocol-turing-rev-c.md § 19.
- `storage info/ls` (+`--json`): interna 65,9 MiB, cartão detectado; recusas sem `--yes` não enviam nada.
- PNG, MP4 nativo e MP4 1920x1080 convertido (90°, centro, 24 fps, sem áudio) enviados, verificados e
  tocados na interna e no cartão; um MP4 de 79 MiB recusado por falta de espaço sem apagar nada.
- Cancelamento no cartão: antes de `2743b4a` erro cru de flush; depois, parcial reportado e reconexão na
  conexão seguinte. O primeiro upload após o cancelamento recebeu ~191 KB a mais (a verificação pegou) e o
  cartão reportou o tamanho do parcial como usado após apagá-lo (todo `revc-cancel-leftovers`).
- Tema 8.8" horizontal com vídeo de fundo: sem o vídeo, pôster + comando exato; depois de enviado, a tela
  toca o vídeo sob o tema (25 frames, quadro cheio aceito).
- **Pendente (humano):** transparência sobre o vídeo, boot após desligar/ligar, aba do studio na 8.8".
