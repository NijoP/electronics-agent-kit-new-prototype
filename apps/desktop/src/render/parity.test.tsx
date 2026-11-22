// Picking parity (slice A4): the PCB editor driven through the Canvas2D renderer gives the same
// engine commands as the SVG path. The scenarios mirror `tools/pcb/PcbSelect.test.tsx` and the PCB
// delete/track-selection flows, but every pointer event goes to the canvas element at a view
// coordinate: there are no footprint, track, via or zone DOM nodes to target, so what is hit comes
// from the render-object pick table. The SVG-path tests themselves run unchanged (jsdom has no 2D
// context, so they get the SVG fallback).
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "../EditContext";
import { fixture } from "../fixture.test-data";
import { PcbView, pcbBounds } from "../PcbView";
import type { Command, Snapshot } from "../types";
import { fit, type ViewBox } from "../viewport";
import { RENDERER_KEY, setCanvas2dAvailableForTests } from "./flag";
import { stubContext, type Call } from "./stubContext.test-util";

const SIZE = 1000;
const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });
let sent: Command[];
let notify: ReturnType<typeof vi.fn<(m: string) => void>>;
let drawn: Call[];
let contexts = 0;
const realGetContext = HTMLCanvasElement.prototype.getContext;
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(...(Array.isArray(c) ? c : [c]));
  },
  busy: false,
  library: [],
  notify,
});

beforeEach(() => {
  sent = [];
  drawn = [];
  contexts = 0;
  notify = vi.fn<(m: string) => void>();
  localStorage.clear();
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
  // A pane that measures itself (Canvas.tsx measures once, then observes).
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
      unobserve() {}
    },
  );
  setCanvas2dAvailableForTests(true);
  HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement) {
    contexts++;
    return stubContext(this.width, this.height, drawn).ctx;
  } as unknown as typeof HTMLCanvasElement.prototype.getContext;
});
afterEach(() => {
  cleanup();
  HTMLCanvasElement.prototype.getContext = realGetContext;
  setCanvas2dAvailableForTests(null);
  vi.unstubAllGlobals();
});

/** Three footprints in a row: R1 (10,10), D1 (20,10), R2 (40,14); no board outline. */
function board(): Snapshot {
  const s = fixture();
  const r1 = s.project.pcb.footprints.fp_r1;
  s.project.pcb.footprints.fp_r2 = { ...r1, id: "fp_r2", symbol_id: "sym_r2", refdes: "R2", position: pt(40, 14) };
  s.project.pcb.board.outline = [];
  const g1 = s.geometry.footprints.find((f) => f.id === "fp_r1")!;
  s.geometry.footprints.push({ ...g1, id: "fp_r2", symbol_id: "sym_r2", refdes: "R2", position: pt(40, 14), pads: [] });
  return s;
}

let vb: ViewBox;
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
type Mods = { shiftKey?: boolean; ctrlKey?: boolean };
function dragOn(t: Element, from: [number, number], to: [number, number], mods: Mods = {}) {
  fireEvent.pointerDown(t, { ...client(...from), button: 0, buttons: 1, ...mods });
  fireEvent.pointerMove(t, { ...client((from[0] + to[0]) / 2, (from[1] + to[1]) / 2), buttons: 1 });
  fireEvent.pointerMove(t, { ...client(...to), buttons: 1 });
  fireEvent.pointerUp(t, { ...client(...to), button: 0 });
}
function clickOn(t: Element, at: [number, number], mods: Mods = {}) {
  fireEvent.pointerDown(t, { ...client(...at), button: 0, buttons: 1, ...mods });
  fireEvent.pointerUp(t, { ...client(...at), button: 0, ...mods });
}

function setup(snap = board()) {
  vb = fit(pcbBounds(snap));
  const r = render(<PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />);
  const svg = r.container.querySelector("svg.canvas")!;
  return { ...r, svg, snap };
}
const moves = () => sent.flatMap((c) => (c.type === "Batch" ? c.commands : [c]));

describe("Canvas2D PCB renderer", () => {
  it("paints the board on a canvas instead of SVG nodes and keeps the interactive SVG on top", async () => {
    const { container } = setup(fixture());
    expect(container.querySelector('canvas[data-renderer="canvas2d"]')).toBeTruthy();
    expect(container.querySelector("[data-footprint-id]")).toBeNull();
    expect(container.querySelector("[data-track-id]")).toBeNull();
    expect(container.querySelector(".canvas-stack > svg.canvas.canvas-top")).toBeTruthy();
    expect((screen.getByLabelText("PCB renderer") as HTMLSelectElement).value).toBe("canvas");
    const errors: string[] = [];
    vi.spyOn(console, "error").mockImplementation((...a: unknown[]) => void errors.push(a.map((x) => (x instanceof Error ? (x.stack ?? x.message) : String(x))).join(" ")));
    const el = container.querySelector("canvas.scene-canvas") as HTMLCanvasElement;
    // Each stage of the first frame, so a failure says where the pipeline stopped.
    await waitFor(() => expect(el.width, "scene canvas sized to the measured pane").toBe(SIZE));
    expect(contexts, "2D contexts requested").toBeGreaterThan(0);
    await waitFor(() => expect(drawn.some((c) => c.fn === "drawImage") ? "drawImage" : `no drawImage; first calls: ${drawn.map((c) => c.fn).slice(0, 12).join(",")}; errors: ${errors.join(" | ").slice(0, 1500)}`).toBe("drawImage"));
    expect(drawn.some((c) => c.fn === "fillText" && c.args[0] === "R1")).toBe(true);
  });

  it("the renderer flag switches back to the SVG markup and is remembered", () => {
    const { container } = setup(fixture());
    fireEvent.change(screen.getByLabelText("PCB renderer"), { target: { value: "svg" } });
    expect(container.querySelector('[data-footprint-id="fp_r1"]')).toBeTruthy();
    expect(container.querySelector("canvas.scene-canvas")).toBeNull();
    expect(localStorage.getItem(RENDERER_KEY)).toBe("svg");
  });

  it("without a 2D context the SVG path is used and Canvas2D is shown as unavailable", () => {
    setCanvas2dAvailableForTests(false);
    const { container } = setup(fixture());
    expect(container.querySelector('[data-footprint-id="fp_r1"]')).toBeTruthy();
    const opt = screen.getByRole("option", { name: "Canvas2D (unavailable)" }) as HTMLOptionElement;
    expect(opt.disabled).toBe(true);
  });
});

