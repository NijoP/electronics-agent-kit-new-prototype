// PCB Layer Manager, finding navigation, airwires and the zone tool.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import type { EditCtx } from "./EditContext";
import { shellCmd } from "./shell.test-data";
import { fixture } from "./fixture.test-data";
import { useLayerView, sanitizePrefs } from "./layerView";
import { focusBounds } from "./pcbFocus";
import { PcbView, pcbBounds } from "./PcbView";
import { LayerManager } from "./shell/LayerManager";
import type { CheckResult, Command, CommandResponse, ConnectivityReport, Snapshot } from "./types";
import { fit, mm, type ViewBox } from "./viewport";

const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });
function withStackup(count = 4): Snapshot {
  const s = fixture();
  const inner = Array.from({ length: count - 2 }, (_, i) => ({ layer: { inner: i + 1 } as const, kind: "signal" as const, copper_oz: 0.5, dielectric_after_um: 200, er: 4.2 }));
  s.project.pcb.board.stackup = {
    layers: [
      { layer: "top", kind: "signal", copper_oz: 1, dielectric_after_um: 200, er: 4.2 },
      ...inner,
      { layer: "bottom", kind: "signal", copper_oz: 1, dielectric_after_um: 0, er: 4.2 },
    ],
    board_thickness: mm(1.6),
    dielectric: "FR-4",
  };
  return s;
}

describe("pure helpers", () => {
  it("focusBounds boxes the named objects and never zooms tighter than a few mm", () => {
    const s = fixture();
    const b = focusBounds(s, ["trk_1"], null)!;
    expect(b.maxX - b.minX).toBeGreaterThanOrEqual(8);
    const via = focusBounds(s, ["via_1"], null)!;
    expect(via.maxY - via.minY).toBeGreaterThanOrEqual(8);
    expect(focusBounds(s, ["nope"], null)).toBeNull();
    expect(focusBounds(s, [], pt(5, 5))).not.toBeNull();
  });
  it("layer prefs reject malformed storage", () => {
    expect(sanitizePrefs({ colours: { top: "red", in1: "#aabbcc" }, hidden: [1, "in1"], active: 3 })).toEqual({ active: "top", hidden: ["in1"], colours: { in1: "#aabbcc" }, airwires: true });
  });
});

function Harness({ snap, run }: { snap: Snapshot; run: (c: Command | Command[]) => Promise<void> }) {
  const view = useLayerView(null);
  return <LayerManager snapshot={snap} view={view} unrouted={2} edit={run} busy={false} />;
}

describe("layer manager", () => {
  it("lists the engine's layers; layer count and layer type go through engine commands", () => {
    const sent: (Command | Command[])[] = [];
    render(<Harness snap={withStackup(4)} run={async (c) => void sent.push(c)} />);
    const list = screen.getByRole("list", { name: "Layers" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(4);
    expect(within(list).getByText("In2.Cu")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Layer count"), { target: { value: "6" } });
    expect(sent[0]).toEqual({ type: "SetLayerCount", count: 6, board_thickness: null });
    fireEvent.change(screen.getByLabelText("Type In1.Cu"), { target: { value: "plane" } });
    const cmd = sent[1] as { type: string; stackup: { layers: { kind: string }[] } };
    expect(cmd.type).toBe("SetStackup");
    expect(cmd.stackup.layers[1].kind).toBe("plane");
    expect(cmd.stackup.layers).toHaveLength(4);
    expect(screen.getByRole("table", { name: "Stackup" })).toBeTruthy();
    expect(screen.getByText(/2 unrouted/)).toBeTruthy();
  });
  it("visibility and active layer are display state, not engine commands", () => {
    const sent: unknown[] = [];
    render(<Harness snap={withStackup(4)} run={async (c) => void sent.push(c)} />);
    fireEvent.click(screen.getByLabelText("Show In1.Cu"));
    expect((screen.getByLabelText("Show In1.Cu") as HTMLInputElement).checked).toBe(false);
    fireEvent.click(screen.getByLabelText("Active layer In2.Cu"));
    expect((screen.getByLabelText("Active layer In2.Cu") as HTMLInputElement).checked).toBe(true);
    fireEvent.change(screen.getByLabelText("Colour In1.Cu"), { target: { value: "#112233" } });
    expect((screen.getByLabelText("Colour In1.Cu") as HTMLInputElement).value).toBe("#112233");
    expect(sent).toEqual([]);
  });
  it("without a stackup it says so and does not invent layers", () => {
    render(<Harness snap={fixture()} run={async () => {}} />);
    expect(screen.getByText(/no explicit stackup/)).toBeTruthy();
    expect(within(screen.getByRole("list", { name: "Layers" })).getAllByRole("listitem")).toHaveLength(2);
  });
});

const SIZE = 1000;
let sent: Command[][];
const edit = (): EditCtx => ({ execute: async (c) => void sent.push(Array.isArray(c) ? c : [c]), busy: false, library: [], notify: vi.fn() });
function clickAt(el: Element, vb: ViewBox, x: number, y: number) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  const cx = (x - vb.x) * s + (SIZE - vb.w * s) / 2;
  const cy = (y - vb.y) * s + (SIZE - vb.h * s) / 2;
  fireEvent.pointerDown(el, { clientX: cx, clientY: cy, button: 0, buttons: 1 });
  fireEvent.pointerUp(el, { clientX: cx, clientY: cy, button: 0 });
}
beforeEach(() => {
  sent = [];
  localStorage.clear();
  Element.prototype.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});

describe("zone tool", () => {
  it("draws a zone on the active layer and net with a clearance through AddZone", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    fireEvent.click(screen.getByRole("button", { name: "Zone" }));
    const svg = container.querySelector("svg.canvas")!;
    const vb: ViewBox = fit(pcbBounds(fixture()));
    clickAt(svg, vb, 2, -2);
    clickAt(svg, vb, 20, -15);
    fireEvent.click(screen.getByRole("button", { name: /Finish zone/ }));
    expect(sent).toEqual([]); // no net chosen yet: refused, nothing sent
    fireEvent.change(screen.getByLabelText("Net"), { target: { value: "GND" } });
    clickAt(svg, vb, 2, -2);
    clickAt(svg, vb, 20, -15);
    fireEvent.change(screen.getByLabelText("Zone clearance"), { target: { value: "0.3" } });
    fireEvent.click(screen.getByRole("button", { name: /Finish zone/ }));
    expect(sent).toHaveLength(1);
    const z = sent[0][0] as Extract<Command, { type: "AddZone" }>;
    expect(z).toMatchObject({ type: "AddZone", layer: "top", net: "GND", clearance: 300_000 });
    expect(z.outline).toHaveLength(4);
  });
  it("the delete tool removes a zone through DeleteZone", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    const zone = container.querySelector("[data-zone-id]")!;
    fireEvent.pointerDown(zone, { clientX: 500, clientY: 500, button: 0, buttons: 1 });
    fireEvent.pointerUp(zone, { clientX: 500, clientY: 500, button: 0 });
    expect(sent[0]).toEqual([{ type: "DeleteZone", zone_id: "zon_1" }]);
  });
});

