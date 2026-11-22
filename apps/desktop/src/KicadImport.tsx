// "Import from KiCad": one optional interchange option in the library manager. EAK's own library
// format stays the source of truth and nothing here needs KiCad; this runs EAK's importer on a
// file the user picks and shows its report (what was accepted, rejected and not interpreted).
// Committing adds the parts to an imported library only with the licence and source the user
// names (the backend refuses without them, finding F2).
import { useState } from "react";
import { api, pickKicadFile, type KicadImportView, type KicadCommitResult } from "./api";

export function KicadImport({ onCommitted }: { onCommitted?: () => void }) {
  const [filePath, setFilePath] = useState<string | null>(null);
  const [view, setView] = useState<KicadImportView | null>(null);
  const [commitResult, setCommitResult] = useState<KicadCommitResult | null>(null);
  const [licence, setLicence] = useState("");
  const [source, setSource] = useState("");
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

  const missing = !licence.trim()
    ? "Name the library's licence to commit."
    : !/^https?:\/\/\S+$/.test(source.trim())
      ? "Give the http(s) address the library came from to commit."
      : null;

  const commit = async () => {
    if (!filePath || missing) return;
    setErr(null);
    setBusy(true);
    try {
      const res = await api.kicadImportCommit(filePath, licence.trim(), source.trim());
      setCommitResult(res);
      onCommitted?.();
    } catch (e) {
      setErr(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };

  const unsupported = view ? Object.entries(view.report.unsupported) : [];
  const canCommit = view !== null && view.imported > 0 && !commitResult;
  return (
    <section aria-label="Import from KiCad" className="kicad-import">
      <div className="row">
        <button type="button" disabled={busy} onClick={run} title="Read a KiCad symbol library or footprint file and report what EAK's importer understands. Optional; EAK does not need KiCad.">
          {busy ? "Reading…" : "Import from KiCad…"}
        </button>
      </div>
      {canCommit && (
        <div className="row" aria-label="Library licence and source">
          <input aria-label="Library licence" placeholder="Licence, e.g. CC-BY-SA-4.0" value={licence} onChange={(e) => setLicence(e.target.value)} />
          <input aria-label="Library source URL" placeholder="Source, e.g. https://…" value={source} onChange={(e) => setSource(e.target.value)} />
          <button type="button" className="primary" disabled={busy || missing !== null} onClick={commit} title={missing ?? "Commit the imported components into an imported library with this licence and source"}>
            {busy ? "Importing…" : "Commit to Library"}
          </button>
        </div>
      )}
      {canCommit && missing && <p className="muted">{missing} Without a licence nothing is imported.</p>}
      {err && (
        <p className="empty-state" role="alert">
          {err}
        </p>
      )}
      {commitResult && (
        <p className="success" role="status">
          Imported {commitResult.components_created} component(s) into library <strong>{commitResult.library_name}</strong> (licence {commitResult.licence}, source {commitResult.source_url}). Find them in the Library panel under the Imported source and drag one onto a schematic sheet to place it; the project gets its own copy. EAK has not verified these definitions
          {commitResult.placeholder_footprints ? ", and each footprint is a placeholder with one pad per pin: assign the real footprint before layout." : "."}
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
          {canCommit && <p className="muted">Review the report above, name the licence and source, then Commit to Library.</p>}
        </div>
      )}
    </section>
  );
}
