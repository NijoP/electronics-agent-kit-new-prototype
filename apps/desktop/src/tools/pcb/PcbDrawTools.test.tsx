// C4 drawing tools in the PCB editor (slice F5, audit M1 / S6): keep-out, cut-out, text,
// dimension and the outline's arc edges are drawn with clicks and sent as typed commands, and
// the Delete tool removes them. Before this change these objects could only be made over IPC.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { PcbView, pcbBounds } from "../../PcbView";
import type { Command, Snapshot } from "../../types";
import { fit, type ViewBox } from "../../viewport";
import { OutlinePanel } from "./OutlinePanel";

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

let vb: ViewBox;
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
function clickAt(t: Element, x: number, y: number) {
  fireEvent.pointerDown(t, { ...client(x, y), button: 0, buttons: 1 });
  fireEvent.pointerUp(t, { ...client(x, y), button: 0 });
}

/** The fixture board (30 × 20 mm) with one of each drawing object, clear of the copper. */
function drawn(): Snapshot {
  const s = fixture();
  s.project.pcb.regions = { rgn_1: { id: "rgn_1", outline: [pt(2, 13), pt(6, 13), pt(6, 17), pt(2, 17)], forbid: ["tracks", "vias"] } };
  s.project.pcb.cutouts = { cut_1: { id: "cut_1", outline: [pt(24, 13), pt(27, 13), pt(27, 17), pt(24, 17)] } };
  s.project.pcb.texts = { txt_1: { id: "txt_1", layer: "top_silk", position: pt(15, 3), text: "REV A", size: mm(1) } };
  s.project.pcb.dimensions = { dim_1: { id: "dim_1", layer: "documentation", a: pt(2, 18), b: pt(28, 18), offset: 0, unit: "mm", precision: 2, text_size: 0 } };
  return s;
}

function setup(snap: Snapshot = fixture()) {
  vb = fit(pcbBounds(snap));
  const r = render(<PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} />);
  const svg = r.container.querySelector("svg.canvas")!;
  const tool = (name: string) => fireEvent.click(screen.getByRole("button", { name }));
  return { ...r, svg, tool };
}

describe("Keep-out tool (AddRegion)", () => {
  it("two clicks and Finish send one AddRegion that keeps everything out on every layer", () => {
    const { svg, tool } = setup();
    tool("Keep-out");
    clickAt(svg, 3, 3);
    clickAt(svg, 7, 6);
    fireEvent.click(screen.getByRole("button", { name: /Finish keep-out \(2 pts\)/ }));
    expect(sent).toEqual([
      { type: "AddRegion", name: null, outline: [pt(3, 3), pt(7, 3), pt(7, 6), pt(3, 6)], layers: [], forbid: ["tracks", "vias", "components", "zones"], min_track_width: null, max_track_width: null },
    ]);
  });
  it("its options pick the layer, what is kept out and a width rule; Enter finishes", () => {
    const { svg, tool } = setup();
    tool("Keep-out");
    fireEvent.change(screen.getByRole("combobox", { name: "Keep-out layers" }), { target: { value: "top" } });
    fireEvent.change(screen.getByRole("combobox", { name: "Keep out" }), { target: { value: "none" } });
    clickAt(svg, 3, 3);
    clickAt(svg, 7, 6);
    fireEvent.keyDown(window, { key: "Enter" });
    expect(sent).toEqual([]);
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/keep something out or limit the track width/));
    fireEvent.change(screen.getByRole("textbox", { name: "Region max width" }), { target: { value: "0.2" } });
    fireEvent.keyDown(window, { key: "Enter" });
    expect(sent).toMatchObject([{ type: "AddRegion", layers: ["top"], forbid: [], max_track_width: mm(0.2) }]);
  });
});

describe("Cut-out tool (AddBoardCutout)", () => {
  it("two clicks inside the board and Finish send AddBoardCutout", () => {
    const { svg, tool } = setup();
    tool("Cut-out");
    clickAt(svg, 3, 3);
    clickAt(svg, 6, 6);
    fireEvent.click(screen.getByRole("button", { name: /Finish cut-out \(2 pts\)/ }));
    expect(sent).toEqual([{ type: "AddBoardCutout", outline: [pt(3, 3), pt(6, 3), pt(6, 6), pt(3, 6)] }]);
  });
});

