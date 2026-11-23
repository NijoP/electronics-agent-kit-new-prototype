// Phase 1 slice E5 (planes and stack-up completion): the engine zone fill drawn by both PCB
// renderers, stack-up table editing, plane-net assignment and StitchZones, all as engine commands.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { fixture } from "./fixture.test-data";
import { useLayerView } from "./layerView";
import { PcbView } from "./PcbView";
import { buildPcbScene, fillPath } from "./render/pcbScene";
import { LayerManager } from "./shell/LayerManager";
import { assignPlaneNet, buildUpUm, planeNet, stitchBlocked, withThickness } from "./shell/planes";
import type { Command, CopperLayer, FillView, Snapshot } from "./types";

const MM = 1_000_000;
const pt = (x: number, y: number) => ({ x: x * MM, y: y * MM });

/** The fixture as a 4-layer board whose inner layers are planes without zones. */
function fourLayer(): Snapshot {
  const s = fixture();
  s.project.pcb.board.stackup = {
    layers: [
      { layer: "top", kind: "signal", copper_oz: 1, dielectric_after_um: 200, er: 4.2 },
      { layer: { inner: 1 }, kind: "plane", copper_oz: 0.5, dielectric_after_um: 1000, er: 4.2 },
      { layer: { inner: 2 }, kind: "plane", copper_oz: 0.5, dielectric_after_um: 200, er: 4.2 },
      { layer: "bottom", kind: "signal", copper_oz: 1, dielectric_after_um: 0, er: 4.2 },
    ],
    board_thickness: 1_504_400,
    dielectric: "FR-4",
  };
  return s;
}

/** An engine-shaped fill of the fixture's bottom GND zone: two rectangles around a gap. */
const FILL: FillView = {
  zone_id: "zon_1",
  layer: "bottom",
  net: "GND",
  islands: 1,
  removed_islands: 0,
  filled_area_mm2: 480,
  pitch: 100_000,
  rects: [
    [1 * MM, 1 * MM, 29 * MM, 9 * MM],
    [1 * MM, 11 * MM, 29 * MM, 19 * MM],
  ],
};

afterEach(cleanup);

describe("stack-up and plane helpers", () => {
  it("a new thickness scales the dielectric gaps and keeps the copper", () => {
    const st = fourLayer().project.pcb.board.stackup!;
    const next = withThickness(st, 1.6 * MM);
    expect(next.board_thickness).toBe(1.6 * MM);
    expect(next.layers.map((l) => l.copper_oz)).toEqual([1, 0.5, 0.5, 1]);
    expect(next.layers[3].dielectric_after_um).toBe(0);
    expect(Math.abs(buildUpUm(next) - 1600)).toBeLessThan(2);
    // proportions kept: the 1000 µm core stays five times the 200 µm prepregs
    expect(next.layers[1].dielectric_after_um / next.layers[0].dielectric_after_um).toBeCloseTo(5, 1);
  });
  it("a plane net goes to the layer's zones, or a board-outline zone is added", () => {
    const s = fourLayer();
    const outline = s.project.pcb.board.outline;
    expect(planeNet(s, { inner: 1 })).toBe("");
    expect(assignPlaneNet(s, { inner: 1 }, "GND", outline)).toEqual({ type: "AddZone", layer: { inner: 1 }, net: "GND", outline, clearance: null });
    expect(assignPlaneNet(s, { inner: 1 }, "GND", [])).toBeNull();
    expect(assignPlaneNet(s, "bottom", "N1", outline)).toEqual({ type: "EditZone", zone_id: "zon_1", net: "N1" });
    expect(assignPlaneNet(s, "bottom", "GND", outline)).toBeNull();
    s.project.pcb.zones.zon_2 = { ...s.project.pcb.zones.zon_1, id: "zon_2", net: "N1" };
    expect(planeNet(s, "bottom")).toBeNull();
    const batch = assignPlaneNet(s, "bottom", "VCC", outline) as Extract<Command, { type: "Batch" }>;
    expect(batch.type).toBe("Batch");
    expect(batch.commands).toHaveLength(2);
  });
  it("stitching needs a net poured on two or more layers", () => {
    const s = fixture();
    expect(stitchBlocked(s, "GND")).toMatch(/two or more copper layers/);
    expect(stitchBlocked(s, "VCC")).toBe("No VCC zone");
    s.project.pcb.zones.zon_t = { ...s.project.pcb.zones.zon_1, id: "zon_t", layer: "top" };
    expect(stitchBlocked(s, "GND")).toBeNull();
    expect(stitchBlocked(s, null)).toBeNull();
    s.project.pcb.zones = {};
    expect(stitchBlocked(s, null)).toBe("No copper zones to stitch");
  });
});

describe("engine fill drawn by both renderers", () => {
  const order: CopperLayer[] = ["top", "bottom"];
  const zoneOps = (fills?: FillView[] | null) =>
    buildPcbScene(fixture(), { order, hidden: new Set<string>(), colourOf: () => "#c83", activeLayer: "top", airwires: null, fills }).objects.find((o) => o.key === "zone:zon_1")!.ops;
  it("canvas: the outline plus one filled polygon per engine rectangle", () => {
    const ops = zoneOps([FILL]);
    expect(ops).toHaveLength(3);
    expect(ops[0]).toMatchObject({ t: "poly", stroke: "#c83" });
    expect(ops[0]).not.toHaveProperty("fill");
    expect(ops[1]).toMatchObject({ t: "poly", fill: "#c83", pts: [[1, -1], [29, -1], [29, -9], [1, -9]] });
  });
  it("canvas: an empty fill draws the outline alone; no fill data keeps the translucent zone area", () => {
    expect(zoneOps([])).toHaveLength(1);
    expect(zoneOps(null)).toEqual([expect.objectContaining({ t: "poly", fill: "#c83", alpha: 0.25 })]);
  });
  it("SVG: the fill is a path from the engine rectangles; the outline stays the hit target", () => {
    const { container } = render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} zoneFills={[FILL]} />);
    expect(fillPath(FILL)).toBe("M1 -1H29V-9H1ZM1 -11H29V-19H1Z");
    expect(container.querySelector('path[data-zone-fill="zon_1"]')!.getAttribute("d")).toBe(fillPath(FILL));
    expect(container.querySelector('polygon[data-zone-id="zon_1"]')!.getAttribute("class")).toContain("has-fill");
    expect(container.querySelector('section[aria-label="PCB view"]')!.getAttribute("data-zone-fills")).toBe("1");
  });
});

