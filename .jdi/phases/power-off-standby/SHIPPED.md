shipped_at: 2026-10-03T23:12:15Z
verdict: APPROVED
by: alison amorim

## Learnings
- Prove Windows on the last code commit's CI `rust-windows` job (or a check-only stub toolchain): a local msvc clippy of the library crates misses app code whose types differ per OS (an `Inhibitor` with `Drop` only on Linux broke the studio's clippy).
- Never prove a wiring with a grep of source text (`RunEvent::Exit` also matched a comment): drive the real handler in a test with the dependency injected.
- For every "asks first" criterion write the refusal test first; a test asserting the happy overwrite locked a missing confirmation in place.
- Sequences that span two ports (brightness, then OPTIONS) need one ordered log in the fake; separate logs let the order invert unseen.
- Give DoD test counts as minimums plus `--exact` names, so a critic's extra test never breaks a proof.
