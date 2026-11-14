// PCB editor: board outline, zones, tracks, vias and footprints from the snapshot, with
// tools that turn clicks into typed engine commands (outline, move, route, via, delete).
import { useEffect, useRef, useState } from "react";
import { Canvas } from "./Canvas";
import { isTyping, type EditCtx } from "./EditContext";
import { mmToNm, nextRotation, outlineFrom, segments, snapPoint } from "./editing";
import { layerKey, layerName, sameLayer, stackLayers, viaKind, viaLayers } from "./layers";
import { useLayerView, type LayerView } from "./layerView";
import { focusBounds, type Focus } from "./pcbFocus";
import { useDocId, useRegisterTools } from "./shell/viewState";
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
  /** Layer manager state (active layer, visibility, colours); the view keeps its own if absent. */
  layerView?: LayerView;
  /** Finding navigation: zoom to and highlight these objects / this location. */
  focus?: Focus | null;
  /** Unrouted connections from the engine's connectivity analysis: [net, from, to]. */
  airwires?: [string, Point, Point][] | null;
}

type Tool = "select" | "outline" | "move" | "route" | "via" | "delete" | "zone";
const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["outline", "Board outline"],
  ["move", "Move"],
  ["route", "Route"],
  ["via", "Via"],
  ["delete", "Delete"],
  ["zone", "Zone"],
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

