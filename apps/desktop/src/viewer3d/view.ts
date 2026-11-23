// Pure helpers of the 3D document (slice C6 / E6): camera presets, the stack-up table read from
// the IR, the measure readout and the board appearance taken from the project's manufacturing
// settings. No three.js here, so every rule is unit-tested without WebGL.
import { layerName, stackLayers } from "../layers";
import type { Snapshot } from "../types";

export type View3dPreset = "top" | "bottom" | "front" | "back" | "left" | "right" | "iso";
export const PRESETS: readonly { id: View3dPreset; label: string; title: string }[] = [
  { id: "top", label: "Top", title: "Look straight down at the top side" },
  { id: "bottom", label: "Bottom", title: "Look up at the bottom side" },
  { id: "front", label: "Front", title: "Look at the board edge on the PCB's +Y side" },
  { id: "back", label: "Back", title: "Look at the board edge on the PCB's -Y side" },
  { id: "left", label: "Left", title: "Look at the board's left edge (-X)" },
  { id: "right", label: "Right", title: "Look at the board's right edge (+X)" },
  { id: "iso", label: "Iso / fit", title: "Isometric view, fit to board" },
];
export type Projection = "perspective" | "orthographic";

type V3 = [number, number, number];
export interface Pose {
  position: V3;
  target: V3;
  /** Camera distance from the target (also sizes the orthographic frustum). */
  distance: number;
}

/**
 * Camera pose that frames `bounds` (PCB mm: [minX, minY, maxX, maxY]) from a preset. World axes:
 * PCB x -> X, PCB y -> Z, up -> Y. A tiny offset keeps the up vector defined straight down/up.
 */
const ISO = Math.hypot(0.55, 0.7, 0.7);

export function presetPose(p: View3dPreset, bounds: readonly [number, number, number, number], fovDeg: number): Pose {
  const [x0, y0, x1, y1] = bounds;
  const cx = (x0 + x1) / 2;
  const cz = (y0 + y1) / 2;
  const r = Math.max(Math.hypot(x1 - x0, y1 - y0) / 2, 5);
  const d = (r / Math.tan((fovDeg * Math.PI) / 360)) * 1.15;
  const e = d * 0.001;
  const pos: Record<View3dPreset, V3> = {
    top: [cx, d, cz + e],
    bottom: [cx, -d, cz + e],
    front: [cx, e, cz + d],
    back: [cx, e, cz - d],
    left: [cx - d, e, cz],
    right: [cx + d, e, cz],
    // Unit direction (0.55, 0.7, 0.7) so iso frames the board at the same distance as the sides.
    iso: [cx + (d * 0.55) / ISO, (d * 0.7) / ISO, cz + (d * 0.7) / ISO],
  };
  const position = pos[p];
  return { position, target: [cx, 0, cz], distance: Math.hypot(position[0] - cx, position[1], position[2] - cz) };
}

/** Half height of an orthographic frustum that shows what a perspective camera at `distance` shows. */
export const orthoHalfHeight = (distance: number, fovDeg: number) => distance * Math.tan((fovDeg * Math.PI) / 360);

/** One row of the stack-up table, top to bottom: copper layers with the dielectric below each. */
export interface StackRow {
  kind: "copper" | "dielectric";
  name: string;
  /** Copper: "signal" or "plane"; dielectric: the material. */
  detail: string;
  /** Thickness in µm. */
  thicknessUm: number;
  /** Copper weight (oz) or relative permittivity (εr). */
  value: string;
}
export interface StackTable {
  /** False when the project has no stack-up: the 3D view then assumes 2 layers, 1.6 mm. */
  fromIr: boolean;
  boardThicknessMm: number;
  rows: StackRow[];
}

const OZ_UM = 35;

