// Select-tool interaction tests: marquee and click selection, drag-move as ONE Batch with
// rubber-banded wires, and the keyboard shortcuts. Every edit must arrive as typed engine
// commands; the view never changes the design itself.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { SchematicView } from "../../SchematicView";
import type { Command, Snapshot } from "../../types";
import { boundsOf, fit, type ViewBox } from "../../viewport";

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
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

const vb: ViewBox = fit(
  boundsOf([
    [0, 0],
    [297, -210],
  ])!,
);
/** Client coordinates of an EAK-IR point given in mm (Y up). */
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
function dragOn(target: Element, from: [number, number], to: [number, number], shiftKey = false) {
  fireEvent.pointerDown(target, { ...client(...from), button: 0, buttons: 1, shiftKey });
  fireEvent.pointerMove(target, { ...client((from[0] + to[0]) / 2, (from[1] + to[1]) / 2), buttons: 1 });
  fireEvent.pointerMove(target, { ...client(...to), buttons: 1 });
  fireEvent.pointerUp(target, { ...client(...to), button: 0 });
}
function clickOn(target: Element, at: [number, number], shiftKey = false) {
  fireEvent.pointerDown(target, { ...client(...at), button: 0, buttons: 1, shiftKey });
  fireEvent.pointerUp(target, { ...client(...at), button: 0, shiftKey });
}

/** Wire from R1 pin 2 (54,50) to D1 pin 2 (76,50); GND label on D1 pin 1 (84,50). */
function wired(): Snapshot {
  const s = fixture();
  s.project.schematic.wires.wir_1 = { id: "wir_1", sheet_id: "sht_1", a: pt(54, 50), b: pt(76, 50) };
  s.project.schematic.labels.lab_1.position = pt(84, 50);
  return s;
}

function setup(snap = wired(), onSelect = vi.fn()) {
  const r = render(<SchematicView snapshot={snap} selected={null} onSelect={onSelect} edit={edit()} />);
  const svg = r.container.querySelector("svg.canvas")!;
  const symbol = (id: string) => r.container.querySelector(`[data-symbol-id="${id}"] rect.body`)!;
  return { ...r, svg, symbol, onSelect };
}

