// Schematic symbol drawing (slice B2): turns a library symbol's schema v3 graphics and pin
// geometry (ADR-0040) into view-space primitives for one placed instance. Pure and backend
// neutral: the schematic SVG draws the result, and every `d` string is also a valid Canvas2D
// `Path2D` description, so a canvas backend can draw the same list without new geometry code.
//
// View coordinates are millimetres with Y down (`toView`); library coordinates are nanometres
// with Y up, mirrored about the local Y axis and then rotated exactly as the engine places pins
// (`SymbolInstance::place` in eak-core), so drawn pin ends always meet the engine's pin points.
import type { Graphic, LineStyle, Nm, Point, PowerStyle, Rotation, Stroke } from "../types";
import { mm, toView } from "../viewport";

/** The parts of a symbol definition the renderer reads (fixtures may omit v3 fields). */
export interface DrawableSymbol {
  pins: {
    number: string;
    name: string;
    position?: Point;
    length?: Nm;
    orientation?: "right" | "left" | "up" | "down";
    name_visible?: boolean;
    number_visible?: boolean;
  }[];
  body_half_w?: Nm;
  body_half_h?: Nm;
  graphics?: Graphic[];
}

/** Where and how an instance sits on the sheet. */
export interface Placement {
  position: Point;
  rotation: Rotation;
  mirror?: boolean;
}

type Fill = "body" | "solid";
/** One body primitive. `fill`: "body" = symbol background, "solid" = line colour, "none". */
export type BodyPrim =
  | { kind: "rect"; x: number; y: number; w: number; h: number; fill: Fill; width?: number; dash?: string }
  | { kind: "circle"; cx: number; cy: number; r: number; fill: Fill; width?: number; dash?: string }
  | { kind: "path"; d: string; fill: Fill | "none"; width?: number; dash?: string };

export interface TextPrim {
  x: number;
  y: number;
  text: string;
  /** Character height, mm. */
  size: number;
  anchor: "start" | "middle" | "end";
  /** Degrees, SVG sense (clockwise on screen); 0 or -90 so text always reads upright. */
  rotate: number;
  role: "pin-name" | "pin-number" | "text";
}

export interface PinPrim {
  number: string;
  /** Connection point (view mm): where wires attach. */
  at: [number, number];
  /** Body end of the pin line; equals `at` for a zero-length pin. */
  inner: [number, number];
}

export interface SymbolDrawing {
  body: BodyPrim[];
  pins: PinPrim[];
  texts: TextPrim[];
}

const PIN_TEXT = 1.0; // mm, pin name / number height
const NAME_GAP = 0.5; // mm, pin name distance from the pin's body end
const NUMBER_LIFT = 0.35; // mm, pin number distance from the pin line

const rotate = (r: Rotation, p: Point): Point => {
  switch (r) {
    case "R90":
      return { x: -p.y, y: p.x };
    case "R180":
      return { x: -p.x, y: -p.y };
    case "R270":
      return { x: p.y, y: -p.x };
    default:
      return p;
  }
};

/** Sheet point of a library point (mirror about local Y, then rotate, then move). */
export function placeLocal(at: Placement, local: Point): Point {
  const m = at.mirror ? { x: -local.x, y: local.y } : local;
  const r = rotate(at.rotation, m);
  return { x: at.position.x + r.x, y: at.position.y + r.y };
}

const view = (at: Placement, local: Point): [number, number] => toView(placeLocal(at, local));
// Rounded to 0.1 µm so drawings are stable strings (and -0 prints as 0).
const f = (n: number) => Number(n.toFixed(4)) + 0;
const pathOf = (pts: [number, number][], closed: boolean) =>
  pts.map(([x, y], i) => `${i === 0 ? "M" : "L"}${f(x)} ${f(y)}`).join("") + (closed ? "Z" : "");

function strokeOf(s: Stroke | undefined): { width?: number; dash?: string } {
  const width = s && s.width > 0 ? mm(s.width) : undefined;
  const w = width ?? 0.25;
  const dashes: Record<LineStyle, string | undefined> = {
    solid: undefined,
    dashed: `${f(w * 4)} ${f(w * 2)}`,
    dotted: `${f(w)} ${f(w * 2)}`,
    dash_dot: `${f(w * 4)} ${f(w * 2)} ${f(w)} ${f(w * 2)}`,
  };
  const dash = s ? dashes[s.style] : undefined;
  return { ...(width !== undefined ? { width } : {}), ...(dash ? { dash } : {}) };
}

