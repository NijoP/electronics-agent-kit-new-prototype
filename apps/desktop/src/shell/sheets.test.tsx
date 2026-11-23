// Phase 1 B6 (ADR-0043): the sheet order, the delete batch and the Pages-tab sheet actions. Every
// action submits one typed command; the engine side (refusals, undo, netlist) is tested in
// crates/eak-runtime/tests/sheets.rs.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import type { Command, Snapshot } from "../types";
import { DesignManager } from "./DesignManager";
import { pagesOf } from "./designManagerData";
import { deleteSheetCommand, moveSheetCommand, nextSheetName, renameProblem, sheetContents, sheetsInOrder } from "./sheets";

/** The fixture (R1, D1, a wire and a GND label on sht_1) plus an empty "Power" sheet. */
function twoSheets(): Snapshot {
  const s = fixture(3);
  s.project.schematic.sheets.sht_2 = { id: "sht_2", name: "Power", width: 1, height: 1 };
  return s;
}

beforeEach(() => mockIPC((cmd) => (cmd === "netlist" ? [] : null)));
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("sheet model", () => {
  it("orders by `order`, then id; sheets without `order` keep their id order", () => {
    const s = twoSheets();
    s.project.schematic.sheets.sht_0 = { id: "sht_0", name: "Z", width: 1, height: 1 };
    expect(sheetsInOrder(s.project.schematic).map((x) => x.id)).toEqual(["sht_0", "sht_1", "sht_2"]);
    s.project.schematic.sheets.sht_2.order = 1;
    s.project.schematic.sheets.sht_1.order = 2;
    s.project.schematic.sheets.sht_0.order = 3;
    expect(sheetsInOrder(s.project.schematic).map((x) => x.id)).toEqual(["sht_2", "sht_1", "sht_0"]);
    // The Pages tab lists sheets in that order, the board last.
    expect(pagesOf(s).map((p) => p.label)).toEqual(["Power", "Sheet 1", "Z", "PCB"]);
  });

  it("moves one place up or down and refuses to move past either end", () => {
    const sch = twoSheets().project.schematic;
    expect(moveSheetCommand(sch, "sht_2", -1)).toEqual({ type: "ReorderSheets", sheet_ids: ["sht_2", "sht_1"] });
    expect(moveSheetCommand(sch, "sht_1", 1)).toEqual({ type: "ReorderSheets", sheet_ids: ["sht_2", "sht_1"] });
    expect(moveSheetCommand(sch, "sht_1", -1)).toBeNull();
    expect(moveSheetCommand(sch, "sht_2", 1)).toBeNull();
  });

  it("deletes an empty sheet alone and a non-empty one in one batch that empties it first", () => {
    const s = twoSheets();
    expect(sheetContents(s, "sht_2")).toEqual([]);
    expect(deleteSheetCommand(s, "sht_2")).toEqual({ type: "DeleteSheet", sheet_id: "sht_2" });
    expect(sheetContents(s, "sht_1")).toEqual(["2 symbol(s)", "1 wire(s)", "1 net label(s)"]);
    const c = deleteSheetCommand(s, "sht_1");
    if (c.type !== "Batch") throw new Error(`expected a Batch, got ${c.type}`);
    expect(c.label).toBe("Delete sheet Sheet 1");
    expect(c.commands.map((x) => x.type)).toEqual(["DeleteNetLabel", "DeleteWire", "DeleteSymbol", "DeleteSymbol", "DeleteSheet"]);
    expect(c.commands.at(-1)).toEqual({ type: "DeleteSheet", sheet_id: "sht_1" });
  });

  it("names new sheets after the count and checks renames like the engine", () => {
    const sch = twoSheets().project.schematic;
    expect(nextSheetName(sch)).toBe("Sheet 3");
    sch.sheets.sht_2.name = "Sheet 3";
    expect(nextSheetName(sch)).toBe("Sheet 4");
    expect(renameProblem(sch, "sht_2", "  ")).toMatch(/needs a name/);
    expect(renameProblem(sch, "sht_2", " Sheet 1 ")).toMatch(/already named/);
    expect(renameProblem(sch, "sht_2", "Analog")).toBeNull();
  });
});

