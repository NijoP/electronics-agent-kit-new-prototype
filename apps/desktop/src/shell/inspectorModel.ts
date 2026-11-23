// Context properties inspector (Phase 1 A5). Pure parts and a small store:
//  * each editor publishes, per document, its active tool, that tool's option schema and its
//    selection (`publishEditorContext`); the right dock's Inspector renders whichever applies;
//  * `selectionModel` / `documentModel` turn the snapshot into field rows whose edits are typed
//    engine commands (one command per edit, a `Batch` for several objects, so one revision and
//    one undo step);
//  * fields the IR cannot hold yet are rows with an "Unavailable: …" reason, never controls.
// Field sets follow the EasyEDA property-panel study (eak-p1-rea-easyeda §4), in EAK terms.
import { useEffect, useSyncExternalStore } from "react";
import { mmToNm } from "../editing";
import { LAYER_COUNTS, layerKey, layerName, stackLayers } from "../layers";
import { trackEdit, widthFromMm } from "../routeTool";
import type { Command, CopperLayer, Id, Point, Rotation, Side, Snapshot } from "../types";
import { mm } from "../viewport";
import { moveSymbolsTo } from "../tools/schematic/selection";
import { sheetsInOrder } from "./sheets";

// ---- Tool option schema --------------------------------------------------------------------

export interface Choice {
  value: string;
  label: string;
}
/** One option of the active tool. `name` is the control's accessible name. */
export type OptionField =
  | { kind: "select"; name: string; label: string; value: string; options: Choice[]; onChange: (v: string) => void; title?: string }
  | { kind: "text"; name: string; label: string; value: string; onChange: (v: string) => void; placeholder?: string; title?: string }
  | { kind: "info"; name: string; label: string; value: string }
  | { kind: "unavailable"; name: string; label: string; reason: string };

/** What a selection holds, by object kind (ids). */
export interface SelectionRef {
  symbols: Id[];
  wires: Id[];
  labels: Id[];
  footprints: Id[];
  tracks: Id[];
  vias: Id[];
  zones: Id[];
}
export const NO_SELECTION: SelectionRef = { symbols: [], wires: [], labels: [], footprints: [], tracks: [], vias: [], zones: [] };
export const selectionSize = (s: SelectionRef) => (Object.keys(NO_SELECTION) as (keyof SelectionRef)[]).reduce((n, k) => n + s[k].length, 0);

export interface EditorContext {
  editor: "schematic" | "pcb";
  /** Active tool id; "select" means no drawing tool is active. */
  tool: string;
  toolLabel: string;
  options: OptionField[];
  selection: SelectionRef;
  /** The sheet the schematic editor shows (document mode). */
  sheetId?: Id | null;
  /**
   * Editing operations the editor offers the action registry (slice B3), by registry action id:
   * whether each can run now (or why not) and how to run it. The state is part of what is
   * published, so menus re-render when it changes; `run` is always the latest callback.
   */
  ops?: Record<string, EditorOp>;
}
export interface EditorOp {
  state: true | string;
  run: () => void;
}

// ---- Store ---------------------------------------------------------------------------------

