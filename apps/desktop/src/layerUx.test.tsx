// Phase 1 slice C1 (layer UX): layer locks block selection and edits, active-layer-exclusive
// picking, the canvas layer tab strip and layer keys, colour presets, flip-board and outline view
// modes, and per-project persistence of the layer view.
import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "./EditContext";
import { fixture } from "./fixture.test-data";
import { commandLayers, lockProblem, objectLocked, pickableOn } from "./layerLock";
import { statusLayerName } from "./layers";
import { COLOUR_PRESETS, sanitizePrefs, stepLayer, useLayerView } from "./layerView";
import { PcbView, pcbBounds } from "./PcbView";
import { buildPcbScene } from "./render/pcbScene";
import { LayerManager } from "./shell/LayerManager";
import type { Command, CopperLayer, Snapshot } from "./types";
import { fit, type ViewBox } from "./viewport";

const SIZE = 1000;
let sent: Command[];
let notify: ReturnType<typeof vi.fn<(m: string) => void>>;
const edit = (): EditCtx => ({ execute: async (c) => void sent.push(...(Array.isArray(c) ? c : [c])), busy: false, library: [], notify });
const nm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: nm(x), y: nm(y) });

beforeEach(() => {
  sent = [];
  notify = vi.fn<(m: string) => void>();
  localStorage.clear();
  Element.prototype.getBoundingClientRect = () =>
    ({ x: 0, y: 0, left: 0, top: 0, right: SIZE, bottom: SIZE, width: SIZE, height: SIZE, toJSON: () => ({}) }) as DOMRect;
});
afterEach(() => {
  cleanup();
  localStorage.clear();
});

/** Client point of view point (x, y) (view mm, Y down); mirrored when the board is flipped. */
function client(vb: ViewBox, x: number, y: number, mirror = false) {
  const s = Math.min(SIZE / vb.w, SIZE / vb.h);
  const cx = (x - vb.x) * s + (SIZE - vb.w * s) / 2;
  return { clientX: mirror ? SIZE - cx : cx, clientY: (y - vb.y) * s + (SIZE - vb.h * s) / 2 };
}
function press(el: Element, at = { clientX: 500, clientY: 500 }) {
  fireEvent.pointerDown(el, { ...at, button: 0, buttons: 1 });
  fireEvent.pointerUp(el, { ...at, button: 0 });
}

/** The Layers panel and the PCB view sharing one layer view, as in the workspace. */
function Board({ snap = fixture() }: { snap?: Snapshot }) {
  const view = useLayerView(null);
  return (
    <>
      <LayerManager snapshot={snap} view={view} unrouted={0} edit={async () => {}} busy={false} />
      <PcbView snapshot={snap} selected={null} onSelect={() => {}} edit={edit()} layerView={view} />
    </>
  );
}

describe("layer lock (acceptance: locking a layer blocks selection and edits on it)", () => {
  it("a locked F.Cu track, footprint and via cannot be selected, deleted or added; unlocking restores them", () => {
    const { container } = render(<Board />);
    const track = () => container.querySelector('[data-track-id="trk_1"] line')!;
    // Unlocked: a click selects the track.
    press(track());
    expect(screen.getByRole("group", { name: "Track selection" })).toBeTruthy();

    fireEvent.click(screen.getByLabelText("Lock F.Cu"));
    // The selected track drops out, and a click no longer selects it or the F.Cu footprint.
    expect(screen.queryByRole("group", { name: "Track selection" })).toBeNull();
    press(track());
    expect(screen.queryByRole("group", { name: "Track selection" })).toBeNull();
    press(container.querySelector('[data-footprint-id="fp_r1"] rect.courtyard')!);
    expect(screen.queryByRole("group", { name: "Footprint selection" })).toBeNull();
    expect(screen.getByRole("tab", { name: "F.Cu" }).getAttribute("data-locked")).toBe("true");

    // Edits: the delete tool does not pick it; a via spanning F.Cu is refused with the reason.
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    press(track());
    fireEvent.click(screen.getByRole("button", { name: "Via" }));
    fireEvent.change(screen.getByLabelText("Net"), { target: { value: "GND" } });
    const vb = fit(pcbBounds(fixture()));
    press(container.querySelector("svg.canvas")!, client(vb, 25, -5));
    expect(sent).toEqual([]);
    expect(notify).toHaveBeenLastCalledWith(expect.stringMatching(/F\.Cu is locked/));

    // Unlock: the same via goes through, and the track can be selected again.
    fireEvent.click(screen.getByLabelText("Lock F.Cu"));
    press(container.querySelector("svg.canvas")!, client(vb, 25, -5));
    expect(sent).toEqual([{ type: "AddVia", position: pt(25, 5), net: "GND", drill: null, diameter: null, from: "top", to: "bottom" }]);
    fireEvent.click(screen.getByRole("button", { name: "Select" }));
    press(track());
    expect(screen.getByRole("group", { name: "Track selection" })).toBeTruthy();
  });

  it("a locked board outline refuses outline edits", () => {
    render(<Board />);
    fireEvent.click(screen.getByLabelText("Lock Board outline"));
    expect((screen.getByLabelText("Lock Board outline") as HTMLInputElement).checked).toBe(true);
    expect(lockProblem(fixture(), new Set(["outline"]), { type: "SetBoardOutline", points: [] })).toMatch(/Board outline is locked/);
  });

  it("lock rules: objects sit on their layers, commands name the layers they change", () => {
    const s = fixture();
    const top = new Set(["top"]);
    expect(objectLocked(s, top, "track", "trk_1")).toBe(true);
    expect(objectLocked(s, top, "zone", "zon_1")).toBe(false);
    expect(objectLocked(s, top, "via", "via_1")).toBe(true); // a through via spans F.Cu
    expect(objectLocked(s, top, "footprint", "fp_r1")).toBe(true);
    expect(objectLocked(s, new Set(), "track", "trk_1")).toBe(false);
    const batch: Command = { type: "Batch", commands: [{ type: "DeleteTrack", track_id: "trk_1" }, { type: "AddZone", layer: "bottom", net: "GND", outline: [], clearance: null }] };
    expect(commandLayers(s, batch).sort()).toEqual(["bottom", "top"]);
    expect(lockProblem(s, new Set(["bottom"]), batch)).toMatch(/^B\.Cu is locked/);
    expect(lockProblem(s, top, { type: "MoveFootprint", footprint_id: "fp_r1", position: pt(1, 1), rotation: null, side: null })).toMatch(/F\.Cu is locked/);
    expect(lockProblem(s, new Set(["in1"]), { type: "AutoRoute", nets: null })).toBeNull(); // 2-layer board: no In1
    expect(lockProblem(s, top, { type: "SetSymbolValue", symbol_id: "sym_r1", value: "1k" })).toBeNull();
  });
});

