# Testing

| Level | Where | Command |
|---|---|---|
| Unit | `crates/*/src/**` `#[cfg(test)]` | `cargo test --workspace` |
| Contract / integration | `crates/eak-runtime/tests/engine.rs`, `process.rs` | `cargo test -p eak-runtime` |
| End-to-end engineering slice | `crates/eak-manufacturing/tests/end_to_end.rs` | `cargo test -p eak-manufacturing --release` |
| Desktop IPC contract (headless) | `apps/desktop/src-tauri/tests/ipc.rs` | `cargo test -p eak-desktop --release` |
| Desktop UI (vitest + jsdom, headless) | `apps/desktop/src/**/*.test.ts(x)` | `npm test` in `apps/desktop` |
| Desktop production build | `apps/desktop` | `npm run tauri -- build --no-bundle` |
| Policy | `scripts/check-no-python.sh` | `bash scripts/check-no-python.sh` |
| LCSC provider, BOM, JLCPCB prep (local test HTTP server, never the live site) | `crates/eak-manufacturing/tests/lcsc_jlcpcb.rs`, `apps/desktop/src-tauri/tests/parts_ipc.rs`, `apps/desktop/src/parts.test.tsx` | `cargo test -p eak-manufacturing --release --test lcsc_jlcpcb`, `cargo test -p eak-desktop --release --test parts_ipc` |
| Sourcing (offline fixtures) | `crates/eak-manufacturing/tests/sourcing.rs`, `crates/eak-validation/src/sourcing.rs` | `cargo test -p eak-manufacturing --test sourcing --release`, `cargo test -p eak-validation` |

Rules: every bug fix gets a regression test; tests never depend on network, AI providers or
DESK; engineering reference vectors cite their source (see `docs/engineering/ELECTRICAL_RULES.md`).
Use `--release` for the end-to-end slice (routing is slow in debug builds).

Supplier-facing coverage (research, LCSC sourcing, library integrity, JLCPCB preparation, failure
handling, offline operation) is tracked in `docs/engineering/INTEGRATION_TEST_MATRIX.md`. Sourcing
fixtures are synthetic; no network-dependent test exists.

Reference results are recorded per milestone in STATUS.md.
