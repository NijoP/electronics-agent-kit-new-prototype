# Static review log

Nothing here was compiled, type-checked or run (owner decision: no builds). Every entry is
**reviewed by reading only**; entries are per scope, one section per reviewer.

## apps/desktop/src (TypeScript/React) — `fm/eak-sr-front`

Scope: all 109 `.ts`/`.tsx` files, `package.json` scripts and dependencies, `api.ts` IPC bindings
against the `src-tauri` command registrations. Method: reading plus read-only scripts (relative
import and named-export resolution, unused imports under `noUnusedLocals`, JSX props against declared
prop types, `Command` variants against `eak-runtime`'s `Command`). No `tsc`, `npm`, `cargo` or build was run.

| File | Error | Root cause | Fix |
|---|---|---|---|
| `src/api.ts` | `call<LockState>("project_lock_state", …)` is not assignable to `IpcCommand`; the `tests/ipc.rs` name-list check would also fail | `project_lock_state` was added to `IPC_COMMANDS` in `src-tauri/src/lib.rs` and to `api` but not to the TS `IPC_COMMANDS` list that types `call` | added `"project_lock_state"` to `IPC_COMMANDS` |

Checked and consistent (reviewed-only):

- Every IPC command name in TS (`IPC_COMMANDS`, `TERMINAL_IPC`) equals the Rust lists; every command
  argument key matches the Rust parameter name, and every command carries `rename_all = "snake_case"`
  where it has multi-word arguments (terminal commands with single-word arguments need none).
- All relative imports resolve and every named import exists as an export; no unused imports.
- Every package imported in `src` is listed in `package.json` (`occt-import-js` is typed by
  `src/occt-import-js.d.ts`).
- `LockState` in `types.ts` matches the serde shape of `eak_runtime::store::LockState`.
- Every `Command` `type` literal the UI sends exists in both the TS union and the Rust enum.

Not covered (needs a real `tsc`/test run): structural typing of large object literals, generic
inference and strict-null narrowing inside the big views (`PcbView`, `SchematicView`, `Workspace`,
`Viewer3D`).

## Rust kernel crates (eak-core, eak-runtime, eak-engineering, eak-validation, eak-manufacturing, eak-simulation) — `fm/eak-sr-kernel`

Reviewed-only, **not compiled**. Method: reading every `src`/`tests` file plus `rustfmt --check` (parse only) and grep across
the workspace for consumers. Checked: `mod` declarations vs files, every `use` path, struct fields and fn signatures at call
sites, enum-match exhaustiveness, serde derives on IPC types, Cargo.toml dependencies vs `Cargo.lock`, ureq 3.5.2 calls and
`live-http` gating. **No hard compile error was found by reading.**

| File | Error | Root cause | Fix |
|---|---|---|---|
| `eak-engineering/src/placement_intel.rs:15` | unused import `to_mm` (error under `-D warnings`) | imported, never used | removed |
| `eak-engineering/src/placement_intel.rs:335` | clippy `unnecessary_lazy_evaluations` | `.then(\|\| cmp)` | `.then_some(cmp)` |
| `eak-validation/src/sourcing.rs:51` | clippy `manual_range_contains` | `len >= 2 && len <= 13` | `(2..=13).contains(&len)` |
| `eak-validation/src/drc.rs:465` | clippy `uninlined_format_args` | `format!("zone {} …", z)` | `format!("zone {z} …")` |
| `eak-manufacturing/tests/distributor_fabricator.rs:1` | would not compile with `--no-default-features` | imports `UreqFetch`, which exists only under `live-http` | `#![cfg(feature = "live-http")]` |
| 9 files (engineering: `interactive`, `placement_intel`, `placer`, `router`, `routing_intel`; validation: `report`, `state`; manufacturing: `provider`, `sourcing`) | `cargo fmt --check` would fail | formatting drift | `rustfmt` (whitespace only) |

Not settled by reading (watch on the first real build): inference of `.sum()`/`.collect()` in `routing_intel::diff_pairs` and
`placement_intel::airwire`; tuple-of-closure coercion in `eak-runtime/tests/schematic_edit.rs:244`; `net_of` closure lifetime in
`eak-simulation/src/run.rs`; `Option::is_none_or`/`usize::is_multiple_of` vs `rust-version = "1.80"` (clippy `incompatible_msrv`);
pinned SHA-256 values and file counts in manufacturing tests and the `builtin_m1_parts.json` golden fixture were not re-hashed.

## Rust: eak-intelligence, eak-harness, eak-terminal, apps/desktop/src-tauri — `fm/eak-sr-agents`

Reviewed-only, **not compiled**. Method: reading the source and the new `tests/` of each crate against the real APIs they
call: the registry sources of `portable-pty` 0.9.0, `ureq` 3.5.2 and `keyring` 3.6.3 (read, not built), and the workspace
crates' own signatures (`eak-runtime::Engine`/`store`/`index`, `eak-core` IR, `eak-engineering`, `eak-validation`,
`eak-manufacturing::provider`, `eak-simulation`). Checked: `mod` declarations vs files, every `use` path, struct fields and
function signatures at call sites, enum-match exhaustiveness, serde derives on IPC types, `Cargo.toml` dependencies and
features vs `Cargo.lock` (all present, no hand-merge gap found), `include_str!`/`include` paths, embedded JSON
(`discipline_models.json` parsed with `jq` against its Rust types), struct-literal field visibility across modules, borrow and
closure-capture patterns, `#[tauri::command]` attributes and the `generate_handler!` list against `IPC_COMMANDS`,
`TERMINAL_COMMANDS` and `src/api.ts` (every TS command name is registered; every argument key is snake_case where the Rust
command sets `rename_all = "snake_case"`), `tauri.conf.json` and `capabilities/default.json`.

