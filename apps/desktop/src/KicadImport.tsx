// "Import from KiCad": one optional interchange option in the library manager. EAK's own library
// format stays the source of truth and nothing here needs KiCad; this runs EAK's importer on a
// file the user picks and shows its report (what was accepted, rejected and not interpreted).
// It does not add anything to a library by itself.
import { useState } from "react";
import { api, pickKicadFile, type KicadImportView } from "./api";

export function KicadImport() {
  const [view, setView] = useState<KicadImportView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setErr(null);
    try {
      const path = await pickKicadFile("Import from KiCad (.kicad_sym or .kicad_mod)");
      if (!path) return;
      setBusy(true);
      setView(await api.kicadImportReport(path));
    } catch (e) {
      setView(null);
      setErr(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };

  const unsupported = view ? Object.entries(view.report.unsupported) : [];
  return (
    <section aria-label="Import from KiCad" className="kicad-import">
      <button type="button" disabled={busy} onClick={run} title="Read a KiCad symbol library or footprint file and report what EAK's importer understands. Optional; EAK does not need KiCad.">
        {busy ? "Reading…" : "Import from KiCad…"}
      </button>
      {err && <p className="empty-state" role="alert">{err}</p>}
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
          <p className="muted">This is a report only; nothing was added to a library.</p>
        </div>
      )}
    </section>
  );
}
