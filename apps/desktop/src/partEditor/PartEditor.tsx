// Symbol and footprint editor documents (Phase 1 slice D2).
//
// One component of the project library is edited as a draft: the pin table, the body graphics
// (B1 primitives) and the pin-to-pad map in the symbol editor; the pad table (shape, size, drill)
// and the courtyard in the footprint editor. The canvas draws the draft with the A4 Canvas2D
// renderer; a click selects a pin, pad or graphic, a drag moves a pin or pad on the editor grid.
// Nothing is written until Save, which sends one typed command: `UpsertComponentDefinition` for
// the symbol, `AssignComponentFootprint` for the footprint. The engine validates it again and
// may refuse it (for example a footprint change while the part is placed on the PCB).
import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { PickIndex } from "../render/pick";
import { PcbSceneCanvas } from "../render/PcbSceneCanvas";
import type { OverlayItem, OverlayScene, PickRef } from "../render/types";
import type { Command, ComponentDef, Graphic, PadDef, Point, SymbolPin } from "../types";
import { fit, visibleViewBox, type Size, type ViewBox } from "../viewport";
import {
  addPad,
  addPin,
  deletePad,
  deletePin,
  draftProblems,
  fmtMm,
  fmtPoints,
  footprintScene,
  fromMm,
  GRAPHIC_KINDS,
  moveItem,
  newGraphic,
  normDef,
  ORIENTATIONS,
  parseList,
  rebase,
  parsePoints,
  pinsOfPad,
  PIN_TYPES,
  renamePad,
  renamePin,
  sameSlice,
  setPadPins,
  setPinPads,
  snapToIr,
  symbolScene,
  type GraphicKind,
  type PartEditorKind,
} from "./model";

interface Props {
  kind: PartEditorKind;
  /** The engine's current definition (project library). */
  def: ComponentDef;
  /** Why writes are not possible now (busy, blocked), or null. */
  disabled: string | null;
  /** Number of placed PCB footprints of this part (the engine refuses a footprint change then). */
  placedOnPcb: number;
  execute: (c: Command | Command[]) => Promise<void>;
  /** The draft differs from the engine's definition (tab marker, close confirmation). */
  onDirty: (dirty: boolean) => void;
}

/** Grid a dragged pin (symbol) or pad (footprint) snaps to, in mm. */
const GRID_MM: Record<PartEditorKind, number> = { symbol: 1.27, footprint: 0.05 };

