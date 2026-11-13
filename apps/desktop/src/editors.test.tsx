// Editor interaction tests: pointer clicks on the canvases must become exactly the typed
// engine commands (with snapped nanometre coordinates); the views never mutate anything.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "./EditContext";
import { fixture } from "./fixture.test-data";
import { PcbView, pcbBounds } from "./PcbView";
import { SchematicView } from "./SchematicView";
import type { Command, LibraryEntry } from "./types";
import { boundsOf, fit, mm, type ViewBox } from "./viewport";

const SIZE = 1000;
const library: LibraryEntry[] = [
  { id: "R_0603", name: "Resistor 0603", category: "Resistor", refdes_prefix: "R", default_value: "10k", pins: [["1", "~"], ["2", "~"]] },
  { id: "LED_0603", name: "LED 0603", category: "LED", refdes_prefix: "D", default_value: "Green", pins: [["1", "K"], ["2", "A"]] },
];
let sent: Command[][];
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(Array.isArray(c) ? c : [c]);
  },
  busy: false,
  library,
  notify: vi.fn(),
});

beforeEach(() => {
  sent = [];
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

/** Click at view-space (mm, Y down) coordinates given the canvas's initial viewBox. */
function clickAt(el: Element, vb: ViewBox, x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  const cx = (x - vb.x) * s + (SIZE - vb.w * s) / 2;
  const cy = (y - vb.y) * s + (SIZE - vb.h * s) / 2;
  fireEvent.pointerDown(el, { clientX: cx, clientY: cy, button: 0, buttons: 1 });
  fireEvent.pointerUp(el, { clientX: cx, clientY: cy, button: 0 });
}
const sheetVb = () =>
  fit(
    boundsOf([
      [0, 0],
      [297, -210],
    ])!,
  );
const pcbVb = () => fit(pcbBounds(fixture()));
const tool = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

describe("schematic editor", () => {
  it("places the chosen library part on the snapped grid", () => {
    const { container } = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Place");
    fireEvent.change(screen.getByLabelText("Part"), { target: { value: "LED_0603" } });
    clickAt(container.querySelector("svg.canvas")!, sheetVb(), 100.2, -100.4);
    expect(sent).toEqual([
      [
        {
          type: "PlaceSymbol",
          sheet_id: "sht_1",
          component_id: "LED_0603",
          position: { x: 100_330_000, y: 100_330_000 },
          rotation: "R0",
          refdes: null,
          value: null,
        },
      ],
    ]);
  });

  it("wires with pin capture and splits a diagonal into two AddWire legs", () => {
    const { container } = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Wire");
    const svg = container.querySelector("svg.canvas")!;
    clickAt(svg, sheetVb(), 54.3, -50.2); // captures R1 pin 2 at (54, 50)
    clickAt(svg, sheetVb(), 76, -60); // free point on the 1.27 mm grid → (76.2, 59.69)
    expect(sent).toHaveLength(1);
    const [a, b] = sent[0] as Extract<Command, { type: "AddWire" }>[];
    expect(a.a).toEqual({ x: 54_000_000, y: 50_000_000 });
    expect(a.b).toEqual({ x: b.a.x, y: a.a.y });
    expect(b.b.x).toBe(a.b.x);
    expect(a.a.y).toBe(50_000_000);
  });

  it("connects two pins with ConnectPins and deletes by clicking an object", () => {
    const { container } = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    const svg = container.querySelector("svg.canvas")!;
    tool("Connect pins");
    clickAt(svg, sheetVb(), 54, -50);
    clickAt(svg, sheetVb(), 76, -50);
    expect(sent[0]).toEqual([{ type: "ConnectPins", a_symbol: "sym_r1", a_pin: "2", b_symbol: "sym_d1", b_pin: "2" }]);

    tool("Delete");
    const wire = container.querySelector("[data-wire-id] .hit")!;
    fireEvent.pointerDown(wire, { clientX: 5, clientY: 5, button: 0, buttons: 1 });
    fireEvent.pointerUp(wire, { clientX: 5, clientY: 5, button: 0 });
    expect(sent[1]).toEqual([{ type: "DeleteWire", wire_id: "wir_1" }]);
  });

  it("edits the selected symbol's value and rotates it from the keyboard", () => {
    render(<SchematicView snapshot={fixture()} selected="sym_r1" onSelect={() => {}} edit={edit()} />);
    const v = screen.getByLabelText("Value");
    fireEvent.change(v, { target: { value: "4k7" } });
    fireEvent.blur(v);
    expect(sent[0]).toEqual([{ type: "SetSymbolValue", symbol_id: "sym_r1", value: "4k7" }]);
    fireEvent.keyDown(window, { key: "r" });
    expect(sent[1]).toEqual([{ type: "RotateSymbol", symbol_id: "sym_r1", rotation: "R90" }]);
  });

  it("is read-only without an edit context", () => {
    render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} />);
    expect(screen.queryByRole("toolbar")).toBeNull();
  });
});

