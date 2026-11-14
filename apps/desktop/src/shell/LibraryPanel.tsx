// Component library browser (plan Phase 5). Lists the built-in catalogue merged with the
// project library via the `library_search` IPC, previews the symbol and footprint, shows
// provenance and verification state, and edits supplier properties / footprint assignment
// through typed commands (`SetComponentProperties`, `AssignComponentFootprint`,
// `ImportLibraryComponent`). Placing arms the schematic place tool (`PlaceSymbol`).
// The UI keeps no library model of its own: every list comes from the engine.
import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { Command, ComponentDef, FootprintCandidate, FootprintDef, LibraryPart } from "../types";

const NM_PER_MM = 1_000_000;
const mm = (nm: number) => nm / NM_PER_MM;

/** Symbol body and pins, drawn from the definition (nm -> mm). */
export function SymbolPreview({ def }: { def: ComponentDef }) {
  const s = def.symbol;
  const xs = s.pins.map((p) => mm(p.position.x));
  const ys = s.pins.map((p) => mm(p.position.y));
  const hw = Math.max(mm(s.body_half_w), ...xs.map(Math.abs), 1);
  const hh = Math.max(mm(s.body_half_h), ...ys.map(Math.abs), 1);
  const pad = Math.max(hw, hh) * 0.3;
  return (
    <svg role="img" aria-label={`Symbol preview of ${def.name}`} className="lib-preview" viewBox={`${-hw - pad} ${-hh - pad} ${2 * (hw + pad)} ${2 * (hh + pad)}`}>
      <rect x={-mm(s.body_half_w)} y={-mm(s.body_half_h)} width={2 * mm(s.body_half_w)} height={2 * mm(s.body_half_h)} fill="none" stroke="currentColor" strokeWidth={hw * 0.03} />
      {s.pins.map((p) => (
        <g key={p.number}>
          <line x1={mm(p.position.x)} y1={mm(p.position.y)} x2={mm(p.position.x) * 0.8} y2={mm(p.position.y) * 0.8} stroke="currentColor" strokeWidth={hw * 0.03} />
          <circle cx={mm(p.position.x)} cy={mm(p.position.y)} r={hw * 0.05} fill="var(--accent)" />
          <text x={mm(p.position.x)} y={mm(p.position.y)} fontSize={hw * 0.12} fill="currentColor">
            {p.number}
          </text>
        </g>
      ))}
    </svg>
  );
}

/** Pads and courtyard of a footprint (nm -> mm). */
export function FootprintPreview({ fp }: { fp: FootprintDef }) {
  const ext = Math.max(mm(fp.courtyard_half_w), mm(fp.courtyard_half_h), ...fp.pads.flatMap((p) => [Math.abs(mm(p.position.x)) + mm(p.size_x) / 2, Math.abs(mm(p.position.y)) + mm(p.size_y) / 2]), 0.5);
  const v = ext * 1.2;
  return (
    <svg role="img" aria-label={`Footprint preview of ${fp.name}`} className="lib-preview" viewBox={`${-v} ${-v} ${2 * v} ${2 * v}`}>
      <rect x={-mm(fp.courtyard_half_w)} y={-mm(fp.courtyard_half_h)} width={2 * mm(fp.courtyard_half_w)} height={2 * mm(fp.courtyard_half_h)} fill="none" stroke="currentColor" strokeDasharray={`${v * 0.05} ${v * 0.05}`} strokeWidth={v * 0.015} />
      {fp.pads.map((p) =>
        p.shape === "rect" ? (
          <rect key={p.number} data-pad={p.number} x={mm(p.position.x) - mm(p.size_x) / 2} y={mm(p.position.y) - mm(p.size_y) / 2} width={mm(p.size_x)} height={mm(p.size_y)} fill="var(--accent)" opacity={0.8} />
        ) : (
          <ellipse key={p.number} data-pad={p.number} cx={mm(p.position.x)} cy={mm(p.position.y)} rx={mm(p.size_x) / 2} ry={mm(p.size_y) / 2} fill="var(--accent)" opacity={0.8} />
        ),
      )}
    </svg>
  );
}

interface Props {
  /** Project revision; the list is refetched whenever it changes. */
  revision: number;
  busy: boolean;
  /** Submit typed commands to the engine. */
  execute: (c: Command | Command[]) => Promise<void>;
  onPlace: (p: { id: string; name: string }) => void;
}

const emptyToNull = (s: string) => (s.trim() === "" ? null : s.trim());
const LABELS = { manufacturer: "Manufacturer", mpn: "MPN", lcsc: "LCSC id", datasheet: "Datasheet URL" } as const;

