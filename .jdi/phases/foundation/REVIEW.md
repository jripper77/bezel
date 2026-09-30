# Phase 1: Review  (slug: foundation)

**Verdict:** APPROVED_PENDING_MANUAL

Gates run by `jdi-reviewer-bezel` rules on a clean worktree of the phase head (`80270f2` + docs), bash on Fedora 44, Rust 1.98.1.

## Gates
| Gate | Status | Details |
|---|---|---|
| Build | PASS | `cargo build --workspace --locked` |
| Tests | PASS | 28 passed, 0 failed, 0 ignored |
| Coverage | PASS | 84.67% lines (TOTAL; `main.rs`/`build.rs` excluded), threshold 80% |
| Lint | PASS | `cargo fmt --check` + `clippy -D warnings`; no `#[allow]` |
| Hexagonal/Safety/Protocol/Hygiene | PASS | 5.1 core deps = thiserror only; 5.2 no I/O/threads/cfg in core; 5.3 ports only in core; 5.5 no `unsafe`; 5.6 unwrap/expect only inside `#[cfg(test)]`; 5.9 `/dev/tty*`/`COM*` in core appear only in doc comments and test fixtures (judged OK); 5.11 `cargo audit` clean with the two documented exceptions |
| Consistency | PASS | commits scoped `foundation`; D-2026-09-30-foundation-1..5 respected (GPL-3.0+, pin 71f8b07 + written Sonar waiver, catalog in core, studio from v0.1.0, sanitized docs) |
| UI Validation | SKIPPED | `has_frontend` not declared yet (turned on in phase `studio-app`); the app's own suite passed anyway: 15 unit + 6 Playwright, 0 axe critical/serious |
| DoD | PASS_PENDING_MANUAL | 7/7 auto PASS, 3 manual pending |

## Blockers
- none

## Warnings
- Sonar waived in writing until the SonarQube Cloud project `slipalison_bezel` and the `SONAR_TOKEN` secret exist (D-2026-09-30-foundation-2).
- `pipeline.yml` pinned to `71f8b07`: github-workflows #15 landed on `pacotes-tauri-release`, not `main`.

## DoD Checklist (gate 8)

| # | Criterion | Source | Type | Status | Evidence |
|---|---|---|---|---|---|
| 1 | `cargo test --workspace` exits 0 | PROJECT | Auto | PASS | 28 passed |
| 2 | Coverage >= 80% of lines | PROJECT | Auto | PASS | 84.67% |
| 3 | No TODO/FIXME without issue | PROJECT | Auto | PASS | `git grep` empty |
| 4 | `bezel devices --json` lists the fake Turing 8.8" | CONTEXT | Auto | PASS | `devices_json_lists_fake_turing_88` 1 passed |
| 5 | MCU + SoC grouped into one screen | CONTEXT | Auto | PASS | `discovery::tests::groups_turing_88_mcu_and_soc` 1 passed |
| 6 | Local install puts a working `bezel` in `~/.local/bin` | CONTEXT | Auto | PASS | `bezel 0.0.0-dev.9+f0fbd8b` |
| 7 | CI of the pushed `main` head is green | CONTEXT | Auto | PASS | run 36713677654 success (attempt 2), release v0.1.0 |
| 8 | CHANGELOG updated per release | PROJECT | Manual | MANUAL_REQUIRED | suggested: `## [Unreleased]` entries present; the pipeline released v0.1.0 |
| 9 | README describes current behavior | PROJECT | Manual | MANUAL_REQUIRED | suggested: README documents `bezel devices`, install and udev |
| 10 | `bezel devices` shows the real 8.8" with MCU and SoC | CONTEXT | Manual | MANUAL_REQUIRED | suggested: output in SUMMARY.md § Hardware validation |

**Totals:** 10 items | Auto: 7 (7 PASS, 0 FAIL) | Manual: 3 pending

**Manual confirmation required:** run `/jdi-confirm-dod foundation`, then `/jdi-ship foundation`.

## Recommendation
Approve. Confirm the three manual items (the hardware one was observed by the agent; the user should see it on their own screen) and ship.
