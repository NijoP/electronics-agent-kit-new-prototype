import { describe, expect, it } from "vitest";
import {
  deleteTracks,
  feedbackText,
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
  trackEdit,
  widthFromMm,
  type RouteDraft,
} from "./routeTool";
import type { FootprintGeom, Point, RouteStepReport, Track } from "./types";

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
