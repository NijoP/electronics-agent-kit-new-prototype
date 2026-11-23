// Slice D4: settings dialog, hotkey table from the A1 registry, presets, conflicts, canvas colours
// and grid defaults. Acceptance: a rebinding persists and shows in tooltips and the palette; a
// conflict is refused with a reason.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import { GRID_KINDS, getGrid, updateGrid } from "../grid";
import { SIDE_VIEWS } from "../shell/ActivityBar";
import { actionForKey, keysOf, menusFor, paletteFor, toolbarFor } from "../actions/registry";
import { workspaceActions, type ActionDeps } from "../actions/sets";
import type { Action } from "../actions/types";
import { applyColours, PAPER, resetColourStore, setColour, setColourPreset } from "./canvasColours";
import { bind, conflictFor, conflicts, EASYEDA_STYLE, EDITOR_RESERVED, getKeymap, normalize, parseKeymap, resetKeymapStore, setPreset, specOf } from "./keymap";
import { SettingsDialog } from "./SettingsDialog";

function deps(): ActionDeps {
  const base: Partial<ActionDeps> = {
    snapshot: fixture(3),
    busy: false,
    selected: null,
    theme: "dark",
    recent: [],
    library: [],
    docs: [],
    openIds: [],
    projects: [],
    sideViews: SIDE_VIEWS,
    edaViews: [{ id: "viewer3d", label: "3D view", shortcut: "Mod+3" }],
    sideView: "project",
    grid: { kind: "schematic", cfg: GRID_KINDS.schematic.defaults, set: vi.fn() },
    toolbarLabels: false,
  };
  return new Proxy(base, { get: (t, k: string) => (k in t ? t[k as keyof ActionDeps] : vi.fn()) }) as ActionDeps;
}
const all = () => workspaceActions(deps());
const byId = (id: string) => all().find((a) => a.id === id) as Action;
const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

beforeEach(() => {
  resetKeymapStore();
  resetColourStore();
});
afterEach(cleanup);

