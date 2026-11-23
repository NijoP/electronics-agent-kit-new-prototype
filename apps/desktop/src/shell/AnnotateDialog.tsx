// Annotate options (Phase 1 B5, ADR-0046): numbering order, scope and reset, sent as one typed
// Annotate command; the engine refuses what has nothing to do and the dialog shows why.
import { useState } from "react";
import type { AnnotateOrder, AnnotateScope, Command, Id } from "../types";

/** The Annotate command the options describe; `null` when the selection scope has no symbol. */
export function annotateCommand(order: AnnotateOrder, scope: AnnotateScope, reset: boolean, selected?: Id): Command | null {
  if (scope === "selection" && !selected) return null;
  return { type: "Annotate", order, scope, symbols: scope === "selection" && selected ? [selected] : [], reset };
}

interface Props {
  busy: boolean;
  /** The selected schematic symbol, if any: the selection scope numbers only it. */
  selected?: Id;
  /** Run the command; resolves to `false` when it was refused or could not run (busy). */
  apply: (c: Command) => Promise<boolean | void>;
  onClose: () => void;
}

export function AnnotateDialog({ busy, selected, apply, onClose }: Props) {
  const [order, setOrder] = useState<AnnotateOrder>("across");
  const [scope, setScope] = useState<AnnotateScope>("unannotated");
  const [reset, setReset] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const go = async () => {
    const cmd = annotateCommand(order, scope, reset, selected);
    if (!cmd) return setError("Select a schematic symbol first, or choose another scope.");
    setError(null);
    try {
      // Stay open on a refusal so the reason can be read and the options changed (S18).
      if ((await apply(cmd)) === false) return setError("Not applied: the engine refused it or another command was running. The status bar says why.");
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" role="dialog" aria-label="Annotate options" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>Annotate schematic</h2>
          <button type="button" onClick={onClose}>Close</button>
        </div>
        <p className="muted hint">Numbers follow the sheet order, then the order below. Designators outside the scope never change and are never reused.</p>
        {error && <p role="alert" className="error">{error}</p>}
        <label>
          Scope{" "}
          <select aria-label="Annotate scope" value={scope} onChange={(e) => setScope(e.target.value as AnnotateScope)}>
            <option value="unannotated">Unannotated symbols (designator ends in ?)</option>
            <option value="all">All symbols</option>
            <option value="selection" disabled={!selected}>Selected symbol</option>
          </select>
        </label>
        <label>
          Order{" "}
          <select aria-label="Annotate order" value={order} onChange={(e) => setOrder(e.target.value as AnnotateOrder)} disabled={reset} title={reset ? "Reset gives every designator in scope its prefix and ?, so there is no numbering order" : undefined}>
            <option value="across">Across (by row, then column)</option>
            <option value="down">Down (by column, then row)</option>
          </select>
        </label>
        <label>
          <input type="checkbox" aria-label="Reset designators" checked={reset} onChange={(e) => setReset(e.target.checked)} /> Reset the designators in scope to their prefix and ?
        </label>
        <div className="modal-actions">
          <button type="button" className="primary" disabled={busy} title={busy ? "Another command is running; wait for it to finish" : "Send the Annotate command"} onClick={() => void go()}>
            {reset ? "Reset designators" : "Annotate"}
          </button>
        </div>
      </div>
    </div>
  );
}
