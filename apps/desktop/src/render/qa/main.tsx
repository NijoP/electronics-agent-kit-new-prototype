// Render QA page (slice A4). Dev-server only (`render-qa.html`), driven over the Chrome DevTools
// Protocol by `e2e/render-qa.mjs` in CI:
//   - `__qa.snapshot(name)`: a reference board drawn by the Canvas2D renderer, as a PNG data URL;
//   - `__qa.diff(name, png)`: per-pixel comparison against a committed baseline (the reference-
//     render method of the EasyEDA viewer's QA, re-implemented without dependencies);
//   - `__qa.bench()`: the 2,000-footprint / 20,000-segment benchmark (pan and zoom frame rates);
//   - `?view=pcb&board=…&renderer=canvas|svg`: the real PCB editor component on a reference board,
//     for screenshots and real-input picking checks (`__qa.clientOf`, `__qa.selectionText`).
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../../design/tokens.css";
import "../../styles.css";
import type { EditCtx } from "../../EditContext";
import { layerColour, stackLayers } from "../../layers";
import { PcbView } from "../../PcbView";
import type { Command, Point, Snapshot } from "../../types";
import { fit, visibleViewBox, type Size } from "../../viewport";
import { Canvas2DRenderer, documentColour } from "../canvas2d";
import { RENDERER_KEY } from "../flag";
import { BENCH_FOOTPRINTS, BENCH_SEGMENTS, benchmarkBoard, REFERENCE_BOARDS } from "../fixtures";
import { buildPcbScene } from "../pcbScene";
import { viewTransform } from "../PcbSceneCanvas";
import type { Scene, ViewTransform } from "../types";

const SNAP: Size = { width: 800, height: 600 };
/** A channel difference above this counts the pixel as changed (antialiasing noise stays under). */
const CHANNEL_TOL = 48;

function boardOf(name: string): Snapshot {
  if (name === "benchmark") return benchmarkBoard();
  const b = REFERENCE_BOARDS.find((x) => x.name === name);
  if (!b) throw new Error(`unknown board ${name}`);
  return b.build();
}
const airwiresOf = (name: string): [string, Point, Point][] | null =>
  name === "ref-four-layer" ? [["GND", { x: 15_000_000, y: 15_000_000 }, { x: 42_000_000, y: 22_000_000 }]] : null;

function sceneOf(snap: Snapshot, name: string): Scene {
  const order = stackLayers(snap.project.pcb.board.stackup).map((s) => s.layer);
  return buildPcbScene(snap, { order, hidden: new Set(), colourOf: layerColour, activeLayer: "top", airwires: airwiresOf(name) });
}

function canvas(size: Size): { el: HTMLCanvasElement; ctx: CanvasRenderingContext2D } {
  const el = document.createElement("canvas");
  el.width = size.width;
  el.height = size.height;
  const ctx = el.getContext("2d");
  if (!ctx) throw new Error("no 2D canvas context");
  return { el, ctx };
}

function fitView(scene: Scene, size: Size): ViewTransform {
  const b = scene.bounds ?? { minX: 0, minY: -10, maxX: 10, maxY: 0 };
  return viewTransform(visibleViewBox(fit(b), size), size, 1);
}

/** Draw a board as the PCB view shows it: canvas background, board fill, then the scene. */
function drawBoard(name: string): HTMLCanvasElement {
  const snap = boardOf(name);
  const scene = sceneOf(snap, name);
  const view = fitView(scene, SNAP);
  const out = canvas(SNAP);
  out.ctx.fillStyle = documentColour("--bg-canvas");
  out.ctx.fillRect(0, 0, SNAP.width, SNAP.height);
  const outline = snap.project.pcb.board.outline;
  if (outline.length) {
    const k = view.scale;
    out.ctx.beginPath();
    outline.forEach((p, i) => {
      const x = (p.x / 1e6 - view.x0) * k;
      const y = (-p.y / 1e6 - view.y0) * k;
      if (i === 0) out.ctx.moveTo(x, y);
      else out.ctx.lineTo(x, y);
    });
    out.ctx.closePath();
    out.ctx.fillStyle = documentColour("--c-board");
    out.ctx.fill();
  }
  const layer = canvas(SNAP);
  const r = new Canvas2DRenderer(layer.ctx);
  r.setScene(scene);
  r.render(view);
  out.ctx.drawImage(layer.el, 0, 0);
  return out.el;
}

function load(src: string): Promise<HTMLImageElement> {
  return new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = () => rej(new Error("baseline image failed to decode"));
    img.src = src;
  });
}

