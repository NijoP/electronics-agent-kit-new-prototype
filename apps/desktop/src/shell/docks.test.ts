// Phase 1 A3: dock layout model (per workspace, migration, clamping), design manager data and the
// canvas strip's numbers. Pure functions; the app-level behaviour is in docks.app.test.tsx.
import { afterEach, describe, expect, it } from "vitest";
import { fixture } from "../fixture.test-data";
import { GRID_KINDS } from "../grid";
import { componentsOf, editorFor, filterItems, netsOf, objectsOf, pagesOf, pcbObjects, schematicPlace } from "./designManagerData";
import { clampSize, defaultLayout, layoutKey, LIMITS, loadLayout, migrateV1, patchLayout, sanitizeLayout, saveLayout } from "./layout";
import { stripText } from "./CanvasStrip";
import { rightTabs, shownRightTab } from "./RightDock";
import type { ConnectivityReport } from "../types";

afterEach(() => localStorage.clear());

describe("dock layout (spec UI-01)", () => {
  it("defaults per workspace: editors open the design manager and the right dock, the PCB shows Layers", () => {
    expect(defaultLayout("schematic").left.tab).toBe("design");
    expect(defaultLayout("schematic").right).toMatchObject({ open: true, tab: "properties" });
    expect(defaultLayout("pcb").right).toMatchObject({ open: true, tab: "layers" });
    expect(defaultLayout("home").right.open).toBe(false);
    expect(defaultLayout("3d").left.tab).toBe("project");
  });
  it("is saved per workspace: changing the PCB docks leaves the schematic's alone", () => {
    saveLayout("pcb", patchLayout(loadLayout("pcb"), "right", { open: false, size: 333 }));
    expect(loadLayout("pcb").right).toEqual({ open: false, size: 333, tab: "layers" });
    expect(localStorage.getItem(layoutKey("schematic"))).toBeNull();
    expect(loadLayout("schematic")).toEqual(defaultLayout("schematic"));
  });
  it("clamps sizes and rejects malformed values and tabs from storage", () => {
    expect(clampSize("right", 10)).toBe(LIMITS.right[0]);
    expect(clampSize("left", 9999)).toBe(LIMITS.left[1]);
    const l = sanitizeLayout("schematic", { left: { open: false, size: 9999, tab: "<script>" }, right: "x", bottom: { size: 12.6 } });
    expect(l.left).toEqual({ open: false, size: LIMITS.left[1], tab: "design" });
    expect(l.right).toEqual(defaultLayout("schematic").right);
    expect(l.bottom.size).toBe(LIMITS.bottom[0]);
    localStorage.setItem(layoutKey("pcb"), "{not json");
    expect(loadLayout("pcb")).toEqual(defaultLayout("pcb"));
  });
  it("patches one dock: function patches, clamped size, an invalid tab keeps the current one", () => {
    const l = defaultLayout("schematic");
    const p = patchLayout(l, "left", (d) => ({ size: d.size + 10000, tab: "BAD TAB" }));
    expect(p.left).toEqual({ open: true, size: LIMITS.left[1], tab: "design" });
    expect(p.bottom).toBe(l.bottom);
  });
  it("migrates the pre-A3 machine-wide layout into every workspace", () => {
    const old = { sideOpen: false, sideWidth: 300, bottomOpen: false, bottomHeight: 200 };
    const d = defaultLayout("pcb");
    expect(migrateV1("pcb", old)).toEqual({ ...d, left: { ...d.left, open: false, size: 300 }, bottom: { ...d.bottom, open: false, size: 200 } });
    localStorage.setItem("eak.layout.v1", JSON.stringify(old));
    expect(loadLayout("schematic").left).toMatchObject({ open: false, size: 300 });
    // once a workspace has its own layout, the old one no longer applies to it
    saveLayout("schematic", defaultLayout("schematic"));
    expect(loadLayout("schematic")).toEqual(defaultLayout("schematic"));
  });
});

describe("right dock tabs", () => {
  it("Layers exists only in the PCB; Filter is disabled with its reason; a stored tab that does not apply falls back", () => {
    expect(rightTabs(false).map((t) => t.id)).toEqual(["properties", "filter"]);
    expect(rightTabs(true).map((t) => t.id)).toEqual(["properties", "filter", "layers"]);
    expect(rightTabs(true).find((t) => t.id === "filter")!.disabled).toMatch(/B3/);
    expect(shownRightTab("layers", false)).toBe("properties");
    expect(shownRightTab("filter", true)).toBe("properties");
    expect(shownRightTab("layers", true)).toBe("layers");
  });
});

