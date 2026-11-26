// The EDA workspace's navigation rail (product vision §1), drawn by NavRail.tsx. Three groups:
//  * areas, in the vision's order: Home · Schematic · PCB · 3D · Library · Agents · Checks ·
//    Manufacturing · Terminal. Each runs the existing operation it stands for: Schematic and PCB
//    open that editor document, 3D (and any view registered in registry.ts) opens its document
//    tab, Library and Checks show the bottom dock's Library and Findings tabs, Manufacturing shows
//    the manufacturing side view, Agents opens Model settings (providers and models; agent
//    sessions run in the Terminal), and Home / Terminal switch the app shell to that area;
//  * project panels (Explorer, Overview, Design manager, Domain, Assets): each switches the left
//    dock, and pressing the active one collapses it. Library, Checks and Manufacturing toggle the
//    same way;
//  * theme and settings, at the foot.
// Editor panels live in the docks (Phase 1 A3): Layers in the right dock, the library in the bottom dock.
import type { ReactNode } from "react";
import { effectiveKeys, useKeymap } from "../settings/keymap";
import { Icons } from "./icons";
import { NavRail, type NavItem } from "./NavRail";
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

/** Bottom-dock tabs the rail opens as areas. */
export type RailDockTab = "library" | "findings";

/** A top-level area outside the EDA workspace that the app shell can show (Home, Terminal). */
export interface ShellArea {
  id: string;
  label: string;
  icon: ReactNode;
  title?: string;
  shortcut?: string;
  run: () => void;
}

/** Tooltips shared with the app shell's own rail (App.tsx). */
export const HOME_TITLE = "Home: projects, new design, continue";
export const AGENTS_TITLE = "Agents: model providers and models (Model settings); agent sessions run in the Terminal";

/** Registry action whose key opens each side view (none for the others). */
const SIDE_ACTIONS: Partial<Record<SideView, string>> = { project: "show-project", design: "show-design" };

/** First effective key of a registry action (the user's rebinding, else the preset's). */
const keyOf = (id: string | undefined) => (id ? effectiveKeys({ id })[0] : undefined);

/** The side views listed as project panels (Manufacturing is an area of its own). */
const PANEL_VIEWS: SideView[] = ["project", "overview", "design", "domain", "assets"];

interface Props {
  sideView: SideView;
  sideOpen: boolean;
  onSide: (v: SideView) => void;
  /** Which workspace is shown: "schematic", "pcb", a registered id, or "" (e.g. a file viewer). */
  workspace: string;
  onWorkspace: (id: string) => void;
  registered: WorkspaceEntry[];
  /** The bottom dock's shown tab, or null while the bottom dock is closed. */
  bottomTab: string | null;
  /** Shows a bottom-dock tab, or collapses the dock when that tab is already shown. */
  onBottom: (tab: RailDockTab) => void;
  /** Opens Model settings (the Agents area). */
  onSettings: () => void;
  /** Opens Settings (hotkeys, canvas colours, grid defaults, theme). */
  onPreferences?: () => void;
  /** Home, given by the app shell (absent when the workspace is shown on its own). */
  home?: ShellArea;
  /** Top-level workspaces of the app shell, such as the Terminal. */
  apps?: ShellArea[];
  theme: "dark" | "light";
  onTheme: () => void;
}

export function ActivityBar({ sideView, sideOpen, onSide, workspace, onWorkspace, registered, bottomTab, onBottom, onSettings, onPreferences, home, apps = [], theme, onTheme }: Props) {
  useKeymap(); // tooltips follow a rebinding
  const side = (id: SideView, icon: ReactNode = Icons[id]): NavItem => {
    const v = SIDE_VIEWS.find((x) => x.id === id)!;
    const on = sideOpen && sideView === id;
    return { id, label: v.label, icon, current: on, title: on ? `${v.label} (press again to collapse the side panel)` : v.label, shortcut: keyOf(SIDE_ACTIONS[id]), run: () => onSide(id) };
  };
  const ws = (id: string, label: string, icon: ReactNode, shortcut?: string): NavItem => ({
    id,
    label,
    icon,
    current: workspace === id,
    title: `${label} workspace`,
    shortcut,
    run: () => onWorkspace(id),
  });
  const dock = (tab: RailDockTab, label: string, icon: ReactNode, what: string): NavItem => {
    const on = bottomTab === tab;
    return { id: tab === "findings" ? "checks" : tab, label, icon, current: on, title: on ? `${label}: ${what} (press again to collapse the bottom panel)` : `${label}: ${what}`, run: () => onBottom(tab) };
  };
  const shell = (a: ShellArea): NavItem => ({ id: a.id, label: a.label, icon: a.icon, title: a.title, shortcut: a.shortcut, run: a.run });
  const areas: NavItem[] = [
    ...(home ? [shell(home)] : []),
    ...EDITOR_WORKSPACES.map((w) => ws(w.id, w.label, Icons[w.id], keyOf(w.action))),
    ...registered.map((w) => ws(w.id, w.label, w.icon, w.shortcut)),
    dock("library", "Library", Icons.parts, "parts, symbols and footprints"),
    { id: "agents", label: "Agents", icon: Icons.sparkle, title: AGENTS_TITLE, run: onSettings },
    dock("findings", "Checks", Icons.checks, "ERC and DRC findings"),
    side("manufacturing"),
    ...apps.map(shell),
  ];
  const footer: NavItem[] = [
    { id: "theme", label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme", icon: theme === "dark" ? Icons.sun : Icons.moon, title: "Toggle light/dark theme", run: onTheme },
    ...(onPreferences ? [{ id: "preferences", label: "Preferences", icon: Icons.settings, title: "Settings: hotkeys, canvas colours, grid defaults and theme", run: onPreferences }] : []),
  ];
  return <NavRail areas={areas} panels={PANEL_VIEWS.map((v) => side(v))} footer={footer} />;
}
