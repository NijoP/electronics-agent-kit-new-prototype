// Library dock (Phase 1 D1, gaps #36-#38): the Library tab of the bottom dock.
//  * Browse: kind tabs (Part / Symbol / Footprint / 3D), scopes (built-in, project, personal,
//    imported, distributor), a category tree, a paged results grid with supplier, stock and price
//    columns (ADR-0028: commercial data next to the part, never its identity), a symbol / footprint
//    / 3D preview, Place and drag-to-place. Offline mode keeps the built-in and project libraries.
//  * Common parts: an EAK-curated palette of generic R/C/L/D/Q with their package variants.
//  * Standardisation: the SOURCING check (eak-validation) over the placed parts.
//  * Manage: the existing part editor, library switches, KiCad and open-source imports.
// Placing arms the schematic Place tool; the click on the sheet runs `PlaceSymbol`, which copies
// the definition into the project library (built-in: the engine; personal/imported: one Batch,
// ADR-0033). Every list comes from the backend; the dock keeps no library model of its own.
import { useEffect, useMemo, useState, type DragEvent } from "react";
import { api } from "../api";
import { setDraggedComponent, EAK_COMPONENT_MIME } from "../dragState";
import { LibraryBrowser } from "../LibraryBrowser";
import { PartsPanel } from "../PartsPanel";
import type { Command, ComponentDef, LibraryBrowse, LibraryEntry, LibraryEntry2, Report } from "../types";
import { FootprintPreview, LibraryPanel, SymbolPreview, entryOf } from "./LibraryPanel";
import {
  COMMON_PARTS,
  KINDS,
  SCOPES,
  categoryTree,
  footprintRows,
  inCategory,
  pageOf,
  resolvePalette,
  scopeUnavailable,
  supplierCols,
  type FootprintRow,
  type LibKind,
  type LibScope,
} from "./libraryDockModel";

type Mode = "browse" | "common" | "standard" | "manage";
const MODES: [Mode, string][] = [
  ["browse", "Browse"],
  ["common", "Common parts"],
  ["standard", "Standardisation"],
  ["manage", "Manage"],
];

const OFFLINE_KEY = "eak.library.offline";
const NM_PER_MM = 1_000_000;
const mm = (nm: number) => nm / NM_PER_MM;
const fmtMm = (nm: number | null | undefined) => (nm === null || nm === undefined ? "not recorded" : `${+mm(nm).toFixed(2)} mm`);
const keyOf = (e: LibraryEntry2) => `${e.scope_label}/${e.id}`;

/** Drag payload the schematic drop target reads (same as the other library lists). */
function startDrag(ev: DragEvent, d: ComponentDef) {
  const p = { id: d.id, name: d.name, category: d.category, footprint: d.footprint?.name };
  setDraggedComponent(p);
  ev.dataTransfer.setData(EAK_COMPONENT_MIME, JSON.stringify(p));
  ev.dataTransfer.setData("text/plain", d.id);
  ev.dataTransfer.effectAllowed = "copy";
}
const endDrag = () => setDraggedComponent(null);

/** Isometric view of the generated box body (courtyard x height, model colour). A model file is
 *  named, not rendered: the 3D view loads it. */
export function ModelPreview({ def }: { def: ComponentDef }) {
  const fp = def.footprint;
  const w = Math.max(mm(fp.courtyard_half_w) * 2, 0.2);
  const d = Math.max(mm(fp.courtyard_half_h) * 2, 0.2);
  const h = Math.max(mm(fp.body_height ?? NM_PER_MM), 0.1);
  const p = (x: number, y: number, z: number): [number, number] => [(x - y) * 0.866, (x + y) * 0.5 - z];
  const corners = [0, w].flatMap((x) => [0, d].flatMap((y) => [0, h].map((z) => p(x, y, z))));
  const xs = corners.map((c) => c[0]);
  const ys = corners.map((c) => c[1]);
  const pad = Math.max(w, d, h) * 0.15;
  const minX = Math.min(...xs) - pad;
  const minY = Math.min(...ys) - pad;
  const vb = `${minX} ${minY} ${Math.max(...xs) + pad - minX} ${Math.max(...ys) + pad - minY}`;
  const poly = (pts: [number, number, number][]) => pts.map(([x, y, z]) => p(x, y, z).join(",")).join(" ");
  const color = def.model3d?.color ?? "#808080";
  const sw = Math.max(w, d, h) * 0.01;
  return (
    <svg role="img" aria-label={`3D body preview of ${def.name}`} className="lib-preview" viewBox={vb}>
      <polygon points={poly([[w, 0, 0], [w, d, 0], [w, d, h], [w, 0, h]])} fill={color} opacity={0.75} stroke="currentColor" strokeWidth={sw} />
      <polygon points={poly([[0, d, 0], [w, d, 0], [w, d, h], [0, d, h]])} fill={color} opacity={0.9} stroke="currentColor" strokeWidth={sw} />
      <polygon points={poly([[0, 0, h], [w, 0, h], [w, d, h], [0, d, h]])} fill={color} stroke="currentColor" strokeWidth={sw} />
    </svg>
  );
}