describe("Pages tab sheet actions", () => {
  const mount = (s: Snapshot, onCommand: (c: Command) => void, busy = false) =>
    render(<DesignManager snapshot={s} conn={null} selected={null} activeDoc={null} tab="pages" onTab={() => {}} onNavigate={() => {}} onCommand={onCommand} busy={busy} />);

  it("Add sheet submits AddSheet with the next free name", () => {
    const run = vi.fn();
    mount(twoSheets(), run);
    fireEvent.click(within(screen.getByRole("toolbar", { name: "Sheet actions" })).getByRole("button", { name: "Add sheet" }));
    expect(run).toHaveBeenCalledWith({ type: "AddSheet", name: "Sheet 3" });
  });

  it("Rename edits inline: Enter submits RenameSheet, a clash is refused in place, Escape cancels", () => {
    const run = vi.fn();
    mount(twoSheets(), run);
    fireEvent.click(screen.getByRole("button", { name: "Rename sheet Power" }));
    const input = screen.getByRole("textbox", { name: "Sheet name" });
    fireEvent.change(input, { target: { value: "Sheet 1" } });
    fireEvent.submit(screen.getByRole("form", { name: "Rename sheet" }));
    expect(screen.getByRole("alert").textContent).toMatch(/already named/);
    expect(run).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: " Analog " } });
    fireEvent.submit(screen.getByRole("form", { name: "Rename sheet" }));
    expect(run).toHaveBeenCalledWith({ type: "RenameSheet", sheet_id: "sht_2", name: "Analog" });
    expect(screen.queryByRole("form", { name: "Rename sheet" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Rename sheet Power" }));
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Sheet name" }), { key: "Escape" });
    expect(screen.queryByRole("form", { name: "Rename sheet" })).toBeNull();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("Move up/down submit ReorderSheets and are disabled, with the reason, at either end", () => {
    const run = vi.fn();
    mount(twoSheets(), run);
    const up = screen.getByRole("button", { name: "Move sheet Sheet 1 up" }) as HTMLButtonElement;
    expect(up.disabled).toBe(true);
    expect(up.title).toBe("Already the first sheet");
    expect((screen.getByRole("button", { name: "Move sheet Power down" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Move sheet Power up" }));
    expect(run).toHaveBeenCalledWith({ type: "ReorderSheets", sheet_ids: ["sht_2", "sht_1"] });
  });

  it("Delete removes an empty sheet at once and asks first, listing what goes, for a non-empty one", () => {
    const run = vi.fn();
    mount(twoSheets(), run);
    fireEvent.click(screen.getByRole("button", { name: "Delete sheet Power" }));
    expect(run).toHaveBeenLastCalledWith({ type: "DeleteSheet", sheet_id: "sht_2" });
    fireEvent.click(screen.getByRole("button", { name: "Delete sheet Sheet 1" }));
    const dialog = screen.getByRole("dialog", { name: "Delete sheet" });
    expect(dialog.textContent).toMatch(/2 symbol\(s\), 1 wire\(s\), 1 net label\(s\)/);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Delete sheet" })).toBeNull();
    expect(run).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Delete sheet Sheet 1" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Delete sheet" })).getByRole("button", { name: "Delete sheet and contents" }));
    const last = run.mock.calls.at(-1)![0] as Command;
    expect(last.type).toBe("Batch");
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("the only sheet cannot be deleted, and nothing is clickable while a command is in flight", () => {
    const run = vi.fn();
    const { unmount } = mount(fixture(3), run);
    const del = screen.getByRole("button", { name: "Delete sheet Sheet 1" }) as HTMLButtonElement;
    expect(del.disabled).toBe(true);
    expect(del.title).toBe("A project keeps at least one sheet");
    unmount();
    mount(twoSheets(), run, true);
    for (const b of within(screen.getByRole("group", { name: "Sheet Power actions" })).getAllByRole("button")) expect((b as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Add sheet" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("without onCommand the Pages tab only navigates (no sheet actions)", () => {
    render(<DesignManager snapshot={twoSheets()} conn={null} selected={null} activeDoc={null} tab="pages" onTab={() => {}} onNavigate={() => {}} />);
    expect(screen.queryByRole("toolbar", { name: "Sheet actions" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Rename sheet Power" })).toBeNull();
  });
});