/** Points along a counter-clockwise arc (library coordinates), at most 5 degrees apart. */
export function arcPoints(center: Point, radius: Nm, startDeg: number, endDeg: number): Point[] {
  let sweep = endDeg - startDeg;
  while (sweep <= 0) sweep += 360;
  while (sweep > 360) sweep -= 360;
  const n = Math.max(2, Math.ceil(sweep / 5));
  const out: Point[] = [];
  for (let i = 0; i <= n; i++) {
    const a = ((startDeg + (sweep * i) / n) * Math.PI) / 180;
    out.push({ x: center.x + radius * Math.cos(a), y: center.y + radius * Math.sin(a) });
  }
  return out;
}

const ROT_DEG: Record<Rotation, number> = { R0: 0, R90: 90, R180: 180, R270: 270 };

function graphicPrims(g: Graphic, at: Placement): { body?: BodyPrim; text?: TextPrim } {
  const v = (p: Point) => view(at, p);
  switch (g.kind) {
    case "line":
      return { body: { kind: "path", d: pathOf([v(g.a), v(g.b)], false), fill: "none", ...strokeOf(g.stroke) } };
    case "polyline":
      return { body: { kind: "path", d: pathOf(g.points.map(v), false), fill: "none", ...strokeOf(g.stroke) } };
    case "polygon":
      return { body: { kind: "path", d: pathOf(g.points.map(v), true), fill: g.fill ? "solid" : "body", ...strokeOf(g.stroke) } };
    case "arc":
      return {
        body: { kind: "path", d: pathOf(arcPoints(g.center, g.radius, g.start_deg, g.end_deg).map(v), false), fill: "none", ...strokeOf(g.stroke) },
      };
    case "circle": {
      const [cx, cy] = v(g.center);
      return { body: { kind: "circle", cx: f(cx), cy: f(cy), r: f(mm(g.radius)), fill: g.fill ? "solid" : "body", ...strokeOf(g.stroke) } };
    }
    case "rectangle": {
      const [x1, y1] = v(g.min);
      const [x2, y2] = v(g.max);
      return {
        body: {
          kind: "rect",
          x: f(Math.min(x1, x2)),
          y: f(Math.min(y1, y2)),
          w: f(Math.abs(x2 - x1)),
          h: f(Math.abs(y2 - y1)),
          fill: g.fill ? "solid" : "body",
          ...strokeOf(g.stroke),
        },
      };
    }
    case "text": {
      const [x, y] = v(g.position);
      // Text turns with the symbol but is never mirrored or drawn upside down.
      const deg = (ROT_DEG[g.rotation ?? "R0"] + ROT_DEG[at.rotation]) % 180;
      return { text: { x: f(x), y: f(y), text: g.text, size: f(mm(g.size)), anchor: "middle", rotate: deg === 90 ? -90 : 0, role: "text" } };
    }
  }
}

type Orientation = NonNullable<DrawableSymbol["pins"][number]["orientation"]>;
const DIR: Record<Orientation, Point> = {
  right: { x: 1, y: 0 },
  left: { x: -1, y: 0 },
  up: { x: 0, y: 1 },
  down: { x: 0, y: -1 },
};

/** Whether a pin name carries information worth drawing ("~" and empty mean unnamed). */
const named = (name: string) => name.trim() !== "" && name !== "~";

function pinTexts(pin: PinPrim, name: string, showName: boolean, showNumber: boolean): TextPrim[] {
  const { at, inner, number } = pin;
  const dx = inner[0] - at[0];
  const dy = inner[1] - at[1];
  const len = Math.hypot(dx, dy);
  if (len === 0) return [];
  const [ux, uy] = [dx / len, dy / len];
  const horizontal = Math.abs(ux) >= Math.abs(uy);
  const out: TextPrim[] = [];
  if (showName && named(name)) {
    // Inside the body, reading away from the pin.
    const x = inner[0] + ux * NAME_GAP;
    const y = inner[1] + uy * NAME_GAP;
    const anchor = horizontal ? (ux > 0 ? "start" : "end") : uy < 0 ? "start" : "end";
    out.push({ x: f(x), y: f(horizontal ? y + PIN_TEXT * 0.35 : y), text: name, size: PIN_TEXT, anchor, rotate: horizontal ? 0 : -90, role: "pin-name" });
  }
  if (showNumber) {
    // Above (or left of) the middle of the pin line.
    const mx = (at[0] + inner[0]) / 2;
    const my = (at[1] + inner[1]) / 2;
    out.push(
      horizontal
        ? { x: f(mx), y: f(my - NUMBER_LIFT), text: number, size: PIN_TEXT, anchor: "middle", rotate: 0, role: "pin-number" }
        : { x: f(mx - NUMBER_LIFT), y: f(my), text: number, size: PIN_TEXT, anchor: "middle", rotate: -90, role: "pin-number" },
    );
  }
  return out;
}

