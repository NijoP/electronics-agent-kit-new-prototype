// Live synchronisation with the engine: the backend emits one typed event after it commits a
// revision the UI did not make itself (a terminal agent's harness call; src-tauri/src/sync.rs).
// Pure decisions live here so they are testable; the Workspace applies them.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Must match `REVISION_EVENT` in src-tauri/src/sync.rs. */
export const REVISION_EVENT = "eak://revision-changed";

/** Must match `RevisionChange` in src-tauri/src/sync.rs. */
export interface RevisionChange {
  project_id: string;
  dir: string | null;
  revision_before: number;
  revision_after: number;
  /** `harness` for terminal-agent calls. */
  source: string;
  /** Terminal session id (`EAK_SESSION_ID`) of the caller, when it gave one. */
  session_id: string | null;
  /** Top-level IR sections that changed; empty means unknown (treat everything as changed). */
  areas: string[];
}

/** What the screen currently shows. */
export interface Shown {
  projectId: string;
  revision: number;
}

/** An event is stale when it is for another project or the screen already shows that revision or a newer one. */
export function isStale(ev: RevisionChange, shown: Shown): boolean {
  return ev.project_id !== shown.projectId || ev.revision_after <= shown.revision;
}

/** A fetched snapshot may replace the one on screen only if it is the same project and strictly newer. */
export function acceptsSnapshot(fetched: { projectId: string; revision: number }, shown: Shown): boolean {
  return fetched.projectId === shown.projectId && fetched.revision > shown.revision;
}

/**
 * The change that produced `revision`, kept only while it is exact: the event must start from the
 * revision the screen showed, otherwise earlier changes were missed and nothing can be said about
 * which areas moved (`null`: refetch everything).
 */
export function exactChange(ev: RevisionChange, shown: Shown): RevisionChange | null {
  return ev.revision_before === shown.revision ? ev : null;
}

/**
 * Whether a view reading `inputs` (IR sections) must refetch at `revision`. Without an exact
 * change for that revision, or with no area information, it must.
 */
export function needsRefetch(change: RevisionChange | null, revision: number, inputs: readonly string[]): boolean {
  if (!change || change.revision_after !== revision || change.areas.length === 0) return true;
  return change.areas.some((a) => inputs.includes(a));
}

export function onRevisionChange(f: (ev: RevisionChange) => void): Promise<UnlistenFn> {
  return listen<RevisionChange>(REVISION_EVENT, (e) => f(e.payload));
}
