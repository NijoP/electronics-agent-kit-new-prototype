// Agent proposal review (slice S9, ADR-0051), the generalised form of the Update PCB review
// (EcoPreviewDialog): an agent session in propose mode stores its edits as a proposal and the
// project does not change. This dialog lists the project's proposals and shows EAK's review of
// one: every change grouped by the object it touches (part, net, footprint, rule…) with a checkbox
// per change, ERC and DRC counts before and after on a copy, and evidence labels. A step (one typed
// command) is the smallest unit Apply can leave out, so checking a row checks its step, and steps
// that use an object another step creates go together. Every selection change asks EAK to review
// it again; Apply is enabled only for the selection on screen, and sends it as one Batch at the
// reviewed revision (stale is refused by the engine): one revision, one undo step, a checkpoint.
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { ReviewTable } from "./ReviewTable";
import {
  buildProposalRows,
  countChange,
  groupProposalRows,
  LABEL_TEXT,
  proposalApplyState,
  selectionArg,
  toggleStep,
  type ApplyOutcome,
  type ProposalGroupBy,
  type ProposalReview,
  type ProposalSummary,
} from "./proposalReview";
import { useEscape } from "./useEscape";

const message = (e: unknown) => String(e).replace(/^Error: /, "");
const reviewable = (p: ProposalSummary) => p.status === "open" || p.status === "ready";

interface Props {
  /** The project's revision; a change reloads the list and the review. */
  revision: number;
  /** Proposal to show first (from the activity view); default the newest reviewable one. */
  initialId?: string;
  busy: boolean;
  /** Called after an accepted apply (the workspace refreshes and reports it). */
  onApplied: (outcome: ApplyOutcome) => void;
  onClose: () => void;
}