describe("airwires and finding highlight in the PCB view", () => {
  it("draws the engine's ratsnest lines and highlights a focused finding", () => {
    const { container } = render(
      <PcbView
        snapshot={fixture()}
        selected={null}
        onSelect={() => {}}
        airwires={[["N1", pt(11, 10), pt(19, 10)], ["GND", pt(5, 5), pt(9, 9)]]}
        focus={{ n: 1, objects: ["trk_1"], location: pt(15, 10) }}
      />,
    );
    expect(container.querySelectorAll(".airwire")).toHaveLength(2);
    expect(container.querySelector("[data-track-id].finding-hit")).toBeTruthy();
    expect(container.querySelector(".finding-marker")).toBeTruthy();
  });
});

// ---- App-level: DRC finding click opens the PCB, highlights, shows measured vs required ----
let revision: number;
const drc = (): CheckResult => ({
  report: {
    check: "DRC", project_id: "prj_1", revision, input_sha256: "ab", tool: "t", outcome: "ERRORS", errors: 1, warnings: 0, not_implemented: [], limitations: [],
    findings: [{ id: "DRC-001-abc", rule_id: "DRC-001", severity: "error", message: "clearance violation on N1", objects: ["trk_1"], location: pt(15, 10), measured: { value: 0.12, unit: "mm" }, required: { bound: "min", value: 0.2, unit: "mm" }, suggested_fix: "Move the track away or reduce the width." }],
  },
  evidence_path: null,
});
const ratsnest = (rev: number): ConnectivityReport => ({ revision: rev, net_groups: { N1: 2 }, shorts: [], ratsnest: [["N1", pt(11, 10), pt(19, 10)]] });
const resp = (): CommandResponse => ({ op_id: "o", command: "x", class: "reversible_local", accepted: true, revision_before: 7, revision_after: 8, created: [], affected: [], warnings: [], error: null, validation_note: "" });
beforeEach(() => {
  revision = 7;
  mockIPC((cmd) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects": return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project": case "current_project": return fixture(revision);
      case "events_since": return [];
      case "run_check": return drc();
      case "connectivity": return ratsnest(revision);
      case "library_components": return [];
      case "model_providers": return [];
      case "execute_command": return resp();
      default: { const h = shellCmd(cmd, null); if (h.handled) return h.value; throw new Error(`unmocked ${cmd}`); }
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  localStorage.clear();
});

describe("finding navigation", () => {
  it("click opens the PCB tab, highlights the object and shows measured vs required", async () => {
    render(<App />);
    fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
    await screen.findByRole("img", { name: "PCB canvas" });
    // airwires come from the connectivity command
    await waitFor(() => expect(document.querySelectorAll(".airwire")).toHaveLength(1));
    // close the PCB tab so navigation has to reopen it
    fireEvent.click(screen.getByRole("button", { name: "Close PCB" }));
    await waitFor(() => expect(screen.queryByRole("img", { name: "PCB canvas" })).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "PCB" })); // DRC lives in the PCB view
    fireEvent.click(screen.getByRole("button", { name: "Run DRC" }));
    fireEvent.click(await screen.findByRole("button", { name: /DRC-001 clearance violation on N1/ }));
    await screen.findByRole("img", { name: "PCB canvas" });
    await waitFor(() => expect(document.querySelector("[data-track-id].finding-hit")).toBeTruthy());
    const detail = await screen.findByRole("group", { name: "Finding detail" });
    expect(detail.textContent).toContain("0.12 mm");
    expect(detail.textContent).toContain("≥ 0.2 mm");
    expect(detail.textContent).toContain("Move the track away");
    expect(document.querySelector(".finding-marker")).toBeTruthy();
  });
  it("the Layers view is reachable from the activity bar", async () => {
    render(<App />);
    fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
    await screen.findByRole("img", { name: "PCB canvas" });
    fireEvent.click(screen.getByRole("button", { name: "PCB Layers" }));
    expect(await screen.findByRole("list", { name: "Layers" })).toBeTruthy();
  });
});
