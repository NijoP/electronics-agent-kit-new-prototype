// No-connect markers (pure). A marker belongs to a symbol pin, so it is on the sheet of that
// symbol; the No connect tool toggles it: a click on an unmarked pin sends `SetNoConnect`, a
// click on a marked pin sends `DeleteNoConnect` (ADR-0038). The engine refuses a second marker.
import type { Command, Id, NoConnect, Snapshot } from "../../types";

/** The markers whose symbol is on `sheetId` (markers of other sheets are not drawn here). */
export function sheetNoConnects(s: Snapshot, sheetId: Id): NoConnect[] {
  const sc = s.project.schematic;
  return Object.values(sc.no_connects).filter((n) => sc.symbols[n.symbol_id]?.sheet_id === sheetId);
}

/** The marker on this pin, if any. */
export const noConnectOn = (s: Snapshot, symbolId: Id, pin: string): NoConnect | undefined =>
  Object.values(s.project.schematic.no_connects).find((n) => n.symbol_id === symbolId && n.pin === pin);

/** A click with the No connect tool on a pin: mark it, or remove its marker. */
export function toggleNoConnect(s: Snapshot, symbolId: Id, pin: string): Command {
  const nc = noConnectOn(s, symbolId, pin);
  return nc ? { type: "DeleteNoConnect", no_connect_id: nc.id } : { type: "SetNoConnect", symbol_id: symbolId, pin };
}