async function diff(name: string, baseline: string) {
  const cur = drawBoard(name);
  const img = await load(baseline);
  const w = cur.width;
  const h = cur.height;
  if (img.naturalWidth !== w || img.naturalHeight !== h) return { name, width: w, height: h, sizeMatch: false, mismatched: w * h, ratio: 1, diffPng: "" };
  const base = canvas({ width: w, height: h });
  base.ctx.drawImage(img, 0, 0);
  const a = cur.getContext("2d")!.getImageData(0, 0, w, h).data;
  const b = base.ctx.getImageData(0, 0, w, h).data;
  const out = canvas({ width: w, height: h });
  const d = out.ctx.createImageData(w, h);
  let mismatched = 0;
  for (let i = 0; i < a.length; i += 4) {
    const changed = Math.max(Math.abs(a[i] - b[i]), Math.abs(a[i + 1] - b[i + 1]), Math.abs(a[i + 2] - b[i + 2]), Math.abs(a[i + 3] - b[i + 3])) > CHANNEL_TOL;
    if (changed) mismatched++;
    // Diff image: changed pixels red, unchanged ones a dim grey copy of the baseline.
    const g = Math.round((b[i] + b[i + 1] + b[i + 2]) / 12);
    d.data[i] = changed ? 255 : g;
    d.data[i + 1] = changed ? 0 : g;
    d.data[i + 2] = changed ? 0 : g;
    d.data[i + 3] = 255;
  }
  out.ctx.putImageData(d, 0, 0);
  return { name, width: w, height: h, sizeMatch: true, mismatched, ratio: mismatched / (w * h), diffPng: out.el.toDataURL("image/png") };
}

const frame = () => new Promise<number>((res) => requestAnimationFrame(res));

interface RunResult {
  scenario: string;
  frames: number;
  fps: number;
  p95FrameMs: number;
  meanRenderMs: number;
  maxRenderMs: number;
  fullLayersPerFrame: number;
}

