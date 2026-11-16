// "Import from KiCad": one optional interchange option in the library manager. EAK's own library
// format stays the source of truth and nothing here needs KiCad; this runs EAK's importer on a
// file the user picks and shows its report (what was accepted, rejected and not interpreted).
// It does not add anything to a library by itself.
import { useState } from "react";
import { api, pickKicadFile, type KicadImportView, type KicadCommitResult } from "./api";

export function KicadImport({ onCommitted }: { onCommitted?: () => void }) {
  const [filePath, setFilePath] = useState<string | null>(null);
  const [view, setView] = useState<KicadImportView | null>(null);
  const [commitResult, setCommitResult] = useState<KicadCommitResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setErr(null);
    setCommitResult(null);
    try {
      const path = await pickKicadFile("Import from KiCad (.kicad_sym or .kicad_mod)");
      if (!path) return;
      setFilePath(path);
      setBusy(true);
      setView(await api.kicadImportReport(path));
    } catch (e) {
      setView(null);
      setFilePath(null);
      setErr(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };

  const commit = async () => {
    if (!filePath) return;
    setErr(null);
    setBusy(true);
    try {
      const res = await api.kicadImportCommit(filePath);
      setCommitResult(res);
      onCommitted?.();
    } catch (e) {
      setErr(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };

  const unsupported = view ? Object.entries(view.report.unsupported) : [];
  return (
    <section aria-label="Import from KiCad" className="kicad-import">
      <div className="row">
        <button type="button" disabled={busy} onClick={run} title="Read a KiCad symbol library or footprint file and report what EAK's importer understands. Optional; EAK does not need KiCad.">
          {busy ? "Reading…" : "Import from KiCad…"}
        </button>
        {view && view.imported > 0 && !commitResult && (
          <button type="button" className="primary" disabled={busy} onClick={commit} title="Commit imported components into EAK's library store">
            {busy ? "Importing…" : "Commit to Library"}
          </button>
        )}
      </div>
      {err && <p className="empty-state" role="alert">{err}</p>}
      {commitResult && (
        <p className="success" role="status">
          Imported {commitResult.components_created} component(s) into library <strong>{commitResult.library_name}</strong>. Ready for placement on schematics!
        </p>
      )}
      {view && (
        <div aria-label="KiCad import report">
          <p>
            <strong>{view.file}</strong> ({view.report.format}): {view.imported} imported, {view.rejected} rejected
          </p>
          <ul>
            {view.report.items.map((i) => (
              <li key={i.name}>
                {i.name}: {i.status}
                {i.notes.length > 0 && <span className="muted"> — {i.notes.join("; ")}</span>}
              </li>
            ))}
          </ul>
          {unsupported.length > 0 && (
            <p className="muted">
              Not interpreted: {unsupported.map(([k, n]) => `${k} ×${n}`).join(", ")}
            </p>
          )}
          {!commitResult && <p className="muted">Review the report above, then click Commit to Library to make components available.</p>}
        </div>
      )}
    </section>
  );
}
