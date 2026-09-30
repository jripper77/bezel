# Phase 1: Fundação — Plan  (slug: foundation)

## Goal
Workspace hexagonal (core + adapters + CLI + app), especificação de engenharia reversa em docs/, CI pelo pipeline.yml com Release, instalação local; `bezel devices` lista as telas conectadas (somente leitura).

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-foundation-1..5 (licença GPL-3.0+, CI fixado em 71f8b07 + dispensa de Sonar, catálogo no core, Studio mínimo desde a v0.1.0, docs como contrato)

## Tasks

### Wave 1

#### T-1.1: Workspace scaffold
- **Specialist:** jdi-doer-bezel
- **Files modified:** `Cargo.toml`, `Cargo.lock`, `clippy.toml`, `rustfmt.toml`, `LICENSE`, `README.md`, `CHANGELOG.md`, `.cargo/audit.toml`
- **Acceptance:** `cargo build --workspace --locked` passes with the member crates of T-1.3/T-1.4/T-1.5; workspace lints deny `unsafe_code`, warn `unwrap_used`/`expect_used`/`panic`; version `0.0.0`
- **Dependencies:** none
- **Test:** `cargo build --workspace --locked`
- **Status:** pending

#### T-1.2: Reverse-engineering specification
- **Files modified:** `docs/reverse-engineering/*.md`
- **Acceptance:** one file per protocol family + formats + sensors + UI inventory + device catalog; no decompiled code; serials redacted
- **Dependencies:** none
- **Test:** `ls docs/reverse-engineering/*.md` + manual read
- **Status:** pending

### Wave 2

#### T-1.3: Core domain — device catalog and discovery port
- **Files modified:** `crates/bezel-core/**`
- **Acceptance:** `DeviceModel` catalog with every known model (family, ids, resolution, native orientation, capabilities); `DeviceBus` driven port; `list_devices` use case grouping endpoints into screens; deps = thiserror only
- **Dependencies:** T-1.1
- **Test:** `cargo test -p bezel-core`
- **Status:** pending

#### T-1.4: Devices adapter — read-only discovery
- **Files modified:** `crates/bezel-devices/**`
- **Acceptance:** `SystemBus` enumerates serial ports (serialport) and USB devices (nusb) without opening any; `FakeBus` for tests; test `discovery::tests::groups_turing_88_mcu_and_soc`
- **Dependencies:** T-1.3
- **Test:** `cargo test -p bezel-devices`
- **Status:** pending

### Wave 3

#### T-1.5: CLI `bezel devices`
- **Files modified:** `crates/bezel-cli/**`
- **Acceptance:** `bezel --version`, `bezel devices [--json]`; composition root in main.rs; integration test `devices_json_lists_fake_turing_88` via a hidden `--fake` bus
- **Dependencies:** T-1.4
- **Test:** `cargo test -p bezel-cli`
- **Status:** pending

#### T-1.6: Bezel Studio skeleton
- **Files modified:** `apps/bezel-studio/**`
- **Acceptance:** Tauri 2 window listing detected screens through a `list_devices` async command; i18n pt-BR/en; demo bridge; `npm test` (node --test with coverage ≥ 80% of `src/` logic modules + Playwright smoke)
- **Dependencies:** T-1.4
- **Test:** `cargo test -p bezel-studio`, `npm test` in apps/bezel-studio
- **Status:** pending

### Wave 4

#### T-1.7: CI + packaging
- **Files modified:** `.github/workflows/ci.yml`, `scripts/ci/sonar-coverage.sh`, `packaging/linux/*`, `.trivyignore`
- **Acceptance:** one job calling `pipeline.yml@71f8b07` with rust-linux (ubuntu-22.04, 80%), rust-windows, node-ui components; udev `uaccess` rules for every catalog VID:PID in deb/rpm
- **Dependencies:** T-1.5, T-1.6
- **Test:** CI run on the PR/main
- **Status:** pending

#### T-1.8: Local install + GitHub repo
- **Files modified:** `scripts/install-local.sh`, `README.md`
- **Acceptance:** script builds release `bezel` + `bezel-studio`, installs into `~/.local` (bin, desktop entry, icon) without sudo; repo `slipalison/bezel` created and pushed; CI green on main
- **Dependencies:** T-1.7
- **Test:** DoD rows of CONTEXT.md
- **Status:** pending

## Execution
- Total tasks: 8
- Waves: 4

## Test requirements
- Rust: `cargo test --workspace --locked`, coverage ≥ 80% (`cargo llvm-cov`)
- UI: `npm test` in `apps/bezel-studio`
