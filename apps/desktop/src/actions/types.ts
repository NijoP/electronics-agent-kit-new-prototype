// Action registry types (frontend only, no shared engine contract). One definition per
// operation; the menu bar, the toolbar, the command palette and the key handler are generated
// from it, so a tool is reachable the same way from every route.
import type { IconName } from "../design/icons/icons";
import type { ToolbarGroupId } from "../shell/Toolbar";

/**
 * What the editor area shows, which decides the chrome:
 *  * "home": the project workspace with no document open;
 *  * "schematic", "pcb": that editor is the active document;
 *  * "3d": a registered EDA view (today the 3D board viewer) is the active document tab;
 *  * "file": a project file is open in the file viewer (common actions only).
 */
export type DocContext = "home" | "schematic" | "pcb" | "3d" | "file";
export const DOC_CONTEXTS: readonly DocContext[] = ["home", "schematic", "pcb", "3d", "file"];

/** Top-level menus, in menu-bar order. A menu with no visible action is not shown. */
export const MENU_ORDER = ["File", "Edit", "View", "Place", "Route", "Design", "Simulation", "Manufacturing", "Project", "Tools", "Settings", "Help"] as const;
export type MenuName = (typeof MENU_ORDER)[number];

/** What a right-click on the canvas hit (slice B3): a schematic object kind, or the empty sheet. */
export type CanvasTarget = "symbol" | "wire" | "label" | "sheet";

/** Toolbar split-button families. */
export type SplitId = "sch-marks" | "pcb-regions" | "pcb-checks";

export interface Action {
  /** Stable id; also the key of the default key map (`design/shortcuts.ts`). */
  id: string;
  /** Menu and palette text. */
  label: string;
  /** Shorter toolbar text (defaults to `label`). */
  short?: string;
  /** Toolbar tooltip (defaults to the label). */
  title?: string;
  /** Contexts the action exists in; absent means every context. An action outside them is absent, not disabled. */
  docs?: readonly DocContext[];
  /** Menu that lists the action, and whether a divider goes above it. */
  menu?: MenuName;
  sep?: boolean;
  /** Toolbar group that shows the action as a button. Every toolbar action also has a menu. */
  toolbar?: ToolbarGroupId;
  /** Toolbar icon (EAK icon set, `design/icons/`); every toolbar action has one. */
  icon?: IconName;
  /**
   * Tool family: toolbar actions of one family share a split button whose main part runs the
   * last member picked from its dropdown (see `SPLITS` in `shell/Toolbar.tsx`).
   */
  split?: SplitId;
  /** Listed in the command palette (default true). */
  palette?: boolean;
  paletteGroup?: string;
  /** Key specs; filled from the default key map when absent. */
  keys?: readonly string[];
  /** true when it can run now, otherwise the reason it cannot (shown beside the disabled entry). */
  enabled: () => true | string;
  run: () => void;
  checked?: boolean;
  /** Asks for confirmation before running (ADR-0004). */
  destructive?: boolean;
  /** The key also works while a text field has focus (default false; e.g. Save, palette). */
  whileTyping?: boolean;
  /**
   * A key the active editor handles itself (it owns the canvas focus), shown beside the entry only;
   * the registry key handler never runs it, so the key cannot fire twice.
   */
  hint?: string;
  /** Canvas context-menu entry for these right-click targets (slice B3; EAK's version of EasyEDA `s20`). */
  context?: readonly CanvasTarget[];
  /** Divider above the entry in the canvas context menu. */
  contextSep?: boolean;
}

/** Context of an editor-area document kind ("schematic", "pcb", "file"; a view tab is "3d"; nothing open is "home"). */
export const docContext = (kind: string | undefined): DocContext => (kind === "schematic" || kind === "pcb" || kind === "file" ? kind : kind === "view" ? "3d" : "home");
