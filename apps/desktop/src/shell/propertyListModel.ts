// Property List (Phase 1 slice E4): every object of one kind as a grid row, its properties as
// columns. Pure, and built on the A5 inspector model, so a cell shows what the Inspector shows
// for that object and a batch edit commits what the Inspector commits for the selected objects:
// one command, a `Batch` for several (one revision, one undo step). Fields the IR cannot hold
// are not offered as columns (the Inspector lists them as unavailable).
import type { Id, Snapshot } from "../types";
import { layerName } from "../layers";
import { NO_SELECTION, selectionModel, type Choice, type Commit, type PropField, type SelectionRef } from "./inspectorModel";

export type ListKind = "symbols" | "footprints" | "labels" | "tracks";
export const LIST_KINDS: readonly { kind: ListKind; label: string }[] = [
  { kind: "symbols", label: "Components" },
  { kind: "footprints", label: "Footprints" },
  { kind: "labels", label: "Net labels" },
  { kind: "tracks", label: "Tracks" },
];

export interface ListColumn {
  id: string;
  label: string;
  editable: boolean;
  /** Set for a column edited by picking a value. */
  choices?: Choice[];
}
export interface ListRow {
  id: Id;
  label: string;
  cells: Record<string, string>;
}
export interface ListModel {
  columns: ListColumn[];
  rows: ListRow[];
}

const refOf = (kind: ListKind, ids: Id[]): SelectionRef => ({ ...NO_SELECTION, [kind]: ids });
/** A field the grid shows: not unavailable, and not the schematic Group row (grouping, not a property). */
const shown = (f: PropField) => !f.unavailable && f.id !== "group";
const byText = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

function labelled(s: Snapshot, kind: ListKind): { id: Id; label: string }[] {
  const { schematic, pcb } = s.project;
  const out =
    kind === "symbols"
      ? Object.values(schematic.symbols).map((y) => ({ id: y.id, label: y.refdes }))
      : kind === "footprints"
        ? Object.values(pcb.footprints).map((f) => ({ id: f.id, label: f.refdes }))
        : kind === "labels"
          ? Object.values(schematic.labels).map((l) => ({ id: l.id, label: l.name }))
          : Object.values(pcb.tracks).map((t) => ({ id: t.id, label: `${t.net} · ${layerName(t.layer)}` }));
  return out.sort((a, b) => byText(a.label, b.label) || byText(a.id, b.id));
}

/** Count of objects of each kind (the kind picker shows it). */
export function kindCount(s: Snapshot, kind: ListKind): number {
  const { schematic, pcb } = s.project;
  return Object.keys(kind === "symbols" ? schematic.symbols : kind === "footprints" ? pcb.footprints : kind === "labels" ? schematic.labels : pcb.tracks).length;
}

/** The grid: one row per object (sorted by its label), the columns of one object's fields. */
export function listModel(s: Snapshot, kind: ListKind): ListModel {
  const rows: ListRow[] = [];
  let columns: ListColumn[] = [];
  for (const { id, label } of labelled(s, kind)) {
    const m = selectionModel(s, refOf(kind, [id]));
    if (!m) continue;
    const fields = m.fields.filter(shown);
    if (rows.length === 0)
      columns = fields.map((f) => ({ id: f.id, label: f.label, editable: !!f.edit, ...(f.edit?.kind === "select" ? { choices: f.edit.options } : {}) }));
    rows.push({ id, label, cells: Object.fromEntries(fields.map((f) => [f.id, f.value])) });
  }
  return { columns, rows };
}

/**
 * Commit `value` to one column of the given objects: the Inspector's edit for that selection, so
 * N cells are one command (a `Batch`), never N separate revisions. A message when the edit cannot
 * apply (read-only column, a designator for several parts, a locked footprint, a bad number),
 * null when every cell already holds the value.
 */
export function commitCells(s: Snapshot, kind: ListKind, column: string, ids: Id[], value: string): Commit {
  if (ids.length === 0) return "Select one or more cells first.";
  const f = selectionModel(s, refOf(kind, ids))?.fields.find((x) => x.id === column);
  if (!f) return "This column cannot be edited.";
  if (f.unavailable) return f.unavailable;
  if (!f.edit) return `${f.label} is read-only.`;
  return f.edit.commit(value);
}

/** The value a batch edit starts from: the cells' common value, or empty when they differ. */
export function commonValue(rows: ListRow[], column: string, ids: Id[]): string {
  const values = new Set(rows.filter((r) => ids.includes(r.id)).map((r) => r.cells[column] ?? ""));
  return values.size === 1 ? [...values][0] : "";
}

/** Rows from `a` to `b` inclusive in grid order (Shift+click), in either direction. */
export function rowRange(rows: ListRow[], a: Id, b: Id): Id[] {
  const i = rows.findIndex((r) => r.id === a);
  const j = rows.findIndex((r) => r.id === b);
  if (i < 0 || j < 0) return [b];
  const [lo, hi] = i <= j ? [i, j] : [j, i];
  return rows.slice(lo, hi + 1).map((r) => r.id);
}
