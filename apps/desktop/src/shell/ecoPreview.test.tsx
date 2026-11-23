// ECO preview rows and selection rules (slice C2), and the review dialog's own controls.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { applyState, buildEcoRows, groupRows, toggleRow } from "./ecoPreview";
import { EcoPreviewDialog } from "./EcoPreviewDialog";
import type { EngineChange } from "./syncReport";

const fp = (o: Record<string, unknown>) => ({ id: "fp_x", symbol_id: "sym_1", component_id: "R_0603", refdes: "R1", side: "top", locked: false, position: { x: 0, y: 0 }, rotation: "R0", pad_nets: {}, ...o });
const ch = (key: string, before: unknown, after: unknown): EngineChange => ({ path: ["pcb", "footprints"], key, before, after });

const CHANGES: EngineChange[] = [
  ch("fp_new", null, fp({ id: "fp_new", symbol_id: "sym_2", refdes: "R2", pad_nets: { "1": "VCC", "2": "GND" } })),
  ch("fp_old", fp({ id: "fp_old", symbol_id: "sym_gone", refdes: "R9", pad_nets: { "1": "N3" } }), null),
  ch("fp_x", fp({ refdes: "R1", pad_nets: { "1": "N1", "2": "N2" } }), fp({ refdes: "R5", component_id: "R_0805", pad_nets: { "1": "N4", "3": "N5" } })),
];

afterEach(cleanup);

describe("ECO preview rows", () => {
  const rows = buildEcoRows(CHANGES);
  it("lists every change with action, object, before and after; pad nets of added or deleted footprints depend on them", () => {
    expect(rows.map((r) => [r.id, r.action, r.object, r.name, r.before, r.after, r.requires ?? ""])).toEqual([
      ["fp+:sym_2", "Add", "Footprint", "R2", "—", "R_0603", ""],
      ["net:sym_2:1", "Add", "Pad net", "R2.1", "—", "VCC", "fp+:sym_2"],
      ["net:sym_2:2", "Add", "Pad net", "R2.2", "—", "GND", "fp+:sym_2"],
      ["fp-:sym_gone", "Delete", "Footprint", "R9", "R_0603", "—", ""],
      ["net:sym_gone:1", "Delete", "Pad net", "R9.1", "N3", "—", "fp-:sym_gone"],
      ["ref:sym_1", "Update", "Reference", "R5", "R1", "R5", ""],
      ["part:sym_1", "Update", "Part", "R5", "R_0603", "R_0805", ""],
      ["net:sym_1:1", "Update", "Pad net", "R5.1", "N1", "N4", ""],
      ["net:sym_1:3", "Add", "Pad net", "R5.3", "—", "N5", ""],
      ["net:sym_1:2", "Delete", "Pad net", "R5.2", "N2", "—", ""],
    ]);
  });

  it("an in-sync PCB has no rows and nothing to apply", () => {
    expect(buildEcoRows([])).toEqual([]);
    expect(applyState([], new Set()).ok).toBe(false);
  });

  it("unchecking a footprint unchecks its pad nets; checking a pad net checks its footprint", () => {
    const all = new Set(rows.map((r) => r.id));
    const off = toggleRow(rows, all, "fp+:sym_2", false);
    expect(["fp+:sym_2", "net:sym_2:1", "net:sym_2:2"].some((id) => off.has(id))).toBe(false);
    expect(off.size).toBe(rows.length - 3);
    const back = toggleRow(rows, off, "net:sym_2:2", true);
    expect(back.has("fp+:sym_2") && back.has("net:sym_2:2") && !back.has("net:sym_2:1")).toBe(true);
    // independent rows stay independent
    expect(toggleRow(rows, all, "net:sym_1:1", false).size).toBe(rows.length - 1);
  });

  it("only the complete change list can be applied; a partial selection says why", () => {
    const all = new Set(rows.map((r) => r.id));
    expect(applyState(rows, all)).toEqual({ ok: true, reason: null });
    const partial = applyState(rows, toggleRow(rows, all, "ref:sym_1", false));
    expect(partial.ok).toBe(false);
    expect(partial.reason).toMatch(/^1 change\(s\) unchecked\. Applying only some changes is not available yet/);
  });

  it("groups by action or object in first-seen order", () => {
    expect(groupRows(rows, "action").map((g) => [g.group, g.rows.length])).toEqual([["Add", 4], ["Delete", 3], ["Update", 3]]);
    expect(groupRows(rows, "object").map((g) => [g.group, g.rows.length])).toEqual([["Footprint", 2], ["Pad net", 6], ["Reference", 1], ["Part", 1]]);
  });
});

describe("ECO review dialog", () => {
  const preview = { revision: 4, changes: CHANGES, warnings: ["U1 not placed on the PCB: pin 3 has no pad"] };

  it("Select all toggles every change; Apply is offered only with all checked; Escape closes", () => {
    const onApply = vi.fn();
    const onClose = vi.fn();
    render(<EcoPreviewDialog preview={preview} busy={false} onApply={onApply} onExport={vi.fn(async () => "x")} onClose={onClose} />);
    const dlg = screen.getByRole("dialog", { name: "Update PCB: review changes" });
    expect(within(dlg).getByText("U1 not placed on the PCB: pin 3 has no pad")).toBeTruthy();
    const apply = within(dlg).getByRole("button", { name: "Apply changes" }) as HTMLButtonElement;
    const all = within(dlg).getByRole("checkbox", { name: "Select all changes" }) as HTMLInputElement;
    expect(all.checked).toBe(true);
    fireEvent.click(all);
    expect(within(dlg).getAllByRole("checkbox").filter((c) => (c as HTMLInputElement).checked)).toHaveLength(0);
    expect(apply.disabled).toBe(true);
    expect(apply.title).toMatch(/10 change\(s\) unchecked/);
    fireEvent.click(all);
    expect(apply.disabled).toBe(false);
    fireEvent.click(apply);
    expect(onApply).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("a failed export says so instead of claiming the report was saved", async () => {
    render(<EcoPreviewDialog preview={preview} busy={false} onApply={vi.fn()} onExport={vi.fn(async () => Promise.reject(new Error("this project has no folder")))} onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Export report" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Report not saved");
  });
});
