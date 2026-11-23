// "Update PCB" review (ECO preview, Phase 1 C2, ADR-0041): every change the schematic-to-PCB sync
// would make, from the engine's read-only `sync_preview`, grouped by action or object with a
// checkbox per change. Nothing changes until Apply, which sends one Batch at the previewed revision
// (a project that moved on rejects it as stale): the full sync with every change checked, or only
// the checked changes through SyncSchematicToPcbSelected (ADR-0042). Either way one undo step.
import { useMemo, useState } from "react";
import type { SyncChange } from "../types";
import { applyState, buildEcoRows, groupRows, toggleRow, type EcoGroupBy, type SyncPreview } from "./ecoPreview";
import { useEscape } from "./useEscape";

interface Props {
  preview: SyncPreview;
  busy: boolean;
  /** Apply the previewed changes: null for all of them, otherwise only the checked ones. */
  onApply: (changes: SyncChange[] | null) => void;
  /** Save the engine's preview as project evidence; resolves to the saved file path. */
  onExport: (excluded: string[]) => Promise<string>;
  onClose: () => void;
}

export function EcoPreviewDialog({ preview, busy, onApply, onExport, onClose }: Props) {
  useEscape(onClose);
  const rows = useMemo(() => buildEcoRows(preview.changes), [preview]);
  const [selected, setSelected] = useState<Set<string>>(() => new Set(rows.map((r) => r.id)));
  const [by, setBy] = useState<EcoGroupBy>("action");
  const [exported, setExported] = useState<{ ok: boolean; text: string } | null>(null);
  const groups = groupRows(rows, by);
  const state = applyState(rows, selected);
  const all = rows.length > 0 && selected.size === rows.length;
  const exportReport = async () => {
    try {
      const file = await onExport(rows.filter((r) => !selected.has(r.id)).map((r) => r.id));
      setExported({ ok: true, text: `Report saved: ${file}` });
    } catch (e) {
      setExported({ ok: false, text: `Report not saved: ${String(e)}` });
    }
  };
  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette eco-preview" role="dialog" aria-label="Update PCB: review changes" onClick={(e) => e.stopPropagation()}>
        <h3>Update PCB: review changes (revision {preview.revision})</h3>
        <p className="muted">Nothing on the PCB changes until you apply. {rows.length} change(s) from the schematic.</p>
        <div className="eco-bar">
          <label>
            Group by{" "}
            <select aria-label="Group changes by" value={by} onChange={(e) => setBy(e.target.value as EcoGroupBy)}>
              <option value="action">Action</option>
              <option value="object">Object</option>
            </select>
          </label>
          <label>
            <input type="checkbox" aria-label="Select all changes" checked={all} disabled={rows.length === 0} onChange={(e) => setSelected(e.target.checked ? new Set(rows.map((r) => r.id)) : new Set())} /> All
          </label>
        </div>
        <div className="eco-table-wrap">
          <table className="eco-table" aria-label="Changes to apply">
            <thead>
              <tr>
                <th scope="col">Apply</th>
                <th scope="col">Action</th>
                <th scope="col">Object</th>
                <th scope="col">Name</th>
                <th scope="col">Before</th>
                <th scope="col">After</th>
              </tr>
            </thead>
            {groups.map((g) => (
              <tbody key={g.group} aria-label={`${g.group} (${g.rows.length})`}>
                <tr className="eco-group">
                  <th scope="rowgroup" colSpan={6}>
                    {g.group} <span className="count">{g.rows.length}</span>
                  </th>
                </tr>
                {g.rows.map((r) => (
                  <tr key={r.id} data-eco-row={r.id} className={selected.has(r.id) ? undefined : "off"}>
                    <td>
                      <input type="checkbox" aria-label={`Apply ${r.action} ${r.object} ${r.name}`} checked={selected.has(r.id)} onChange={(e) => setSelected((s) => toggleRow(rows, s, r.id, e.target.checked))} />
                    </td>
                    <td>{r.action}</td>
                    <td>{r.object}</td>
                    <td>{r.name}</td>
                    <td>{r.before}</td>
                    <td>{r.after}</td>
                  </tr>
                ))}
              </tbody>
            ))}
          </table>
        </div>
        {preview.warnings.length > 0 && (
          <section className="eco-warnings" aria-label="Sync notes">
            <h4>Notes from the engine</h4>
            <ul>
              {preview.warnings.map((w, i) => (
                <li key={i}>{w}</li>
              ))}
            </ul>
          </section>
        )}
        {state.reason && <p className="eco-reason" role="note">{state.reason}</p>}
        {state.ok && state.changes && (
          <p className="eco-partial" role="note">
            Applying {state.count} of {rows.length} change(s) as one undo step; the unchecked ones stay listed the next time you update the PCB.
          </p>
        )}
        {exported && <p role={exported.ok ? undefined : "alert"} aria-live="polite" className={exported.ok ? "muted" : "explorer-error"}>{exported.text}</p>}
        <div className="dlg-actions">
          <button type="button" onClick={exportReport} disabled={busy} title="Save this preview as evidence/eco-r<revision>.json in the project">Export report</button>
          <button type="button" className="primary" onClick={() => onApply(state.changes)} disabled={busy || !state.ok} title={state.reason ?? `Apply ${state.count === rows.length ? rows.length : `${state.count} of ${rows.length}`} change(s) as one undo step`}>Apply changes</button>
          <button type="button" onClick={onClose}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
