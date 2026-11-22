// PCB footprint manipulation (directive 3.3): independent select and move, marquee,
// Shift/Ctrl selection, rotate, flip, align, distribute, lock. Before this change a drag on a
// footprint panned the whole board (every footprint slid together) and nothing was sent.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { GridControls } from "../../grid";
import { PcbView, pcbBounds } from "../../PcbView";
import type { Command, Snapshot } from "../../types";
import { fit, type ViewBox } from "../../viewport";
import { alignCommands, distributeCommands, lockCommands, marquee, moveCommands, pick, rectOf } from "./selection";

const SIZE = 1000;
const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });
let sent: Command[];
let notify: ReturnType<typeof vi.fn<(m: string) => void>>;
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
  notify = vi.fn<(m: string) => void>();
  localStorage.clear();
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

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
  // The grid combos live in the main toolbar; they share the PCB grid store with the editor.
  const r = render(
    <>
      <GridControls kind="pcb" />
      <PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />
    </>,
  );
  const svg = r.container.querySelector("svg.canvas")!;
  const court = (id: string) => r.container.querySelector(`[data-footprint-id="${id}"] rect.courtyard`)!;
  return { ...r, svg, court, snap };
}
const moves = () => sent.flatMap((c) => (c.type === "Batch" ? c.commands : [c]));

describe("PCB footprints move independently", () => {
  it("dragging one footprint moves only it, snapped to the grid, without panning", () => {
    const { svg, court } = setup();
    const before = svg.getAttribute("viewBox");
    dragOn(court("fp_r1"), [10, 10], [12.1, 10]);
    expect(svg.getAttribute("viewBox")).toBe(before);
    expect(sent).toEqual([{ type: "MoveFootprint", footprint_id: "fp_r1", position: pt(12, 10), rotation: null, side: null }]);
  });

  it("Move tool: dragging a footprint moves only it", () => {
    const { court } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Move" }));
    dragOn(court("fp_d1"), [20, 10], [20, 13]);
    expect(moves().map((c) => c.type === "MoveFootprint" && c.footprint_id)).toEqual(["fp_d1"]);
  });

  it("Shift adds, Ctrl toggles; a group drag is one Batch", () => {
    const { court } = setup();
    clickOn(court("fp_r1"), [10, 10]);
    clickOn(court("fp_d1"), [20, 10], { shiftKey: true });
    clickOn(court("fp_r2"), [40, 14], { ctrlKey: true });
    clickOn(court("fp_r2"), [40, 14], { ctrlKey: true }); // toggled back out
    expect(screen.getByText(/2 footprints selected/)).toBeTruthy();
    dragOn(court("fp_d1"), [20, 10], [20, 11]);
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
    // Distributing three footprints moves only the middle one: a single command, no Batch.
    expect(sent.map((c) => (c.type === "Batch" ? c.label : c.type))).toEqual(["Align top", "MoveFootprint", "Lock 3 footprints"]);
  });

  it("a locked footprint is not dragged and the user is told why", () => {
    const snap = board();
    snap.project.pcb.footprints.fp_r1.locked = true;
    const { court, container } = setup(snap);
    expect(container.querySelector('[data-footprint-id="fp_r1"]')!.classList.contains("locked")).toBe(true);
    dragOn(court("fp_r1"), [10, 10], [15, 10]);
    expect(sent).toEqual([]);
    expect(notify).toHaveBeenCalledWith("1 footprint locked; unlock to move.");
    expect(screen.getByRole("button", { name: "Lock" })).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("button", { name: "Unlock" }));
    expect(sent).toEqual([{ type: "SetFootprintLocked", footprint_id: "fp_r1", locked: false }]);
  });

  it("R rotates and F flips the selection", () => {
    const { court } = setup();
    clickOn(court("fp_d1"), [20, 10]);
    fireEvent.keyDown(window, { key: "r" });
    fireEvent.keyDown(window, { key: "f" });
    expect(sent).toEqual([
      { type: "MoveFootprint", footprint_id: "fp_d1", position: pt(20, 10), rotation: "R90", side: null },
      { type: "MoveFootprint", footprint_id: "fp_d1", position: pt(20, 10), rotation: null, side: "bottom" },
    ]);
  });

  it("draws a grid, rulers and the cursor coordinates; spacing feeds the snap", () => {
    const { container, svg, court } = setup();
    expect(container.querySelector("g.grid-pcb")).toBeTruthy();
    expect(container.querySelector("g.rulers")).toBeTruthy();
    fireEvent.pointerMove(svg, client(12.5, 7.25));
    expect(screen.getByLabelText("Cursor position").textContent).toMatch(/X 12\.5\d* +Y 7\.2\d* mm/);
    fireEvent.change(screen.getByLabelText("Grid spacing"), { target: { value: "1" } });
    dragOn(court("fp_r1"), [10, 10], [11.4, 10]);
    expect(sent).toEqual([{ type: "MoveFootprint", footprint_id: "fp_r1", position: pt(11, 10), rotation: null, side: null }]);
    fireEvent.click(screen.getByLabelText("Show grid"));
    expect(container.querySelector("g.grid-pcb")).toBeNull();
  });

  it("toolbar unit and grid size move the PCB snap (slice A2): 1 mm becomes 50 mil, then 10 mil (a 1.4 mm drag snaps to 1.27 mm, then to 1.524 mm)", () => {
    const { svg, court } = setup();
    fireEvent.change(screen.getByLabelText("Grid spacing"), { target: { value: "1" } });
    fireEvent.change(screen.getByLabelText("Unit"), { target: { value: "mil" } });
    expect((screen.getByLabelText("Grid spacing") as HTMLSelectElement).value).toBe("1.27");
    fireEvent.pointerMove(svg, client(12.7, 7.62));
    expect(screen.getByLabelText("Cursor position").textContent).toMatch(/X 500\.0 +Y 300\.0 mil/);
    dragOn(court("fp_r1"), [10, 10], [11.4, 10]);
    fireEvent.change(screen.getByLabelText("Grid spacing"), { target: { value: "0.254" } });
    dragOn(court("fp_r1"), [10, 10], [11.4, 10]);
    expect(sent).toEqual([
      { type: "MoveFootprint", footprint_id: "fp_r1", position: { x: mm(10) + 1_270_000, y: mm(10) }, rotation: null, side: null },
      { type: "MoveFootprint", footprint_id: "fp_r1", position: { x: mm(10) + 1_524_000, y: mm(10) }, rotation: null, side: null },
    ]);
  });
});

