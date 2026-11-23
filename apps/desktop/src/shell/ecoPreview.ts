// ECO preview (Phase 1 C2, ADR-0041): the rows of the "Update PCB" review dialog, built from the
// engine's `sync_preview` change list (the delta SyncSchematicToPcb would journal), plus the
// selection rules. Pure functions: the IPC calls live in the workspace.
import type { SyncChange } from "../types";
import type { EngineChange } from "./syncReport";

export interface SyncPreview {
  /** Revision the preview was computed for: the change is applied only at this revision. */
  revision: number;
  changes: EngineChange[];
  /** The sync's own warnings, e.g. a part left off the PCB because its pin map is unusable. */
  warnings: string[];
}

export type EcoAction = "Add" | "Delete" | "Update";
export type EcoObject = "Footprint" | "Reference" | "Part" | "Pad net" | "Other";
export type EcoGroupBy = "action" | "object";

export interface EcoRow {
  /** Stable within one preview; listed in the exported report when unchecked. */
  id: string;
  action: EcoAction;
  object: EcoObject;
  /** What the row is about: a reference (`R1`) or a pad (`R1.2`). */
  name: string;
  before: string;
  after: string;
  /** Row this one cannot be applied without (a pad net of a footprint being added or deleted). */
  requires?: string;
  /**
   * The engine change this row applies on its own (SyncSchematicToPcbSelected, ADR-0042). Absent
   * for a pad net of a footprint being deleted (it goes with the footprint) and for a change the
   * selective command cannot name, which only the full update applies.
   */
  change?: SyncChange;
}

interface FootprintLike {
  symbol_id?: string;
  refdes?: string;
  component_id?: string;
  side?: string;
  locked?: boolean;
  position?: { x: number; y: number };
  rotation?: unknown;
  pad_nets?: Record<string, string>;
}

const NONE = "—";
const asFp = (v: unknown): FootprintLike | null => (v && typeof v === "object" ? (v as FootprintLike) : null);
const pads = (fp: FootprintLike | null) => new Map(Object.entries(fp?.pad_nets ?? {}));
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** One row per reviewable change, in the engine's order (pad nets right after their footprint). */
export function buildEcoRows(changes: EngineChange[]): EcoRow[] {
  const rows: EcoRow[] = [];
  for (const c of changes) {
    const isFp = c.path.length === 2 && c.path[0] === "pcb" && c.path[1] === "footprints";
    const before = isFp ? asFp(c.before) : null;
    const after = isFp ? asFp(c.after) : null;
    // A footprint the sync creates has a fresh id each time, so key its rows by its symbol.
    const key = (after ?? before)?.symbol_id ?? c.key ?? "?";
    if (!before && !after) {
      const what = `${c.path.join(".")}${c.key ? ` ${c.key}` : ""}`;
      rows.push({ id: `other:${c.path.join(".")}:${c.key ?? ""}`, action: "Update", object: "Other", name: what, before: c.before == null ? NONE : "changed", after: c.after == null ? NONE : "changed" });
      continue;
    }
    if (!before && after) {
      const ref = after.refdes ?? key;
      const id = `fp+:${key}`;
      rows.push({ id, action: "Add", object: "Footprint", name: ref, before: NONE, after: after.component_id ?? "", change: { kind: "add_footprint", symbol_id: key } });
      for (const [pad, net] of pads(after)) rows.push({ id: `net:${key}:${pad}`, action: "Add", object: "Pad net", name: `${ref}.${pad}`, before: NONE, after: net, requires: id, change: { kind: "pad_net", symbol_id: key, pad } });
      continue;
    }
    if (before && !after) {
      const ref = before.refdes ?? key;
      const id = `fp-:${key}`;
      rows.push({ id, action: "Delete", object: "Footprint", name: ref, before: before.component_id ?? "", after: NONE, change: { kind: "remove_footprint", symbol_id: key } });
      for (const [pad, net] of pads(before)) rows.push({ id: `net:${key}:${pad}`, action: "Delete", object: "Pad net", name: `${ref}.${pad}`, before: net, after: NONE, requires: id });
      continue;
    }
    const b = before!;
    const a = after!;
    const ref = a.refdes ?? key;
    if (b.refdes !== a.refdes) rows.push({ id: `ref:${key}`, action: "Update", object: "Reference", name: ref, before: b.refdes ?? NONE, after: a.refdes ?? NONE, change: { kind: "refdes", symbol_id: key } });
    if (b.component_id !== a.component_id) rows.push({ id: `part:${key}`, action: "Update", object: "Part", name: ref, before: b.component_id ?? NONE, after: a.component_id ?? NONE, change: { kind: "part", symbol_id: key } });
    const other = (["side", "locked", "position", "rotation"] as const).filter((f) => !same(b[f], a[f]));
    if (other.length) rows.push({ id: `fp~:${key}`, action: "Update", object: "Footprint", name: ref, before: other.join(", "), after: "changed" });
    const bp = pads(b);
    const ap = pads(a);
    const padNet = (pad: string): SyncChange => ({ kind: "pad_net", symbol_id: key, pad });
    for (const [pad, net] of ap) {
      const old = bp.get(pad);
      if (old === undefined) rows.push({ id: `net:${key}:${pad}`, action: "Add", object: "Pad net", name: `${ref}.${pad}`, before: NONE, after: net, change: padNet(pad) });
      else if (old !== net) rows.push({ id: `net:${key}:${pad}`, action: "Update", object: "Pad net", name: `${ref}.${pad}`, before: old, after: net, change: padNet(pad) });
    }
    for (const [pad, net] of bp) if (!ap.has(pad)) rows.push({ id: `net:${key}:${pad}`, action: "Delete", object: "Pad net", name: `${ref}.${pad}`, before: net, after: NONE, change: padNet(pad) });
  }
  return rows;
}