function Panel({ snap, sent, fills = null }: { snap: Snapshot; sent: Command[]; fills?: FillView[] | null }) {
  const view = useLayerView(null);
  return <LayerManager snapshot={snap} view={view} unrouted={0} edit={async (c) => void sent.push(...(Array.isArray(c) ? c : [c]))} busy={false} fills={fills} />;
}

describe("Layers panel: stack-up table editing and planes", () => {
  it("copper weight, gap and εr cells send SetStackup with that one field changed", () => {
    const sent: Command[] = [];
    render(<Panel snap={fourLayer()} sent={sent} />);
    const oz = screen.getByLabelText("Copper oz F.Cu");
    fireEvent.change(oz, { target: { value: "2" } });
    fireEvent.keyDown(oz, { key: "Enter" });
    fireEvent.change(screen.getByLabelText("Gap µm In1.Cu"), { target: { value: "900" } });
    fireEvent.blur(screen.getByLabelText("Gap µm In1.Cu"));
    fireEvent.change(screen.getByLabelText("εr In2.Cu"), { target: { value: "3.7" } });
    fireEvent.blur(screen.getByLabelText("εr In2.Cu"));
    expect(sent).toHaveLength(3);
    const [a, b, c] = sent as Extract<Command, { type: "SetStackup" }>[];
    expect(a.type).toBe("SetStackup");
    expect(a.stackup.layers[0].copper_oz).toBe(2);
    expect(b.stackup.layers[1].dielectric_after_um).toBe(900);
    expect(c.stackup.layers[2].er).toBe(3.7);
    // the bottom layer has no dielectric below it: no gap or εr cell
    expect(screen.queryByLabelText("Gap µm B.Cu")).toBeNull();
  });
  it("an unchanged or empty value sends nothing", () => {
    const sent: Command[] = [];
    render(<Panel snap={fourLayer()} sent={sent} />);
    const oz = screen.getByLabelText("Copper oz F.Cu");
    fireEvent.blur(oz);
    fireEvent.change(oz, { target: { value: "" } });
    fireEvent.blur(oz);
    expect(sent).toEqual([]);
  });
  it("board thickness and dielectric material go through SetStackup", () => {
    const sent: Command[] = [];
    render(<Panel snap={fourLayer()} sent={sent} />);
    fireEvent.change(screen.getByLabelText("Board thickness mm"), { target: { value: "1.6" } });
    fireEvent.keyDown(screen.getByLabelText("Board thickness mm"), { key: "Enter" });
    fireEvent.change(screen.getByLabelText("Dielectric material"), { target: { value: "FR-4 TG170" } });
    fireEvent.keyDown(screen.getByLabelText("Dielectric material"), { key: "Enter" });
    const [t, d] = sent as Extract<Command, { type: "SetStackup" }>[];
    expect(t.stackup.board_thickness).toBe(1.6 * MM);
    expect(Math.abs(buildUpUm(t.stackup) - 1600)).toBeLessThan(2);
    expect(d.stackup.dielectric).toBe("FR-4 TG170");
  });
  it("a plane layer's net select adds or moves its zone; signal layers have none", () => {
    const sent: Command[] = [];
    render(<Panel snap={fourLayer()} sent={sent} />);
    expect(screen.queryByLabelText("Plane net F.Cu")).toBeNull();
    fireEvent.change(screen.getByLabelText("Plane net In1.Cu"), { target: { value: "GND" } });
    expect(sent[0]).toMatchObject({ type: "AddZone", layer: { inner: 1 }, net: "GND", outline: [pt(0, 0), pt(30, 0), pt(30, 20), pt(0, 20)] });
  });
  it("lists each zone with the engine's fill and stitches a net poured on two layers", () => {
    const s = fixture();
    s.project.pcb.zones.zon_t = { ...s.project.pcb.zones.zon_1, id: "zon_t", layer: "top" };
    const sent: Command[] = [];
    const { container } = render(<Panel snap={s} sent={sent} fills={[FILL]} />);
    const list = screen.getByRole("list", { name: "Zones" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(container.querySelector('[data-zone-row="zon_1"]')!.getAttribute("data-fill-area")).toBe("480.0");
    expect(within(list).getByText(/480\.0 mm² · 1 island/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Stitch GND" }));
    expect(sent).toEqual([{ type: "StitchZones", net: "GND" }]);
  });
  it("without engine fills it says the fill is not computed, and one-layer pours cannot be stitched", () => {
    render(<Panel snap={fixture()} sent={[]} />);
    expect(screen.getByText("fill not computed yet")).toBeTruthy();
    const b = screen.getByRole("button", { name: "Stitch GND" }) as HTMLButtonElement;
    expect(b.disabled).toBe(true);
    expect(b.title).toMatch(/two or more copper layers/);
  });
});
