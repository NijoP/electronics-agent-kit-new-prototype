// Left navigation rail (product vision §1: a 56 px rail on the deepest surface). One component,
// two hosts: the EDA workspace renders it with its live dock and document state (ActivityBar.tsx),
// and the app shell renders it beside Home and the Terminal workspace (App.tsx), so the rail is
// in the same place in every area. It only presents items; each item's `run` is the existing
// operation it stands for (a document, a dock tab, a side view, a dialog or a top-level
// workspace). An item that cannot act right now is disabled and says why; nothing is decorative.
import type { ReactNode } from "react";
import { tip } from "../design/shortcuts";

export interface NavItem {
  id: string;
  /** Accessible name and tooltip label. */
  label: string;
  icon: ReactNode;
  /** The area or panel this item opens is the one shown (aria-pressed, accent mark). */
  current?: boolean;
  /** Longer tooltip text; the label is used when absent. */
  title?: string;
  /** Key spec such as "Mod+Shift+T", shown in the tooltip. */
  shortcut?: string;
  run: () => void;
  /** Why the item cannot act right now (rendered disabled with this reason). */
  unavailable?: string;
}

interface Props {
  /** Where the user works: Home, Schematic, PCB, 3D, Library, Agents, Checks, Manufacturing, Terminal. */
  areas: NavItem[];
  /** Project panels shown in the left dock (Explorer, Overview, Design manager, ...). */
  panels?: NavItem[];
  /** Theme and settings, at the foot of the rail. */
  footer?: NavItem[];
}

function RailButton({ item }: { item: NavItem }) {
  const title = item.unavailable ? `${item.label}: ${item.unavailable}` : tip(item.title ?? item.label, item.shortcut);
  return (
    <button
      type="button"
      className={item.current ? "ab on" : "ab"}
      data-nav={item.id}
      aria-label={item.label}
      aria-pressed={item.current ?? false}
      title={title}
      disabled={!!item.unavailable}
      onClick={item.run}
    >
      {item.icon}
    </button>
  );
}

export function NavRail({ areas, panels = [], footer = [] }: Props) {
  return (
    <nav className="activity-bar nav-rail" aria-label="Activity bar">
      <div className="rail-group" role="group" aria-label="Areas">
        {areas.map((i) => (
          <RailButton key={i.id} item={i} />
        ))}
      </div>
      {panels.length > 0 && (
        <>
          <span className="ab-sep" role="separator" />
          <div className="rail-group" role="group" aria-label="Project panels">
            {panels.map((i) => (
              <RailButton key={i.id} item={i} />
            ))}
          </div>
        </>
      )}
      <span className="spacer" />
      {footer.map((i) => (
        <RailButton key={i.id} item={i} />
      ))}
    </nav>
  );
}