export function LibraryPanel({ revision, busy, execute, onPlace }: Props) {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("");
  const [source, setSource] = useState("all");
  const [parts, setParts] = useState<LibraryPart[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selId, setSelId] = useState<string | null>(null);
  const [form, setForm] = useState({ manufacturer: "", mpn: "", lcsc: "", datasheet: "" });
  const [candidates, setCandidates] = useState<FootprintCandidate[]>([]);
  const [fpChoice, setFpChoice] = useState("");

  useEffect(() => {
    let live = true;
    api
      .librarySearch(query, category, source)
      .then((r) => {
        if (!live) return;
        setParts(r);
        setError(null);
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [query, category, source, revision]);

  useEffect(() => {
    let live = true;
    api
      .librarySearch("", "", "all")
      .then((r) => live && setCategories([...new Set(r.map((p) => p.def.category))].sort()))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [revision]);

  const sel = useMemo(() => parts.find((p) => p.id === selId) ?? null, [parts, selId]);

  useEffect(() => {
    if (!sel) return;
    setForm({ manufacturer: sel.def.manufacturer ?? "", mpn: sel.def.mpn ?? "", lcsc: sel.def.lcsc ?? "", datasheet: sel.def.datasheet ?? "" });
    setFpChoice("");
    let live = true;
    api
      .libraryFootprintCandidates(sel.id)
      .then((c) => live && setCandidates(c))
      .catch(() => live && setCandidates([]));
    return () => {
      live = false;
    };
  }, [sel]);

  const saveProps = () =>
    sel &&
    execute({
      type: "SetComponentProperties",
      component_id: sel.id,
      manufacturer: emptyToNull(form.manufacturer),
      mpn: emptyToNull(form.mpn),
      lcsc: emptyToNull(form.lcsc),
      datasheet: emptyToNull(form.datasheet),
    });
  const assign = () => {
    const c = candidates.find((x) => x.name === fpChoice);
    if (sel && c) void execute({ type: "AssignComponentFootprint", component_id: sel.id, footprint: c.footprint, pin_map: c.pin_map });
  };
  const dirty = sel && (form.manufacturer !== (sel.def.manufacturer ?? "") || form.mpn !== (sel.def.mpn ?? "") || form.lcsc !== (sel.def.lcsc ?? "") || form.datasheet !== (sel.def.datasheet ?? ""));

  return (
    <div className="panel-body library-panel">
      <input aria-label="Search library" placeholder="Search name, MPN, LCSC, footprint…" value={query} onChange={(e) => setQuery(e.target.value)} />
      <div className="row">
        <select aria-label="Category filter" value={category} onChange={(e) => setCategory(e.target.value)}>
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <select aria-label="Source filter" value={source} onChange={(e) => setSource(e.target.value)}>
          <option value="all">All sources</option>
          <option value="builtin">Built-in</option>
          <option value="project">Project</option>
        </select>
      </div>
      {error && (
        <p className="empty-state" role="alert">
          Library unavailable: {error}
        </p>
      )}
      {!error && parts.length === 0 && <p className="empty-state">No part matches the search.</p>}
      <ul className="tree" aria-label="Library parts">
        {parts.map((p) => (
          <li key={p.id}>
            <button type="button" className={p.id === selId ? "on" : ""} aria-pressed={p.id === selId} onClick={() => setSelId(p.id)}>
              <strong>{p.def.name}</strong>{" "}
              <span className="muted">
                {p.def.category} · {p.def.footprint.name} · {p.source}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {sel && (
        <section className="lib-detail" aria-label={`Details of ${sel.def.name}`}>
          <h3>{sel.def.name}</h3>
          <div className="lib-previews">
            <SymbolPreview def={sel.def} />
            <FootprintPreview fp={sel.def.footprint} />
          </div>
          <dl className="lib-facts">
            <dt>ID</dt>
            <dd>{sel.id}</dd>
            <dt>Source</dt>
            <dd>{sel.source === "project" ? "Project library" : sel.in_project ? "Built-in (copy in project)" : "Built-in catalogue"}</dd>
            <dt>Footprint</dt>
            <dd>
              {sel.def.footprint.name} · {sel.def.footprint.pads.length} pads
            </dd>
            <dt>Provenance</dt>
            <dd data-testid="lib-provenance">{sel.def.provenance}</dd>
            <dt>Licence</dt>
            <dd className="muted">Not recorded by the library model (no licence field in EAK-IR yet).</dd>
            <dt>Ratings</dt>
            <dd>{sel.def.ratings.length === 0 ? "none" : `${sel.def.ratings.length - sel.unverified_ratings} verified, ${sel.unverified_ratings} unverified`}</dd>
          </dl>
          <div className="row">
            <button type="button" className="primary" disabled={busy} onClick={() => onPlace({ id: sel.id, name: sel.def.name })} title="Arm the schematic place tool with this part; the footprint is created by Sync to PCB">
              Place on schematic
            </button>
            {!sel.in_project && (
              <button type="button" disabled={busy} onClick={() => void execute({ type: "ImportLibraryComponent", component_id: sel.id })} title="Copy into the project library so it can be edited">
                Add to project
              </button>
            )}
          </div>
          {sel.in_project ? (
            <>
              <h3>Part properties</h3>
              {(Object.keys(LABELS) as (keyof typeof LABELS)[]).map((k) => (
                <label key={k} className="field">
                  <span>{LABELS[k]}</span>
                  <input aria-label={LABELS[k]} value={form[k]} onChange={(e) => setForm({ ...form, [k]: e.target.value })} />
                </label>
              ))}
              <button type="button" disabled={busy || !dirty} onClick={() => void saveProps()}>
                Save properties
              </button>
              <h3>Footprint assignment</h3>
              {candidates.length === 0 ? (
                <p className="empty-state">No other library footprint fits this symbol&apos;s pins.</p>
              ) : (
                <div className="row">
                  <select aria-label="Assign footprint" value={fpChoice} onChange={(e) => setFpChoice(e.target.value)}>
                    <option value="">Choose a footprint…</option>
                    {candidates.map((c) => (
                      <option key={c.name} value={c.name}>
                        {c.name} ({c.footprint.pads.length} pads)
                      </option>
                    ))}
                  </select>
                  <button type="button" disabled={busy || !fpChoice} onClick={assign}>
                    Assign
                  </button>
                </div>
              )}
              <p className="muted">Refused while PCB footprints of this part exist.</p>
            </>
          ) : (
            <p className="muted">Add the part to the project to edit its properties or footprint.</p>
          )}
        </section>
      )}
    </div>
  );
}
