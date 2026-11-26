// Phase 1 F2 (audit S2, S7, S15, M2): the PCB editor deletes, selects and edits its own objects.
//  * Edit › Delete in the PCB runs the PCB's published `delete` op (DeleteTrack / DeleteVia /
//    DeleteZone / DeleteFootprint, one Batch), never the schematic fallback `DeleteSymbol`;
//  * a track width or layer edit is `EditTrack`, so the same track id stays selected;
//  * vias and zones are selectable and edited in the inspector with `MoveVia` / `EditZone`;
//  * the Via tool has no Layer field (a via spans From..To; the active layer played no part).
// Written without running locally (no local builds or tests, captain's rule); CI runs it.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { workspaceActions, type ActionDeps } from "./actions/sets";
import type { EditCtx } from "./EditContext";
import { fixture } from "./fixture.test-data";
import { GRID_KINDS } from "./grid";
import { PcbView } from "./PcbView";
import { SIDE_VIEWS } from "./shell/ActivityBar";
import { NO_SELECTION, editorContext, resetInspectorStore, selectionModel, type PropField } from "./shell/inspectorModel";
import { DocIdContext } from "./shell/viewState";
import type { Command, Snapshot } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => null }));

const DOC = "pcb:prj_1";
let sent: Command[];
const edit = (): EditCtx => ({ execute: async (c) => void sent.push(...(Array.isArray(c) ? c : [c])), busy: false, library: [], notify: vi.fn() });

beforeEach(() => {
  sent = [];
  resetInspectorStore();
  localStorage.clear();
  Element.prototype.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: 1000, bottom: 1000, width: 1000, height: 1000, toJSON: () => ({}) }) as DOMRect;
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function press(el: Element) {
  fireEvent.pointerDown(el, { clientX: 500, clientY: 500, button: 0, buttons: 1 });
  fireEvent.pointerUp(el, { clientX: 500, clientY: 500, button: 0 });
}
function Pcb({ snap = fixture(), selected = null }: { snap?: Snapshot; selected?: string | null }) {
  return (
    <DocIdContext.Provider value={DOC}>
      <PcbView snapshot={snap} selected={selected} onSelect={() => {}} edit={edit()} />
    </DocIdContext.Provider>
  );
}

/** The workspace's action set, its editor ops read from the PCB's published context (as Workspace.tsx does). */
function actions(deleteSel: () => void) {
  const fn = () => vi.fn();
  const snapshot = fixture();
  const d: ActionDeps = {
    editorOp: (op) => {
      const o = editorContext(DOC)?.ops?.[op];
      return o ? { enabled: () => editorContext(DOC)?.ops?.[op]?.state ?? o.state, run: () => (editorContext(DOC)?.ops?.[op] ?? o).run() } : null;
    },
    snapshot,
    busy: false,
    selected: "sym_r1",
    selectedSymbol: snapshot.project.schematic.symbols["sym_r1"],
    fpOfSelected: snapshot.project.pcb.footprints["fp_r1"],
    theme: "dark",
    sideOpen: true,
    bottomOpen: true,
    rightOpen: true,
    split: false,
    recent: [],
    library: [],
    docs: [{ id: DOC, kind: "pcb", title: "PCB" }],
    openIds: [DOC],
    projects: [],
    sideViews: SIDE_VIEWS,
    edaViews: [],
    dialog: fn(), save: fn(), undo: fn(), redo: fn(), close: fn(), openRecent: fn(), execute: fn(), check: fn(), syncPcb: fn(),
    refillZones: fn(), rotateSel: fn(), deleteSel, tool: fn(), zoomBy: fn(), fit: fn(), select: fn(), toggleSplit: fn(),
    showSide: fn(), toggleSide: fn(), toggleBottom: fn(), toggleRight: fn(), showBottom: fn(), showRight: fn(), onTheme: fn(), resetLayout: fn(), onSettings: fn(), preferences: fn(),
    palette: fn(), quickOpen: fn(), help: fn(), about: fn(), sideView: "project", notify: fn(), open: fn(), closeAll: fn(), showWorkspace: fn(), switchTo: fn(), placePart: fn(),
    grid: { kind: "pcb", cfg: GRID_KINDS.pcb.defaults, set: fn() },
    toolbarLabels: false,
    toggleToolbarLabels: fn(),
    tour: fn(),
  };
  return workspaceActions(d);
}
const deleteAction = (deleteSel: () => void) => actions(deleteSel).find((a) => a.id === "delete")!;
const types = () => sent.map((c) => c.type);

