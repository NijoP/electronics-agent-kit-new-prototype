// Regression test for the captain's report (directive section 2): "two resistors on a sheet,
// select one and drag it, both move". Root cause: a left-button drag over a symbol under any
// tool other than Select fell through to the canvas pan, so the whole view (both resistors)
// slid under the cursor and no command was sent. The Place tool stays armed after placing the
// two resistors, so that is exactly the state the report describes.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { GridControls } from "../../grid";
import { SchematicView } from "../../SchematicView";
import type { Command, Snapshot } from "../../types";
import { boundsOf, fit, type ViewBox } from "../../viewport";

const SIZE = 1000;
const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });
let sent: Command[];
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(...(Array.isArray(c) ? c : [c]));
  },
  busy: false,
  library: [{ id: "R_0603", name: "Resistor" } as EditCtx["library"][number]],
  notify: () => {},
});

beforeEach(() => {
  sent = [];
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

const vb: ViewBox = fit(boundsOf([[0, 0], [297, -210]])!);
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
function dragOn(target: Element, from: [number, number], to: [number, number], mods: { shiftKey?: boolean; ctrlKey?: boolean } = {}) {
  fireEvent.pointerDown(target, { ...client(...from), button: 0, buttons: 1, ...mods });
  fireEvent.pointerMove(target, { ...client((from[0] + to[0]) / 2, (from[1] + to[1]) / 2), buttons: 1 });
  fireEvent.pointerMove(target, { ...client(...to), buttons: 1 });
  fireEvent.pointerUp(target, { ...client(...to), button: 0 });
}
function clickOn(target: Element, at: [number, number], mods: { shiftKey?: boolean; ctrlKey?: boolean } = {}) {
  fireEvent.pointerDown(target, { ...client(...at), button: 0, buttons: 1, ...mods });
  fireEvent.pointerUp(target, { ...client(...at), button: 0, ...mods });
}

/** R1 at (50,50) and R2 at (80,50): two independent resistors, nothing wired. */
function twoResistors(): Snapshot {
  const s = fixture();
  const sc = s.project.schematic;
  delete sc.symbols.sym_d1;
  sc.symbols.sym_r2 = { ...sc.symbols.sym_r1, id: "sym_r2", refdes: "R2", position: pt(80, 50) };
  sc.labels = {};
  sc.wires = {};
  sc.no_connects = {};
  const r1 = s.geometry.symbols.find((g) => g.id === "sym_r1")!;
  s.geometry.symbols = [
    r1,
    { ...r1, id: "sym_r2", refdes: "R2", position: pt(80, 50), pins: r1.pins.map((p) => ({ ...p, position: { x: p.position.x + mm(30), y: p.position.y } })) },
  ];
  return s;
}

function setup(selected: string | null = null) {
  // The grid combos live in the main toolbar; they share the schematic grid store with the editor.
  const r = render(
    <>
      <GridControls kind="schematic" />
      <SchematicView snapshot={twoResistors()} selected={selected} onSelect={() => {}} edit={edit()} />
    </>,
  );
  const svg = r.container.querySelector("svg.canvas")!;
  const body = (id: string) => r.container.querySelector(`[data-symbol-id="${id}"] rect.body`)!;
  return { ...r, svg, body };
}

const moved = (id: string) =>
  sent.flatMap((c) => (c.type === "Batch" ? c.commands : [c])).filter((c) => c.type === "MoveSymbol" && c.symbol_id === id);

describe("two resistors: dragging one never moves the other", () => {
  for (const tool of ["Select", "Place", "Move"]) {
    it(`${tool} tool: dragging R1 moves only R1 and does not pan the view`, () => {
      const { svg, body } = setup();
      if (tool !== "Select") fireEvent.click(screen.getByRole("button", { name: tool }));
      const before = svg.getAttribute("viewBox");
      dragOn(body("sym_r1"), [50, 50], [50, 60.16]);
      expect(svg.getAttribute("viewBox")).toBe(before);
      expect(moved("sym_r1")).toHaveLength(1);
      expect(moved("sym_r2")).toHaveLength(0);
      expect(sent).toEqual([{ type: "MoveSymbol", symbol_id: "sym_r1", position: { x: mm(50), y: mm(50) + mm(10.16) } }]);
    });
  }

  it("with R1 selected elsewhere, dragging R2 moves only R2", () => {
    const { body } = setup("sym_r1");
    dragOn(body("sym_r2"), [80, 50], [80, 60.16]);
    expect(moved("sym_r1")).toHaveLength(0);
    expect(moved("sym_r2")).toHaveLength(1);
  });

  it("both move together only when both were explicitly selected", () => {
    const { body } = setup();
    clickOn(body("sym_r1"), [50, 50]);
    clickOn(body("sym_r2"), [80, 50], { shiftKey: true });
    dragOn(body("sym_r2"), [80, 50], [80, 60.16]);
    expect(moved("sym_r1")).toHaveLength(1);
    expect(moved("sym_r2")).toHaveLength(1);
  });

  it("after a group selection, a plain click on one resistor selects only it", () => {
    const { body } = setup();
    clickOn(body("sym_r1"), [50, 50]);
    clickOn(body("sym_r2"), [80, 50], { shiftKey: true });
    clickOn(body("sym_r1"), [50, 50]);
    dragOn(body("sym_r1"), [50, 50], [50, 60.16]);
    expect(moved("sym_r1")).toHaveLength(1);
    expect(moved("sym_r2")).toHaveLength(0);
  });
});

describe("schematic selection editing (directive section 2)", () => {
  it("Ctrl-click toggles a resistor out of the selection", () => {
    const { body } = setup();
    clickOn(body("sym_r1"), [50, 50]);
    clickOn(body("sym_r2"), [80, 50], { ctrlKey: true });
    expect(screen.getByText(/2 objects selected/)).toBeTruthy();
    clickOn(body("sym_r1"), [50, 50], { ctrlKey: true });
    expect(screen.getByText(/1 object selected/)).toBeTruthy();
    dragOn(body("sym_r2"), [80, 50], [80, 60.16]);
    expect(moved("sym_r1")).toHaveLength(0);
    expect(moved("sym_r2")).toHaveLength(1);
  });

  it("X / Y flip and the selection bar buttons send FlipSymbol, Duplicate pastes a copy", () => {
    const { body } = setup();
    clickOn(body("sym_r1"), [50, 50]);
    fireEvent.keyDown(window, { key: "x" });
    fireEvent.keyDown(window, { key: "y" });
    fireEvent.click(screen.getByRole("button", { name: "Flip H" }));
    expect(sent).toEqual([
      { type: "FlipSymbol", symbol_id: "sym_r1", axis: "horizontal" },
      { type: "FlipSymbol", symbol_id: "sym_r1", axis: "vertical" },
      { type: "FlipSymbol", symbol_id: "sym_r1", axis: "horizontal" },
    ]);
    sent = [];
    fireEvent.keyDown(window, { key: "d", ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Duplicate" }));
    expect(sent).toHaveLength(2);
    for (const c of sent) {
      expect(c).toMatchObject({ type: "PlaceSymbol", component_id: "R_0603", refdes: null, position: { x: mm(50) + 4 * mm(1.27), y: mm(50) + 4 * mm(1.27) } });
    }
  });

  it("Ctrl+X copies then deletes; Ctrl+V pastes it back", () => {
    const { body } = setup();
    clickOn(body("sym_r2"), [80, 50]);
    fireEvent.keyDown(window, { key: "x", ctrlKey: true });
    expect(sent).toEqual([{ type: "DeleteSymbol", symbol_id: "sym_r2" }]);
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    expect(sent[1]).toMatchObject({ type: "PlaceSymbol", component_id: "R_0603" });
  });

  it("selected symbols get a selection boundary; the Place tool shows a placement preview", () => {
    const { body, container, svg } = setup();
    clickOn(body("sym_r1"), [50, 50]);
    expect(container.querySelectorAll(".sel-box")).toHaveLength(1);
    expect(container.querySelector('[data-symbol-id="sym_r1"] .sel-box')).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Place" }));
    fireEvent.pointerMove(svg, client(100.3, 100.2));
    expect(container.querySelector('[data-place-ghost="R_0603"] rect')).toBeTruthy();
  });

  it("grid: visible by default, toggles off; snap off moves by the exact drag distance", () => {
    localStorage.clear();
    const { body, container } = setup();
    expect(container.querySelector("g.grid-schematic")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Show grid"));
    expect(container.querySelector("g.grid-schematic")).toBeNull();
    fireEvent.click(screen.getByLabelText("Snap to grid"));
    dragOn(body("sym_r1"), [50, 50], [50, 53]);
    const c = moved("sym_r1")[0] as Extract<Command, { type: "MoveSymbol" }>;
    expect(Math.abs(c.position.y - mm(53))).toBeLessThan(mm(0.01));
    localStorage.clear();
  });
});

describe("toolbar grid size and unit move the schematic snap (slice A2)", () => {
  it("0.1 in snaps a 3 mm drag to 2.54 mm; 25 mil snaps it to 3.175 mm", () => {
    localStorage.clear();
    const { body } = setup();
    fireEvent.change(screen.getByLabelText("Unit"), { target: { value: "inch" } });
    expect((screen.getByLabelText("Grid spacing") as HTMLSelectElement).value).toBe("1.27"); // 0.05 in
    fireEvent.change(screen.getByLabelText("Grid spacing"), { target: { value: "2.54" } });
    dragOn(body("sym_r1"), [50, 50], [50, 53]);
    const a = moved("sym_r1")[0] as Extract<Command, { type: "MoveSymbol" }>;
    expect(Math.abs(a.position.y - mm(52.54))).toBeLessThan(10);
    fireEvent.change(screen.getByLabelText("Unit"), { target: { value: "mil" } });
    fireEvent.change(screen.getByLabelText("Grid spacing"), { target: { value: "0.635" } });
    expect(screen.getByLabelText("Cursor position").textContent).toMatch(/mil$/);
    dragOn(body("sym_r1"), [50, 50], [50, 53]);
    const b = moved("sym_r1")[1] as Extract<Command, { type: "MoveSymbol" }>;
    expect(Math.abs(b.position.y - mm(53.175))).toBeLessThan(10);
    localStorage.clear();
  });
});