/** The A4 benchmark: frame rates while panning and zooming the target-size board. */
async function bench() {
  const size: Size = { width: 1600, height: 1000 };
  const t0 = performance.now();
  const snap = benchmarkBoard();
  const t1 = performance.now();
  const scene = sceneOf(snap, "benchmark");
  const t2 = performance.now();
  const view = canvas(size);
  view.el.style.cssText = "position:fixed;left:0;top:0;width:1600px;height:1000px;z-index:10;background:#0d0f11";
  document.body.appendChild(view.el);
  const r = new Canvas2DRenderer(view.ctx);
  r.setScene(scene);
  const t3 = performance.now();
  const fitV = fitView(scene, size);
  const first = r.render(fitV);
  await frame();

  const run = async (scenario: string, ms: number, step: (i: number) => ViewTransform): Promise<RunResult> => {
    const renderMs: number[] = [];
    const intervals: number[] = [];
    let full = 0;
    let i = 0;
    const start = await frame();
    let last = start;
    for (;;) {
      const s = r.render(step(i++));
      renderMs.push(s.ms);
      full += s.fullLayers;
      const t = await frame();
      intervals.push(t - last);
      last = t;
      if (t - start >= ms) break;
    }
    const sorted = [...intervals].sort((x, y) => x - y);
    return {
      scenario,
      frames: intervals.length,
      fps: intervals.length / ((last - start) / 1000),
      p95FrameMs: sorted[Math.floor(sorted.length * 0.95)] ?? 0,
      meanRenderMs: renderMs.reduce((x, y) => x + y, 0) / renderMs.length,
      maxRenderMs: Math.max(...renderMs),
      fullLayersPerFrame: full / renderMs.length,
    };
  };
  // Same centre, new scale.
  const at = (v: ViewTransform, scale: number): ViewTransform => {
    const cx = v.x0 + v.width / v.scale / 2;
    const cy = v.y0 + v.height / v.scale / 2;
    return { ...v, scale, x0: cx - v.width / scale / 2, y0: cy - v.height / scale / 2 };
  };
  const pan =
    (v: ViewTransform, px: number) =>
    (i: number): ViewTransform => ({ ...v, x0: v.x0 + ((i % 200) - 100) * (px / v.scale) });
  const zoom =
    (v: ViewTransform) =>
    (i: number): ViewTransform =>
      at(v, v.scale * (1 + 0.35 * Math.sin(i / 12)));
  const editing = at(fitV, 20); // a typical editing zoom: 20 px per mm
  const runs: RunResult[] = [];
  runs.push(await run("pan, whole board in view (2 px steps)", 3000, pan(fitV, 2)));
  runs.push(await run("zoom, whole board in view", 3000, zoom(fitV)));
  runs.push(await run("pan, editing zoom 20 px/mm (2 px steps)", 3000, pan(editing, 2)));
  runs.push(await run("zoom, editing zoom around 20 px/mm", 3000, zoom(editing)));
  runs.push(await run("pan, whole board, sub-pixel steps (no cache reuse)", 3000, pan(fitV, 2.5)));

  // Picking cost over the full table.
  const b = scene.bounds!;
  let seed = 7;
  const rnd = () => {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  };
  const tp = performance.now();
  let hits = 0;
  for (let i = 0; i < 2000; i++) if (r.pick(b.minX + rnd() * (b.maxX - b.minX), b.minY + rnd() * (b.maxY - b.minY), { tol: 0.15 })) hits++;
  const pickUs = ((performance.now() - tp) / 2000) * 1000;

  // One edit: move one track, rebuild the scene, redraw (a dirty rectangle, not a full redraw).
  r.render(editing);
  const te = performance.now();
  // A track inside the editing view, so the edit shows as dirty rectangles on screen.
  const inView = (p: Point) => {
    const x = p.x / 1e6;
    const y = -p.y / 1e6;
    return x > editing.x0 && x < editing.x0 + editing.width / editing.scale && y > editing.y0 && y < editing.y0 + editing.height / editing.scale;
  };
  const tracks = Object.values(snap.project.pcb.tracks);
  const t = tracks.find((k) => inView(k.a) && inView(k.b)) ?? tracks[0];
  t.b = { x: t.b.x + 500_000, y: t.b.y };
  const edited = sceneOf(snap, "benchmark");
  const tb = performance.now();
  r.setScene(edited);
  const after = r.render(editing);
  const tr = performance.now();
  view.el.remove();
  return {
    board: { footprints: BENCH_FOOTPRINTS, segments: BENCH_SEGMENTS, renderObjects: scene.objects.length },
    viewport: `${size.width}x${size.height} CSS px, devicePixelRatio 1`,
    userAgent: navigator.userAgent,
    hardwareConcurrency: navigator.hardwareConcurrency,
    deviceMemoryGb: (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? null,
    ms: { generateBoard: t1 - t0, buildScene: t2 - t1, setScene: t3 - t2, firstFullRender: first.ms },
    runs,
    pick: { calls: 2000, hits, meanUs: pickUs },
    edit: { rebuildSceneMs: tb - te, setSceneAndRenderMs: tr - tb, stats: after },
  };
}

// ---- PCB editor view (screenshots and real-input picking) ----
const sent: Command[] = [];
function mountPcb(name: string, renderer: string) {
  try {
    localStorage.setItem(RENDERER_KEY, renderer === "svg" ? "svg" : "canvas");
  } catch {
    /* storage unavailable: the default renderer is used */
  }
  const snap = boardOf(name);
  const edit: EditCtx = {
    execute: async (c) => {
      sent.push(...(Array.isArray(c) ? c : [c]));
    },
    busy: false,
    library: [],
    notify: (m) => console.info(`notify: ${m}`),
  };
  const host = document.getElementById("root")!;
  host.style.cssText = "width:1600px;height:1000px;display:grid";
  createRoot(host).render(
    <StrictMode>
      <PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit} airwires={airwiresOf(name)} />
    </StrictMode>,
  );
}

/** Client pixel of a board point (mm, Y up) on the PCB canvas, for real-input clicks. */
function clientOf(x: number, y: number): [number, number] | null {
  const svg = document.querySelector("svg.canvas");
  const vb = svg?.getAttribute("viewBox")?.split(" ").map(Number);
  if (!svg || !vb || vb.length !== 4) return null;
  const r = svg.getBoundingClientRect();
  const s = Math.min(r.width / vb[2], r.height / vb[3]);
  const ox = (r.width - vb[2] * s) / 2;
  const oy = (r.height - vb[3] * s) / 2;
  return [r.left + ox + (x - vb[0]) * s, r.top + oy + (-y - vb[1]) * s];
}

const params = new URLSearchParams(location.search);
const qa = {
  ready: false,
  boards: REFERENCE_BOARDS.map((b) => b.name),
  snapshot: async (name: string) => drawBoard(name).toDataURL("image/png"),
  diff,
  bench,
  clientOf,
  sent: () => sent.map((c) => c.type),
  selectionText: () => document.querySelector('[aria-label="Footprint selection"]')?.textContent ?? null,
  renderer: () => (document.querySelector('canvas[data-renderer="canvas2d"]') ? "canvas" : document.querySelector("[data-footprint-id]") ? "svg" : "none"),
};
(window as unknown as { __qa: typeof qa }).__qa = qa;
if (params.get("view") === "pcb") mountPcb(params.get("board") ?? "ref-four-layer", params.get("renderer") ?? "canvas");
void frame()
  .then(frame)
  .then(() => {
    qa.ready = true;
  });
