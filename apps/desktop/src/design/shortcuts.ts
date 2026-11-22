// One registry of keyboard shortcuts, shown in tooltips and in the command palette.
// "Mod" is Cmd on macOS and Ctrl elsewhere.
export const isMac = () => typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent);

export const SHORTCUTS = {
  palette: "Mod+K",
  paletteAlt: "Mod+Shift+P",
  quickOpen: "Mod+P",
  toggleSide: "Mod+B",
  toggleBottom: "Mod+J",
  showSchematic: "Mod+1",
  showPcb: "Mod+2",
  showExplorer: "Mod+Shift+E",
  newTerminalSpace: "Mod+Shift+T",
  undo: "Mod+Z",
  redo: "Mod+Shift+Z",
  save: "Mod+S",
} as const;

/** Project Home only (not listed in the workspace shortcut help, where they do nothing). */
export const HOME_SHORTCUTS = {
  openProject: "Mod+O",
  newProject: "Mod+N",
  find: "Mod+F",
} as const;

/** Human text for a shortcut spec such as "Mod+Shift+P". */
export function formatShortcut(spec: string, mac = isMac()): string {
  return spec
    .split("+")
    .map((k) => (k === "Mod" ? (mac ? "⌘" : "Ctrl") : k === "Shift" ? (mac ? "⇧" : "Shift") : k === "Alt" ? (mac ? "⌥" : "Alt") : k))
    .join(mac ? "" : "+");
}

/** Tooltip text: "Undo (Ctrl+Z)". */
export const tip = (label: string, spec?: string) => (spec ? `${label} (${formatShortcut(spec)})` : label);

/** True when the keyboard event matches the spec. */
export function matches(e: KeyboardEvent, spec: string): boolean {
  const parts = spec.split("+");
  const key = parts[parts.length - 1].toLowerCase();
  const wantMod = parts.includes("Mod");
  const wantShift = parts.includes("Shift");
  const wantAlt = parts.includes("Alt");
  return (
    (e.ctrlKey || e.metaKey) === wantMod && e.shiftKey === wantShift && e.altKey === wantAlt && e.key.toLowerCase() === key
  );
}

/**
 * Default key map of the action registry (`src/actions/`): action id → key specs. The registry
 * shows the first spec beside the action in menus, toolbar tooltips and the palette, and the
 * workspace's key handler runs the action for any of them. Editor tool keys (1-7, R, F) stay
 * with the editors, which own the canvas focus.
 */
export const DEFAULT_KEYMAP: Readonly<Record<string, readonly string[]>> = {
  palette: [SHORTCUTS.palette, SHORTCUTS.paletteAlt],
  "quick-open": [SHORTCUTS.quickOpen],
  "toggle-side": [SHORTCUTS.toggleSide],
  "toggle-bottom": [SHORTCUTS.toggleBottom],
  "ws-schematic": [SHORTCUTS.showSchematic],
  "ws-pcb": [SHORTCUTS.showPcb],
  "show-project": [SHORTCUTS.showExplorer],
  save: [SHORTCUTS.save],
  undo: [SHORTCUTS.undo],
  redo: [SHORTCUTS.redo, "Mod+Y"],
};
