# Next action

1. When the testing freeze lifts: `cargo fmt`, `clippy`, `cargo test --workspace --release`, `npm test`, fix integration fallout (harness + terminal + multi-project merge points: `apps/desktop/src-tauri/src/lib.rs`, `workspace.rs`).
2. Export `EAK_HARNESS_SOCKET` / `EAK_HARNESS_TOKEN_FILE` to terminal sessions.
3. Trial Claude Code, OpenCode and Pi through `eak mcp` / `eak` before listing any as supported.
4. Cut a release built from this tree; publish Windows/macOS packages or keep them labelled unavailable.
