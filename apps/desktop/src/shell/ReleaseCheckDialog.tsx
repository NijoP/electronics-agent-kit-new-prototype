// Release check (slice E7): a read-only view of the release gate of the current revision. Its
// rows are the ERC, DRC, MECH and DFM reports saved for exactly this revision and the fail-closed
// engineering gates of eak-validation (gates.rs), as src-tauri computes them with the export's own
// blocking rules. Opening the view runs no check: a report not saved for this revision is NOT_RUN,
// and a required row that is NOT_RUN, FAIL or UNKNOWN keeps every output of the revision DRAFT.
// "Run release checks" saves the four reports as evidence (what every export does first); it
// changes no design data. The wording never certifies the board for fabrication.
import { useEffect, useState } from "react";
import { api } from "../api";
import type { ReleaseCheck, ReleaseGate } from "../types";

const message = (e: unknown) => String(e).replace(/^Error: /, "");

const PILL: Record<ReleaseGate["state"], string> = { PASS: "ok", FAIL: "bad", UNKNOWN: "warn", NOT_RUN: "warn", DISABLED: "" };

/** Whether a row counts towards the release. */
export const counts = (g: ReleaseGate) => (g.state === "DISABLED" ? "not counted" : g.required ? "required" : "advisory");

/** The status line: DRAFT unless the engine says every required gate passes. */
export const statusLine = (c: ReleaseCheck) =>
  c.status === "DRAFT"
    ? "DRAFT — outputs of this revision are not for fabrication"
    : "Every required gate passes — outputs of this revision are labelled internally verified release candidate";

export const NOT_CERTIFIED =
  "Gates are necessary, not sufficient: passing them means the implemented checks found no violation in this revision. They never certify the board for fabrication; independent engineering review and the fabricator's own DFM review remain required.";

export function ReleaseCheckDialog({ revision, busy, onClose }: { revision: number; busy: boolean; onClose: () => void }) {
  const [check, setCheck] = useState<ReleaseCheck | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  // Read-only: the engine reads the saved reports and evaluates the gates, per revision.
  useEffect(() => {
    let live = true;
    api
      .releaseCheck()
      .then((c) => {
        if (!live) return;
        setCheck(c);
        setError(null);
      })
      .catch((e) => {
        if (live) setError(message(e));
      });
    return () => {
      live = false;
    };
  }, [revision]);

  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      setCheck(await api.runReleaseChecks());
    } catch (e) {
      setError(message(e));
    } finally {
      setRunning(false);
    }
  };
  const blocked = busy ? "An engine command is running" : undefined;
  const required = check?.gates.filter((g) => g.required) ?? [];
  const open = required.filter((g) => g.state !== "PASS");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal output-dialog" role="dialog" aria-label="Release check" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>Release check</h2>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
        <p className="muted hint">
          Read-only view of the release gate for revision {revision}. Opening it runs nothing: ERC, DRC, MECH and DFM count only when their report was saved for exactly this revision. A required gate that is NOT_RUN, FAIL or UNKNOWN keeps every output DRAFT.
        </p>
        <div className="row">
          <button type="button" className="primary" disabled={running || !!blocked} title={blocked ?? "Run ERC, DRC, MECH and DFM on this revision and save each report as evidence (no design data changes)"} onClick={() => void run()}>
            {running ? "Running checks…" : "Run release checks"}
          </button>
        </div>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {check && check.revision !== revision && (
          <p className="error">
            This view is for revision {check.revision}; the project is now at revision {revision}. Reading it again…
          </p>
        )}
        {!check && !error && <p className="muted">Reading the release gate…</p>}
        {check && (
          <>
            <section aria-label="Release status">
              <p>
                <strong>{statusLine(check)}</strong>
              </p>
              <p className="muted">
                {open.length === 0
                  ? `All ${required.length} required gates pass for revision ${check.revision}.`
                  : `${open.length} of ${required.length} required gates do not pass for revision ${check.revision}.`}
              </p>
            </section>
            <section aria-label="Gates">
              <table className="bom" aria-label="Release gates">
                <thead>
                  <tr>
                    <th>Gate</th>
                    <th>State</th>
                    <th>Counts</th>
                    <th>Reasons and evidence</th>
                  </tr>
                </thead>
                <tbody>
                  {check.gates.map((g) => (
                    <tr key={g.id} data-gate={g.id}>
                      <td>{g.title}</td>
                      <td className="gate-state">
                        <span className={`pill ${PILL[g.state]}`}>{g.state}</span>
                      </td>
                      <td>{counts(g)}</td>
                      <td>
                        {g.reasons.map((r, i) => (
                          <div key={`r${i}`}>{r}</div>
                        ))}
                        {g.evidence.map((e, i) => (
                          <div key={`e${i}`} className="muted">
                            {e}
                          </div>
                        ))}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
            {check.blocking.length > 0 && (
              <ul aria-label="Blocking reasons">
                {check.blocking.map((b) => (
                  <li key={b} className="error">
                    {b}
                  </li>
                ))}
              </ul>
            )}
            <p className="muted">{NOT_CERTIFIED}</p>
            <p className="muted">Not verified: {check.not_verified.join("; ")}.</p>
          </>
        )}
      </div>
    </div>
  );
}
