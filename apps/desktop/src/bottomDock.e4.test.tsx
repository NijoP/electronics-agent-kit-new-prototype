// Phase 1 slice E4: the bottom dock's Property List (N cells edited = ONE engine command, so one
// revision and one undo step), Diagnostics (Unknown is never shown as OK) and Find Results (the
// B3 find bar's search, listed for every sheet, a click selecting the hit in its sheet).
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fixture } from "./fixture.test-data";
import { FALLBACK_PROFILE, type ResourceProfile } from "./resourceProfile";
import { SchematicView } from "./SchematicView";
import { BottomPanel } from "./shell/BottomPanel";
import { DiagnosticsTab } from "./shell/DiagnosticsTab";
import { modelRows, performanceRows, recoveryRows, taskRows } from "./shell/diagnostics";
import { findSearch, publishFindSearch, requestFindPick, resetFindStore, useFindReveal } from "./shell/findResults";
import { FindResultsTab } from "./shell/FindResultsTab";
import { resetInspectorStore } from "./shell/inspectorModel";
import { commitCells, commonValue, listModel, rowRange } from "./shell/propertyListModel";
import { DocIdContext } from "./shell/viewState";
import { DEFAULT_FIND } from "./tools/schematic/find";
import { FindBar } from "./tools/schematic/FindBar";
import type { Command, ConnectionStatus, ProviderView, Snapshot } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => null }));