| File | Error | Root cause | Fix |
|---|---|---|---|
| `apps/desktop/src-tauri/src/lib.rs:927` | **hard compile error** (E0308, mismatched array length): `IPC_COMMANDS: [&str; 54]` initialised with 55 names | `project_lock_state` was added to the list (ADR-0029) without raising the length | length `54` -> `55` (the TS list was fixed separately by `fm/eak-sr-front`) |
| `apps/desktop/src-tauri/src/lib.rs:843` | clippy `needless_borrow` (error under `-D warnings`): `merged_library(&project, …)` where `project` is already `&BTreeMap` | borrow of a reference | `merged_library(project, …)` |
| `apps/desktop/src-tauri/src/terminal.rs:65` | clippy `let_and_return` (error under `-D warnings`) in `locate` | `let exe = if … ; exe` | return the `if` expression directly |
| `crates/eak-harness/src/rework.rs:238,239,1066` | `Option<&str> != Some(&String)` relies on `&String -> &str` coercion inside `Some(..)` on the right of a binary operator | comparison written with a borrowed `String` | `Some(x.as_str())` (same meaning, no inference needed; not proven to be an error) |

Checked and consistent (reviewed-only): `portable-pty` 0.9 (`process_group_leader`, `ChildKiller`, `Child + Send + Sync`,
`ExitStatus::signal`, `take_writer`, `try_clone_reader`); `ureq` 3.5.2 (`Agent::config_builder`, `timeout_global`,
`http_status_as_error`, `Body::into_with_config().limit().reader()`, `with_config().read_to_string()`, `Error` variants); `keyring`
3 features and `Entry` API; `Manager`/`Send + Sync` across `thread::spawn`; Tauri 2 command shapes (`async` commands with
`AppHandle` + `spawn_blocking`, `State<'_, T>`, `tauri::ipc::Response`, `app.manage` order in `setup`); `Fn`/`FnMut` closure
captures in the provider streaming paths; every `pub(crate)` field and function used across modules; `serde` untagged/flatten/
adjacently-tagged shapes; serde key shapes between `PortableConfig`, `ModelSettings` and `src/api.ts`.

Not settled by reading (watch on the first real build, none is a known error):

- `clippy::incompatible_msrv`: `Option::is_none_or` (1.82) is used in `eak-harness` (`design.rs`, `rework.rs`),
  `eak-intelligence` (`adaptor.rs`, `readonly.rs`, `domain/*`) while the workspace declares `rust-version = "1.80"`; the installed
  toolchain is newer. Raising `rust-version` is a shared-config decision (also covers `usize::is_multiple_of` noted by the kernel review).
- `eak-terminal/src/proc_info.rs`: `agent_from_argv` is used outside tests only by the Linux `agent`, so on macOS/Windows it
  is `dead_code` (a warning, an error only with `-D warnings` on those targets).
- Closure reborrow of a `&mut Value` parameter called repeatedly (`anthropic::StreamState::on_data`, `append`) and
  struct-literal reborrows of `&mut Engine`/`&mut dyn Approver` into `Ctx` (`agent.rs`) are valid by the reborrow rules but were not compiled.
- Linux builds need the system `libdbus-1` development package for `keyring`'s `sync-secret-service` (build-time, not a Rust error).
- The new tests (`tests/*.rs`, `#[cfg(test)]` modules) were read for API and type agreement only; none was run. Tauri 2 argument
  casing for single-word terminal arguments and `app.state::<Session>()` inside `terminal_state` need a window run (already in `NEXT_ACTION.md`).
