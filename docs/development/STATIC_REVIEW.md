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
