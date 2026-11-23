// Sheet drawings of one schematic sheet (slice B4): the free graphics and texts of the Line,
// Rectangle, Circle and Text tools (EAK-IR `schematic.graphics`), and the buses and bus entries
// (`schematic.buses`, `schematic.bus_entries`). Each carries its id as a data attribute, so the
// Delete tool can remove it with the matching engine command. Graphics never connect anything.
import type { CSSProperties } from "react";
import { arcPoints } from "../../render/symbolDraw";
import type { Graphic, Id, Snapshot } from "../../types";
import { mm, toView } from "../../viewport";

const pts = (ps: { x: number; y: number }[]) => ps.map((p) => toView(p).join(",")).join(" ");
const DASH: Record<string, string | undefined> = { dashed: "1 0.6", dotted: "0.2 0.5", dash_dot: "1 0.5 0.2 0.5" };

/** SVG of one graphic in sheet coordinates. */
export function GraphicShape({ g }: { g: Graphic }) {
  const stroke: CSSProperties = "stroke" in g && g.stroke ? { ...(g.stroke.width ? { strokeWidth: mm(g.stroke.width) } : {}), strokeDasharray: DASH[g.stroke.style] } : {};
  const fill = "fill" in g && g.fill ? " filled" : "";
  switch (g.kind) {
    case "line": {
      const [x1, y1] = toView(g.a);
      const [x2, y2] = toView(g.b);
      return <line className="gfx" x1={x1} y1={y1} x2={x2} y2={y2} style={stroke} />;
    }
    case "polyline":
      return <polyline className="gfx" points={pts(g.points)} style={stroke} />;
    case "polygon":
      return <polygon className={`gfx${fill}`} points={pts(g.points)} style={stroke} />;
    case "arc":
      return <polyline className="gfx" points={pts(arcPoints(g.center, g.radius, g.start_deg, g.end_deg))} style={stroke} />;
    case "circle": {
      const [cx, cy] = toView(g.center);
      return <circle className={`gfx${fill}`} cx={cx} cy={cy} r={mm(g.radius)} style={stroke} />;
    }
    case "rectangle": {
      const [x1, y1] = toView(g.min);
      const [x2, y2] = toView(g.max);
      return <rect className={`gfx${fill}`} x={Math.min(x1, x2)} y={Math.min(y1, y2)} width={Math.abs(x2 - x1)} height={Math.abs(y2 - y1)} style={stroke} />;
    }
    case "text": {
      const [x, y] = toView(g.position);
      const turn = g.rotation === "R90" ? -90 : g.rotation === "R180" ? 180 : g.rotation === "R270" ? 90 : 0;
      return (
        <text className="gfx-text" x={x} y={y} transform={turn ? `rotate(${turn} ${x} ${y})` : undefined} style={{ fontSize: mm(g.size) }}>
          {g.text}
        </text>
      );
    }
  }
}

/** Graphics, buses and bus entries of `sheetId`, drawn under the wires and symbols. */
export function SheetGraphics({ snapshot, sheetId }: { snapshot: Snapshot; sheetId: Id }) {
  const sch = snapshot.project.schematic;
  return (
    <>
      {Object.values(sch.graphics ?? {})
        .filter((g) => g.sheet_id === sheetId)
        .map((g) => (
          <g key={g.id} className="sheet-graphic" data-graphic-id={g.id} data-graphic-kind={g.graphic.kind}>
            <GraphicShape g={g.graphic} />
          </g>
        ))}
      {Object.values(sch.buses ?? {})
        .filter((b) => b.sheet_id === sheetId)
        .map((b) => {
          const [lx, ly] = toView(b.points[0] ?? { x: 0, y: 0 });
          return (
            <g key={b.id} data-bus-id={b.id}>
              <polyline className="bus" points={pts(b.points)} />
              <polyline className="hit" points={pts(b.points)} />
              <text className="bus-name" x={lx + 0.6} y={ly - 1}>
                {b.name}
              </text>
            </g>
          );
        })}
      {Object.values(sch.bus_entries ?? {})
        .filter((e) => e.sheet_id === sheetId)
        .map((e) => {
          const [x1, y1] = toView(e.bus_point);
          const [x2, y2] = toView(e.wire_point);
          return (
            <g key={e.id} data-bus-entry-id={e.id}>
              <line className="bus-entry" x1={x1} y1={y1} x2={x2} y2={y2} />
              <line className="hit" x1={x1} y1={y1} x2={x2} y2={y2} />
            </g>
          );
        })}
    </>
  );
}
