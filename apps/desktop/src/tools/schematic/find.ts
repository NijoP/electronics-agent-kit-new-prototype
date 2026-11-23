// Find and replace in the schematic (slice B3): reference designators, values and net-label
// names. Pure: hits come from the engine snapshot, and a replace is a list of typed commands
// the editor sends as one Batch (one revision, one undo step).
import type { Command, Id, Snapshot } from "../../types";
import type { Kind } from "./selection";

export type FindField = "refdes" | "value" | "label";
export const FIND_FIELDS: readonly { field: FindField; label: string }[] = [
  { field: "refdes", label: "Designators" },
  { field: "value", label: "Values" },
  { field: "label", label: "Net labels" },
];

export interface FindOptions {
  fields: Record<FindField, boolean>;
  matchCase: boolean;
  /** The whole text must equal the query (otherwise any part of it may match). */
  whole: boolean;
}
export const DEFAULT_FIND: FindOptions = { fields: { refdes: true, value: true, label: true }, matchCase: false, whole: false };

export interface FindHit {
  kind: Kind;
  id: Id;
  sheet: Id;
  field: FindField;
  text: string;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const pattern = (query: string, o: FindOptions) => new RegExp(o.whole ? `^${escape(query)}$` : escape(query), o.matchCase ? "g" : "gi");

/** Every object whose searched text matches, in sheet order then by text. */
export function findHits(snap: Snapshot, query: string, o: FindOptions): FindHit[] {
  if (!query) return [];
  const re = pattern(query, o);
  const test = (t: string) => {
    re.lastIndex = 0;
    return re.test(t);
  };
  const { symbols, labels } = snap.project.schematic;
  const hits: FindHit[] = [];
  for (const s of Object.values(symbols)) {
    if (o.fields.refdes && test(s.refdes)) hits.push({ kind: "symbol", id: s.id, sheet: s.sheet_id, field: "refdes", text: s.refdes });
    if (o.fields.value && test(s.value)) hits.push({ kind: "symbol", id: s.id, sheet: s.sheet_id, field: "value", text: s.value });
  }
  if (o.fields.label) for (const l of Object.values(labels)) if (test(l.name)) hits.push({ kind: "label", id: l.id, sheet: l.sheet_id, field: "label", text: l.name });
  const sheetName = (id: Id) => snap.project.schematic.sheets[id]?.name ?? id;
  return hits.sort((a, b) => sheetName(a.sheet).localeCompare(sheetName(b.sheet)) || a.text.localeCompare(b.text, undefined, { numeric: true }) || a.field.localeCompare(b.field));
}

/** The text after replacing every match of the query (the whole text when `whole`). */
export function replaced(text: string, query: string, replacement: string, o: FindOptions): string {
  return query ? text.replace(pattern(query, o), () => replacement) : text;
}

/**
 * Commands that replace the query in every hit: `SetRefdes`, `SetSymbolValue`, and for a net
 * label `RenameNetLabel` (ADR-0045: the label keeps its id, position, kind, scope and style).
 * Hits whose text would not change, or a designator that would become empty, are skipped.
 */
export function replaceCommands(snap: Snapshot, hits: readonly FindHit[], query: string, replacement: string, o: FindOptions): Command[] {
  const { labels } = snap.project.schematic;
  const out: Command[] = [];
  for (const h of hits) {
    const to = replaced(h.text, query, replacement, o);
    if (to === h.text) continue;
    if (h.field === "refdes") {
      if (to.trim()) out.push({ type: "SetRefdes", symbol_id: h.id, refdes: to.trim() });
    } else if (h.field === "value") out.push({ type: "SetSymbolValue", symbol_id: h.id, value: to });
    else {
      const l = labels[h.id];
      if (!l || !to.trim() || to.trim() === l.name) continue;
      out.push({ type: "RenameNetLabel", label_id: l.id, name: to.trim() });
    }
  }
  return out;
}
