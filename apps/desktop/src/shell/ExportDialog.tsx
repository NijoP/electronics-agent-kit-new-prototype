// Export manufacturing files dialog: says what will happen, runs the export through the backend
// (ERC, DRC and MECH on the current revision, then Gerber, Excellon, BOM, CPL), and shows the
// manifest: status wording comes from the engine; nothing is ever called manufacturing-ready.
import { ExportPanel } from "../ExportPanel";
import type { ExportResult } from "../types";

interface Props {
  result: ExportResult | null;
  revision: number;
  busy: boolean;
  /** Reason the export cannot run right now (for example no board outline), if known. */
  blocked?: string;
  onRun: () => void;
  onClose: () => void;
}

export function ExportDialog({ result, revision, busy, blocked, onRun, onClose }: Props) {
  const stale = result && result.manifest.revision !== revision;
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" role="dialog" aria-label="Export manufacturing files" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>Export manufacturing files</h2>
          <button type="button" onClick={onClose}>Close</button>
        </div>
        <p className="muted hint">
          Runs ERC, DRC and mechanical checks on revision {revision}, saves them as evidence, then writes Gerber, Excellon drill, BOM and pick-and-place files into
          <code> output/rev-{revision}</code> inside the project folder. The package is DRAFT unless every gate passes; EAK never calls it manufacturing-ready.
        </p>
        {blocked && <p role="alert" className="error">{blocked}</p>}
        <div className="row">
          <button type="button" className="primary" disabled={busy || !!blocked} onClick={onRun}>
            {busy ? "Exporting…" : result ? "Export again" : "Export"}
          </button>
          <button type="button" onClick={onClose}>Close</button>
        </div>
        {stale && <p className="error">This result is for revision {result!.manifest.revision}; the project is now at revision {revision}. Export again.</p>}
        <ExportPanel result={result} />
      </div>
    </div>
  );
}
