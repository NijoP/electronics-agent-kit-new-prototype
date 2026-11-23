// Sheet management (Phase 1 B6, ADR-0043): the sheet order the engine uses, what is on a sheet,
// and the typed commands the Pages tab submits. Nothing here edits the design: every change is a
// RenameSheet / DeleteSheet / ReorderSheets / AddSheet command (a Batch for a non-empty delete).
import type { Command, Id, Sheet, Snapshot } from "../types";

type Sch = Pick<Snapshot["project"]["schematic"], "sheets">;

/** Sheets in the engine's order: by `order` (absent = 0), then id (byte order, as in Rust). */
export function sheetsInOrder(sch: Sch): Sheet[] {
  return Object.values(sch.sheets).sort((a, b) => (a.order ?? 0) - (b.order ?? 0) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

const on = <T extends { sheet_id: Id }>(m: Record<Id, T> | undefined, sheetId: Id) => Object.values(m ?? {}).filter((x) => x.sheet_id === sheetId);

/** What is on a sheet, as "2 symbol(s)" phrases (empty = nothing). */
export function sheetContents(s: Snapshot, sheetId: Id): string[] {
  const sch = s.project.schematic;
  const counts: [number, string][] = [
    [on(sch.symbols, sheetId).length, "symbol"],
    [on(sch.wires, sheetId).length, "wire"],
    [on(sch.labels, sheetId).length, "net label"],
    [on(sch.buses, sheetId).length, "bus"],
    [on(sch.bus_entries, sheetId).length, "bus entry"],
    [on(sch.graphics, sheetId).length, "drawing"],
  ];
  return counts.filter(([n]) => n > 0).map(([n, what]) => `${n} ${what}(s)`);
}

/**
 * The command that deletes a sheet: DeleteSheet alone when it is empty, otherwise one Batch that
 * deletes everything on it first (one undo step). Symbols take their no-connects and footprints.
 */
export function deleteSheetCommand(s: Snapshot, sheetId: Id): Command {
  const sch = s.project.schematic;
  const steps: Command[] = [
    ...on(sch.graphics, sheetId).map((g): Command => ({ type: "DeleteSheetGraphic", graphic_id: g.id })),
    ...on(sch.labels, sheetId).map((l): Command => ({ type: "DeleteNetLabel", label_id: l.id })),
    ...on(sch.bus_entries, sheetId).map((e): Command => ({ type: "DeleteBusEntry", entry_id: e.id })),
    ...on(sch.buses, sheetId).map((b): Command => ({ type: "DeleteBus", bus_id: b.id })),
    ...on(sch.wires, sheetId).map((w): Command => ({ type: "DeleteWire", wire_id: w.id })),
    ...on(sch.symbols, sheetId).map((y): Command => ({ type: "DeleteSymbol", symbol_id: y.id })),
  ];
  const del: Command = { type: "DeleteSheet", sheet_id: sheetId };
  if (steps.length === 0) return del;
  const name = sch.sheets[sheetId]?.name ?? sheetId;
  return { type: "Batch", commands: [...steps, del], label: `Delete sheet ${name}` };
}

/** ReorderSheets moving one sheet one place up (-1) or down (+1); null at either end. */
export function moveSheetCommand(sch: Sch, sheetId: Id, by: -1 | 1): Command | null {
  const ids = sheetsInOrder(sch).map((x) => x.id);
  const i = ids.indexOf(sheetId);
  const j = i + by;
  if (i < 0 || j < 0 || j >= ids.length) return null;
  [ids[i], ids[j]] = [ids[j], ids[i]];
  return { type: "ReorderSheets", sheet_ids: ids };
}

/** "Sheet N" with the smallest N no sheet is named yet (counting from the sheet count + 1). */
export function nextSheetName(sch: Sch): string {
  const names = new Set(Object.values(sch.sheets).map((x) => x.name.trim()));
  let n = Object.keys(sch.sheets).length + 1;
  while (names.has(`Sheet ${n}`)) n += 1;
  return `Sheet ${n}`;
}

/** Why a new sheet name is refused before it is submitted (the engine checks the same). */
export function renameProblem(sch: Sch, sheetId: Id, name: string): string | null {
  const n = name.trim();
  if (!n) return "A sheet needs a name.";
  if (sch.sheets[sheetId]?.name === n) return "That is already the sheet's name.";
  const other = Object.values(sch.sheets).find((x) => x.id !== sheetId && x.name.trim() === n);
  return other ? `Another sheet is already named “${n}”.` : null;
}
