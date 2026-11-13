// JLCPCB manufacturing preparation: BOM view, DFM rule check against JLCPCB's published
// 2-layer limits, and a bundle written to a folder the user names. Everything shown is
// "prepared for review"; nothing is uploaded or ordered, and rules this software cannot
// evaluate are listed as not checked, never as passing.
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { DfmStatus, ManufacturingPreview, PrepareResult } from "./types";

const LABEL: Record<DfmStatus, string> = {
  pass: "pass",
  warn: "warning",
  fail: "FAIL",
  not_checked: "not checked",
};

export function ManufacturingPanel({ revision, onPrepared }: { revision: number; onPrepared: (m: string) => void }) {
  const [preview, setPreview] = useState<ManufacturingPreview | null>(null);
  const [outDir, setOutDir] = useState("");
  const [result, setResult] = useState<PrepareResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setError(null);
      setPreview(await api.manufacturingPreview());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // The BOM and the checks are views of the current revision: refresh when it moves.
  useEffect(() => {
    void load();
  }, [load, revision]);

  const prepare = async () => {
    setBusy(true);
    try {
      setError(null);
      const r = await api.jlcpcbPrepare(outDir);
      setResult(r);
      onPrepared(`JLCPCB bundle prepared for review in ${r.prep.out_dir}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mfg" aria-label="Manufacturing prep">
      <h2>Manufacturing prep · JLCPCB</h2>
      <p className="muted">
        Prepared for review only. Not manufacturing-ready, nothing is uploaded, quoted or ordered.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {preview && (
        <>
          <h3>BOM · revision {preview.revision}</h3>
          <table aria-label="Bill of materials">
            <thead>
              <tr>
                <th>Qty</th>
                <th>Designators</th>
                <th>Value</th>
                <th>Footprint</th>
                <th>LCSC #</th>
                <th>Stock</th>
                <th>Unit $</th>
              </tr>
            </thead>
            <tbody>
              {preview.bom.map((l) => (
                <tr key={`${l.component_id}|${l.value}`} className={l.issues.length ? "has-issues" : undefined}>
                  <td>{l.quantity}</td>
                  <td>{l.designators.join(", ")}</td>
                  <td>{l.value}</td>
                  <td>{l.footprint}</td>
                  <td>{l.lcsc ?? <span className="error">missing</span>}</td>
                  <td>{l.stock === null ? "—" : l.stock.toLocaleString("en-US")}</td>
                  <td>{l.unit_price_usd ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <h3>
            JLCPCB capability check{" "}
            <span className="muted">
              {preview.dfm.profile.id} · limits read {preview.dfm.profile.verified_on}
            </span>
          </h3>
          <p role="status">{preview.dfm.statement}</p>
          <table aria-label="DFM checks">
            <tbody>
              {preview.dfm.checks.map((c) => (
                <tr key={c.id} className={`dfm-${c.status}`}>
                  <td>{c.id}</td>
                  <td>{c.title}</td>
                  <td>{LABEL[c.status]}</td>
                  <td className="muted">
                    {c.measured}
                    {c.detail.length > 0 && ` — ${c.detail[0]}`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      <label>
        Output folder (absolute path; empty = inside the project)
        <input value={outDir} onChange={(e) => setOutDir(e.target.value)} placeholder={preview?.default_out_dir ?? ""} />
      </label>
      <button type="button" onClick={prepare} disabled={busy || !preview}>
        Generate JLCPCB bundle
      </button>
      {result && (
        <div aria-label="Prepared bundle">
          <p role="status">{result.prep.manifest.statement}</p>
          <p className="muted">{result.prep.out_dir}</p>
          <ul className="files">
            {result.prep.manifest.files.map((f) => (
              <li key={f.name}>
                {f.name}{" "}
                <span className="muted">
                  ({f.kind}, {f.bytes} B)
                </span>
              </li>
            ))}
          </ul>
          <p className="muted">Not verified: {result.prep.manifest.not_verified.join("; ")}.</p>
        </div>
      )}
    </section>
  );
}
