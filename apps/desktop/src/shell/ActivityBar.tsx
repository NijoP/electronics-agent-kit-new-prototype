// Left activity bar. Two kinds of entry:
//  * workspaces (Schematic, PCB, and anything registered in registry.ts such as a 3D viewer or the
//    terminal): each fills the editor area on its own, one at a time;
//  * side views (Explorer, Library / Parts, ...): each switches the side panel, and pressing the
//    active one collapses it.
import { tip } from "../design/shortcuts";
import { Icons } from "./icons";
import type { WorkspaceEntry } from "./registry";

export type SideView = "project" | "parts" | "layers" | "domain" | "manufacturing";

export const SIDE_VIEWS: { id: SideView; label: string }[] = [
  { id: "project", label: "Explorer" },
  { id: "parts", label: "Library / Parts" },
  { id: "layers", label: "PCB Layers" },
  { id: "domain", label: "Domain" },
  { id: "manufacturing", label: "Manufacturing" },
];

/** Built-in workspaces that open an editor document. */
export const EDITOR_WORKSPACES = [
  { id: "schematic", label: "Schematic", shortcut: "Mod+1" },
  { id: "pcb", label: "PCB", shortcut: "Mod+2" },
] as const;

interface Props {
  sideView: SideView;
  sideOpen: boolean;
  onSide: (v: SideView) => void;
  /** Which workspace is shown: "schematic", "pcb", a registered id, or "" (e.g. a file viewer). */
  workspace: string;
  onWorkspace: (id: string) => void;
  registered: WorkspaceEntry[];
  onSettings: () => void;
  theme: "dark" | "light";
  onTheme: () => void;
}

export function ActivityBar({ sideView, sideOpen, onSide, workspace, onWorkspace, registered, onSettings, theme, onTheme }: Props) {
  const side = (id: SideView) => {
    const v = SIDE_VIEWS.find((x) => x.id === id)!;
    const on = sideOpen && sideView === id;
    return (
      <button key={id} type="button" className={on ? "ab on" : "ab"} aria-label={v.label} aria-pressed={on} title={tip(v.label, id === "project" ? "Mod+Shift+E" : "Mod+B")} onClick={() => onSide(id)}>
        {Icons[id === "parts" ? "parts" : id]}
      </button>
    );
  };
  const ws = (id: string, label: string, icon: React.ReactNode, shortcut?: string) => (
    <button key={id} type="button" className={workspace === id ? "ab ws on" : "ab ws"} aria-label={label} aria-pressed={workspace === id} title={tip(`${label} workspace`, shortcut)} onClick={() => onWorkspace(id)}>
      {icon}
    </button>
  );
  return (
    <nav className="activity-bar" aria-label="Activity bar">
      {side("project")}
      {EDITOR_WORKSPACES.map((w) => ws(w.id, w.label, Icons[w.id], w.shortcut))}
      {registered.map((w) => ws(w.id, w.label, w.icon, w.shortcut))}
      <span className="ab-sep" role="separator" />
      {SIDE_VIEWS.filter((v) => v.id !== "project").map((v) => side(v.id))}
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
