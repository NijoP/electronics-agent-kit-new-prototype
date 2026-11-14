# Next action

1. When the testing freeze lifts: `cargo fmt`, `clippy`, `cargo test --workspace --release`, `npm test`, fix integration fallout (harness + terminal + multi-project merge points: `apps/desktop/src-tauri/src/lib.rs`, `workspace.rs`, `crates/eak-terminal/src/lib.rs` `set_context`/`create_space`).
2. Runtime-check the Phase 2 wiring (static review only so far): in an EAK terminal on an open project, `env | grep EAK_` shows `EAK_PROJECT_DIR`, `EAK_PROJECT_ID`, `EAK_SESSION_ID`, `EAK_HARNESS_SOCKET`, `EAK_HARNESS_TOKEN_FILE`; `eak exec ...` routes `app` and the open editor re-renders within ~1.5 s.
3. Needs a compiler to settle: Tauri 2 argument casing for the remaining terminal commands (all single-word args today, so unaffected); whether `app.state::<Session>()` inside `terminal_state` resolves before `setup` returns (it is managed earlier in the same closure); `Cargo.lock`/`package-lock.json` hand-merge.
4. Trial Claude Code, OpenCode and Pi through `eak mcp` / `eak` before listing any as supported.
5. Cut a release built from this tree; publish Windows/macOS packages or keep them labelled unavailable.
