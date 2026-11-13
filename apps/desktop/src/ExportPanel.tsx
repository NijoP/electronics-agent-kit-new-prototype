// Result of the last manufacturing export. Status wording comes from the engine's
// manifest; this panel never upgrades it ("manufacturing-ready" is never claimed).
import type { ExportResult } from "./types";

export function ExportPanel({ result }: { result: ExportResult | null }) {
  if (!result) return null;
  const m = result.manifest;
  return (
    <section className="export" aria-label="Manufacturing export">
      <h2>Export · revision {m.revision}</h2>
      <p>
        <strong>{m.status === "DRAFT" ? "DRAFT — not for fabrication" : "Internally verified release candidate"}</strong>
      </p>
      <p className="muted">{m.statement}</p>
      <p className="muted">{result.out_dir}</p>
      {m.blocking_reasons.length > 0 && (
        <ul aria-label="Blocking reasons">
          {m.blocking_reasons.map((b) => (
            <li key={b} className="error">
              {b}
            </li>
          ))}
        </ul>
      )}
      <ul aria-label="Exported files" className="files">
        {m.files.map((f) => (
          <li key={f.name}>
            {f.name} <span className="muted">({f.kind}, {f.bytes} B)</span>
          </li>
        ))}
      </ul>
      <p className="muted">Not verified: {m.not_verified.join("; ")}.</p>
    </section>
  );
}
