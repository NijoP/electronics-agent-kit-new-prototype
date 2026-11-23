import { describe, expect, it } from "vitest";
import {
  COMMON_VIAS,
  deleteTracks,
  draftLength,
  draftShoved,
  feedbackText,
  loopTracks,
  MIL,
  mmText,
  netLengthText,
  netTrackLength,
  parseVia,
  viaPresets,
  viaPresetValue,
  widthPresets,
  widthPresetValue,
  finishOn,
  netPriority,
  nextConnections,
  onBoard,
  padAt,
  ripUpAndReroute,
  routeBatch,
  routeReducer,
  startProblem,
  stepRequest,
  layerChangeVias,
  trackEdit,
  viaSpan,
  widthFromMm,
  type BoardCopper,
  type RouteDraft,
} from "./routeTool";
import type { Constraints, FootprintGeom, Point, RouteStepReport, Track, Via } from "./types";

const P = (x: number, y: number): Point => ({ x, y });

function report(over: Partial<RouteStepReport>): RouteStepReport {
  return {
    revision: 3,
    net: "SIG",
    layer: "top",
    width: 250_000,
    clearance: 200_000,
    mode: "diagonal45",
    segments: [],
    via: null,
    end: P(0, 0),
    reached: true,
    walkaround: false,
    blocking: [],
    min_gap: null,
    message: null,
    ...over,
  };
}

const started = (): RouteDraft =>
  routeReducer(null, { type: "start", net: "SIG", layer: "top", at: P(0, 0), mode: "diagonal45" })!;

describe("route tool reducer", () => {
  it("starts on a net and asks the engine for a step from the anchor", () => {
    const d = started();
    expect(d.steps).toEqual([]);
    expect(stepRequest(d, P(5, 5))).toEqual({
      net: "SIG",
      layer: "top",
      from: P(0, 0),
      to: P(5, 5),
      mode: "diagonal45",
      width: null,
      via_at_start: false,
      conflict: "walkaround",
      via: null,
      pending: { tracks: [], vias: [], shoved: [] },
    });
  });

  it("fixes the engine proposal and moves the anchor to its end", () => {
    const d = routeReducer(started(), {
      type: "fix",
      step: report({ segments: [{ a: P(0, 0), b: P(5, 5) }, { a: P(5, 5), b: P(9, 5) }], end: P(9, 5) }),
    })!;
    expect(d.steps).toHaveLength(1);
    expect(d.anchor).toEqual(P(9, 5));
  });

  it("ignores stale proposals (wrong anchor, net or layer) and empty ones", () => {
    const d = started();
    expect(routeReducer(d, { type: "fix", step: report({ segments: [{ a: P(1, 1), b: P(2, 2) }], end: P(2, 2) }) })).toBe(d);
    expect(routeReducer(d, { type: "fix", step: report({ net: "GND", segments: [{ a: P(0, 0), b: P(2, 2) }] }) })).toBe(d);
    expect(routeReducer(d, { type: "fix", step: report({ layer: "bottom", segments: [{ a: P(0, 0), b: P(2, 2) }] }) })).toBe(d);
    expect(routeReducer(d, { type: "fix", step: report({}) })).toBe(d);
  });

  it("a layer switch requests a via at the anchor; switching back cancels it", () => {
    let d = routeReducer(started(), { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(4, 0) }], end: P(4, 0) }) })!;
    d = routeReducer(d, { type: "switchLayer", layer: "bottom" })!;
    expect(d.viaPending).toBe(true);
    expect(stepRequest(d, P(9, 0)).via_at_start).toBe(true);
    expect(stepRequest(d, P(9, 0)).layer).toBe("bottom");
    const back = routeReducer(d, { type: "switchLayer", layer: "top" })!;
    expect(back.viaPending).toBe(false);
    // A proposal without the requested via is stale.
    expect(routeReducer(d, { type: "fix", step: report({ layer: "bottom", segments: [{ a: P(4, 0), b: P(9, 0) }], end: P(9, 0) }) })).toBe(d);
    const via = { position: P(4, 0), drill: 300_000, diameter: 600_000 };
    d = routeReducer(d, { type: "fix", step: report({ layer: "bottom", via, segments: [{ a: P(4, 0), b: P(9, 0) }], end: P(9, 0) }) })!;
    expect(d.viaPending).toBe(false);
    expect(d.steps[1].via).toEqual(via);
  });

  it("toggles 45/90 mode, steps back and cancels", () => {
    let d = started();
    d = routeReducer(d, { type: "toggleMode" })!;
    expect(d.mode).toBe("orthogonal90");
    d = routeReducer(d, { type: "fix", step: report({ mode: "orthogonal90", segments: [{ a: P(0, 0), b: P(4, 0) }], end: P(4, 0) }) })!;
    d = routeReducer(d, { type: "back" })!;
    expect(d.steps).toEqual([]);
    expect(d.anchor).toEqual(P(0, 0));
    expect(routeReducer(d, { type: "back" })).toBeNull();
    expect(routeReducer(d, { type: "cancel" })).toBeNull();
  });

  it("commits the whole route as ONE Batch of AddVia/AddTrack commands", () => {
    let d = routeReducer(started(), { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(4, 0) }], end: P(4, 0) }) })!;
    d = routeReducer(d, { type: "switchLayer", layer: "bottom" })!;
    const via = { position: P(4, 0), drill: 300_000, diameter: 600_000 };
    d = routeReducer(d, { type: "fix", step: report({ layer: "bottom", via, segments: [{ a: P(4, 0), b: P(9, 0) }], end: P(9, 0) }) })!;
    expect(routeBatch(d)).toEqual({
      type: "Batch",
      label: "Route SIG",
      commands: [
        { type: "AddTrack", layer: "top", a: P(0, 0), b: P(4, 0), width: 250_000, net: "SIG" },
        { type: "AddVia", position: P(4, 0), net: "SIG", drill: 300_000, diameter: 600_000, from: null, to: null },
        { type: "AddTrack", layer: "bottom", a: P(4, 0), b: P(9, 0), width: 250_000, net: "SIG" },
      ],
    });
    expect(routeBatch(started())).toBeNull();
  });

  it("rip-up and re-route is one Batch over the engine's ClearRouting and AutoRoute", () => {
    expect(ripUpAndReroute("SIG")).toEqual({
      type: "Batch",
      label: "Rip up and re-route SIG",
      commands: [
        { type: "ClearRouting", net: "SIG" },
        { type: "AutoRoute", nets: ["SIG"] },
      ],
    });
  });

  it("shows live clearance feedback from the engine result", () => {
    const t = feedbackText(
      report({
        walkaround: true,
        min_gap: { object: "pad U1.3", measured: 210_000, required: 200_000 },
        blocking: [{ object: "track trk_1", measured: 50_000, required: 200_000 }],
      }),
    );
    expect(t).toContain("width 0.250 mm");
    expect(t).toContain("walkaround");
    expect(t).toContain("min gap 0.210 mm to pad U1.3");
    expect(t).toContain("blocked by track trk_1");
    expect(feedbackText(null)).toBe("");
  });
});

