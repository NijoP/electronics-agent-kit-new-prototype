// Output dialogs (slice D3). Export BOM: column picker, column order and headers, per-column
// sort, grouping, filter rules and CSV/XLSX, with a preview the engine computes from the same
// configuration it exports. Export schematic and netlist: schematic PDF/SVG and IPC-D-356/JSON
// netlists. Both run ERC, DRC, MECH and DFM on the current revision first; the folder's manifest
// lists every file and is DRAFT unless the release gate passes (never "manufacturing-ready").
import { useEffect, useState } from "react";
import { api } from "../api";
import { ExportPanel } from "../ExportPanel";
import type { BomConfig, BomFormat, BomTable, ExportResult, OutputRequest } from "../types";
import { BOM_FIELDS, FILTER_OPS, defaultBomConfig, fieldLabel, loadBomConfig, moveColumn, saveBomConfig, toggleColumn, updateColumn, updateRule } from "./bomConfig";

interface Common {
  revision: number;
  /** Another engine command is running (the export waits for it). */
  busy: boolean;
  onExported: (r: ExportResult) => void;
  onClose: () => void;
}

const PREVIEW_ROWS = 50;
const message = (e: unknown) => String(e).replace(/^Error: /, "");

function Stale({ result, revision }: { result: ExportResult | null; revision: number }) {
  if (!result || result.manifest.revision === revision) return null;
  return (
    <p className="error">
      This result is for revision {result.manifest.revision}; the project is now at revision {revision}. Export again.
    </p>
  );
}