export function ProposalReviewDialog({ revision, initialId, busy, onApplied, onClose }: Props) {
  useEscape(onClose);
  const [list, setList] = useState<ProposalSummary[] | null>(null);
  const [id, setId] = useState<string | null>(null);
  const [review, setReview] = useState<ProposalReview | null>(null);
  // null until the first review names the steps: then every step is checked.
  const [selected, setSelected] = useState<Set<number> | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [by, setBy] = useState<ProposalGroupBy>("object");
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [working, setWorking] = useState(false);
  const [reload, setReload] = useState(0);

  // The list (read-only). The newest reviewable proposal is shown first, else the newest one.
  useEffect(() => {
    let live = true;
    api
      .proposalList()
      .then((l) => {
        if (!live) return;
        setList(l);
        setId((cur) => (cur && l.some((p) => p.id === cur) ? cur : initialId && l.some((p) => p.id === initialId) ? initialId : ([...l].reverse().find(reviewable) ?? l[l.length - 1])?.id ?? null));
      })
      .catch((e) => live && setError(message(e)));
    return () => {
      live = false;
    };
  }, [revision, reload]);

  // EAK's review of the checked steps, recomputed on a copy whenever the selection changes.
  // A request is (proposal, steps, revision, reload); checking every step after the first review
  // asks for the same one again, so it is not sent twice.
  const key = selected === null ? "all" : [...selected].sort((a, b) => a - b).join(",");
  const last = useRef("");
  useEffect(() => {
    if (!id) return;
    const steps = selected === null || !review || review.proposal.id !== id ? null : selectionArg(review.steps, selected);
    const request = JSON.stringify([id, steps, revision, reload]);
    if (request === last.current) return;
    last.current = request;
    // Only the answer to the latest request is shown (an older one may arrive later).
    const current = () => last.current === request;
    setReviewing(true);
    api
      .proposalReview(id, steps)
      .then((r) => {
        if (!current()) return;
        setReview(r);
        setError(null);
        if (selected === null) setSelected(new Set(r.selected));
      })
      .catch((e) => current() && setError(message(e)))
      .finally(() => current() && setReviewing(false));
    // `review` is read for its step list only; re-running on it would loop.
  }, [id, key, revision, reload]);

  const choose = (next: string) => {
    setId(next);
    setReview(null);
    setSelected(null);
    setNote(null);
    setConfirmDiscard(false);
  };

  const steps = review && review.proposal.id === id ? review.steps : [];
  const rows = useMemo(() => buildProposalRows(steps), [steps]);
  const chosen = selected ?? new Set<number>();
  const groups = groupProposalRows(rows, steps, by);
  const state = proposalApplyState(review && review.proposal.id === id ? review : null, chosen, reviewing);
  const open = review?.proposal.status === "open" || review?.proposal.status === "ready";
  const off = busy || working;

  const apply = async () => {
    if (!review || !id) return;
    setWorking(true);
    try {
      const out = await api.proposalApply(id, review.current_revision, state.steps);
      onApplied(out);
    } catch (e) {
      setNote({ ok: false, text: `Not applied: ${message(e)}` });
      setReload((n) => n + 1);
    } finally {
      setWorking(false);
    }
  };
  const discard = async () => {
    if (!id) return;
    setWorking(true);
    try {
      await api.proposalDiscard(id);
      setNote({ ok: true, text: "Proposal discarded: it stays listed for the record and can no longer be applied." });
      setConfirmDiscard(false);
      setReload((n) => n + 1);
    } catch (e) {
      setNote({ ok: false, text: `Not discarded: ${message(e)}` });
    } finally {
      setWorking(false);
    }
  };
  const exportReport = async () => {
    if (!id || !review) return;
    try {
      const file = await api.proposalExport(id, selectionArg(review.steps, chosen));
      setNote({ ok: true, text: `Report saved: ${file}` });
    } catch (e) {
      setNote({ ok: false, text: `Report not saved: ${message(e)}` });
    }
  };

  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette eco-preview proposal-review" role="dialog" aria-modal="true" aria-label="Review agent proposal" onClick={(e) => e.stopPropagation()}>
        <h3>Review agent proposal</h3>
        {/* Only the body scrolls, so the reason and the action row stay in the window. */}
        <div className="proposal-body">
        {error && (
          <p role="alert" className="explorer-error">
            {error}
          </p>
        )}
        {list && list.length === 0 && (
          <p className="muted proposal-empty">
            No agent proposals in this project. An agent session started in propose mode (eak --propose, or EAK_HARNESS_APPLY_MODE=propose) stores its edits here for you to review; nothing changes until you apply them.
          </p>
        )}
        {list && list.length > 0 && (
          <div className="eco-bar">
            <label>
              Proposal{" "}
              <select aria-label="Proposal" value={id ?? ""} onChange={(e) => choose(e.target.value)}>
                {list.map((p) => (
                  <option key={p.id} value={p.id}>
                    {`${p.title} — ${p.agent}, ${p.steps} step(s), ${p.stale ? "stale" : p.status}`}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Group by{" "}
              <select aria-label="Group changes by" value={by} onChange={(e) => setBy(e.target.value as ProposalGroupBy)}>
                <option value="object">Object</option>
                <option value="step">Step</option>
              </select>
            </label>
          </div>
        )}
        {review && review.proposal.id === id && (
          <>
            <p className="muted">
              By {review.proposal.agent} (session {review.proposal.session_id}) at revision {review.proposal.base_revision}; the project is at revision {review.current_revision}. Status: {review.proposal.status}. Nothing changes until you apply.
            </p>
            {review.proposal.rationale && (
              <p className="proposal-rationale">
                Agent's rationale ({LABEL_TEXT.assumed}): {review.proposal.rationale}
              </p>
            )}
            {rows.length > 0 && (
              <ReviewTable
                label="Proposed changes"
                columns={["Step", "Action", "Object", "Name", "Before", "After"]}
                rowAttr="data-proposal-row"
                groups={groups.map((g) => ({
                  group: g.group,
                  rows: g.rows.map((r) => {
                    const s = steps.find((x) => x.index === r.step);
                    const needs = s?.requires.length ? `; needs step ${s.requires.map((i) => i + 1).join(", ")}` : "";
                    return {
                      id: r.id,
                      cells: [String(r.step + 1), r.action, r.object, r.name, r.before, r.after],
                      checkLabel: `Apply step ${r.step + 1} ${r.action} ${r.object} ${r.name}`,
                      checked: chosen.has(r.step),
                      title: `Step ${r.step + 1}: ${s?.command ?? ""} (${s?.class ?? ""})${needs}`,
                    };
                  }),
                }))}
                onToggle={(rowId, on) => {
                  const r = rows.find((x) => x.id === rowId);
                  if (r) setSelected(toggleStep(steps, chosen, r.step, on));
                }}
              />
            )}
            {review.checks.length > 0 && (
              <section aria-label="Checks on a copy">
                <h4>Checks on a copy of the project {reviewing ? "(reviewing…)" : "(checked steps applied)"}</h4>
                <table className="eco-table" aria-label="Checks on a copy">
                  <thead>
                    <tr>
                      <th scope="col">Check</th>
                      <th scope="col">Errors</th>
                      <th scope="col">Warnings</th>
                      <th scope="col">Unknown rules</th>
                      <th scope="col">New / resolved</th>
                      <th scope="col">Label</th>
                    </tr>
                  </thead>
                  <tbody>
                    {review.checks.map((c) => (
                      <tr key={c.check} data-check={c.check} title={[...c.newly_unknown.map((u) => `Newly UNKNOWN: ${u}`), ...c.limitations].join("\n") || undefined}>
                        <td>{c.check}</td>
                        <td>{countChange(c.before.errors, c.after.errors)}</td>
                        <td>{countChange(c.before.warnings, c.after.warnings)}</td>
                        <td>{countChange(c.before.unknown_rules, c.after.unknown_rules)}</td>
                        <td>
                          +{c.appeared.length} / −{c.resolved.length}
                        </td>
                        <td>{LABEL_TEXT[c.label]}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {review.checks.some((c) => c.appeared.length > 0) && (
                  <ul className="proposal-findings" aria-label="Findings the proposal introduces">
                    {review.checks.flatMap((c) =>
                      c.appeared.map((f) => (
                        <li key={`${c.check}-${f.id}`}>
                          {c.check} {f.severity}: {f.message}
                        </li>
                      )),
                    )}
                  </ul>
                )}
                {review.checks.some((c) => c.not_implemented.length > 0) && (
                  <p className="muted">Not implemented, so not checked: {[...new Set(review.checks.flatMap((c) => c.not_implemented))].join("; ")}</p>
                )}
              </section>
            )}
            {review.evidence.length > 0 && (
              <section aria-label="Evidence labels">
                <h4>Evidence (labelled by EAK)</h4>
                <ul>
                  {review.evidence.map((ev, i) => (
                    <li key={i}>
                      {ev.subject}: <strong>{LABEL_TEXT[ev.label]}</strong> — {ev.basis}
                    </li>
                  ))}
                </ul>
              </section>
            )}
            <p className="muted">{review.note}</p>
          </>
        )}
        </div>
        {review && review.proposal.id === id && state.reason && (
          <p className="eco-reason proposal-reason" role="note">
            {state.reason}
          </p>
        )}
        {state.ok && state.steps && (
          <p className="eco-partial proposal-partial" role="note">
            Applying {state.count} of {steps.length} step(s) as one undo step; the unchecked steps are not applied.
          </p>
        )}
        {note && (
          <p role={note.ok ? undefined : "alert"} aria-live="polite" className={note.ok ? "muted" : "explorer-error"}>
            {note.text}
          </p>
        )}
        <div className="dlg-actions">
          {review && review.proposal.id === id && (
            <>
              <button type="button" onClick={exportReport} disabled={off} title="Save EAK's review of the checked steps as evidence/proposal-<id>-r<revision>.json in the project">
                Export report
              </button>
              {confirmDiscard ? (
                <>
                  <button type="button" className="danger" onClick={discard} disabled={off} title="Mark the proposal discarded; the design is not changed">
                    Confirm discard
                  </button>
                  <button type="button" onClick={() => setConfirmDiscard(false)}>
                    Keep proposal
                  </button>
                </>
              ) : (
                <button type="button" onClick={() => setConfirmDiscard(true)} disabled={off || !open} title={open ? "Discard this proposal (asks to confirm); the design is not changed" : "Only an open proposal can be discarded"}>
                  Discard proposal
                </button>
              )}
              <button type="button" className="primary" onClick={apply} disabled={off || !state.ok} title={state.reason ?? `Apply ${state.count} step(s) as one undo step`}>
                Apply proposal
              </button>
            </>
          )}
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
