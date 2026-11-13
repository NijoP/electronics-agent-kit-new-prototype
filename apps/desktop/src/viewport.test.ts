import { describe, expect, it } from "vitest";
import { boundsOf, clientToView, fit, pan, toView, zoomAt } from "./viewport";

describe("viewport", () => {
  it("converts EAK-IR nanometres (Y up) to view millimetres (Y down)", () => {
    expect(toView({ x: 2_500_000, y: 1_000_000 })).toEqual([2.5, -1]);
  });

  it("fits bounds with a margin", () => {
    const vb = fit({ minX: 0, minY: 0, maxX: 100, maxY: 50 }, 0.1);
    expect(vb).toEqual({ x: -10, y: -5, w: 120, h: 60 });
    expect(boundsOf([])).toBeNull();
  });

  it("zooms about the cursor so the point under it stays fixed", () => {
    const vb = { x: 0, y: 0, w: 100, h: 100 };
    const size = { width: 200, height: 200 };
    const before = clientToView(vb, size, 50, 150);
    const z = zoomAt(vb, 0.5, before[0], before[1]);
    expect(z.w).toBe(50);
    expect(clientToView(z, size, 50, 150)).toEqual(before);
  });

  it("maps clients through letterboxing (meet) and pans by pixels", () => {
    const vb = { x: 0, y: 0, w: 100, h: 50 };
    const size = { width: 200, height: 200 }; // scale 2, 50 px bars top and bottom
    expect(clientToView(vb, size, 0, 50)).toEqual([0, 0]);
    expect(pan(vb, size, 20, -10)).toEqual({ x: -10, y: 5, w: 100, h: 50 });
  });
});