/** A millimetre field that commits on blur or Enter (typing never sends a half-typed number). */
function MmInput({ label, nm, onCommit, allowEmpty }: { label: string; nm: number | null; onCommit: (nm: number | null) => void; allowEmpty?: boolean }) {
  const shown = nm === null ? "" : fmtMm(nm);
  const [text, setText] = useState(shown);
  useEffect(() => setText(shown), [shown]);
  const commit = () => {
    const v = fromMm(text);
    if (v === null && !(allowEmpty && text.trim() === "")) return setText(shown);
    if (v !== nm) onCommit(v);
  };
  return <input className="pe-num" aria-label={label} value={text} inputMode="decimal" onChange={(e) => setText(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />;
}
/** A text field that commits on blur or Enter. */
function TextInput({ label, value, onCommit, className }: { label: string; value: string; onCommit: (v: string) => void; className?: string }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => text !== value && onCommit(text);
  return <input className={className} aria-label={label} value={text} onChange={(e) => setText(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />;
}

/** The pane the canvas is drawn in, measured in CSS pixels. */
function useSize(ref: RefObject<HTMLDivElement | null>): Size {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setSize((s) => (s.width === r.width && s.height === r.height ? s : { width: r.width, height: r.height }));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return size;
}

export function PartEditor({ kind, def, disabled, placedOnPcb, execute, onDirty }: Props) {
  const [draft, setDraft] = useState<ComponentDef>(() => normDef(def));
  const [selected, setSelected] = useState<PickRef | null>(null);
  const [canvasFailed, setCanvasFailed] = useState(false);
  // The engine's definition changed (a save here, or a change elsewhere): follow it when the draft
  // had no edits of its own, else keep the edits and say the engine moved on.
  const prev = useRef(def);
  const [stale, setStale] = useState(false);
  useEffect(() => {
    if (prev.current === def) return;
    // This editor's slice is unchanged (a refreshed snapshot, or a save by the other editor):
    // keep the draft's edits and take the rest of the definition from the engine.
    if (sameSlice(kind, prev.current, def)) setDraft((d) => rebase(kind, def, d));
    else if (sameSlice(kind, draft, prev.current) || sameSlice(kind, draft, def)) {
      setDraft(normDef(def));
      setStale(false);
    } else {
      setDraft((d) => rebase(kind, def, d));
      setStale(true);
    }
    prev.current = def;
  }, [def]);
  const dirty = !sameSlice(kind, draft, def);
  useEffect(() => onDirty(dirty), [dirty]);
  useEffect(() => () => onDirty(false), []);

  const problems = draftProblems(draft);
  const footprintRefused =
    kind === "footprint" && placedOnPcb > 0 ? `${def.id} is placed on the PCB (${placedOnPcb} footprint(s)); the engine changes a footprint only while it is not placed, so delete those footprints first` : null;
  const saveWhy = disabled ?? (!dirty ? "No changes to save" : (footprintRefused ?? (problems.length ? `Fix first: ${problems[0]}` : null)));
  const save = () => {
    if (saveWhy) return;
    void execute(
      kind === "symbol"
        ? {
            type: "UpsertComponentDefinition",
            definition: { ...def, name: draft.name, category: draft.category, refdes_prefix: draft.refdes_prefix, default_value: draft.default_value, symbol: draft.symbol, pin_map: draft.pin_map },
          }
        : { type: "AssignComponentFootprint", component_id: def.id, footprint: draft.footprint, pin_map: draft.pin_map },
    );
  };
  const revert = () => {
    setDraft(normDef(def));
    setStale(false);
    setSelected(null);
  };

  // ---- Canvas.
  const drawn = kind === "symbol" ? draft.symbol : draft.footprint;
  const scene = useMemo(() => (kind === "symbol" ? symbolScene(draft.symbol) : footprintScene(draft.footprint)), [kind, drawn]);
  const pane = useRef<HTMLDivElement>(null);
  const size = useSize(pane);
  const drag = useRef<PickRef | null>(null);
  // The view fits the part; it is frozen during a drag so the part does not shift under the pointer.
  const [frozen, setFrozen] = useState<ViewBox | null>(null);
  const b = scene.bounds;
  const fitted = useMemo(() => fit(b ?? { minX: -5, minY: -5, maxX: 5, maxY: 5 }, 0.15), [b?.minX, b?.minY, b?.maxX, b?.maxY]);
  const vb = visibleViewBox(frozen ?? fitted, size);
  const picker = useMemo(() => new PickIndex(scene), [scene]);
  const toWorld = (e: ReactPointerEvent): [number, number] | null => {
    const r = pane.current?.getBoundingClientRect();
    if (!r || size.width <= 0 || vb.w <= 0) return null;
    const s = size.width / vb.w;
    return [vb.x + (e.clientX - r.left) / s, vb.y + (e.clientY - r.top) / s];
  };
  const onDown = (e: ReactPointerEvent) => {
    const p = toWorld(e);
    if (!p) return;
    const hit = picker.pick(p[0], p[1], { tol: 4 / (size.width / vb.w) });
    setSelected(hit?.ref ?? null);
    if (hit && (hit.ref.kind === "pin" || hit.ref.kind === "pad")) {
      drag.current = hit.ref;
      setFrozen(fitted);
      (e.target as Element).setPointerCapture?.(e.pointerId);
    }
  };
  const onMove = (e: ReactPointerEvent) => {
    const d = drag.current;
    const p = d && toWorld(e);
    if (!d || !p) return;
    const to = snapToIr(p[0], p[1], GRID_MM[kind]);
    const which = d.kind === "pin" ? "pin" : "pad";
    const cur = which === "pin" ? draft.symbol.pins.find((x) => x.number === d.id)?.position : draft.footprint.pads.find((x) => x.number === d.id)?.position;
    if (cur && (cur.x !== to.x || cur.y !== to.y)) setDraft((x) => moveItem(x, which, d.id, to));
  };
  const onUp = () => {
    drag.current = null;
    setFrozen(null);
  };
  const overlay: OverlayScene = useMemo(() => {
    const items: OverlayItem[] = [];
    const o = selected && scene.objects.find((x) => x.ref?.kind === selected.kind && x.ref.id === selected.id);
    if (o?.selPoly) items.push({ pts: o.selPoly, closed: true, colour: "--accent", px: 2 });
    else if (o?.pathPts) items.push({ pts: o.pathPts, closed: false, colour: "--accent", px: 3 });
    return { items };
  }, [selected, scene]);

  // ---- Edits.
  const set = (f: (d: ComponentDef) => ComponentDef) => setDraft((d) => f(d));
  const setPin = (number: string, patch: Partial<SymbolPin>) => set((d) => ({ ...d, symbol: { ...d.symbol, pins: d.symbol.pins.map((p) => (p.number === number ? { ...p, ...patch } : p)) } }));
  const setPad = (number: string, patch: Partial<PadDef>) => set((d) => ({ ...d, footprint: { ...d.footprint, pads: d.footprint.pads.map((p) => (p.number === number ? { ...p, ...patch } : p)) } }));
  const setGraphic = (i: number, g: Graphic) => set((d) => ({ ...d, symbol: { ...d.symbol, graphics: (d.symbol.graphics ?? []).map((x, k) => (k === i ? g : x)) } }));
  const rowSel: RowSel = (k, id) => ({ className: selected?.kind === k && selected.id === id ? "sel" : undefined, onClick: () => setSelected({ kind: k, id }) });

  const title = kind === "symbol" ? `Symbol editor ${def.id}` : `Footprint editor ${def.id}`;
  return (
    <section className="part-editor" aria-label={title} data-dirty={dirty ? "true" : "false"}>
      <header className="pe-head">
        <strong>{kind === "symbol" ? "Symbol" : "Footprint"}</strong>
        <span className="pe-id">{def.id}</span>
        <span className="muted">{def.name}</span>
        <span className="spacer" />
        <span className={dirty ? "pe-state dirty" : "pe-state"} role="status">
          {dirty ? "Unsaved changes (not in the project until Save)" : "Same as the project library"}
        </span>
        <button type="button" disabled={!dirty} title={dirty ? "Discard the draft and reload the part from the project library" : "Nothing to revert"} onClick={revert}>
          Revert
        </button>
        <button type="button" className="primary" disabled={!!saveWhy} title={saveWhy ?? (kind === "symbol" ? "Save with UpsertComponentDefinition" : "Save with AssignComponentFootprint")} onClick={save}>
          Save
        </button>
      </header>
      {stale && (
        <p className="pe-note" role="alert">
          The part changed in the project library while you edited it. Save writes your draft over it; Revert loads the library version.
        </p>
      )}
      <div className="pe-body">
        <div className="pe-canvas" ref={pane} role="img" aria-label={`${kind === "symbol" ? "Symbol" : "Footprint"} canvas of ${def.id}`} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
          {canvasFailed ? (
            <p className="empty-state">The canvas cannot be drawn here (no 2D context); the tables still edit the part.</p>
          ) : (
            <PcbSceneCanvas scene={scene} overlay={overlay} vb={vb} size={size} onUnavailable={() => setCanvasFailed(true)} />
          )}
          <span className="pe-hint">
            Click selects; drag a {kind === "symbol" ? "pin" : "pad"} to move it ({GRID_MM[kind]} mm grid)
          </span>
        </div>
        <div className="pe-side">
          {kind === "symbol" ? <SymbolTables draft={draft} set={set} setPin={setPin} setGraphic={setGraphic} rowSel={rowSel} /> : <FootprintTables draft={draft} set={set} setPad={setPad} rowSel={rowSel} />}
          {problems.length > 0 && (
            <div className="pe-problems" role="group" aria-label="Problems">
              <strong>Not saveable yet</strong>
              <ul>
                {problems.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </div>
          )}
          {footprintRefused && dirty && <p className="pe-note">{footprintRefused}.</p>}
        </div>
      </div>
    </section>
  );
}

type RowSel = (k: PickRef["kind"], id: string) => { className: string | undefined; onClick: () => void };
type SetDraft = (f: (d: ComponentDef) => ComponentDef) => void;

function SymbolTables({ draft, set, setPin, setGraphic, rowSel }: { draft: ComponentDef; set: SetDraft; setPin: (n: string, patch: Partial<SymbolPin>) => void; setGraphic: (i: number, g: Graphic) => void; rowSel: RowSel }) {
  const s = draft.symbol;
  const [newKind, setNewKind] = useState<GraphicKind>("line");
  return (
    <>
      <fieldset className="pe-fields">
        <legend>Part</legend>
        <label>
          Part name <TextInput label="Part name" value={draft.name} onCommit={(v) => set((d) => ({ ...d, name: v }))} />
        </label>
        <label>
          Reference prefix <TextInput label="Part reference prefix" value={draft.refdes_prefix} onCommit={(v) => set((d) => ({ ...d, refdes_prefix: v.trim() }))} />
        </label>
        <label>
          Default value <TextInput label="Part default value" value={draft.default_value} onCommit={(v) => set((d) => ({ ...d, default_value: v }))} />
        </label>
        <label>
          Category <TextInput label="Part category" value={draft.category} onCommit={(v) => set((d) => ({ ...d, category: v }))} />
        </label>
        <label>
          Body half width (mm) <MmInput label="Body half width" nm={s.body_half_w} onCommit={(v) => v !== null && set((d) => ({ ...d, symbol: { ...d.symbol, body_half_w: v } }))} />
        </label>
        <label>
          Body half height (mm) <MmInput label="Body half height" nm={s.body_half_h} onCommit={(v) => v !== null && set((d) => ({ ...d, symbol: { ...d.symbol, body_half_h: v } }))} />
        </label>
      </fieldset>
      <div className="pe-table-head">
        <strong>Pins</strong>
        <button type="button" title="Add a pin (mapped to the pad of the same number when the footprint has one)" onClick={() => set(addPin)}>
          Add pin
        </button>
      </div>
      <div className="pe-scroll">
        <table className="pe-table" aria-label="Pins">
          <thead>
            <tr>
              <th>No.</th>
              <th>Name</th>
              <th>Type</th>
              <th>X mm</th>
              <th>Y mm</th>
              <th>Length</th>
              <th>Towards body</th>
              <th title="Show the pin name">Name shown</th>
              <th title="Show the pin number">No. shown</th>
              <th>Pads</th>
              <th>
                <span className="sr-only">Delete</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {s.pins.map((p, i) => (
              <tr key={i} data-pin={p.number} {...rowSel("pin", p.number)}>
                <td>
                  <TextInput className="pe-short" label={`Pin ${p.number} number`} value={p.number} onCommit={(v) => set((d) => renamePin(d, p.number, v.trim()))} />
                </td>
                <td>
                  <TextInput label={`Pin ${p.number} name`} value={p.name} onCommit={(v) => setPin(p.number, { name: v })} />
                </td>
                <td>
                  <select aria-label={`Pin ${p.number} type`} value={p.electrical_type} onChange={(e) => setPin(p.number, { electrical_type: e.target.value as SymbolPin["electrical_type"] })}>
                    {PIN_TYPES.map((t) => (
                      <option key={t} value={t}>
                        {t.replace("_", " ")}
                      </option>
                    ))}
                  </select>
                </td>
                <td>
                  <MmInput label={`Pin ${p.number} X`} nm={p.position.x} onCommit={(v) => v !== null && setPin(p.number, { position: { ...p.position, x: v } })} />
                </td>
                <td>
                  <MmInput label={`Pin ${p.number} Y`} nm={p.position.y} onCommit={(v) => v !== null && setPin(p.number, { position: { ...p.position, y: v } })} />
                </td>
                <td>
                  <MmInput label={`Pin ${p.number} length`} nm={p.length ?? 0} onCommit={(v) => v !== null && setPin(p.number, { length: v })} />
                </td>
                <td>
                  <select aria-label={`Pin ${p.number} orientation`} value={p.orientation ?? "right"} onChange={(e) => setPin(p.number, { orientation: e.target.value as SymbolPin["orientation"] })}>
                    {ORIENTATIONS.map((o) => (
                      <option key={o} value={o}>
                        {o}
                      </option>
                    ))}
                  </select>
                </td>
                <td>
                  <input type="checkbox" aria-label={`Show pin ${p.number} name`} checked={p.name_visible ?? true} onChange={(e) => setPin(p.number, { name_visible: e.target.checked })} />
                </td>
                <td>
                  <input type="checkbox" aria-label={`Show pin ${p.number} number`} checked={p.number_visible ?? true} onChange={(e) => setPin(p.number, { number_visible: e.target.checked })} />
                </td>
                <td>
                  <TextInput className="pe-short" label={`Pin ${p.number} pads`} value={(draft.pin_map[p.number] ?? []).join(", ")} onCommit={(v) => set((d) => setPinPads(d, p.number, parseList(v)))} />
                </td>
                <td>
                  <button type="button" className="icon" aria-label={`Delete pin ${p.number}`} title={`Delete pin ${p.number}`} onClick={() => set((d) => deletePin(d, p.number))}>
                    ×
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="pe-table-head">
        <strong>Body graphics</strong>
        <select aria-label="New graphic kind" value={newKind} onChange={(e) => setNewKind(e.target.value as GraphicKind)}>
          {GRAPHIC_KINDS.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
        <button type="button" title="Add a graphic of the chosen kind to the symbol body" onClick={() => set((d) => ({ ...d, symbol: { ...d.symbol, graphics: [...(d.symbol.graphics ?? []), newGraphic(newKind, d.symbol)] } }))}>
          Add graphic
        </button>
      </div>
      <ol className="pe-graphics" aria-label="Body graphics">
        {(s.graphics ?? []).map((g, i) => (
          <li key={i} data-graphic={i} {...rowSel("graphic", String(i))}>
            <span className="pe-kind">{g.kind}</span>
            <GraphicFields g={g} i={i} onChange={(x) => setGraphic(i, x)} />
            <button
              type="button"
              className="icon"
              aria-label={`Delete graphic ${i + 1}`}
              title={`Delete graphic ${i + 1} (${g.kind})`}
              onClick={() => set((d) => ({ ...d, symbol: { ...d.symbol, graphics: (d.symbol.graphics ?? []).filter((_, k) => k !== i) } }))}
            >
              ×
            </button>
          </li>
        ))}
      </ol>
    </>
  );
}

function PointFields({ label, p, onChange }: { label: string; p: Point; onChange: (p: Point) => void }) {
  return (
    <span className="pe-pt">
      {label.replace(/^Graphic \d+ /, "")}
      <MmInput label={`${label} X`} nm={p.x} onCommit={(v) => v !== null && onChange({ ...p, x: v })} />
      <MmInput label={`${label} Y`} nm={p.y} onCommit={(v) => v !== null && onChange({ ...p, y: v })} />
    </span>
  );
}

function GraphicFields({ g, i, onChange }: { g: Graphic; i: number; onChange: (g: Graphic) => void }) {
  const n = `Graphic ${i + 1}`;
  const width =
    g.kind === "text" ? null : (
      <span className="pe-pt">
        width
        <MmInput label={`${n} line width (0 = default)`} nm={g.stroke?.width ?? 0} onCommit={(v) => v !== null && onChange({ ...g, stroke: { width: v, style: g.stroke?.style ?? "solid" } })} />
      </span>
    );
  const fill =
    g.kind === "rectangle" || g.kind === "circle" || g.kind === "polygon" ? (
      <label className="pe-pt">
        <input type="checkbox" aria-label={`${n} filled`} checked={g.fill ?? false} onChange={(e) => onChange({ ...g, fill: e.target.checked })} />
        fill
      </label>
    ) : null;
  const deg = (text: string) => (text.trim() !== "" && Number.isFinite(Number(text)) ? Number(text) : null);
  switch (g.kind) {
    case "line":
      return (
        <>
          <PointFields label={`${n} start`} p={g.a} onChange={(a) => onChange({ ...g, a })} />
          <PointFields label={`${n} end`} p={g.b} onChange={(b) => onChange({ ...g, b })} />
          {width}
        </>
      );
    case "rectangle":
      return (
        <>
          <PointFields label={`${n} min`} p={g.min} onChange={(min) => onChange({ ...g, min })} />
          <PointFields label={`${n} max`} p={g.max} onChange={(max) => onChange({ ...g, max })} />
          {width}
          {fill}
        </>
      );
    case "circle":
      return (
        <>
          <PointFields label={`${n} centre`} p={g.center} onChange={(center) => onChange({ ...g, center })} />
          <span className="pe-pt">
            r
            <MmInput label={`${n} radius`} nm={g.radius} onCommit={(v) => v !== null && onChange({ ...g, radius: v })} />
          </span>
          {width}
          {fill}
        </>
      );
    case "arc":
      return (
        <>
          <PointFields label={`${n} centre`} p={g.center} onChange={(center) => onChange({ ...g, center })} />
          <span className="pe-pt">
            r
            <MmInput label={`${n} radius`} nm={g.radius} onCommit={(v) => v !== null && onChange({ ...g, radius: v })} />
          </span>
          <span className="pe-pt">
            from°
            <TextInput
              className="pe-num"
              label={`${n} start angle`}
              value={String(g.start_deg)}
              onCommit={(t) => {
                const a = deg(t);
                if (a !== null) onChange({ ...g, start_deg: a });
              }}
            />
            to°
            <TextInput
              className="pe-num"
              label={`${n} end angle`}
              value={String(g.end_deg)}
              onCommit={(t) => {
                const a = deg(t);
                if (a !== null) onChange({ ...g, end_deg: a });
              }}
            />
          </span>
          {width}
        </>
      );
    case "polyline":
    case "polygon":
      return (
        <>
          <span className="pe-pt">
            points (x y; …)
            <TextInput
              className="pe-points"
              label={`${n} points`}
              value={fmtPoints(g.points)}
              onCommit={(t) => {
                const pts = parsePoints(t);
                if (pts) onChange({ ...g, points: pts });
              }}
            />
          </span>
          {width}
          {fill}
        </>
      );
    case "text":
      return (
        <>
          <PointFields label={`${n} position`} p={g.position} onChange={(position) => onChange({ ...g, position })} />
          <TextInput label={`${n} text`} value={g.text} onCommit={(text) => onChange({ ...g, text })} />
          <span className="pe-pt">
            size
            <MmInput label={`${n} size`} nm={g.size} onCommit={(v) => v !== null && onChange({ ...g, size: v })} />
          </span>
        </>
      );
  }
}

function FootprintTables({ draft, set, setPad, rowSel }: { draft: ComponentDef; set: SetDraft; setPad: (n: string, patch: Partial<PadDef>) => void; rowSel: RowSel }) {
  const f = draft.footprint;
  return (
    <>
      <fieldset className="pe-fields">
        <legend>Footprint</legend>
        <label>
          Footprint name <TextInput label="Footprint name" value={f.name} onCommit={(v) => set((d) => ({ ...d, footprint: { ...d.footprint, name: v.trim() } }))} />
        </label>
        <label>
          Courtyard half width (mm) <MmInput label="Courtyard half width" nm={f.courtyard_half_w} onCommit={(v) => v !== null && set((d) => ({ ...d, footprint: { ...d.footprint, courtyard_half_w: v } }))} />
        </label>
        <label>
          Courtyard half height (mm) <MmInput label="Courtyard half height" nm={f.courtyard_half_h} onCommit={(v) => v !== null && set((d) => ({ ...d, footprint: { ...d.footprint, courtyard_half_h: v } }))} />
        </label>
        <label>
          Body height (mm, empty = unknown) <MmInput label="Body height" nm={f.body_height} allowEmpty onCommit={(v) => set((d) => ({ ...d, footprint: { ...d.footprint, body_height: v } }))} />
        </label>
      </fieldset>
      <div className="pe-table-head">
        <strong>Pads</strong>
        <button type="button" title="Add a pad one pitch right of the right-most pad, like the last pad" onClick={() => set(addPad)}>
          Add pad
        </button>
      </div>
      <div className="pe-scroll">
        <table className="pe-table" aria-label="Pads">
          <thead>
            <tr>
              <th>No.</th>
              <th>Kind</th>
              <th>Shape</th>
              <th>X mm</th>
              <th>Y mm</th>
              <th>Size X</th>
              <th>Size Y</th>
              <th>Drill</th>
              <th>Pins</th>
              <th>
                <span className="sr-only">Delete</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {f.pads.map((p, i) => (
              <tr key={i} data-pad={p.number} {...rowSel("pad", p.number)}>
                <td>
                  <TextInput className="pe-short" label={`Pad ${p.number} number`} value={p.number} onCommit={(v) => set((d) => renamePad(d, p.number, v.trim()))} />
                </td>
                <td>
                  <select
                    aria-label={`Pad ${p.number} kind`}
                    value={p.kind}
                    onChange={(e) => {
                      const k = e.target.value as PadDef["kind"];
                      setPad(p.number, { kind: k, drill: k === "tht" ? (p.drill ?? Math.round(Math.min(p.size_x, p.size_y) / 2)) : null });
                    }}
                  >
                    <option value="smd">SMD</option>
                    <option value="tht">Through-hole</option>
                  </select>
                </td>
                <td>
                  <select aria-label={`Pad ${p.number} shape`} value={p.shape} onChange={(e) => setPad(p.number, { shape: e.target.value as PadDef["shape"] })}>
                    <option value="rect">rect</option>
                    <option value="circle">circle</option>
                    <option value="oval">oval</option>
                  </select>
                </td>
                <td>
                  <MmInput label={`Pad ${p.number} X`} nm={p.position.x} onCommit={(v) => v !== null && setPad(p.number, { position: { ...p.position, x: v } })} />
                </td>
                <td>
                  <MmInput label={`Pad ${p.number} Y`} nm={p.position.y} onCommit={(v) => v !== null && setPad(p.number, { position: { ...p.position, y: v } })} />
                </td>
                <td>
                  <MmInput label={`Pad ${p.number} size X`} nm={p.size_x} onCommit={(v) => v !== null && setPad(p.number, { size_x: v })} />
                </td>
                <td>
                  <MmInput label={`Pad ${p.number} size Y`} nm={p.size_y} onCommit={(v) => v !== null && setPad(p.number, { size_y: v })} />
                </td>
                <td>{p.kind === "tht" ? <MmInput label={`Pad ${p.number} drill`} nm={p.drill} onCommit={(v) => v !== null && setPad(p.number, { drill: v })} /> : <span className="muted">none (SMD)</span>}</td>
                <td>
                  <TextInput className="pe-short" label={`Pad ${p.number} pins`} value={pinsOfPad(draft, p.number).join(", ")} onCommit={(v) => set((d) => setPadPins(d, p.number, parseList(v)))} />
                </td>
                <td>
                  <button type="button" className="icon" aria-label={`Delete pad ${p.number}`} title={`Delete pad ${p.number}`} onClick={() => set((d) => deletePad(d, p.number))}>
                    ×
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="pe-note" data-unavailable="mask-paste">
        Solder-mask and paste expansion per pad: not available yet. The project format (EAK-IR) has no per-pad mask or paste field, so the board-wide manufacturing defaults apply; adding one is a schema change (ADR and
        migration) owned by the IR lane.
      </p>
    </>
  );
}