describe("key map", () => {
  it("normalizes specs and key events to one canonical form", () => {
    expect(normalize("shift+mod+k")).toBe("Mod+Shift+K");
    expect(specOf({ key: "k", ctrlKey: true, metaKey: false, shiftKey: true, altKey: false })).toBe("Mod+Shift+K");
    expect(specOf({ key: "Shift", ctrlKey: false, metaKey: false, shiftKey: true, altKey: false })).toBeNull();
    expect(parseKeymap("{bad")).toEqual({ preset: "eak", user: {} });
    expect(parseKeymap('{"preset":"nope","user":{"save":["Mod+S"],"x":3}}')).toEqual({ preset: "eak", user: { save: ["Mod+S"] } });
  });

  it("a rebinding persists and shows in menus, toolbar tooltips, the palette and the key handler", () => {
    expect(bind(all(), byId("undo"), "Mod+Alt+U")).toBeNull();
    expect(JSON.parse(localStorage.getItem("eak.keymap")!).user.undo).toEqual(["Mod+Alt+U"]);
    expect(keysOf(byId("undo"))).toEqual(["Mod+Alt+U"]);
    const edit = menusFor(all(), "schematic").find((m) => m.label === "Edit")!;
    expect(edit.items.find((i) => i.label === "Undo")?.shortcut).toBe("Mod+Alt+U");
    expect(toolbarFor(all(), "schematic").flat().find((b) => b.id === "undo")?.shortcut).toBe("Mod+Alt+U");
    expect(paletteFor(all(), "schematic").find((p) => p.id === "undo")?.shortcut).toBe("Mod+Alt+U");
    expect(actionForKey(all(), "schematic", key({ key: "u", ctrlKey: true, altKey: true }), false)?.id).toBe("undo");
    expect(actionForKey(all(), "schematic", key({ key: "z", ctrlKey: true }), false)?.id).not.toBe("undo");
  });

  it("refuses a conflicting key with a reason and keeps the old binding", () => {
    expect(bind(all(), byId("undo"), "Mod+S")).toMatch(/Mod\+S is already bound to "Save/);
    expect(keysOf(byId("undo"))).toEqual(["Mod+Z"]);
    expect(conflictFor(all(), byId("t-wire"), "W")).toMatch(/schematic editor/);
    expect(conflictFor(all(), byId("undo"), "Escape")).toMatch(/cannot be rebound/);
    // an action that exists only in the PCB editor may use a key only the schematic editor owns
    expect(conflictFor(all(), byId("t-via"), "X")).toBeNull();
  });

  it("both presets are free of conflicts and of editor-owned keys; the EasyEDA-style map only names real actions", () => {
    const ids = new Set(all().map((a) => a.id));
    for (const id of Object.keys(EASYEDA_STYLE)) expect(ids.has(id), id).toBe(true);
    for (const p of ["eak", "easyeda-style"] as const) {
      setPreset(p);
      expect(conflicts(all()), p).toEqual([]);
      for (const a of all())
        for (const k of keysOf(a)) {
          const owned = EDITOR_RESERVED[normalize(k)];
          expect(Boolean(owned && (a.docs ?? ["schematic", "pcb"]).some((c) => owned.includes(c))), `${p} ${a.id} ${k}`).toBe(false);
        }
    }
    expect(keysOf(byId("t-wire"))).toEqual(["Alt+W"]);
    expect(keysOf(byId("save"))).toEqual(["Mod+S"]);
    expect(getKeymap().preset).toBe("easyeda-style");
  });
});

describe("canvas colours", () => {
  it("the paper preset and a user colour override the tokens on the document; reset restores the theme", () => {
    const root = document.documentElement;
    setColourPreset("paper");
    expect(root.style.getPropertyValue("--c-sheet")).toBe(PAPER["--c-sheet"]);
    setColour("--c-wire", "#123456");
    expect(root.style.getPropertyValue("--c-wire")).toBe("#123456");
    setColour("--c-wire", "red"); // not a hex colour: ignored
    expect(root.style.getPropertyValue("--c-wire")).toBe("#123456");
    resetColourStore();
    applyColours();
    expect(root.style.getPropertyValue("--c-sheet")).toBe("");
  });
});

describe("settings dialog", () => {
  const open = () => render(<SettingsDialog actions={all()} theme="dark" onTheme={vi.fn()} onModel={vi.fn()} onClose={vi.fn()} />);

  it("the tree search filters pages", () => {
    open();
    const tree = screen.getByRole("tree", { name: "Settings tree" });
    fireEvent.change(screen.getByRole("searchbox", { name: "Search settings" }), { target: { value: "paper" } });
    expect(within(tree).getByRole("button", { name: "Canvas colours" })).toBeTruthy();
    expect(within(tree).queryByRole("button", { name: "Hotkeys" })).toBeNull();
    expect(screen.getByRole("table", { name: "Canvas colours" })).toBeTruthy();
  });

  it("rebinds by key press, refuses a conflict with the reason, and searches by key and by command", () => {
    open();
    const table = screen.getByRole("table", { name: "Hotkeys" });
    fireEvent.click(within(table).getByRole("button", { name: "Change keys of Undo" }));
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    expect(screen.getByRole("alert").textContent).toMatch(/was not bound to "Undo": Mod\+S is already bound to "Save/);
    fireEvent.click(within(table).getByRole("button", { name: "Change keys of Undo" }));
    fireEvent.keyDown(window, { key: "u", ctrlKey: true, altKey: true });
    expect(screen.getByRole("status").textContent).toMatch(/"Undo" is now/);
    expect(keysOf(byId("undo"))).toEqual(["Mod+Alt+U"]);
    fireEvent.click(screen.getByRole("button", { name: "Find by key press" }));
    fireEvent.keyDown(window, { key: "u", ctrlKey: true, altKey: true });
    expect(within(table).getAllByRole("row").slice(1).map((r) => r.getAttribute("data-action"))).toEqual(["undo"]);
    fireEvent.click(screen.getByRole("button", { name: /Clear the key filter/ }));
    fireEvent.change(screen.getByRole("searchbox", { name: "Search hotkeys by command or key" }), { target: { value: "auto-route" } });
    expect(within(table).getAllByRole("row").slice(1).map((r) => r.getAttribute("data-action"))).toEqual(["route"]);
  });

  it("the preset select switches the key map", () => {
    open();
    fireEvent.change(screen.getByRole("combobox", { name: "Key map preset" }), { target: { value: "easyeda-style" } });
    expect(getKeymap().preset).toBe("easyeda-style");
  });

  it("grid defaults per editor write that editor's grid store", () => {
    updateGrid("pcb", GRID_KINDS.pcb.defaults);
    updateGrid("schematic", GRID_KINDS.schematic.defaults);
    open();
    fireEvent.click(screen.getByRole("button", { name: "PCB grid" }));
    fireEvent.change(screen.getByRole("combobox", { name: "Unit" }), { target: { value: "mil" } });
    expect(getGrid("pcb").unit).toBe("mil");
    expect(getGrid("schematic").unit).toBe(GRID_KINDS.schematic.defaults.unit);
    fireEvent.click(screen.getByRole("button", { name: "Restore EAK defaults" }));
    expect(getGrid("pcb")).toEqual(GRID_KINDS.pcb.defaults);
  });
});