export function modelText(def: ComponentDef): string {
  const m = def.model3d?.model_ref;
  if (!m) return "No model file: the 3D view draws this generated box and marks the part as missing a model.";
  return `${m.format.toUpperCase()} file ${m.source.path} (${m.quality ?? "quality not declared"}; licence ${m.licence ?? "not recorded"}; ${m.provenance}). The preview shows the generated body; the 3D view loads the file.`;
}

interface PlaceProps {
  onPlace: (p: LibraryEntry) => void;
}

/** Arm the Place tool with a part and mark it recently used. */
function placeEntry(e: LibraryEntry2, onPlace: PlaceProps["onPlace"]) {
  onPlace(entryOf(e.def));
  void api.libraryMeta(e.id, { touch: true }).catch(() => undefined);
}

// ---- Browse ------------------------------------------------------------------------------------

interface PreviewProps extends PlaceProps {
  kind: LibKind;
  entry: LibraryEntry2 | null;
  fpRow: FootprintRow | null;
  onFavourite: (e: LibraryEntry2) => void;
  busy: boolean;
}

function Preview({ kind, entry, fpRow, onPlace, onFavourite, busy }: PreviewProps) {
  if (fpRow) {
    return (
      <section className="lib-dock-preview" aria-label={`Preview of ${fpRow.name}`}>
        <h4>{fpRow.name}</h4>
        <FootprintPreview fp={fpRow.fp} />
        <p className="muted">
          {fpRow.fp.pads.length} pads · courtyard {fmtMm(fpRow.fp.courtyard_half_w * 2)} x {fmtMm(fpRow.fp.courtyard_half_h * 2)}. A footprint is placed
          with its part: Update PCB creates it from the schematic.
        </p>
        <ul className="tree" aria-label={`Parts using ${fpRow.name}`}>
          {fpRow.parts.map((e) => (
            <li key={keyOf(e)}>
              <button
                type="button"
                disabled={busy}
                draggable={true}
                onDragStart={(ev) => startDrag(ev, e.def)}
                onDragEnd={endDrag}
                onClick={() => placeEntry(e, onPlace)}
                title="Arm the schematic Place tool with this part (or drag it onto the sheet)"
              >
                Place {e.def.name} <span className="muted">({e.scope_label})</span>
              </button>
            </li>
          ))}
        </ul>
      </section>
    );
  }
  if (!entry) return <p className="empty-state lib-dock-preview">Select a row to preview it. Drag a part onto the schematic to place it.</p>;
  const d = entry.def;
  const sup = supplierCols(entry);
  return (
    <section className="lib-dock-preview" aria-label={`Preview of ${d.name}`}>
      <h4>
        {d.name} <span className="muted">({entry.scope_label})</span>
      </h4>
      <div className="lib-previews" draggable={true} onDragStart={(ev) => startDrag(ev, d)} onDragEnd={endDrag} title="Drag onto the schematic to place">
        {(kind === "part" || kind === "symbol") && <SymbolPreview def={d} />}
        {(kind === "part" || kind === "footprint") && <FootprintPreview fp={d.footprint} />}
        {kind === "model" && <ModelPreview def={d} />}
      </div>
      <dl className="lib-facts">
        <dt>ID</dt>
        <dd>{d.id}</dd>
        <dt>Package</dt>
        <dd>
          {d.footprint.name} · {d.footprint.pads.length} pads
        </dd>
        <dt>Pins</dt>
        <dd>{d.symbol.pins.length}</dd>
        {kind === "model" && (
          <>
            <dt>3D</dt>
            <dd>{modelText(d)}</dd>
          </>
        )}
        <dt>Supplier</dt>
        <dd>
          {sup.supplier === "—" ? "none recorded" : `${sup.supplier}, stock ${sup.stock}, price ${sup.price}`}
          {sup.asOf !== null && ` (snapshot of ${new Date(sup.asOf * 1000).toLocaleDateString()}, not a quote or a suitability check)`}
        </dd>
        <dt>Ratings</dt>
        <dd>{d.ratings.length === 0 ? "none recorded" : `${d.ratings.length - entry.unverified_ratings} verified, ${entry.unverified_ratings} unverified`}</dd>
        <dt>Provenance</dt>
        <dd>{d.provenance}</dd>
      </dl>
      <div className="row">
        <button type="button" className="primary" disabled={busy} onClick={() => placeEntry(entry, onPlace)} title="Arm the schematic Place tool with this part; placing copies it into the project library">
          Place on schematic
        </button>
        <button type="button" aria-pressed={entry.favourite} disabled={busy} onClick={() => onFavourite(entry)} title={entry.favourite ? "Remove from favourites" : "Add to favourites"}>
          {entry.favourite ? "★ Favourite" : "☆ Favourite"}
        </button>
      </div>
    </section>
  );
}

