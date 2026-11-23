// Board outline tool panel: geometry menu (rectangle, rounded rectangle, circle, polygon),
// exact numeric entry in mm or mil, live closure validation and a vertex table for exact
// vertex editing, and an arc sweep per edge (`SetOutlineArc`). Every change is one typed command
// through the engine (one undo step); a vertex insert or delete keeps the arc edges on their
// edges in the same Batch (`drawTools.vertexEdit`).
import { useState } from "react";
import type { EditCtx } from "../../EditContext";
import type { OutlineArc, Point } from "../../types";
import * as D from "./drawTools";
import * as O from "./outline";

type Shape = "rectangle" | "rounded" | "circle" | "polygon";
const SHAPES: [Shape, string][] = [
  ["rectangle", "Rectangle"],
  ["rounded", "Rounded rectangle"],
  ["circle", "Circle"],
  ["polygon", "Polygon (vertices)"],
];

interface Props {
  edit: EditCtx;
  /** The committed board outline. */
  outline: Point[];
  /** Its arc edges (schema v4). */
  arcs: OutlineArc[];
  /** Points clicked on the canvas (or typed) for the polygon being built. */
  corners: Point[];
  setCorners: (f: (c: Point[]) => Point[]) => void;
}

export function OutlinePanel({ edit, outline, arcs, corners, setCorners }: Props) {
  const [shape, setShape] = useState<Shape>("rectangle");
  const [unit, setUnit] = useState<O.Unit>("mm");
  const [f, setF] = useState({ x: "0", y: "0", w: "50", h: "30", r: "3", size: "40", vx: "", vy: "" });
  const [byDiameter, setByDiameter] = useState(true);
  const set = (k: keyof typeof f) => (e: { target: { value: string } }) => setF((s) => ({ ...s, [k]: e.target.value }));
  const num = (s: string) => {
    const n = O.parseNum(s);
    return n === null ? null : O.unitToNm(n, unit);
  };

  const build = (): Point[] | string => {
    const x = num(f.x);
    const y = num(f.y);
    if (shape === "polygon") return corners.length >= 3 ? corners : "A polygon needs at least 3 vertices.";
    if (x === null || y === null) return "Enter numeric X and Y.";
    if (shape === "circle") {
      const s = num(f.size);
      if (s === null || !(s > 0)) return "Enter a positive diameter/radius.";
      return O.circle({ x, y }, byDiameter ? s / 2 : s) ?? "Invalid circle.";
    }
    const w = num(f.w);
    const h = num(f.h);
    if (w === null || h === null || !(w > 0) || !(h > 0)) return "Width and height must be positive numbers.";
    if (shape === "rectangle") return O.rectangle({ x, y }, w, h) ?? "Invalid rectangle.";
    const r = num(f.r);
    if (r === null || r < 0) return "Corner radius must be a number ≥ 0.";
    return O.roundedRectangle({ x, y }, w, h, r) ?? "Corner radius is more than half of the shorter side.";
  };
  const built = build();
  const issues = typeof built === "string" ? [built] : O.validateOutline(built);

  const apply = () => {
    if (typeof built === "string") return edit.notify(built);
    const bad = O.validateOutline(built);
    if (bad.length) return edit.notify(`Outline not closed/valid: ${bad[0]}.`);
    setCorners(() => []);
    void edit.execute({ type: "SetBoardOutline", points: built });
  };

  const commit = (pts: Point[]) => {
    const bad = O.validateOutline(pts);
    if (bad.length) return edit.notify(`Edit rejected: ${bad[0]}.`);
    void edit.execute({ type: "SetBoardOutline", points: pts });
  };
  const vertex = (op: "insert" | "delete", i: number) => {
    const r = D.vertexEdit(outline, arcs, op, i);
    if (typeof r === "string") return edit.notify(r);
    const n = r.dropped.length;
    if (n) edit.notify(`${n === 1 ? "Edge" : "Edges"} ${r.dropped.map((e) => e + 1).join(" and ")} lost ${n === 1 ? "its arc" : "their arcs"}: the deleted vertex joined the two edges into one straight edge.`);
    void edit.execute(r.command);
  };
  const setArc = (edge: number, text: string) => {
    const c = D.arcCommand(edge, text, arcs.find((a) => a.edge === edge)?.sweep_deg);
    if (typeof c === "string") return edit.notify(c);
    if (c) void edit.execute(c);
  };
  const addVertex = () => {
    const x = num(f.vx);
    const y = num(f.vy);
    if (x === null || y === null) return edit.notify("Enter numeric X and Y for the vertex.");
    setCorners((c) => [...c, { x, y }]);
    setF((s) => ({ ...s, vx: "", vy: "" }));
  };

  const ext = O.extent(outline);
  const current = O.validateOutline(outline);
  const field = (label: string, k: keyof typeof f) => (
    <label>
      {label}
      <input aria-label={label} inputMode="decimal" size={7} value={f[k]} onChange={set(k)} />
    </label>
  );

  return (
    <div className="outline-panel" role="group" aria-label="Board outline tool">
      <select aria-label="Outline geometry" value={shape} onChange={(e) => setShape(e.target.value as Shape)}>
        {SHAPES.map(([s, l]) => (
          <option key={s} value={s}>
            {l}
          </option>
        ))}
      </select>
      <select aria-label="Outline units" value={unit} onChange={(e) => setUnit(e.target.value as O.Unit)}>
        <option value="mm">mm</option>
        <option value="mil">mil</option>
      </select>
      {shape !== "polygon" && (
        <>
          {field(shape === "circle" ? "Center X" : "Origin X", "x")}
          {field(shape === "circle" ? "Center Y" : "Origin Y", "y")}
        </>
      )}
      {(shape === "rectangle" || shape === "rounded") && (
        <>
          {field("Width", "w")}
          {field("Height", "h")}
        </>
      )}
      {shape === "rounded" && field("Corner radius", "r")}
      {shape === "circle" && (
        <>
          <select aria-label="Circle size by" value={byDiameter ? "d" : "r"} onChange={(e) => setByDiameter(e.target.value === "d")}>
            <option value="d">Diameter</option>
            <option value="r">Radius</option>
          </select>
          {field(byDiameter ? "Diameter" : "Radius", "size")}
        </>
      )}
      {shape === "polygon" && (
        <>
          {field("Vertex X", "vx")}
          {field("Vertex Y", "vy")}
          <button type="button" onClick={addVertex}>
            Add vertex
          </button>
          <button type="button" onClick={() => setCorners((c) => c.slice(0, -1))} disabled={!corners.length}>
            Undo vertex
          </button>
          <span className="muted">{corners.length} vertices · click the board too · the last joins the first</span>
        </>
      )}
      <button type="button" onClick={apply} disabled={issues.length > 0}>
        Apply outline
      </button>
      <span className={issues.length ? "warn" : "muted"} role="status" aria-label="Outline closure">
        {issues.length ? `not closed/valid: ${issues[0]}` : "closed polygon · valid"}
      </span>
      {ext && (
        <span className="muted" aria-label="Outline size">
          current {O.fmtLen(ext.width, unit)} × {O.fmtLen(ext.height, unit)} {unit} · {outline.length} vertices
          {current.length ? ` · INVALID: ${current[0]}` : ""}
        </span>
      )}
      {outline.length >= 3 && (
        <details className="vertex-table">
          <summary>Vertices ({outline.length})</summary>
          {outline.map((p, i) => {
            const sweep = arcs.find((a) => a.edge === i)?.sweep_deg;
            return (
              <VertexRow
                key={`${i}:${p.x},${p.y}:${unit}:${sweep ?? 0}`}
                i={i}
                n={outline.length}
                p={p}
                unit={unit}
                sweep={sweep}
                onMove={(q) => commit(O.moveVertex(outline, i, q))}
                onDelete={() => vertex("delete", i)}
                onInsert={() => vertex("insert", i)}
                onArc={(t) => setArc(i, t)}
              />
            );
          })}
        </details>
      )}
    </div>
  );
}

