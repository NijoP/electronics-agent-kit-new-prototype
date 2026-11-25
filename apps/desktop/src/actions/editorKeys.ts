// Keys the schematic and PCB editors handle themselves (they own the canvas focus, so these are
// not registry actions). The digit keys are generated from each editor's own tool list, the same
// list its key handler indexes, so Help cannot drift from what a digit selects.
import { TOOLS as PCB_TOOLS } from "../PcbView";
import { TOOLS as SCH_TOOLS } from "../SchematicView";
import type { DocContext } from "./types";

export interface EditorKey {
  keys: string;
  label: string;
}

// Only nine digits exist: later tools are chosen from the toolbar or the Place menu.
const digits = (tools: readonly (readonly [string, string])[]): EditorKey[] => tools.map(([, label], i) => ({ keys: i < 9 ? String(i + 1) : "Place menu", label: `${label} tool` }));

const SCHEMATIC: EditorKey[] = [
  { keys: "W", label: "Wire tool, from any other tool" },
  { keys: "R", label: "Rotate selection (while placing: rotate the part)" },
  { keys: "X / Y", label: "Mirror selection horizontally / vertically" },
  { keys: "Delete / Backspace", label: "Delete selection" },
  { keys: "Mod+C / Mod+X / Mod+V", label: "Copy / cut / paste" },
  { keys: "Mod+D", label: "Duplicate selection" },
  { keys: "Mod+A", label: "Select everything on the sheet (the selection filter applies)" },
  { keys: "Mod+F", label: "Find and replace designators, values and net labels" },
  { keys: "Mod+G / Mod+Shift+G", label: "Group the selection / ungroup it" },
  { keys: "Right-click", label: "Actions for the clicked part, wire, label or sheet" },
  { keys: "Esc", label: "Clear the selection and the net highlight" },
  { keys: "/", label: "Wire and bus corner mode: orthogonal, 45°, any angle" },
  { keys: "Space", label: "Flip the bend of the wire or bus leg being drawn" },
  { keys: "Shift", label: "Hold: keep the next wire or bus point on one axis" },
  { keys: "Enter", label: "Finish the bus (Bus tool)" },
  { keys: "R", label: "Turn the bus entry (Bus entry tool)" },
];

const PCB: EditorKey[] = [
  { keys: "R / F", label: "Rotate / flip selected footprints (Select and Move tools)" },
  { keys: "Delete / Backspace", label: "Delete the selected track (Select tool)" },
  { keys: "Enter", label: "Finish the route, outline, zone, keep-out or cut-out" },
  { keys: "Backspace", label: "Undo the last route segment (Route tool)" },
  { keys: "/", label: "Change the route corner mode (Route tool)" },
  { keys: "L", label: "Next copper layer (Route tool)" },
  { keys: "+ / -", label: "Next / previous copper layer (any tool; while routing, a via)" },
  { keys: "Alt+T / Alt+B", label: "Top / bottom copper layer" },
  { keys: "Mod+A", label: "Select every footprint" },
];

/** Editor-owned keys of the active document; none outside the schematic and PCB editors. */
export function editorKeys(ctx: DocContext): EditorKey[] {
  if (ctx === "schematic") return [...digits(SCH_TOOLS), ...SCHEMATIC];
  if (ctx === "pcb") return [...digits(PCB_TOOLS), ...PCB];
  return [];
}
