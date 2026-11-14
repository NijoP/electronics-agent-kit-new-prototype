// Grid density follows the zoom: the configured spacing at close zoom, coarsened by the
// major division when the view is wide, so the screen never fills with lines.
import { describe, expect, it } from "vitest";
import { GRID_KINDS, MAX_LINES, gridSteps, ticks } from "./grid";

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