function VertexRow({ i, n, p, unit, sweep, onMove, onDelete, onInsert, onArc }: { i: number; n: number; p: Point; unit: O.Unit; sweep: number | undefined; onMove: (p: Point) => void; onDelete: () => void; onInsert: () => void; onArc: (text: string) => void }) {
  const dp = unit === "mm" ? 3 : 1;
  const [x, setX] = useState(O.nmToUnit(p.x, unit).toFixed(dp));
  const [y, setY] = useState(O.nmToUnit(p.y, unit).toFixed(dp));
  const go = () => {
    const nx = O.parseNum(x);
    const ny = O.parseNum(y);
    if (nx === null || ny === null) return;
    const q = { x: O.unitToNm(nx, unit), y: O.unitToNm(ny, unit) };
    // Only an actual edit sends a command (a rounded display value must not rewrite the vertex).
    if (O.fmtLen(q.x, unit) !== O.fmtLen(p.x, unit) || O.fmtLen(q.y, unit) !== O.fmtLen(p.y, unit)) onMove(q);
  };
  const key = (e: React.KeyboardEvent) => e.key === "Enter" && go();
  const [arc, setArc] = useState(sweep === undefined ? "" : String(sweep));
  const arcGo = () => {
    if (arc.trim() !== (sweep === undefined ? "" : String(sweep))) onArc(arc);
  };
  return (
    <div className="vertex-row">
      <span>{i + 1}</span>
      <input aria-label={`Vertex ${i + 1} X`} inputMode="decimal" size={9} value={x} onChange={(e) => setX(e.target.value)} onBlur={go} onKeyDown={key} />
      <input aria-label={`Vertex ${i + 1} Y`} inputMode="decimal" size={9} value={y} onChange={(e) => setY(e.target.value)} onBlur={go} onKeyDown={key} />
      <input
        aria-label={`Edge ${i + 1} arc`}
        title={`Edge ${i + 1} (vertex ${i + 1} to ${((i + 1) % n) + 1}) as an arc: sweep in degrees, positive counter-clockwise; empty or 0 = straight`}
        inputMode="decimal"
        size={5}
        placeholder="arc°"
        value={arc}
        onChange={(e) => setArc(e.target.value)}
        onBlur={arcGo}
        onKeyDown={(e) => e.key === "Enter" && arcGo()}
      />
      <button type="button" title="Insert a vertex at the midpoint of the next edge (an arc edge is split in two halves)" onClick={onInsert}>
        +
      </button>
      <button type="button" title="Delete vertex" onClick={onDelete}>
        ×
      </button>
    </div>
  );
}
