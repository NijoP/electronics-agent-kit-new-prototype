// Design manager data (Phase 1 A3): the Pages / Nets / Components / Objects lists of the left dock
// and where each entry navigates to. Everything is derived from the engine snapshot, the engine's
// netlist (`netlist` IPC, eak-engineering) and its connectivity report (`connectivity` IPC); the
// manager keeps no design model of its own.
import type { ConnectivityReport, Id, Snapshot } from "../types";
import { boundsOf, mm, toView, type Bounds } from "../viewport";
import { availableDocs } from "./documents";
import { netNames } from "./panels";

export type ManagerTab = "pages" | "nets" | "components" | "objects";
export const MANAGER_TABS: { id: ManagerTab; label: string }[] = [
  { id: "pages", label: "Pages" },
  { id: "nets", label: "Nets" },
  { id: "components", label: "Components" },
  { id: "objects", label: "Objects" },
];

export type ObjectKind = "wire" | "label" | "track" | "via" | "zone";
export type Target =
  | { kind: "page"; docId: string }
  | { kind: "component"; symbolId: Id }
  | { kind: "net"; name: string }
  | { kind: "object"; obj: ObjectKind; id: Id };

export interface ManagerItem {
  key: string;
  /** Group heading inside the tab (Objects: "Wires", "Tracks", ...). */
  group?: string;
  label: string;
  detail: string;
  target: Target;
  /** Schematic pages: the sheet this entry is (the Pages tab's sheet actions act on it). */
  sheetId?: Id;
}

export interface NetRow {
  name: string;
  pins: string[];
}

const byText = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });
const at = (p: { x: number; y: number }) => `(${mm(p.x).toFixed(2)}, ${mm(p.y).toFixed(2)}) mm`;

export function pagesOf(s: Snapshot): ManagerItem[] {
  return availableDocs(s).map((d) => ({
    key: d.id,
    label: d.title,
    detail:
      d.kind === "pcb"
        ? `board · ${Object.keys(s.project.pcb.footprints).length} footprint(s)`
        : `sheet · ${Object.values(s.project.schematic.symbols).filter((y) => y.sheet_id === d.ref).length} symbol(s)`,
    target: { kind: "page", docId: d.id },
    sheetId: d.kind === "schematic" ? d.ref : undefined,
  }));
}

/** Every net the engine's netlist or the PCB knows, with its pin count and unrouted connections. */
export function netsOf(s: Snapshot, netlist: NetRow[] | null, conn: ConnectivityReport | null): ManagerItem[] {
  const pins = new Map((netlist ?? []).map((n) => [n.name, n.pins.length]));
  const unrouted = new Map<string, number>();
  for (const [net] of conn && conn.revision === s.project.revision ? conn.ratsnest : []) unrouted.set(net, (unrouted.get(net) ?? 0) + 1);
  const names = new Set([...pins.keys(), ...netNames(s)]);
  return [...names].sort(byText).map((name) => ({
    key: `net:${name}`,
    label: name,
    detail: [netlist === null ? "" : pins.has(name) ? `${pins.get(name)} pin(s)` : "not in the netlist", unrouted.get(name) ? `${unrouted.get(name)} unrouted` : ""].filter(Boolean).join(" · "),
    target: { kind: "net", name },
  }));
}

export function componentsOf(s: Snapshot): ManagerItem[] {
  const fp = new Set(Object.values(s.project.pcb.footprints).map((f) => f.symbol_id));
  return Object.values(s.project.schematic.symbols)
    .sort((a, b) => byText(a.refdes, b.refdes))
    .map((y) => ({ key: y.id, label: y.refdes, detail: `${y.value}${fp.has(y.id) ? "" : " · no footprint"}`, target: { kind: "component", symbolId: y.id } }));
}

export function objectsOf(s: Snapshot): ManagerItem[] {
  const { schematic: sch, pcb } = s.project;
  const out: ManagerItem[] = [];
  const add = (group: string, obj: ObjectKind, id: Id, label: string, detail: string) => out.push({ key: `${obj}:${id}`, group, label, detail, target: { kind: "object", obj, id } });
  Object.values(sch.wires).forEach((w, i) => add("Wires", "wire", w.id, `Wire ${i + 1}`, `${at(w.a)} → ${at(w.b)}`));
  Object.values(sch.labels).forEach((l) => add("Net labels", "label", l.id, l.name, `${l.kind === "power" ? "power port" : "net label"} at ${at(l.position)}`));
  Object.values(pcb.tracks).forEach((t, i) => add("Tracks", "track", t.id, `Track ${i + 1}`, `${t.net || "no net"} · ${t.layer} · ${mm(t.width)} mm`));
  Object.values(pcb.vias).forEach((v, i) => add("Vias", "via", v.id, `Via ${i + 1}`, `${v.net || "no net"} at ${at(v.position)}`));
  Object.values(pcb.zones).forEach((z, i) => add("Zones", "zone", z.id, `Zone ${i + 1}`, `${z.net || "no net"} · ${z.layer}`));
  return out;
}