// Written without running (no local builds/tests in this phase): unrun.
const fp = (id: string, refdes: string, pads: [string, number, number, string | null][]): FootprintGeom => ({
  id,
  symbol_id: `s_${id}`,
  refdes,
  side: "top",
  position: P(0, 0),
  half_w: 2_000_000,
  half_h: 2_000_000,
  pads: pads.map(([number, x, y, net]) => ({ number, kind: "smd", shape: "rect", position: P(x, y), size_x: 1_000_000, size_y: 1_000_000, drill: null, net })),
});
const board = [P(0, 0), P(50_000_000, 0), P(50_000_000, 40_000_000), P(0, 40_000_000)];
const fps = [fp("fp1", "R1", [["1", 10_000_000, 10_000_000, "SIG"], ["2", 12_000_000, 10_000_000, "GND"]]), fp("fp2", "R2", [["1", 30_000_000, 10_000_000, "SIG"], ["2", 32_000_000, 10_000_000, null]])];

describe("manual routing helpers", () => {
  it("hits a pad anywhere on its copper and resolves its centre", () => {
    const h = padAt(fps, P(10_400_000, 9_700_000), 0);
    expect(h?.refdes).toBe("R1");
    expect(h?.at).toEqual(P(10_000_000, 10_000_000));
    expect(padAt(fps, P(20_000_000, 20_000_000), 0)).toBeNull();
  });

  it("refuses to start off the board or on a pad without a net", () => {
    expect(startProblem(padAt(fps, P(10_000_000, 10_000_000), 0)!, board)).toBeNull();
    expect(startProblem(padAt(fps, P(32_000_000, 10_000_000), 0)!, board)).toMatch(/no net/);
    expect(startProblem({ footprint: "x", refdes: "U9", number: "1", net: "SIG", at: P(60_000_000, 1) }, board)).toMatch(/outside the board/);
    expect(onBoard([], P(-1, -1))).toBe(true);
  });

  it("finishes on a same-net pad and refuses a pad of another net", () => {
    const d = started();
    expect(finishOn(d, padAt(fps, P(30_000_000, 10_000_000), 0)).kind).toBe("finish");
    expect(finishOn(d, padAt(fps, P(12_000_000, 10_000_000), 0)).kind).toBe("refuse");
    expect(finishOn(d, null).kind).toBe("point");
  });

  it("carries the chosen track width into route_step", () => {
    const d = routeReducer(started(), { type: "setWidth", width: widthFromMm("0.5") })!;
    expect(stepRequest(d, P(5, 5)).width).toBe(500_000);
    expect(widthFromMm("")).toBeNull();
    expect(widthFromMm("-1")).toBeNull();
  });

  it("edits a track as one Batch and deletes tracks", () => {
    const t: Track = { id: "trk1", layer: "top", width: 250_000, a: P(0, 0), b: P(5, 0), net: "SIG" };
    expect(trackEdit(t, { width: 250_000 })).toBeNull();
    expect(trackEdit(t, { width: 400_000 })).toEqual({
      type: "Batch",
      commands: [
        { type: "DeleteTrack", track_id: "trk1" },
        { type: "AddTrack", layer: "top", a: P(0, 0), b: P(5, 0), width: 400_000, net: "SIG" },
      ],
      label: "Edit track SIG",
    });
    expect(deleteTracks(["trk1"])).toEqual({ type: "DeleteTrack", track_id: "trk1" });
  });

  it("orders unrouted connections most constrained first (DESK priority model)", () => {
    expect(netPriority("VCC", 2, undefined)).toBeGreaterThan(netPriority("SIG", 2, undefined));
    const q = nextConnections(
      [
        ["SIG", P(0, 0), P(1, 0)],
        ["VCC", P(0, 0), P(9, 0)],
      ],
      fps,
      undefined,
    );
    expect(q[0][0]).toBe("VCC");
  });
});

