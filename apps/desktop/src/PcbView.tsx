// Read-only PCB view: board outline, zones, tracks, vias and footprints from the snapshot.
import { Canvas } from "./Canvas";
import type { Id, PadGeom, Point, Snapshot } from "./types";
import { boundsOf, mm, toView, type Bounds } from "./viewport";

interface Props {
  snapshot: Snapshot;
  selected: Id | null;
  onSelect: (symbolId: Id) => void;
}

const poly = (pts: Point[]) => pts.map((p) => toView(p).join(",")).join(" ");

function Pad({ pad }: { pad: PadGeom }) {
  const [x, y] = toView(pad.position);
  const w = mm(pad.size_x);
  const h = mm(pad.size_y);
  const r = pad.shape === "circle" ? Math.min(w, h) / 2 : pad.shape === "oval" ? Math.min(w, h) / 2 : 0;
  return (
    <g className={`pad ${pad.kind}`}>
      <title>{`pad ${pad.number}${pad.net ? ` · ${pad.net}` : ""}`}</title>
      <rect x={x - w / 2} y={y - h / 2} width={w} height={h} rx={r} ry={r} />
      {pad.drill !== null && <circle className="drill" cx={x} cy={y} r={mm(pad.drill) / 2} />}
    </g>
  );
}

export function pcbBounds(s: Snapshot): Bounds {
  const outline = s.project.pcb.board.outline.map(toView);
  const parts = s.geometry.footprints.flatMap((f) => {
    const [x, y] = toView(f.position);
    return [
      [x - mm(f.half_w), y - mm(f.half_h)],
      [x + mm(f.half_w), y + mm(f.half_h)],
    ] as [number, number][];
  });
  return boundsOf(outline.length ? outline : parts) ?? { minX: 0, minY: -80, maxX: 100, maxY: 0 };
}

export function PcbView({ snapshot, selected, onSelect }: Props) {
  const { pcb } = snapshot.project;
  const outline = pcb.board.outline;
  return (
    <section className="view" aria-label="PCB view">
      <header className="view-header">
        <span>PCB</span>
        <span className="muted">
          {outline.length ? "" : "no board outline yet · "}read-only · wheel zoom · drag pan · double-click fit
        </span>
      </header>
      <Canvas label="PCB canvas" bounds={pcbBounds(snapshot)}>
        {outline.length > 0 && <polygon className="board" points={poly(outline)} />}
        {Object.values(pcb.zones).map((z) => (
          <polygon key={z.id} className={`zone ${z.layer}`} points={poly(z.outline)}>
            <title>{`zone ${z.net} (${z.layer})`}</title>
          </polygon>
        ))}
        {Object.values(pcb.tracks).map((t) => {
          const [x1, y1] = toView(t.a);
          const [x2, y2] = toView(t.b);
          return (
            <line key={t.id} className={`track ${t.layer}`} x1={x1} y1={y1} x2={x2} y2={y2} strokeWidth={mm(t.width)}>
              <title>{`${t.net} (${t.layer})`}</title>
            </line>
          );
        })}
        {snapshot.geometry.footprints.map((f) => {
          const [x, y] = toView(f.position);
          const hw = mm(f.half_w);
          const hh = mm(f.half_h);
          return (
            <g
              key={f.id}
              data-footprint-id={f.id}
              data-symbol-id={f.symbol_id}
              className={`footprint ${f.side}${f.symbol_id === selected ? " selected" : ""}`}
              onClick={() => onSelect(f.symbol_id)}
            >
              <rect className="courtyard" x={x - hw} y={y - hh} width={2 * hw} height={2 * hh} />
              {f.pads.map((p) => (
                <Pad key={p.number} pad={p} />
              ))}
              <text className="refdes" x={x} y={y - hh - 0.5}>
                {f.refdes}
              </text>
            </g>
          );
        })}
        {Object.values(pcb.vias).map((v) => {
          const [x, y] = toView(v.position);
          return (
            <g key={v.id} className="via">
              <title>{`via ${v.net}`}</title>
              <circle cx={x} cy={y} r={mm(v.diameter) / 2} />
              <circle className="drill" cx={x} cy={y} r={mm(v.drill) / 2} />
            </g>
          );
        })}
      </Canvas>
    </section>
  );
}