beforeEach(() => {
  resetInspectorStore();
  resetFindStore();
  Element.prototype.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: 1000, bottom: 1000, width: 1000, height: 1000, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

const batchOf = (c: Command | string | null) => (c && typeof c === "object" && c.type === "Batch" ? c.commands : null);

describe("Property List model", () => {
  it("lists every object of a kind as a row, sorted, with only fields the IR holds as columns", () => {
    const m = listModel(fixture(), "symbols");
    expect(m.rows.map((r) => r.label)).toEqual(["D1", "R1"]);
    const ids = m.columns.map((c) => c.id);
    expect(ids).toEqual(expect.arrayContaining(["refdes", "value", "component", "x", "y", "rotation"]));
    // unavailable fields (supplier, BOM, ...) and the Group row are not columns
    for (const id of ["supplier", "bom", "group"]) expect(ids).not.toContain(id);
    // F10 (S16): the IR holds the part's manufacturer / MPN, so it is a read-only column
    expect(m.columns.find((c) => c.id === "mpn")?.editable).toBe(false);
    expect(m.columns.find((c) => c.id === "value")?.editable).toBe(true);
    expect(m.columns.find((c) => c.id === "component")?.editable).toBe(false);
    expect(m.columns.find((c) => c.id === "rotation")?.choices?.map((c) => c.value)).toEqual(["R0", "R90", "R180", "R270"]);
    expect(m.rows.find((r) => r.label === "R1")?.cells.value).toBe("330");
  });

  it("N picked cells commit as ONE Batch (one revision, one undo step); one cell as one command", () => {
    const s = fixture();
    const two = commitCells(s, "symbols", "value", ["sym_r1", "sym_d1"], "4k7");
    expect(batchOf(two)).toEqual([
      { type: "SetSymbolValue", symbol_id: "sym_r1", value: "4k7" },
      { type: "SetSymbolValue", symbol_id: "sym_d1", value: "4k7" },
    ]);
    expect(commitCells(s, "symbols", "value", ["sym_r1"], "4k7")).toEqual({ type: "SetSymbolValue", symbol_id: "sym_r1", value: "4k7" });
    expect(batchOf(commitCells(s, "footprints", "x", ["fp_r1", "fp_d1"], "12.5"))?.map((c) => c.type)).toEqual(["MoveFootprint", "MoveFootprint"]);
  });

  it("refuses with a reason instead of sending something wrong", () => {
    const s = fixture();
    expect(commitCells(s, "symbols", "value", ["sym_r1"], "330")).toBeNull(); // no change
    expect(commitCells(s, "symbols", "refdes", ["sym_r1", "sym_d1"], "R9")).toMatch(/designators are unique/);
    expect(commitCells(s, "symbols", "component", ["sym_r1"], "X")).toMatch(/read-only/);
    expect(commitCells(s, "symbols", "x", ["sym_r1", "sym_d1"], "abc")).toMatch(/not a number/);
    expect(commitCells(s, "symbols", "value", [], "1")).toMatch(/Select/);
    const locked: Snapshot = fixture();
    for (const f of Object.values(locked.project.pcb.footprints)) f.locked = true;
    expect(commitCells(locked, "footprints", "x", ["fp_r1", "fp_d1"], "1")).toMatch(/locked/);
  });

  it("starts a batch from the common value and picks Shift ranges in grid order", () => {
    const m = listModel(fixture(), "symbols");
    expect(commonValue(m.rows, "rotation", ["sym_r1", "sym_d1"])).toBe("R0");
    expect(commonValue(m.rows, "value", ["sym_r1", "sym_d1"])).toBe("");
    expect(rowRange(m.rows, "sym_r1", "sym_d1")).toEqual(["sym_d1", "sym_r1"]);
  });
});

describe("Property List tab", () => {
  it("Pick every Value cell, type, Apply: execute is called ONCE with one Batch", async () => {
    const sent: Command[] = [];
    render(
      <BottomPanel tab="proplist" onTab={() => {}} snapshot={fixture()} results={{}} events={[]} onProbe={() => {}} onClose={() => {}} execute={async (c) => void sent.push(c)} />,
    );
    const list = within(screen.getByRole("region", { name: "Property list" }));
    fireEvent.click(list.getByRole("button", { name: "Pick every Value cell" }));
    expect(list.getByLabelText("Batch selection").textContent).toBe("2 Value cells picked");
    fireEvent.change(list.getByRole("textbox", { name: "New value" }), { target: { value: "1k" } });
    fireEvent.click(list.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].type).toBe("Batch");
    expect(batchOf(sent[0])).toHaveLength(2);
  });

  it("mixed cells with an empty field: Apply is disabled with the reason until a value is given (F3, S3)", () => {
    const sent: Command[] = [];
    const snap = fixture();
    snap.project.schematic.symbols.sym_d1.rotation = "R90";
    render(<BottomPanel tab="proplist" onTab={() => {}} snapshot={snap} results={{}} events={[]} onProbe={() => {}} onClose={() => {}} execute={async (c) => void sent.push(c)} />);
    const list = within(screen.getByRole("region", { name: "Property list" }));
    const apply = () => list.getByRole("button", { name: "Apply" }) as HTMLButtonElement;
    // Value: 330 and Green, so the field starts empty; Apply would blank both
    fireEvent.click(list.getByRole("button", { name: "Pick every Value cell" }));
    expect(apply().disabled).toBe(true);
    expect(apply().title).toMatch(/different values/);
    fireEvent.keyDown(list.getByRole("textbox", { name: "New value" }), { key: "Enter" });
    expect(sent).toHaveLength(0);
    fireEvent.change(list.getByRole("textbox", { name: "New value" }), { target: { value: "1k" } });
    expect(apply().disabled).toBe(false);
    // Rotation: R0 and R90 show "(mixed)"; the engine would refuse "", so Apply waits for a choice
    fireEvent.click(list.getByRole("button", { name: "Pick every Rotation cell" }));
    expect(apply().disabled).toBe(true);
    fireEvent.change(list.getByRole("combobox", { name: "New value" }), { target: { value: "R180" } });
    expect(apply().disabled).toBe(false);
    expect(sent).toHaveLength(0);
  });

  it("Ctrl+click adds and removes cells of one column; a click in another column starts over", () => {
    render(<BottomPanel tab="proplist" onTab={() => {}} snapshot={fixture()} results={{}} events={[]} onProbe={() => {}} onClose={() => {}} execute={async () => {}} />);
    const list = within(screen.getByRole("region", { name: "Property list" }));
    const sel = () => list.getByLabelText("Batch selection").textContent;
    fireEvent.click(list.getByRole("button", { name: "R1 Value" }));
    fireEvent.click(list.getByRole("button", { name: "D1 Value" }), { ctrlKey: true });
    expect(sel()).toBe("2 Value cells picked");
    expect(list.getByRole("button", { name: "D1 Value" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(list.getByRole("button", { name: "D1 Value" }), { ctrlKey: true });
    expect(sel()).toBe("1 Value cell picked");
    fireEvent.click(list.getByRole("button", { name: "D1 Rotation" }), { ctrlKey: true });
    expect(sel()).toBe("1 Rotation cell picked");
    expect(list.getByRole("button", { name: "R1 Value" }).getAttribute("aria-pressed")).toBe("false");
  });

  it("without an engine connection the grid is read-only: Apply stays disabled with the reason", () => {
    render(<BottomPanel tab="proplist" onTab={() => {}} snapshot={fixture()} results={{}} events={[]} onProbe={() => {}} onClose={() => {}} />);
    const apply = screen.getByRole("button", { name: "Apply" }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    expect(apply.title).toMatch(/read-only/);
  });
});

const status = (state: ConnectionStatus["state"], over: Partial<ConnectionStatus> = {}): ConnectionStatus => ({ state, error_class: null, message: null, checked_at_ms: null, model: null, ...over });
const provider = (id: string, s: ConnectionStatus, over: Partial<ProviderView> = {}): ProviderView => ({
  id, label: id, provider: "ollama", kind: "ollama", base_url: null, model: null, org_id: null, project_id: null, models: [], status: s,
  provider_name: "Ollama", endpoint: "http://127.0.0.1:11434", has_key: false, needs_key: false, key_error: null, is_default: false, ...over,
});

describe("Diagnostics rows: Unknown is never OK", () => {
  it("model connectivity: untested, unreadable and still-loading are Unknown; only a passed test is OK", () => {
    expect(modelRows([provider("a", status("untested"))], null)[0].state).toBe("unknown");
    expect(modelRows(null, "ipc down")[0].state).toBe("unknown");
    expect(modelRows(null, null)[0].state).toBe("unknown");
    expect(modelRows([], null)[0].state).toBe("info");
    expect(modelRows([provider("a", status("connected", { checked_at_ms: 1, model: "m" }))], null)[0].state).toBe("ok");
    expect(modelRows([provider("a", status("failed", { message: "refused" }))], null)[0]).toMatchObject({ state: "error", providerId: "a" });
    // a provider that needs a key it does not have is not "connected", whatever its old status says
    expect(modelRows([provider("a", status("connected"), { needs_key: true, has_key: false })], null)[0].state).toBe("warning");
  });

  it("performance: a profile not read yet, or the fallback used when reading failed, is Unknown", () => {
    expect(performanceRows(null).map((r) => r.state)).toEqual(["unknown"]);
    expect(performanceRows(FALLBACK_PROFILE).map((r) => r.state)).toEqual(["unknown"]);
    const p: ResourceProfile = { ...FALLBACK_PROFILE, mode: "balanced", logical_cpus: 8, limitations: ["GPU not detected"] };
    const rows = performanceRows(p);
    expect(rows.some((r) => r.state === "ok")).toBe(false);
    expect(rows.find((r) => r.id === "perf-cpus")?.detail).toBe("8");
    expect(rows.find((r) => r.id === "perf-limit-0")).toMatchObject({ state: "warning", detail: "GPU not detected" });
  });

  it("recovery: an unknown outcome is Unknown and blocks are errors; a clean open is OK", () => {
    expect(recoveryRows(fixture()).map((r) => [r.id, r.state])).toEqual([["rec-clean", "ok"]]);
    const s: Snapshot = { ...fixture(), blocked: "an interrupted operation has an unknown outcome", recovery: { reconciled: [["op_1", "unknown_outcome"], ["op_2", "committed"]], torn_temp_file_removed: true } };
    expect(recoveryRows(s).map((r) => [r.id, r.state])).toEqual([
      ["rec-blocked", "error"],
      ["rec-op_1", "unknown"],
      ["rec-op_2", "info"],
      ["rec-torn", "warning"],
    ]);
  });

  it("tasks: none run is information, a failure is an error with its reason", () => {
    expect(taskRows([]).map((r) => r.state)).toEqual(["info"]);
    const rows = taskRows([
      { id: 1, label: "Run DRC", status: "done", startedAt: 0, endedAt: 1 },
      { id: 2, label: "Export", status: "failed", startedAt: 0, endedAt: 1, detail: "disk full" },
    ]);
    expect(rows.find((r) => r.id === "tasks-failed")).toMatchObject({ state: "error" });
    expect(rows.find((r) => r.id === "tasks-failed")?.detail).toMatch(/Export: disk full/);
  });

  it("the tab shows Unknown (not OK) when the IPC gives no provider list and no profile", async () => {
    render(<DiagnosticsTab snapshot={fixture()} tasks={[]} />);
    const tab = within(screen.getByRole("region", { name: "Diagnostics" }));
    await waitFor(() => expect(tab.getByRole("table", { name: "Model connectivity" }).querySelector('[data-row="model-error"] [data-state="unknown"]')).toBeTruthy());
    await waitFor(() => expect(tab.getByRole("table", { name: "Performance profile" }).querySelector('[data-state="unknown"]')).toBeTruthy());
    for (const name of ["Model connectivity", "Performance profile"]) expect(tab.getByRole("table", { name }).querySelector('[data-state="ok"]')).toBeNull();
    expect(tab.getByRole("table", { name: "Recovery" }).querySelector('[data-row="rec-clean"] [data-state="ok"]')?.textContent).toBe("OK");
  });
});

describe("Find Results (wired to the B3 find bar)", () => {
  it("lists the bar's search over every sheet from the live snapshot; a click picks the hit", () => {
    const picks: string[] = [];
    publishFindSearch("r", DEFAULT_FIND);
    render(<FindResultsTab snapshot={fixture()} onPick={(h) => picks.push(`${h.kind}:${h.id}:${h.field}`)} />);
    const region = within(screen.getByRole("region", { name: "Find results" }));
    expect(region.getByRole("status").textContent).toMatch(/^2 matches for “r”/);
    fireEvent.click(region.getByRole("button", { name: "R1" }));
    fireEvent.click(region.getByRole("button", { name: "Green" }));
    expect(picks).toEqual(["symbol:sym_r1:refdes", "symbol:sym_d1:value"]);
  });

  it("says how to search when the bar has no query", () => {
    render(<FindResultsTab snapshot={fixture()} onPick={() => {}} />);
    expect(screen.getByText(/No search yet/)).toBeTruthy();
  });

  it("the find bar publishes its query, and List in Find Results asks for the tab", () => {
    let reveals = 0;
    function Reveal() {
      reveals = useFindReveal();
      return null;
    }
    render(
      <>
        <Reveal />
        <FindBar snapshot={fixture()} sheetId="sht_1" busy={false} onSelect={() => {}} onReplace={() => {}} onClose={() => {}} />
      </>,
    );
    const list = screen.getByRole("button", { name: "List in Find Results" }) as HTMLButtonElement;
    expect(list.disabled).toBe(true);
    fireEvent.change(screen.getByRole("textbox", { name: "Find" }), { target: { value: "GND" } });
    expect(findSearch()?.query).toBe("GND");
    const before = reveals;
    fireEvent.click(list);
    expect(reveals).toBe(before + 1);
  });

  it("the schematic editor of the hit's sheet takes the pick and selects the net label", () => {
    requestFindPick({ kind: "label", id: "lab_1", sheet: "sht_1" });
    const { container } = render(
      <DocIdContext.Provider value="doc">
        <SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} />
      </DocIdContext.Provider>,
    );
    expect(container.querySelector('g[data-label-id="lab_1"]')?.getAttribute("class")).toMatch(/\bselected\b/);
    // taken once: a later mount does not select it again
    cleanup();
    const again = render(
      <DocIdContext.Provider value="doc">
        <SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} />
      </DocIdContext.Provider>,
    );
    expect(again.container.querySelector('g[data-label-id="lab_1"]')?.getAttribute("class")).not.toMatch(/\bselected\b/);
    act(() => resetFindStore());
  });
});
