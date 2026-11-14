// Main toolbar. Data-driven like the menus: every button runs a real operation or is disabled
// with its reason as the tooltip (no decorative controls).
import { formatShortcut } from "../design/shortcuts";

export interface ToolbarItem {
  id: string;
  label: string;
  run?: () => void;
  shortcut?: string;
  /** Why the control is unavailable; presence disables it. */
  disabled?: string;
  pressed?: boolean;
  title?: string;
}
export type ToolbarGroup = ToolbarItem[];

/** What the editor area is showing: a schematic, the PCB, a file viewer, a registered view (3D), or nothing. */
export type ToolbarContext = "schematic" | "pcb" | "file" | "view" | "none";

const EDITING: ToolbarContext[] = ["schematic", "pcb"];
/** Where each context-specific button applies. Buttons not listed here are always shown. */
export const TOOLBAR_CONTEXT: Record<string, ToolbarContext[]> = {
  undo: EDITING, redo: EDITING, select: EDITING, move: EDITING, rotate: EDITING, delete: EDITING,
  zout: EDITING, zin: EDITING, fit: EDITING, grid: EDITING, rules: EDITING, sync: EDITING,
  erc: ["schematic"], sim: ["schematic"],
  drc: ["pcb"], dfm: ["pcb"], route: ["pcb"], export: ["pcb"],
};

/** The toolbar for the current context: drops buttons that do not apply and groups left empty. */
export function filterToolbar(groups: ToolbarGroup[], ctx: ToolbarContext): ToolbarGroup[] {
  return groups
    .map((g) => g.filter((it) => !TOOLBAR_CONTEXT[it.id] || TOOLBAR_CONTEXT[it.id].includes(ctx)))
    .filter((g) => g.length > 0);
}

export function Toolbar({ groups, left }: { groups: ToolbarGroup[]; left?: React.ReactNode }) {
  return (
    <header className="toolbar" role="toolbar" aria-label="Main toolbar">
      {left}
      {groups.map((g, i) => (
        <div key={i} className="tb-group" role="group">
          {g.map((it) => (
            <button
              key={it.id}
              type="button"
              aria-label={it.label}
              aria-pressed={it.pressed}
              aria-disabled={it.disabled ? true : undefined}
              disabled={!!it.disabled}
              title={it.disabled ? `${it.label}: ${it.disabled}` : `${it.title ?? it.label}${it.shortcut ? ` (${formatShortcut(it.shortcut)})` : ""}`}
              onClick={it.run}
            >
              {it.label}
            </button>
          ))}
        </div>
      ))}
    </header>
  );
}
