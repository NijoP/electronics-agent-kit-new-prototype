// Quick open (Go to part, net or sheet…): library parts to place, symbols to select, nets to report
// and sheets to open. A pure builder over its dependencies, so the UI inventory gate
// (`qa/uiInventory.ts`) probes every item the way it probes the registry actions.
import type { PaletteAction } from "../CommandPalette";
import { netNames } from "../shell/panels";
import { sheetsInOrder } from "../shell/sheets";
import type { LibraryEntry, Snapshot } from "../types";

export interface QuickOpenDeps {
  snapshot: Snapshot;
  library: LibraryEntry[];
  placePart: (c: LibraryEntry) => void;
  select: (id: string) => void;
  /** Reports the net (pads and track segments on the PCB). */
  showNet: (name: string) => void;
  open: (docId: string) => void;
}

export function quickOpenItems(d: QuickOpenDeps): PaletteAction[] {
  return [
    ...d.library.map((c) => ({ id: `part-${c.id}`, label: c.name, group: "Part · place", run: () => d.placePart(c) })),
    ...Object.values(d.snapshot.project.schematic.symbols).map((sy) => ({ id: `sym-${sy.id}`, label: `${sy.refdes} ${sy.value}`, group: "Symbol", run: () => d.select(sy.id) })),
    ...netNames(d.snapshot).map((n) => ({ id: `net-${n}`, label: n, group: "Net", run: () => d.showNet(n) })),
    ...sheetsInOrder(d.snapshot.project.schematic).map((sh) => ({ id: `sheet-${sh.id}`, label: sh.name, group: "Sheet", run: () => d.open(`sch:${sh.id}`) })),
  ];
}
