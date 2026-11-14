// Workspace registry: how a feature adds a whole workspace without editing the shell. Two places:
//  * placement "app" (default): a top-level environment next to "Project / EDA" in the app bar,
//    such as the Terminal Workspace. It fills the whole main area and stays mounted (hidden) while
//    another workspace is shown, so switching never kills its processes;
//  * placement "eda": an independent view inside the EDA workspace next to Schematic and PCB
//    (an activity-bar entry), such as the 3D board viewer.
// A module in `src/workspaces/` named `*.workspace.ts(x)` is imported at start-up (workspaces.ts)
// and calls `registerWorkspace`. Nothing is registered by default: an entry exists only when a
// module that opens something real registers it, so there are no placeholder controls.
import { useSyncExternalStore, type ReactNode } from "react";
import type { OpenProject } from "../types";

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
  /** True while this workspace is the one shown (it stays mounted, hidden, when another is). */
  active: boolean;
}

export interface WorkspaceEntry {
  /** Stable id, e.g. "terminal" or "viewer3d". Must not collide with built-in ids. */
  id: string;
  label: string;
  placement?: "app" | "eda";
  /** 20 px icon (placement "eda" only: it is the activity-bar button), drawn with `currentColor`. */
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
