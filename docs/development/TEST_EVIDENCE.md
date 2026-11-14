# Test evidence

Labels: *runtime-verified* · *build/test-verified* · *code-reviewed only* (see `STATUS.md`).

| Area | Level | Evidence |
|---|---|---|
| Engine, validation, manufacturing, editors (jsdom), IDE shell at #49 | build/test-verified (before the freeze) | `STATUS_LOG.md`, `TEST_RESULTS.md`, `docs/qa/E2E_RESULTS.md` (older screenshots predate the IDE shell) |
| Terminal engine PTY tests | runtime-verified on Linux before the override | `crates/eak-terminal/tests/pty.rs` (later edits not re-run) |
| Terminal UI, workspace registration, IPC test fix | code-reviewed only | |
| Harness crate, CLI, MCP, app endpoint | code-reviewed only | `crates/eak-harness/tests/e2e.rs` written, never run |
| Provider catalog, streaming | code-reviewed only | |
| This integration | code-reviewed only | only `git diff --check`, `bash -n`, JSON syntax and grep checks ran |

Known risk: `Cargo.lock` and `package-lock.json` were merged by hand; first compile may rewrite them.

Phase 2 wiring (`fm/eak-p2-wiring`): code-reviewed only. New test `spaces_on_the_project_folder_are_bound_to_the_project_id` (`crates/eak-terminal/tests/pty.rs`) written, never run. Checks run: `git diff --check`, grep of IPC lists (frontend `IPC_COMMANDS`+`TERMINAL_IPC` = backend `IPC_COMMANDS`+`TERMINAL_COMMANDS` = `generate_handler!` registrations), Cargo deps vs crate `use` paths.