describe("PCB editor", () => {
  it("draws a rectangular board outline from two corners", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Board outline");
    const svg = container.querySelector("svg.canvas")!;
    clickAt(svg, pcbVb(), 0, 0);
    clickAt(svg, pcbVb(), 40, -25);
    fireEvent.click(screen.getByRole("button", { name: /Finish outline/ }));
    expect(sent[0]).toEqual([
      {
        type: "SetBoardOutline",
        points: [
          { x: 0, y: 0 },
          { x: 40_000_000, y: 0 },
          { x: 40_000_000, y: 25_000_000 },
          { x: 0, y: 25_000_000 },
        ],
      },
    ]);
  });

  it("routes a track on the chosen layer starting from a pad's net", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Route");
    fireEvent.change(screen.getByLabelText("Layer"), { target: { value: "bottom" } });
    const svg = container.querySelector("svg.canvas")!;
    clickAt(svg, pcbVb(), 9.3, -10.1); // captures R1 pad 1 (net N1) at (9.2, 10)
    clickAt(svg, pcbVb(), 15, -10); // axis-aligned → one segment
    expect(sent).toEqual([
      [
        {
          type: "AddTrack",
          layer: "bottom",
          a: { x: 9_200_000, y: 10_000_000 },
          b: { x: 15_000_000, y: 10_000_000 },
          width: null,
          net: "N1",
        },
      ],
    ]);
  });

  it("refuses to route without a net and reports it", () => {
    const ctx = edit();
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={ctx} />);
    tool("Route");
    const svg = container.querySelector("svg.canvas")!;
    clickAt(svg, pcbVb(), 5, -5);
    clickAt(svg, pcbVb(), 8, -5);
    expect(sent).toEqual([]);
    expect(ctx.notify).toHaveBeenCalledWith("Pick the net to route first.");
  });

  it("places a via on a pad's net and deletes tracks and vias", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Via");
    clickAt(container.querySelector("svg.canvas")!, pcbVb(), 20.1, -10);
    expect(sent[0]).toEqual([{ type: "AddVia", position: { x: 20_000_000, y: 10_000_000 }, net: "GND", drill: null, diameter: null }]);
    tool("Delete");
    const via = container.querySelector("[data-via-id] circle")!;
    fireEvent.pointerDown(via, { clientX: 1, clientY: 1, button: 0, buttons: 1 });
    fireEvent.pointerUp(via, { clientX: 1, clientY: 1, button: 0 });
    expect(sent[1]).toEqual([{ type: "DeleteVia", via_id: "via_1" }]);
  });

  it("moves a footprint with two clicks and flips the selected one", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected="sym_r1" onSelect={() => {}} edit={edit()} />);
    tool("Move");
    const svg = container.querySelector("svg.canvas")!;
    const fp = container.querySelector('[data-footprint-id="fp_r1"] .courtyard')!;
    fireEvent.pointerDown(fp, { clientX: 500, clientY: 500, button: 0, buttons: 1 });
    fireEvent.pointerUp(fp, { clientX: 500, clientY: 500, button: 0 });
    clickAt(svg, pcbVb(), 6.1, -5.2);
    expect(sent[0]).toEqual([{ type: "MoveFootprint", footprint_id: "fp_r1", position: { x: 6_000_000, y: 5_000_000 }, rotation: null, side: null }]);
    fireEvent.keyDown(window, { key: "f" });
    expect(sent[1]).toEqual([{ type: "MoveFootprint", footprint_id: "fp_r1", position: { x: 10_000_000, y: 10_000_000 }, rotation: null, side: "bottom" }]);
  });
});

describe("view geometry sanity", () => {
  it("mm is the inverse of the nanometre unit", () => {
    expect(mm(1_000_000)).toBe(1);
  });
});
