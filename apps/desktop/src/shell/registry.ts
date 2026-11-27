// Workspace registry: how a feature adds a whole workspace without editing the shell. Two places:
//  * placement "app" (default): a top-level area of the navigation rail, after the EDA areas, such
//    as the Terminal Workspace. It fills the main area beside the rail and stays mounted (hidden)
//    while another workspace is shown, so switching never kills its processes;
//  * placement "eda": a document of the EDA workspace next to Schematic and PCB, such as the 3D
//    board viewer: a navigation-rail entry that opens it as a document tab (slice C6).
// A module in `src/workspaces/` named `*.workspace.ts(x)` is imported at start-up (workspaces.ts)
// and calls `registerWorkspace`. Nothing is registered by default: an entry exists only when a
// module that opens something real registers it, so there are no placeholder controls.
import { useSyncExternalStore, type ReactNode } from "react";
import type { Command, CopperLayer, OpenProject } from "../types";

/** What the shell tells a workspace. */
export interface WorkspaceContext {
  /** Active engine project, or null when none is open (an app workspace must cope with that). */
  projectId: string | null;
  projectName: string | null;
  /** Active project folder on disk. A terminal space must copy this at creation, not follow it. */
  projectDir: string | null;
  revision: number | null;
  /** Every project open in the window, with its folder. */
  openProjects: OpenProject[];
  /** Opens Model settings (providers and models), the dialog the rail's Agents item used to open. */
  openModelSettings?: () => void;
  /** Shows another top-level workspace by id (e.g. "terminal"). */
  showWorkspace?: (id: string) => void;
  /** True while this workspace is the one shown (it stays mounted, hidden, when another is). */
  active: boolean;
  // Placement "eda" only (a document of the EDA workspace, such as the 3D view):
  /** Engine symbol id selected in the editors (cross-probe in). */
  selected?: string | null;
  /** Selects an engine symbol id in the editors (cross-probe out). */
  select?: (symbolId: string) => void;
  /** Opens the PCB or the schematic on the current selection. */
  probeTo?: (kind: "pcb" | "schematic") => void;
  /** Runs a typed engine command the way the editors do (same busy state, same log). */
  execute?: (c: Command) => Promise<void>;
  busy?: boolean;
  /** Display colour of a copper layer in the PCB layer manager (user override or default). */
  layerColour?: (l: CopperLayer) => string;
  /** Copper layer keys hidden in the PCB layer manager, and its toggle (one visibility store). */
  layerHidden?: ReadonlySet<string>;
  toggleLayer?: (l: CopperLayer) => void;
}

export interface WorkspaceEntry {
  /** Stable id, e.g. "terminal" or "viewer3d". Must not collide with built-in ids. */
  id: string;
  label: string;
  /** Shorter name for the navigation rail (e.g. "Terminal" for "Terminal Workspace"); `label` when absent. */
  navLabel?: string;
  placement?: "app" | "eda";
  /** 20 px icon for the navigation rail button, drawn with `currentColor`. */
  icon?: ReactNode;
  /** Position among registered entries (lower first). */
  order?: number;
  /** Optional shortcut spec such as "Mod+Shift+T"; shown in the tooltip and palette. */
  shortcut?: string;
  /**
   * placement "app": the app bar's "+" runs this (create a new terminal space). The "+" exists only
   * while some registered workspace provides it. It should make the workspace ready; the shell
   * switches to the workspace afterwards.
   */
  onNew?: () => void;
  newLabel?: string;
  /** The workspace body, mounted the first time it is opened and then kept mounted. */
  render: (ctx: WorkspaceContext) => ReactNode;
}

const workspaces = new Map<string, WorkspaceEntry>();
const listeners = new Set<() => void>();
let version = 0;

const emit = () => {
  version += 1;
  listeners.forEach((l) => l());
};

/** Built-in workspace ids a registration may not take. */
export const RESERVED_IDS = ["editor", "schematic", "pcb", "explorer"] as const;

/** Add (or replace) a workspace. Returns the function that removes it again. */
export function registerWorkspace(entry: WorkspaceEntry): () => void {
  if ((RESERVED_IDS as readonly string[]).includes(entry.id)) throw new Error(`workspace id "${entry.id}" is reserved`);
  workspaces.set(entry.id, entry);
  emit();
  return () => {
    if (workspaces.get(entry.id) === entry) {
      workspaces.delete(entry.id);
      emit();
    }
  };
}

const byOrder = <T extends { order?: number; label: string }>(a: T, b: T) => (a.order ?? 100) - (b.order ?? 100) || a.label.localeCompare(b.label);

export const listWorkspaces = (placement: "app" | "eda"): WorkspaceEntry[] =>
  [...workspaces.values()].filter((w) => (w.placement ?? "app") === placement).sort(byOrder);

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** Re-renders when anything is (un)registered. */
export function useRegistryVersion(): number {
  return useSyncExternalStore(subscribe, () => version);
}
