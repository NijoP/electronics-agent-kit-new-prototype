// Editor interaction tests: pointer clicks on the canvases must become exactly the typed
// engine commands (with snapped nanometre coordinates); the views never mutate anything.
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "./EditContext";
import { fixture } from "./fixture.test-data";
import { PcbView, pcbBounds } from "./PcbView";
import { SchematicView } from "./SchematicView";
import type { Command, LibraryEntry } from "./types";
import { boundsOf, fit, mm, type ViewBox } from "./viewport";

// IPC stand-in for the route tool: answers `route_step` with the direct 90-degree path, the
// shape eak-engineering returns in free space (the real computation is tested in Rust:
// interactive.rs and src-tauri/tests/ipc.rs).
const ipc = vi.hoisted(() => ({ calls: [] as { cmd: string; args: unknown }[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (cmd: string, args: { request: { net: string; layer: unknown; from: { x: number; y: number }; to: { x: number; y: number }; mode: string } }) => {
    ipc.calls.push({ cmd, args });
    const { net, layer, from, to, mode } = args.request;
    const corner = { x: to.x, y: from.y };
    const segments = [
      { a: from, b: corner },
      { a: corner, b: to },
    ].filter((g) => g.a.x !== g.b.x || g.a.y !== g.b.y);
    return { revision: 1, net, layer, width: 250_000, clearance: 200_000, mode, segments, via: null, end: to, reached: true, walkaround: false, blocking: [], min_gap: null, message: null };
  },
}));

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

  it("wires with pin capture and splits a diagonal into two AddWire legs sent as ONE Batch", () => {
    const { container } = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Wire");
    const svg = container.querySelector("svg.canvas")!;
    clickAt(svg, sheetVb(), 54.3, -50.2); // captures R1 pin 2 at (54, 50)
    clickAt(svg, sheetVb(), 76, -60); // free point on the 1.27 mm grid → (76.2, 59.69)
    // Regression (inventory F3): the two legs used to be two commands (two revisions/undos).
    expect(sent).toHaveLength(1);
    expect(sent[0]).toHaveLength(1);
    const batch = sent[0][0] as Extract<Command, { type: "Batch" }>;
    expect(batch).toMatchObject({ type: "Batch", label: "Wire" });
    const [a, b] = batch.commands as Extract<Command, { type: "AddWire" }>[];
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

  it("click-click Move on a wired part drags the attached wire end in one Batch (F3, S4)", () => {
    const snap = fixture();
    snap.project.schematic.wires.wir_1 = { id: "wir_1", sheet_id: "sht_1", a: { x: 54_000_000, y: 50_000_000 }, b: { x: 76_000_000, y: 50_000_000 } };
    const { container } = render(<SchematicView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Move");
    const body = container.querySelector('[data-symbol-id="sym_r1"]')!;
    fireEvent.pointerDown(body, { clientX: 5, clientY: 5, button: 0, buttons: 1 });
    fireEvent.pointerUp(body, { clientX: 5, clientY: 5, button: 0 });
    clickAt(container.querySelector("svg.canvas")!, sheetVb(), 63.5, -55.88); // on the 1.27 mm grid
    // R1 (50,50) -> (63.5,55.88): its pin 2 (54,50) goes to (67.5,55.88) and the wire end follows
    expect(sent).toEqual([
      [
        {
          type: "Batch",
          label: "Move 1 object",
          commands: [
            { type: "MoveWire", wire_id: "wir_1", a: { x: 67_500_000, y: 55_880_000 }, b: { x: 76_000_000, y: 50_000_000 } },
            { type: "MoveSymbol", symbol_id: "sym_r1", position: { x: 63_500_000, y: 55_880_000 } },
          ],
        },
      ],
    ]);
  });

  it("one delete policy: Del, the selection bar and the Delete tool delete at once, no prompt (F3, S22)", () => {
    const confirm = vi.spyOn(window, "confirm");
    const ctx = edit();
    render(<SchematicView snapshot={fixture()} selected="sym_r1" onSelect={() => {}} edit={ctx} />);
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent[0]).toEqual([{ type: "DeleteSymbol", symbol_id: "sym_r1" }]);
    expect(ctx.notify).toHaveBeenLastCalledWith("Deleted 1 object: Ctrl+Z restores it.");
    cleanup();
    render(<SchematicView snapshot={fixture()} selected="sym_d1" onSelect={() => {}} edit={ctx} />);
    fireEvent.click(within(screen.getByRole("group", { name: "Selection" })).getByRole("button", { name: "Delete" }));
    expect(sent[1]).toEqual([{ type: "DeleteSymbol", symbol_id: "sym_d1" }]);
    cleanup();
    const { container } = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} edit={ctx} />);
    tool("Delete");
    const label = container.querySelector('[data-label-id="lab_1"]')!;
    fireEvent.pointerDown(label, { clientX: 5, clientY: 5, button: 0, buttons: 1 });
    fireEvent.pointerUp(label, { clientX: 5, clientY: 5, button: 0 });
    expect(sent[2]).toEqual([{ type: "DeleteNetLabel", label_id: "lab_1" }]);
    expect(confirm).not.toHaveBeenCalled();
    confirm.mockRestore();
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
    // F3 (S14): no decorative "read-only" hint either (the app always edits; tests render bare)
    expect(screen.queryByText(/read-only/)).toBeNull();
  });
});

