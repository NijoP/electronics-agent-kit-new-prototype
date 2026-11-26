// Regression tests for the installed-app QA fixes (docs/qa/REMEDIATION_LEDGER.md). Written, not run.
import { describe, expect, it } from "vitest";
import css from "./styles.css?raw";
import { visibleViewBox } from "./viewport";

const rule = (selector: string): string => {
  const m = css.match(new RegExp(`^${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")} \\{([^}]*)\\}`, "m"));
  expect(m, `${selector} rule exists`).not.toBeNull();
  return m![1];
};

describe("terminal layout CSS", () => {
  it("terminal body grid has a bounded row so panes never grow past the status bar", () => {
    const r = rule(".term-body");
    expect(r).toContain("grid-template-rows: minmax(0, 1fr)");
    expect(r).toContain("overflow: hidden");
  });
  it("pane containers clip instead of overflowing", () => {
    for (const s of [".term-center", ".term-main", ".term-panes", ".term-pane-wrap"]) expect(rule(s)).toContain("overflow: hidden");
  });
  it("session tab bar is one row that scrolls horizontally", () => {
    const r = rule(".term-tabs");
    expect(r).toContain("flex-wrap: nowrap");
    expect(r).toContain("overflow-x: auto");
  });
});

// Regression: selecting a part mounted the selection bar (and the symbol fields) in the tool
// options strip, which wrapped to a second row and pushed the schematic canvas down.
describe("editor tool options strip", () => {
  it("is one row of fixed height, so the selection bar appearing never moves the canvas", () => {
    const r = rule(".tool-options");
    expect(r).toContain("flex-wrap: nowrap");
    expect(r).toMatch(/(^|[; ])height: calc\(/);
    expect(r).not.toContain("min-height");
    expect(r).toContain("overflow-x: auto");
    expect(rule(".tool-options > *")).toContain("flex-shrink: 0");
  });
  it("long menus scroll inside the window instead of running past its bottom edge", () => {
    const r = rule(".menu-list");
    expect(r).toContain("max-height: calc(100vh");
    expect(r).toContain("overflow-y: auto");
  });
});

describe("visibleViewBox (grid fills the pane)", () => {
  it("grows a too-narrow view box to the pane aspect ratio around the same centre", () => {
    const v = visibleViewBox({ x: 0, y: 0, w: 100, h: 100 }, { width: 400, height: 200 });
    expect(v.h).toBeCloseTo(100);
    expect(v.w).toBeCloseTo(200);
    expect(v.x).toBeCloseTo(-50);
    expect(v.y).toBeCloseTo(0);
  });
  it("grows a too-short view box vertically", () => {
    const v = visibleViewBox({ x: 10, y: 10, w: 100, h: 50 }, { width: 200, height: 200 });
    expect(v.w).toBeCloseTo(100);
    expect(v.h).toBeCloseTo(100);
    expect(v.y).toBeCloseTo(-15);
  });
  it("returns the view box unchanged before the pane is measured", () => {
    const vb = { x: 1, y: 2, w: 3, h: 4 };
    expect(visibleViewBox(vb, { width: 0, height: 0 })).toBe(vb);
  });
});

// Regression (C1, found by the real-window ui-evidence run): the PCB view has four rows (header,
// tool options, layer tab strip, canvas) but used the 3-row `.view` grid, so the layer strip took
// the flexible row and the canvas SVG covered its tabs and buttons.
describe("PCB view rows", () => {
  it("the canvas takes the last flexible row, after the layer tab strip", () => {
    expect(rule(".view.pcb-view")).toContain("grid-template-rows: auto auto auto minmax(0, 1fr)");
    expect(rule(".view.pcb-view > .canvas, .view.pcb-view > .canvas-stack")).toContain("grid-row: 4");
  });
});

// Regression (F6, found by the real-window ui-evidence row `f6-canvas-fits`): the bare SVG canvas
// was the view's grid item with `height: 100%`, which WebKitGTK resolved against the whole view
// (a 399 px box in a 257 px row), so its lower part ran under the canvas strip and bottom panel.
// The SVG now always sits in `.canvas-stack`, which the grid stretches to the flexible row (no
// percentage height), and every layer inside it is out of flow.
describe("canvas box", () => {
  it("is pinned to the view's last, flexible row and has no percentage height", () => {
    expect(rule(".view")).toContain("grid-template-rows: auto auto auto auto minmax(0, 1fr)");
    expect(rule(".view > .canvas-stack")).toContain("grid-row: -2");
    const box = rule(".canvas-stack");
    expect(box).not.toMatch(/height: 100%/);
    expect(box).toContain("min-height: 0");
    expect(box).toContain("overflow: hidden");
  });
  it("the interactive SVG fills the box out of flow", () => {
    expect(rule(".canvas-stack > .canvas")).toContain("position: absolute; inset: 0");
  });
});
