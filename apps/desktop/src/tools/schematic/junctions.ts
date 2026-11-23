// Junction dots are derived from connectivity, never stored (EAK-IR has no junction object; the
// engine already joins a node that lies on a wire's interior, see eak-engineering netlist). A dot
// is drawn where three or more connection points meet: wire ends and pins at the same point, or a
// wire end / pin that lands on the interior of another (axis-aligned) wire (a T-junction).
import type { Point, Wire } from "../../types";

const key = (p: Point) => `${p.x},${p.y}`;

/** True when `p` lies strictly inside the axis-aligned wire a-b (integer nm, exact). */
export function onInterior(p: Point, a: Point, b: Point): boolean {
  if (a.x === b.x && p.x === a.x) {
    const lo = Math.min(a.y, b.y);
    const hi = Math.max(a.y, b.y);
    return p.y > lo && p.y < hi;
  }
  if (a.y === b.y && p.y === a.y) {
    const lo = Math.min(a.x, b.x);
    const hi = Math.max(a.x, b.x);
    return p.x > lo && p.x < hi;
  }
  return false;
}

/** Points that need a junction dot, given the wires and pin positions of one sheet. */
export function junctionPoints(wires: Wire[], pins: Point[]): Point[] {
  const nodes = new Map<string, { p: Point; n: number }>();
  const add = (p: Point) => {
    const k = key(p);
    const e = nodes.get(k);
    if (e) e.n += 1;
    else nodes.set(k, { p, n: 1 });
  };
  for (const w of wires) {
    add(w.a);
    add(w.b);
  }
  for (const p of pins) add(p);
  const out: Point[] = [];
  for (const { p, n } of nodes.values()) {
    // A node on another wire's interior joins it: that wire counts as two more connections.
    const touches = wires.some((w) => onInterior(p, w.a, w.b));
    if (n + (touches ? 2 : 0) >= 3) out.push(p);
  }
  return out;
}