describe("Text tool (AddPcbText, MovePcbText)", () => {
  it("places the typed text where clicked; with no text it says so and sends nothing", () => {
    const { svg, tool } = setup();
    tool("Text");
    clickAt(svg, 5, 15);
    expect(sent).toEqual([]);
    expect(notify).toHaveBeenCalledWith("Type the text to place first.");
    fireEvent.change(screen.getByRole("textbox", { name: "PCB text" }), { target: { value: "REV B" } });
    fireEvent.change(screen.getByRole("combobox", { name: "Drawing layer" }), { target: { value: "bottom_silk" } });
    clickAt(svg, 5, 15);
    expect(sent).toEqual([{ type: "AddPcbText", layer: "bottom_silk", position: pt(5, 15), text: "REV B", size: null, stroke_width: null, rotation: null }]);
  });
  it("a click on a text picks it up, the next click moves it; Rotate turns it", () => {
    const { svg, tool } = setup(drawn());
    tool("Text");
    clickAt(svg, 15, 3.4);
    expect(sent).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Rotate 90°" }));
    expect(sent).toEqual([{ type: "MovePcbText", text_id: "txt_1", position: pt(15, 3), rotation: "R90" }]);
    clickAt(svg, 12, 6);
    expect(sent[1]).toEqual({ type: "MovePcbText", text_id: "txt_1", position: pt(12, 6), rotation: null });
    expect(screen.queryByRole("group", { name: "Text selection" })).toBeNull();
  });
});

describe("Dimension tool (AddDimension)", () => {
  it("two clicks send one AddDimension on the documentation layer", () => {
    const { svg, tool } = setup();
    tool("Dimension");
    fireEvent.change(screen.getByRole("textbox", { name: "Dimension offset" }), { target: { value: "2" } });
    clickAt(svg, 2, 18);
    expect(sent).toEqual([]);
    clickAt(svg, 28, 18);
    expect(sent).toEqual([{ type: "AddDimension", layer: "documentation", a: pt(2, 18), b: pt(28, 18), offset: mm(2), unit: null, precision: null, text_size: null }]);
  });
});

describe("Delete tool hit-tests the drawing objects", () => {
  it("deletes the keep-out, cut-out, text and dimension under the pointer", () => {
    const { svg, tool } = setup(drawn());
    tool("Delete");
    clickAt(svg, 4, 15);
    clickAt(svg, 25.5, 15);
    clickAt(svg, 15, 3.4);
    clickAt(svg, 15, 18);
    expect(sent).toEqual([
      { type: "DeleteRegion", region_id: "rgn_1" },
      { type: "DeleteBoardCutout", cutout_id: "cut_1" },
      { type: "DeletePcbText", text_id: "txt_1" },
      { type: "DeleteDimension", dimension_id: "dim_1" },
    ]);
  });
  it("copper under the pointer still wins over a drawing object (the via)", () => {
    const { container, tool } = setup(drawn());
    tool("Delete");
    const via = container.querySelector('[data-via-id="via_1"] circle')!;
    clickAt(via, 15, 15);
    expect(sent).toEqual([{ type: "DeleteVia", via_id: "via_1" }]);
  });
  it("the PCB view reports how many drawing objects the board has", () => {
    const { container } = setup(drawn());
    const s = container.querySelector('section[aria-label="PCB view"]')!;
    expect([s.getAttribute("data-regions"), s.getAttribute("data-cutouts"), s.getAttribute("data-texts"), s.getAttribute("data-dimensions")]).toEqual(["1", "1", "1", "1"]);
  });
});

describe("Outline panel arc edges (SetOutlineArc, S6)", () => {
  const square = [pt(0, 0), pt(40, 0), pt(40, 30), pt(0, 30)];
  const panel = (arcs = [{ edge: 2, sweep_deg: 40 }]) => render(<OutlinePanel edit={edit()} outline={square} arcs={arcs} corners={[]} setCorners={() => {}} />);
  const row = (c: HTMLElement, i: number) => c.querySelectorAll(".vertex-row")[i] as HTMLElement;
  it("typing a sweep into an edge's arc field sends SetOutlineArc for that edge", () => {
    const { container } = panel();
    const f = container.querySelector('input[aria-label="Edge 1 arc"]')!;
    fireEvent.change(f, { target: { value: "30" } });
    fireEvent.blur(f);
    expect(sent).toEqual([{ type: "SetOutlineArc", edge: 0, sweep_deg: 30 }]);
    expect((container.querySelector('input[aria-label="Edge 3 arc"]') as HTMLInputElement).value).toBe("40");
  });
  it("a vertex insert re-indexes the arcs in the same Batch (they are not made straight)", () => {
    const { container } = panel();
    fireEvent.click(row(container, 0).querySelector('button[title^="Insert"]')!);
    expect(sent).toHaveLength(1);
    const b = sent[0] as Extract<Command, { type: "Batch" }>;
    expect(b.type).toBe("Batch");
    expect(b.commands.map((c) => c.type)).toEqual(["SetOutlineArc", "SetBoardOutline", "SetOutlineArc"]);
    expect(b.commands[2]).toEqual({ type: "SetOutlineArc", edge: 3, sweep_deg: 40 });
  });
  it("a vertex delete next to an arc edge says the arc is lost", () => {
    const { container } = panel([{ edge: 0, sweep_deg: 20 }, { edge: 3, sweep_deg: 10 }]);
    fireEvent.click(row(container, 0).querySelector('button[title="Delete vertex"]')!);
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/^Edges 1 and 4 lost their arcs/));
    expect(sent).toHaveLength(1);
  });
});
