// PCB editor: board outline, zones, tracks, vias and footprints from the snapshot, with
// tools that turn clicks into typed engine commands (outline, move, route, via, delete).
import { useEffect, useRef, useState } from "react";
import { Canvas } from "./Canvas";
import { isTyping, type EditCtx } from "./EditContext";
import { mmToNm, nextRotation, outlineFrom, segments, snapPoint } from "./editing";
import type { CopperLayer, Id, PadGeom, Point, Snapshot } from "./types";
import { boundsOf, fromView, mm, toView, type Bounds } from "./viewport";

interface Props {
  snapshot: Snapshot;
  selected: Id | null;
  onSelect: (symbolId: Id) => void;
  edit?: EditCtx;
  /** Symbols an AI proposal would touch (their footprints are drawn dashed). */
  aiPreview?: Id[];
  /** Reports the active copper layer and grid for the status bar. */
  onStatus?: (s: { layer: CopperLayer; gridMm: number }) => void;
}

type Tool = "select" | "outline" | "move" | "route" | "via" | "delete";
const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["outline", "Board outline"],
  ["move", "Move"],
  ["route", "Route"],
  ["via", "Via"],
  ["delete", "Delete"],
];
const GRID = mmToNm(0.5);
const CAPTURE = mmToNm(0.8);
const poly = (pts: Point[]) => pts.map((p) => toView(p).join(",")).join(" ");

const hit = (e: { target: EventTarget }, attr: string): string | null =>
  e.target instanceof Element ? (e.target.closest(`[${attr}]`)?.getAttribute(attr) ?? null) : null;

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

