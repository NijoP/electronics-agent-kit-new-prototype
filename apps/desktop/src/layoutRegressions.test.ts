// Regression tests for the installed-app QA fixes (docs/qa/REMEDIATION_LEDGER.md). Written, not run.
import { describe, expect, it } from "vitest";
import css from "./styles.css?raw";
import workspaceSrc from "./terminal/TerminalWorkspace.tsx?raw";
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
  // Regression: panes were classed `row`, picking up the global `.row { align-items: center }`
  // utility, so the pane (whose xterm host is absolutely positioned) collapsed to 0 px height and
  // every terminal showed as an empty black area (found on Windows, all platforms affected).
  it("terminal panes stretch to full height and never use the generic row/column classes", () => {
    expect(rule(".term-panes")).toContain("align-items: stretch");
    expect(workspaceSrc).not.toMatch(/className=\{`term-(panes|divider) \$\{tab\.direction\}`\}/);
    expect(workspaceSrc).toContain("term-panes split-${tab.direction}");
    expect(rule(".term-panes.split-column")).toContain("flex-direction: column");
  });
  it("session tab bar is one row that scrolls horizontally", () => {
    const r = rule(".term-tabs");
    expect(r).toContain("flex-wrap: nowrap");
    expect(r).toContain("overflow-x: auto");
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
