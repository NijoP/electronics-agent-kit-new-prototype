// Real-time DRC (Phase 1 slice C5): while enabled, each new revision is checked once edits
// pause. The run is the same engine `run_check` as Run DRC, on that revision's snapshot, and does
// not block editing. A result for an older revision than the one already shown is dropped by the
// caller; a run the engine refuses (the project moved on meanwhile) is retried by the next
// revision's run.
import { useEffect } from "react";
import type { CheckResult } from "./types";

export const RT_DRC_DEBOUNCE_MS = 800;

export function useRealtimeDrc(
  enabled: boolean,
  revision: number,
  drcRevision: number | undefined,
  run: () => Promise<CheckResult>,
  onResult: (r: CheckResult) => void,
  onError: (message: string) => void,
) {
  useEffect(() => {
    if (!enabled || drcRevision === revision) return;
    let live = true;
    const timer = setTimeout(() => {
      run()
        .then((r) => live && onResult(r))
        .catch((e) => live && onError(`Real-time DRC did not run on revision ${revision}: ${String(e).replace(/^Error: /, "")}`));
    }, RT_DRC_DEBOUNCE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // The callbacks are recreated on every render; the run depends only on these three values.
  }, [enabled, revision, drcRevision]);
}
