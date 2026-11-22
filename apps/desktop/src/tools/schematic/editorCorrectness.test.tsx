// Slice E2 (editor correctness and atomicity), schematic side. Each case is a regression test
// for a defect from the Phase 1 inventory: no-connect markers drawn on every sheet (F7), a
// second marker on every click and no way to remove one (F7, ADR-0038), and a Place-tool click
// on an existing symbol placing a new part on top of it.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { SchematicView } from "../../SchematicView";
import type { Command, Snapshot } from "../../types";
import { boundsOf, fit, type ViewBox } from "../../viewport";
import { noConnectOn, sheetNoConnects, toggleNoConnect } from "./noConnect";

const SIZE = 1000;
const mm = (v: number) => v * 1_000_000;
let sent: Command[];
let notify: ReturnType<typeof vi.fn<(m: string) => void>>;
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(...(Array.isArray(c) ? c : [c]));
  },
  busy: false,
  library: [{ id: "R_0603", name: "Resistor 0603", category: "Resistor", refdes_prefix: "R", default_value: "10k", pins: [["1", "~"], ["2", "~"]] }],
  notify,
});

beforeEach(() => {
  sent = [];
  notify = vi.fn<(m: string) => void>();
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

/**
 * Two sheets. Sheet 1 has R1 with a marker on pin 1; sheet 2 has R9 at the same place as R1
 * with a marker on pin 2, so a marker of the wrong sheet would be drawn at R1's pin 2.
 */
function twoSheets(): Snapshot {
  const s = fixture();
  const sc = s.project.schematic;
  sc.sheets.sht_2 = { id: "sht_2", name: "Sheet 2", width: mm(297), height: mm(210) };
  sc.symbols.sym_r9 = { ...sc.symbols.sym_r1, id: "sym_r9", sheet_id: "sht_2", refdes: "R9" };
  sc.no_connects = {
    nc_1: { id: "nc_1", symbol_id: "sym_r1", pin: "1" },
    nc_9: { id: "nc_9", symbol_id: "sym_r9", pin: "2" },
  };
  const r1 = s.geometry.symbols.find((g) => g.id === "sym_r1")!;
  s.geometry.symbols.push({ ...r1, id: "sym_r9", sheet_id: "sht_2", refdes: "R9" });
  return s;
}

describe("no-connect markers (F7)", () => {
  it("each sheet draws only the markers of its own symbols", () => {
    const s = twoSheets();
    expect(sheetNoConnects(s, "sht_1").map((n) => n.id)).toEqual(["nc_1"]);
    expect(sheetNoConnects(s, "sht_2").map((n) => n.id)).toEqual(["nc_9"]);
    const one = render(<SchematicView snapshot={s} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_1" />);
    expect(one.container.querySelectorAll("path.no-connect")).toHaveLength(1);
    one.unmount();
    const two = render(<SchematicView snapshot={s} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_2" />);
    expect(two.container.querySelectorAll("path.no-connect")).toHaveLength(1);
  });

  it("the No connect tool marks an unmarked pin and removes the marker of a marked pin", () => {
    const s = twoSheets();
    expect(toggleNoConnect(s, "sym_r1", "2")).toEqual({ type: "SetNoConnect", symbol_id: "sym_r1", pin: "2" });
    expect(toggleNoConnect(s, "sym_r1", "1")).toEqual({ type: "DeleteNoConnect", no_connect_id: "nc_1" });
    expect(noConnectOn(s, "sym_r9", "1")).toBeUndefined();

    const r = render(<SchematicView snapshot={s} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_1" />);
    const svg = r.container.querySelector("svg.canvas")!;
    fireEvent.click(screen.getByRole("button", { name: "No connect" }));
    clickOn(svg, [46.3, 50.2]); // R1 pin 1 at (46, 50): already marked
    expect(sent).toEqual([{ type: "DeleteNoConnect", no_connect_id: "nc_1" }]);
  });
});

describe("Place tool", () => {
  it("a click on an existing symbol never places a new part on top of it", () => {
    const r = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    fireEvent.click(screen.getByRole("button", { name: "Place" }));
    const body = r.container.querySelector(`[data-symbol-id="sym_r1"] rect.body`)!;
    clickOn(body, [50, 50]);
    expect(sent.filter((c) => c.type === "PlaceSymbol")).toEqual([]);
    // Empty sheet still places.
    clickOn(r.container.querySelector("svg.canvas")!, [150, 150]);
    expect(sent.filter((c) => c.type === "PlaceSymbol")).toHaveLength(1);
  });
});