/** The engine's placed geometry of an instance (rotated body half-extents, absolute pins). */
export interface PlacedGeometry {
  half_w: Nm;
  half_h: Nm;
  pins: { number: string; position: Point }[];
}

/**
 * The drawing of one placed symbol. `placed` (the engine's geometry) is used when the
 * definition has no v3 graphics or pin geometry, so a v2 fixture still draws as a box with
 * pin stubs, exactly as before slice B2.
 */
export function symbolDrawing(def: DrawableSymbol | undefined, at: Placement, placed?: PlacedGeometry): SymbolDrawing {
  const body: BodyPrim[] = [];
  const texts: TextPrim[] = [];
  const graphics = def?.graphics ?? [];
  for (const g of graphics) {
    const p = graphicPrims(g, at);
    if (p.body) body.push(p.body);
    if (p.text) texts.push(p.text);
  }
  const [x, y] = toView(at.position);
  const hw = mm(placed?.half_w ?? def?.body_half_w ?? 0);
  const hh = mm(placed?.half_h ?? def?.body_half_h ?? 0);
  if (graphics.length === 0 && hw > 0 && hh > 0) {
    body.push({ kind: "rect", x: f(x - hw), y: f(y - hh), w: f(2 * hw), h: f(2 * hh), fill: "body" });
  }
  const pins: PinPrim[] = [];
  const enginePins = new Map((placed?.pins ?? []).map((p) => [p.number, p.position]));
  const defPins: DrawableSymbol["pins"] = def?.pins ?? placed?.pins.map((p) => ({ number: p.number, name: "" })) ?? [];
  for (const p of defPins) {
    const local = p.position;
    const engine = enginePins.get(p.number);
    const end = local ? view(at, local) : engine ? toView(engine) : null;
    if (!end) continue;
    let inner: [number, number];
    if (local && p.length && p.length > 0) {
      const d = DIR[p.orientation ?? "right"];
      inner = view(at, { x: local.x + d.x * p.length, y: local.y + d.y * p.length });
    } else if (graphics.length === 0) {
      // v2 box: stub from the body edge to the connection point.
      inner = [Math.min(Math.max(end[0], x - hw), x + hw), Math.min(Math.max(end[1], y - hh), y + hh)];
    } else {
      inner = end;
    }
    const pin: PinPrim = { number: p.number, at: [f(end[0]), f(end[1])], inner: [f(inner[0]), f(inner[1])] };
    pins.push(pin);
    // Without v3 pin geometry (a v2 fixture) numbers stay hidden, as they were before B2.
    const v3 = p.length !== undefined;
    texts.push(...pinTexts(pin, p.name, v3 && (p.name_visible ?? true), v3 && (p.number_visible ?? true)));
  }
  return { body, pins, texts };
}

/** Which power-port shape a label draws: its own style, else ground for ground-like names. */
export function powerStyleOf(name: string, style?: PowerStyle | null): PowerStyle {
  if (style) return style;
  return /^([ADPS]GND|GND|VSS|0V$|EARTH)/i.test(name.trim()) ? "ground" : "rail";
}

/** A power port's shape at view point (x, y): one stroked path, and where its name goes. */
export function powerPortShape(style: PowerStyle, x: number, y: number): { d: string; text: { x: number; y: number } } {
  const p = (dx: number, dy: number) => `${f(x + dx)} ${f(y + dy)}`;
  switch (style) {
    case "ground":
      return {
        d: `M${p(0, 0)}L${p(0, 1.27)}M${p(-1.27, 1.27)}L${p(1.27, 1.27)}M${p(-0.8, 1.75)}L${p(0.8, 1.75)}M${p(-0.3, 2.23)}L${p(0.3, 2.23)}`,
        text: { x: f(x), y: f(y + 3.8) },
      };
    case "chassis":
      return {
        d: `M${p(0, 0)}L${p(0, 1.27)}M${p(-1.27, 1.27)}L${p(1.27, 1.27)}M${p(-1.27, 1.27)}L${p(-1.9, 2.2)}M${p(0, 1.27)}L${p(-0.63, 2.2)}M${p(1.27, 1.27)}L${p(0.63, 2.2)}`,
        text: { x: f(x), y: f(y + 3.8) },
      };
    case "rail":
      return {
        d: `M${p(0, 0)}L${p(0, -1.27)}M${p(-1.27, -1.27)}L${p(1.27, -1.27)}`,
        text: { x: f(x), y: f(y - 1.9) },
      };
  }
}
