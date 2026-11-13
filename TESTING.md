# Testing

| Level | Where | Command |
|---|---|---|
| Unit | `crates/*/src/**` `#[cfg(test)]` | `cargo test --workspace` |
| Contract / integration | `crates/eak-runtime/tests/engine.rs`, `process.rs` | `cargo test -p eak-runtime` |
| End-to-end engineering slice | `crates/eak-manufacturing/tests/end_to_end.rs` | `cargo test -p eak-manufacturing --release` |
| Policy | `scripts/check-no-python.sh` | run directly |

Rules: every bug fix gets a regression test; tests never depend on network, AI providers or
DESK; engineering reference vectors cite their source (see `docs/engineering/ELECTRICAL_RULES.md`).
Use `--release` for the end-to-end slice (routing is slow in debug builds).

Reference results are recorded per milestone in STATUS.md.
