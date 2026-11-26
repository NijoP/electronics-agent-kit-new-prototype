// Component library browser (plan Phase 5). Lists the built-in catalogue merged with the
// project library via the `library_search` IPC, previews the symbol and footprint, shows
// provenance and verification state, and edits supplier properties / footprint assignment
// through typed commands (`SetComponentProperties`, `AssignComponentFootprint`,
// `ImportLibraryComponent`). Placing arms the schematic place tool (`PlaceSymbol`).
// The UI keeps no library model of its own: every list comes from the engine.
import { useEffect, useMemo, useState } from "react";
import { api, pickModelFile } from "../api";
import { setDraggedComponent, EAK_COMPONENT_MIME } from "../dragState";
import { VerificationReport } from "../library/VerificationReport";
import { LibraryManager } from "./LibraryManager";
import { newPartDefinition, newPartIdProblem } from "../partEditor/model";
import type { Command, ComponentDef, FootprintCandidate, FootprintDef, LibraryEntry, LibraryPart, ModelFormat, ModelQuality, ModelRef } from "../types";

const FORMAT_BY_EXT: Record<string, ModelFormat> = { step: "step", stp: "step", wrl: "wrl", glb: "glb" };
const formatOf = (path: string): ModelFormat | null => FORMAT_BY_EXT[path.split(".").pop()?.toLowerCase() ?? ""] ?? null;
export const QUALITIES: { value: ModelQuality | ""; label: string }[] = [
  { value: "", label: "Not declared (shown as unclassified)" },
  { value: "VENDOR_EXACT", label: "Vendor exact" },
  { value: "LIBRARY_VERIFIED", label: "Library verified" },
  { value: "DERIVED_VALIDATED", label: "Derived, validated" },
];
type Xyz = [string, string, string];
export interface ModelForm { path: string; quality: ModelQuality | ""; licence: string; provenance: string; offset: Xyz; rotation: Xyz; scale: Xyz }
const emptyModelForm = (): ModelForm => ({ path: "", quality: "", licence: "", provenance: "", offset: ["0", "0", "0"], rotation: ["0", "0", "0"], scale: ["1", "1", "1"] });
export const modelFormOf = (m: ModelRef | null | undefined): ModelForm =>
  m
    ? { path: m.source.path, quality: m.quality ?? "", licence: m.licence ?? "", provenance: m.provenance, offset: m.transform.offset_mm.map(String) as Xyz, rotation: m.transform.rotation_deg.map(String) as Xyz, scale: m.transform.scale.map(String) as Xyz }
    : emptyModelForm();
const nums = (v: Xyz): [number, number, number] | null => {
  const n = v.map((s) => (s.trim() === "" ? NaN : Number(s)));
  return n.every(Number.isFinite) ? (n as [number, number, number]) : null;
};
/** The reference described by the form, or the reason it cannot be attached yet. */
export function modelRefFromForm(f: ModelForm): { ref: ModelRef } | { problem: string } {
  const format = formatOf(f.path);
  if (!f.path.trim()) return { problem: "Choose a model file." };
  if (!format) return { problem: "Use a .step, .stp, .wrl or .glb file." };
  if (!f.provenance.trim()) return { problem: "Record where the model came from (provenance)." };
  const offset = nums(f.offset), rotation = nums(f.rotation), scale = nums(f.scale);
  if (!offset || !rotation || !scale) return { problem: "Offset, rotation and scale must be numbers." };
  if (scale.some((s) => s <= 0)) return { problem: "Scale must be positive." };
  return { ref: { source: { kind: "file", path: f.path.trim() }, format, quality: f.quality || null, transform: { offset_mm: offset, rotation_deg: rotation, scale }, licence: emptyToNull(f.licence), provenance: f.provenance.trim() } };
}

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
  /** Submit typed commands; resolves to `false` when they were not accepted (S18). */
  execute: (c: Command | Command[]) => Promise<boolean | void>;
  /** Arm the Place tool with this part (the full entry, so a personal or imported part that is
   *  not in the built-in list is placed as itself, never replaced by another part). */
  onPlace: (p: LibraryEntry) => void;
  /** Open the symbol or footprint editor of a project-library part (D2); absent: no editors here. */
  onEdit?: (kind: "symbol" | "footprint", componentId: string) => void;
}

