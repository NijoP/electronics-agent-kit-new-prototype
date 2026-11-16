import { describe, expect, it } from "vitest";
import { junctionPoints } from "./junctions";
import type { Wire } from "../../types";

const w = (id: string, ax: number, ay: number, bx: number, by: number): Wire => ({ id, sheet_id: "s", a: { x: ax, y: ay }, b: { x: bx, y: by } });

describe("junctionPoints", () => {
  it("draws no dot where only two wires meet end to end", () => {
    expect(junctionPoints([w("a", 0, 0, 10, 0), w("b", 10, 0, 10, 10)], [])).toEqual([]);
  });
  it("draws a dot where three connection points meet", () => {
    const pts = junctionPoints([w("a", 0, 0, 10, 0), w("b", 10, 0, 10, 10), w("c", 10, 0, 20, 0)], []);
    expect(pts).toEqual([{ x: 10, y: 0 }]);
  });
  it("draws a dot for a T-junction and for a pin landing on a wire", () => {
    expect(junctionPoints([w("a", 0, 0, 20, 0), w("b", 10, 0, 10, 10)], [])).toEqual([{ x: 10, y: 0 }]);
    expect(junctionPoints([w("a", 0, 0, 20, 0)], [{ x: 10, y: 0 }])).toEqual([{ x: 10, y: 0 }]);
  });
  it("draws none for a lone wire or a pin at a wire end", () => {
    expect(junctionPoints([w("a", 0, 0, 20, 0)], [])).toEqual([]);
    expect(junctionPoints([w("a", 0, 0, 20, 0)], [{ x: 0, y: 0 }])).toEqual([]);
  });
});
