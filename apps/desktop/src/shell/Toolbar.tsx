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
