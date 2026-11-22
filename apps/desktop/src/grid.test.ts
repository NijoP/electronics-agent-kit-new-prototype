// Grid density follows the zoom: the configured spacing at close zoom, coarsened by the
// major division when the view is wide, so the screen never fills with lines.
import { describe, expect, it } from "vitest";
import { snapPoint } from "./editing";
import { formatLength, fromMm, getGrid, GRID_KINDS, MAX_LINES, gridSteps, parseGrid, presetsMm, snapStepNm, stepPreset, ticks, toMm, updateGrid, withUnit, type GridConfig } from "./grid";

describe("editor grid", () => {
  it("keeps the configured spacing when zoomed in and coarsens it when zoomed out", () => {
    expect(gridSteps(50, 1.27, 4)).toEqual({ minor: 1.27, major: 5.08 });
    const out = gridSteps(400, 1.27, 4);
    expect(out.minor).toBeCloseTo(5.08);
    expect(400 / out.minor).toBeLessThanOrEqual(MAX_LINES);
    expect(gridSteps(10_000, 0.05, 10).minor).toBeCloseTo(500);
  });

  it("ticks are the multiples of the step inside the range", () => {
    expect(ticks(-1, 2.6, 1.27)).toEqual([0, 1.27, 2.54]);
    expect(ticks(0.1, 0.9, 1)).toEqual([]);
  });

  it("schematic and PCB grids are configured independently", () => {
    expect(GRID_KINDS.schematic.defaults.spacingMm).toBe(1.27);
    expect(GRID_KINDS.pcb.defaults.spacingMm).toBe(0.5);
    expect(GRID_KINDS.schematic.majorEvery).not.toBe(GRID_KINDS.pcb.majorEvery);
  });
});

// Slice A2: grid size and unit per editor, and what they do to snapping.
const nm = (v: number) => Math.round(v * 1_000_000);
const snapX = (xMm: number, cfg: GridConfig) => snapPoint({ x: nm(xMm), y: 0 }, snapStepNm(cfg), [], 0).point.x / 1e6;

describe("grid units and snapping (slice A2)", () => {
  it("converts units exactly at the presets", () => {
    expect(toMm(50, "mil")).toBe(1.27);
    expect(toMm(0.1, "inch")).toBe(2.54);
    expect(toMm(25, "mil")).toBe(0.635);
    expect(fromMm(2.54, "mil")).toBe(100);
    expect(formatLength(1.27, "mil")).toBe("50 mil");
    expect(formatLength(2.54, "inch")).toBe("0.1 in");
    expect(presetsMm("pcb", "mil")).toEqual([0.0254, 0.127, 0.254, 0.635, 1.27, 2.54]);
  });

  it("a unit change moves the spacing to the nearest preset of that unit, and the snap moves with it", () => {
    const pcb = GRID_KINDS.pcb.defaults; // 0.5 mm
    expect(snapX(1.2, pcb)).toBe(1);
    const mil = withUnit("pcb", pcb, "mil");
    expect(mil).toMatchObject({ unit: "mil", spacingMm: 0.635 }); // 25 mil is nearest to 0.5 mm
    expect(snapX(1.2, mil)).toBe(1.27);
    // 1.27 mm is exactly 50 mil and 0.05 in: the schematic grid survives the round trip
    const sch = GRID_KINDS.schematic.defaults;
    expect(withUnit("schematic", sch, "mil").spacingMm).toBe(1.27);
    expect(withUnit("schematic", withUnit("schematic", sch, "inch"), "mm").spacingMm).toBe(1.27);
    // the schematic's 0.5 mm becomes 25 mil (nearer than 10 mil)
    expect(withUnit("schematic", { ...sch, spacingMm: 0.5 }, "mil").spacingMm).toBe(0.635);
  });

  it("a grid size change moves the snap in both editors; snap off places freely", () => {
    for (const kind of ["schematic", "pcb"] as const) {
      const d = GRID_KINDS[kind].defaults;
      const coarse = { ...d, spacingMm: 2.54 };
      expect(snapX(3.9, coarse), kind).toBe(5.08);
      expect(snapX(3.9, { ...coarse, snap: false }), kind).toBe(3.9);
      expect(snapStepNm({ ...coarse, snap: false })).toBe(1);
    }
    expect(snapX(0.33, { ...GRID_KINDS.pcb.defaults, spacingMm: 0.1 })).toBe(0.3);
  });

  it("finer and coarser step through the unit's presets and stop at the ends", () => {
    const pcb = GRID_KINDS.pcb.defaults;
    expect(stepPreset("pcb", pcb, -1)).toBe(0.25);
    expect(stepPreset("pcb", pcb, 1)).toBe(1);
    expect(stepPreset("pcb", { ...pcb, spacingMm: 0.05 }, -1)).toBeNull();
    expect(stepPreset("schematic", { ...GRID_KINDS.schematic.defaults, unit: "inch", spacingMm: 2.54 }, 1)).toBeNull();
  });

  it("saved settings are repaired; settings saved before units existed pick the unit that has the spacing", () => {
    expect(parseGrid("pcb", null)).toEqual(GRID_KINDS.pcb.defaults);
    expect(parseGrid("pcb", "garbage")).toEqual(GRID_KINDS.pcb.defaults);
    expect(parseGrid("pcb", JSON.stringify({ spacingMm: 0.635, visible: false, snap: true }))).toEqual({ unit: "mil", spacingMm: 0.635, visible: false, snap: true });
    expect(parseGrid("pcb", JSON.stringify({ unit: "mm", spacingMm: 0.3 })).spacingMm).toBe(0.5);
    expect(parseGrid("schematic", JSON.stringify({ unit: "furlong", spacingMm: 2.54 })).unit).toBe("mm");
  });

  it("the store is per editor kind and persists", () => {
    localStorage.clear();
    updateGrid("pcb", { unit: "inch" });
    expect(getGrid("pcb")).toMatchObject({ unit: "inch", spacingMm: 0.635 });
    expect(getGrid("schematic")).toEqual(GRID_KINDS.schematic.defaults);
    updateGrid("pcb", { spacingMm: 1.27 });
    expect(JSON.parse(localStorage.getItem("eak.grid.pcb") ?? "{}")).toMatchObject({ unit: "inch", spacingMm: 1.27 });
    updateGrid("pcb", { spacingMm: 0.3 }); // not an inch preset: ignored
    expect(getGrid("pcb").spacingMm).toBe(1.27);
    localStorage.clear();
    expect(getGrid("pcb")).toEqual(GRID_KINDS.pcb.defaults);
  });
});
