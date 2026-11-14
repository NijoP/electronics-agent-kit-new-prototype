// One registry of keyboard shortcuts, shown in tooltips and in the command palette.
// "Mod" is Cmd on macOS and Ctrl elsewhere.
export const isMac = () => typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent);

export const SHORTCUTS = {
  palette: "Mod+K",
  paletteAlt: "Mod+Shift+P",
  quickOpen: "Mod+P",
  toggleSide: "Mod+B",
  toggleBottom: "Mod+J",
  toggleAi: "Mod+L",
  undo: "Mod+Z",
  redo: "Mod+Shift+Z",
  save: "Mod+S",
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