interface GridProps {
  kind: LibKind;
  rows: LibraryEntry2[];
  fps: FootprintRow[];
  selected: string | null;
  onSelect: (key: string) => void;
}

function RowPick({ id, label, def, selected, onSelect }: { id: string; label: string; def?: ComponentDef; selected: string | null; onSelect: (key: string) => void }) {
  return (
    <button
      type="button"
      className={selected === id ? "on" : ""}
      aria-pressed={selected === id}
      onClick={() => onSelect(id)}
      draggable={def !== undefined}
      onDragStart={(ev) => def && startDrag(ev, def)}
      onDragEnd={endDrag}
      title={def ? "Click to preview · Drag onto the schematic to place" : "Click to preview the footprint and the parts that use it"}
    >
      {label}
    </button>
  );
}

function Grid({ kind, rows, fps, selected, onSelect }: GridProps) {
  if (kind === "footprint") {
    return (
      <table className="bom lib-grid" aria-label="Library grid">
        <thead>
          <tr>
            <th>Footprint</th>
            <th>Pads</th>
            <th>Courtyard</th>
            <th>Height</th>
            <th>Used by</th>
          </tr>
        </thead>
        <tbody>
          {fps.map((r) => (
            <tr key={r.name}>
              <td>
                <RowPick id={`fp:${r.name}`} label={r.name} selected={selected} onSelect={onSelect} />
              </td>
              <td>{r.fp.pads.length}</td>
              <td>
                {fmtMm(r.fp.courtyard_half_w * 2)} x {fmtMm(r.fp.courtyard_half_h * 2)}
              </td>
              <td>{fmtMm(r.fp.body_height)}</td>
              <td>{r.parts.length} part(s)</td>
            </tr>
          ))}
        </tbody>
      </table>
    );
  }
  return (
    <table className="bom lib-grid" aria-label="Library grid">
      <thead>
        {kind === "part" && (
          <tr>
            <th>Part</th>
            <th>Category</th>
            <th>Package</th>
            <th>Maker / MPN</th>
            <th>Supplier</th>
            <th>Stock</th>
            <th>Price</th>
            <th>Scope</th>
          </tr>
        )}
        {kind === "symbol" && (
          <tr>
            <th>Symbol</th>
            <th>Pins</th>
            <th>Pin names</th>
            <th>Prefix</th>
            <th>Scope</th>
          </tr>
        )}
        {kind === "model" && (
          <tr>
            <th>Part</th>
            <th>3D model</th>
            <th>Quality</th>
            <th>Licence</th>
            <th>Scope</th>
          </tr>
        )}
      </thead>
      <tbody>
        {rows.map((e) => {
          const d = e.def;
          const m = d.model3d?.model_ref;
          const s = kind === "part" ? supplierCols(e) : null;
          return (
            <tr key={keyOf(e)}>
              <td>
                <RowPick id={keyOf(e)} label={d.name} def={d} selected={selected} onSelect={onSelect} />
              </td>
              {s && (
                <>
                  <td>{d.category}</td>
                  <td>{d.footprint.name}</td>
                  <td>
                    {d.manufacturer ?? "—"} / {d.mpn ?? "—"}
                  </td>
                  <td>{s.supplier}</td>
                  <td>{s.stock}</td>
                  <td>{s.price}</td>
                </>
              )}
              {kind === "symbol" && (
                <>
                  <td>{d.symbol.pins.length}</td>
                  <td className="muted">{d.symbol.pins.map((p) => `${p.number}:${p.name}`).join(" ")}</td>
                  <td>{d.refdes_prefix}</td>
                </>
              )}
              {kind === "model" && (
                <>
                  <td>{m ? `${m.format.toUpperCase()} file` : `generated ${d.model3d?.kind ?? "box"} (no file)`}</td>
                  <td>{m ? (m.quality ?? "not declared") : "—"}</td>
                  <td>{m ? (m.licence ?? "not recorded") : "—"}</td>
                </>
              )}
              <td className="muted">{e.scope_label}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

interface BrowseProps extends PlaceProps {
  revision: number;
  busy: boolean;
  offline: boolean;
  onOffline: (on: boolean) => void;
  onImported: (message: string) => Promise<void>;
}

function Browse({ revision, busy, offline, onOffline, onPlace, onImported }: BrowseProps) {
  const [kind, setKind] = useState<LibKind>("part");
  const [scope, setScope] = useState<LibScope>("builtin");
  const [text, setText] = useState("");
  const [cat, setCat] = useState<string | null>(null);
  const [page, setPage] = useState(0);
  const [sel, setSel] = useState<string | null>(null);
  const [data, setData] = useState<LibraryBrowse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [gen, setGen] = useState(0);

  // Offline mode never reads a scope outside the app and the open project.
  const scopeNow: LibScope = scopeUnavailable(scope, offline) ? "builtin" : scope;
  useEffect(() => {
    if (scopeNow !== scope) setScope(scopeNow);
  }, [scopeNow, scope]);

  useEffect(() => {
    if (scopeNow === "distributor") return;
    let live = true;
    api
      .libraryBrowse({ query: text, source: scopeNow })
      .then((r) => {
        if (!live) return;
        setData(r);
        setError(null);
      })
      .catch((e) => {
        if (!live) return;
        setData(null);
        setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [text, scopeNow, revision, gen]);

  const reset = () => {
    setPage(0);
    setSel(null);
  };
  const chooseCat = (key: string | null) => {
    setCat(key);
    reset();
  };
  const entries = useMemo(() => (data?.entries ?? []).filter((e) => inCategory(e, cat)), [data, cat]);
  const tree = useMemo(() => categoryTree(data?.entries ?? []), [data]);
  const fps = useMemo(() => footprintRows(entries), [entries]);
  const fpPage = pageOf(fps, page);
  const partPage = pageOf(entries, page);
  const pg = kind === "footprint" ? fpPage : partPage;
  const selEntry = kind === "footprint" ? null : (entries.find((e) => keyOf(e) === sel) ?? null);
  const selFp = kind === "footprint" ? (fps.find((r) => `fp:${r.name}` === sel) ?? null) : null;
  const counts = data?.facets.sources ?? [];
  const countOf = (s: LibScope) => counts.filter((f) => (s === "imported" ? f.value.startsWith("imported:") : f.value === s)).reduce((n, f) => n + f.count, 0);
  const favourite = (e: LibraryEntry2) => {
    api
      .libraryMeta(e.id, { favourite: !e.favourite })
      .then(() => setGen((g) => g + 1))
      .catch((err) => setError(String(err)));
  };

  return (
    <div className="lib-dock">
      <nav className="lib-dock-nav" aria-label="Library scopes and categories">
        <ul className="tree" aria-label="Library scopes">
          {SCOPES.map(([id, label]) => {
            const why = scopeUnavailable(id, offline);
            return (
              <li key={id}>
                <button
                  type="button"
                  className={scopeNow === id ? "on" : ""}
                  aria-pressed={scopeNow === id}
                  aria-label={label}
                  disabled={!!why}
                  title={why ?? (id === "distributor" ? "Search the distributor catalogue (needs the network and your API credentials)" : `Show the ${label.toLowerCase()} library`)}
                  onClick={() => {
                    setScope(id);
                    setCat(null);
                    reset();
                  }}
                >
                  {label}
                  {id !== "distributor" && data && <span className="muted"> {countOf(id)}</span>}
                </button>
              </li>
            );
          })}
        </ul>
        <label className="lib-offline" title="Use only the built-in and project libraries; nothing outside the app and the open project is read, and the distributor is never asked">
          <input type="checkbox" aria-label="Offline mode" checked={offline} onChange={(e) => onOffline(e.target.checked)} /> Offline
        </label>
        {scopeNow !== "distributor" && (
          <ul className="tree lib-cats" aria-label="Library categories">
            <li>
              <button type="button" className={cat === null ? "on" : ""} aria-pressed={cat === null} onClick={() => chooseCat(null)}>
                All categories <span className="muted">{data?.entries.length ?? 0}</span>
              </button>
            </li>
            {tree.map((f) => (
              <li key={f.key}>
                <button type="button" className={cat === f.key ? "on" : ""} aria-pressed={cat === f.key} onClick={() => chooseCat(f.key)}>
                  {f.label} <span className="muted">{f.count}</span>
                </button>
                <ul className="tree">
                  {f.children.map((c) => (
                    <li key={c.key}>
                      <button type="button" className={cat === c.key ? "on sub" : "sub"} aria-pressed={cat === c.key} onClick={() => chooseCat(c.key)}>
                        {c.label} <span className="muted">{c.count}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}
      </nav>
      {scopeNow === "distributor" ? (
        <div className="lib-dock-main">
          <PartsPanel revision={revision} onImported={onImported} />
        </div>
      ) : (
        <>
          <div className="lib-dock-main">
            <div className="tabs lib-kinds" role="tablist" aria-label="Library kinds">
              {KINDS.map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  role="tab"
                  aria-selected={kind === id}
                  className={kind === id ? "tab on" : "tab"}
                  onClick={() => {
                    setKind(id);
                    reset();
                  }}
                >
                  {label}
                </button>
              ))}
              <input
                type="search"
                aria-label="Search library"
                placeholder="Name, id, MPN, supplier #, package…"
                value={text}
                onChange={(e) => {
                  setText(e.target.value);
                  reset();
                }}
              />
            </div>
            {error && (
              <p className="empty-state" role="alert">
                Library unavailable: {error}
              </p>
            )}
            {data && data.problems.length > 0 && (
              <p role="alert" className="error">
                {data.problems.length} library file(s) could not be read: {data.problems[0]}
              </p>
            )}
            {!error && data && pg.total === 0 && <p className="empty-state">No part matches the search in this scope.</p>}
            {pg.total > 0 && <Grid kind={kind} rows={partPage.rows} fps={fpPage.rows} selected={sel} onSelect={setSel} />}
            <div className="pager" role="group" aria-label="Pages">
              <button type="button" aria-label="Previous page" disabled={pg.page === 0} onClick={() => setPage(pg.page - 1)} title="Previous page">
                ‹
              </button>
              <span className="muted" data-testid="lib-page">
                {pg.from}–{pg.to} of {pg.total} {kind === "footprint" ? "footprint(s)" : "part(s)"} · page {pg.page + 1} of {pg.pages}
              </span>
              <button type="button" aria-label="Next page" disabled={pg.page >= pg.pages - 1} onClick={() => setPage(pg.page + 1)} title="Next page">
                ›
              </button>
            </div>
          </div>
          <Preview kind={kind} entry={selEntry} fpRow={selFp} onPlace={onPlace} onFavourite={favourite} busy={busy} />
        </>
      )}
    </div>
  );
}

// ---- Common parts ------------------------------------------------------------------------------

function CommonParts({ revision, busy, onPlace }: PlaceProps & { revision: number; busy: boolean }) {
  const [builtin, setBuiltin] = useState<LibraryEntry2[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api
      .libraryBrowse({ source: "builtin" })
      .then((r) => {
        if (!live) return;
        setBuiltin(r.entries);
        setError(null);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [revision]);
  const groups = useMemo(() => resolvePalette(COMMON_PARTS, builtin ?? []), [builtin]);
  if (error)
    return (
      <p className="empty-state" role="alert">
        Library unavailable: {error}
      </p>
    );
  if (!builtin) return <p className="empty-state">Reading the built-in library…</p>;
  if (groups.length === 0) return <p className="empty-state">The built-in library is disabled (Manage, Libraries), so there are no common parts to offer.</p>;
  return (
    <section className="lib-common" aria-label="Common parts">
      <p className="muted">Generic EAK parts. Their ratings are typical class values and stay unverified until you choose a real manufacturer part.</p>
      {groups.map((g) => (
        <div key={g.label} className="lib-common-group" role="group" aria-label={g.label}>
          <strong>
            {g.prefix} · {g.label}
          </strong>
          {g.variants.map((v) => (
            <button
              key={v.entry.id}
              type="button"
              disabled={busy}
              aria-label={`Place ${v.entry.def.name}`}
              title={`Place ${v.entry.def.name} (${v.entry.id}); drag onto the schematic or click to arm the Place tool`}
              draggable={true}
              onDragStart={(ev) => startDrag(ev, v.entry.def)}
              onDragEnd={endDrag}
              onClick={() => placeEntry(v.entry, onPlace)}
            >
              {v.pkg}
            </button>
          ))}
        </div>
      ))}
    </section>
  );
}

// ---- Standardisation ---------------------------------------------------------------------------

export function PartStandardisation({ revision }: { revision: number }) {
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [gen, setGen] = useState(0);
  useEffect(() => {
    let live = true;
    api
      .partStandardisation()
      .then((r) => {
        if (!live) return;
        setReport(r);
        setError(null);
      })
      .catch((e) => {
        if (!live) return;
        setReport(null);
        setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [revision, gen]);
  return (
    <section className="lib-standard" aria-label="Part standardisation">
      <div className="row">
        <p className="muted">
          The sourcing check over every placed part: MPN, supplier part number, datasheet, ratings, the supplier records stored with this project and the
          symbol, footprint and package match. Offline; nothing is fetched or written.
        </p>
        <button type="button" onClick={() => setGen((g) => g + 1)} title="Run the sourcing check again on the current revision">
          Run again
        </button>
      </div>
      {error && (
        <p className="empty-state" role="alert">
          Standardisation unavailable: {error}
        </p>
      )}
      {report && (
        <>
          <p role="status">
            {report.check} · revision {report.revision} · {report.errors} error(s), {report.warnings} warning(s) ·{" "}
            {report.outcome === "ERRORS" ? "parts need attention before a BOM" : "no errors in the implemented checks (not proof that the parts are orderable or suitable)"}
          </p>
          {report.findings.length === 0 ? (
            <p className="empty-state">No findings: place parts in the schematic first, or every implemented check found nothing.</p>
          ) : (
            <table className="bom" aria-label="Standardisation findings">
              <thead>
                <tr>
                  <th>Rule</th>
                  <th>Severity</th>
                  <th>Finding</th>
                </tr>
              </thead>
              <tbody>
                {report.findings.map((f, i) => (
                  <tr key={f.id || `${f.rule_id}-${i}`}>
                    <td>{f.rule_id}</td>
                    <td>{f.severity}</td>
                    <td>{f.message}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <h4>Not checked</h4>
          <ul aria-label="Not implemented">
            {report.not_implemented.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
          <h4>Limitations</h4>
          <ul aria-label="Limitations">
            {report.limitations.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

// ---- Dock --------------------------------------------------------------------------------------

interface Props extends PlaceProps {
  revision: number;
  busy: boolean;
  execute: (c: Command | Command[]) => Promise<void>;
  /** Called after a distributor import so the workspace re-reads the engine snapshot. */
  onImported: (message: string) => Promise<void>;
}

function readOffline(): boolean {
  try {
    return localStorage.getItem(OFFLINE_KEY) === "1";
  } catch {
    return false;
  }
}

export function LibraryDock({ revision, busy, execute, onPlace, onImported }: Props) {
  const [mode, setMode] = useState<Mode>("browse");
  const [offline, setOfflineState] = useState(readOffline);
  const [more, setMore] = useState(false);
  const setOffline = (on: boolean) => {
    setOfflineState(on);
    try {
      localStorage.setItem(OFFLINE_KEY, on ? "1" : "0");
    } catch {
      // the choice still applies to this session
    }
  };
  return (
    <section className="library-dock" aria-label="Library dock">
      <div className="tabs lib-modes" role="tablist" aria-label="Library modes">
        {MODES.map(([id, label]) => (
          <button key={id} type="button" role="tab" aria-selected={mode === id} className={mode === id ? "tab on" : "tab"} onClick={() => setMode(id)}>
            {label}
          </button>
        ))}
        {offline && (
          <span className="chip" title="Only the built-in and project libraries are used">
            Offline
          </span>
        )}
      </div>
      {mode === "browse" && <Browse revision={revision} busy={busy} offline={offline} onOffline={setOffline} onPlace={onPlace} onImported={onImported} />}
      {mode === "common" && <CommonParts revision={revision} busy={busy} onPlace={onPlace} />}
      {mode === "standard" && <PartStandardisation revision={revision} />}
      {mode === "manage" && (
        <div className="lib-manage">
          <LibraryPanel revision={revision} busy={busy} execute={execute} onPlace={onPlace} />
          <details open={more} onToggle={(e) => setMore(e.currentTarget.open)}>
            <summary>Favourites, tags, personal copies, KiCad and open-source imports</summary>
            {more && <LibraryBrowser />}
          </details>
        </div>
      )}
    </section>
  );
}
