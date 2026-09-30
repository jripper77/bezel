# Phase 2: Protocolos de dispositivo — Plan  (slug: device-protocols)

## Goal
Todos os protocolos com vetores de teste, transportes serial/USB, wake do MCU e CLI de controle validados na 8.8" real.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-device-protocols-1..5

## Tasks

### Wave 1 (already implemented against the real screen)

#### T-2.1: Core frames and screen ports
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{frame,screen,pattern,error,mod}.rs`, `crates/bezel-core/src/ports/mod.rs`, `crates/bezel-core/src/app/mod.rs`
- **Acceptance:** RGBA `Frame` with crop/fill/rotation; tile-aligned `dirty_rects`; `Brightness`, `Confirm`, `ScreenIdentity`; `ScreenConnector`/`ScreenLink` ports; `choose_screen`/`open_screen`; `InUse` error
- **Dependencies:** none
- **Test:** `cargo test -p bezel-core`
- **Status:** completed (d126a49, 8903926, 9aac3de)

#### T-2.2: Rev C transport, protocol and driver
- **Files modified:** `crates/bezel-devices/src/{wire,connector,busy}.rs`, `crates/bezel-devices/src/{protocol,driver}/{mod,turing_rev_c}.rs`, `crates/bezel-devices/src/{fake,lib}.rs`
- **Acceptance:** 250-byte packets and 249+1 blocks match the vectors; HELLO/ROM parse; run-list diff; STOP_MEDIA wait; full-frame fallback on `needReSend:1`; wake of an asleep screen; busy-port refusal; `FakeConnector`
- **Dependencies:** T-2.1
- **Test:** `cargo test -p bezel-devices`
- **Status:** completed (90ac992, 571bfed, cdf3223)

#### T-2.3: CLI screen commands
- **Files modified:** `crates/bezel-cli/**`
- **Acceptance:** `test-pattern`, `brightness`, `release`, `--verbose` tracing
- **Dependencies:** T-2.2
- **Test:** `cargo test -p bezel`
- **Status:** completed (bb02945, 91945f2)

### Wave 2 (parallel, one family each)

#### T-2.4: Turing rev A and XuanFang rev B
- **Files modified:** `crates/bezel-devices/src/{protocol,driver}/{turing_rev_a,xuanfang_rev_b}.rs`
- **Acceptance:** vectors of docs § Test vectors; HELLO sub-revision → model; device vs software rotation per spec; RGB565 LE/BE rect bitmaps from dirty rects
- **Dependencies:** T-2.2
- **Status:** completed (b21613c, 29e5e04)

#### T-2.5: Kipye rev D and WeAct
- **Files modified:** `crates/bezel-devices/src/{protocol,driver}/{kipye_rev_d,weact}.rs`
- **Acceptance:** vectors; brightness scaling (D sent twice); 64-byte packets; WeAct LE16 + 0x0A
- **Dependencies:** T-2.2
- **Status:** completed (d21f08e, 1392321)

#### T-2.6: Turing USB (0x1CBE)
- **Files modified:** `crates/bezel-devices/src/{protocol,driver}/turing_usb.rs`, `crates/bezel-devices/src/usb.rs`, `crates/bezel-devices/Cargo.toml`
- **Acceptance:** DES-CBC header vectors (PKCS#7), 512-byte packet, PNG ≤ 1 MiB else JPEG, brightness 0..102, nusb bulk transport behind the `Wire` trait
- **Dependencies:** T-2.2
- **Status:** completed (8d540c0, a390f70)

#### T-2.7: WCH (0x43A8)
- **Files modified:** `crates/bezel-devices/src/{protocol,driver}/wch.rs`
- **Acceptance:** DES-ECB command/reply vectors, 512-byte blocks with the 32-byte header, BGR888 frames, model query 0x40
- **Dependencies:** T-2.6 (shares `usb.rs`)
- **Status:** completed (4908926)

### Wave 3

#### T-2.8: Integration, catalog, docs, hardware validation
- **Files modified:** `crates/bezel-devices/src/connector.rs`, `crates/bezel-core/src/domain/catalog.rs`, `packaging/linux/60-bezel.rules`, `docs/reverse-engineering/*.md`, `CHANGELOG.md`, `README.md`
- **Acceptance:** connector routes every family; TURZX serial models in the catalog (+ udev); TURZX additions in the protocol docs; 8.8" marked hardware-validated; install-local
- **Dependencies:** T-2.4..T-2.7
- **Status:** completed (669cc13, 3ddd867, 9959a0a, c08a52e, fd0032c, d74fb81, 7755ea3)

## Execution
- Total tasks: 8
- Waves: 3

## Test requirements
- `cargo test --workspace --locked`; coverage ≥ 80% (`cargo llvm-cov`)
- Hardware: `bezel test-pattern` in the four orientations on the 8.8"
