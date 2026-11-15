# ADR-0031: Typed revision-changed event for live synchronisation

Status: accepted (2026-11-15)

## Context
A coding agent in an EAK terminal changes the design through the `eak` CLI or MCP server. While
the desktop app has the project open those calls run on the app's own engine (ADR-0018), so the
engine commit is already single-writer and revision-checked. The editor only learned of it by
polling `events_since` every 1.5 s, and every derived view (airwires, classification) refetched on
any revision change, whatever had moved.

## Decision
* **Shared contract (new):** the desktop emits the Tauri event `eak://revision-changed` after a
  harness call moves the revision, with the payload `RevisionChange`
  (`apps/desktop/src-tauri/src/sync.rs`, mirrored in `apps/desktop/src/revisionSync.ts`):
  `project_id`, `dir`, `revision_before`, `revision_after`, `source` (`harness`), `session_id`
  (the caller's `EAK_SESSION_ID`) and `areas` (top-level IR sections that changed).
* `areas` is derived from the engine's own journalled change list for the revisions in
  `(before, after]`, never from the tool request, so it cannot disagree with what was applied. An
  empty list means "unknown": views refetch everything.
* The event carries no design data. The workspace fetches one `current_project` snapshot and
  applies it only if it is the same project and strictly newer than the revision on screen; events
  for another project or at an already-shown revision are dropped (stale results are rejected).
* Derived views refetch only when an area they read changed (airwires: `schematic`, `pcb`;
  classification: `schematic`, `library`, `requirements`, `constraints`), and only when the change
  is exact (it starts at the revision the screen showed).
* No new write path: the mutation is still `Engine::execute` on the app's engine. The 1.5 s event
  poll stays as the fallback if an event is missed.

## Consequences
* Terminal edits appear in the schematic, PCB and 3D views without polling delay, and no terminal
  or design state is dropped when switching workspaces (the workspaces stay mounted).
* UI-originated commits do not emit the event: the caller already receives the response and
  refreshes. AI proposal accept/rollback and undo from the UI follow the same rule.
* Status: code-reviewed only; the tests (`apps/desktop/src-tauri/tests/live_sync.rs`,
  `apps/desktop/src/liveSync.test.tsx`) run in CI.