export function PcbView({ snapshot, selected, onSelect, edit, aiPreview = [], onStatus }: Props) {
  const { pcb } = snapshot.project;
  const outline = pcb.board.outline;
  const [tool, setTool] = useState<Tool>("select");
  const [layer, setLayer] = useState<CopperLayer>("top");
  const [net, setNet] = useState("");
  const [start, setStart] = useState<Point | null>(null);
  const [corners, setCorners] = useState<Point[]>([]);
  const [picked, setPicked] = useState<Id | null>(null);
  const [hover, setHover] = useState<Point | null>(null);

  const pads = snapshot.geometry.footprints.flatMap((f) => f.pads.map((p) => ({ at: p.position, net: p.net })));
  const trackEnds = Object.values(pcb.tracks).flatMap((t) => [
    { at: t.a, net: t.net },
    { at: t.b, net: t.net },
  ]);
  const targets = [...pads, ...trackEnds];
  const nets = [...new Set([...pads.map((p) => p.net), ...Object.values(pcb.tracks).map((t) => t.net)])]
    .filter((n): n is string => !!n)
    .sort();
  const footprint = snapshot.geometry.footprints.find((f) => f.symbol_id === selected);
  const fpInstance = footprint ? pcb.footprints[footprint.id] : undefined;

  const reset = () => {
    setStart(null);
    setCorners([]);
    setPicked(null);
  };
  const choose = (t: Tool) => {
    reset();
    setTool(t);
  };
  useEffect(() => onStatus?.({ layer, gridMm: GRID / 1_000_000 }), [layer, onStatus]);

  const finishOutline = () => {
    const pts = outlineFrom(corners);
    if (!edit) return;
    if (!pts) return edit.notify("Outline needs two opposite corners or at least three points.");
    setCorners([]);
    void edit.execute({ type: "SetBoardOutline", points: pts });
  };

  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (!edit || isTyping(e.target) || e.ctrlKey || e.metaKey) return;
    const k = e.key.toLowerCase();
    const digit = Number(e.key);
    if (digit >= 1 && digit <= TOOLS.length) choose(TOOLS[digit - 1][0]);
    else if (e.key === "Escape") reset();
    else if (e.key === "Enter" && tool === "outline") finishOutline();
    else if (k === "r" && fpInstance) {
      void edit.execute({
        type: "MoveFootprint",
        footprint_id: fpInstance.id,
        position: fpInstance.position,
        rotation: nextRotation(fpInstance.rotation),
        side: null,
      });
    } else if (k === "f" && fpInstance) {
      void edit.execute({
        type: "MoveFootprint",
        footprint_id: fpInstance.id,
        position: fpInstance.position,
        rotation: null,
        side: fpInstance.side === "top" ? "bottom" : "top",
      });
    } else if (k === "l" && tool === "route") setLayer((l) => (l === "top" ? "bottom" : "top"));
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const onPoint = (x: number, y: number, e: React.PointerEvent) => {
    if (!edit || edit.busy || tool === "select") return;
    const snap = snapPoint(
      fromView(x, y),
      GRID,
      targets.map((t) => t.at),
      CAPTURE,
    );
    const snapped = snap.hit === null ? null : targets[snap.hit];
    const fpId = hit(e, "data-footprint-id");
    switch (tool) {
      case "outline":
        return setCorners((c) => [...c, snap.point]);
      case "move": {
        if (!picked) {
          if (fpId) {
            setPicked(fpId);
            const f = snapshot.geometry.footprints.find((g) => g.id === fpId);
            if (f) onSelect(f.symbol_id);
          }
          return;
        }
        const id = picked;
        setPicked(null);
        void edit.execute({ type: "MoveFootprint", footprint_id: id, position: snap.point, rotation: null, side: null });
        return;
      }
      case "route": {
        const useNet = snapped?.net && !start ? snapped.net : net;
        if (snapped?.net && !start) setNet(snapped.net);
        if (!start) return setStart(snap.point);
        if (!useNet) return edit.notify("Pick the net to route first.");
        const legs = segments(start, snap.point, true);
        setStart(snap.point);
        if (legs.length)
          void edit.execute(
            legs.map(([a, b]) => ({ type: "AddTrack" as const, layer, a, b, width: null, net: useNet })),
          );
        return;
      }
      case "via": {
        const n = snapped?.net ?? net;
        if (!n) return edit.notify("Pick the net for the via first.");
        void edit.execute({ type: "AddVia", position: snap.point, net: n, drill: null, diameter: null });
        return;
      }
      case "delete": {
        const t = hit(e, "data-track-id");
        const v = hit(e, "data-via-id");
        if (v) void edit.execute({ type: "DeleteVia", via_id: v });
        else if (t) void edit.execute({ type: "DeleteTrack", track_id: t });
        return;
      }
    }
  };

  const hoverSnap = hover ? snapPoint(hover, GRID, targets.map((t) => t.at), CAPTURE).point : null;
  const preview = tool === "route" && start && hoverSnap ? segments(start, hoverSnap, true) : [];

  return (
    <section className="view" aria-label="PCB view">
      <header className="view-header">
        <span>PCB</span>
        {edit ? (
          <div className="tools" role="toolbar" aria-label="PCB tools">
            {TOOLS.map(([t, label], i) => (
              <button
                key={t}
                type="button"
                aria-pressed={tool === t}
                className={tool === t ? "on" : ""}
                title={`${label} (${i + 1})`}
                onClick={() => choose(t)}
              >
                {label}
              </button>
            ))}
          </div>
        ) : (
          <span className="muted">
            {outline.length ? "" : "no board outline yet · "}read-only · wheel zoom · drag pan · double-click fit
          </span>
        )}
      </header>
      {edit && (
        <div className="tool-options">
          {(tool === "route" || tool === "via") && (
            <>
              <select aria-label="Layer" value={layer} onChange={(e) => setLayer(e.target.value as CopperLayer)}>
                <option value="top">Top copper</option>
                <option value="bottom">Bottom copper</option>
              </select>
              <select aria-label="Net" value={net} onChange={(e) => setNet(e.target.value)}>
                <option value="">net…</option>
                {nets.map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </>
          )}
          {tool === "outline" && (
            <button type="button" onClick={finishOutline} disabled={corners.length < 2}>
              Finish outline ({corners.length} pts)
            </button>
          )}
          {footprint && (
            <span className="muted">
              {footprint.refdes} · {footprint.side} · R rotates · F flips
            </span>
          )}
          <span className="muted hint">
            {outline.length === 0 && tool !== "outline" && "No board outline yet — use Board outline. "}
            {tool === "outline" && "Click two opposite corners (or 3+ points), then Enter"}
            {tool === "move" && (picked ? "Click the new position" : "Click a footprint, then its destination")}
            {tool === "route" && (start ? "Click to end the track · L switches layer · Esc stops" : "Click a pad or point to start")}
            {tool === "via" && "Click a pad, track end or point"}
            {tool === "delete" && "Click a track or via"}
          </span>
        </div>
      )}
      <Canvas
        label="PCB canvas"
        bounds={pcbBounds(snapshot)}
        tool={!!edit && tool !== "select"}
        onPoint={edit ? onPoint : undefined}
        onHover={edit ? (x, y) => setHover(fromView(x, y)) : undefined}
      >
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
            <g key={t.id} data-track-id={t.id}>
              <line className={`track ${t.layer}`} x1={x1} y1={y1} x2={x2} y2={y2} strokeWidth={mm(t.width)}>
                <title>{`${t.net} (${t.layer})`}</title>
              </line>
              <line className="hit" x1={x1} y1={y1} x2={x2} y2={y2} />
            </g>
          );
        })}
        {preview.map(([a, b], i) => {
          const [x1, y1] = toView(a);
          const [x2, y2] = toView(b);
          return <line key={i} className={`track ${layer} preview`} x1={x1} y1={y1} x2={x2} y2={y2} strokeWidth={0.25} />;
        })}
        {corners.length > 0 && <polyline className="outline-draft" points={poly([...corners, ...(hoverSnap ? [hoverSnap] : [])])} />}
        {snapshot.geometry.footprints.map((f) => {
          const [x, y] = toView(f.position);
          const hw = mm(f.half_w);
          const hh = mm(f.half_h);
          return (
            <g
              key={f.id}
              data-footprint-id={f.id}
              data-symbol-id={f.symbol_id}
              className={`footprint ${f.side}${f.symbol_id === selected ? " selected" : ""}${aiPreview.includes(f.symbol_id) ? " ai-preview" : ""}`}
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
            <g key={v.id} className="via" data-via-id={v.id}>
              <title>{`via ${v.net}`}</title>
              <circle cx={x} cy={y} r={mm(v.diameter) / 2} />
              <circle className="drill" cx={x} cy={y} r={mm(v.drill) / 2} />
            </g>
          );
        })}
        {start && <circle className="anchor" cx={toView(start)[0]} cy={toView(start)[1]} r={0.5} />}
      </Canvas>
    </section>
  );
}