describe("schematic select tool", () => {
  it("drag-moving a symbol submits one Batch that rubber-bands the wire and pin label", () => {
    const { symbol, onSelect } = setup();
    dragOn(symbol("sym_d1"), [80, 50], [80, 60.2]); // snaps to 8 grid steps = 10.16 mm
    const d = mm(10.16);
    expect(onSelect).toHaveBeenCalledWith("sym_d1");
    expect(sent).toEqual([
      {
        type: "Batch",
        label: "Move 1 object",
        commands: [
          { type: "MoveWire", wire_id: "wir_1", a: pt(54, 50), b: { x: mm(76), y: mm(50) + d } },
          { type: "MoveNetLabel", label_id: "lab_1", position: { x: mm(84), y: mm(50) + d } },
          { type: "MoveSymbol", symbol_id: "sym_d1", position: { x: mm(80), y: mm(50) + d } },
        ],
      },
    ]);
  });

  it("shows a live preview while dragging and submits nothing until release", () => {
    const { container, symbol } = setup();
    const target = symbol("sym_d1");
    fireEvent.pointerDown(target, { ...client(80, 50), button: 0, buttons: 1 });
    fireEvent.pointerMove(target, { ...client(80, 55.08), buttons: 1 });
    expect(sent).toEqual([]);
    expect(container.querySelector('[data-symbol-id="sym_d1"]')!.getAttribute("transform")).toMatch(/^translate\(0 -5\.08/);
    expect(container.querySelectorAll("line.wire.preview")).toHaveLength(1);
    fireEvent.pointerLeave(target);
    expect(container.querySelector('[data-symbol-id="sym_d1"]')!.getAttribute("transform")).toBeNull();
    expect(sent).toEqual([]);
  });

  it("marquee selects what is inside; Delete removes it as one Batch", () => {
    const { svg } = setup();
    dragOn(svg, [40, 60], [90, 40]);
    expect(screen.getByText(/4 objects selected/)).toBeTruthy();
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent).toEqual([
      {
        type: "Batch",
        label: "Delete 4 objects",
        commands: [
          { type: "DeleteNetLabel", label_id: "lab_1" },
          { type: "DeleteWire", wire_id: "wir_1" },
          { type: "DeleteSymbol", symbol_id: "sym_r1" },
          { type: "DeleteSymbol", symbol_id: "sym_d1" },
        ],
      },
    ]);
  });

  it("click selects, shift-click adds, click on empty sheet clears", () => {
    const { svg, symbol } = setup();
    clickOn(symbol("sym_r1"), [50, 50]);
    clickOn(symbol("sym_d1"), [80, 50], true);
    expect(screen.getByText(/2 objects selected/)).toBeTruthy();
    clickOn(svg, [150, 150]);
    expect(screen.getByText(/Click or drag a box to select/)).toBeTruthy();
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent).toEqual([]);
  });

  it("R rotates the selection and drags the attached wire end", () => {
    const { symbol } = setup();
    clickOn(symbol("sym_r1"), [50, 50]);
    fireEvent.keyDown(window, { key: "r" });
    expect(sent).toEqual([
      {
        type: "Batch",
        label: "Rotate 1 object",
        commands: [
          { type: "MoveWire", wire_id: "wir_1", a: pt(50, 54), b: pt(76, 50) },
          { type: "RotateSymbol", symbol_id: "sym_r1", rotation: "R90" },
        ],
      },
    ]);
  });

  it("Ctrl+C / Ctrl+V pastes a copy at the cursor with fresh designators", () => {
    const { svg, symbol } = setup();
    clickOn(symbol("sym_r1"), [50, 50]);
    fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    expect(notify).toHaveBeenCalledWith("Copied 1 object.");
    fireEvent.pointerMove(svg, client(100, 100));
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    expect(sent).toHaveLength(1);
    const c = sent[0] as Extract<Command, { type: "PlaceSymbol" }>;
    expect(c).toMatchObject({ type: "PlaceSymbol", sheet_id: "sht_1", component_id: "R_0603", refdes: null, value: "330", rotation: "R0" });
    // The clip's anchor (R1 at 50,50) lands on the grid point nearest the cursor.
    expect(Math.abs(c.position.x - mm(100))).toBeLessThan(mm(1.27));
    expect(Math.abs(c.position.y - mm(100))).toBeLessThan(mm(1.27));
  });

  it("W arms the wire tool; Esc returns to select and clears", () => {
    const { symbol } = setup();
    clickOn(symbol("sym_r1"), [50, 50]);
    fireEvent.keyDown(window, { key: "w" });
    expect(screen.getByRole("button", { name: "Wire" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("button", { name: "Select" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByText(/Click or drag a box to select/)).toBeTruthy();
  });

  it("Ctrl+A selects everything on the sheet", () => {
    setup();
    act(() => {
      fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    });
    expect(screen.getByText(/4 objects selected/)).toBeTruthy();
  });

  it("is inert without an edit context", () => {
    const r = render(<SchematicView snapshot={wired()} selected={null} onSelect={() => {}} />);
    const body = r.container.querySelector('[data-symbol-id="sym_d1"] rect.body')!;
    dragOn(body, [80, 50], [80, 60]);
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent).toEqual([]);
  });
});

describe("pin tools (evaluator C3: ERC-001 remedy)", () => {
  it("No connect marks the clicked pin with SetNoConnect", () => {
    const { svg } = setup();
    fireEvent.click(screen.getByRole("button", { name: "No connect" }));
    clickOn(svg, [46.3, 50.2]); // captures R1 pin 1 at (46, 50)
    expect(sent).toEqual([{ type: "SetNoConnect", symbol_id: "sym_r1", pin: "1" }]);
    clickOn(svg, [150, 150]); // no pin: nothing sent, user told why
    expect(sent).toHaveLength(1);
    expect(notify).toHaveBeenCalledWith("Click a pin to mark it as intentionally unconnected.");
  });

  it("Pin label puts the typed net name on the clicked pin with LabelPin", () => {
    const { svg } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Pin label" }));
    fireEvent.change(screen.getByLabelText("Net name"), { target: { value: "VCC" } });
    fireEvent.change(screen.getByLabelText("Label kind"), { target: { value: "power" } });
    clickOn(svg, [76.2, 49.8]); // D1 pin 2 at (76, 50)
    expect(sent).toEqual([{ type: "LabelPin", symbol_id: "sym_d1", pin: "2", name: "VCC", kind: "power" }]);
    fireEvent.change(screen.getByLabelText("Net name"), { target: { value: "  " } });
    clickOn(svg, [76.2, 49.8]);
    expect(sent).toHaveLength(1);
    expect(notify).toHaveBeenCalledWith("Enter a net name first.");
  });
});