/** "New part…": creates a two-pin starting definition in the project library and opens its symbol editor (D2). */
function NewPart({ busy, taken, execute, onEdit }: { busy: boolean; taken: Set<string>; execute: Props["execute"]; onEdit: NonNullable<Props["onEdit"]> }) {
  const [f, setF] = useState({ id: "", name: "", prefix: "U", category: "custom" });
  const [refused, setRefused] = useState(false);
  const problem = newPartIdProblem(f.id, taken) ?? (!f.name.trim() ? "Enter a part name." : !f.prefix.trim() ? "Enter a reference prefix (U, R, J…)." : null);
  const create = async () => {
    if (problem) return;
    const id = f.id.trim();
    const ok = await execute({ type: "UpsertComponentDefinition", definition: newPartDefinition(id, f.name.trim(), f.prefix.trim(), f.category.trim() || "custom") });
    // Opens only if the engine accepted it: a part document exists only for a project-library part (S18).
    setRefused(ok === false);
    if (ok === false) return;
    onEdit("symbol", id);
    setF({ id: "", name: "", prefix: f.prefix, category: f.category });
  };
  return (
    <details className="new-part">
      <summary>New part…</summary>
      <p className="muted">Starts a two-pin box symbol on a two-pad SMD footprint in the project library, then opens the symbol editor. Edit both before placing it.</p>
      <label className="field">
        <span>New part id</span>
        <input aria-label="New part id" value={f.id} onChange={(e) => setF({ ...f, id: e.target.value })} placeholder="e.g. MY_SENSOR" />
      </label>
      <label className="field">
        <span>New part name</span>
        <input aria-label="New part name" value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} />
      </label>
      <label className="field">
        <span>New part reference prefix</span>
        <input aria-label="New part reference prefix" value={f.prefix} onChange={(e) => setF({ ...f, prefix: e.target.value })} />
      </label>
      <label className="field">
        <span>New part category</span>
        <input aria-label="New part category" value={f.category} onChange={(e) => setF({ ...f, category: e.target.value })} />
      </label>
      {problem && f.id !== "" && <p className="muted">{problem}</p>}
      {refused && (
        <p role="alert" className="error">
          Part not created: the engine refused it or another command was running. The status bar says why.
        </p>
      )}
      <button type="button" className="primary" disabled={busy || !!problem} title={busy ? BUSY : problem ?? "Create the part (UpsertComponentDefinition) and open its symbol editor"} onClick={() => void create()}>
        Create part
      </button>
    </details>
  );
}

/** Why a library action is unavailable while another command runs (S17). */
export const BUSY = "Another command is running; wait for it to finish";
const emptyToNull = (s: string) => (s.trim() === "" ? null : s.trim());
const LABELS = { manufacturer: "Manufacturer", mpn: "MPN", distributor: "Distributor id", datasheet: "Datasheet URL" } as const;

/** The Place-tool entry for a library definition. */
export function entryOf(d: ComponentDef): LibraryEntry {
  return { id: d.id, name: d.name, category: d.category, refdes_prefix: d.refdes_prefix, default_value: d.default_value, pins: d.symbol.pins.map((p) => [p.number, p.name]) };
}

/** Where the shown definition comes from, and what placing it does. */
export function sourceLabel(p: Pick<LibraryPart, "source" | "in_project">): string {
  if (p.source === "project") return "Project library";
  if (p.source === "builtin") return p.in_project ? "Built-in (copy in project)" : "Built-in catalogue";
  const lib = p.source === "personal" ? "Personal library" : `Imported library ${p.source.replace(/^imported:/, "")}`;
  return p.in_project ? `${lib} (copy in project)` : `${lib}; placing copies it into the project`;
}