/**
 * Check or uncheck one row, keeping dependencies consistent: a pad net of a footprint being added
 * or deleted is never selected without that footprint, so unchecking the footprint unchecks its
 * pad nets and checking such a pad net checks its footprint. A pad net of a footprint being
 * deleted leaves with the footprint, so toggling it toggles the footprint and all its pad nets.
 */
export function toggleRow(rows: EcoRow[], selected: ReadonlySet<string>, id: string, on: boolean): Set<string> {
  const next = new Set(selected);
  const row = rows.find((r) => r.id === id);
  if (!row) return next;
  if (row.requires && !row.change) return toggleRow(rows, selected, row.requires, on);
  if (on) {
    for (const r of rows) if (r.requires === id && !r.change) next.add(r.id);
    next.add(id);
    if (row.requires) next.add(row.requires);
  } else {
    next.delete(id);
    for (const r of rows) if (r.requires === id) next.delete(r.id);
  }
  return next;
}

/** Group rows for display, keeping first-seen group order and the row order inside each group. */
export function groupRows(rows: EcoRow[], by: EcoGroupBy): { group: string; rows: EcoRow[] }[] {
  const out = new Map<string, EcoRow[]>();
  for (const r of rows) {
    const g = by === "action" ? r.action : r.object;
    out.set(g, [...(out.get(g) ?? []), r]);
  }
  return [...out].map(([group, rs]) => ({ group, rows: rs }));
}

export interface EcoApply {
  ok: boolean;
  /** Why Apply is refused, or null. */
  reason: string | null;
  /**
   * What Apply sends: null for the complete change list (SyncSchematicToPcb), otherwise the chosen
   * engine changes (SyncSchematicToPcbSelected). Either way one command, one revision, one undo.
   */
  changes: SyncChange[] | null;
  /** How many reviewable rows the apply covers. */
  count: number;
}

/**
 * What Apply would send for the selection. Every row checked applies the full update; a partial
 * selection applies only the chosen changes through the selective command, as one revision. When
 * the list holds a change the selective command cannot name, only the full update can be applied,
 * so a partial selection is refused with the reason.
 */
export function applyState(rows: EcoRow[], selected: ReadonlySet<string>): EcoApply {
  const chosen = rows.filter((r) => selected.has(r.id));
  const count = chosen.length;
  if (rows.length === 0) return { ok: false, reason: "Nothing to apply: the PCB already matches the schematic.", changes: null, count };
  if (count === rows.length) return { ok: true, reason: null, changes: null, count };
  if (count === 0) return { ok: false, reason: "No change checked: check the changes to apply, or cancel.", changes: null, count };
  // Checked or not, such a row cannot be part of a partial apply: the selective command would
  // silently leave a checked one out.
  const fixed = rows.filter((r) => !r.change && !r.requires);
  if (fixed.length > 0) {
    return {
      ok: false,
      reason: `${fixed.map((r) => r.name).join(", ")} can only be applied with the full update: check every change to apply, or cancel.`,
      changes: null,
      count,
    };
  }
  const changes = chosen.flatMap((r) => (r.change ? [r.change] : []));
  return { ok: true, reason: null, changes, count };
}
