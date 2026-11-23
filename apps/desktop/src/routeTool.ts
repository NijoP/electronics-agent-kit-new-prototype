// Interactive PCB route tool state (pure). The engine computes every legal path
// (`route_step` IPC -> eak_engineering::interactive); this reducer only records the steps the
// user fixed with a click and turns the finished route into ONE `Batch` command, so a whole
// route is one revision and one undo step. Nothing here writes the project.
import type { BendMode, Command, ConflictMode, Constraints, CopperLayer, FootprintGeom, Nm, PadGeom, Point, RouteSegment, RouteStepReport, RouteStepRequest, ShovedTrack, Track, Via, ViaSize } from "./types";
import { layerKey, layerName, sameLayer, viaLayers } from "./layers";

/** One fixed piece of the route: segments on one layer, optionally starting with a via. */
export interface RouteStep {
  layer: CopperLayer;
  width: Nm;
  segments: RouteSegment[];
  via: { position: Point; drill: Nm; diameter: Nm } | null;
  /** Layer the via starts on (the layer before the switch); null when there is no via. */
  viaFrom: CopperLayer | null;
  /** Committed tracks this step pushed out of the way (push mode), as replacement polylines. */
  shoved: ShovedTrack[];
}

export interface RouteDraft {
  net: string;
  layer: CopperLayer;
  mode: BendMode;
  /** Where the route started (its first point, on `startLayer`). */
  origin: Point;
  startLayer: CopperLayer;
  /** Where the next step starts (end of the last fixed step). */
  anchor: Point;
  steps: RouteStep[];
  /** The next step starts with a via at `anchor` (layer switched since the last step). */
  viaPending: boolean;
  /** Layer before the pending switch (switching back cancels the via). */
  previousLayer: CopperLayer | null;
  /** Track width override in nm; null uses the net-class width. */
  width: Nm | null;
  /** What the engine does with other-net copper in the way. */
  conflict: ConflictMode;
  /** Via size for layer switches; null uses the net-class via. */
  via: ViaSize | null;
}

export type RouteAction =
  | { type: "start"; net: string; layer: CopperLayer; at: Point; mode: BendMode; width?: Nm | null; conflict?: ConflictMode; via?: ViaSize | null }
  | { type: "setWidth"; width: Nm | null }
  | { type: "setConflict"; conflict: ConflictMode }
  | { type: "setVia"; via: ViaSize | null }
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
    return {
      net: a.net,
      layer: a.layer,
      mode: a.mode,
      origin: a.at,
      startLayer: a.layer,
      anchor: a.at,
      steps: [],
      viaPending: false,
      previousLayer: null,
      width: a.width ?? null,
      conflict: a.conflict ?? "walkaround",
      via: a.via ?? null,
    };
  if (a.type === "cancel" || !d) return null;
  switch (a.type) {
    case "fix": {
      const s = a.step;
      // Ignore stale proposals (other net or layer, or not starting at the anchor).
      if (s.net !== d.net || !sameLayer(s.layer, d.layer)) return d;
      if (s.segments.length && !samePt(s.segments[0].a, d.anchor)) return d;
      if (d.viaPending && !s.via) return d;
      if (!s.segments.length && !s.via) return d;
      const viaFrom = s.via ? (d.previousLayer ?? null) : null;
      const step: RouteStep = { layer: s.layer, width: s.width, segments: s.segments, via: s.via, viaFrom, shoved: s.shoved ?? [] };
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
    case "setConflict":
      return { ...d, conflict: a.conflict };
    case "setVia":
      return { ...d, via: a.via && a.via.drill > 0 && a.via.diameter > a.via.drill ? a.via : null };
    case "toggleMode":
      return { ...d, mode: d.mode === "diagonal45" ? "orthogonal90" : "diagonal45" };
    case "back": {
      if (!d.steps.length) return null;
      const last = d.steps[d.steps.length - 1];
      const anchor = last.via ? last.via.position : last.segments[0].a;
      return {
        ...d,
        steps: d.steps.slice(0, -1),
        anchor,
        layer: last.layer,
        viaPending: !!last.via,
        previousLayer: last.via ? last.viaFrom : null,
      };
    }
  }
  return d;
}

/**
 * The tracks the route has pushed so far: per track, the latest step's polyline (a later step
 * moves a track again from its committed place, so the latest move replaces the earlier one).
 */
export function draftShoved(d: RouteDraft): ShovedTrack[] {
  const byId = new Map<string, ShovedTrack>();
  for (const s of d.steps) for (const t of s.shoved) byId.set(t.id, t);
  return [...byId.values()];
}

