# Phase 4: Motor de renderização — Plan  (slug: render-engine)

## Goal
Modelo de tema, renderer WYSIWYG, formato nativo, importadores e runtime headless.

## Locked decisions (from CONTEXT.md)
- D-2026-09-30-render-engine-1..5

## Tasks

### Wave 1

#### T-4.1: Core theme model, clock, histories, runtime and ports
- **Specialist:** jdi-doer-bezel
- **Files modified:** `crates/bezel-core/src/domain/{theme,clock,history,mod}.rs`, `crates/bezel-core/src/app/{mod,runtime}.rs`, `crates/bezel-core/src/ports/mod.rs`
- **Acceptance:** model of D-1; `format_clock` (pt-BR/en names); `Histories` with gaps and adoption after edits; `FrameRenderer`, `ThemeStore`, `RenderContext`; `ThemeRuntime::{frame, show, replace}`
- **Dependencies:** none
- **Test:** `cargo test -p bezel-core`
- **Status:** completed (58e2518, 73bcd39)

### Wave 2 (parallel)

#### T-4.2: bezel-render
- **Files modified:** `crates/bezel-render/**` (new), root `Cargo.toml`, `Cargo.lock`
- **Acceptance:** `SkiaRenderer` implements `FrameRenderer` for every element kind and background, gradients, opacity, fits, GIF frame by time, cosmic-text shaping with bundled/system fonts; golden tests; ≤ 15 ms for a typical 480x1920 theme in release
- **Dependencies:** T-4.1
- **Status:** completed (96f7d4b, d3a4158)

#### T-4.3: bezel-themes native format
- **Files modified:** `crates/bezel-themes/src/{lib,native,dto}.rs` (new crate), root `Cargo.toml`, `Cargo.lock`
- **Acceptance:** `FsThemeStore` implements `ThemeStore` for `.bezeltheme` zips and folders; versioned DTOs; round-trip test
- **Dependencies:** T-4.1
- **Status:** completed (6e6cbc8, d3a4158)

#### T-4.4: Importers (Python YAML, TURZX .turtheme)
- **Files modified:** `crates/bezel-themes/src/import/**`, `crates/bezel-themes/tests/import_corpus.rs`
- **Acceptance:** both importers map every element kind the model has and report what they drop; ignored corpus test over the local TURZX and Python theme folders
- **Dependencies:** T-4.3
- **Status:** completed (504ab76)

### Wave 3

#### T-4.5: CLI render/run/import + bundled themes
- **Files modified:** `crates/bezel-cli/**`, `themes/**`
- **Acceptance:** `bezel render <theme> -o out.png`, `bezel run <theme>` (Ctrl+C releases the screen), `bezel import <src> -o <dst>`; bundled themes for the main panel sizes; `ThemeRuntime` integration tests with the fakes
- **Dependencies:** T-4.2, T-4.3
- **Status:** completed (cff8794)

## Execution
- Total tasks: 5
- Waves: 3

## Test requirements
- `cargo test --workspace --locked`, coverage ≥ 80%
- Hardware: `bezel run` on the 8.8" for one minute
