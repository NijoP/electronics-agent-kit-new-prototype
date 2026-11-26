// Quick open (Go to part, net or sheet…): library parts to place, symbols and nets to go to (the
// Design manager's navigation: open the sheet or board, select, zoom; F7) and sheets to open. A
// pure builder over its dependencies, so the UI inventory gate (`qa/uiInventory.ts`) probes every
// item the way it probes the registry actions.
import type { PaletteAction } from "../CommandPalette";
import { netNames } from "../shell/panels";
import { sheetsInOrder } from "../shell/sheets";
import type { LibraryEntry, Snapshot } from "../types";

export interface QuickOpenDeps {
  snapshot: Snapshot;
  library: LibraryEntry[];
  placePart: (c: LibraryEntry) => void;
  /** Opens the symbol's sheet (or the PCB when active), selects it and zooms to it. */
  showSymbol: (symbolId: string) => void;
  /** Zooms to the net in the active editor (schematic sheet or PCB). */
  showNet: (name: string) => void;
  open: (docId: string) => void;
}

export function quickOpenItems(d: QuickOpenDeps): PaletteAction[] {
  return [
    ...d.library.map((c) => ({ id: `part-${c.id}`, label: c.name, group: "Part · place", run: () => d.placePart(c) })),
    ...Object.values(d.snapshot.project.schematic.symbols).map((sy) => ({ id: `sym-${sy.id}`, label: `${sy.refdes} ${sy.value}`, group: "Symbol", run: () => d.showSymbol(sy.id) })),
    ...netNames(d.snapshot).map((n) => ({ id: `net-${n}`, label: n, group: "Net", run: () => d.showNet(n) })),
    ...sheetsInOrder(d.snapshot.project.schematic).map((sh) => ({ id: `sheet-${sh.id}`, label: sh.name, group: "Sheet", run: () => d.open(`sch:${sh.id}`) })),
  ];
}
