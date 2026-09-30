# Phase 1: Fundação — Context  (slug: foundation)

## Goal
Workspace hexagonal (core + adapters + CLI + app), especificação de engenharia reversa em docs/, CI pelo pipeline.yml com Release, instalação local; `bezel devices` lista as telas conectadas (somente leitura).

## Locked decisions
- D-2026-09-30-foundation-1: licença GPL-3.0-or-later; nada do TURZX (binário, fonte, tema, código descompilado) entra no repo.
- D-2026-09-30-foundation-2: repositório público `slipalison/bezel`; CI = `pipeline.yml` fixado em `71f8b07`; Sonar com dispensa escrita até existir o projeto `slipalison_bezel` + secret.
- D-2026-09-30-foundation-3: catálogo de dispositivos é dado de domínio no core; a descoberta nunca escreve numa porta.
- D-2026-09-30-foundation-4: Studio Tauri mínimo desde a v0.1.0 (pacotes pelo pipeline); CLI `bezel` como binário extra.
- D-2026-09-30-foundation-5: `docs/reverse-engineering/` é o contrato byte a byte, sanitizado.

## Canonical refs
- `/home/slipalison/repos/ddc-control` (CI, hooks, packaging, install local)
- `slipalison/github-workflows@71f8b07` `exemplos/ci-rust-desktop.yml`
- Relatórios de análise (scratchpad da sessão) → consolidados em `docs/reverse-engineering/`

## Out of scope
- Qualquer escrita em tela (phase `device-protocols`)
- Sensores, renderer, editor (phases seguintes)

## Definition of Done

### Auto-verifiable
- [ ] `bezel devices --json` lists the catalog-matched devices through the fake bus in a test
      **Verify:** `cargo test -p bezel --locked --test devices -- --exact devices_json_lists_fake_turing_88 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] The 8.8" MCU + SoC pair is classified as ONE device with two endpoints
      **Verify:** `cargo test -p bezel-devices --locked -- --exact discovery::tests::groups_turing_88_mcu_and_soc 2>&1 | grep -q '1 passed' && echo OK`
      **Source:** CONTEXT
- [ ] Local install puts a working `bezel` in `~/.local/bin`
      **Verify:** `bash scripts/install-local.sh >/dev/null && ~/.local/bin/bezel --version | grep -qE '^bezel [0-9]+\.[0-9]+\.[0-9]+' && echo OK`
      **Source:** CONTEXT
- [ ] The CI run of the pushed `main` head is green
      **Verify:** `gh run list -R slipalison/bezel --workflow CI --branch main --limit 1 --json conclusion,headSha -q '.[0].conclusion' | grep -qx success && echo OK`
      **Source:** CONTEXT

### Manual
- [ ] On the real Turing 8.8", `bezel devices` shows one screen with its MCU and SoC ports
      **Verify:** human confirmation required
      **Evidence:** command output recorded in SUMMARY.md § Hardware validation
      **Source:** CONTEXT

## Notes
- Crates desta phase: `bezel-core` (thiserror only), `bezel-devices`, `bezel-cli`, `apps/bezel-studio`. `bezel-render`, `bezel-sensors` e `bezel-themes` nascem nas suas phases (YAGNI).
- Versão do workspace `0.0.0` (o pipeline carimba), igual ao ddc-control.
