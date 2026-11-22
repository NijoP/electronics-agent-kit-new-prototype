// The action registry: turns one list of actions into the menu bar, the toolbar, the command
// palette and the key handler for the active document. Nothing here knows any operation; the
// per-document sets (`sets.ts`) define them.
import type { PaletteAction } from "../CommandPalette";
import { matches } from "../design/shortcuts";
import { effectiveKeys } from "../settings/keymap";
import type { Menu } from "../shell/MenuBar";
import type { ToolbarGroup, ToolbarGroupId } from "../shell/Toolbar";
import { MENU_ORDER, type Action, type DocContext } from "./types";

/** Toolbar groups, left to right. */
export const TOOLBAR_ORDER: readonly ToolbarGroupId[] = ["file", "edit", "view", "checks", "mfg", "app"];

/** Key specs of an action: the user's rebinding (Settings > Keyboard), else the chosen preset's (its own keys, else the default key map). */
export const keysOf = (a: Action): readonly string[] => effectiveKeys(a);

/** Actions that exist in this context (an action that does not apply is absent, not disabled). */
export const visibleIn = (actions: readonly Action[], ctx: DocContext): Action[] => actions.filter((a) => !a.docs || a.docs.includes(ctx));

const reason = (a: Action): string | undefined => {
  const e = a.enabled();
  return e === true ? undefined : e;
};

export function menusFor(actions: readonly Action[], ctx: DocContext): Menu[] {
  const shown = visibleIn(actions, ctx);
  return MENU_ORDER.map((name) => ({
    label: name,
    items: shown
      .filter((a) => a.menu === name)
      .map((a) => ({ label: a.label, run: a.run, shortcut: keysOf(a)[0], disabled: reason(a), checked: a.checked, destructive: a.destructive, sep: a.sep })),
  })).filter((m) => m.items.length > 0);
}

export function toolbarFor(actions: readonly Action[], ctx: DocContext): ToolbarGroup[] {
  const shown = visibleIn(actions, ctx);
  return TOOLBAR_ORDER.map((g) =>
    shown
      .filter((a) => a.toolbar === g)
      .map((a) => ({ id: a.id, group: g, label: a.short ?? a.label, title: a.title, icon: a.icon, split: a.split, pressed: a.checked, run: a.run, shortcut: keysOf(a)[0], disabled: reason(a) })),
  ).filter((g) => g.length > 0);
}

export function paletteFor(actions: readonly Action[], ctx: DocContext): PaletteAction[] {
  return visibleIn(actions, ctx)
    .filter((a) => a.palette !== false)
    .map((a) => ({ id: a.id, label: a.label, run: a.run, disabled: a.enabled() !== true, destructive: a.destructive, shortcut: keysOf(a)[0], group: a.paletteGroup ?? a.menu }));
}

/** The visible action bound to this key event, if any (typing: focus is in a text field). */
export function actionForKey(actions: readonly Action[], ctx: DocContext, e: KeyboardEvent, typing: boolean): Action | undefined {
  return visibleIn(actions, ctx).find((a) => (!typing || a.whileTyping) && keysOf(a).some((k) => matches(e, k)));
}

/** Runs an action the way the menu does: only when enabled, confirming destructive ones. */
export function runAction(a: Action, confirm: (m: string) => boolean = (m) => window.confirm(m)): boolean {
  if (a.enabled() !== true) return false;
  if (a.destructive && !confirm(`${a.label}: this is a destructive change. Continue?`)) return false;
  a.run();
  return true;
}
