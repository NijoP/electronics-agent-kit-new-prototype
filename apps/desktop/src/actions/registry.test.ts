// Action registry (slice A1): one definition per operation drives menus, toolbar, palette and keys.
import { describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import { ICON_NAMES } from "../design/icons/icons";
import { GRID_KINDS } from "../grid";
import { SIDE_VIEWS } from "../shell/ActivityBar";
import { actionForKey, contextMenuFor, menusFor, paletteFor, runAction, toolbarFor, visibleIn } from "./registry";
import { NO_EDITOR, workspaceActions, type ActionDeps } from "./sets";
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
    rightOpen: true,
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
    showSide: fn(), toggleSide: fn(), toggleBottom: fn(), toggleRight: fn(), showBottom: fn(), showRight: fn(), onTheme: fn(), resetLayout: fn(), onSettings: fn(), preferences: fn(),
    palette: fn(), quickOpen: fn(), help: fn(), about: fn(), sideView: "project", notify: fn(), open: fn(), closeAll: fn(), showWorkspace: fn(), switchTo: fn(), placePart: fn(),
    grid: { kind: "schematic", cfg: GRID_KINDS.schematic.defaults, set: fn() },
    toolbarLabels: false,
    toggleToolbarLabels: fn(),
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
    expect(labels("schematic", "Place")).toEqual(["Place part", "Wire", "Bus", "Bus entry", "Power port GND", "Power port +3V3", "Power port +5V", "Power port AGND", "Power port CHASSIS", "Net / power label", "Net port (this sheet)", "Off-page connector", "No connect", "Label pin", "Line", "Rectangle", "Circle", "Text", "Fan out net labels", "Fan out no-connects", "Part: Resistor"]);
    expect(labels("pcb", "Place")).toEqual(["Via", "Board outline", "Copper zone"]);
    expect(labels("pcb", "Route")).toEqual(["Route track", "Auto-route all nets", "Clear all routing"]);
    expect(labels("schematic", "Design")).toEqual(["Update PCB from schematic", "Design rules and net classes…", "Run ERC", "Annotate schematic"]);
    expect(labels("pcb", "Design")).toEqual(["Update PCB from schematic", "Design rules and net classes…", "Run DRC", "Run DFM (Fabricator standard)", "Auto-place footprints", "Refill zones", "Stitch zones"]);
    // the 3D view has no editing tools at all, only the board outputs and shared chrome
    expect(labels("3d", "Place")).toBeNull();
    expect(labels("3d", "Manufacturing")).toContain("Export manufacturing files…");
    expect(toolbarFor(workspaceActions(deps()), "3d").flat().map((b) => b.id)).not.toContain("t-wire");
    expect(labels("home", "Edit")).toEqual(["Undo", "Redo", "Clear selection"]);
  });

  it("the 3D document has its own menu set (C6/E6), driving the viewer's published controls", () => {
    const fn = () => vi.fn();
    const v3d = { projection: "perspective" as const, measuring: false, measured: false, labels: true, modelCount: 0, preset: fn(), reset: fn(), zoomBy: fn(), setProjection: fn(), toggleMeasure: fn(), clearMeasure: fn(), toggleLabels: fn(), reloadModels: fn() };
    const snap = fixture(7);
    const probeTo = vi.fn();
    const d = deps({ view3d: v3d, probeTo, selected: "sym_r1", selectedSymbol: snap.project.schematic.symbols.sym_r1, fpOfSelected: snap.project.pcb.footprints.fp_r1 });
    const view = labels("3d", "View", d)!;
    for (const l of ["Top view", "Bottom view", "Front view", "Back view", "Left view", "Right view", "Isometric view (fit)", "Reset view", "Orthographic projection", "Placeholder labels", "Show selection in PCB", "Show selection in schematic"]) expect(view, l).toContain(l);
    expect(labels("3d", "Tools", d)).toEqual(expect.arrayContaining(["Measure distance", "Clear measurement", "Reload 3D models"]));
    // none of it leaks into the editors
    expect(labels("pcb", "View", d)).not.toContain("Top view");
    expect(labels("schematic", "Tools", d)).not.toContain("Measure distance");
    const item = (label: string, menu = "View", dd = d) => menusFor(workspaceActions(dd), "3d").find((m) => m.label === menu)!.items.find((i) => i.label === label)!;
    item("Front view").run!();
    expect(v3d.preset).toHaveBeenLastCalledWith("front");
    item("Orthographic projection").run!();
    expect(v3d.setProjection).toHaveBeenLastCalledWith("orthographic");
    item("Measure distance", "Tools").run!();
    expect(v3d.toggleMeasure).toHaveBeenCalled();
    item("Show selection in PCB").run!();
    expect(probeTo).toHaveBeenLastCalledWith("pcb");
    // state-dependent entries say why they are off
    expect(item("Clear measurement", "Tools").disabled).toBe("No measurement on screen");
    expect(item("Reload 3D models", "Tools").disabled).toBe("No part references a 3D model file");
    expect(toolbarFor(workspaceActions(d), "3d").flat().map((b) => b.id)).toEqual(expect.arrayContaining(["v3d-zin", "v3d-zout", "v3d-reset", "v3d-measure"]));
    // without a drawing viewer (no WebGL) the camera actions are disabled with the reason
    expect(item("Top view", "View", deps()).disabled).toMatch(/not drawing/);
    expect(item("Show selection in PCB", "View", deps({ view3d: v3d, probeTo })).disabled).toMatch(/Select a part first/);
  });

  // Regression: with a full library the Place menu listed ~30 parts before Wire, Net / power
  // label, No connect and Label pin, so those tools sat below the bottom of the window and a
  // pick from the menu (e.g. while Place was armed) never reached them. Tools now come first.
  it("Place menu lists every schematic tool before the library parts, however many parts there are", () => {
    const library = Array.from({ length: 40 }, (_, i) => ({ id: `p${i}`, name: `Part ${i}`, category: "passive", refdes_prefix: "R", default_value: "", pins: [] }));
    const items = labels("schematic", "Place", deps({ library }))!;
    expect(items.slice(0, 20)).toEqual(["Place part", "Wire", "Bus", "Bus entry", "Power port GND", "Power port +3V3", "Power port +5V", "Power port AGND", "Power port CHASSIS", "Net / power label", "Net port (this sheet)", "Off-page connector", "No connect", "Label pin", "Line", "Rectangle", "Circle", "Text", "Fan out net labels", "Fan out no-connects"]);
    expect(items).toHaveLength(60);
    expect(items.slice(20).every((l) => l.startsWith("Part: "))).toBe(true);
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

describe("toolbar icons, families and grid actions (slice A2)", () => {
  it("every toolbar action in every context has an icon from the EAK set", () => {
    const known = new Set<string>(ICON_NAMES);
    for (const ctx of DOC_CONTEXTS)
      for (const b of toolbarFor(workspaceActions(deps()), ctx).flat()) {
        expect(b.icon, `${b.id} in ${ctx}`).toBeTruthy();
        expect(known.has(b.icon!), `${b.id}: ${b.icon}`).toBe(true);
      }
  });

  it("split families group the tools of one kind in one toolbar group", () => {
    const ids = (ctx: DocContext, split: string) => toolbarFor(workspaceActions(deps()), ctx).flat().filter((b) => b.split === split).map((b) => b.id);
    expect(ids("schematic", "sch-marks")).toEqual(["t-noconnect", "t-pinlabel"]);
    expect(ids("schematic", "sch-labels")).toEqual(["t-label", "t-netport", "t-offpage"]);
    expect(ids("schematic", "sch-bus")).toEqual(["t-bus", "t-busentry"]);
    expect(ids("schematic", "sch-power")).toEqual(["t-power-0", "t-power-1", "t-power-2", "t-power-3", "t-power-4"]);
    expect(ids("schematic", "sch-graphics")).toEqual(["t-line", "t-rect", "t-circle", "t-text"]);
    expect(ids("pcb", "pcb-regions")).toEqual(["t-outline", "t-zone"]);
    expect(ids("pcb", "pcb-checks")).toEqual(["drc", "dfm"]);
    for (const ctx of DOC_CONTEXTS)
      for (const g of toolbarFor(workspaceActions(deps()), ctx))
        for (const b of g.filter((x) => x.split)) expect(g.filter((x) => x.split === b.split).length, `${b.split} split across groups`).toBe(toolbarFor(workspaceActions(deps()), ctx).flat().filter((x) => x.split === b.split).length);
  });

  it("the grid is no longer a disabled placeholder: View drives the active editor's grid", () => {
    const set = vi.fn();
    const all = workspaceActions(deps({ grid: { kind: "pcb", cfg: GRID_KINDS.pcb.defaults, set } }));
    expect(all.find((a) => a.id === "grid")).toBeUndefined();
    const view = menusFor(all, "pcb").find((m) => m.label === "View")!.items;
    for (const l of ["Show grid", "Snap to grid", "Finer grid", "Coarser grid", "Unit: mm", "Unit: mil", "Unit: inch", "Toolbar labels"]) expect(view.map((i) => i.label)).toContain(l);
    expect(view.find((i) => i.label === "Unit: mm")?.checked).toBe(true);
    const run = (id: string) => runAction(all.find((a) => a.id === id)!);
    run("grid-snap");
    expect(set).toHaveBeenLastCalledWith({ snap: false });
    run("grid-finer");
    expect(set).toHaveBeenLastCalledWith({ spacingMm: 0.25 });
    run("unit-mil");
    expect(set).toHaveBeenLastCalledWith({ unit: "mil" });
    // at the finest preset "Finer grid" is disabled and says why
    const finest = workspaceActions(deps({ grid: { kind: "pcb", cfg: { ...GRID_KINDS.pcb.defaults, spacingMm: 0.05 }, set } }));
    expect(finest.find((a) => a.id === "grid-finer")!.enabled()).toMatch(/finest mm preset \(0\.05 mm\)/);
    // grid actions exist only in the editors
    expect(menusFor(all, "home").find((m) => m.label === "View")!.items.map((i) => i.label)).not.toContain("Show grid");
  });

  it("Toolbar labels is a checked View option that toggles", () => {
    const d = deps({ toolbarLabels: true });
    const a = workspaceActions(d).find((x) => x.id === "toolbar-labels")!;
    expect(a.checked).toBe(true);
    runAction(a);
    expect(d.toggleToolbarLabels).toHaveBeenCalled();
  });
});

describe("slice E1: no dead or decorative entries", () => {
  it("the inventory's permanently disabled entries are gone, and nothing says 'Not implemented'", () => {
    for (const busy of [false, true]) {
      const all = workspaceActions(deps({ busy }));
      for (const id of ["saveas", "t-junction", "release"]) expect(all.find((a) => a.id === id), id).toBeUndefined();
      for (const ctx of DOC_CONTEXTS) {
        for (const m of menusFor(all, ctx)) for (const i of m.items) expect(i.disabled ?? "", `${m.label} > ${i.label}`).not.toMatch(/not implemented|placeholder/i);
        for (const b of toolbarFor(all, ctx).flat()) expect(b.disabled ?? "", b.id).not.toMatch(/not implemented|placeholder/i);
      }
    }
    expect(labels("pcb", "Manufacturing")).toEqual(["Export manufacturing files…", "Show manufacturing panel"]);
  });

  it("an idle workspace with a selection has no disabled entry that is not explained by its state", () => {
    const d = deps({ selected: "sym_r1", selectedSymbol: fixture(7).project.schematic.symbols.sym_r1 });
    const all = workspaceActions(d);
    // B3: without a schematic editor its editing set (Group and Ungroup included) says so.
    const stateReasons = /Nothing to (undo|redo)|footprint|No copper zones|finest|coarsest|No documents|Nothing is selected|No schematic editor is active|The 3D view is not drawing/;
    for (const ctx of DOC_CONTEXTS)
      for (const a of visibleIn(all, ctx)) {
        const e = a.enabled();
        if (e !== true) expect(e, `${a.id} in ${ctx}`).toMatch(stateReasons);
      }
  });

  it("Show <view> shows the view (never toggles it closed) and is checked while that view is open", () => {
    const d = deps({ sideOpen: true, sideView: "design" });
    const all = workspaceActions(d);
    const design = all.find((a) => a.id === "show-design")!;
    expect(design.checked).toBe(true);
    expect(all.find((a) => a.id === "show-domain")!.checked).toBe(false);
    runAction(design);
    expect(d.showSide).toHaveBeenCalledWith("design");
    expect(d.toggleSide).not.toHaveBeenCalled();
  });

  it("About opens the About dialog; file documents are labelled as files, not sheets", () => {
    const d = deps({ docs: [{ id: "sch:sht_1", kind: "schematic", title: "Sheet 1" }, { id: "file:notes.md", kind: "file", title: "notes.md" }] });
    const all = workspaceActions(d);
    runAction(all.find((a) => a.id === "about")!);
    expect(d.about).toHaveBeenCalled();
    expect(d.notify).not.toHaveBeenCalled();
    expect(labels("home", "Project", d)).toEqual(expect.arrayContaining(["Open sheet Sheet 1", "Open file notes.md"]));
  });
});

describe("dock actions (slice A3)", () => {
  it("each dock has a View toggle and a key; Layers is a PCB-only right-dock tab; the library opens in the bottom dock", () => {
    const d = deps({ rightOpen: false });
    const all = workspaceActions(d);
    const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({ key: k, ctrlKey: true, metaKey: false, shiftKey: false, altKey: false, ...mods }) as unknown as KeyboardEvent;
    expect(actionForKey(all, "schematic", key("b"), false)?.id).toBe("toggle-side");
    expect(actionForKey(all, "schematic", key("j"), false)?.id).toBe("toggle-bottom");
    expect(actionForKey(all, "schematic", key("B", { shiftKey: true }), false)?.id).toBe("toggle-right");
    expect(actionForKey(all, "pcb", key("D", { shiftKey: true }), false)?.id).toBe("show-design");
    const view = (ctx: DocContext) => menusFor(all, ctx).find((m) => m.label === "View")!.items;
    expect(view("schematic").find((i) => i.label === "Right panel")?.checked).toBe(false);
    expect(view("schematic").map((i) => i.label)).not.toContain("Show Layers");
    expect(view("pcb").map((i) => i.label)).toContain("Show Layers");
    // the activity bar views no longer include the panels that moved into the docks
    expect(SIDE_VIEWS.map((v) => v.id)).toEqual(["overview", "project", "design", "domain", "manufacturing"]);
    runAction(all.find((a) => a.id === "show-layers")!);
    expect(d.showRight).toHaveBeenLastCalledWith("layers");
    runAction(all.find((a) => a.id === "library")!);
    expect(d.showBottom).toHaveBeenLastCalledWith("library");
  });
});

describe("schematic editing set and canvas context menu (slice B3)", () => {
  const TARGETS = ["symbol", "wire", "label", "sheet"] as const;
  const editor = () => {
    const ran: string[] = [];
    const state: Record<string, true | string> = {};
    return { ran, state, editorOp: (op: string) => ({ enabled: () => state[op] ?? true, run: () => void ran.push(op) }) };
  };

  it("context-menu items are exactly the registry actions that list the clicked target, in registry order", () => {
    const all = workspaceActions(deps({ editorOp: editor().editorOp }));
    for (const t of TARGETS) {
      const items = contextMenuFor(all, "schematic", t);
      const expected = visibleIn(all, "schematic").filter((a) => a.context?.includes(t));
      expect(items.map((i) => i.id), t).toEqual(expected.map((a) => a.id));
      expect(items.map((i) => i.label), t).toEqual(expected.map((a) => a.label));
      // every entry also lives in a menu of the same context: the context menu adds no operation of its own
      const inMenus = new Set(menusFor(all, "schematic").flatMap((m) => m.items.map((i) => i.label)));
      for (const i of items) expect(inMenus.has(i.label), `${i.label} (${t})`).toBe(true);
    }
    const labelsOf = (t: (typeof TARGETS)[number]) => contextMenuFor(all, "schematic", t).map((i) => i.label);
    expect(labelsOf("symbol")).toEqual(expect.arrayContaining(["Cut", "Copy", "Duplicate", "Delete selection", "Rotate selection", "Mirror selection", "Mirror selection vertically", "Align left edges", "Distribute horizontally", "Array…", "Find similar objects", "Group", "Ungroup"]));
    expect(labelsOf("wire")).toEqual(expect.arrayContaining(["Highlight net", "Clear net highlight", "Cut", "Copy"]));
    expect(labelsOf("wire")).not.toContain("Rotate selection");
    expect(labelsOf("sheet")).toEqual(["Paste", "Select all", "Find and replace…", "Clear net highlight"]);
    // the PCB has no schematic context menu entries
    for (const t of TARGETS) expect(contextMenuFor(all, "pcb", t).filter((i) => i.id === "copy")).toEqual([]);
  });

  it("an entry runs the editor's own operation, shows its reason when it cannot run, and confirms destructive ones", () => {
    const e = editor();
    e.state.paste = "Nothing to paste: copy or cut first";
    const all = workspaceActions(deps({ editorOp: e.editorOp }));
    const item = (t: (typeof TARGETS)[number], id: string) => contextMenuFor(all, "schematic", t).find((i) => i.id === id)!;
    item("symbol", "copy").run();
    item("symbol", "align-left").run();
    item("wire", "highlight-net").run();
    expect(e.ran).toEqual(["copy", "align-left", "highlight-net"]);
    expect(item("sheet", "paste").disabled).toBe("Nothing to paste: copy or cut first");
    item("sheet", "paste").run(); // disabled: runs nothing
    expect(e.ran).toHaveLength(3);
    // Group and Ungroup (ADR-0045) are editor operations like the rest: they run the editor's own op
    item("symbol", "group").run();
    item("wire", "ungroup").run();
    expect(e.ran).toEqual(["copy", "align-left", "highlight-net", "group", "ungroup"]);
    // the menu bar shows the editor-owned keys of the same entries, and the key handler never runs them twice
    const edit = menusFor(all, "schematic").find((m) => m.label === "Edit")!.items;
    expect(edit.find((i) => i.label === "Copy")?.shortcut).toBe("Mod+C");
    const key = { key: "c", ctrlKey: true, metaKey: false, shiftKey: false, altKey: false } as unknown as KeyboardEvent;
    expect(actionForKey(all, "schematic", key, false)).toBeUndefined();
  });

  it("without a schematic editor the editing set is disabled and says why; rotate and delete fall back to the workspace selection", () => {
    const d = deps({ selected: "sym_r1", selectedSymbol: fixture(7).project.schematic.symbols.sym_r1 });
    const all = workspaceActions(d);
    for (const id of ["copy", "cut", "paste", "find", "highlight-net", "align-left", "array"]) expect(all.find((a) => a.id === id)!.enabled(), id).toBe(NO_EDITOR);
    runAction(all.find((a) => a.id === "rotate")!);
    expect(d.rotateSel).toHaveBeenCalled();
    // with an editor, rotate is the editor's (its multi-selection), not the single workspace selection
    const e = editor();
    const withEditor = workspaceActions(deps({ editorOp: e.editorOp, selected: "sym_r1", selectedSymbol: fixture(7).project.schematic.symbols.sym_r1 }));
    runAction(withEditor.find((a) => a.id === "rotate")!);
    expect(e.ran).toEqual(["rotate"]);
  });
});

describe("schematic place tools (slice B4)", () => {
  const item = (d: ActionDeps, label: string) => menusFor(workspaceActions(d), "schematic").find((m) => m.label === "Place")!.items.find((i) => i.label === label)!;

  it("each power-port entry arms the Power port tool with its index; the list follows Settings", () => {
    const d = deps({ powerPorts: [{ name: "VBAT", style: "rail" }, { name: "PGND", style: "ground" }] });
    item(d, "Power port PGND").run!();
    expect(d.tool).toHaveBeenLastCalledWith("power:1");
    const icons = toolbarFor(workspaceActions(d), "schematic").flat().filter((b) => b.split === "sch-power").map((b) => b.icon);
    expect(icons).toEqual(["power", "ground"]);
    // an unusable entry is disabled with its reason; an empty list leaves one disabled entry
    expect(item(deps({ powerPorts: [{ name: "", style: "rail" }] }), "Power port (unnamed)").disabled).toMatch(/net name/);
    expect(item(deps({ powerPorts: [] }), "Power port").disabled).toMatch(/Settings › Power ports/);
  });

  it("net port, off-page, bus, bus entry and the graphics tools arm their editor tools", () => {
    const d = deps();
    for (const [label, tool] of [["Net port (this sheet)", "netport"], ["Off-page connector", "offpage"], ["Bus", "bus"], ["Bus entry", "busentry"], ["Line", "line"], ["Rectangle", "rect"], ["Circle", "circle"], ["Text", "text"]] as const) {
      item(d, label).run!();
      expect(d.tool).toHaveBeenLastCalledWith(tool);
    }
  });

  it("fan out needs a selected part, then sends one Batch for its unconnected pins", () => {
    expect(item(deps(), "Fan out net labels").disabled).toBe("Select a part first");
    const d = deps({ selectedSymbol: fixture(7).project.schematic.symbols.sym_d1 });
    item(d, "Fan out net labels").run!();
    expect(d.execute).toHaveBeenLastCalledWith(expect.objectContaining({ type: "Batch", label: "Fan out net labels on D1" }));
    item(d, "Fan out no-connects").run!();
    expect(d.execute).toHaveBeenLastCalledWith(expect.objectContaining({ type: "Batch", label: "Fan out no-connects on D1" }));
  });
});