describe("active-layer-exclusive picking", () => {
  it("tracks and zones pick only on their own layer, vias on any layer they span, footprints anywhere", () => {
    const s = fixture();
    expect(pickableOn(s, "track", "trk_1", "top")).toBe(true);
    expect(pickableOn(s, "track", "trk_1", "bottom")).toBe(false);
    expect(pickableOn(s, "zone", "zon_1", "top")).toBe(false);
    expect(pickableOn(s, "via", "via_1", "bottom")).toBe(true);
    expect(pickableOn(s, "footprint", "fp_r1", "bottom")).toBe(true);
  });
  it("with B.Cu active, a click on the F.Cu track selects nothing", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    fireEvent.click(screen.getByRole("tab", { name: "B.Cu" }));
    press(container.querySelector('[data-track-id="trk_1"] line')!);
    expect(screen.queryByRole("group", { name: "Track selection" })).toBeNull();
    // Off-layer copper lets the pointer through in the SVG view, as the canvas pick table does.
    expect(container.querySelector('[data-track-id="trk_1"]')!.getAttribute("class")).toContain("off-layer");
  });
});

describe("canvas layer tab strip and layer keys", () => {
  const four = (): Snapshot => {
    const s = fixture();
    const l = (layer: CopperLayer) => ({ layer, kind: "signal" as const, copper_oz: 1, dielectric_after_um: 200, er: 4.2 });
    s.project.pcb.board.stackup = { layers: [l("top"), l({ inner: 1 }), l({ inner: 2 }), l("bottom")], board_thickness: nm(1.6), dielectric: "FR-4" };
    return s;
  };
  const active = () => screen.getByRole("tablist", { name: "Active copper layer" }).querySelector('[aria-selected="true"]')!.getAttribute("data-layer");

  it("lists the stack's copper layers; a tab click, + / -, Alt+T / Alt+B and arrows change the active layer", () => {
    const onStatus = vi.fn();
    render(<PcbView snapshot={four()} selected={null} onSelect={() => {}} edit={edit()} onStatus={onStatus} />);
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual(["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]);
    expect(active()).toBe("top");
    fireEvent.keyDown(window, { key: "+" });
    expect(active()).toBe("in1");
    fireEvent.keyDown(window, { key: "=" });
    expect(active()).toBe("in2");
    fireEvent.keyDown(window, { key: "-" });
    expect(active()).toBe("in1");
    fireEvent.keyDown(window, { key: "b", code: "KeyB", altKey: true });
    expect(active()).toBe("bottom");
    fireEvent.keyDown(window, { key: "+" }); // wraps to the top
    expect(active()).toBe("top");
    fireEvent.keyDown(window, { key: "b", code: "KeyB", altKey: true });
    fireEvent.keyDown(window, { key: "t", code: "KeyT", altKey: true });
    expect(active()).toBe("top");
    fireEvent.click(screen.getByRole("tab", { name: "In2.Cu" }));
    expect(active()).toBe("in2");
    fireEvent.keyDown(screen.getByRole("tab", { name: "In2.Cu" }), { key: "ArrowLeft" });
    expect(active()).toBe("in1");
    expect(onStatus).toHaveBeenLastCalledWith({ layer: { inner: 1 }, gridMm: expect.any(Number) });
    // no engine command for a layer change
    expect(sent).toEqual([]);
  });
  it("stepLayer wraps both ways; the status bar names inner layers (was Top/Bottom only)", () => {
    const order: CopperLayer[] = ["top", { inner: 1 }, "bottom"];
    expect(stepLayer(order, "bottom", 1)).toBe("top");
    expect(stepLayer(order, "top", -1)).toBe("bottom");
    expect(stepLayer(order, "top", 1)).toEqual({ inner: 1 });
    expect([statusLayerName("top"), statusLayerName({ inner: 3 }), statusLayerName("bottom")]).toEqual(["Top", "Inner 3", "Bottom"]);
  });
});

describe("layer panel groups and colour presets", () => {
  it("groups copper layers top / inner / bottom and lists the board outline as mechanical", () => {
    const s = fixture();
    const l = (layer: CopperLayer) => ({ layer, kind: "signal" as const, copper_oz: 1, dielectric_after_um: 200, er: 4.2 });
    s.project.pcb.board.stackup = { layers: [l("top"), l({ inner: 1 }), l("bottom")], board_thickness: nm(1.6), dielectric: "FR-4" };
    const { container } = render(<Board snap={s} />);
    expect([...container.querySelectorAll(".layer-group")].map((g) => g.textContent)).toEqual(["Top", "Inner", "Bottom"]);
    expect(screen.getByRole("list", { name: "Mechanical layers" }).textContent).toContain("Board outline");
    // The active row carries the marker.
    expect(container.querySelector('.layer-row[aria-current="true"]')!.getAttribute("data-layer")).toBe("top");
    fireEvent.click(screen.getByLabelText("Active layer In1.Cu"));
    expect(container.querySelector('.layer-row[aria-current="true"]')!.getAttribute("data-layer")).toBe("in1");
  });
  it("a preset sets every layer's colour; the default preset clears the overrides", () => {
    render(<Board />);
    const colour = (n: string) => (screen.getByLabelText(`Colour ${n}`) as HTMLInputElement).value;
    const before = colour("F.Cu");
    fireEvent.change(screen.getByLabelText("Colour preset"), { target: { value: "contrast" } });
    expect(colour("F.Cu")).toBe(COLOUR_PRESETS.find((p) => p.id === "contrast")!.colour("top"));
    expect(colour("B.Cu")).toBe("#00b4ff");
    fireEvent.change(screen.getByLabelText("Colour preset"), { target: { value: "default" } });
    expect(colour("F.Cu")).toBe(before);
  });
});

describe("flip-board and outline views", () => {
  it("flip mirrors the canvas and maps the pointer back to design coordinates", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    fireEvent.click(screen.getByRole("button", { name: "Flip board" }));
    const svg = container.querySelector("svg.canvas") as SVGSVGElement;
    expect(svg.style.transform).toBe("scaleX(-1)");
    fireEvent.click(screen.getByRole("button", { name: "Via" }));
    fireEvent.change(screen.getByLabelText("Net"), { target: { value: "GND" } });
    press(svg, client(fit(pcbBounds(fixture())), 25, -5, true));
    expect(sent).toEqual([{ type: "AddVia", position: pt(25, 5), net: "GND", drill: null, diameter: null, from: "top", to: "bottom" }]);
  });
  it("outline view draws tracks as outlines in SVG and in the Canvas2D scene", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Outline view" }));
    expect(container.querySelector("polygon.track-outline")).toBeTruthy();
    expect(container.querySelector("line.track")).toBeNull();
    const scene = buildPcbScene(fixture(), { order: ["top", "bottom"], hidden: new Set(["outline"]), colourOf: () => "#ff0000", activeLayer: "top", airwires: null, outlineView: true });
    const op = scene.objects.find((o) => o.key === "track:trk_1")!.ops[0] as { t: string; stroke?: string; fill?: string; pts: unknown[] };
    expect([op.t, op.stroke, op.fill, op.pts.length]).toEqual(["poly", "#ff0000", undefined, 4]);
    expect(scene.objects.find((o) => o.key === "outline")).toBeUndefined(); // board outline hidden
    expect(scene.objects.some((o) => o.layerKey === "drill")).toBe(false);
  });
});

describe("persistence per project view", () => {
  it("locks, flip and outline view survive a remount and stay per project", () => {
    const a = renderHook(() => useLayerView("prj_a"));
    act(() => {
      a.result.current.toggleLock("top");
      a.result.current.setFlip(true);
      a.result.current.setOutline(true);
      a.result.current.toggle("outline");
    });
    a.unmount();
    const again = renderHook(() => useLayerView("prj_a")).result.current;
    expect([...again.locked]).toEqual(["top"]);
    expect([again.flip, again.outline, again.hidden.has("outline")]).toEqual([true, true, true]);
    const other = renderHook(() => useLayerView("prj_b")).result.current;
    expect([other.locked.size, other.flip, other.outline]).toEqual([0, false, false]);
    expect(sanitizePrefs({ locked: ["top", 2], flip: "yes", outline: true })).toMatchObject({ locked: ["top"], flip: false, outline: true });
  });
});
