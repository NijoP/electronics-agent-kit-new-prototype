// Unrun (no builds or tests during the testing freeze): ECAD -> viewer transform for 3D models.
import { describe, expect, it } from "vitest";
import { DEFAULT_TRANSFORM, modelToWorld, transformPoint } from "./modelTransform";

const near = (a: number[], b: number[]) => a.forEach((v, i) => expect(v).toBeCloseTo(b[i], 6));
const top = { x: 10, y: 5, rotationDeg: 0, side: "top" as const, surfaceY: 0.8 };

describe("modelToWorld", () => {
  it("top side, no rotation: model z is up, model y points to -PCB y, origin on the footprint", () => {
    const m = modelToWorld(DEFAULT_TRANSFORM, top);
    near(transformPoint(m, [0, 0, 0]), [10, 0.8, 5]);
    near(transformPoint(m, [1, 0, 0]), [11, 0.8, 5]); // +x -> +X
    near(transformPoint(m, [0, 1, 0]), [10, 0.8, 4]); // model +y -> PCB -y -> world -Z
    near(transformPoint(m, [0, 0, 2]), [10, 2.8, 5]); // +z -> up
  });

  it("footprint rotation of 90 degrees turns +x onto +PCB y (counter-clockwise like Rotation::apply)", () => {
    const m = modelToWorld(DEFAULT_TRANSFORM, { ...top, rotationDeg: 90 });
    near(transformPoint(m, [1, 0, 0]), [10, 0.8, 6]);
  });

  it("bottom side mirrors x like pad positions and hangs below the surface", () => {
    const m = modelToWorld(DEFAULT_TRANSFORM, { x: 0, y: 0, rotationDeg: 0, side: "bottom", surfaceY: -0.8 });
    near(transformPoint(m, [1, 0, 0]), [-1, -0.8, 0]);
    near(transformPoint(m, [0, 0, 2]), [0, -2.8, 0]);
  });

  it("applies scale, then rotation about z, then the offset in footprint coordinates", () => {
    const t = { offset_mm: [1, 2, 3] as [number, number, number], rotation_deg: [0, 0, 90] as [number, number, number], scale: [2, 2, 2] as [number, number, number] };
    const m = modelToWorld(t, { x: 0, y: 0, rotationDeg: 0, side: "top", surfaceY: 0 });
    // model (1,0,0) -> scale (2,0,0) -> Rz90 (0,2,0) -> footprint (0,-2,0) + (1,2,3) = (1,0,3) -> world (1,3,0)
    near(transformPoint(m, [1, 0, 0]), [1, 3, 0]);
  });

  it("is a proper rotation (no mirroring) with unit scale, on both sides", () => {
    for (const side of ["top", "bottom"] as const) {
      const m = modelToWorld(DEFAULT_TRANSFORM, { x: 0, y: 0, rotationDeg: 270, side, surfaceY: 0 });
      const det = m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2]) + m[8] * (m[1] * m[6] - m[5] * m[2]);
      expect(det).toBeCloseTo(1, 6);
    }
  });
});