describe("F2 acceptance: Edit › Delete in the PCB never sends DeleteSymbol (S2)", () => {
  it("a selected track is deleted with DeleteTrack, even while a symbol is cross-probed", () => {
    const { container } = render(<Pcb selected="sym_r1" />);
    press(container.querySelector('[data-track-id="trk_1"] line')!);
    const deleteSel = vi.fn();
    const del = deleteAction(deleteSel);
    expect(del.enabled()).toBe(true);
    del.run();
    expect(sent).toEqual([{ type: "DeleteTrack", track_id: "trk_1" }]);
    expect(deleteSel).not.toHaveBeenCalled();
    expect(types()).not.toContain("DeleteSymbol");
  });

  it("a selected via is deleted with DeleteVia", () => {
    const { container } = render(<Pcb />);
    press(container.querySelector('[data-via-id="via_1"] circle')!);
    expect(screen.getByRole("group", { name: "Via selection" })).toBeTruthy();
    deleteAction(vi.fn()).run();
    expect(sent).toEqual([{ type: "DeleteVia", via_id: "via_1" }]);
  });

  it("the cross-probed footprint is deleted with DeleteFootprint (after confirming), never DeleteSymbol", () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<Pcb selected="sym_r1" />);
    const deleteSel = vi.fn();
    deleteAction(deleteSel).run();
    expect(sent).toEqual([{ type: "DeleteFootprint", footprint_id: "fp_r1" }]);
    expect(deleteSel).not.toHaveBeenCalled();
  });

  it("a track and footprints together are ONE Batch of the PCB's own deletes", () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const { container } = render(<Pcb />);
    press(container.querySelector('[data-track-id="trk_1"] line')!);
    // Ctrl+A adds every footprint; the picked track stays selected.
    fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    deleteAction(vi.fn()).run();
    expect(sent).toHaveLength(1);
    const b = sent[0];
    expect(b.type).toBe("Batch");
    const inner = b.type === "Batch" ? b.commands.map((c) => c.type) : [];
    expect(inner[0]).toBe("DeleteTrack");
    expect(inner.length).toBeGreaterThan(1);
    expect(inner.slice(1).every((t) => t === "DeleteFootprint")).toBe(true);
    expect(inner).not.toContain("DeleteSymbol");
  });

  it("with nothing selected on the PCB, Delete is disabled with a reason", () => {
    render(<Pcb />);
    expect(deleteAction(vi.fn()).enabled()).toBe("Select a track, via, zone or footprint first");
  });

  it("the Delete key in the Select tool deletes the picked track the same way", () => {
    const { container } = render(<Pcb />);
    press(container.querySelector('[data-track-id="trk_1"] line')!);
    fireEvent.keyDown(window, { key: "Delete" });
    expect(sent).toEqual([{ type: "DeleteTrack", track_id: "trk_1" }]);
  });
});

describe("F2 acceptance: a width edit keeps the same track id selected (S7)", () => {
  it("Apply width sends EditTrack and the track stays selected after the new revision", () => {
    const snap = fixture();
    const { container, rerender } = render(<Pcb snap={snap} />);
    press(container.querySelector('[data-track-id="trk_1"] line')!);
    fireEvent.change(screen.getByLabelText("Selected track width"), { target: { value: "0.4" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply width" }));
    expect(sent).toEqual([{ type: "EditTrack", track_id: "trk_1", width: 400_000 }]);
    // The engine's next snapshot: the same track, now 0.4 mm wide.
    const next: Snapshot = structuredClone(snap);
    next.project.revision += 1;
    next.project.pcb.tracks["trk_1"].width = 400_000;
    rerender(<Pcb snap={next} />);
    expect(editorContext(DOC)!.selection.tracks).toEqual(["trk_1"]);
    expect(screen.getByRole("group", { name: "Track selection" })).toBeTruthy();
  });

  it("the inspector's width and layer edits are EditTrack too", () => {
    const m = selectionModel(fixture(), { ...NO_SELECTION, tracks: ["trk_1"] })!;
    const f = (id: string) => m.fields.find((x) => x.id === id)!;
    expect(f("width").edit!.commit("0.4")).toEqual({ type: "EditTrack", track_id: "trk_1", width: 400_000 });
    // trk_1's ends touch no pad, via or other track: a plain in-place layer change.
    expect(f("layer").edit!.commit("bottom")).toEqual({ type: "EditTrack", track_id: "trk_1", layer: "bottom" });
  });
});

describe("F2: vias and zones in Select and the inspector (M2)", () => {
  const field = (fields: PropField[], id: string) => fields.find((x) => x.id === id)!;

  it("a via's X/Y edit is MoveVia; its net, size and span are read-only with a reason", () => {
    const m = selectionModel(fixture(), { ...NO_SELECTION, vias: ["via_1"] })!;
    expect(m.title).toBe("Via GND");
    expect(field(m.fields, "x").edit!.commit("16")).toEqual({ type: "MoveVia", via_id: "via_1", position: { x: 16_000_000, y: 15_000_000 } });
    expect(field(m.fields, "x").edit!.commit("abc")).toMatch(/not a number/);
    expect(field(m.fields, "drill").value).toBe("0.300");
    expect(field(m.fields, "via-edit").unavailable).toMatch(/^Unavailable/);
  });

  it("a zone's net, layer and clearance edits are EditZone", () => {
    const m = selectionModel(fixture(), { ...NO_SELECTION, zones: ["zon_1"] })!;
    expect(m.title).toBe("Zone GND");
    expect(field(m.fields, "net").edit!.commit("N1")).toEqual({ type: "EditZone", zone_id: "zon_1", net: "N1" });
    expect(field(m.fields, "layer").edit!.commit("top")).toEqual({ type: "EditZone", zone_id: "zon_1", layer: "top" });
    expect(field(m.fields, "clearance").edit!.commit("0.5")).toEqual({ type: "EditZone", zone_id: "zon_1", clearance: 500_000 });
    expect(field(m.fields, "clearance").edit!.commit("-1")).toMatch(/negative/);
    expect(field(m.fields, "net").edit!.commit("GND")).toBeNull();
  });

  it("clicking a via in the Select tool publishes it as the selection", () => {
    const { container } = render(<Pcb />);
    press(container.querySelector('[data-via-id="via_1"] circle')!);
    expect(editorContext(DOC)!.selection.vias).toEqual(["via_1"]);
    expect(editorContext(DOC)!.selection.footprints).toEqual([]);
  });
});

describe("F2 (S15): the Via tool has no dead Layer field", () => {
  it("shows Net, From, To and Kind, and no Layer select", () => {
    render(<Pcb />);
    fireEvent.click(screen.getByRole("button", { name: "Via" }));
    expect(editorContext(DOC)!.options.map((o) => o.name)).toEqual(["Net", "Via from", "Via to", "Via kind"]);
  });
});
