// Deterministic boards for the renderer: three reference boards for the pixel-diff snapshots and
// the benchmark board (2,000 footprints and 20,000 track segments, the target size from the
// archived EDA desktop architecture). Synthetic engine-shaped snapshots; no project files.
import type { CopperLayer, FootprintGeom, FootprintInstance, PadGeom, Point, Snapshot, Stackup, Track, Via, Zone } from "../types";

const MM = 1_000_000;
const pt = (x: number, y: number): Point => ({ x: Math.round(x * MM), y: Math.round(y * MM) });

/** Small deterministic PRNG (mulberry32) so every run builds the same board. */
function rng(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function stackup(n: 2 | 4): Stackup {
  const layer = (l: CopperLayer, kind: "signal" | "plane" = "signal") => ({ layer: l, kind, copper_oz: 1, dielectric_after_um: 200, er: 4.5 });
  const layers = n === 2 ? [layer("top"), layer("bottom")] : [layer("top"), layer({ inner: 1 }, "plane"), layer({ inner: 2 }), layer("bottom")];
  return { layers, board_thickness: 1.6 * MM, dielectric: "FR-4" };
}

interface Part {
  inst: FootprintInstance;
  geom: FootprintGeom;
}
interface Board {
  name: string;
  outline: Point[];
  layers: 2 | 4;
  footprints: Part[];
  tracks: Track[];
  vias: Via[];
  zones: Zone[];
}

function rec<T extends { id: string }>(xs: T[]): Record<string, T> {
  const out: Record<string, T> = {};
  for (const x of xs) out[x.id] = x;
  return out;
}

function snapshot(b: Board): Snapshot {
  return {
    project: {
      schema_version: 1,
      id: `prj_${b.name}`,
      name: b.name,
      revision: 1,
      schematic: { sheets: {}, symbols: {}, wires: {}, labels: {}, no_connects: {} },
      pcb: {
        board: { outline: b.outline, stackup: stackup(b.layers) },
        footprints: rec(b.footprints.map((f) => f.inst)),
        tracks: rec(b.tracks),
        vias: rec(b.vias),
        zones: rec(b.zones),
      },
    },
    dir: null,
    can_undo: false,
    can_redo: false,
    blocked: null,
    recovery: { reconciled: [], torn_temp_file_removed: false },
    geometry: { symbols: [], footprints: b.footprints.map((f) => f.geom) },
  };
}

const rect = (w: number, h: number): Point[] => [pt(0, 0), pt(w, 0), pt(w, h), pt(0, h)];

/** A footprint at (x, y) mm; 2 pads = a chip part, more = two rows (SOIC-like, or DIP when THT). */
function footprint(i: number, x: number, y: number, pads: number, side: "top" | "bottom", tht: boolean, nets: (k: number) => string | null): Part {
  const id = `fp_${i}`;
  const pitch = tht ? 2.54 : 1.0;
  const perRow = Math.max(1, Math.ceil(pads / 2));
  const w = tht ? 1.6 : 0.8;
  const h = tht ? 1.6 : 0.9;
  const ps: PadGeom[] = [];
  for (let k = 0; k < pads; k++) {
    const row = k < perRow ? 0 : 1;
    const col = row === 0 ? k : k - perRow;
    const px = pads === 2 ? x + (k === 0 ? -0.8 : 0.8) : x + (col - (perRow - 1) / 2) * pitch;
    const py = pads === 2 ? y : y + (row === 0 ? -1 : 1) * (tht ? 3.81 : 1.6);
    ps.push({
      number: String(k + 1),
      kind: tht ? "tht" : "smd",
      shape: tht ? (k === 0 ? "rect" : "circle") : "rect",
      position: pt(px, py),
      size_x: Math.round(w * MM),
      size_y: Math.round(h * MM),
      drill: tht ? Math.round(0.8 * MM) : null,
      net: nets(k),
    });
  }
  const halfW = pads === 2 ? 1.5 : (perRow * pitch) / 2 + 0.5;
  const halfH = pads === 2 ? 0.8 : tht ? 5 : 2.5;
  const refdes = `${pads === 2 ? "R" : tht ? "J" : "U"}${i + 1}`;
  const component = pads === 2 ? "R_0603" : tht ? "DIP" : "SOIC";
  return {
    inst: { id, symbol_id: `sym_${i}`, component_id: component, refdes, position: pt(x, y), rotation: "R0", side, locked: false },
    geom: { id, symbol_id: `sym_${i}`, refdes, side, position: pt(x, y), half_w: Math.round(halfW * MM), half_h: Math.round(halfH * MM), pads: ps },
  };
}

const pick = (names: string[]) => (k: number): string | null => names[k] ?? null;

/** Reference board 1: two layers, two parts, one track, one via, one bottom pour. */
export function referenceBasic(): Snapshot {
  return snapshot({
    name: "ref-basic",
    outline: rect(30, 20),
    layers: 2,
    footprints: [footprint(0, 10, 10, 2, "top", false, pick(["N1", "GND"])), footprint(1, 20, 10, 2, "top", false, pick(["N1", "VCC"]))],
    tracks: [{ id: "trk_1", layer: "top", width: 0.25 * MM, a: pt(10.8, 10), b: pt(19.2, 10), net: "N1" }],
    vias: [{ id: "via_1", position: pt(15, 15), drill: 0.3 * MM, diameter: 0.6 * MM, net: "GND" }],
    zones: [{ id: "zon_1", layer: "bottom", net: "GND", outline: [pt(1, 1), pt(29, 1), pt(29, 19), pt(1, 19)], clearance: 0.3 * MM }],
  });
}

/** Reference board 2: four layers, a plane, bottom-side and through-hole parts, inner routing. */
export function referenceFourLayer(): Snapshot {
  const fps = [
    footprint(0, 12, 12, 8, "top", false, pick(["SDA", "SCL", "VCC", "GND", "IO1", "IO2", "IO3", "IO4"])),
    footprint(1, 34, 12, 2, "top", false, pick(["SDA", "VCC"])),
    footprint(2, 34, 22, 2, "bottom", false, pick(["SCL", "GND"])),
    footprint(3, 20, 30, 6, "top", true, pick(["IO1", "IO2", "IO3", "IO4", "VCC", "GND"])),
  ];
  const tracks: Track[] = [
    { id: "t1", layer: "top", width: 0.2 * MM, a: pt(10.5, 10.4), b: pt(33.2, 10.4), net: "SDA" },
    { id: "t2", layer: "top", width: 0.2 * MM, a: pt(33.2, 10.4), b: pt(33.2, 12), net: "SDA" },
    { id: "t3", layer: { inner: 2 }, width: 0.15 * MM, a: pt(11.5, 10.4), b: pt(26, 22), net: "SCL" },
    { id: "t4", layer: { inner: 2 }, width: 0.15 * MM, a: pt(26, 22), b: pt(33.2, 22), net: "SCL" },
    { id: "t5", layer: "bottom", width: 0.4 * MM, a: pt(34.8, 22), b: pt(42, 22), net: "GND" },
    { id: "t6", layer: "top", width: 0.5 * MM, a: pt(12.5, 10.4), b: pt(12.5, 4), net: "VCC" },
    { id: "t7", layer: "top", width: 0.5 * MM, a: pt(12.5, 4), b: pt(34.8, 4), net: "VCC" },
    { id: "t8", layer: "top", width: 0.5 * MM, a: pt(34.8, 4), b: pt(34.8, 12), net: "VCC" },
  ];
  const vias: Via[] = [
    { id: "v1", position: pt(26, 22), drill: 0.3 * MM, diameter: 0.6 * MM, net: "SCL", from: "top", to: { inner: 2 } },
    { id: "v2", position: pt(42, 22), drill: 0.3 * MM, diameter: 0.6 * MM, net: "GND" },
  ];
  const zones: Zone[] = [
    { id: "z1", layer: { inner: 1 }, net: "GND", outline: [pt(1, 1), pt(49, 1), pt(49, 39), pt(1, 39)], clearance: 0.3 * MM },
    { id: "z2", layer: "top", net: "GND", outline: [pt(38, 28), pt(48, 28), pt(48, 38), pt(38, 38)], clearance: 0.3 * MM },
  ];
  return snapshot({ name: "ref-four-layer", outline: rect(50, 40), layers: 4, footprints: fps, tracks, vias, zones });
}

/**
 * A dense generated board: `n` footprints on a grid and `segments` track segments routed as
 * short Manhattan/45° runs over four copper layers, one via per ten segments, an inner plane.
 */
export function generatedBoard(n: number, segments: number, seed = 1): Snapshot {
  const r = rng(seed);
  const cols = Math.ceil(Math.sqrt(n * 1.5));
  const rows = Math.ceil(n / cols);
  const pitchX = 6;
  const pitchY = 5;
  const W = cols * pitchX + 10;
  const H = rows * pitchY + 10;
  const nets = Array.from({ length: Math.max(8, Math.floor(n / 2)) }, (_, i) => `N${i}`);
  const net = () => nets[Math.floor(r() * nets.length)] ?? "N0";
  const fps: Part[] = [];
  for (let i = 0; i < n; i++) {
    const x = 5 + (i % cols) * pitchX + pitchX / 2;
    const y = 5 + Math.floor(i / cols) * pitchY + pitchY / 2;
    const kind = r();
    const pads = kind < 0.75 ? 2 : kind < 0.95 ? 8 : 4;
    fps.push(footprint(i, x, y, pads, r() < 0.25 ? "bottom" : "top", pads === 4, () => net()));
  }
  const layers: CopperLayer[] = ["top", { inner: 1 }, { inner: 2 }, "bottom"];
  const tracks: Track[] = [];
  const vias: Via[] = [];
  let x = W / 2;
  let y = H / 2;
  let n0 = net();
  let layer: CopperLayer = "top";
  const DX = [1, 1, 0, -1, -1, -1, 0, 1];
  const DY = [0, 1, 1, 1, 0, -1, -1, -1];
  for (let i = 0; i < segments; i++) {
    if (i % 12 === 0) {
      x = 5 + r() * (W - 10);
      y = 5 + r() * (H - 10);
      n0 = net();
      layer = layers[Math.floor(r() * layers.length)] ?? "top";
    }
    const dir = Math.floor(r() * 8);
    const len = 0.5 + r() * 4;
    const nx = Math.min(W - 1, Math.max(1, x + (DX[dir] ?? 0) * len));
    const ny = Math.min(H - 1, Math.max(1, y + (DY[dir] ?? 0) * len));
    tracks.push({ id: `t${i}`, layer, width: Math.round((r() < 0.8 ? 0.2 : 0.5) * MM), a: pt(x, y), b: pt(nx, ny), net: n0 });
    if (i % 10 === 9) vias.push({ id: `v${i}`, position: pt(nx, ny), drill: 0.3 * MM, diameter: 0.6 * MM, net: n0 });
    x = nx;
    y = ny;
  }
  const zones: Zone[] = [{ id: "z_in1", layer: { inner: 1 }, net: "GND", outline: [pt(1, 1), pt(W - 1, 1), pt(W - 1, H - 1), pt(1, H - 1)], clearance: 0.3 * MM }];
  return snapshot({ name: `generated-${n}x${segments}`, outline: rect(W, H), layers: 4, footprints: fps, tracks, vias, zones });
}

/** Reference board 3: a dense generated board (120 footprints, 1,200 segments). */
export const referenceDense = () => generatedBoard(120, 1200, 7);

/** The A4 benchmark board: 2,000 footprints, 20,000 track segments. */
export const BENCH_FOOTPRINTS = 2000;
export const BENCH_SEGMENTS = 20000;
export const benchmarkBoard = () => generatedBoard(BENCH_FOOTPRINTS, BENCH_SEGMENTS, 42);

export const REFERENCE_BOARDS: { name: string; build: () => Snapshot }[] = [
  { name: "ref-basic", build: referenceBasic },
  { name: "ref-four-layer", build: referenceFourLayer },
  { name: "ref-dense", build: referenceDense },
];
