# Next action

1. When the testing freeze lifts: `cargo fmt`, `clippy`, `cargo test --workspace --release`, `npm test`, fix integration fallout (harness + terminal + multi-project merge points: `apps/desktop/src-tauri/src/lib.rs`, `workspace.rs`, `crates/eak-terminal/src/lib.rs` `set_context`/`create_space`).
2. Runtime-check the Phase 2 wiring (static review only so far): in an EAK terminal on an open project, `env | grep EAK_` shows `EAK_PROJECT_DIR`, `EAK_PROJECT_ID`, `EAK_SESSION_ID`, `EAK_HARNESS_SOCKET`, `EAK_HARNESS_TOKEN_FILE`; `eak exec ...` routes `app` and the open editor re-renders within ~1.5 s.
3. Needs a compiler to settle: Tauri 2 argument casing for the remaining terminal commands (all single-word args today, so unaffected); whether `app.state::<Session>()` inside `terminal_state` resolves before `setup` returns (it is managed earlier in the same closure); `Cargo.lock`/`package-lock.json` hand-merge.
4. Trial Claude Code, OpenCode and Pi through `eak mcp` / `eak` before listing any as supported.
5. Cut a release built from this tree; publish Windows/macOS packages or keep them labelled unavailable.
6. Existing-project rework (directive §3H, from DESK `desk/rework/*`): read-only inspection, project identity check, immutable baseline (checkpoint), protected vs modifiable objects, dependency-aware repair plan as one typed `Batch`, explicit approval, readback and post-mutation gates. Not started.
7. When builds resume: run the new gate/power/units tests (`eak-validation/src/gates.rs`, `eak-engineering/src/power.rs`, `eak-core/src/units.rs`, `eak-terminal/tests/pty.rs::session_environment_is_not_persisted`) and confirm `end_to_end.rs`/`multilayer.rs` still reach the release-candidate label under ADR-0021.

8. Performance work (code-reviewed only): run the `resources.rs` tests and `canvasFrame.test.tsx`, then `docs/development/PERFORMANCE_VERIFICATION_PLAN.md` (not performed). Settle by compiling: Tauri 2 `async fn` commands taking `AppHandle`, `tauri::async_runtime::spawn_blocking` result type, `tokio` "sync" edge added to `Cargo.lock` by hand.
