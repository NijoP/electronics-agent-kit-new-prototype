import { describe, expect, it } from "vitest";
import { nextRotation, outlineFrom, segments, snapPoint } from "./editing";
import { fromView } from "./viewport";

const P = (x: number, y: number) => ({ x, y });

describe("editing helpers", () => {
  it("snaps to the grid and captures nearby targets", () => {
    expect(snapPoint(P(1_300_000, 2_600_000), 1_270_000, [], 0)).toEqual({ point: P(1_270_000, 2_540_000), hit: null });
    const r = snapPoint(P(10_100_000, 5_000_000), 1_270_000, [P(0, 0), P(10_000_000, 5_000_000)], 500_000);
    expect(r).toEqual({ point: P(10_000_000, 5_000_000), hit: 1 });
  });
  it("splits diagonal connections into horizontal then vertical legs", () => {
    expect(segments(P(0, 0), P(5, 0), false)).toEqual([[P(0, 0), P(5, 0)]]);
    expect(segments(P(0, 0), P(5, 5), false)).toHaveLength(2);
    expect(segments(P(0, 0), P(5, 5), true)).toHaveLength(1);
    expect(segments(P(1, 1), P(1, 1), true)).toEqual([]);
  });
  it("turns canvas coordinates into Y-up nanometres without negative zero", () => {
    expect(fromView(2.5, 0)).toEqual(P(2_500_000, 0));
    expect(Object.is(fromView(0, 0).y, 0)).toBe(true);
    expect(fromView(1, 3)).toEqual(P(1_000_000, -3_000_000));
  });
  it("builds outlines and cycles rotation", () => {
    expect(outlineFrom([P(0, 0), P(10, 5)])).toEqual([P(0, 0), P(10, 0), P(10, 5), P(0, 5)]);
    expect(outlineFrom([P(0, 0), P(10, 0)])).toBeNull();
    expect(outlineFrom([P(0, 0)])).toBeNull();
    expect(nextRotation("R270")).toBe("R0");
  });
});
