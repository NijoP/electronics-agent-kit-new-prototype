// Slice B3 in the schematic editor: the clipboard crosses sheets, right-click opens the menu the
// registry generates for the clicked object type, the selection filter limits what can be
// picked, find/replace sends one Batch, and the editor publishes its operations for the registry.
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ContextItem } from "../../actions/registry";
import type { CanvasTarget } from "../../actions/types";
import type { EditCtx } from "../../EditContext";
import { fixture } from "../../fixture.test-data";
import { SchematicView } from "../../SchematicView";
import { editorContext, resetInspectorStore } from "../../shell/inspectorModel";
import { DocIdContext } from "../../shell/viewState";
import type { Command, Snapshot } from "../../types";
import { boundsOf, fit, type ViewBox } from "../../viewport";
import { setClipboard } from "./clipboard";
import { setSelectFilter } from "./filter";
import { ALL_KINDS } from "./selection";

const SIZE = 1000;
const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });
let sent: Command[];
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(...(Array.isArray(c) ? c : [c]));
  },
  busy: false,
  library: [],
  notify: () => {},
});

beforeEach(() => {
  sent = [];
  setClipboard(null);
  setSelectFilter(ALL_KINDS);
  resetInspectorStore();
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(() => {
  cleanup();
  setSelectFilter(ALL_KINDS);
});

const vb: ViewBox = fit(boundsOf([[0, 0], [297, -210]])!);
function client(x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  return { clientX: (x - vb.x) * s + (SIZE - vb.w * s) / 2, clientY: (-y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
function clickOn(target: Element, at: [number, number]) {
  fireEvent.pointerDown(target, { ...client(...at), button: 0, buttons: 1 });
  fireEvent.pointerUp(target, { ...client(...at), button: 0 });
}

/** wired fixture on two sheets: R1, D1, a wire R1.2-D1.2 and a GND label on D1.1 on Sheet 1; Sheet 2 empty. */
function twoSheets(): Snapshot {
  const s = fixture();
  s.project.schematic.sheets.sht_2 = { id: "sht_2", name: "Sheet 2", width: mm(297), height: mm(210) };
  s.project.schematic.wires.wir_1 = { id: "wir_1", sheet_id: "sht_1", a: pt(54, 50), b: pt(76, 50) };
  s.project.schematic.labels.lab_1.position = pt(84, 50);
  return s;
}

describe("slice B3 editing set in the schematic editor", () => {
  it("copy on one sheet, paste on another: new objects on that sheet, the net carried by its label name only", () => {
    const snap = twoSheets();
    const first = render(<SchematicView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_1" />);
    act(() => {
      fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    });
    fireEvent.keyDown(window, { key: "c", ctrlKey: true });
    first.unmount();
    render(<SchematicView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_2" />);
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });
    expect(sent).toHaveLength(1);
    const b = sent[0] as Extract<Command, { type: "Batch" }>;
    expect(b.type).toBe("Batch");
    expect(b.commands.map((c) => c.type).sort()).toEqual(["AddNetLabel", "AddWire", "PlaceSymbol", "PlaceSymbol"]);
    for (const c of b.commands) expect(c, c.type).toMatchObject({ sheet_id: "sht_2" });
    expect(b.commands.find((c) => c.type === "AddNetLabel")).toMatchObject({ name: "GND", kind: "power" });
  });

  it("right-click opens the registry's menu for the clicked object type and selects the object first", () => {
    const run = vi.fn();
    const asked: CanvasTarget[] = [];
    const menu = (t: CanvasTarget): ContextItem[] => {
      asked.push(t);
      return [
        { id: "copy", label: "Copy", run },
        { id: "group", label: "Group", disabled: "Unavailable: no group object", run: () => {} },
      ];
    };
    const r = render(<SchematicView snapshot={twoSheets()} selected={null} onSelect={() => {}} edit={edit()} contextMenu={menu} />);
    const body = r.container.querySelector('[data-symbol-id="sym_r1"] rect.body')!;
    fireEvent.contextMenu(body, client(50, 50));
    const m = within(screen.getByRole("menu", { name: "Part R1" }));
    expect(asked.at(-1)).toBe("symbol");
    expect(r.container.querySelector('[data-symbol-id="sym_r1"] .sel-box')).toBeTruthy();
    expect((m.getByRole("menuitem", { name: "Group" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(m.getByRole("menuitem", { name: "Copy" }));
    expect(run).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
    // the empty sheet asks for the sheet menu; Escape closes it
    fireEvent.contextMenu(r.container.querySelector("svg.canvas")!, client(200, 150));
    expect(asked.at(-1)).toBe("sheet");
    expect(screen.getByRole("menu", { name: "Sheet Sheet 1" })).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("the selection filter keeps switched-off kinds out of a click and Select all", () => {
    const r = render(<SchematicView snapshot={twoSheets()} selected={null} onSelect={() => {}} edit={edit()} />);
    act(() => setSelectFilter({ wire: false, label: false }));
    act(() => {
      fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    });
    expect(screen.getByText(/2 objects selected/)).toBeTruthy();
    act(() => setSelectFilter({ symbol: false, wire: true, label: true }));
    fireEvent.keyDown(window, { key: "Escape" });
    clickOn(r.container.querySelector('[data-symbol-id="sym_r1"] rect.body')!, [50, 50]);
    expect(r.container.querySelector(".sel-box")).toBeNull();
  });

  it("Ctrl+F opens find and replace; Replace all is one engine command for every match", () => {
    render(<SchematicView snapshot={twoSheets()} selected={null} onSelect={() => {}} edit={edit()} />);
    fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    const bar = within(screen.getByRole("search", { name: "Find and replace" }));
    fireEvent.change(bar.getByRole("textbox", { name: "Find" }), { target: { value: "1" } });
    fireEvent.change(bar.getByRole("textbox", { name: "Replace with" }), { target: { value: "5" } });
    fireEvent.click(bar.getByRole("checkbox", { name: "Values" }));
    fireEvent.click(bar.getByRole("checkbox", { name: "Net labels" }));
    expect(bar.getByRole("status").textContent).toBe("2 matches");
    fireEvent.click(bar.getByRole("button", { name: "Replace all" }));
    expect(sent).toEqual([
      {
        type: "Batch",
        label: 'Replace "1" with "5" in 2 object(s)',
        commands: [
          { type: "SetRefdes", symbol_id: "sym_d1", refdes: "D5" },
          { type: "SetRefdes", symbol_id: "sym_r1", refdes: "R5" },
        ],
      },
    ]);
    fireEvent.click(bar.getByRole("button", { name: "Select matches" }));
    expect(screen.getByText(/2 objects selected/)).toBeTruthy();
    fireEvent.click(bar.getByRole("button", { name: "Close find and replace" }));
    expect(screen.queryByRole("search", { name: "Find and replace" })).toBeNull();
  });

  it("publishes its operations for the action registry; each multi-object edit is one Batch", () => {
    render(
      <DocIdContext.Provider value="sch:sht_1">
        <SchematicView snapshot={twoSheets()} selected={null} onSelect={() => {}} edit={edit()} sheetId="sht_1" />
      </DocIdContext.Provider>,
    );
    const ops = () => editorContext("sch:sht_1")!.ops!;
    expect(ops().copy.state).toBe("Select something first");
    expect(ops().paste.state).toMatch(/^Nothing to paste/);
    expect(ops()["align-left"].state).toBe("Select two or more parts first");
    act(() => {
      fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    });
    expect(ops()["align-left"].state).toBe(true);
    expect(ops()["distribute-x"].state).toBe("Select three or more parts first");
    act(() => ops()["align-left"].run());
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({ type: "Batch", label: "Align 2 objects" });
    // Array: an inline panel, then one Batch of copies
    act(() => ops().array.run());
    const panel = within(screen.getByRole("group", { name: "Array" }));
    fireEvent.change(panel.getByRole("textbox", { name: "Columns" }), { target: { value: "2" } });
    fireEvent.click(panel.getByRole("button", { name: /^Create 1 copy/ }));
    expect(sent[1]).toMatchObject({ type: "Batch", label: "Array 2×1: 1 copies" });
    expect(screen.queryByRole("group", { name: "Array" })).toBeNull();
    // Group stays unavailable: no editor operation pretends to group
    expect(ops().group).toBeUndefined();
  });
});
