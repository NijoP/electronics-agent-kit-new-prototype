// Read-only schematic view: draws the active sheet from the engine snapshot.
import { useState } from "react";
import { Canvas } from "./Canvas";
import type { Id, Snapshot } from "./types";
import { boundsOf, mm, toView } from "./viewport";

interface Props {
  snapshot: Snapshot;
  selected: Id | null;
  onSelect: (symbolId: Id) => void;
}

export function SchematicView({ snapshot, selected, onSelect }: Props) {
  const { schematic } = snapshot.project;
  const sheets = Object.values(schematic.sheets);
  const [sheetId, setSheetId] = useState<Id | undefined>(sheets[0]?.id);
  const sheet = sheets.find((s) => s.id === sheetId) ?? sheets[0];
  if (!sheet) return <p className="empty">This project has no schematic sheet.</p>;

  const symbols = snapshot.geometry.symbols.filter((s) => s.sheet_id === sheet.id);
  const pinAt = new Map(
    snapshot.geometry.symbols.flatMap((s) => s.pins.map((p) => [`${s.id}/${p.number}`, p.position] as const)),
  );
  const bounds = boundsOf([
    [0, 0],
    [mm(sheet.width), -mm(sheet.height)],
  ])!;

  return (
    <section className="view" aria-label="Schematic view">
      <header className="view-header">
        <span>Schematic</span>
        {sheets.length > 1 && (
          <select aria-label="Sheet" value={sheet.id} onChange={(e) => setSheetId(e.target.value)}>
            {sheets.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
        <span className="muted">read-only · wheel zoom · drag pan · double-click fit</span>
      </header>
      <Canvas label="Schematic canvas" bounds={bounds}>
        <rect className="sheet" x={0} y={-mm(sheet.height)} width={mm(sheet.width)} height={mm(sheet.height)} />
        {Object.values(schematic.wires)
          .filter((w) => w.sheet_id === sheet.id)
          .map((w) => {
            const [x1, y1] = toView(w.a);
            const [x2, y2] = toView(w.b);
            return <line key={w.id} className="wire" x1={x1} y1={y1} x2={x2} y2={y2} />;
          })}
        {symbols.map((s) => {
          const [x, y] = toView(s.position);
          const hw = mm(s.half_w);
          const hh = mm(s.half_h);
          return (
            <g
              key={s.id}
              data-symbol-id={s.id}
              className={s.id === selected ? "symbol selected" : "symbol"}
              onClick={() => onSelect(s.id)}
            >
              <title>{`${s.refdes} ${s.value}`}</title>
              {s.pins.map((p) => {
                const [px, py] = toView(p.position);
                // Pin stub from the body edge to the connection point.
                const ex = Math.min(Math.max(px, x - hw), x + hw);
                const ey = Math.min(Math.max(py, y - hh), y + hh);
                return (
                  <g key={p.number}>
                    <line className="pin" x1={ex} y1={ey} x2={px} y2={py} />
                    <circle className="pin-end" cx={px} cy={py} r={0.4} />
                  </g>
                );
              })}
              <rect className="body" x={x - hw} y={y - hh} width={2 * hw} height={2 * hh} />
              <text className="refdes" x={x} y={y - hh - 1}>
                {s.refdes}
              </text>
              <text className="value" x={x} y={y + hh + 2.5}>
                {s.value}
              </text>
            </g>
          );
        })}
        {Object.values(schematic.labels)
          .filter((l) => l.sheet_id === sheet.id)
          .map((l) => {
            const [x, y] = toView(l.position);
            return (
              <g key={l.id} className={`label ${l.kind}`}>
                <circle cx={x} cy={y} r={0.5} />
                <text x={x + 1} y={y - 0.8}>
                  {l.name}
                </text>
              </g>
            );
          })}
        {Object.values(schematic.no_connects).map((n) => {
          const at = pinAt.get(`${n.symbol_id}/${n.pin}`);
          if (!at) return null;
          const [x, y] = toView(at);
          return (
            <path key={n.id} className="no-connect" d={`M${x - 1} ${y - 1}L${x + 1} ${y + 1}M${x - 1} ${y + 1}L${x + 1} ${y - 1}`} />
          );
        })}
      </Canvas>
    </section>
  );
}
