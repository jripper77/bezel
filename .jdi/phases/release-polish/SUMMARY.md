# Phase 7: Polimento e release 1.0 — Summary  (slug: release-polish)

**Status:** partial
**Tasks:** 11/12 complete, 0 blocked (T-7.8 = confirmações humanas e corte da 1.0.0)

> `/jdi-do` com o `jdi-doer-bezel` em worktrees, cherry-picked para a `main` (hashes da `main`). T-7.9..T-7.12
> nasceram da passagem na 8.8" e de pedidos do usuário durante a fase.

## Executed tasks
- T-7.1 `41f647d`..`99e7c92`: duplicações da D-9 numa função do core, `BezelError::ThemeFile`, TUR_USB de tamanho
  desconhecido = presente.
- T-7.2 `fbea521`, `ef8e8da`: catálogo único de chaves, hwmon/amdgpu unificados, `SensorOptions`.
- T-7.3 `997c41f`..`66c3001`: connector testável; escrita parada falha em 10 s; sem enchimento após cancelar (D-10).
- T-7.4 `e941130`: modo desktop HID com `Confirm`; `bezel udev-rules`.
- T-7.5 `9071262`: `gpu.fps` (RTSS/MangoHud) e `net.ping`.
- T-7.6 `01f6177`..`a9efb90`: studio em pt-BR/en com mensagens por código, Preferências, comando udev, modo desktop.
- T-7.7 `0fd7f0f`, `4762377`: deb/rpm com `bezel`, unit e temas; guia do usuário em en e pt-BR.
- Correções do verify BLOCKED: B1 Windows (`6bb5ae6`, `e00abb8`, `cd276fd`: imports só-Unix e aspas de PowerShell;
  primeira CI do Windows com todos os testes); W1 `52f48b8` (demo segura o envio); W2 `007e0bb` (`net.ping` só
  quando mostrado, D-11); W3 `5f2b3e8` (`BezelError::SizeMismatch`); W4 docs.
- T-7.9 `7d9dc5f`..`aa746c0`: vídeo e GIF de fundo no studio (porta de pôster, GIF → MP4 da tela).
- T-7.10 `777c0d7`, `2829fa8`: teto de 25 MiB por arquivo rev C (D-12) e reinício pelo MCU (D-13), `bezel restart`,
  automático uma vez, ação no studio.
- T-7.11 `afa6e9c`: GIF no próprio ritmo (só o retângulo, ≤ 30 fps, sem atraso); `bezel run` e o ao vivo reconectam.
- T-7.12 `1b4426a`, `5a966cd`: aba Temas com miniaturas reais (cache, refeitas ao salvar) e filtro tela/orientação.
- Orquestrador: `79920ad` (tela reaberta volta com a porta do SoC, achado no hardware), `c6773ba` (testes do ao vivo
  em pastas únicas), docs, decisões D-11..D-13, fase 8 `storage-manager` aberta a pedido do usuário.

## Blocked tasks
- nenhuma (T-7.8 aguarda o usuário)

## Files modified
- `crates/bezel-core/**` (domínio, portas, app, testes), `crates/bezel-devices/**`, `crates/bezel-sensors/**`,
  `crates/bezel-media/**`, `crates/bezel-render/src/{images,renderer}.rs`, `crates/bezel-themes/src/**`,
  `crates/bezel-cli/**`, `apps/bezel-studio/**`, `packaging/linux/**`, `scripts/**`, `docs/user/**`,
  `docs/reverse-engineering/{protocol-turing-rev-c,protocol-turing-usb,sensors,devices}.md`, `README.md`,
  `CHANGELOG.md`, `Cargo.toml`, `Cargo.lock`
- Fora de `files_modified` (sinalizados nas tarefas): `ports/mod.rs` (doc), `fake.rs`, testes de integração do core e
  da CLI (T-7.1); `sensors.md` § 8 (T-7.5); `build.rs`/capabilities, `tests/hardware.rs`, `import_corpus.rs` (T-7.6);
  `postinstall.sh` (T-7.7); `wire.rs`, `driver/mod.rs`, `README.md` (T-7.10); `bezel-render` (T-7.11).

## Tests
- `cargo test --workspace --locked`: 749 passando, 0 falhando, 9 ignorados (hardware e ffmpeg real, rodados à parte)
- UI: 125 unitários; Playwright 144 (36 cenários × claro/escuro × pt-BR/en, axe sem violações sérias/críticas)
- fmt, clippy `-D warnings` (Linux e `--target x86_64-pc-windows-msvc`), `check-packaging.sh`, `check-docs.sh`
- Coverage (`cargo llvm-cov`): 94,73% de linhas; `connector.rs` 97,78%, `wire.rs` 83,21%, `runtime.rs` 100%,
  `animation.rs` 98,80%, `thumbnails.rs` 97,00%
- CI: verde no Linux, Windows e UI; releases automáticas v0.4.0..v0.8.0 com os 5 pacotes

## Hardware validation
Turing 8.8" (ROM 1.90, cartão de 29,7 GiB); serviço do usuário parado e religado; só `bezel_test_*`, todos apagados.
- Padrão nas 4 orientações (180–199 quadros/s); `bezel run` 60 quadros em 59,6 s; sensores × `nvidia-smi`, `free`,
  `sensors` e `df` dentro do arredondamento.
- Envio: 5,1 MiB em 0,63 s, 11,8 MiB em 1,63 s. O firmware guarda o envio inteiro e para de ler em exatamente
  29.577.216 bytes, a 7 MiB/s ou a 0,94 MiB/s (três parciais idênticos), e trava: era a causa do travamento antes
  atribuído ao enchimento. 24,0 MiB passa. Daí o teto de 25 MiB (D-12): 40 MiB recusado antes de enviar; 1080p de
  157 MiB convertido para 21,2 MiB, enviado e tocado.
- MCU `00 00 00 00 00 c9`: o SoC sai na hora e volta em ~10 s, também travado (D-13). `bezel restart` 10,4 s; tela
  travada + comando comum = reinício automático e resposta em 28 s, sem religar o cabo.
- Cancelar sem enchimento: parcial de 10,7 MiB informado; o PNG seguinte ficou com 963.808 bytes em vez de 7.399, a
  conferência de tamanho pegou; apagado e reenviado exato.
- Vídeo de fundo: GIF de 17 MB → MP4 de 4,0 MiB em 1,7 s; a tela tocou o vídeo sob o tema por 30 s.
- GIF 240×240 como elemento: 24,6 quadros/s (antes 1/s). Reinício externo no meio do `bezel run`: volta sozinho em ~11 s.
- Pacotes da v0.4.0: deb e rpm passam no `check-packaging.sh` estrito.
- **Pendente (T-7.8, humano):** confirmar o que a tela mostrou (legenda nas 4 orientações, tema, vídeo com alfa),
  boot após desligar/ligar, studio ao vivo e bandeja, "Reiniciar a tela", miniaturas reais; instalar os pacotes
  (Linux e Windows); FPS num jogo real; corte da 1.0.0.

## Observações
- Detalhes em inglês dentro de mensagens pt-BR: todo `english-error-details`.
- O preview do studio conta como "mostrado" para o `net.ping` mesmo com a janela na bandeja e o ao vivo desligado.
- Esteira em `71f8b07` (github-workflows#15 fora da `main`); Sonar dispensado.
