// Slim left activity bar. Each button switches the side panel (or opens Settings); pressing
// the active one collapses the panel.
import { tip } from "../design/shortcuts";
import { Icons } from "./icons";

export type SideView = "project" | "parts" | "layers" | "domain" | "manufacturing";

export const SIDE_VIEWS: { id: SideView; label: string }[] = [
  { id: "project", label: "Project" },
  { id: "parts", label: "Library / Parts" },
  { id: "layers", label: "PCB Layers" },
  { id: "domain", label: "Domain" },
  { id: "manufacturing", label: "Manufacturing" },
];

interface Props {
  view: SideView;
  open: boolean;
  onSelect: (v: SideView) => void;
  onSettings: () => void;
  theme: "dark" | "light";
  onTheme: () => void;
}

export function ActivityBar({ view, open, onSelect, onSettings, theme, onTheme }: Props) {
  return (
    <nav className="activity-bar" aria-label="Activity bar">
      {SIDE_VIEWS.map((v) => {
        const on = open && view === v.id;
        return (
          <button
            key={v.id}
            type="button"
            className={on ? "ab on" : "ab"}
            aria-label={v.label}
            aria-pressed={on}
            title={tip(v.label, "Mod+B")}
            onClick={() => onSelect(v.id)}
          >
            {Icons[v.id === "parts" ? "parts" : v.id]}
          </button>
        );
      })}
      <span className="spacer" />
      <button type="button" className="ab" aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"} title="Toggle light/dark theme" onClick={onTheme}>
        {theme === "dark" ? Icons.sun : Icons.moon}
      </button>
      <button type="button" className="ab" aria-label="Model settings" title="Model settings" onClick={onSettings}>
        {Icons.settings}
      </button>
    </nav>
  );
}