const contexts = new Map<string, EditorContext>();
const signatures = new Map<string, string>();
const listeners = new Set<() => void>();
let version = 0;
let hosts = 0;
const emit = () => {
  version += 1;
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

/**
 * Publish (or with null withdraw) an editor's context. The latest callbacks are always kept;
 * subscribers re-render only when something they can see (tool, values, selection) changed.
 */
export function publishEditorContext(docId: string, ctx: EditorContext | null): void {
  if (!ctx) {
    if (contexts.delete(docId)) {
      signatures.delete(docId);
      emit();
    }
    return;
  }
  contexts.set(docId, ctx);
  const sig = JSON.stringify(ctx); // functions are left out, values and selection are not
  if (signatures.get(docId) !== sig) {
    signatures.set(docId, sig);
    emit();
  }
}
/** The context an editor published last (read at event time, so callbacks are never stale). */
export const editorContext = (docId: string | null): EditorContext | null => (docId ? (contexts.get(docId) ?? null) : null);

/** An editor publishes its context after every render and withdraws it when it unmounts. */
export function usePublishEditorContext(docId: string | null, ctx: EditorContext): void {
  useEffect(() => {
    if (docId) publishEditorContext(docId, ctx);
  });
  useEffect(
    () => () => {
      if (docId) publishEditorContext(docId, null);
    },
    [docId],
  );
}
/** Re-renders when a published context changes; returns the document's context. */
export function useEditorContext(docId: string | null): EditorContext | null {
  useSyncExternalStore(subscribe, () => version);
  return editorContext(docId);
}
/** The Inspector registers itself while mounted; editors then leave their option strip to it. */
export function useInspectorHost(): void {
  useEffect(() => {
    hosts += 1;
    emit();
    return () => {
      hosts -= 1;
      emit();
    };
  }, []);
}
export const useHasInspector = (): boolean => useSyncExternalStore(subscribe, () => hosts > 0);
/** Tests: forget every published context and host. */
export function resetInspectorStore(): void {
  contexts.clear();
  signatures.clear();
  hosts = 0;
  emit();
}

// ---- Field rows ----------------------------------------------------------------------------

/** An edit's result: a command to execute, a message why it cannot be applied, or no change. */
export type Commit = Command | string | null;
export interface PropField {
  id: string;
  label: string;
  /** Shown value; empty with `mixed` when the selected objects differ. */
  value: string;
  mixed?: boolean;
  edit?: { kind: "text"; commit: (v: string) => Commit } | { kind: "select"; options: Choice[]; commit: (v: string) => Commit };
  /** Set: the field exists in the reference tool but EAK cannot hold or edit it yet. */
  unavailable?: string;
}
export interface PropModel {
  title: string;
  fields: PropField[];
}

const B1 = "Unavailable: the EAK-IR has no field for this yet (IR v3, slice B1)";
const ROTATIONS: Rotation[] = ["R0", "R90", "R180", "R270"];
export const ROTATION_CHOICES: Choice[] = ROTATIONS.map((r) => ({ value: r, label: `${r.slice(1)}°` }));
const fmt = (nm: number) => mm(nm).toFixed(3);
const pt = (p: Point) => `${fmt(p.x)}, ${fmt(p.y)} mm`;

/** The common value of `values`, or mixed when they differ. */
function common(values: string[]): { value: string; mixed?: boolean } {
  const set = new Set(values);
  return set.size === 1 ? { value: values[0] } : { value: "", mixed: true };
}
/** One command, a `Batch` of several (one revision, one undo step), or null for none. */
export function batch(commands: Command[], label: string): Command | null {
  if (commands.length === 0) return null;
  return commands.length === 1 ? commands[0] : { type: "Batch", commands, label };
}
/** Millimetres typed by the user, in nm; a message when it is not a number. */
function parseMm(v: string): number | string {
  const t = v.trim();
  const n = Number(t);
  return t !== "" && Number.isFinite(n) ? mmToNm(n) : `"${v}" is not a number of millimetres.`;
}
const unavailable = (id: string, label: string, reason = B1): PropField => ({ id, label, value: "", unavailable: reason });
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
const withAxis = (p: Point, k: "x" | "y", n: number): Point => (k === "x" ? { x: n, y: p.y } : { x: p.x, y: n });
const present = <T>(x: T | undefined): x is T => x !== undefined;

function symbolFields(s: Snapshot, ids: Id[]): PropModel {
  const syms = ids.map((id) => s.project.schematic.symbols[id]).filter(present);
  const one = syms.length === 1 ? syms[0] : null;
  const axis = (k: "x" | "y"): PropField => ({
    id: k,
    label: `${k.toUpperCase()} (mm)`,
    ...common(syms.map((y) => fmt(y.position[k]))),
    edit: {
      kind: "text",
      commit: (v) => {
        const n = parseMm(v);
        if (typeof n === "string") return n;
        // Attached wire ends and pin labels follow (rubber-band, as a drag does): one Batch.
        const targets = syms.filter((y) => y.position[k] !== n).map((y) => ({ id: y.id, position: withAxis(y.position, k, n) }));
        return batch(moveSymbolsTo(s, targets), `Set ${k.toUpperCase()} of ${plural(targets.length, "symbol")}`);
      },
    },
  });
  return {
    title: one ? `Component ${one.refdes}` : plural(syms.length, "component"),
    fields: [
      one
        ? { id: "refdes", label: "Designator", value: one.refdes, edit: { kind: "text", commit: (v) => (v.trim() && v.trim() !== one.refdes ? { type: "SetRefdes", symbol_id: one.id, refdes: v.trim() } : null) } }
        : unavailable("refdes", "Designator", "Unavailable for several parts: designators are unique, edit them one at a time"),
      {
        id: "value",
        label: "Value",
        ...common(syms.map((y) => y.value)),
        edit: {
          kind: "text",
          commit: (v) => {
            const cmds: Command[] = syms.filter((y) => y.value !== v).map((y): Command => ({ type: "SetSymbolValue", symbol_id: y.id, value: v }));
            return batch(cmds, `Set value of ${plural(cmds.length, "symbol")}`);
          },
        },
      },
      { id: "component", label: "Component", ...common(syms.map((y) => y.component_id)) },
      { id: "sheet", label: "Sheet", ...common(syms.map((y) => s.project.schematic.sheets[y.sheet_id]?.name ?? y.sheet_id)) },
      axis("x"),
      axis("y"),
      {
        id: "rotation",
        label: "Rotation",
        ...common(syms.map((y) => y.rotation)),
        edit: {
          kind: "select",
          options: ROTATION_CHOICES,
          commit: (v) => {
            const cmds: Command[] = syms.filter((y) => y.rotation !== v).map((y): Command => ({ type: "RotateSymbol", symbol_id: y.id, rotation: v as Rotation }));
            return batch(cmds, `Rotate ${plural(cmds.length, "symbol")}`);
          },
        },
      },
      { id: "mirror", label: "Mirrored", ...common(syms.map((y) => (y.mirror ? "yes" : "no"))) },
      { id: "footprint", label: "Footprint", ...common(syms.map((y) => (Object.values(s.project.pcb.footprints).some((f) => f.symbol_id === y.id) ? "on the PCB" : "not on the PCB yet"))) },
      unavailable("mpn", "Manufacturer / MPN"),
      unavailable("supplier", "Supplier part"),
      unavailable("bom", "In BOM / in PCB"),
      unavailable("unit", "Sub-part (unit)"),
      unavailable("extra", "Extra properties"),
    ],
  };
}

function footprintFields(s: Snapshot, ids: Id[]): PropModel {
  const fps = ids.map((id) => s.project.pcb.footprints[id]).filter(present);
  const free = fps.filter((f) => !f.locked);
  const lockedMsg = `${plural(fps.length - free.length, "footprint")} locked; unlock to move.`;
  const move = (change: (f: (typeof fps)[number]) => Command | null, label: string): Commit => {
    if (!free.length) return lockedMsg;
    return batch(free.map(change).filter((c): c is Command => c !== null), label);
  };
  const axis = (k: "x" | "y"): PropField => ({
    id: k,
    label: `${k.toUpperCase()} (mm)`,
    ...common(fps.map((f) => fmt(f.position[k]))),
    edit: {
      kind: "text",
      commit: (v) => {
        const n = parseMm(v);
        if (typeof n === "string") return n;
        return move((f) => (f.position[k] === n ? null : { type: "MoveFootprint", footprint_id: f.id, position: withAxis(f.position, k, n), rotation: null, side: null }), `Set ${k.toUpperCase()} of ${plural(free.length, "footprint")}`);
      },
    },
  });
  return {
    title: fps.length === 1 ? `Footprint ${fps[0].refdes}` : plural(fps.length, "footprint"),
    fields: [
      { id: "refdes", label: "Designator", ...common(fps.map((f) => f.refdes)) },
      { id: "component", label: "Component", ...common(fps.map((f) => f.component_id)) },
      {
        id: "side",
        label: "Layer (side)",
        ...common(fps.map((f) => f.side)),
        edit: {
          kind: "select",
          options: [
            { value: "top", label: "Top" },
            { value: "bottom", label: "Bottom" },
          ],
          commit: (v) => move((f) => (f.side === v ? null : { type: "MoveFootprint", footprint_id: f.id, position: f.position, rotation: null, side: v as Side }), `Move ${plural(free.length, "footprint")} to the ${v}`),
        },
      },
      axis("x"),
      axis("y"),
      {
        id: "rotation",
        label: "Rotation",
        ...common(fps.map((f) => f.rotation)),
        edit: {
          kind: "select",
          options: ROTATION_CHOICES,
          commit: (v) => move((f) => (f.rotation === v ? null : { type: "MoveFootprint", footprint_id: f.id, position: f.position, rotation: v as Rotation, side: null }), `Rotate ${plural(free.length, "footprint")}`),
        },
      },
      {
        id: "locked",
        label: "Locked",
        ...common(fps.map((f) => (f.locked ? "yes" : "no"))),
        edit: {
          kind: "select",
          options: [
            { value: "no", label: "no" },
            { value: "yes", label: "yes" },
          ],
          commit: (v) => {
            const locked = v === "yes";
            const cmds: Command[] = fps.filter((f) => f.locked !== locked).map((f): Command => ({ type: "SetFootprintLocked", footprint_id: f.id, locked }));
            return batch(cmds, `${locked ? "Lock" : "Unlock"} ${plural(cmds.length, "footprint")}`);
          },
        },
      },
      unavailable("model3d", "3D model"),
      unavailable("bom", "In BOM"),
      unavailable("mpn", "Manufacturer / MPN"),
    ],
  };
}

function wireFields(s: Snapshot, ids: Id[]): PropModel {
  const ws = ids.map((id) => s.project.schematic.wires[id]).filter(present);
  return {
    title: ws.length === 1 ? "Wire" : plural(ws.length, "wire"),
    fields: [
      { id: "sheet", label: "Sheet", ...common(ws.map((w) => s.project.schematic.sheets[w.sheet_id]?.name ?? w.sheet_id)) },
      { id: "a", label: "Start", ...common(ws.map((w) => pt(w.a))) },
      { id: "b", label: "End", ...common(ws.map((w) => pt(w.b))) },
      unavailable("width", "Line width"),
      unavailable("colour", "Colour"),
      unavailable("style", "Line style"),
    ],
  };
}

function labelFields(s: Snapshot, ids: Id[]): PropModel {
  const ls = ids.map((id) => s.project.schematic.labels[id]).filter(present);
  // A rename keeps the label (and its id): `RenameNetLabel`, which also carries the net name onto
  // the PCB and the constraints (ADR-0045); a Batch for several labels, so one undo step.
  const rename = (name: string): Commit =>
    batch(
      ls.filter((l) => l.name !== name).map((l): Command => ({ type: "RenameNetLabel", label_id: l.id, name })),
      `Rename ${plural(ls.length, "label")}`,
    );
  // A kind change keeps the label in place: `SetNetLabelKind` (ADR-0049) keeps its id, group,
  // scope and power style; a Batch for several labels, so one undo step.
  const rekind = (kind: "net" | "power"): Commit =>
    batch(
      ls.filter((l) => l.kind !== kind).map((l): Command => ({ type: "SetNetLabelKind", label_id: l.id, kind })),
      `Change ${plural(ls.length, "label")} kind`,
    );
  const axis = (k: "x" | "y"): PropField => ({
    id: k,
    label: `${k.toUpperCase()} (mm)`,
    ...common(ls.map((l) => fmt(l.position[k]))),
    edit: {
      kind: "text",
      commit: (v) => {
        const n = parseMm(v);
        if (typeof n === "string") return n;
        const cmds: Command[] = ls.filter((l) => l.position[k] !== n).map((l): Command => ({ type: "MoveNetLabel", label_id: l.id, position: withAxis(l.position, k, n) }));
        return batch(cmds, `Set ${k.toUpperCase()} of ${plural(cmds.length, "label")}`);
      },
    },
  });
  return {
    title: ls.length === 1 ? `Label ${ls[0].name}` : plural(ls.length, "label"),
    fields: [
      { id: "name", label: "Net name", ...common(ls.map((l) => l.name)), edit: { kind: "text", commit: (v) => (v.trim() ? rename(v.trim()) : "A net label needs a name.") } },
      {
        id: "kind",
        label: "Kind",
        ...common(ls.map((l) => l.kind)),
        edit: {
          kind: "select",
          options: [
            { value: "net", label: "net label" },
            { value: "power", label: "power port" },
          ],
          commit: (v) => rekind(v as "net" | "power"),
        },
      },
      axis("x"),
      axis("y"),
      unavailable("font", "Font, size and colour"),
    ],
  };
}

function trackFields(s: Snapshot, ids: Id[]): PropModel {
  const ts = ids.map((id) => s.project.pcb.tracks[id]).filter(present);
  const order = stackLayers(s.project.pcb.board.stackup).map((l) => l.layer);
  const board = { order, tracks: Object.values(s.project.pcb.tracks), vias: Object.values(s.project.pcb.vias), footprints: s.geometry.footprints };
  const edit = (change: { width?: number; layer?: CopperLayer }): Commit => {
    const out: Command[] = [];
    for (const t of ts) {
      const c = trackEdit(t, change, board);
      if (c && "refuse" in c) return c.refuse;
      if (c) out.push(c);
    }
    return batch(out, `Edit ${plural(out.length, "track")}`);
  };
  return {
    title: ts.length === 1 ? `Track ${ts[0].net}` : plural(ts.length, "track"),
    fields: [
      { id: "net", label: "Net", ...common(ts.map((t) => t.net)) },
      {
        id: "layer",
        label: "Layer",
        ...common(ts.map((t) => layerKey(t.layer))),
        edit: {
          kind: "select",
          options: order.map((l) => ({ value: layerKey(l), label: layerName(l) })),
          commit: (v) => {
            const l = order.find((o) => layerKey(o) === v);
            return l ? edit({ layer: l }) : null;
          },
        },
      },
      {
        id: "width",
        label: "Width (mm)",
        ...common(ts.map((t) => fmt(t.width))),
        edit: {
          kind: "text",
          commit: (v) => {
            const w = widthFromMm(v);
            return w === null ? `"${v}" is not a positive width in millimetres.` : edit({ width: w });
          },
        },
      },
      { id: "a", label: "Start", ...common(ts.map((t) => pt(t.a))) },
      { id: "b", label: "End", ...common(ts.map((t) => pt(t.b))) },
      unavailable("lock", "Lock"),
    ],
  };
}

/** F2 (M2): a via's position is edited with `MoveVia` (one command per via, a Batch for several). */
function viaFields(s: Snapshot, ids: Id[]): PropModel {
  const vs = ids.map((id) => s.project.pcb.vias[id]).filter(present);
  const order = stackLayers(s.project.pcb.board.stackup).map((l) => l.layer);
  const span = (v: (typeof vs)[number]) => {
    const from = v.from ?? order[0] ?? "top";
    const to = v.to ?? order[order.length - 1] ?? "bottom";
    return `${layerName(from)} – ${layerName(to)}`;
  };
  const axis = (k: "x" | "y"): PropField => ({
    id: k,
    label: `${k.toUpperCase()} (mm)`,
    ...common(vs.map((v) => fmt(v.position[k]))),
    edit: {
      kind: "text",
      commit: (v) => {
        const n = parseMm(v);
        if (typeof n === "string") return n;
        const cmds: Command[] = vs.filter((x) => x.position[k] !== n).map((x): Command => ({ type: "MoveVia", via_id: x.id, position: withAxis(x.position, k, n) }));
        return batch(cmds, `Set ${k.toUpperCase()} of ${plural(cmds.length, "via")}`);
      },
    },
  });
  const noEdit = "Unavailable: the engine has no command to change a placed via's net, drill, diameter or span; delete it and place a new one";
  return {
    title: vs.length === 1 ? `Via ${vs[0].net}` : plural(vs.length, "via"),
    fields: [
      { id: "net", label: "Net", ...common(vs.map((v) => v.net)) },
      axis("x"),
      axis("y"),
      { id: "drill", label: "Drill (mm)", ...common(vs.map((v) => fmt(v.drill))) },
      { id: "diameter", label: "Diameter (mm)", ...common(vs.map((v) => fmt(v.diameter))) },
      { id: "span", label: "Span", ...common(vs.map(span)) },
      unavailable("via-edit", "Change net, size or span", noEdit),
    ],
  };
}

/** F2 (M2): a zone's net, layer and clearance are edited with `EditZone` (a Batch for several). */
function zoneFields(s: Snapshot, ids: Id[]): PropModel {
  const zs = ids.map((id) => s.project.pcb.zones[id]).filter(present);
  const order = stackLayers(s.project.pcb.board.stackup).map((l) => l.layer);
  const nets = [...new Set([...s.geometry.footprints.flatMap((f) => f.pads.map((p) => p.net)), ...Object.values(s.project.pcb.tracks).map((t) => t.net), ...zs.map((z) => z.net)])]
    .filter((n): n is string => !!n)
    .sort();
  const edit = (change: (z: (typeof zs)[number]) => Command | null, label: string): Commit => batch(zs.map(change).filter((c): c is Command => c !== null), label);
  return {
    title: zs.length === 1 ? `Zone ${zs[0].net}` : plural(zs.length, "zone"),
    fields: [
      {
        id: "net",
        label: "Net",
        ...common(zs.map((z) => z.net)),
        edit: { kind: "select", options: nets.map((n) => ({ value: n, label: n })), commit: (v) => edit((z) => (z.net === v ? null : { type: "EditZone", zone_id: z.id, net: v }), `Set net of ${plural(zs.length, "zone")}`) },
      },
      {
        id: "layer",
        label: "Layer",
        ...common(zs.map((z) => layerKey(z.layer))),
        edit: {
          kind: "select",
          options: order.map((l) => ({ value: layerKey(l), label: layerName(l) })),
          commit: (v) => {
            const l = order.find((o) => layerKey(o) === v);
            return l ? edit((z) => (layerKey(z.layer) === v ? null : { type: "EditZone", zone_id: z.id, layer: l }), `Move ${plural(zs.length, "zone")} to ${layerName(l)}`) : null;
          },
        },
      },
      {
        id: "clearance",
        label: "Clearance (mm)",
        ...common(zs.map((z) => fmt(z.clearance))),
        edit: {
          kind: "text",
          commit: (v) => {
            const n = parseMm(v);
            if (typeof n === "string") return n;
            if (n < 0) return "Zone clearance cannot be negative.";
            return edit((z) => (z.clearance === n ? null : { type: "EditZone", zone_id: z.id, clearance: n }), `Set clearance of ${plural(zs.length, "zone")}`);
          },
        },
      },
      { id: "outline", label: "Outline", ...common(zs.map((z) => `${z.outline.length} points`)) },
      unavailable("fill", "Fill method, islands, priority", "Unavailable: the engine fills every zone solid and the EAK-IR has no fill-method, island or priority fields"),
    ],
  };
}

/**
 * Field rows for a selection. Objects of one kind show their common fields (a value that
 * differs is "mixed"); a selection of several kinds lists what it holds.
 */
export function selectionModel(s: Snapshot, sel: SelectionRef): PropModel | null {
  const kinds = (Object.keys(NO_SELECTION) as (keyof SelectionRef)[]).filter((k) => sel[k].length > 0);
  if (kinds.length === 0) return null;
  const grouping = groupField(s, sel);
  if (kinds.length > 1)
    return {
      title: `${selectionSize(sel)} objects`,
      fields: [
        ...kinds.map((k): PropField => ({ id: k, label: k[0].toUpperCase() + k.slice(1), value: String(sel[k].length) })),
        ...(grouping ? [grouping] : []),
        unavailable("common", "Common fields", "Unavailable: objects of different kinds share no editable field; select one kind to edit it"),
      ],
    };
  const k = kinds[0];
  const build = { symbols: symbolFields, footprints: footprintFields, wires: wireFields, labels: labelFields, tracks: trackFields, vias: viaFields, zones: zoneFields }[k];
  const m = build(s, sel[k]);
  return grouping ? { ...m, fields: [...m.fields, grouping] } : m;
}

const UNGROUPED = "none";
const NEW_GROUP = "new";
/**
 * The Group row of a schematic selection (ADR-0045): the group(s) it touches, with "none" to
 * ungroup them, or "group these objects" for two or more ungrouped objects. One command (a Batch
 * for several groups), so one undo step. Null for PCB selections, which cannot be grouped.
 */
export function groupField(s: Snapshot, sel: SelectionRef): PropField | null {
  const members = [...sel.symbols, ...sel.wires, ...sel.labels];
  if (members.length === 0 || sel.footprints.length > 0 || sel.tracks.length > 0 || sel.vias.length > 0 || sel.zones.length > 0) return null;
  const touched = Object.values(s.project.schematic.groups ?? {}).filter((g) => g.members.some((m) => members.includes(m)));
  if (touched.length > 0) {
    const ungroup = touched.map((g): Command => ({ type: "Ungroup", group_id: g.id }));
    return {
      id: "group",
      label: "Group",
      ...(touched.length === 1 ? { value: touched[0].id } : { value: "", mixed: true }),
      edit: {
        kind: "select",
        options: [...touched.map((g) => ({ value: g.id, label: `${g.name} (${plural(g.members.length, "object")})` })), { value: UNGROUPED, label: "none (ungroup)" }],
        commit: (v) => (v === UNGROUPED ? batch(ungroup, `Ungroup ${plural(touched.length, "group")}`) : null),
      },
    };
  }
  if (members.length < 2) return { id: "group", label: "Group", value: "not grouped (select two or more objects to group them)" };
  return {
    id: "group",
    label: "Group",
    value: UNGROUPED,
    edit: {
      kind: "select",
      options: [
        { value: UNGROUPED, label: "not grouped" },
        { value: NEW_GROUP, label: `group these ${plural(members.length, "object")}` },
      ],
      commit: (v) => (v === NEW_GROUP ? { type: "Group", members, name: null } : null),
    },
  };
}

/** Document fields: sheet size and title block in the schematic, board and stack-up in the PCB. */
export function documentModel(s: Snapshot, pcb: boolean, sheetId?: Id | null): PropModel {
  const { schematic, pcb: board } = s.project;
  const count = (o: object) => String(Object.keys(o).length);
  if (pcb) {
    const o = board.board.outline;
    const xs = o.map((p) => mm(p.x));
    const ys = o.map((p) => mm(p.y));
    const stack = stackLayers(board.board.stackup);
    const st = board.board.stackup;
    const counts = LAYER_COUNTS.includes(stack.length) ? LAYER_COUNTS : [...LAYER_COUNTS, stack.length].sort((a, b) => a - b);
    return {
      title: "PCB document",
      fields: [
        { id: "outline", label: "Board outline", value: o.length ? `${(Math.max(...xs) - Math.min(...xs)).toFixed(2)} × ${(Math.max(...ys) - Math.min(...ys)).toFixed(2)} mm (${o.length} points)` : "none yet" },
        {
          id: "layers",
          label: "Copper layer count",
          value: String(stack.length),
          edit: {
            kind: "select",
            options: counts.map((n) => ({ value: String(n), label: String(n) })),
            commit: (v) => (Number(v) === stack.length ? null : { type: "SetLayerCount", count: Number(v), board_thickness: null }),
          },
        },
        { id: "stackup", label: "Stack-up", value: stack.map((l) => `${layerName(l.layer)}${l.kind === "plane" ? " (plane)" : ""}`).join(" · ") },
        { id: "thickness", label: "Board thickness", value: st ? `${mm(st.board_thickness).toFixed(2)} mm` : "not set" },
        { id: "dielectric", label: "Dielectric", value: st?.dielectric || "not set" },
        { id: "footprints", label: "Footprints", value: count(board.footprints) },
        { id: "tracks", label: "Tracks", value: count(board.tracks) },
        { id: "vias", label: "Vias", value: count(board.vias) },
        { id: "zones", label: "Zones", value: count(board.zones) },
        unavailable("layer-materials", "Loss tangent", "Unavailable: the EAK-IR stack-up has no loss tangent; copper weight, gaps, εr and thickness are edited in the Layers panel"),
      ],
    };
  }
  const sheet = (sheetId ? schematic.sheets[sheetId] : undefined) ?? sheetsInOrder(schematic)[0];
  return {
    title: sheet ? `Sheet ${sheet.name}` : "Schematic document",
    fields: [
      { id: "project", label: "Project", value: s.project.name },
      { id: "revision", label: "Revision", value: String(s.project.revision) },
      { id: "size", label: "Sheet size", value: sheet ? `${mm(sheet.width).toFixed(0)} × ${mm(sheet.height).toFixed(0)} mm` : "no sheet" },
      unavailable("title-block", "Title block"),
      unavailable("sheet-size-edit", "Change sheet size"),
      { id: "sheets", label: "Sheets", value: count(schematic.sheets) },
      { id: "symbols", label: "Symbols", value: count(schematic.symbols) },
      { id: "wires", label: "Wires", value: count(schematic.wires) },
      { id: "labels", label: "Net labels", value: count(schematic.labels) },
    ],
  };
}