describe("PCB selection reducers", () => {
  it("pick, marquee, move skip locked, align edges, distribute centres, lock only changes", () => {
    const s = board();
    expect(pick(["a"], "b", "replace")).toEqual(["b"]);
    expect(pick(["a"], "b", "add")).toEqual(["a", "b"]);
    expect(pick(["a", "b"], "a", "toggle")).toEqual(["b"]);
    expect(marquee(s, rectOf(pt(0, 0), pt(25, 20)))).toEqual(["fp_r1", "fp_d1"]);
    s.project.pcb.footprints.fp_d1.locked = true;
    expect(moveCommands(s, ["fp_r1", "fp_d1"], pt(1, 0))).toEqual([
      { type: "MoveFootprint", footprint_id: "fp_r1", position: pt(11, 10), rotation: null, side: null },
    ]);
    // Left edges: min courtyard x is 10-1.5 = 8.5; D1 is locked (reference only), R2 moves.
    expect(alignCommands(s, ["fp_r1", "fp_d1", "fp_r2"], "left")).toEqual([
      { type: "MoveFootprint", footprint_id: "fp_r2", position: pt(10, 14), rotation: null, side: null },
    ]);
    s.project.pcb.footprints.fp_d1.locked = false;
    expect(distributeCommands(s, ["fp_r1", "fp_d1", "fp_r2"], "x")).toEqual([
      { type: "MoveFootprint", footprint_id: "fp_d1", position: pt(25, 10), rotation: null, side: null },
    ]);
    expect(distributeCommands(s, ["fp_r1", "fp_d1"], "x")).toEqual([]);
    expect(lockCommands(s, ["fp_r1", "fp_d1"], false)).toEqual([]);
  });
});
