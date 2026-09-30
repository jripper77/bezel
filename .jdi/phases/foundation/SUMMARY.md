# Phase 1: Fundação — Summary  (slug: foundation)

**Status:** complete
**Tasks:** 8/8 complete, 0 blocked

## Executed tasks
- T-1.1: workspace Cargo (resolver 3, edition 2024, lints `unsafe_code=deny`, `unwrap/expect/panic=warn`), GPL-3.0-or-later, README, CHANGELOG — `cd2fcba`
- T-1.2: `docs/reverse-engineering/` (17 arquivos, 3.785 linhas: índice, catálogo de dispositivos, protocolos A/B/C/D/WeAct/TUR_USB/WCH, formatos de pixel, vídeo, temas YAML e `.turtheme`, renderização, sensores, inventário de UI, artefatos de runtime), sanitizado conforme D-2026-09-30-foundation-5
- T-1.3: `bezel-core` — catálogo de 24 modelos + 22 regras de reconhecimento USB, geometria/orientação, agrupamento MCU+SoC por hub — `cd2fcba`
- T-1.4: `bezel-devices` — `SystemBus` (serialport com `usbportinfo-location` + nusb), `FakeBus` — `58e8895`
- T-1.5: CLI `bezel devices [--json]` (pacote `bezel`, lib `bezel_cli`) — `939ddaa`, `f677591`
- T-1.6: Bezel Studio (Tauri 2) com lista de telas, preview em proporção real, inspetor, i18n pt-BR/en, modo demo; `node --test` + Playwright/axe — `9690814`
- T-1.7: CI = `pipeline.yml@71f8b07` (rust-linux, rust-windows, node-ui), regra udev gerada do catálogo + teste de sincronia, `.cargo/audit.toml`, `.trivyignore`, script do Sonar pronto — `f677591`
- T-1.8: `scripts/install-local.sh` (~/.local, sem sudo, versão carimbada pelo git); repo público `slipalison/bezel` criado — `80270f2`

## Blocked tasks
- nenhuma

## Files modified
- `Cargo.toml`, `Cargo.lock`, `clippy.toml`, `LICENSE`, `README.md`, `CHANGELOG.md`, `.cargo/audit.toml`, `.trivyignore`
- `crates/bezel-core/**`, `crates/bezel-devices/**`, `crates/bezel-cli/**`, `apps/bezel-studio/**`
- `.github/workflows/ci.yml`, `scripts/ci/sonar-coverage.sh`, `scripts/install-local.sh`, `packaging/linux/*`
- `docs/reverse-engineering/*.md`

## Tests
- Rust: 28 passing (`cargo test --workspace --locked`, worktree limpo do HEAD da phase)
- UI: 15 unit (`node --test`, 100% de linhas dos módulos de lógica) + 6 Playwright (claro/escuro, axe sem critical/serious)
- Coverage: 84.67% lines (`cargo llvm-cov --summary-only`, `main.rs`/`build.rs` excluídos)

## CI / Release
- Run 36713677654 (push de `80270f2` na main): qualidade verde nos três componentes; na 1ª tentativa o upload de SARIF (Semgrep/Trivy e CodeQL JS) falhou sem mensagem no repositório recém-criado; `gh run rerun --failed` → tudo verde, `Portao` success, `lancar` publicou **v0.1.0** com deb, rpm, AppImage, msi, NSIS, `bezel-*.tar.gz/.zip` e `SHA256SUMS`.

## Hardware validation
- `bezel devices` na máquina do dev (leitura apenas):
  ```
  1. Turing Smart Screen 8.8"  480x1920  [awake]
     family   turing-rev-c
     display  /dev/ttyACM1   0525:a4a7  serial -  usb 3-1.2
     wake     /dev/ttyACM0   1a86:ca88  serial CT88INCH  usb 3-1.1
  ```
- Bezel Studio aberto na sessão KDE Wayland (NVIDIA): sem crash (reexec com `WEBKIT_DISABLE_DMABUF_RENDERER=1`), lista a 8.8" real.
- Instalação local: `bezel 0.0.0-dev.9+f0fbd8b` e `bezel-studio` em `~/.local/bin`, `.desktop` e ícones em `~/.local/share`.

## Learnings
- O `pipeline.yml` constrói `binarios_extra` por nome de pacote cargo **e** procura o binário com o mesmo nome: pacote do CLI = `bezel`.
- Primeiro upload de SARIF num repositório novo pode falhar sem mensagem; `rerun --failed` resolve.
- O usuário roda `turing-smart-screen.service` (systemd user) segurando `/dev/ttyACM1`: dois programas na mesma porta intercalam pacotes. O Bezel passa a detectar o dono da porta (phase `device-protocols`).
