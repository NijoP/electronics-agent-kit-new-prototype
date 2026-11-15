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
| `eak-manufacturing/tests/lcsc_jlcpcb.rs:1` | would not compile with `--no-default-features` | imports `UreqFetch`, which exists only under `live-http` | `#![cfg(feature = "live-http")]` |
| 9 files (engineering: `interactive`, `placement_intel`, `placer`, `router`, `routing_intel`; validation: `report`, `state`; manufacturing: `provider`, `sourcing`) | `cargo fmt --check` would fail | formatting drift | `rustfmt` (whitespace only) |

Not settled by reading (watch on the first real build): inference of `.sum()`/`.collect()` in `routing_intel::diff_pairs` and
`placement_intel::airwire`; tuple-of-closure coercion in `eak-runtime/tests/schematic_edit.rs:244`; `net_of` closure lifetime in
`eak-simulation/src/run.rs`; `Option::is_none_or`/`usize::is_multiple_of` vs `rust-version = "1.80"` (clippy `incompatible_msrv`);
pinned SHA-256 values and file counts in manufacturing tests and the `builtin_m1_parts.json` golden fixture were not re-hashed.
