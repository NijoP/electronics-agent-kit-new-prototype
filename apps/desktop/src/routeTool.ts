// Interactive PCB route tool state (pure). The engine computes every legal path
// (`route_step` IPC -> eak_engineering::interactive); this reducer only records the steps the
// user fixed with a click and turns the finished route into ONE `Batch` command, so a whole
// route is one revision and one undo step. Nothing here writes the project.
import type { BendMode, Command, Constraints, CopperLayer, FootprintGeom, Nm, Point, RouteSegment, RouteStepReport, RouteStepRequest, Track } from "./types";
import { sameLayer } from "./layers";

/** One fixed piece of the route: segments on one layer, optionally starting with a via. */
export interface RouteStep {
  layer: CopperLayer;
  width: Nm;
  segments: RouteSegment[];
  via: { position: Point; drill: Nm; diameter: Nm } | null;
}

export interface RouteDraft {
  net: string;
  layer: CopperLayer;
  mode: BendMode;
  /** Where the next step starts (end of the last fixed step). */
  anchor: Point;
  steps: RouteStep[];
  /** The next step starts with a via at `anchor` (layer switched since the last step). */
  viaPending: boolean;
  /** Layer before the pending switch (switching back cancels the via). */
  previousLayer: CopperLayer | null;
  /** Track width override in nm; null uses the net-class width. */
  width: Nm | null;
}

export type RouteAction =
  | { type: "start"; net: string; layer: CopperLayer; at: Point; mode: BendMode; width?: Nm | null }
  | { type: "setWidth"; width: Nm | null }
  /** Fix the engine's proposal for the current cursor (the result of `route_step`). */
  | { type: "fix"; step: RouteStepReport }
  | { type: "switchLayer"; layer: CopperLayer }
  | { type: "toggleMode" }
  /** Remove the last fixed step (Backspace). */
  | { type: "back" }
  | { type: "cancel" };

const samePt = (a: Point, b: Point) => a.x === b.x && a.y === b.y;

export function routeReducer(d: RouteDraft | null, a: RouteAction): RouteDraft | null {
  if (a.type === "start")
    return { net: a.net, layer: a.layer, mode: a.mode, anchor: a.at, steps: [], viaPending: false, previousLayer: null, width: a.width ?? null };
  if (a.type === "cancel" || !d) return null;
  switch (a.type) {
    case "fix": {
      const s = a.step;
      // Ignore stale proposals (other net or layer, or not starting at the anchor).
      if (s.net !== d.net || !sameLayer(s.layer, d.layer)) return d;
      if (s.segments.length && !samePt(s.segments[0].a, d.anchor)) return d;
      if (d.viaPending && !s.via) return d;
      if (!s.segments.length && !s.via) return d;
      const step: RouteStep = { layer: s.layer, width: s.width, segments: s.segments, via: s.via };
      return { ...d, anchor: s.end, steps: [...d.steps, step], viaPending: false, previousLayer: null };
    }
    case "switchLayer": {
      if (sameLayer(a.layer, d.layer)) return d;
      // Switching back before anything was drawn on the new layer cancels the via.
      if (d.viaPending && d.previousLayer && sameLayer(a.layer, d.previousLayer))
        return { ...d, layer: a.layer, viaPending: false, previousLayer: null };
      return { ...d, layer: a.layer, viaPending: true, previousLayer: d.viaPending ? d.previousLayer : d.layer };
    }
    case "setWidth":
      return { ...d, width: a.width !== null && a.width > 0 ? a.width : null };
    case "toggleMode":
      return { ...d, mode: d.mode === "diagonal45" ? "orthogonal90" : "diagonal45" };
    case "back": {
      if (!d.steps.length) return null;
      const last = d.steps[d.steps.length - 1];
      const anchor = last.via ? last.via.position : last.segments[0].a;
      const before = d.steps.length > 1 ? d.steps[d.steps.length - 2].layer : null;
      return {
        ...d,
        steps: d.steps.slice(0, -1),
        anchor,
        layer: last.layer,
        viaPending: !!last.via,
        previousLayer: last.via ? before : null,
      };
    }
  }
  return d;
}