// Slice E2 regressions (inventory F6 and PCB defect 7).
describe("route via span and track layer changes (E2)", () => {
  const via = (x: number) => ({ position: P(x, 0), drill: 300_000, diameter: 600_000 });
  it("a via onto or off an inner layer carries its span (blind/buried), outer-to-outer stays through", () => {
    let d = routeReducer(started(), { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(4, 0) }], end: P(4, 0) }) })!;
    d = routeReducer(d, { type: "switchLayer", layer: { inner: 1 } })!;
    d = routeReducer(d, { type: "fix", step: report({ layer: { inner: 1 }, via: via(4), segments: [{ a: P(4, 0), b: P(9, 0) }], end: P(9, 0) }) })!;
    d = routeReducer(d, { type: "switchLayer", layer: { inner: 2 } })!;
    d = routeReducer(d, { type: "fix", step: report({ layer: { inner: 2 }, via: via(9), segments: [{ a: P(9, 0), b: P(12, 0) }], end: P(12, 0) }) })!;
    const b = routeBatch(d);
    const vias = b && b.type === "Batch" ? b.commands.filter((c) => c.type === "AddVia") : [];
    expect(vias).toEqual([
      { type: "AddVia", position: P(4, 0), net: "SIG", drill: 300_000, diameter: 600_000, from: "top", to: { inner: 1 } },
      { type: "AddVia", position: P(9, 0), net: "SIG", drill: 300_000, diameter: 600_000, from: { inner: 1 }, to: { inner: 2 } },
    ]);
    // Stepping back over a via restores the layer it started on, so the redo keeps the span.
    const back = routeReducer(d, { type: "back" })!;
    expect(back.previousLayer).toEqual({ inner: 1 });
    expect(viaSpan("top", "bottom")).toEqual({ from: null, to: null });
    expect(viaSpan(null, { inner: 1 })).toEqual({ from: null, to: null });
  });

  it("a via switched before the first segment starts on the route's start layer", () => {
    let d = routeReducer(started(), { type: "switchLayer", layer: { inner: 1 } })!;
    d = routeReducer(d, { type: "fix", step: report({ layer: { inner: 1 }, via: via(0), segments: [{ a: P(0, 0), b: P(5, 0) }], end: P(5, 0) }) })!;
    const b = routeBatch(d)!;
    expect(b.type === "Batch" && b.commands[0]).toMatchObject({ type: "AddVia", from: "top", to: { inner: 1 } });
  });

  const t: Track = { id: "trk1", layer: "top", width: 250_000, a: P(0, 0), b: P(5, 0), net: "SIG" };
  const pad = (kind: "smd" | "tht", at: Point) => ({ number: "1", kind, shape: "rect" as const, position: at, size_x: 2, size_y: 2, drill: null, net: "SIG" }); // P() is raw nm: a 2 nm pad covers only its own point
  const fp = (kind: "smd" | "tht", at: Point): FootprintGeom => ({ id: "fp", symbol_id: "s", refdes: "U1", side: "top", position: at, half_w: 1, half_h: 1, pads: [pad(kind, at)] });
  const board = (over: Partial<BoardCopper>): BoardCopper => ({ order: ["top", { inner: 1 }, "bottom"], tracks: [t], vias: [], footprints: [], ...over });

  it("moving a track to another layer adds a via where it joins old-layer copper", () => {
    const joined: Track = { id: "trk2", layer: "top", width: 250_000, a: P(5, 0), b: P(9, 0), net: "SIG" };
    const b = board({ tracks: [t, joined], footprints: [fp("tht", P(0, 0))] });
    expect(trackEdit(t, { layer: "bottom" }, b)).toEqual({
      type: "Batch",
      label: "Edit track SIG",
      commands: [
        { type: "DeleteTrack", track_id: "trk1" },
        { type: "AddTrack", layer: "bottom", a: P(0, 0), b: P(5, 0), width: 250_000, net: "SIG" },
        { type: "AddVia", position: P(5, 0), net: "SIG", drill: null, diameter: null, from: null, to: null },
      ],
    });
    expect(layerChangeVias(t, { inner: 1 }, b)).toEqual({
      vias: [{ type: "AddVia", position: P(5, 0), net: "SIG", drill: null, diameter: null, from: "top", to: { inner: 1 } }],
    });
  });

  it("refuses a layer change that would leave an end on an SMD pad or a via that does not reach", () => {
    expect(trackEdit(t, { layer: "bottom" }, board({ footprints: [fp("smd", P(5, 0))] }))).toHaveProperty("refuse");
    const blind: Via = { id: "v", position: P(5, 0), drill: 300_000, diameter: 600_000, net: "SIG", from: "top", to: { inner: 1 } };
    expect(trackEdit(t, { layer: "bottom" }, board({ vias: [blind] }))).toHaveProperty("refuse");
    // The same via reaches In1, so no extra via is needed there.
    expect(layerChangeVias(t, { inner: 1 }, board({ vias: [blind] }))).toEqual({ vias: [] });
    // A width-only edit never needs vias.
    expect(trackEdit(t, { width: 400_000 }, board({ footprints: [fp("smd", P(5, 0))] }))).toMatchObject({ type: "Batch" });
  });
});

