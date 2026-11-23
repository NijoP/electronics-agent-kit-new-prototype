// Junction dots are derived from connectivity, never stored (EAK-IR has no junction object; the
// engine already joins a node that lies on a wire's interior, see eak-engineering netlist). A dot
// is drawn where three or more connection points meet and at least one of them is a wire: wire
// ends and pins at the same point, or a wire end / pin that lands on the interior of another wire
// of any angle (a T-junction; 45° legs included). Pins that only touch each other get no dot
// (EasyEDA's rule table: with no wire end at a node there is no junction).
import type { Point, Wire } from "../../types";
import { onSegment } from "./placeTools";

const key = (p: Point) => `${p.x},${p.y}`;
const same = (a: Point, b: Point) => a.x === b.x && a.y === b.y;

/** True when `p` lies strictly inside the wire a-b (integer nm, exact). */
export const onInterior = (p: Point, a: Point, b: Point) => !same(p, a) && !same(p, b) && onSegment(p, a, b);

/** Points that need a junction dot, given the wires and pin positions of one sheet. */
export function junctionPoints(wires: Wire[], pins: Point[]): Point[] {
  const nodes = new Map<string, { p: Point; n: number; wireEnds: number }>();
  const add = (p: Point, wire: boolean) => {
    const k = key(p);
    const e = nodes.get(k) ?? { p, n: 0, wireEnds: 0 };
    e.n += 1;
    if (wire) e.wireEnds += 1;
    nodes.set(k, e);
  };
  for (const w of wires) {
    add(w.a, true);
    add(w.b, true);
  }
  for (const p of pins) add(p, false);
  const out: Point[] = [];
  for (const { p, n, wireEnds } of nodes.values()) {
    // A node on another wire's interior joins it: that wire counts as two more wire ends.
    const through = wires.some((w) => onInterior(p, w.a, w.b)) ? 2 : 0;
    if (wireEnds + through > 0 && n + through >= 3) out.push(p);
  }
  return out;
}