describe("PCB editor", () => {
  // Unrun: an empty board explains itself and offers the one action; it disappears once drawing starts.
  it("shows a Draw board outline empty state when there is no outline", () => {
    const snap = fixture();
    snap.project.pcb.board.outline = [];
    render(<PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />);
    expect(screen.getByRole("note", { name: "No board outline" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Draw board outline" }));
    expect(screen.queryByRole("note", { name: "No board outline" })).toBeNull();
    expect(screen.getByRole("button", { name: "Board outline" }).getAttribute("aria-pressed")).toBe("true");
  });

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

  it("routes from a pad's net with the engine's route step and commits ONE Batch", async () => {
    ipc.calls = [];
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Route");
    fireEvent.change(screen.getByLabelText("Layer"), { target: { value: "bottom" } });
    const svg = container.querySelector("svg.canvas")!;
    clickAt(svg, pcbVb(), 9.3, -10.1); // captures R1 pad 1 (net N1) at (9.2, 10)
    expect(sent).toEqual([]); // starting commits nothing
    await act(async () => clickAt(svg, pcbVb(), 15, -10));
    const step = ipc.calls.find((c) => c.cmd === "route_step")!;
    expect(step.args).toEqual({
      request: { net: "N1", layer: "bottom", from: { x: 9_200_000, y: 10_000_000 }, to: { x: 15_000_000, y: 10_000_000 }, mode: "diagonal45", width: null, via_at_start: false, conflict: "walkaround", via: null, pending: { tracks: [], vias: [], shoved: [] } },
    });
    expect(sent).toEqual([]); // fixing a step commits nothing either
    await act(async () => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    expect(sent).toEqual([
      [
        {
          type: "Batch",
          label: "Route N1",
          commands: [
            { type: "AddTrack", layer: "bottom", a: { x: 9_200_000, y: 10_000_000 }, b: { x: 15_000_000, y: 10_000_000 }, width: 250_000, net: "N1" },
          ],
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
    expect(ctx.notify).toHaveBeenCalledWith("Start on a pad or track end of a net, or pick the net first.");
  });

  it("places a via on a pad's net and deletes tracks and vias", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Via");
    clickAt(container.querySelector("svg.canvas")!, pcbVb(), 20.1, -10);
    expect(sent[0]).toEqual([{ type: "AddVia", position: { x: 20_000_000, y: 10_000_000 }, net: "GND", drill: null, diameter: null, from: "top", to: "bottom" }]);
    tool("Delete");
    const via = container.querySelector("[data-via-id] circle")!;
    fireEvent.pointerDown(via, { clientX: 1, clientY: 1, button: 0, buttons: 1 });
    fireEvent.pointerUp(via, { clientX: 1, clientY: 1, button: 0 });
    expect(sent[1]).toEqual([{ type: "DeleteVia", via_id: "via_1" }]);
  });

  it("deletes a footprint with the Delete tool and draws schema v4 objects (ADR-0042)", () => {
    const snap = fixture();
    const pcb = snap.project.pcb;
    pcb.tracks["trk_arc"] = { id: "trk_arc", layer: "top", width: 250_000, a: { x: 30_000_000, y: 5_000_000 }, b: { x: 34_000_000, y: 5_000_000 }, net: "N1", arc_deg: 90 };
    pcb.cutouts = { cut_1: { id: "cut_1", outline: [{ x: 2_000_000, y: 2_000_000 }, { x: 4_000_000, y: 2_000_000 }, { x: 4_000_000, y: 4_000_000 }] } };
    pcb.regions = { rgn_1: { id: "rgn_1", outline: [{ x: 5_000_000, y: 2_000_000 }, { x: 7_000_000, y: 2_000_000 }, { x: 7_000_000, y: 4_000_000 }], forbid: ["vias"] } };
    pcb.texts = { txt_1: { id: "txt_1", layer: "top_silk", position: { x: 15_000_000, y: 2_000_000 }, text: "REV A", size: 1_000_000 } };
    pcb.dimensions = { dim_1: { id: "dim_1", layer: "documentation", a: { x: 0, y: 0 }, b: { x: 10_000_000, y: 0 }, offset: -2_000_000, unit: "mm", precision: 2, text_size: 0 } };
    const { container } = render(<PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />);
    expect(container.querySelector('[data-track-id="trk_arc"] polyline.track')!.getAttribute("points")!.split(" ").length).toBeGreaterThan(5);
    expect(container.querySelectorAll("polygon.board-cutout")).toHaveLength(1);
    expect(container.querySelector('[data-region-id="rgn_1"]')!.getAttribute("class")).toContain("keepout");
    expect(container.querySelector('[data-text-id="txt_1"]')!.textContent).toBe("REV A");
    expect(container.querySelector('[data-dimension-id="dim_1"] text')!.textContent).toBe("10.00 mm");
    tool("Delete");
    const court = container.querySelector('[data-footprint-id="fp_r1"] .courtyard')!;
    fireEvent.pointerDown(court, { clientX: 1, clientY: 1, button: 0, buttons: 1 });
    fireEvent.pointerUp(court, { clientX: 1, clientY: 1, button: 0 });
    expect(sent[0]).toEqual([{ type: "DeleteFootprint", footprint_id: "fp_r1" }]);
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

  it("slice E1 regression: R and F act on the selection only under the Select and Move tools", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Select");
    const fp = container.querySelector('[data-footprint-id="fp_r1"] .courtyard')!;
    fireEvent.pointerDown(fp, { clientX: 500, clientY: 500, button: 0, buttons: 1 });
    fireEvent.pointerUp(fp, { clientX: 500, clientY: 500, button: 0 });
    for (const t of ["Via", "Route", "Zone", "Board outline"]) {
      tool(t);
      fireEvent.keyDown(window, { key: "r" });
      fireEvent.keyDown(window, { key: "f" });
    }
    expect(sent).toEqual([]);
    tool("Select");
    fireEvent.keyDown(window, { key: "f" });
    expect(sent).toHaveLength(1);
    expect(sent[0]).toEqual([{ type: "MoveFootprint", footprint_id: "fp_r1", position: { x: 10_000_000, y: 10_000_000 }, rotation: null, side: "bottom" }]);
  });
});

describe("view geometry sanity", () => {
  it("mm is the inverse of the nanometre unit", () => {
    expect(mm(1_000_000)).toBe(1);
  });
});
