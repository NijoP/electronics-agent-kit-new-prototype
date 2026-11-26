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
import { actorName, checkDeltas, conflictText, firstChanged, groupByActor, latestRevision, type ActivityItem, type EngineerRequest, type WeftConflict } from "./activity";
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
  // null until the first read returns; a failed read is shown as an error, never as "none".
  const [conflicts, setConflicts] = useState<WeftConflict[] | null>(null);
  const [conflictsError, setConflictsError] = useState<string | null>(null);
  const [queue, setQueue] = useState<EngineerRequest[] | null>(null);
  const [queueError, setQueueError] = useState<string | null>(null);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [working, setWorking] = useState(false);
  const [reload, setReload] = useState(0);
  useEffect(() => {
    let live = true;
    // Conflicts review every open proposal on a copy: one request at a time.
    let checking = false;
    const load = () => {
      api
        .activityTimeline()
        .then((xs) => {
          if (!live) return;
          setItems(xs);
          setError(null);
        })
        .catch((e) => live && setError(message(e)));
      api
        .weftEngineerQueue()
        .then((q) => {
          if (!live) return;
          setQueue(q);
          setQueueError(null);
        })
        .catch((e) => live && setQueueError(message(e)));
      if (checking) return;
      checking = true;
      api
        .weftConflicts()
        .then((c) => {
          if (!live) return;
          setConflicts(c);
          setConflictsError(null);
        })
        .catch((e) => {
          if (!live) return;
          setConflicts(null);
          setConflictsError(message(e));
        })
        .finally(() => {
          checking = false;
        });
    };
    load();
    const t = setInterval(load, POLL_MS);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [revision, reload]);

  // The engineer's answer goes on the WEFT bus to the proposal's author; Reject and Re-propose
  // then discard the proposal, only once the verdict is sent; a failure after sending says so
  // ("verdict sent to ..., but"). Nothing here applies a proposal (Approve opens the review).
  const answer = async (id: string, verdict: "approve" | "request_changes" | "reject", body: string, discard: boolean, done: string) => {
    setWorking(true);
    try {
      await api.weftEngineerVerdict(id, verdict, body, discard);
      setNote({ ok: true, text: done });
      if (verdict === "approve") onReview(id);
    } catch (e) {
      const m = message(e);
      setNote({ ok: false, text: /^verdict (sent|not sent)/.test(m) ? m : `Not sent: ${m}` });
    } finally {
      setWorking(false);
      setReload((n) => n + 1);
    }
  };
  const reject = (id: string) => answer(id, "reject", "Rejected by the engineer.", true, `Proposal ${id} rejected: discarded, and its agent was told on the WEFT bus.`);
  const repropose = (id: string) =>
    answer(id, "request_changes", `Propose again on revision ${revision}.`, true, `Proposal ${id} sent back: discarded, and its agent was asked on the WEFT bus to propose again on revision ${revision}.`);
  const off = busy || working;

  // A failed read shows its error only, never the last list as if it were current.
  const requests = queueError ? [] : (queue ?? []);
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
          {note && (
            <p role="status" className={note.ok ? "muted activity-note" : "explorer-error activity-note"}>
              {note.text}
            </p>
          )}
          <section className="activity-conflicts" aria-label="Conflicts">
            <h4>Conflicts among open proposals</h4>
            {conflictsError && (
              <p role="alert" className="explorer-error">
                Conflicts could not be checked: {conflictsError}
              </p>
            )}
            {conflicts === null && !conflictsError && <p className="muted">Checking open proposals for conflicts…</p>}
            {conflicts && conflicts.length === 0 && <p className="muted">None: no open proposal is stale or touches what another one touches.</p>}
            {conflicts?.map((c, i) => (
              <div key={`${c.kind}-${c.proposals.join("-")}-${i}`} className="activity-conflict" data-conflict={c.kind}>
                <p>
                  <strong>{conflictText(c)}</strong> <span className="muted">{c.resolve}</span>
                </p>
                {c.proposals.map((id) => (
                  <div key={id} className="activity-actions" data-conflict-proposal={id}>
                    <code>{id}</code>
                    <button type="button" onClick={() => onReview(id)}>
                      Review proposal
                    </button>
                    <button type="button" disabled title="EAK does not rebase a proposal (ADR-0051): send it back to be proposed again on the current revision">
                      Rebase
                    </button>
                    <button type="button" disabled={off} onClick={() => void repropose(id)} title="Discard it and ask its agent, on the WEFT bus, to propose again on the current revision">
                      Re-propose
                    </button>
                    <button type="button" className="danger" disabled={off} onClick={() => void reject(id)} title="Discard it and tell its agent on the WEFT bus">
                      Reject
                    </button>
                  </div>
                ))}
              </div>
            ))}
          </section>
          <section className="activity-requests" aria-label="Review requests for you">
            <h4>Review requests for you</h4>
            {queueError && (
              <p role="alert" className="explorer-error">
                Review requests could not be read: {queueError}
              </p>
            )}
            {queue === null && !queueError && <p className="muted">Reading review requests…</p>}
            {!queueError && queue?.length === 0 && <p className="muted">No agent is waiting for your review.</p>}
            {requests.map((r) => (
              <div key={r.proposal_id} className="activity-actions" data-review-request={r.proposal_id}>
                <span>
                  {actorName(r.from)} asks you to review <strong>{r.title || r.proposal_id}</strong>
                  {r.body ? `: ${r.body}` : ""}
                </span>
                <button type="button" className="primary" disabled={off} onClick={() => void answer(r.proposal_id, "approve", "Approved by the engineer.", false, `Approval sent to its agent: apply proposal ${r.proposal_id} in the review.`)} title="Tell its agent you approve, then open the review to apply it">
                  Approve
                </button>
                <button type="button" className="danger" disabled={off} onClick={() => void reject(r.proposal_id)}>
                  Reject
                </button>
              </div>
            ))}
          </section>
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
