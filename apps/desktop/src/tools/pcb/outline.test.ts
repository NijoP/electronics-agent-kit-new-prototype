// Unrun (no builds in this phase): board-outline geometry.
import { describe, expect, it } from "vitest";
import * as O from "./outline";

const P = (x: number, y: number) => ({ x, y });

describe("board outline geometry", () => {
  it("converts mm and mil exactly", () => {
    expect(O.unitToNm(1, "mm")).toBe(1_000_000);
    expect(O.unitToNm(100, "mil")).toBe(2_540_000);
    expect(O.parseNum(" 2,5 ")).toBe(2.5);
    expect(O.parseNum("x")).toBeNull();
  });
  it("builds rectangles, circles and rounded rectangles", () => {
    expect(O.rectangle(P(0, 0), 10, 5)).toEqual([P(0, 0), P(10, 0), P(10, 5), P(0, 5)]);
    expect(O.rectangle(P(0, 0), 0, 5)).toBeNull();
    const c = O.circle(P(0, 0), 1_000_000, 32)!;
    expect(c).toHaveLength(32);
    expect(O.validateOutline(c)).toEqual([]);
    const r = O.roundedRectangle(P(0, 0), 20_000_000, 10_000_000, 2_000_000)!;
    expect(O.validateOutline(r)).toEqual([]);
    expect(O.roundedRectangle(P(0, 0), 4, 4, 3)).toBeNull();
  });
  it("edits vertices", () => {
    const sq = O.rectangle(P(0, 0), 10, 10)!;
    expect(O.insertVertex(sq, 0)[1]).toEqual(P(5, 0));
    expect(O.deleteVertex(sq, 0)).toHaveLength(3);
    expect(O.moveVertex(sq, 2, P(12, 12))[2]).toEqual(P(12, 12));
    expect(O.nearestVertex(sq, P(9, 9), 3)).toBe(2);
  });
  it("flags unclosed or self-crossing outlines", () => {
    expect(O.validateOutline([P(0, 0), P(1, 1)])[0]).toContain("at least 3");
    expect(O.validateOutline([P(0, 0), P(10, 10), P(10, 0), P(0, 10)])[0]).toContain("cross");
    expect(O.validateOutline([P(0, 0), P(5, 0), P(10, 0)])).toContain("outline has zero area");
  });
});
