// The single ECAD -> three.js transform for attached 3D models: model space -> viewer world space.
// Pure maths on plain arrays (no three.js) so it is unit-testable; the viewer applies the result
// with `Matrix4.fromArray`. Nothing else in the viewer may position a model.
//
// Frames (all lengths in millimetres)
//   model     STEP/KiCad convention: x right, y "up the page", z up from the mounting surface.
//   footprint EAK / PCB editor: x right, y DOWN the page, z up from the mounting surface.
//             model -> footprint is (x, -y, z).
//   world     viewer: PCB (x, y) -> world (X, Z), world Y is up. The mounting surface of the top side
//             is the outer top copper face, of the bottom side the outer bottom copper face
//             (`surfaceY`, which already accounts for board thickness and copper).
//
// Pipeline for a model point v, with the component's `ModelTransform` t:
//   1. model origin / orientation / scale (model frame): A = Rz(rz) * Ry(ry) * Rx(rx) * S(scale)
//   2. model -> footprint frame, then add t.offset_mm (footprint coordinates, z = height offset)
//   3. bottom side: mirror x and flip z (the part sits under the board, upside down). The x mirror
//      is the same one the engine applies to pad positions (`Project::pad_position`).
//   4. footprint rotation about the vertical axis, counter-clockwise in PCB (x, y) exactly like
//      `Rotation::apply`, then translate to the footprint origin on the mounting surface.

import type { ModelTransform } from "../types";

export interface Placement {
  /** Footprint origin on the board, mm (PCB x, y). */
  x: number;
  y: number;
  /** Footprint rotation in degrees (0/90/180/270). */
  rotationDeg: number;
  side: "top" | "bottom";
  /** World Y of the mounting surface for that side, mm. */
  surfaceY: number;
}

type M3 = number[]; // row-major 3x3

const mul = (a: M3, b: M3): M3 => {
  const o = new Array<number>(9).fill(0);
  for (let r = 0; r < 3; r++) for (let c = 0; c < 3; c++) for (let k = 0; k < 3; k++) o[r * 3 + c] += a[r * 3 + k] * b[k * 3 + c];
  return o;
};
const apply = (m: M3, v: number[]): number[] => [0, 1, 2].map((r) => m[r * 3] * v[0] + m[r * 3 + 1] * v[1] + m[r * 3 + 2] * v[2]);

const rad = (d: number) => (d * Math.PI) / 180;
const rx = (d: number): M3 => {
  const c = Math.cos(rad(d)), s = Math.sin(rad(d));
  return [1, 0, 0, 0, c, -s, 0, s, c];
};
const ry = (d: number): M3 => {
  const c = Math.cos(rad(d)), s = Math.sin(rad(d));
  return [c, 0, s, 0, 1, 0, -s, 0, c];
};
const rz = (d: number): M3 => {
  const c = Math.cos(rad(d)), s = Math.sin(rad(d));
  return [c, -s, 0, s, c, 0, 0, 0, 1];
};

export const DEFAULT_TRANSFORM: ModelTransform = { offset_mm: [0, 0, 0], rotation_deg: [0, 0, 0], scale: [1, 1, 1] };

/** Column-major 4x4 (three.js `Matrix4.fromArray` order) taking model-space mm to world mm. */
export function modelToWorld(t: ModelTransform, p: Placement): number[] {
  const [sx, sy, sz] = t.scale;
  const [ax, ay, az] = t.rotation_deg;
  const user = mul(mul(mul(rz(az), ry(ay)), rx(ax)), [sx, 0, 0, 0, sy, 0, 0, 0, sz]);
  const toFootprint: M3 = [1, 0, 0, 0, -1, 0, 0, 0, 1];
  const bottom = p.side === "bottom";
  const flip: M3 = [bottom ? -1 : 1, 0, 0, 0, 1, 0, 0, 0, bottom ? -1 : 1];
  const board = rz(p.rotationDeg);
  // footprint (x, y, z) -> world (X, Y, Z) = (x, z, y)
  const toWorld: M3 = [1, 0, 0, 0, 0, 1, 0, 1, 0];
  const linear = mul(toWorld, mul(board, mul(flip, mul(toFootprint, user))));
  const off = apply(toWorld, apply(board, apply(flip, t.offset_mm)));
  return [
    linear[0], linear[3], linear[6], 0,
    linear[1], linear[4], linear[7], 0,
    linear[2], linear[5], linear[8], 0,
    off[0] + p.x, off[1] + p.surfaceY, off[2] + p.y, 1,
  ];
}

/** Apply a column-major 4x4 to a point. */
export function transformPoint(m: number[], v: [number, number, number]): [number, number, number] {
  return [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14],
  ];
}