/** The `route_step` request for the cursor position, with the route so far as `pending`. */
export function stepRequest(d: RouteDraft, cursor: Point): RouteStepRequest {
  const pending = {
    tracks: d.steps.flatMap((s) => s.segments.map((g) => ({ layer: s.layer, width: s.width, a: g.a, b: g.b }))),
    vias: d.steps.flatMap((s) => (s.via ? [s.via] : [])),
    shoved: draftShoved(d),
  };
  return { net: d.net, layer: d.layer, from: d.anchor, to: cursor, mode: d.mode, width: d.width, via_at_start: d.viaPending, conflict: d.conflict, via: d.via, pending };
}

/**
 * The finished route as one Batch (null when nothing was fixed). A via between the two outer
 * layers is a through via; a via that starts or ends on an inner layer is sent with its span
 * (`from`/`to`), so the engine makes a blind or buried via instead of a through via.
 * `remove` lists committed tracks the route replaces (loop removal); they are deleted first,
 * in the same Batch, so the new route and the removal are one undo step. Tracks push mode
 * moved are replaced the same way: deleted, then re-added along their new polyline.
 */
export function routeBatch(d: RouteDraft, remove: string[] = []): Command | null {
  if (!d.steps.length) return null;
  const shoved = draftShoved(d);
  // A pushed track is replaced by its polyline: deleted, then re-added segment by segment.
  const ids = [...new Set([...remove, ...shoved.map((s) => s.id)])];
  const commands: Command[] = ids.map((track_id) => ({ type: "DeleteTrack" as const, track_id }));
  for (const s of shoved)
    for (let k = 0; k + 1 < s.points.length; k++)
      commands.push({ type: "AddTrack", layer: s.layer, a: s.points[k], b: s.points[k + 1], width: s.width, net: s.net });
  for (const s of d.steps) {
    if (s.via)
      commands.push({ type: "AddVia", position: s.via.position, net: d.net, drill: s.via.drill, diameter: s.via.diameter, ...viaSpan(s.viaFrom, s.layer) });
    for (const g of s.segments) commands.push({ type: "AddTrack", layer: s.layer, a: g.a, b: g.b, width: s.width, net: d.net });
  }
  return commands.length ? { type: "Batch", commands, label: `Route ${d.net}` } : null;
}