export function LibraryPanel({ revision, busy, execute, onPlace, onEdit }: Props) {
  // Every part id any library already uses (a new part must not shadow one).
  const [allIds, setAllIds] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("");
  const [source, setSource] = useState("all");
  const [parts, setParts] = useState<LibraryPart[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selId, setSelId] = useState<string | null>(null);
  const [form, setForm] = useState({ manufacturer: "", mpn: "", distributor: "", datasheet: "" });
  const [candidates, setCandidates] = useState<FootprintCandidate[]>([]);
  const [fpChoice, setFpChoice] = useState("");
  const [mf, setMf] = useState<ModelForm>(emptyModelForm);
  // Bumped when a library is enabled or disabled, so the search reflects it at once.
  const [libGen, setLibGen] = useState(0);

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
  }, [query, category, source, revision, libGen]);

  useEffect(() => {
    let live = true;
    api
      .librarySearch("", "", "all")
      .then((r) => {
        if (!live) return;
        setCategories([...new Set(r.map((p) => p.def.category))].sort());
        setAllIds(new Set(r.map((p) => p.id)));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [revision, libGen]);

  const sel = useMemo(() => parts.find((p) => p.id === selId) ?? null, [parts, selId]);

  useEffect(() => {
    if (!sel) return;
    setForm({ manufacturer: sel.def.manufacturer ?? "", mpn: sel.def.mpn ?? "", distributor: sel.def.distributor ?? "", datasheet: sel.def.datasheet ?? "" });
    setFpChoice("");
    setMf(modelFormOf(sel.def.model3d?.model_ref));
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
      distributor: emptyToNull(form.distributor),
      datasheet: emptyToNull(form.datasheet),
    });
  const assign = () => {
    const c = candidates.find((x) => x.name === fpChoice);
    if (sel && c) void execute({ type: "AssignComponentFootprint", component_id: sel.id, footprint: c.footprint, pin_map: c.pin_map });
  };
  const modelCheck = modelRefFromForm(mf);
  const withModel = (ref: ModelRef | null): Command | null =>
    sel ? { type: "UpsertComponentDefinition", definition: { ...sel.def, model3d: { kind: "box", color: "#808080", external_ref: null, ...sel.def.model3d, model_ref: ref } } } : null;
  const attachModel = () => {
    const c = "ref" in modelCheck ? withModel(modelCheck.ref) : null;
    if (c) void execute(c);
  };
  const removeModel = () => {
    const c = withModel(null);
    if (c) void execute(c);
  };
  const browseModel = async () => {
    const p = await pickModelFile("Choose a 3D model (STEP, WRL or GLB)");
    if (p) setMf((f) => ({ ...f, path: p }));
  };
  const setXyz = (k: "offset" | "rotation" | "scale", i: number, v: string) =>
    setMf((f) => ({ ...f, [k]: f[k].map((x, j) => (j === i ? v : x)) as Xyz }));
  const dirty = sel && (form.manufacturer !== (sel.def.manufacturer ?? "") || form.mpn !== (sel.def.mpn ?? "") || form.distributor !== (sel.def.distributor ?? "") || form.datasheet !== (sel.def.datasheet ?? ""));

  return (
    <div className="panel-body library-panel">
      <LibraryManager revision={revision} onChanged={() => setLibGen((n) => n + 1)} />
      {onEdit && <NewPart busy={busy} taken={allIds} execute={execute} onEdit={onEdit} />}
      <input aria-label="Search library" placeholder="Search name, MPN, Distributor, footprint…" value={query} onChange={(e) => setQuery(e.target.value)} />
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
          <option value="personal">Personal</option>
          <option value="imported">Imported</option>
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
            <button
              type="button"
              className={p.id === selId ? "on" : ""}
              aria-pressed={p.id === selId}
              onClick={() => setSelId(p.id)}
              draggable={true}
              onDragStart={(e) => {
                setDraggedComponent({ id: p.id, name: p.def.name, category: p.def.category, footprint: p.def.footprint?.name });
                e.dataTransfer.setData(EAK_COMPONENT_MIME, JSON.stringify({ id: p.id, name: p.def.name, category: p.def.category, footprint: p.def.footprint?.name }));
                e.dataTransfer.setData("text/plain", p.id);
                e.dataTransfer.effectAllowed = "copy";
              }}
              onDragEnd={() => {
                setDraggedComponent(null);
              }}
              title="Click to select · Drag onto schematic to place"
            >
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
          <div
            className="lib-previews"
            draggable={true}
            onDragStart={(e) => {
              setDraggedComponent({ id: sel.id, name: sel.def.name, category: sel.def.category, footprint: sel.def.footprint?.name });
              e.dataTransfer.setData(EAK_COMPONENT_MIME, JSON.stringify({ id: sel.id, name: sel.def.name, category: sel.def.category, footprint: sel.def.footprint?.name }));
              e.dataTransfer.setData("text/plain", sel.id);
              e.dataTransfer.effectAllowed = "copy";
            }}
            onDragEnd={() => {
              setDraggedComponent(null);
            }}
            style={{ cursor: "grab" }}
            title="Drag symbol onto schematic to place"
          >
            <SymbolPreview def={sel.def} />
            <FootprintPreview fp={sel.def.footprint} />
          </div>
          <dl className="lib-facts">
            <dt>ID</dt>
            <dd>{sel.id}</dd>
            <dt>Source</dt>
            <dd>{sourceLabel(sel)}</dd>
            <dt>Footprint</dt>
            <dd>
              {sel.def.footprint.name} · {sel.def.footprint.pads.length} pads
            </dd>
            <dt>Provenance</dt>
            <dd data-testid="lib-provenance">{sel.def.provenance}</dd>
            <dt>Licence</dt>
            <dd className="muted">{sel.def.model3d?.model_ref ? `3D model: ${sel.def.model3d.model_ref.licence ?? "not recorded"}. ` : ""}Part definition licence is not recorded by the library model.</dd>
            <dt>Ratings</dt>
            <dd>{sel.def.ratings.length === 0 ? "none" : `${sel.def.ratings.length - sel.unverified_ratings} verified, ${sel.unverified_ratings} unverified`}</dd>
          </dl>
          <VerificationReport id={sel.id} />
          <div className="row">
            <button type="button" className="primary" disabled={busy} onClick={() => onPlace(entryOf(sel.def))} title={busy ? BUSY : "Arm the schematic place tool with this part; the footprint is created by Sync to PCB"}>
              Place on schematic
            </button>
            {!sel.in_project && (
              <button type="button" disabled={busy} onClick={() => void execute({ type: "ImportLibraryComponent", component_id: sel.id })} title={busy ? BUSY : "Copy into the project library so it can be edited"}>
                Add to project
              </button>
            )}
          </div>
          {sel.in_project ? (
            <>
              {onEdit && (
                <div className="row">
                  <button type="button" onClick={() => onEdit("symbol", sel.id)} title="Open the symbol editor: pin table, body graphics, pin-to-pad map (saved with UpsertComponentDefinition)">
                    Edit symbol
                  </button>
                  <button type="button" onClick={() => onEdit("footprint", sel.id)} title="Open the footprint editor: pads, drill, courtyard (saved with AssignComponentFootprint)">
                    Edit footprint
                  </button>
                </div>
              )}
              <h3>Part properties</h3>
              {(Object.keys(LABELS) as (keyof typeof LABELS)[]).map((k) => (
                <label key={k} className="field">
                  <span>{LABELS[k]}</span>
                  <input aria-label={LABELS[k]} value={form[k]} onChange={(e) => setForm({ ...form, [k]: e.target.value })} />
                </label>
              ))}
              <button type="button" disabled={busy || !dirty} title={busy ? BUSY : !dirty ? "Nothing to save: no property was changed" : "Save the part properties (SetComponentProperties)"} onClick={() => void saveProps()}>
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
                  <button type="button" disabled={busy || !fpChoice} title={busy ? BUSY : !fpChoice ? "Choose a footprint first" : "Assign the chosen footprint (AssignComponentFootprint)"} onClick={assign}>
                    Assign
                  </button>
                </div>
              )}
              <p className="muted">Refused while PCB footprints of this part exist.</p>
              <h3>3D model</h3>
              <p className="muted">Models are not bundled: point at a STEP, WRL or GLB file you own. Without a model the viewer draws an approximate box and marks the part as missing.</p>
              <label className="field">
                <span>Model file</span>
                <input aria-label="3D model path" value={mf.path} onChange={(e) => setMf({ ...mf, path: e.target.value })} placeholder="Absolute path to .step, .stp, .wrl or .glb" />
              </label>
              <button type="button" onClick={() => void browseModel()}>Browse…</button>
              <label className="field">
                <span>Quality</span>
                <select aria-label="3D model quality" value={mf.quality} onChange={(e) => setMf({ ...mf, quality: e.target.value as ModelQuality | "" })}>
                  {QUALITIES.map((q) => (
                    <option key={q.value} value={q.value}>{q.label}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>Licence</span>
                <input aria-label="3D model licence" value={mf.licence} onChange={(e) => setMf({ ...mf, licence: e.target.value })} placeholder="e.g. CC-BY-SA-4.0" />
              </label>
              <label className="field">
                <span>Provenance</span>
                <input aria-label="3D model provenance" value={mf.provenance} onChange={(e) => setMf({ ...mf, provenance: e.target.value })} placeholder="Library, URL or author" />
              </label>
              {([["offset", "Offset mm (x, y, z)"], ["rotation", "Rotation deg (x, y, z)"], ["scale", "Scale (x, y, z)"]] as const).map(([k, label]) => (
                <div key={k} className="row">
                  <span>{label}</span>
                  {[0, 1, 2].map((i) => (
                    <input key={i} aria-label={`3D model ${k} ${"xyz"[i]}`} size={5} value={mf[k][i]} onChange={(e) => setXyz(k, i, e.target.value)} />
                  ))}
                </div>
              ))}
              {"problem" in modelCheck && <p className="muted">{modelCheck.problem}</p>}
              <div className="row">
                <button type="button" disabled={busy || !("ref" in modelCheck)} title={busy ? BUSY : "problem" in modelCheck ? modelCheck.problem : "Attach the 3D model to the part (UpsertComponentDefinition)"} onClick={attachModel}>{sel.def.model3d?.model_ref ? "Update 3D model" : "Attach 3D model"}</button>
                {sel.def.model3d?.model_ref && <button type="button" disabled={busy} title={busy ? BUSY : "Remove the 3D model reference from the part"} onClick={removeModel}>Remove 3D model</button>}
              </div>
            </>
          ) : (
            <>
              <p data-testid="lib-model-readonly" className="muted">3D model: {sel.def.model3d?.model_ref ? `${sel.def.model3d.model_ref.format.toUpperCase()} · ${sel.def.model3d.model_ref.quality ?? "quality not declared"} · ${sel.def.model3d.model_ref.provenance}` : "none (approximate box)"}</p>
            <p className="muted">Add the part to the project to edit its properties, symbol or footprint.</p>
            </>
          )}
        </section>
      )}
    </div>
  );
}
