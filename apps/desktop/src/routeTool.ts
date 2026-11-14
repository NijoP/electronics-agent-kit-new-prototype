// Interactive PCB route tool state (pure). The engine computes every legal path
// (`route_step` IPC -> eak_engineering::interactive); this reducer only records the steps the
// user fixed with a click and turns the finished route into ONE `Batch` command, so a whole
// route is one revision and one undo step. Nothing here writes the project.
import type { BendMode, Command, CopperLayer, Nm, Point, RouteSegment, RouteStepReport, RouteStepRequest } from "./types";
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
}

export type RouteAction =
  | { type: "start"; net: string; layer: CopperLayer; at: Point; mode: BendMode }
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
    return { net: a.net, layer: a.layer, mode: a.mode, anchor: a.at, steps: [], viaPending: false, previousLayer: null };
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
  return { net: d.net, layer: d.layer, from: d.anchor, to: cursor, mode: d.mode, width: null, via_at_start: d.viaPending };
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