// Slice C3: routing options. Written without running (no local builds/tests): unrun.
describe("routing options (C3)", () => {
  const constraints = (): Constraints => ({
    min_track_width: 150_000,
    min_clearance: 150_000,
    min_drill: 300_000,
    min_annular_ring: 130_000,
    board_edge_clearance: 300_000,
    default_class: { name: "Default", track_width: 250_000, clearance: 200_000, via_drill: 300_000, via_diameter: 600_000, nets: [] },
    net_classes: [{ name: "Power", track_width: 500_000, clearance: 200_000, via_drill: 400_000, via_diameter: 800_000, nets: ["VCC"] }],
    temp_rise_c: 10,
  });

  it("width presets: the net's class first, the other classes, then the common widths", () => {
    const w = widthPresets(constraints(), "VCC", null);
    expect(w[0]).toEqual({ value: "", label: "Net class Power (0.500 mm)" });
    expect(w.map((p) => p.value)).toContain("250000"); // the default class
    expect(w.map((p) => p.value)).toContain(String(10 * MIL));
    expect(widthPresets(constraints(), "SIG", null)[0].label).toBe("Net class Default (0.250 mm)");
    expect(widthPresetValue(w, null)).toBe("");
    expect(widthPresetValue(w, 500_000 + 1)).toBe("custom");
    expect(widthPresets(constraints(), "VCC", 123_000).at(-1)).toEqual({ value: "custom", label: "Custom (0.123 mm)" });
    expect(widthPresetValue(w, 8 * MIL)).toBe(String(8 * MIL));
    // The text field round-trips a preset exactly (8 mil = 0.2032 mm).
    expect(widthFromMm(mmText(8 * MIL))).toBe(8 * MIL);
  });

  it("via presets choose the via the engine places on a layer switch", () => {
    const v = viaPresets(constraints(), "VCC");
    expect(v[0].label).toBe("Net class Power (0.400 / 0.800 mm)");
    expect(v.map((p) => p.value)).toContain(`${COMMON_VIAS[0].drill}/${COMMON_VIAS[0].diameter}`);
    expect(parseVia("")).toBeNull();
    expect(parseVia("400000/300000")).toBeNull(); // diameter must exceed the drill
    const size = parseVia(v[2].value)!;
    expect(viaPresetValue(size)).toBe(v[2].value);
    let d = routeReducer(null, { type: "start", net: "VCC", layer: "top", at: P(0, 0), mode: "diagonal45", via: size, conflict: "block" })!;
    expect(stepRequest(d, P(1, 1))).toMatchObject({ via: size, conflict: "block" });
    d = routeReducer(d, { type: "setVia", via: null })!;
    d = routeReducer(d, { type: "setConflict", conflict: "walkaround" })!;
    expect(stepRequest(d, P(1, 1))).toMatchObject({ via: null, conflict: "walkaround" });
    expect(routeReducer(d, { type: "setVia", via: { drill: 5, diameter: 5 } })!.via).toBeNull();
  });

  it("live net length: committed tracks of the net plus the fixed steps and the live proposal", () => {
    const tracks: Track[] = [
      { id: "a", layer: "top", width: 1, a: P(0, 0), b: P(3_000_000, 0), net: "SIG" },
      { id: "b", layer: "top", width: 1, a: P(0, 0), b: P(0, 9_000_000), net: "GND" },
    ];
    expect(netTrackLength(tracks, "SIG")).toBe(3_000_000);
    let d = started();
    d = routeReducer(d, { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(0, 4_000_000) }], end: P(0, 4_000_000) }) })!;
    const live = report({ segments: [{ a: P(0, 4_000_000), b: P(3_000_000, 8_000_000) }] });
    expect(draftLength(d, null)).toBe(4_000_000);
    expect(draftLength(d, live)).toBe(9_000_000);
    expect(draftLength(null, live)).toBe(0);
    expect(netLengthText(3_000_000, 9_000_000)).toBe("12.000 mm (3.000 committed + 9.000 this route)");
  });

  describe("loop removal", () => {
    const tr = (id: string, a: Point, b: Point, net = "SIG", layer: Track["layer"] = "top"): Track => ({ id, layer, width: 1, a, b, net });
    // A route from (0,0) to (10,0) on top, drawn below an old chain (0,0)-(0,5)-(10,5)-(10,0).
    const routed = (): RouteDraft => {
      let d = started();
      d = routeReducer(d, { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(10, 0) }], end: P(10, 0) }) })!;
      return d;
    };
    const chain = [tr("c1", P(0, 0), P(0, 5)), tr("c2", P(0, 5), P(10, 5)), tr("c3", P(10, 5), P(10, 0))];

    it("deletes the unbranched old chain between the route's ends, in the same Batch", () => {
      const remove = loopTracks(routed(), chain, [], []);
      expect(remove.sort()).toEqual(["c1", "c2", "c3"]);
      const b = routeBatch(routed(), remove)!;
      expect(b.type === "Batch" && b.commands.map((c) => c.type)).toEqual(["DeleteTrack", "DeleteTrack", "DeleteTrack", "AddTrack"]);
    });

    it("keeps the chain when it branches, passes a pad or via, is on another layer or another net", () => {
      expect(loopTracks(routed(), [...chain, tr("br", P(0, 5), P(-4, 5))], [], [])).toEqual([]);
      expect(loopTracks(routed(), [...chain, tr("tee", P(5, 9), P(5, 5))], [], [])).toEqual([]); // ends on c2's interior
      const via: Via = { id: "v", position: P(10, 5), drill: 1, diameter: 2, net: "SIG", from: "top", to: "bottom" };
      expect(loopTracks(routed(), chain, [via], [])).toEqual([]);
      const padAt5: FootprintGeom = { id: "f", symbol_id: "s", refdes: "U1", side: "top", position: P(0, 5), half_w: 1, half_h: 1, pads: [{ number: "1", kind: "smd", shape: "rect", position: P(0, 5), size_x: 2, size_y: 2, drill: null, net: "SIG" }] };
      expect(loopTracks(routed(), chain, [], [padAt5])).toEqual([]);
      expect(loopTracks(routed(), chain.map((t) => ({ ...t, layer: "bottom" as const })), [], [])).toEqual([]);
      expect(loopTracks(routed(), chain.map((t) => ({ ...t, net: "GND" })), [], [])).toEqual([]);
    });

    it("removes nothing when the route does not end on the old chain's far end", () => {
      let d = started();
      d = routeReducer(d, { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(7, 0) }], end: P(7, 0) }) })!;
      expect(loopTracks(d, chain, [], [])).toEqual([]);
      expect(loopTracks(started(), chain, [], [])).toEqual([]);
      expect(routeBatch(started(), ["c1"])).toBeNull();
    });
  });
});

