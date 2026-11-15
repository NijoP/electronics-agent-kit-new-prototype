# ADR-0029: Typed project lock state and safe takeover

Status: accepted (code-reviewed only, never compiled)

## Context
`.lock` held only a pid. A failed open returned the string "project is locked by process N", the UI
showed it raw, and `take_over_lock` deleted the lock unconditionally, so a second window or a live
session could be overridden (two writers on one project).

## Decision
- `eak_runtime::store::LockState` (serde-tagged `state`): `unlocked`, `open_elsewhere{this_app}`,
  `interrupted{pending_operations}`, `unverifiable{reason}`, `unavailable{reason}`.
- `inspect_lock(dir)` is read-only. Interrupted means: same host (recorded second line of `.lock`;
  old pid-only files are judged on this host) and the pid is provably not running (`/proc` on Linux,
  `kill -0` on other Unix, `tasklist` on Windows). A reused pid reads as alive, the safe side.
- `acquire_lock(dir, take_over)` removes a lock only for `interrupted`; live, foreign-host, malformed
  or unjudgeable locks are never overridden. Recovery of journalled operations is unchanged
  (`recover` on open reconciles pending entries; unknown outcomes still block writes).
- New IPC command `project_lock_state(path)` (read-only). `open_project` keeps its signature; its
  error text no longer contains process ids.
- UI: Project Home explains the state in plain words and offers "Recover and open" (explicit
  confirmation) only for `interrupted`.

## Consequences
Shared type added (`LockState`, TypeScript mirror in `types.ts`). Not verified at runtime.