/** The stack-up exactly as the IR holds it (`SetStackup`); nothing is filled in. */
export function stackTable(s: Snapshot): StackTable {
  const st = s.project.pcb.board.stackup;
  if (!st || st.layers.length === 0) {
    return {
      fromIr: false,
      boardThicknessMm: 1.6,
      rows: stackLayers(undefined).map((l) => ({ kind: "copper", name: layerName(l.layer), detail: l.kind, thicknessUm: OZ_UM, value: "1 oz (assumed)" })),
    };
  }
  const rows: StackRow[] = [];
  st.layers.forEach((l, i) => {
    rows.push({ kind: "copper", name: layerName(l.layer), detail: l.kind ?? "signal", thicknessUm: l.copper_oz * OZ_UM, value: `${l.copper_oz} oz` });
    if (i < st.layers.length - 1) rows.push({ kind: "dielectric", name: `Dielectric ${i + 1}`, detail: st.dielectric, thicknessUm: l.dielectric_after_um, value: `εr ${l.er}` });
  });
  return { fromIr: true, boardThicknessMm: st.board_thickness / 1e6, rows };
}

/** Measured distance between two picked points, world coordinates converted back to PCB axes (mm). */
export interface Measure {
  distance: number;
  dx: number;
  dy: number;
  dz: number;
}
export function measure(a: V3, b: V3): Measure {
  // World X = PCB x, world Z = PCB y, world Y = height.
  const dx = b[0] - a[0];
  const dy = b[2] - a[2];
  const dz = b[1] - a[1];
  return { distance: Math.hypot(dx, dy, dz), dx, dy, dz };
}
export const measureText = (m: Measure) => `${m.distance.toFixed(3)} mm (Δx ${m.dx.toFixed(3)}, Δy ${m.dy.toFixed(3)}, Δz ${m.dz.toFixed(3)})`;

/** Solder-mask colours and surface finishes offered for the board (stored in the IR via `SetManufacturing`). */
export const MASK_COLOURS: Record<string, number> = {
  Green: 0x1f5d3a,
  Black: 0x1b1d20,
  Blue: 0x1d3f7a,
  Red: 0x8a1f1f,
  White: 0xdcdcd6,
  Yellow: 0xb89a1c,
  Purple: 0x4b2a6e,
};
export const FINISHES: Record<string, number> = {
  HASL: 0xc8c8c8,
  "Lead-free HASL": 0xc8c8c8,
  ENIG: 0xd8b45a,
  OSP: 0xc98a52,
  "Immersion silver": 0xdedede,
  "Immersion tin": 0xcfcfcf,
};

export interface Appearance {
  maskName: string;
  finishName: string;
  mask: number;
  pad: number;
  /** IR values that have no colour here (drawn with the default, and said so). */
  unknown: string[];
}
const keyOf = (rec: Record<string, number>, v: string) => Object.keys(rec).find((k) => k.toLowerCase() === v.trim().toLowerCase());
/** Board and pad colours from the IR's manufacturing settings; defaults (Green, HASL) when absent. */
export function appearance(s: Snapshot): Appearance {
  const m = s.project.manufacturing;
  const maskName = m?.solder_mask_color ?? "Green";
  const finishName = m?.surface_finish ?? "HASL";
  const mk = keyOf(MASK_COLOURS, maskName);
  const fk = keyOf(FINISHES, finishName);
  const unknown = [...(mk ? [] : [`mask colour "${maskName}"`]), ...(fk ? [] : [`finish "${finishName}"`])];
  return { maskName, finishName, mask: MASK_COLOURS[mk ?? "Green"], pad: FINISHES[fk ?? "HASL"], unknown };
}

/** Why a part has no real model on screen, or null when its model loaded. Shown on its placeholder. */
export function placeholderReason(hasModel: boolean, load: { ok: boolean; error?: string } | undefined, hasBody: boolean): string | null {
  if (hasModel && load?.ok) return null;
  const why = !hasModel ? "no 3D model attached" : !load ? "model loading…" : `model failed: ${load.error ?? "unknown error"}`;
  return hasBody ? `${why} · box is approximate` : `${why} · no body height, outline only`;
}