/** The `route_step` request for the cursor position. */
export function stepRequest(d: RouteDraft, cursor: Point): RouteStepRequest {
  return { net: d.net, layer: d.layer, from: d.anchor, to: cursor, mode: d.mode, width: d.width, via_at_start: d.viaPending };
}

/** The finished route as one Batch (null when nothing was fixed). Vias are through vias. */
export function routeBatch(d: RouteDraft): Command | null {
  const commands: Command[] = [];
  for (const s of d.steps) {
    if (s.via)
      commands.push({ type: "AddVia", position: s.via.position, net: d.net, drill: s.via.drill, diameter: s.via.diameter, from: null, to: null });
    for (const g of s.segments) commands.push({ type: "AddTrack", layer: s.layer, a: g.a, b: g.b, width: s.width, net: d.net });
  }
  return commands.length ? { type: "Batch", commands, label: `Route ${d.net}` } : null;
}

/** Rip up a net's tracks and vias and re-route it with the engine's router, as one undo step. */
export const ripUpAndReroute = (net: string): Command => ({
  type: "Batch",
  commands: [
    { type: "ClearRouting", net },
    { type: "AutoRoute", nets: [net] },
  ],
  label: `Rip up and re-route ${net}`,
});

/** One-line live clearance feedback for the tool hint. */
export function feedbackText(s: RouteStepReport | null): string {
  if (!s) return "";
  const mm = (v: number) => (v / 1_000_000).toFixed(3);
  const parts = [`width ${mm(s.width)} mm · clearance ${mm(s.clearance)} mm · ${s.mode === "diagonal45" ? "45°" : "90°"}`];
  if (s.via) parts.push("via");
  if (s.walkaround) parts.push("walkaround");
  if (s.min_gap) parts.push(`min gap ${mm(s.min_gap.measured)} mm to ${s.min_gap.object}`);
  if (s.blocking.length) parts.push(`blocked by ${s.blocking.map((b) => b.object).join(", ")}`);
  if (s.message) parts.push(s.message);
  return parts.join(" · ");
}

// ---- Pad hit-testing and finish validation -------------------------------------------------

/** A pad under the pointer, resolved to its centre (where routes start and end). */
export interface PadHit {
  footprint: string;
  refdes: string;
  number: string;
  net: string | null;
  at: Point;
}

/**
 * The pad whose copper (plus `slop`) contains `p`, nearest centre first. A click anywhere on a
 * pad starts or ends a route there, not only within a small radius of the pad centre.
 */
export function padAt(footprints: FootprintGeom[], p: Point, slop: Nm): PadHit | null {
  let best: PadHit | null = null;
  let bestD = Infinity;
  for (const f of footprints)
    for (const pad of f.pads) {
      const dx = Math.abs(p.x - pad.position.x);
      const dy = Math.abs(p.y - pad.position.y);
      if (dx > pad.size_x / 2 + slop || dy > pad.size_y / 2 + slop) continue;
      const d = dx * dx + dy * dy;
      if (d < bestD) {
        bestD = d;
        best = { footprint: f.id, refdes: f.refdes, number: pad.number, net: pad.net, at: pad.position };
      }
    }
  return best;
}

export const padLabel = (h: PadHit) => `${h.refdes}.${h.number}`;

