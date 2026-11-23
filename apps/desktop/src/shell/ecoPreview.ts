// ECO preview (Phase 1 C2, ADR-0041): the rows of the "Update PCB" review dialog, built from the
// engine's `sync_preview` change list (the delta SyncSchematicToPcb would journal), plus the
// selection rules. Pure functions: the IPC calls live in the workspace.
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
      rows.push({ id, action: "Add", object: "Footprint", name: ref, before: NONE, after: after.component_id ?? "" });
      for (const [pad, net] of pads(after)) rows.push({ id: `net:${key}:${pad}`, action: "Add", object: "Pad net", name: `${ref}.${pad}`, before: NONE, after: net, requires: id });
      continue;
    }
    if (before && !after) {
      const ref = before.refdes ?? key;
      const id = `fp-:${key}`;
      rows.push({ id, action: "Delete", object: "Footprint", name: ref, before: before.component_id ?? "", after: NONE });
      for (const [pad, net] of pads(before)) rows.push({ id: `net:${key}:${pad}`, action: "Delete", object: "Pad net", name: `${ref}.${pad}`, before: net, after: NONE, requires: id });
      continue;
    }
    const b = before!;
    const a = after!;
    const ref = a.refdes ?? key;
    if (b.refdes !== a.refdes) rows.push({ id: `ref:${key}`, action: "Update", object: "Reference", name: ref, before: b.refdes ?? NONE, after: a.refdes ?? NONE });
    if (b.component_id !== a.component_id) rows.push({ id: `part:${key}`, action: "Update", object: "Part", name: ref, before: b.component_id ?? NONE, after: a.component_id ?? NONE });
    const other = (["side", "locked", "position", "rotation"] as const).filter((f) => !same(b[f], a[f]));
    if (other.length) rows.push({ id: `fp~:${key}`, action: "Update", object: "Footprint", name: ref, before: other.join(", "), after: "changed" });
    const bp = pads(b);
    const ap = pads(a);
    for (const [pad, net] of ap) {
      const old = bp.get(pad);
      if (old === undefined) rows.push({ id: `net:${key}:${pad}`, action: "Add", object: "Pad net", name: `${ref}.${pad}`, before: NONE, after: net });
      else if (old !== net) rows.push({ id: `net:${key}:${pad}`, action: "Update", object: "Pad net", name: `${ref}.${pad}`, before: old, after: net });
    }
    for (const [pad, net] of bp) if (!ap.has(pad)) rows.push({ id: `net:${key}:${pad}`, action: "Delete", object: "Pad net", name: `${ref}.${pad}`, before: net, after: NONE });
  }
  return rows;
}

/**
 * Check or uncheck one row, keeping dependencies consistent: a pad net of a footprint being added
 * or deleted is never selected without that footprint, so unchecking the footprint unchecks its
 * pad nets and checking such a pad net checks its footprint.
 */
export function toggleRow(rows: EcoRow[], selected: ReadonlySet<string>, id: string, on: boolean): Set<string> {
  const next = new Set(selected);
  const row = rows.find((r) => r.id === id);
  if (!row) return next;
  if (on) {
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

/**
 * Whether the selection can be applied. The engine applies the schematic-to-PCB sync as one
 * command, so only the complete change list can be applied today; a partial selection is refused
 * with the reason instead of being applied as if it were complete.
 */
export function applyState(rows: EcoRow[], selected: ReadonlySet<string>): { ok: boolean; reason: string | null } {
  if (rows.length === 0) return { ok: false, reason: "Nothing to apply: the PCB already matches the schematic." };
  const unchecked = rows.filter((r) => !selected.has(r.id)).length;
  if (unchecked === 0) return { ok: true, reason: null };
  return {
    ok: false,
    reason: `${unchecked} change(s) unchecked. Applying only some changes is not available yet: the engine applies the schematic-to-PCB update as one command, and selective apply needs a new engine command. Check every change to apply, or cancel.`,
  };
}