/** Case-insensitive search over the label and the detail. */
export function filterItems(items: ManagerItem[], query: string): ManagerItem[] {
  const q = query.trim().toLowerCase();
  return q ? items.filter((i) => i.label.toLowerCase().includes(q) || i.detail.toLowerCase().includes(q)) : items;
}

/** Which editor an entry navigates to. Components and nets follow the active editor (PCB or schematic). */
export function editorFor(t: Target, activeKind: string | undefined): "schematic" | "pcb" | null {
  if (t.kind === "page") return null;
  if (t.kind === "object") return t.obj === "wire" || t.obj === "label" ? "schematic" : "pcb";
  return activeKind === "pcb" ? "pcb" : "schematic";
}

/** PCB objects to select (highlight) and zoom to; empty when the target has nothing on the board. */
export function pcbObjects(s: Snapshot, t: Target): Id[] {
  const { pcb } = s.project;
  if (t.kind === "component") return Object.values(pcb.footprints).some((f) => f.symbol_id === t.symbolId) ? [t.symbolId] : [];
  if (t.kind === "object") return pcb.tracks[t.id] || pcb.vias[t.id] || pcb.zones[t.id] ? [t.id] : [];
  if (t.kind !== "net") return [];
  const n = t.name;
  return [
    ...Object.values(pcb.tracks).filter((x) => x.net === n).map((x) => x.id),
    ...Object.values(pcb.vias).filter((x) => x.net === n).map((x) => x.id),
    ...Object.values(pcb.zones).filter((x) => x.net === n).map((x) => x.id),
    ...s.geometry.footprints.filter((f) => f.pads.some((p) => p.net === n)).map((f) => f.id),
  ];
}

const MIN_SPAN_MM = 10;
const padded = (b: Bounds | null): Bounds | null => {
  if (!b) return null;
  const px = Math.max(0, (MIN_SPAN_MM - (b.maxX - b.minX)) / 2);
  const py = Math.max(0, (MIN_SPAN_MM - (b.maxY - b.minY)) / 2);
  return { minX: b.minX - px, minY: b.minY - py, maxX: b.maxX + px, maxY: b.maxY + py };
};

/**
 * Where a target sits in the schematic: its sheet and its view-space box. A net's box covers the
 * pins the engine's netlist puts on it ("R1.2") and its labels, on the first sheet it appears on.
 */
export function schematicPlace(s: Snapshot, t: Target, netlist: NetRow[] | null): { sheetId: Id; bounds: Bounds } | null {
  const { schematic: sch } = s.project;
  const pts: [number, number][] = [];
  const on: { sheet: Id | null } = { sheet: null };
  const take = (sheetId: Id, p: [number, number][]) => {
    if (on.sheet === null) on.sheet = sheetId;
    if (sheetId === on.sheet) pts.push(...p);
  };
  if (t.kind === "component") {
    const g = s.geometry.symbols.find((x) => x.id === t.symbolId);
    const y = sch.symbols[t.symbolId];
    if (g) {
      const [x0, y0] = toView(g.position);
      take(g.sheet_id, [
        [x0 - mm(g.half_w), y0 - mm(g.half_h)],
        [x0 + mm(g.half_w), y0 + mm(g.half_h)],
      ]);
    } else if (y) take(y.sheet_id, [toView(y.position)]);
  } else if (t.kind === "net") {
    const row = netlist?.find((n) => n.name === t.name);
    for (const ref of row?.pins ?? []) {
      const dot = ref.lastIndexOf(".");
      const g = s.geometry.symbols.find((x) => x.refdes === ref.slice(0, dot));
      const pin = g?.pins.find((p) => p.number === ref.slice(dot + 1));
      if (g && pin) take(g.sheet_id, [toView(pin.position)]);
    }
    for (const l of Object.values(sch.labels)) if (l.name === t.name) take(l.sheet_id, [toView(l.position)]);
  } else if (t.kind === "object") {
    const w = sch.wires[t.id];
    if (w) take(w.sheet_id, [toView(w.a), toView(w.b)]);
    const l = sch.labels[t.id];
    if (l) take(l.sheet_id, [toView(l.position)]);
  }
  const b = padded(boundsOf(pts));
  return on.sheet !== null && b ? { sheetId: on.sheet, bounds: b } : null;
}