/** Is `p` inside the board outline (true when there is no closed outline yet)? */
export function onBoard(outline: Point[], p: Point): boolean {
  if (outline.length < 3) return true;
  let inside = false;
  for (let i = 0, j = outline.length - 1; i < outline.length; j = i++) {
    const a = outline[i];
    const b = outline[j];
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

/** Why a route cannot start on this pad, or null when it can. */
export function startProblem(pad: PadHit, outline: Point[]): string | null {
  if (!pad.net) return `Pad ${padLabel(pad)} has no net. Connect it in the schematic and run Update PCB first.`;
  if (!onBoard(outline, pad.at))
    return `${pad.refdes} is outside the board outline. Move the footprints onto the board before routing (tracks must keep the board-edge clearance).`;
  return null;
}

/**
 * What a click on `pad` means while routing `d`: finish the route there (same net), refuse it
 * (another net or no net: that would be a short or a dangling track), or just add a point.
 */
export function finishOn(d: RouteDraft, pad: PadHit | null): { kind: "finish" } | { kind: "point" } | { kind: "refuse"; why: string } {
  if (!pad || samePt(pad.at, d.anchor)) return { kind: "point" };
  if (pad.net === d.net) return { kind: "finish" };
  return {
    kind: "refuse",
    why: pad.net
      ? `Pad ${padLabel(pad)} is on net ${pad.net}, not ${d.net}; ending there would short the nets.`
      : `Pad ${padLabel(pad)} has no net; a route of ${d.net} cannot end there.`,
  };
}

// ---- Track editing -------------------------------------------------------------------------

/**
 * Change a committed track's width and/or layer as ONE undoable `Batch` (delete + re-add with
 * the same end points and net). Null when nothing changes.
 */
export function trackEdit(t: Track, change: { width?: Nm; layer?: CopperLayer }): Command | null {
  const width = change.width ?? t.width;
  const layer = change.layer ?? t.layer;
  if (!(width > 0) || (width === t.width && sameLayer(layer, t.layer))) return null;
  return {
    type: "Batch",
    commands: [
      { type: "DeleteTrack", track_id: t.id },
      { type: "AddTrack", layer, a: t.a, b: t.b, width, net: t.net },
    ],
    label: `Edit track ${t.net}`,
  };
}

/** Delete several tracks as one undo step. */
export function deleteTracks(ids: string[]): Command | null {
  if (!ids.length) return null;
  if (ids.length === 1) return { type: "DeleteTrack", track_id: ids[0] };
  return { type: "Batch", commands: ids.map((track_id) => ({ type: "DeleteTrack" as const, track_id })), label: `Delete ${ids.length} tracks` };
}

/** Millimetre text field -> nm (null when empty or not a positive number). */
export function widthFromMm(text: string): Nm | null {
  const v = Number(text.trim());
  return text.trim() !== "" && Number.isFinite(v) && v > 0 ? Math.round(v * 1_000_000) : null;
}

// ---- Next connection to route --------------------------------------------------------------
// Ported from the "constrainedness" priority mental model of DESK (desk/routing/priority.py,
// MIT, Copyright (c) 2026 NijoP; see third_party/licenses/DESK): route the most constrained
// connections first. Priority = 3*criticality + 1*topology + 2*urgency, using what the EAK-IR
// knows (net names, terminal counts, declared net currents); escape/congestion terms are not
// ported because EAK's engine owns copper geometry. Ties go to the shortest connection.

const GND = /^(GND|VSS|AGND|DGND|PGND)$/i;
const PWR = /^(VCC|VDD|3V3|5V|1V8|VBAT|VSYS|VBUS|VIN|\+)/i;

export function netPriority(net: string, terminals: number, currentA: number | undefined): number {
  const crit = GND.test(net) ? 7 : PWR.test(net) || (currentA ?? 0) > 0.5 ? 8 : 4;
  const topo = terminals > 10 ? 8 : terminals > 4 ? 5 : 2;
  const urgency = (currentA ?? 0) > 1.5 ? 9 : 5;
  return 3 * crit + topo + 2 * urgency;
}

/** Unrouted connections ordered by priority (most constrained first), then by length. */
export function nextConnections(
  airwires: [string, Point, Point][],
  footprints: FootprintGeom[],
  constraints: Constraints | undefined,
): [string, Point, Point][] {
  const terminals = new Map<string, number>();
  for (const f of footprints) for (const p of f.pads) if (p.net) terminals.set(p.net, (terminals.get(p.net) ?? 0) + 1);
  const score = (n: string) => netPriority(n, terminals.get(n) ?? 2, constraints?.net_currents_a?.[n]);
  const len = ([, a, b]: [string, Point, Point]) => Math.hypot(b.x - a.x, b.y - a.y);
  return [...airwires].sort((x, y) => score(y[0]) - score(x[0]) || len(x) - len(y));
}
