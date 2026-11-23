// Net highlight in the schematic (slice B3). Which pins and labels form a net is the engine's
// answer (the `netlist` IPC, eak-engineering); this module only finds, on one sheet, the drawn
// objects that belong to it: wires joined end to end or on a wire's interior (a T-join, as the
// engine joins them, see junctions.ts), and the pins and labels sitting on those points.
import type { Id, Point, Snapshot } from "../../types";
import { onInterior } from "./junctions";

/** One engine netlist row: the net's name, its pins ("R1.2") and its label names. */
export interface NetRow {
  name: string;
  pins: string[];
  label_names: string[];
}

/** Objects of one sheet that are electrically joined by drawing. */
export interface Group {
  wires: Id[];
  labels: Id[];
  /** "symbolId/pin" keys of the pins in the group. */
  pinKeys: string[];
  /** Engine pin references ("R1.2"). */
  pins: string[];
  labelNames: string[];
}

const key = (p: Point) => `${p.x},${p.y}`;

/** Connected groups of a sheet (union-find over connection points). */
export function sheetGroups(snap: Snapshot, sheet: Id): Group[] {
  const wires = Object.values(snap.project.schematic.wires).filter((w) => w.sheet_id === sheet);
  const labels = Object.values(snap.project.schematic.labels).filter((l) => l.sheet_id === sheet);
  const syms = snap.geometry.symbols.filter((s) => s.sheet_id === sheet);
  const parent = new Map<string, string>();
  const find = (k: string): string => {
    let r = k;
    while (parent.has(r) && parent.get(r) !== r) r = parent.get(r)!;
    parent.set(k, r);
    return r;
  };
  const node = (p: Point) => {
    const k = key(p);
    if (!parent.has(k)) parent.set(k, k);
    return k;
  };
  const join = (a: string, b: string) => {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) parent.set(ra, rb);
  };
  for (const w of wires) join(node(w.a), node(w.b));
  const points: Point[] = [...wires.flatMap((w) => [w.a, w.b]), ...labels.map((l) => l.position), ...syms.flatMap((s) => s.pins.map((p) => p.position))];
  for (const p of points) {
    node(p);
    for (const w of wires) if (onInterior(p, w.a, w.b)) join(key(p), key(w.a));
  }
  const groups = new Map<string, Group>();
  const at = (p: Point) => {
    const r = find(key(p));
    const known = groups.get(r);
    if (known) return known;
    const g: Group = { wires: [], labels: [], pinKeys: [], pins: [], labelNames: [] };
    groups.set(r, g);
    return g;
  };
  for (const w of wires) at(w.a).wires.push(w.id);
  for (const l of labels) {
    const g = at(l.position);
    g.labels.push(l.id);
    if (!g.labelNames.includes(l.name)) g.labelNames.push(l.name);
  }
  for (const s of syms)
    for (const p of s.pins) {
      const g = at(p.position);
      g.pinKeys.push(`${s.id}/${p.number}`);
      g.pins.push(`${s.refdes}.${p.number}`);
    }
  return [...groups.values()];
}

/** What is highlighted: the net's name (null for an unnamed, floating group) and how to find it. */
export interface NetHighlight {
  name: string | null;
  pins: string[];
  labelNames: string[];
  /** The wire or label the highlight started from, so an unnamed group can still be found. */
  seed: Id;
}

const groupOf = (groups: Group[], id: Id) => groups.find((g) => g.wires.includes(id) || g.labels.includes(id));

/**
 * The net of a wire or label: the engine netlist row that shares a pin or a label name with its
 * group, else (no row, or no netlist) the group's own first label name, else an unnamed net.
 */
export function netAt(groups: Group[], rows: readonly NetRow[] | null, id: Id): NetHighlight | null {
  const g = groupOf(groups, id);
  if (!g) return null;
  const row = rows?.find((r) => r.pins.some((p) => g.pins.includes(p)) || r.label_names.some((n) => g.labelNames.includes(n)));
  if (row) return { name: row.name, pins: row.pins, labelNames: row.label_names, seed: id };
  return { name: g.labelNames[0] ?? null, pins: g.pins, labelNames: g.labelNames, seed: id };
}

/** Drawn members of a highlighted net on a sheet. */
export interface NetMembers {
  wires: Set<Id>;
  labels: Set<Id>;
  pinKeys: Set<string>;
}

export function netMembers(groups: Group[], h: NetHighlight): NetMembers {
  const m: NetMembers = { wires: new Set(), labels: new Set(), pinKeys: new Set() };
  for (const g of groups) {
    const on = g.wires.includes(h.seed) || g.labels.includes(h.seed) || g.pins.some((p) => h.pins.includes(p)) || g.labelNames.some((n) => h.labelNames.includes(n));
    if (!on) continue;
    g.wires.forEach((w) => m.wires.add(w));
    g.labels.forEach((l) => m.labels.add(l));
    g.pinKeys.forEach((p) => m.pinKeys.add(p));
  }
  return m;
}