export function PcbView({ snapshot, selected, onSelect, edit, aiPreview = [], onStatus, layerView, focus, airwires }: Props) {
  const ownView = useLayerView(null);
  const lv = layerView ?? ownView;
  const { pcb } = snapshot.project;
  const outline = pcb.board.outline;
  const [tool, setTool] = useState<Tool>("select");
  const [viaFrom, setViaFrom] = useState<CopperLayer>("top");
  const [viaTo, setViaTo] = useState<CopperLayer>("bottom");
  const stack = stackLayers(pcb.board.stackup);
  const order = stack.map((s) => s.layer);
  const shown = (l: CopperLayer) => !lv.hidden.has(layerKey(l));
  const toggle = lv.toggle;
  // The active layer must exist in the current stackup (it may have been removed).
  const layer: CopperLayer = order.find((o) => layerKey(o) === lv.active) ?? order[0] ?? "top";
  const setLayer = lv.setActive;
  const layerColour = lv.colourOf;
  const [zoneClearance, setZoneClearance] = useState("");
  const highlighted = new Set(focus?.objects ?? []);
  const zoomTo = focus ? { bounds: focusBounds(snapshot, focus.objects, focus.location), n: focus.n } : null;
  const [net, setNet] = useState("");
  const [start, setStart] = useState<Point | null>(null);
  const [corners, setCorners] = useState<Point[]>([]);
  const docId = useDocId();
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

  const hasZone = (l: CopperLayer) => Object.values(pcb.zones).some((z) => sameLayer(z.layer, l));
  // One zone over the board outline for every plane layer that has none, on the chosen net.
  const fillPlanes = () => {
    if (!edit || !outline.length || !net) return;
    const cmds = stack
      .filter((s) => s.kind === "plane" && !hasZone(s.layer))
      .map((s) => ({ type: "AddZone" as const, layer: s.layer, net, outline, clearance: null }));
    if (cmds.length) void edit.execute(cmds);
  };

  const reset = () => {
    setStart(null);
    setCorners([]);
    setPicked(null);
  };
  const choose = (t: Tool) => {
    reset();
    setTool(t);
  };
  useRegisterTools(docId, (t) => {
    const hit = TOOLS.find(([id]) => id === t);
    if (hit) choose(hit[0]);
    return !!hit;
  });
  useEffect(() => {
    onStatus?.({ layer, gridMm: GRID / 1_000_000 });
  }, [layer, onStatus]);

  const finishZone = () => {
    if (!edit) return;
    const pts = outlineFrom(corners);
    if (!pts) return edit.notify("A zone needs two opposite corners or at least three points.");
    if (!net) return edit.notify("Pick the net the zone belongs to first.");
    const cl = zoneClearance.trim() === "" ? null : mmToNm(Number(zoneClearance));
    if (cl !== null && !(cl >= 0)) return edit.notify("Zone clearance must be a number of millimetres.");
    setCorners([]);
    void edit.execute({ type: "AddZone", layer, net, outline: pts, clearance: cl });
  };

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
    else if (e.key === "Enter" && tool === "zone") finishZone();
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
    } else if (k === "l" && tool === "route") {
      const i = order.findIndex((o) => sameLayer(o, layer));
      const next = order[(i + 1) % order.length];
      setLayer(next);
      // Switching layer in the middle of a track needs a via where the track stops.
      if (start && net) void edit.execute({ type: "AddVia", position: start, net, drill: null, diameter: null, from: layer, to: next });
    }
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
      case "zone":
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
        void edit.execute({ type: "AddVia", position: snap.point, net: n, drill: null, diameter: null, from: viaFrom, to: viaTo });
        return;
      }
      case "delete": {
        const t = hit(e, "data-track-id");
        const v = hit(e, "data-via-id");
        const z = hit(e, "data-zone-id");
        if (v) void edit.execute({ type: "DeleteVia", via_id: v });
        else if (z && !t) void edit.execute({ type: "DeleteZone", zone_id: z });
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
          {(tool === "route" || tool === "via" || tool === "zone") && (
            <>
              <select
                aria-label="Layer"
                value={layerKey(layer)}
                onChange={(e) => setLayer(order.find((o) => layerKey(o) === e.target.value) ?? "top")}
              >
                {stack.map((s) => (
                  <option key={layerKey(s.layer)} value={layerKey(s.layer)}>
                    {layerName(s.layer)}
                    {s.kind === "plane" ? " (plane)" : ""}
                  </option>
                ))}
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
          {tool === "via" && (
            <>
              <select
                aria-label="Via from"
                value={layerKey(viaFrom)}
                onChange={(e) => setViaFrom(order.find((o) => layerKey(o) === e.target.value) ?? "top")}
              >
                {order.map((o) => (
                  <option key={layerKey(o)} value={layerKey(o)}>
                    {layerName(o)}
                  </option>
                ))}
              </select>
              <select
                aria-label="Via to"
                value={layerKey(viaTo)}
                onChange={(e) => setViaTo(order.find((o) => layerKey(o) === e.target.value) ?? "bottom")}
              >
                {order.map((o) => (
                  <option key={layerKey(o)} value={layerKey(o)}>
                    {layerName(o)}
                  </option>
                ))}
              </select>
              <span className="muted">{viaKind(viaFrom, viaTo)} via</span>
            </>
          )}
          {tool === "zone" && (
            <>
              <label>
                Clearance (mm)
                <input aria-label="Zone clearance" inputMode="decimal" placeholder="rule default" size={9} value={zoneClearance} onChange={(e) => setZoneClearance(e.target.value)} />
              </label>
              <button type="button" onClick={finishZone} disabled={corners.length < 2}>
                Finish zone ({corners.length} pts)
              </button>
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
            {tool === "delete" && "Click a track, via or zone"}
            {tool === "zone" && "Pick layer and net, click two opposite corners (or 3+ points), then Enter"}
          </span>
        </div>
      )}
      <div className="tool-options layer-panel" role="group" aria-label="Layers">
        {edit && (
          <select
            aria-label="Copper layers"
            value={stack.length}
            onChange={(e) => void edit.execute({ type: "SetLayerCount", count: Number(e.target.value), board_thickness: null })}
          >
            {[2, 4, 6, 8].map((n) => (
              <option key={n} value={n}>
                {n} copper layers
              </option>
            ))}
          </select>
        )}
        {stack.map((s) => (
          <label key={layerKey(s.layer)} style={{ color: layerColour(s.layer) }}>
            <input
              type="checkbox"
              checked={shown(s.layer)}
              onChange={() => toggle(s.layer)}
              aria-label={`Show ${layerName(s.layer)}`}
            />{" "}
            {layerName(s.layer)}
            {s.kind === "plane" ? " ▭" : ""}
          </label>
        ))}
        {edit && !stack.every((s) => s.kind !== "plane" || hasZone(s.layer)) && (
          <button type="button" onClick={fillPlanes} disabled={!outline.length || !net}>
            Fill planes with net
          </button>
        )}
      </div>
      <Canvas
        label="PCB canvas"
        bounds={pcbBounds(snapshot)}
        zoomTo={zoomTo && zoomTo.bounds ? { bounds: zoomTo.bounds, n: zoomTo.n } : null}
        tool={!!edit && tool !== "select"}
        onPoint={edit ? onPoint : undefined}
        onHover={edit ? (x, y) => setHover(fromView(x, y)) : undefined}
      >
        {outline.length > 0 && <polygon className="board" points={poly(outline)} />}
        {Object.values(pcb.zones)
          .filter((z) => shown(z.layer))
          .map((z) => (
            <polygon
              key={z.id}
              data-zone-id={z.id}
              className={`zone ${layerKey(z.layer)}${highlighted.has(z.id) ? " finding-hit" : ""}`}
              style={{ fill: layerColour(z.layer), stroke: layerColour(z.layer) }}
              points={poly(z.outline)}
            >
              <title>{`zone ${z.net} (${layerName(z.layer)})`}</title>
            </polygon>
          ))}
        {Object.values(pcb.tracks)
          .filter((t) => shown(t.layer))
          .map((t) => {
          const [x1, y1] = toView(t.a);
          const [x2, y2] = toView(t.b);
          return (
            <g key={t.id} data-track-id={t.id} className={highlighted.has(t.id) ? "finding-hit" : undefined}>
              <line className={`track ${layerKey(t.layer)}`} style={{ stroke: layerColour(t.layer) }} x1={x1} y1={y1} x2={x2} y2={y2} strokeWidth={mm(t.width)}>
                <title>{`${t.net} (${layerName(t.layer)})`}</title>
              </line>
              <line className="hit" x1={x1} y1={y1} x2={x2} y2={y2} />
            </g>
          );
        })}
        {preview.map(([a, b], i) => {
          const [x1, y1] = toView(a);
          const [x2, y2] = toView(b);
          return <line key={i} className={`track ${layerKey(layer)} preview`} style={{ stroke: layerColour(layer) }} x1={x1} y1={y1} x2={x2} y2={y2} strokeWidth={0.25} />;
        })}
        {lv.airwires &&
          (airwires ?? []).map(([n, a, b], i) => {
            const [x1, y1] = toView(a);
            const [x2, y2] = toView(b);
            return (
              <line key={i} className="airwire" x1={x1} y1={y1} x2={x2} y2={y2}>
                <title>{`unrouted: ${n}`}</title>
              </line>
            );
          })}
        {focus?.location && (
          <circle className="finding-marker" cx={toView(focus.location)[0]} cy={toView(focus.location)[1]} r={1.6} />
        )}
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
              className={`footprint ${f.side}${highlighted.has(f.id) || highlighted.has(f.symbol_id) ? " finding-hit" : ""}${f.symbol_id === selected ? " selected" : ""}${aiPreview.includes(f.symbol_id) ? " ai-preview" : ""}`}
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
        {Object.values(pcb.vias)
          .filter((v) => viaLayers(order, v.from ?? "top", v.to ?? "bottom").some(shown))
          .map((v) => {
          const [x, y] = toView(v.position);
          return (
            <g key={v.id} className={highlighted.has(v.id) ? "via finding-hit" : "via"} data-via-id={v.id}>
              <title>{`${viaKind(v.from ?? "top", v.to ?? "bottom")} via ${v.net} (${layerName(v.from ?? "top")}→${layerName(v.to ?? "bottom")})`}</title>
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
