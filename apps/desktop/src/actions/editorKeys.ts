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

const digits = (tools: readonly (readonly [string, string])[]): EditorKey[] => tools.map(([, label], i) => ({ keys: String(i + 1), label: `${label} tool` }));

const SCHEMATIC: EditorKey[] = [
  { keys: "W", label: "Wire tool, from any other tool" },
  { keys: "R", label: "Rotate selection (while placing: rotate the part)" },
  { keys: "X / Y", label: "Mirror selection horizontally / vertically" },
  { keys: "Delete / Backspace", label: "Delete selection" },
  { keys: "Mod+C / Mod+X / Mod+V", label: "Copy / cut / paste" },
  { keys: "Mod+D", label: "Duplicate selection" },
  { keys: "Mod+A", label: "Select everything on the sheet" },
];

const PCB: EditorKey[] = [
  { keys: "R / F", label: "Rotate / flip selected footprints (Select and Move tools)" },
  { keys: "Delete / Backspace", label: "Delete the selected track (Select tool)" },
  { keys: "Enter", label: "Finish the route, outline or zone" },
  { keys: "Backspace", label: "Undo the last route segment (Route tool)" },
  { keys: "/", label: "Change the route corner mode (Route tool)" },
  { keys: "L", label: "Next copper layer (Route tool)" },
  { keys: "Mod+A", label: "Select every footprint" },
];

/** Editor-owned keys of the active document; none outside the schematic and PCB editors. */
export function editorKeys(ctx: DocContext): EditorKey[] {
  if (ctx === "schematic") return [...digits(SCH_TOOLS), ...SCHEMATIC];
  if (ctx === "pcb") return [...digits(PCB_TOOLS), ...PCB];
  return [];
}
