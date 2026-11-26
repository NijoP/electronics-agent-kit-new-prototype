// Agent activity (ADR-0053): what every agent and the engineer did on this project, one timeline
// per actor, read from the project's journals (`activity_timeline`): committed revisions with the
// command, revision before → after and the ERC/DRC errors of the reports saved for those
// revisions; proposal events; WEFT tasks and messages; saved check reports. Re-read when the
// revision changes and every 2 s while open (WEFT messages do not move the revision). Actions go
// through the existing paths: a proposal opens the review dialog (Apply / Discard there), the
// newest revision is taken back by Edit › Undo, and Show on canvas selects and zooms to the first
// changed part or footprint.
import { useEffect, useState } from "react";
import { api } from "../api";
import { actorName, checkDeltas, firstChanged, groupByActor, latestRevision, type ActivityItem } from "./activity";
import { useEscape } from "./useEscape";

const message = (e: unknown) => String(e).replace(/^Error: /, "");
const time = (ms: number) => (ms ? new Date(ms).toLocaleTimeString() : "—");
const POLL_MS = 2000;

interface Props {
  revision: number;
  busy: boolean;
  onReview: (proposalId: string) => void;
  onUndo: () => void;
  onShow: (target: { editor: "schematic" | "pcb"; id: string }) => void;
  onClose: () => void;
}

export function ActivityDialog({ revision, busy, onReview, onUndo, onShow, onClose }: Props) {
  useEscape(onClose);
  const [items, setItems] = useState<ActivityItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const load = () =>
      api
        .activityTimeline()
        .then((xs) => {
          if (!live) return;
          setItems(xs);
          setError(null);
        })
        .catch((e) => live && setError(message(e)));
    void load();
    const t = setInterval(load, POLL_MS);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [revision]);

  const all = items ?? [];
  const latest = latestRevision(all, revision);
  const groups = groupByActor(all);
  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette eco-preview proposal-review" role="dialog" aria-modal="true" aria-label="Agent activity" onClick={(e) => e.stopPropagation()}>
        <h3>Agent activity (revision {revision})</h3>
        <div className="proposal-body">
          <p className="muted">From the project's journals only: revisions, proposals, WEFT messages and saved checks. Updated while open.</p>
          {error && (
            <p role="alert" className="explorer-error">
              {error}
            </p>
          )}
          {items && items.length === 0 && <p className="muted activity-empty">Nothing recorded yet: no revision, proposal, agent message or saved check in this project.</p>}
          {groups.map((g) => (
            <section key={g.actor} className="activity-actor" aria-label={`Activity of ${actorName(g.actor)}`} data-actor={g.actor}>
              <h4>
                {actorName(g.actor)} <span className="count">{g.items.length}</span>
              </h4>
              <ol className="activity-list">
                {g.items.map((it, i) => {
                  const deltas = checkDeltas(all, it);
                  const show = firstChanged(it);
                  const isLatest = latest !== null && it === latest && !/^(undo|redo)$/i.test(it.kind);
                  return (
                    <li key={`${it.source}-${it.op_id ?? it.proposal_id ?? ""}-${it.ts_ms}-${i}`} data-activity={it.source} data-kind={it.kind}>
                      <span className="muted">{time(it.ts_ms)}</span> <span className="pill">{it.source}</span> <strong>{it.kind}</strong> {it.summary}
                      {it.source === "revision" && it.revision_before !== null && (
                        <span className="muted">
                          {" "}
                          (r{it.revision_before} → r{it.revision_after})
                        </span>
                      )}
                      {deltas.map((d) => (
                        <span key={d.check} className="activity-delta" data-check={d.check}>
                          {" "}
                          {d.text}
                        </span>
                      ))}
                      <span className="activity-actions">
                        {it.proposal_id && it.source !== "check" && (
                          <button type="button" onClick={() => onReview(it.proposal_id!)}>
                            Review proposal
                          </button>
                        )}
                        {show && (
                          <button type="button" onClick={() => onShow(show)}>
                            Show on canvas
                          </button>
                        )}
                        {isLatest && (
                          <button type="button" onClick={onUndo} disabled={busy} title={busy ? "An engine command is running" : "Take this revision back (Edit › Undo, one undo step)"}>
                            Undo this revision
                          </button>
                        )}
                      </span>
                    </li>
                  );
                })}
              </ol>
            </section>
          ))}
        </div>
        <div className="dlg-actions">
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
