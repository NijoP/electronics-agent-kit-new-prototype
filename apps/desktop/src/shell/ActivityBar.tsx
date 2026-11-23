// Left activity bar. Two kinds of entry:
//  * workspaces (Schematic, PCB, and anything registered in registry.ts such as a 3D viewer or the
//    terminal): each fills the editor area on its own, one at a time;
//  * side views (Overview, Explorer, Design manager, Domain, Manufacturing, Assets): each switches the left
//    dock, and pressing the active one collapses it. Editor panels live in the docks (Phase 1 A3):
//    Layers in the right dock, the library in the bottom dock.
import { tip } from "../design/shortcuts";
import { effectiveKeys, useKeymap } from "../settings/keymap";
import { Icons } from "./icons";
import type { WorkspaceEntry } from "./registry";

export type SideView = "overview" | "project" | "design" | "domain" | "manufacturing" | "assets";

export const SIDE_VIEWS: { id: SideView; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "project", label: "Explorer" },
  { id: "design", label: "Design manager" },
  { id: "domain", label: "Domain" },
  { id: "manufacturing", label: "Manufacturing" },
  { id: "assets", label: "Assets" },
];

/** Built-in workspaces that open an editor document, with the registry action whose key opens each. */
export const EDITOR_WORKSPACES = [
  { id: "schematic", label: "Schematic", action: "ws-schematic" },
  { id: "pcb", label: "PCB", action: "ws-pcb" },
] as const;

/** Registry action whose key opens each side view (none for the others). */
const SIDE_ACTIONS: Partial<Record<SideView, string>> = { project: "show-project", design: "show-design" };

/** First effective key of a registry action (the user's rebinding, else the preset's). */
const keyOf = (id: string | undefined) => (id ? effectiveKeys({ id })[0] : undefined);

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
  useKeymap(); // tooltips follow a rebinding
  const side = (id: SideView) => {
    const v = SIDE_VIEWS.find((x) => x.id === id)!;
    const on = sideOpen && sideView === id;
    return (
      <button key={id} type="button" className={on ? "ab on" : "ab"} aria-label={v.label} aria-pressed={on} title={tip(on ? `${v.label} (press again to collapse the side panel)` : v.label, keyOf(SIDE_ACTIONS[id]))} onClick={() => onSide(id)}>
        {Icons[id]}
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
      {side("overview")}
      {side("project")}
      {EDITOR_WORKSPACES.map((w) => ws(w.id, w.label, Icons[w.id], keyOf(w.action)))}
      {registered.map((w) => ws(w.id, w.label, w.icon, w.shortcut))}
      <span className="ab-sep" role="separator" />
      {SIDE_VIEWS.filter((v) => v.id !== "project" && v.id !== "overview").map((v) => side(v.id))}
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
