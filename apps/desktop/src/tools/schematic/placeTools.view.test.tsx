// Slice B4 place tools in the schematic editor: each tool's click sends its typed command (bus
// with Enter, 45° wire legs as one Batch, power port from the list, off-page connector, text),
// the Delete tool removes sheet drawings, and buses, entries, graphics and label scopes are drawn.
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { SchematicView } from "../../SchematicView";
import { DocIdContext, setEditorTool } from "../../shell/viewState";
import type { Command, Snapshot } from "../../types";
import { boundsOf, fit, type ViewBox } from "../../viewport";

const SIZE = 1000;
const DOC = "sch:b4";
const nm = (v: number) => Math.round(v * 1_000_000);
const pt = (x: number, y: number) => ({ x: nm(x), y: nm(y) });
let sent: Command[];
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(...(Array.isArray(c) ? c : [c]));
  },
  busy: false,
  library: [{ id: "R_0603", name: "Resistor 0603", category: "Resistor", refdes_prefix: "R", default_value: "10k", pins: [["1", "~"], ["2", "~"]] }],
  notify: () => {},
});

beforeEach(() => {
  sent = [];
  localStorage.clear();
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

const vb: ViewBox = fit(boundsOf([[0, 0], [297, -210]])!);
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
function clickOn(target: Element, at: [number, number]) {
  fireEvent.pointerDown(target, { ...client(...at), button: 0, buttons: 1 });
  fireEvent.pointerUp(target, { ...client(...at), button: 0 });
}
function view(s: Snapshot = fixture()) {
  const r = render(
    <DocIdContext.Provider value={DOC}>
      <SchematicView snapshot={s} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_1" />
    </DocIdContext.Provider>,
  );
  return { r, svg: r.container.querySelector("svg.canvas")! };
}
const arm = (tool: string) => act(() => void setEditorTool(DOC, tool));

describe("B4 place tools send their commands", () => {
  it("Bus: the clicked vertices go out as one AddBus when Enter finishes it", () => {
    const { svg } = view();
    arm("bus");
    clickOn(svg, [25.4, 101.6]);
    clickOn(svg, [76.2, 101.6]);
    expect(sent).toEqual([]);
    fireEvent.keyDown(window, { key: "Enter" });
    expect(sent).toEqual([{ type: "AddBus", sheet_id: "sht_1", name: "D[0..7]", points: [pt(25.4, 101.6), pt(76.2, 101.6)] }]);
  });

  it("Wire in 45° mode (/): a diagonal leg then a straight leg, as one Batch", () => {
    const { svg } = view();
    fireEvent.click(screen.getByRole("button", { name: "Wire" }));
    fireEvent.keyDown(window, { key: "/" });
    clickOn(svg, [25.4, 50.8]);
    clickOn(svg, [35.56, 55.88]);
    expect(sent).toEqual([
      {
        type: "Batch",
        label: "Wire",
        commands: [
          { type: "AddWire", sheet_id: "sht_1", a: pt(25.4, 50.8), b: pt(30.48, 55.88) },
          { type: "AddWire", sheet_id: "sht_1", a: pt(30.48, 55.88), b: pt(35.56, 55.88) },
        ],
      },
    ]);
  });

  it("Power port entry 0 of the default list places a GND ground port; Off-page and Text place theirs", () => {
    const { svg } = view();
    arm("power:0");
    clickOn(svg, [101.6, 101.6]);
    arm("offpage");
    clickOn(svg, [127, 101.6]);
    arm("text");
    clickOn(svg, [152.4, 101.6]);
    expect(sent).toEqual([
      { type: "AddNetLabel", sheet_id: "sht_1", name: "GND", position: pt(101.6, 101.6), kind: "power", power_style: "ground" },
      { type: "AddNetLabel", sheet_id: "sht_1", name: "NET1", position: pt(127, 101.6), kind: "net", scope: "off_page" },
      { type: "AddSheetGraphic", sheet_id: "sht_1", graphic: { kind: "text", position: pt(152.4, 101.6), text: "Note", size: 1_800_000, rotation: "R0" } },
    ]);
  });

  it("Bus entry: a click on a bus adds an entry with its wire end 0.1 in up-right; off a bus nothing is sent", () => {
    const s = fixture();
    s.project.schematic.buses = { bus_1: { id: "bus_1", sheet_id: "sht_1", name: "D[0..7]", points: [pt(25.4, 101.6), pt(76.2, 101.6)] } };
    const { svg } = view(s);
    arm("busentry");
    clickOn(svg, [50.8, 101.6]);
    clickOn(svg, [50.8, 120.65]);
    expect(sent).toEqual([{ type: "AddBusEntry", bus_id: "bus_1", bus_point: pt(50.8, 101.6), wire_point: pt(53.34, 104.14) }]);
  });
});

describe("B4 sheet drawings", () => {
  function drawn(): Snapshot {
    const s = fixture();
    const sc = s.project.schematic;
    sc.buses = { bus_1: { id: "bus_1", sheet_id: "sht_1", name: "D[0..7]", points: [pt(25.4, 101.6), pt(76.2, 101.6)] } };
    sc.bus_entries = { be_1: { id: "be_1", sheet_id: "sht_1", bus_id: "bus_1", bus_point: pt(50.8, 101.6), wire_point: pt(53.34, 104.14) } };
    sc.graphics = {
      gfx_1: { id: "gfx_1", sheet_id: "sht_1", graphic: { kind: "line", a: pt(10, 10), b: pt(40, 10), stroke: { width: 0, style: "dashed" } } },
      gfx_2: { id: "gfx_2", sheet_id: "sht_1", graphic: { kind: "text", position: pt(10, 20), text: "Hello", size: nm(2), rotation: "R0" } },
    };
    sc.labels.lab_2 = { id: "lab_2", sheet_id: "sht_1", name: "SDA", position: pt(120, 60), kind: "net", scope: "off_page" };
    sc.labels.lab_3 = { id: "lab_3", sheet_id: "sht_1", name: "EN", position: pt(120, 80), kind: "net", scope: "local" };
    return s;
  }

  it("draws buses with their names, bus entries, graphics, text and the label scopes", () => {
    const { r } = view(drawn());
    const c = r.container;
    expect(c.querySelector('[data-bus-id="bus_1"] polyline.bus')).toBeTruthy();
    expect(c.querySelector('[data-bus-id="bus_1"] text.bus-name')?.textContent).toBe("D[0..7]");
    expect(c.querySelector('[data-bus-entry-id="be_1"] line.bus-entry')).toBeTruthy();
    expect(c.querySelector('[data-graphic-id="gfx_1"] line.gfx')).toBeTruthy();
    expect(c.querySelector('[data-graphic-id="gfx_2"] text.gfx-text')?.textContent).toBe("Hello");
    expect(c.querySelector('[data-label-id="lab_2"][data-scope="off_page"] path.port-shape')).toBeTruthy();
    expect(c.querySelector('[data-label-id="lab_3"][data-scope="local"]')?.classList.contains("local")).toBe(true);
    expect(c.querySelector('[data-label-id="lab_1"]')?.getAttribute("data-scope")).toBeNull(); // power port
  });

  it("the Delete tool removes a sheet graphic, a bus entry and a bus with their own commands", () => {
    const { r } = view(drawn());
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    clickOn(r.container.querySelector('[data-graphic-id="gfx_1"] line.gfx')!, [25, 10]);
    clickOn(r.container.querySelector('[data-bus-entry-id="be_1"] line.hit')!, [52, 102.9]);
    clickOn(r.container.querySelector('[data-bus-id="bus_1"] polyline.hit')!, [30, 101.6]);
    expect(sent).toEqual([
      { type: "DeleteSheetGraphic", graphic_id: "gfx_1" },
      { type: "DeleteBusEntry", entry_id: "be_1" },
      { type: "DeleteBus", bus_id: "bus_1" },
    ]);
  });

  it("crosshair guides follow the pointer while a place tool is armed, not under Select", () => {
    const { r, svg } = view();
    fireEvent.pointerMove(svg, { ...client(50, 50) });
    expect(r.container.querySelector('[data-guide="crosshair"]')).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Wire" }));
    fireEvent.pointerMove(svg, { ...client(50, 50) });
    expect(r.container.querySelector('[data-guide="crosshair"]')).toBeTruthy();
  });
});