describe("design manager data", () => {
  const s = fixture(7);
  const netlist = [
    { name: "N1", pins: ["R1.2", "D1.2"] },
    { name: "GND", pins: ["D1.1"] },
  ];
  const conn: ConnectivityReport = { revision: 7, net_groups: {}, shorts: [], ratsnest: [["GND", { x: 0, y: 0 }, { x: 1, y: 1 }]] };
  it("lists pages, components, nets (engine netlist + PCB) and objects", () => {
    expect(pagesOf(s).map((p) => p.label)).toEqual(["Sheet 1", "PCB"]);
    expect(componentsOf(s).map((c) => `${c.label} ${c.detail}`)).toEqual(["D1 Green", "R1 330"]);
    const nets = netsOf(s, netlist, conn);
    expect(nets.map((n) => n.label)).toEqual(["GND", "N1"]);
    expect(nets[0].detail).toBe("1 pin(s) · 1 unrouted");
    // a connectivity report of another revision adds nothing
    expect(netsOf(s, netlist, { ...conn, revision: 6 })[0].detail).toBe("1 pin(s)");
    expect(objectsOf(s).map((o) => o.group)).toEqual(["Wires", "Net labels", "Tracks", "Vias", "Zones"]);
  });
  it("search filters on label and detail, case-insensitively", () => {
    expect(filterItems(componentsOf(s), "green").map((i) => i.label)).toEqual(["D1"]);
    expect(filterItems(componentsOf(s), "  ")).toHaveLength(2);
    expect(filterItems(objectsOf(s), "gnd").map((i) => i.group)).toEqual(["Net labels", "Vias", "Zones"]);
  });
  it("targets: nets and components follow the active editor; a net's PCB objects are its tracks, vias, zones and footprints", () => {
    expect(editorFor({ kind: "net", name: "GND" }, "pcb")).toBe("pcb");
    expect(editorFor({ kind: "net", name: "GND" }, "schematic")).toBe("schematic");
    expect(editorFor({ kind: "object", obj: "wire", id: "wir_1" }, "pcb")).toBe("schematic");
    expect(pcbObjects(s, { kind: "net", name: "GND" })).toEqual(["via_1", "zon_1", "fp_d1"]);
    expect(pcbObjects(s, { kind: "component", symbolId: "sym_r1" })).toEqual(["sym_r1"]);
  });
  it("schematic place: a component's body box, a net's pins and labels, on their sheet", () => {
    const c = schematicPlace(s, { kind: "component", symbolId: "sym_r1" }, null)!;
    expect(c.sheetId).toBe("sht_1");
    // body 48..52 mm, widened to the 10 mm minimum span around its centre (x 50, view y -50)
    expect(c.bounds).toEqual({ minX: 45, minY: -55, maxX: 55, maxY: -45 });
    const n = schematicPlace(s, { kind: "net", name: "GND" }, netlist)!;
    // D1 pin 1 at x 84 and the GND power port at x 85
    expect(n.bounds.minX).toBeLessThan(84);
    expect(n.bounds.maxX).toBeGreaterThan(85);
    expect(schematicPlace(s, { kind: "net", name: "nowhere" }, netlist)).toBeNull();
  });
});

describe("canvas strip", () => {
  const cfg = { ...GRID_KINDS.pcb.defaults, snap: false };
  it("shows design coordinates (Y up), the offset from the last press and the zoom", () => {
    const t = stripText({ at: { x: 12.5, y: -4 }, anchor: { x: 10, y: -1 }, pxPerMm: 96 / 25.4 }, cfg);
    expect(t).toEqual({ x: "12.500", y: "4.000", dx: "2.500", dy: "3.000", zoom: "100 %" });
  });
  it("snaps to the grid when snap is on, and converts the unit", () => {
    const t = stripText({ at: { x: 12.6, y: -4.2 }, anchor: null, pxPerMm: null }, { ...cfg, snap: true, spacingMm: 0.5 });
    expect([t.x, t.y, t.dx, t.zoom]).toEqual(["12.500", "4.000", "—", "—"]);
    expect(stripText({ at: { x: 2.54, y: 0 }, anchor: null, pxPerMm: null }, { ...cfg, unit: "mil", spacingMm: 0.254 }).x).toBe("100.0");
  });
  it("shows dashes while the pointer is outside the canvas, and never -0", () => {
    expect(stripText({ at: null, anchor: { x: 1, y: 1 }, pxPerMm: 10 }, cfg).x).toBe("—");
    expect(stripText({ at: { x: 0, y: 0 }, anchor: null, pxPerMm: null }, cfg).y).toBe("0.000");
  });
});