describe("picking parity with the SVG path (PcbSelect scenarios on the canvas)", () => {
  it("dragging one footprint moves only it, snapped to the grid, without panning", () => {
    const { svg } = setup();
    const before = svg.getAttribute("viewBox");
    dragOn(svg, [10, 10], [12.1, 10]);
    expect(svg.getAttribute("viewBox")).toBe(before);
    expect(sent).toEqual([{ type: "MoveFootprint", footprint_id: "fp_r1", position: pt(12, 10), rotation: null, side: null }]);
  });

  it("Move tool: dragging a footprint moves only it", () => {
    const { svg } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Move" }));
    dragOn(svg, [20, 10], [20, 13]);
    expect(moves().map((c) => c.type === "MoveFootprint" && c.footprint_id)).toEqual(["fp_d1"]);
  });

  it("Shift adds, Ctrl toggles; a group drag is one Batch", () => {
    const { svg } = setup();
    clickOn(svg, [10, 10]);
    clickOn(svg, [20, 10], { shiftKey: true });
    clickOn(svg, [40, 14], { ctrlKey: true });
    clickOn(svg, [40, 14], { ctrlKey: true }); // toggled back out
    expect(screen.getByText(/2 footprints selected/)).toBeTruthy();
    dragOn(svg, [20, 10], [20, 11]);
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({ type: "Batch", label: "Move 2 footprints" });
    expect(moves().map((c) => c.type === "MoveFootprint" && c.footprint_id)).toEqual(["fp_r1", "fp_d1"]);
  });

  it("marquee selects footprints whose courtyard is inside; align/distribute/lock buttons send commands", () => {
    const { svg } = setup();
    dragOn(svg, [5, 20], [45, 5]);
    expect(screen.getByText(/3 footprints selected/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Align top" }));
    fireEvent.click(screen.getByRole("button", { name: "Distribute H" }));
    fireEvent.click(screen.getByRole("button", { name: "Lock" }));
    expect(sent.map((c) => (c.type === "Batch" ? c.label : c.type))).toEqual(["Align top", "MoveFootprint", "Lock 3 footprints"]);
  });

  it("a locked footprint is not dragged and the user is told why", () => {
    const snap = board();
    snap.project.pcb.footprints.fp_r1.locked = true;
    const { svg } = setup(snap);
    dragOn(svg, [10, 10], [15, 10]);
    expect(sent).toEqual([]);
    expect(notify).toHaveBeenCalledWith("1 footprint locked; unlock to move.");
  });

  it("R rotates and F flips the selection", () => {
    const { svg } = setup();
    clickOn(svg, [20, 10]);
    fireEvent.keyDown(window, { key: "r" });
    fireEvent.keyDown(window, { key: "f" });
    expect(sent).toEqual([
      { type: "MoveFootprint", footprint_id: "fp_d1", position: pt(20, 10), rotation: "R90", side: null },
      { type: "MoveFootprint", footprint_id: "fp_d1", position: pt(20, 10), rotation: null, side: "bottom" },
    ]);
  });

  it("an empty-board click clears the selection", () => {
    const { svg } = setup();
    clickOn(svg, [10, 10]);
    expect(screen.getByText(/R1 · top/)).toBeTruthy();
    clickOn(svg, [30, 4]);
    expect(screen.queryByRole("group", { name: "Footprint selection" })).toBeNull();
  });
});

describe("picking parity: delete tool and track selection", () => {
  it("Delete picks the via over the zone, the track over the zone, and the zone alone", () => {
    const { svg } = setup(fixture());
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    clickOn(svg, [15, 15]);
    clickOn(svg, [15, 10]);
    clickOn(svg, [5, 15]);
    expect(sent).toEqual([
      { type: "DeleteVia", via_id: "via_1" },
      { type: "DeleteTrack", track_id: "trk_1" },
      { type: "DeleteZone", zone_id: "zon_1" },
    ]);
  });

  it("Select picks a track for editing", () => {
    const { svg } = setup(fixture());
    clickOn(svg, [15, 10]);
    const bar = screen.getByRole("group", { name: "Track selection" });
    expect(bar.textContent).toMatch(/track N1 · F\.Cu/);
  });
});
