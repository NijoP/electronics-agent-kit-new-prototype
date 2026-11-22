// Action registry (slice A1): one definition per operation drives menus, toolbar, palette and keys.
import { describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import { SIDE_VIEWS } from "../shell/ActivityBar";
import { actionForKey, menusFor, paletteFor, runAction, toolbarFor, visibleIn } from "./registry";
import { workspaceActions, type ActionDeps } from "./sets";
import { DOC_CONTEXTS, type DocContext } from "./types";

function deps(over: Partial<ActionDeps> = {}): ActionDeps {
  const snapshot = fixture(7);
  const fn = () => vi.fn();
  return {
    snapshot,
    busy: false,
    selected: null,
    selectedSymbol: undefined,
    fpOfSelected: undefined,
    theme: "dark",
    sideOpen: true,
    bottomOpen: true,
    split: false,
    recent: [{ path: "/o.eak", project_id: "prj_other", name: "Other", revision: 1, last_opened_ms: 1 }],
    library: [{ id: "res", name: "Resistor", category: "passive", refdes_prefix: "R", default_value: "10k", pins: [] }],
    docs: [{ id: "sch:sht_1", kind: "schematic", title: "Sheet 1" }, { id: "pcb:prj_1", kind: "pcb", title: "PCB" }],
    openIds: ["sch:sht_1"],
    projects: [],
    sideViews: SIDE_VIEWS,
    edaViews: [{ id: "viewer3d", label: "3D view", shortcut: "Mod+3" }],
    dialog: fn(), save: fn(), undo: fn(), redo: fn(), close: fn(), openRecent: fn(), execute: fn(), check: fn(), syncPcb: fn(),
    refillZones: fn(), rotateSel: fn(), deleteSel: fn(), tool: fn(), zoomBy: fn(), fit: fn(), select: fn(), toggleSplit: fn(),
    showSide: fn(), toggleSide: fn(), toggleBottom: fn(), showBottom: fn(), onTheme: fn(), resetLayout: fn(), onSettings: fn(),
    palette: fn(), quickOpen: fn(), help: fn(), notify: fn(), open: fn(), closeAll: fn(), showWorkspace: fn(), switchTo: fn(), placePart: fn(),
    ...over,
  };
}
const labels = (ctx: DocContext, menu: string, d = deps()) => menusFor(workspaceActions(d), ctx).find((m) => m.label === menu)?.items.map((i) => i.label) ?? null;

describe("action registry", () => {
  it("every action visible in a context is either enabled with run, or disabled with a reason", () => {
    for (const busy of [false, true])
      for (const ctx of DOC_CONTEXTS)
        for (const a of visibleIn(workspaceActions(deps({ busy })), ctx)) {
          expect(typeof a.run, a.id).toBe("function");
          const e = a.enabled();
          if (e !== true) expect(e.trim().length, `${a.id} in ${ctx}`).toBeGreaterThan(0);
        }
  });

  it("ids are unique and no action is reachable from the toolbar only", () => {
    const all = workspaceActions(deps());
    expect(new Set(all.map((a) => a.id)).size).toBe(all.length);
    for (const a of all) if (a.toolbar) expect(a.menu, a.id).toBeTruthy();
    // and what the toolbar renders is in that context's menus too
    for (const ctx of DOC_CONTEXTS) {
      const inMenus = new Set(menusFor(all, ctx).flatMap((m) => m.items.map((i) => i.label)));
      for (const b of toolbarFor(all, ctx).flat()) expect(inMenus.has(all.find((a) => a.id === b.id)!.label), `${b.id} in ${ctx}`).toBe(true);
    }
  });

  it("schematic and PCB menu sets differ: each shows only its own tools", () => {
    const sch = menusFor(workspaceActions(deps()), "schematic").map((m) => m.label);
    const pcb = menusFor(workspaceActions(deps()), "pcb").map((m) => m.label);
    expect(sch).toContain("Simulation");
    expect(sch).not.toContain("Route");
    expect(sch).not.toContain("Manufacturing");
    expect(pcb).toContain("Route");
    expect(pcb).toContain("Manufacturing");
    expect(pcb).not.toContain("Simulation");
    expect(labels("schematic", "Place")).toEqual(["Place part", "Part: Resistor", "Wire", "Net / power label", "No connect", "Label pin", "Junction"]);
    expect(labels("pcb", "Place")).toEqual(["Via", "Board outline", "Copper zone"]);
    expect(labels("pcb", "Route")).toEqual(["Route track", "Auto-route all nets", "Clear all routing"]);
    expect(labels("schematic", "Design")).toEqual(["Update PCB from schematic", "Design rules and net classes…", "Run ERC", "Annotate schematic"]);
    expect(labels("pcb", "Design")).toEqual(["Update PCB from schematic", "Design rules and net classes…", "Run DRC", "Run DFM (Fabricator standard)", "Auto-place footprints", "Refill zones"]);
    // the 3D view has no editing tools at all, only the board outputs and shared chrome
    expect(labels("3d", "Place")).toBeNull();
    expect(labels("3d", "Manufacturing")).toContain("Export manufacturing files…");
    expect(toolbarFor(workspaceActions(deps()), "3d").flat().map((b) => b.id)).not.toContain("t-wire");
    expect(labels("home", "Edit")).toEqual(["Undo", "Redo", "Clear selection"]);
  });

  it("menu Place/Route entries arm the same editor tool as the toolbar", () => {
    const d = deps();
    const all = workspaceActions(d);
    for (const [ctx, menu, label, tool] of [["schematic", "Place", "Wire", "wire"], ["schematic", "Place", "Net / power label", "label"], ["pcb", "Route", "Route track", "route"], ["pcb", "Place", "Via", "via"]] as const) {
      const item = menusFor(all, ctx).find((m) => m.label === menu)!.items.find((i) => i.label === label)!;
      const button = toolbarFor(all, ctx).flat().find((b) => b.label === label)!;
      expect(item.run).toBe(button.run);
      item.run!();
      expect(d.tool).toHaveBeenLastCalledWith(tool);
    }
  });

  it("keys come from the default key map and respect context, typing and enablement", () => {
    const d = deps();
    const all = workspaceActions(d);
    const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({ key: k, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods }) as unknown as KeyboardEvent;
    expect(actionForKey(all, "schematic", key("s", { ctrlKey: true }), true)?.id).toBe("save");
    expect(actionForKey(all, "schematic", key("z", { ctrlKey: true }), false)?.id).toBe("undo");
    expect(actionForKey(all, "schematic", key("z", { ctrlKey: true }), true)).toBeUndefined(); // the text field undoes
    expect(actionForKey(all, "schematic", key("y", { ctrlKey: true }), false)?.id).toBe("redo");
    expect(actionForKey(all, "home", key("3", { ctrlKey: true }), false)?.id).toBe("ws-viewer3d");
    // runAction runs only an enabled action and confirms a destructive one
    const undo = all.find((a) => a.id === "undo")!;
    expect(runAction(undo)).toBe(undo.enabled() === true);
    const clear = all.find((a) => a.id === "clear")!;
    expect(runAction(clear, () => false)).toBe(false);
    expect(d.execute).not.toHaveBeenCalled();
    expect(runAction(clear, () => true)).toBe(true);
    expect(d.execute).toHaveBeenCalledWith({ type: "ClearRouting", net: null });
  });

  it("the palette lists the context's actions with their shortcuts, disabled ones flagged", () => {
    const pal = paletteFor(workspaceActions(deps({ busy: true })), "pcb");
    expect(pal.find((p) => p.id === "drc")).toMatchObject({ disabled: true, group: "Design" });
    expect(pal.find((p) => p.id === "erc")).toBeUndefined();
    expect(pal.find((p) => p.id === "save")?.shortcut).toBe("Mod+S");
  });
});