// Slice C3 push-and-shove in the route tool. Written without running (no local builds): unrun.
describe("push mode (C3)", () => {
  const bump = (id: string, y: number): ShovedTrack => ({ id, layer: "top", width: 250_000, net: "GND", points: [P(0, 300), P(1, 300), P(2, y), P(8, y), P(9, 300), P(10, 300)] });
  const pushed = (): RouteDraft => {
    let d = routeReducer(started(), { type: "setConflict", conflict: "push" })!;
    d = routeReducer(d, { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(4, 0) }], end: P(4, 0), shoved: [bump("gnd", 600)] }) })!;
    return routeReducer(d, { type: "fix", step: report({ segments: [{ a: P(4, 0), b: P(9, 0) }], end: P(9, 0), shoved: [bump("gnd", 700)] }) })!;
  };

  it("sends push mode and the route so far (segments, vias, latest moves) as pending", () => {
    const d = pushed();
    const r = stepRequest(d, P(12, 0));
    expect(r.conflict).toBe("push");
    expect(r.pending.tracks).toEqual([
      { layer: "top", width: 250_000, a: P(0, 0), b: P(4, 0) },
      { layer: "top", width: 250_000, a: P(4, 0), b: P(9, 0) },
    ]);
    expect(r.pending.vias).toEqual([]);
    // The later step's move of the same track replaces the earlier one.
    expect(r.pending.shoved).toEqual([bump("gnd", 700)]);
    expect(draftShoved(d)).toEqual([bump("gnd", 700)]);
  });

  it("commits the moved track as DeleteTrack + its polyline, in the route's one Batch", () => {
    const b = routeBatch(pushed())!;
    expect(b.type).toBe("Batch");
    if (b.type !== "Batch") return;
    expect(b.commands.map((c) => c.type)).toEqual(["DeleteTrack", ...Array(5).fill("AddTrack"), "AddTrack", "AddTrack"]);
    expect(b.commands[0]).toEqual({ type: "DeleteTrack", track_id: "gnd" });
    expect(b.commands[3]).toEqual({ type: "AddTrack", layer: "top", a: P(2, 700), b: P(8, 700), width: 250_000, net: "GND" });
    // A track both pushed and on a removed loop is deleted once.
    const twice = routeBatch(pushed(), ["gnd"])!;
    expect(twice.type === "Batch" && twice.commands.filter((c) => c.type === "DeleteTrack").length).toBe(1);
  });

  it("stepping back drops that step's move; walkaround steps move nothing", () => {
    const d = routeReducer(pushed(), { type: "back" })!;
    expect(draftShoved(d)).toEqual([bump("gnd", 600)]);
    const w = routeReducer(started(), { type: "fix", step: report({ segments: [{ a: P(0, 0), b: P(4, 0) }], end: P(4, 0) }) })!;
    expect(draftShoved(w)).toEqual([]);
    const wb = routeBatch(w)!;
    expect(wb.type === "Batch" && wb.commands.map((c) => c.type)).toEqual(["AddTrack"]);
  });

  it("shows which tracks the step pushed", () => {
    expect(feedbackText(report({ shoved: [bump("gnd", 600)] }))).toContain("pushed gnd");
    expect(feedbackText(report({}))).not.toContain("pushed");
  });
});