export function BomExportDialog({ revision, busy, onExported, onClose }: Common) {
  const [config, setConfig] = useState<BomConfig>(() => loadBomConfig());
  const [format, setFormat] = useState<BomFormat>("csv");
  const [preview, setPreview] = useState<BomTable | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<ExportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The engine computes the preview from the configuration, per revision.
  useEffect(() => {
    saveBomConfig(config);
    let live = true;
    api
      .bomTable(config)
      .then((t) => {
        if (!live) return;
        setPreview(t);
        setProblem(null);
      })
      .catch((e) => {
        if (!live) return;
        setPreview(null);
        setProblem(message(e));
      });
    return () => {
      live = false;
    };
  }, [config, revision]);

  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      const r = await api.exportBom(config, format);
      setResult(r);
      onExported(r);
    } catch (e) {
      setError(message(e));
    } finally {
      setRunning(false);
    }
  };
  const blocked = busy ? "An engine command is running" : problem ? `Fix the configuration first: ${problem}` : undefined;
  const has = (f: string) => config.columns.some((c) => c.field === f);
  const last = config.columns.length - 1;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal rules output-dialog" role="dialog" aria-label="Export BOM" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>Export BOM</h2>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
        <p className="muted hint">
          Runs ERC, DRC, MECH and DFM on revision {revision}, then writes the BOM and its configuration into <code>output/rev-{revision}/bom</code>. The manifest lists both files and is DRAFT unless every gate passes. Stock and prices come only from stored Distributor records.
        </p>

        <fieldset aria-label="Columns">
          <legend>Columns</legend>
          <div className="output-picker">
            {BOM_FIELDS.map((f) => {
              const only = has(f.field) && config.columns.length === 1;
              return (
                <label key={f.field} className="inline">
                  <input
                    type="checkbox"
                    aria-label={`Column ${f.label}`}
                    checked={has(f.field)}
                    disabled={only}
                    title={only ? "A BOM needs at least one column" : undefined}
                    onChange={() => setConfig((c) => toggleColumn(c, f.field))}
                  />
                  {f.label}
                </label>
              );
            })}
          </div>
        </fieldset>

        <table className="bom" aria-label="Column order">
          <thead>
            <tr>
              <th>Column</th>
              <th>Header</th>
              <th>Sort</th>
              <th>Order</th>
            </tr>
          </thead>
          <tbody>
            {config.columns.map((c, i) => (
              <tr key={c.field}>
                <td>{fieldLabel(c.field)}</td>
                <td>
                  <input
                    aria-label={`Header for ${fieldLabel(c.field)}`}
                    value={c.header ?? ""}
                    placeholder={fieldLabel(c.field)}
                    onChange={(e) => setConfig((x) => updateColumn(x, i, { header: e.target.value || undefined }))}
                  />
                </td>
                <td>
                  <select aria-label={`Sort ${fieldLabel(c.field)}`} value={c.sort} onChange={(e) => setConfig((x) => updateColumn(x, i, { sort: e.target.value as "none" | "asc" | "desc" }))}>
                    <option value="none">No sort</option>
                    <option value="asc">Ascending</option>
                    <option value="desc">Descending</option>
                  </select>
                </td>
                <td>
                  <button type="button" aria-label={`Move ${fieldLabel(c.field)} up`} disabled={i === 0} title={i === 0 ? "Already the first column" : "Move left in the exported table"} onClick={() => setConfig((x) => moveColumn(x, i, -1))}>
                    Up
                  </button>
                  <button type="button" aria-label={`Move ${fieldLabel(c.field)} down`} disabled={i === last} title={i === last ? "Already the last column" : "Move right in the exported table"} onClick={() => setConfig((x) => moveColumn(x, i, 1))}>
                    Down
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="muted">Sorted columns sort in table order: the first sorted column is the primary key.</p>

        <div className="row">
          <label>
            Group rows
            <select aria-label="Group rows" value={config.grouping} onChange={(e) => setConfig((c) => ({ ...c, grouping: e.target.value === "designator" ? "designator" : "part" }))}>
              <option value="part">One row per part (component and value)</option>
              <option value="designator">One row per designator</option>
            </select>
          </label>
          <label>
            Format
            <select aria-label="File format" value={format} onChange={(e) => setFormat(e.target.value === "xlsx" ? "xlsx" : "csv")}>
              <option value="csv">CSV</option>
              <option value="xlsx">XLSX (Excel workbook)</option>
            </select>
          </label>
        </div>

        <fieldset aria-label="Filter rules">
          <legend>Remove rows where</legend>
          {config.exclude.length === 0 && <p className="muted">No filter rules: every row is exported.</p>}
          {config.exclude.map((r, i) => {
            const needsValue = FILTER_OPS.find((o) => o.op === r.op)?.needsValue ?? true;
            return (
              <div key={i} className="row">
                <select aria-label={`Filter ${i + 1} column`} value={r.field} onChange={(e) => setConfig((c) => updateRule(c, i, { field: e.target.value as typeof r.field }))}>
                  {BOM_FIELDS.map((f) => (
                    <option key={f.field} value={f.field}>
                      {f.label}
                    </option>
                  ))}
                </select>
                <select aria-label={`Filter ${i + 1} condition`} value={r.op} onChange={(e) => setConfig((c) => updateRule(c, i, { op: e.target.value as typeof r.op }))}>
                  {FILTER_OPS.map((o) => (
                    <option key={o.op} value={o.op}>
                      {o.label}
                    </option>
                  ))}
                </select>
                {needsValue && <input aria-label={`Filter ${i + 1} value`} value={r.value} onChange={(e) => setConfig((c) => updateRule(c, i, { value: e.target.value }))} />}
                <button type="button" aria-label={`Remove filter ${i + 1}`} onClick={() => setConfig((c) => ({ ...c, exclude: c.exclude.filter((_, k) => k !== i) }))}>
                  Remove
                </button>
              </div>
            );
          })}
          <button type="button" onClick={() => setConfig((c) => ({ ...c, exclude: [...c.exclude, { field: "comment", op: "contains", value: "DNP" }] }))}>
            Add filter rule
          </button>
        </fieldset>

        <div className="row">
          <button type="button" className="primary" disabled={running || !!blocked} title={blocked} onClick={() => void run()}>
            {running ? "Exporting…" : result ? "Export again" : `Export ${format.toUpperCase()}`}
          </button>
          <button type="button" title="Restore the default columns, sorting and grouping, and remove every filter rule" onClick={() => setConfig(defaultBomConfig())}>
            Restore default
          </button>
        </div>
        {problem && (
          <p role="alert" className="error">
            {problem}
          </p>
        )}
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}

        {preview && (
          <section aria-label="BOM preview">
            <h3>
              Preview · {preview.rows.length} row(s), {preview.parts} part(s){preview.removed > 0 ? `, ${preview.removed} removed by filters` : ""}
            </h3>
            {preview.rows.length === 0 ? (
              <p className="empty-state">No rows: the schematic has no parts, or every row was removed by a filter rule.</p>
            ) : (
              <table className="bom" aria-label="BOM preview table">
                <thead>
                  <tr>
                    {preview.headers.map((h, i) => (
                      <th key={i}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.slice(0, PREVIEW_ROWS).map((r, i) => (
                    <tr key={i}>
                      {r.map((c, k) => (
                        <td key={k}>{c}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            {preview.rows.length > PREVIEW_ROWS && <p className="muted">Showing the first {PREVIEW_ROWS} rows; the export has all {preview.rows.length}.</p>}
          </section>
        )}
        <Stale result={result} revision={revision} />
        <ExportPanel result={result} />
      </div>
    </div>
  );
}

const DOCUMENTS: readonly { key: keyof OutputRequest; label: string; title: string }[] = [
  { key: "schematic_pdf", label: "Schematic PDF", title: "One page per sheet, with a title block (project, sheet, revision, date, status, EAK-IR hash)" },
  { key: "schematic_svg", label: "Schematic SVG", title: "One SVG file per sheet, with the same title block" },
  { key: "ipc_d_356", label: "IPC-D-356 netlist", title: "Bare-board electrical test netlist for the fabricator: every PCB pad and via with its net" },
  { key: "netlist_json", label: "JSON netlist", title: "Components and schematic nets with their pins, plus the PCB pad nets" },
];

export function DocumentExportDialog({ revision, busy, onExported, onClose }: Common) {
  const [request, setRequest] = useState<OutputRequest>({ schematic_pdf: true, schematic_svg: false, ipc_d_356: true, netlist_json: false });
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<ExportResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const none = !DOCUMENTS.some((d) => request[d.key]);
  const blocked = busy ? "An engine command is running" : none ? "Choose at least one output" : undefined;
  const run = async () => {
    setRunning(true);
    setError(null);
    try {
      const r = await api.exportDocuments(request);
      setResult(r);
      onExported(r);
    } catch (e) {
      setError(message(e));
    } finally {
      setRunning(false);
    }
  };
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal output-dialog" role="dialog" aria-label="Export schematic and netlist" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>Export schematic and netlist</h2>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
        <p className="muted hint">
          Runs ERC, DRC, MECH and DFM on revision {revision}, then writes the chosen documents into <code>output/rev-{revision}/documents</code>. The manifest lists every file and is DRAFT unless every gate passes; the documents carry the same label.
        </p>
        <fieldset aria-label="Outputs">
          <legend>Outputs</legend>
          {DOCUMENTS.map((d) => (
            <label key={d.key} className="inline" title={d.title}>
              <input type="checkbox" aria-label={d.label} checked={request[d.key]} onChange={() => setRequest((r) => ({ ...r, [d.key]: !r[d.key] }))} />
              {d.label}
            </label>
          ))}
        </fieldset>
        <div className="row">
          <button type="button" className="primary" disabled={running || !!blocked} title={blocked} onClick={() => void run()}>
            {running ? "Exporting…" : result ? "Export again" : "Export"}
          </button>
        </div>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <Stale result={result} revision={revision} />
        <ExportPanel result={result} />
      </div>
    </div>
  );
}