/** The `from`/`to` of a route via: null/null (through) unless an end is an inner layer. */
export function viaSpan(from: CopperLayer | null, to: CopperLayer): { from: CopperLayer | null; to: CopperLayer | null } {
  if (!from || (typeof from === "string" && typeof to === "string")) return { from: null, to: null };
  return { from, to };
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
  if (s.shoved?.length) parts.push(`pushed ${s.shoved.map((t) => t.id).join(", ")}`);
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

/** The copper a track's ends can join: used to keep a track connected when it changes layer. */
export interface BoardCopper {
  /** Copper layers top to bottom. */
  order: CopperLayer[];
  tracks: Track[];
  vias: Via[];
  footprints: FootprintGeom[];
}

const padCovers = (pad: PadGeom, p: Point) =>
  Math.abs(p.x - pad.position.x) <= pad.size_x / 2 && Math.abs(p.y - pad.position.y) <= pad.size_y / 2;

/**
 * What moving track `t` from its layer to `to` needs at each end to stay connected: a via
 * where the end joins another track of the net on the old layer, or nothing where a
 * through-hole pad or a via that already reaches `to` is there. An end on an SMD pad of the
 * old layer, or on a via that does not reach `to`, cannot be kept connected by the edit, so
 * the change is refused with the reason.
 */
export function layerChangeVias(t: Track, to: CopperLayer, board: BoardCopper): { vias: Command[] } | { refuse: string } {
  const from = t.layer;
  const vias: Command[] = [];
  for (const [end, p] of [["start", t.a], ["end", t.b]] as const) {
    const pads = board.footprints.flatMap((f) => f.pads.filter((pad) => padCovers(pad, p)).map((pad) => ({ f, pad })));
    if (pads.some(({ pad }) => pad.kind === "tht")) continue;
    const smd = pads.find(({ f }) => sameLayer(f.side, from));
    if (smd)
      return {
        refuse: `The track's ${end} is on SMD pad ${smd.f.refdes}.${smd.pad.number} on ${layerName(from)}; moving the track to ${layerName(to)} would disconnect it. Re-route it with the Route tool instead.`,
      };
    const via = board.vias.find((v) => samePt(v.position, p));
    if (via) {
      if (viaLayers(board.order, via.from ?? "top", via.to ?? "bottom").some((l) => sameLayer(l, to))) continue;
      return { refuse: `The via at the track's ${end} does not reach ${layerName(to)}; moving the track there would disconnect it.` };
    }
    const joins = board.tracks.some((o) => o.id !== t.id && o.net === t.net && sameLayer(o.layer, from) && (samePt(o.a, p) || samePt(o.b, p)));
    if (joins) vias.push({ type: "AddVia", position: p, net: t.net, drill: null, diameter: null, ...viaSpan(from, to) });
  }
  return { vias };
}

/**
 * Change a committed track's width and/or layer as ONE undoable `Batch` (delete + re-add with
 * the same end points and net). A layer change also adds the vias that keep its ends connected
 * (`layerChangeVias`) when `board` is given, and is refused when it cannot. Null when nothing
 * changes.
 */
export function trackEdit(t: Track, change: { width?: Nm; layer?: CopperLayer }, board?: BoardCopper): Command | { refuse: string } | null {
  const width = change.width ?? t.width;
  const layer = change.layer ?? t.layer;
  if (!(width > 0) || (width === t.width && sameLayer(layer, t.layer))) return null;
  const keep = board && !sameLayer(layer, t.layer) ? layerChangeVias(t, layer, board) : { vias: [] };
  if ("refuse" in keep) return keep;
  return {
    type: "Batch",
    commands: [
      { type: "DeleteTrack", track_id: t.id },
      { type: "AddTrack", layer, a: t.a, b: t.b, width, net: t.net },
      ...keep.vias,
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

// ---- Routing options (slice C3) ------------------------------------------------------------
// Width and via presets, live net length and loop removal. The presets only choose the values
// sent to the engine; `route_step` still checks every segment and via against the rules.

export const MIL = 25_400;
/** Common track widths: 5, 8, 10, 12, 20 and 50 mil. */
export const COMMON_WIDTHS: Nm[] = [5, 8, 10, 12, 20, 50].map((m) => m * MIL);
/** Common via drill/diameter pairs: 8/16, 10/20, 12/24 mil and 0.3/0.6 mm. */
export const COMMON_VIAS: ViaSize[] = [
  { drill: 8 * MIL, diameter: 16 * MIL },
  { drill: 10 * MIL, diameter: 20 * MIL },
  { drill: 12 * MIL, diameter: 24 * MIL },
  { drill: 300_000, diameter: 600_000 },
];

export interface Preset {
  value: string;
  label: string;
}

/** Millimetres for a label (3 decimals). */
export const mmLabel = (nm: Nm) => (nm / 1_000_000).toFixed(3);
/** Millimetres for the width text field: exact, so `widthFromMm` gives the same nm back. */
export const mmText = (nm: Nm) => String(nm / 1_000_000);

/** The net class that applies to `net` (the default class when no class lists it). */
export function classOf(c: Constraints | undefined, net: string) {
  return c?.net_classes?.find((k) => k.nets.includes(net)) ?? c?.default_class;
}

/**
 * Track-width choices for `net`: "" follows the net's class, then the other net classes'
 * widths, then the common widths (value = nm). A typed width that is none of them is shown as
 * "custom".
 */
export function widthPresets(c: Constraints | undefined, net: string, width: Nm | null): Preset[] {
  const own = classOf(c, net);
  const out: Preset[] = [{ value: "", label: own ? `Net class ${own.name} (${mmLabel(own.track_width)} mm)` : "Net class" }];
  const add = (nm: Nm, label: string) => {
    if (!out.some((p) => p.value === String(nm))) out.push({ value: String(nm), label });
  };
  for (const k of [c?.default_class, ...(c?.net_classes ?? [])]) if (k && k !== own) add(k.track_width, `${k.name} class: ${mmLabel(k.track_width)} mm`);
  for (const w of COMMON_WIDTHS) add(w, `${w / MIL} mil (${mmLabel(w)} mm)`);
  if (width !== null && !out.some((p) => p.value === String(width))) out.push({ value: "custom", label: `Custom (${mmLabel(width)} mm)` });
  return out;
}

/** The width select's value for the current width override (one of `presets`). */
export const widthPresetValue = (presets: Preset[], width: Nm | null) =>
  width === null ? "" : presets.some((p) => p.value === String(width)) ? String(width) : "custom";

const viaValue = (v: ViaSize) => `${v.drill}/${v.diameter}`;
const viaText = (v: ViaSize) => `${mmLabel(v.drill)} / ${mmLabel(v.diameter)} mm`;

/** Via choices for `net`: "" follows the net's class, then other classes, then the common pairs. */
export function viaPresets(c: Constraints | undefined, net: string): Preset[] {
  const own = classOf(c, net);
  const out: Preset[] = [{ value: "", label: own ? `Net class ${own.name} (${viaText({ drill: own.via_drill, diameter: own.via_diameter })})` : "Net class" }];
  const add = (v: ViaSize, label: string) => {
    if (!out.some((p) => p.value === viaValue(v))) out.push({ value: viaValue(v), label });
  };
  for (const k of [c?.default_class, ...(c?.net_classes ?? [])])
    if (k && k !== own) add({ drill: k.via_drill, diameter: k.via_diameter }, `${k.name} class: ${viaText({ drill: k.via_drill, diameter: k.via_diameter })}`);
  for (const v of COMMON_VIAS) add(v, viaText(v));
  return out;
}

/** The via select's value -> via size (null = net class, or not a valid drill/diameter pair). */
export function parseVia(value: string): ViaSize | null {
  const m = /^(\d+)\/(\d+)$/.exec(value);
  if (!m) return null;
  const v = { drill: Number(m[1]), diameter: Number(m[2]) };
  return v.drill > 0 && v.diameter > v.drill ? v : null;
}
export const viaPresetValue = (v: ViaSize | null) => (v ? viaValue(v) : "");

const segLen = (a: Point, b: Point) => Math.hypot(b.x - a.x, b.y - a.y);

/** Total length (nm) of the committed tracks of `net`. */
export const netTrackLength = (tracks: Track[], net: string) => tracks.filter((t) => t.net === net).reduce((s, t) => s + segLen(t.a, t.b), 0);

/** Length (nm) of the route being drawn: its fixed steps plus the engine's live proposal. */
export function draftLength(d: RouteDraft | null, live: RouteStepReport | null): number {
  if (!d) return 0;
  const fixed = d.steps.reduce((s, st) => s + st.segments.reduce((t, g) => t + segLen(g.a, g.b), 0), 0);
  return fixed + (live ? live.segments.reduce((t, g) => t + segLen(g.a, g.b), 0) : 0);
}

/** Live net-length readout: committed copper plus the route being drawn. */
export const netLengthText = (committed: number, route: number) =>
  `${mmLabel(committed + route)} mm (${mmLabel(committed)} committed + ${mmLabel(route)} this route)`;

/**
 * Loop removal: the committed tracks of the route's net that the finished route replaces.
 * They are an unbranched chain of tracks on one layer from the route's start to its end, so
 * once the new route joins the same two points the chain is a redundant loop. The chain may
 * not pass a pad or a via, may not have a branch (a third track at a joint, or a track ending
 * on one of its segments), and must be on a layer both ends reach (the route's start and end
 * layers, or any layer at a through-hole pad). Anything else removes nothing: the rule never
 * disconnects copper.
 */
export function loopTracks(d: RouteDraft, tracks: Track[], vias: Via[], footprints: FootprintGeom[]): string[] {
  if (!d.steps.length) return [];
  const own = tracks.filter((t) => t.net === d.net && !samePt(t.a, t.b));
  const endLayer = d.steps[d.steps.length - 1].layer;
  const pads = footprints.flatMap((f) => f.pads);
  const tht = (p: Point) => pads.some((pad) => pad.kind === "tht" && padCovers(pad, p));
  const stop = (p: Point) => vias.some((v) => samePt(v.position, p)) || pads.some((pad) => padCovers(pad, p));
  const key = (l: CopperLayer, p: Point) => `${layerKey(l)}:${p.x},${p.y}`;
  const at = new Map<string, Track[]>();
  for (const t of own) for (const p of [t.a, t.b]) at.set(key(t.layer, p), [...(at.get(key(t.layer, p)) ?? []), t]);
  const onInterior = (p: Point, t: Track) => {
    if (samePt(p, t.a) || samePt(p, t.b)) return false;
    const len = segLen(t.a, t.b);
    return Math.abs(segLen(t.a, p) + segLen(p, t.b) - len) < 1;
  };
  const layers = [...new Map(own.map((t): [string, CopperLayer] => [layerKey(t.layer), t.layer])).values()];
  for (const l of layers) {
    if (!(sameLayer(l, d.startLayer) || tht(d.origin)) || !(sameLayer(l, endLayer) || tht(d.anchor))) continue;
    const s = key(l, d.origin);
    const e = key(l, d.anchor);
    if (s === e) continue;
    for (const first of at.get(s) ?? []) {
      let chain: Track[] | null = [first];
      let prev = first;
      let node = samePt(first.a, d.origin) ? first.b : first.a;
      while (chain && key(l, node) !== e) {
        const inc = at.get(key(l, node)) ?? [];
        if (inc.length !== 2 || stop(node) || chain.length > own.length) chain = null;
        else {
          const next = inc[0] === prev ? inc[1] : inc[0];
          chain.push(next);
          node = samePt(next.a, node) ? next.b : next.a;
          prev = next;
        }
      }
      if (!chain) continue;
      const found = chain;
      const inChain = new Set(found.map((t) => t.id));
      const branched = own.some((o) => !inChain.has(o.id) && sameLayer(o.layer, l) && found.some((t) => onInterior(o.a, t) || onInterior(o.b, t)));
      if (!branched) return found.map((t) => t.id);
    }
  }
  return [];
}
