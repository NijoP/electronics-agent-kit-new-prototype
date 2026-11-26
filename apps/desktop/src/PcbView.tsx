// PCB editor: board outline, zones, tracks, vias and footprints from the snapshot, with
// tools that turn clicks into typed engine commands (outline, move, route, via, delete).
import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import { api } from "./api";
import { Canvas, type DragHandler } from "./Canvas";
import { isTyping, type EditCtx } from "./EditContext";
import { mmToNm, outlineFrom, samePoint, snapPoint } from "./editing";
import { Coords, GridLayer, Rulers, snapStepNm, useGridConfig } from "./grid";
import * as P from "./tools/pcb/selection";
import * as O from "./tools/pcb/outline";
import { OutlinePanel } from "./tools/pcb/OutlinePanel";
import * as D from "./tools/pcb/drawTools";
import { layerCountOptions, layerKey, layerName, planeFill, sameLayer, stackLayers, viaKind, viaLayers } from "./layers";
import { stepLayer, useLayerView, type LayerView } from "./layerView";
import { isLockKind, lockProblem, objectLocked, pickableOn } from "./layerLock";
import { focusBounds, type Focus } from "./pcbFocus";
import {
  draftLength,
  draftShoved,
  feedbackText,
  finishOn,
  loopTracks,
  mmText,
  netLengthText,
  netTrackLength,
  nextConnections,
  padAt,
  parseVia,
  ripUpAndReroute,
  routeBatch,
  routeReducer,
  routingFields,
  routingKey,
  saveRouting,
  startProblem,
  stepRequest,
  trackEdit,
  viaChoicesWith,
  widthFromMm,
  widthPresets,
  widthPresetValue,
  type RouteDraft,
} from "./routeTool";
import type { BendMode, Command, ConflictMode, CopperLayer, DrawLayer, FillView, Id, PadGeom, Point, RouteStepReport, Snapshot } from "./types";
import { isActiveEditor, useDocId, useRegisterTools } from "./shell/viewState";
import { ToolOptionFields } from "./shell/Inspector";
import { NO_SELECTION, useHasInspector, usePublishEditorContext, type OptionField } from "./shell/inspectorModel";
import { boundsOf, fromView, mm, toView, type Bounds } from "./viewport";
import { usePcbRenderer, type PcbRenderer } from "./render/flag";
import { buildPcbScene, fillPath } from "./render/pcbScene";
import { dimensionDrawing, outlinePath, trackPath } from "./render/pcbObjects";
import { PcbSceneCanvas } from "./render/PcbSceneCanvas";
import { ATTR_KIND, PickIndex } from "./render/pick";
import { whenConfirmed } from "./confirm";
import type { OverlayItem, OverlayScene, RenderObject } from "./render/types";

/** Canvas picking tolerance in screen pixels. */
const PICK_PX = 3;

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
  /** Engine zone fills of this revision (slice E5); null while they are being computed. */
  zoneFills?: FillView[] | null;
}

type Tool = "select" | "outline" | "move" | "route" | "via" | "delete" | "zone" | "region" | "cutout" | "text" | "dimension";
/** The editor's tools; the first nine have digit keys (in this order). */
export const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["outline", "Board outline"],
  ["move", "Move"],
  ["route", "Route"],
  ["via", "Via"],
  ["delete", "Delete"],
  ["zone", "Zone"],
  ["region", "Keep-out"],
  ["cutout", "Cut-out"],
  ["text", "Text"],
  ["dimension", "Dimension"],
];
/** Hidden-layer key of a drawing layer (as the renderers file them). */
const drawKey = (l: string) => (l === "top_silk" ? "top:silk" : l === "bottom_silk" ? "bottom:silk" : "outline");
const CAPTURE = mmToNm(0.8);
/** Disabled state and title for a button: the reason it is disabled, else what it does (S17). */
const gate = (reason: string | null, title: string) => ({ disabled: reason !== null, title: reason ?? title });
/** Tools under which pressing on a footprint and dragging moves it (instead of panning). */
const MOVE_TOOLS: Tool[] = ["select", "move"];
type Drag = { mode: "move" | "marquee"; from: Point; to: Point; additive: boolean };
const poly = (pts: Point[]) => pts.map((p) => toView(p).join(",")).join(" ");

const hit = (e: { target: EventTarget }, attr: string): string | null =>
  e.target instanceof Element ? (e.target.closest(`[${attr}]`)?.getAttribute(attr) ?? null) : null;

function Pad({ pad, active }: { pad: PadGeom; active?: boolean }) {
  const [x, y] = toView(pad.position);
  const w = mm(pad.size_x);
  const h = mm(pad.size_y);
  const r = pad.shape === "circle" ? Math.min(w, h) / 2 : pad.shape === "oval" ? Math.min(w, h) / 2 : 0;
  return (
    <g className={`pad ${pad.kind}${active ? " net-active" : ""}`} data-pad={pad.number}>
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

/** A track drawn as its outline (outline view): the rectangle around the segment, square ends. */
function segOutline(x1: number, y1: number, x2: number, y2: number, w: number): string {
  const len = Math.hypot(x2 - x1, y2 - y1) || 1;
  const nx = (-(y2 - y1) / len) * (w / 2);
  const ny = ((x2 - x1) / len) * (w / 2);
  const ex = ((x2 - x1) / len) * (w / 2);
  const ey = ((y2 - y1) / len) * (w / 2);
  return [
    [x1 - ex + nx, y1 - ey + ny],
    [x2 + ex + nx, y2 + ey + ny],
    [x2 + ex - nx, y2 + ey - ny],
    [x1 - ex - nx, y1 - ey - ny],
  ]
    .map((p) => p.join(","))
    .join(" ");
}

export function PcbView({ snapshot, selected, onSelect, edit: editProp, aiPreview = [], onStatus, layerView, focus, airwires, zoneFills = null }: Props) {
  const ownView = useLayerView(null);
  const lv = layerView ?? ownView;
  // Layer locks (C1): nothing on a locked layer can be selected, and no command that would change
  // a locked layer is sent (`layerLock.ts`); the user is told which layer to unlock.
  const locks = lv.locked;
  const edit: EditCtx | undefined =
    editProp && locks.size
      ? {
          ...editProp,
          execute: (c) => {
            for (const x of Array.isArray(c) ? c : [c]) {
              const why = lockProblem(snapshot, locks, x);
              if (why) {
                editProp.notify(why);
                return Promise.resolve();
              }
            }
            return editProp.execute(c);
          },
        }
      : editProp;
  const { pcb } = snapshot.project;
  const outline = pcb.board.outline;
  const [tool, setTool] = useState<Tool>("select");
  const [viaFrom, setViaFrom] = useState<CopperLayer>("top");
  const [viaTo, setViaTo] = useState<CopperLayer>("bottom");
  const stack = stackLayers(pcb.board.stackup);
  const order = stack.map((s) => s.layer);
  const shown = (l: CopperLayer) => !lv.hidden.has(layerKey(l));
  const edgeShown = !lv.hidden.has("outline");
  // SVG view: copper off the active layer (or on a locked one) lets the pointer through.
  const offLayer = (l: CopperLayer) => layerKey(l) !== activeKey || lv.locked.has(layerKey(l));
  // The active layer must exist in the current stackup (it may have been removed).
  const layer: CopperLayer = order.find((o) => layerKey(o) === lv.active) ?? order[0] ?? "top";
  const setLayer = lv.setActive;
  const layerColour = lv.colourOf;
  const [zoneClearance, setZoneClearance] = useState("");
  // C4 drawing tools (keep-out, cut-out, text, dimension): their options and the text being moved.
  const [regionForbid, setRegionForbid] = useState("all");
  const [regionLayer, setRegionLayer] = useState("all");
  const [regionName, setRegionName] = useState("");
  const [regionMin, setRegionMin] = useState("");
  const [regionMax, setRegionMax] = useState("");
  const [textLayer, setTextLayer] = useState<DrawLayer>("top_silk");
  const [dimLayer, setDimLayer] = useState<DrawLayer>("documentation");
  const [textValue, setTextValue] = useState("");
  const [textSize, setTextSize] = useState("");
  const [dimOffset, setDimOffset] = useState("");
  const [movingText, setMovingText] = useState<Id | null>(null);
  const highlighted = new Set(focus?.objects ?? []);
  const zoomTo = focus ? { bounds: focusBounds(snapshot, focus.objects, focus.location), n: focus.n } : null;
  const [net, setNet] = useState("");
  const [corners, setCorners] = useState<Point[]>([]);
  const cornersReason = corners.length < 2 ? `Click at least two points first (${corners.length} so far)` : null;
  // Board-outline vertex handle being dragged (index + current snapped position).
  const [vdrag, setVdrag] = useState<{ i: number; to: Point } | null>(null);
  const docId = useDocId();
  // Move tool click-then-click: the footprint picked up and the pointer point it was picked at,
  // so the second click moves it by the grid-snapped pointer delta (as a drag does) and never
  // snaps its origin onto another part's pad or track end.
  const [picked, setPicked] = useState<{ id: Id; from: Point } | null>(null);
  const [hover, setHover] = useState<Point | null>(null);
  // Interactive route: fixed steps (pure reducer) plus the engine's live proposal for the cursor.
  const [draft, dispatch] = useReducer(routeReducer, null);
  const [proposal, setProposal] = useState<RouteStepReport | null>(null);
  const stepSeq = useRef(0);
  // C3 routing options, saved with the board (schema v5, ADR-0044): they start from the board's
  // saved options and every change is saved back with `SetRoutingOptions`.
  const savedRouting = pcb.routing;
  const initial = routingFields(savedRouting);
  const [mode, setMode] = useState<BendMode>(initial.mode);
  // Track width for new routes (mm text; empty = net-class width).
  const [widthText, setWidthText] = useState(initial.widthText);
  // Via preset ("" = net class), obstacle handling and loop removal.
  const [viaChoice, setViaChoice] = useState(initial.viaChoice);
  const [conflict, setConflict] = useState<ConflictMode>(initial.conflict);
  const [removeLoops, setRemoveLoops] = useState(initial.removeLoops);
  const fields = { mode, widthText, viaChoice, conflict, removeLoops };
  const savedKey = routingKey(savedRouting);
  // The board's options changed (opened, undo, another window): show them.
  useEffect(() => {
    if (!saveRouting(savedRouting, fields)) return;
    const s = routingFields(savedRouting);
    setMode(s.mode);
    setWidthText(s.widthText);
    setViaChoice(s.viaChoice);
    setConflict(s.conflict);
    setRemoveLoops(s.removeLoops);
  }, [savedKey]);
  // A changed option is saved after a short pause, so typing a width sends one command.
  const pendingSave = edit ? saveRouting(savedRouting, fields) : null;
  const pendingKey = pendingSave && pendingSave.type === "SetRoutingOptions" ? routingKey(pendingSave.options) : "";
  useEffect(() => {
    if (!edit || !pendingSave) return;
    const t = setTimeout(() => void edit.execute(pendingSave), 600);
    return () => clearTimeout(t);
  }, [pendingKey, savedKey]);
  // Select tool: one committed track picked for editing (width, layer, delete).
  const [trackSel, setTrackSel] = useState<Id | null>(null);
  const [trackWidthText, setTrackWidthText] = useState("");
  // A track on a layer locked after it was picked drops out of the selection.
  const selTrack = trackSel && !objectLocked(snapshot, locks, "track", trackSel) ? pcb.tracks[trackSel] : undefined;
  // Select tool: one via or zone picked for the inspector (MoveVia / EditZone) or Delete (F2).
  const [copperSel, setCopperSel] = useState<{ kind: "via" | "zone"; id: Id } | null>(null);
  const selVia = copperSel?.kind === "via" && !objectLocked(snapshot, locks, "via", copperSel.id) ? pcb.vias[copperSel.id] : undefined;
  const selZone = copperSel?.kind === "zone" && !objectLocked(snapshot, locks, "zone", copperSel.id) ? pcb.zones[copperSel.id] : undefined;
  // Net highlight: selecting a track lights every track, via and zone of its net.
  const hiNet = selTrack?.net ?? null;
  const netHit = (net: string) => (hiNet !== null && net === hiNet ? " net-hit" : "");
  const routeNet = draft?.net ?? null;

  const pads = snapshot.geometry.footprints.flatMap((f) => f.pads.map((p) => ({ at: p.position, net: p.net })));
  const trackEnds = Object.values(pcb.tracks).flatMap((t) => [
    { at: t.a, net: t.net },
    { at: t.b, net: t.net },
  ]);
  const targets = [...pads, ...trackEnds];
  const nets = [...new Set([...pads.map((p) => p.net), ...Object.values(pcb.tracks).map((t) => t.net)])]
    .filter((n): n is string => !!n)
    .sort();
  const [grid] = useGridConfig("pcb");
  // Snap step in nm; with snap off, points and moves land on whole nanometres.
  const GRID = snapStepNm(grid);
  const snapDelta = (from: Point, to: Point): Point => ({
    x: Math.round((to.x - from.x) / GRID) * GRID,
    y: Math.round((to.y - from.y) / GRID) * GRID,
  });
  // Footprint selection: the canvas selection, or the footprint of the symbol selected
  // elsewhere (explorer, schematic cross-probe, findings).
  const [selRaw, setSel] = useState<Id[]>([]);
  const [drag, setDrag] = useState<Drag | null>(null);
  const pressed = useRef<Id | null>(null);
  const footprint = snapshot.geometry.footprints.find((f) => f.symbol_id === selected);
  const pruned = P.prune(selRaw, snapshot);
  const fpLocked = (id: Id) => objectLocked(snapshot, locks, "footprint", id);
  // A picked track, via or zone replaces the cross-probed footprint (Delete must not take it too).
  const sel = (pruned.length ? pruned : footprint && !trackSel && !copperSel ? [footprint.id] : []).filter((id) => !fpLocked(id));
  const one = sel.length === 1 ? snapshot.geometry.footprints.find((f) => f.id === sel[0]) : undefined;
  const lockedCount = sel.filter((id) => pcb.footprints[id]?.locked).length;
  // Footprint edits (R, F, the selection bar) belong to the Select and Move tools only.
  const footprintTool = MOVE_TOOLS.includes(tool);

  // Renderer: the Canvas2D scene (default where a 2D context exists) or the SVG fallback.
  const [rendererChoice, setRenderer, canvasOk] = usePcbRenderer();
  const [ctxFailed, setCtxFailed] = useState(false);
  const useCanvas = rendererChoice === "canvas" && !ctxFailed;
  const activeKey = layerKey(layer);
  const hiddenKey = [...lv.hidden].sort().join(",");
  const colourKey = order.map((l) => lv.colourOf(l)).join(",");
  const sceneAirwires = lv.airwires ? (airwires ?? null) : null;
  const scene = useMemo(
    () =>
      useCanvas
        ? buildPcbScene(snapshot, { order, hidden: lv.hidden, colourOf: lv.colourOf, activeLayer: activeKey, airwires: sceneAirwires, outlineView: lv.outline, fills: zoneFills })
        : null,
    [useCanvas, snapshot, hiddenKey, colourKey, activeKey, sceneAirwires, lv.outline, zoneFills],
  );
  const picker = useMemo(() => (scene ? new PickIndex(scene) : null), [scene]);
  const byKey = useMemo(() => new Map<string, RenderObject>((scene?.objects ?? []).map((o) => [o.key, o])), [scene]);
  // Pixels per millimetre of the last canvas frame, for a pick tolerance of a few screen pixels.
  const scaleRef = useRef(0);
  /**
   * What is under the pointer, as the attribute the SVG markup carries (`data-footprint-id`, …).
   * SVG: the event target's nearest element with that attribute. Canvas: the topmost pickable
   * render object at the view point (x, y), returned only when it is of the asked kind, which is
   * how the topmost SVG element behaves. Outline vertex handles stay SVG in both renderers.
   */
  // C1: picking is active-layer-exclusive (tracks and zones only on the active layer, vias on any
  // layer they span, footprints everywhere) and nothing on a locked layer is ever returned.
  const hitAt = (e: { target: EventTarget }, attr: string, x: number, y: number): string | null => {
    const pk = attr in ATTR_KIND ? ATTR_KIND[attr] : null;
    const kind = pk && isLockKind(pk) ? pk : null;
    let id: string | null;
    if (!picker || !kind) id = hit(e, attr);
    else {
      const s = scaleRef.current > 0 ? scaleRef.current : 10;
      const top = picker.pick(x, y, { tol: PICK_PX / s, activeLayer: activeKey });
      id = top && top.ref.kind === kind ? top.ref.id : null;
    }
    if (id === null || !kind) return id;
    return pickableOn(snapshot, kind, id, activeKey) && !objectLocked(snapshot, locks, kind, id) ? id : null;
  };

  const hasZone = (l: CopperLayer) => Object.values(pcb.zones).some((z) => sameLayer(z.layer, l));
  // One zone over the board outline for every plane layer that has none, on the chosen net:
  // one Batch, so it is one revision and one undo step, and all-or-nothing.
  const fillPlanes = () => {
    if (!edit || !outline.length || !net) return;
    const cmd = planeFill(stack, hasZone, net, outline);
    if (cmd) void edit.execute(cmd);
  };

  const reset = () => {
    dispatch({ type: "cancel" });
    setTrackSel(null);
    setCopperSel(null);
    setProposal(null);
    setCorners([]);
    setPicked(null);
    setMovingText(null);
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
    onStatus?.({ layer, gridMm: grid.spacingMm });
  }, [layer, onStatus, grid.spacingMm]);

  /** Submit a selection edit as one engine command (a `Batch` when there are several). */
  const submit = (commands: Command[], label: string) => {
    if (!edit || !commands.length) return;
    void edit.execute(commands.length === 1 ? commands[0] : { type: "Batch", commands, label });
  };
  const fps = (n: number) => `${n} footprint${n === 1 ? "" : "s"}`;
  /** Warn when locked footprints were left out of a move-type edit. */
  const lockedNote = () => {
    const n = P.movable(snapshot, sel).locked;
    if (n && edit) edit.notify(`${fps(n)} locked; unlock to move.`);
  };
  // Selection actions, shared by the selection bar and the keyboard shortcuts.
  const actions = {
    rotate: () => (lockedNote(), submit(P.rotateCommands(snapshot, sel), `Rotate ${fps(sel.length)}`)),
    flip: () => (lockedNote(), submit(P.flipCommands(snapshot, sel), `Flip ${fps(sel.length)}`)),
    align: (how: P.Align) => (lockedNote(), submit(P.alignCommands(snapshot, sel, how), `Align ${how}`)),
    distribute: (axis: "x" | "y") => (lockedNote(), submit(P.distributeCommands(snapshot, sel, axis), `Distribute ${axis === "x" ? "horizontally" : "vertically"}`)),
    lock: (locked: boolean) => submit(P.lockCommands(snapshot, sel, locked), `${locked ? "Lock" : "Unlock"} ${fps(sel.length)}`),
  };

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

  /** Send a drawing-tool command, or tell the user why it cannot be sent. */
  const draw = (c: Command | string) => {
    if (!edit) return;
    if (typeof c === "string") return edit.notify(c);
    setCorners([]);
    void edit.execute(c);
  };
  const widthLimit = (t: string): number | null | string => {
    if (t.trim() === "") return null;
    const w = widthFromMm(t);
    return w === null || !(w > 0) ? "Region track-width limits must be positive millimetres." : w;
  };
  const finishRegion = () => {
    const min = widthLimit(regionMin);
    const max = widthLimit(regionMax);
    if (typeof min === "string" || typeof max === "string") return edit?.notify((typeof min === "string" ? min : max) as string);
    const l = regionLayer === "all" ? null : (order.find((o) => layerKey(o) === regionLayer) ?? null);
    draw(D.regionCommand(corners, { layer: l, forbid: D.forbidOf(regionForbid), name: regionName, minWidth: min, maxWidth: max }));
  };
  const finishCutout = () => draw(D.cutoutCommand(corners, outline));
  const rotateText = () => {
    const t = movingText ? pcb.texts?.[movingText] : undefined;
    if (t && edit) void edit.execute({ type: "MovePcbText", text_id: t.id, position: t.position, rotation: D.nextTextRotation(t.rotation) });
  };

  // The finished route as ONE Batch (one revision, one undo step), with the committed tracks it
  // makes redundant removed in the same Batch when loop removal is on.
  const routeCommand = (d: RouteDraft) =>
    routeBatch(d, removeLoops ? loopTracks(d, Object.values(pcb.tracks), Object.values(pcb.vias), snapshot.geometry.footprints) : []);
  const finishRoute = () => {
    const cmd = draft ? routeCommand(draft) : null;
    dispatch({ type: "cancel" });
    setProposal(null);
    if (cmd && edit) void edit.execute(cmd);
  };

  const toggleMode = () => {
    setMode((m) => (m === "diagonal45" ? "orthogonal90" : "diagonal45"));
    dispatch({ type: "toggleMode" });
  };
  // Mid-route a layer change means "via here, continue on that layer" (the engine checks the via).
  const changeLayer = (next: CopperLayer) => {
    if (draft && locks.has(layerKey(next))) return edit?.notify(`${layerName(next)} is locked: unlock it in the Layers panel to route on it.`);
    setLayer(next);
    if (draft) dispatch({ type: "switchLayer", layer: next });
  };
  const changeWidth = (text: string) => {
    setWidthText(text);
    dispatch({ type: "setWidth", width: widthFromMm(text) });
  };
  const changeVia = (v: string) => {
    setViaChoice(v);
    dispatch({ type: "setVia", via: parseVia(v) });
  };
  const changeConflict = (c: ConflictMode) => {
    setConflict(c);
    dispatch({ type: "setConflict", conflict: c });
  };
  const startRoute = (n: string, at: Point) => {
    if (locks.has(layerKey(layer))) return edit?.notify(`${layerName(layer)} is locked: unlock it in the Layers panel, or make another layer active.`);
    setNet(n);
    setTrackSel(null);
    dispatch({ type: "start", net: n, layer, at, mode, width: widthFromMm(widthText), conflict, via: parseVia(viaChoice) });
  };
  // F2: everything the PCB has selected (the picked track, via or zone and the footprints) is
  // deleted with the PCB's own commands as ONE command (a Batch: one revision, one undo step).
  // Locked footprints are left out with a note; the schematic symbol is never deleted from here.
  const deletion = (): { cmd: Command | null; footprints: number; locked: number } => {
    const free = sel.filter((id) => !pcb.footprints[id]?.locked);
    const cmds: Command[] = [
      ...(selTrack ? [{ type: "DeleteTrack" as const, track_id: selTrack.id }] : []),
      ...(selVia ? [{ type: "DeleteVia" as const, via_id: selVia.id }] : []),
      ...(selZone ? [{ type: "DeleteZone" as const, zone_id: selZone.id }] : []),
      ...free.map((footprint_id): Command => ({ type: "DeleteFootprint", footprint_id })),
    ];
    const n = cmds.length;
    return { cmd: n === 0 ? null : n === 1 ? cmds[0] : { type: "Batch", commands: cmds, label: `Delete ${n} PCB objects` }, footprints: free.length, locked: sel.length - free.length };
  };
  const deleteSelection = () => {
    const { cmd, footprints, locked } = deletion();
    if (!edit) return;
    if (!cmd) return edit.notify(locked ? `${fps(locked)} locked; unlock to delete.` : "Nothing is selected on the PCB.");
    const go = () => {
      if (locked) edit.notify(`${fps(locked)} locked; left in place.`);
      setTrackSel(null);
      setCopperSel(null);
      setSel([]);
      void edit.execute(cmd);
    };
    if (footprints) void whenConfirmed(`Delete ${fps(footprints)} from the PCB? This is a destructive change.`, go);
    else go();
  };
  // EditTrack keeps the track's id, so the edited track stays selected.
  const applyTrackEdit = (change: { width?: number; layer?: CopperLayer }) => {
    const board = { order, tracks: Object.values(pcb.tracks), vias: Object.values(pcb.vias), footprints: snapshot.geometry.footprints };
    const cmd = selTrack ? trackEdit(selTrack, change, board) : null;
    if (cmd && "refuse" in cmd) return edit?.notify(cmd.refuse);
    if (cmd && edit) void edit.execute(cmd);
  };
  // Clearance and connectivity validation is eak-validation's DRC, never the route tool's guess.
  const checkRouting = () => {
    if (!edit) return;
    api
      .runCheck("drc")
      .then(({ report: r }) =>
        edit.notify(
          `DRC: ${r.errors} error(s), ${r.warnings} warning(s)` +
            `${r.not_implemented.length ? ` · ${r.not_implemented.length} check(s) not implemented` : ""}` +
            ` · ${(airwires ?? []).length} unrouted connection(s)`,
        ),
      )
      .catch((err) => edit.notify(`DRC failed: ${String(err)}`));
  };
  const unrouted = airwires ?? [];
  const queue = nextConnections(unrouted, snapshot.geometry.footprints, snapshot.project.constraints);

  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (!edit || isTyping(e.target) || !isActiveEditor(docId)) return;
    const k = e.key.toLowerCase();
    if (e.ctrlKey || e.metaKey) {
      if (k === "a" && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        setSel(snapshot.geometry.footprints.map((f) => f.id).filter((id) => !fpLocked(id)));
      }
      return;
    }
    if (e.altKey && (k === "t" || k === "b" || e.code === "KeyT" || e.code === "KeyB")) {
      e.preventDefault();
      changeLayer(k === "t" || e.code === "KeyT" ? (order[0] ?? "top") : (order[order.length - 1] ?? "bottom"));
      return;
    }
    if (e.key === "+" || e.key === "=" || e.key === "-") {
      e.preventDefault();
      changeLayer(stepLayer(order, layer, e.key === "-" ? -1 : 1));
      return;
    }
    const digit = Number(e.key);
    if (digit >= 1 && digit <= TOOLS.length) choose(TOOLS[digit - 1][0]);
    else if (e.key === "Escape") {
      reset();
      setDrag(null);
      setVdrag(null);
      setSel([]);
    }
    else if (e.key === "Enter" && tool === "route") finishRoute();
    else if (e.key === "Backspace" && tool === "route") dispatch({ type: "back" });
    else if (e.key === "/" && tool === "route") toggleMode();
    else if ((e.key === "Delete" || e.key === "Backspace") && tool === "select" && deletion().cmd) deleteSelection();
    else if (e.key === "Enter" && tool === "outline") finishOutline();
    else if (e.key === "Enter" && tool === "zone") finishZone();
    else if (e.key === "Enter" && tool === "region") finishRegion();
    else if (e.key === "Enter" && tool === "cutout") finishCutout();
    else if (k === "r" && footprintTool && sel.length) actions.rotate();
    else if (k === "f" && footprintTool && sel.length) actions.flip();
    else if (k === "l" && tool === "route") {
      const i = order.findIndex((o) => sameLayer(o, layer));
      changeLayer(order[(i + 1) % order.length]);
    }
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // Select tool: a click selects exactly the footprint under the pointer (Shift adds, Ctrl/Cmd
  // toggles; empty board clears). Pressing on a footprint and dragging moves the selection
  // (Select and Move tools); pressing an unselected footprint first makes it the selection, so
  // only it moves. A drag on empty board draws a marquee (Select) or pans (other tools).
  const selectClick = (e: React.PointerEvent, x: number, y: number) => {
    const id = hitAt(e, "data-footprint-id", x, y);
    const justPicked = pressed.current;
    pressed.current = null;
    const mode = P.pickMode(e);
    // Copper under the pointer when no footprint is: a via (drawn above tracks), a track, a zone.
    const viaId = id ? null : hitAt(e, "data-via-id", x, y);
    const trackId = id || viaId ? null : hitAt(e, "data-track-id", x, y);
    const zoneId = id || viaId || trackId ? null : hitAt(e, "data-zone-id", x, y);
    setTrackSel(trackId);
    setCopperSel(viaId ? { kind: "via", id: viaId } : zoneId ? { kind: "zone", id: zoneId } : null);
    if (viaId || zoneId) return setSel([]);
    if (trackId) {
      const t = pcb.tracks[trackId];
      if (t) setTrackWidthText((t.width / 1_000_000).toString());
      return setSel([]);
    }
    if (!id) return setSel(mode === "replace" ? [] : sel);
    if (justPicked === id) return;
    setSel(P.pick(sel, id, mode));
    const f = snapshot.geometry.footprints.find((g) => g.id === id);
    if (f) onSelect(f.symbol_id);
  };
  const dragHandler: DragHandler = {
    start: (x, y, e) => {
      if (!edit || edit.busy) return false;
      const at = fromView(x, y);
      if (tool === "outline") {
        const vi = hitAt(e, "data-outline-vertex", x, y);
        if (vi === null) return false;
        if (locks.has("outline")) {
          edit.notify("Board outline is locked: unlock it in the Layers panel to edit.");
          return false;
        }
        setVdrag({ i: Number(vi), to: snapPoint(at, GRID, [], CAPTURE).point });
        return true;
      }
      const id = hitAt(e, "data-footprint-id", x, y);
      pressed.current = null;
      if (!id && tool !== "select") return false;
      if (id && !sel.includes(id)) {
        pressed.current = id;
        setSel(P.pick(sel, id, P.pickMode(e) === "replace" ? "replace" : "add"));
        const f = snapshot.geometry.footprints.find((g) => g.id === id);
        if (f) onSelect(f.symbol_id);
      }
      setDrag({ mode: id ? "move" : "marquee", from: at, to: at, additive: P.pickMode(e) !== "replace" });
      return true;
    },
    move: (x, y) => {
      if (vdrag) return setVdrag({ ...vdrag, to: snapPoint(fromView(x, y), GRID, [], CAPTURE).point });
      setDrag((d) => d && { ...d, to: fromView(x, y) });
    },
    end: (x, y) => {
      if (vdrag) {
        const v = vdrag;
        setVdrag(null);
        const to = snapPoint(fromView(x, y), GRID, [], CAPTURE).point;
        const next = O.moveVertex(outline, v.i, to);
        const bad = O.validateOutline(next);
        if (bad.length) edit?.notify(`Vertex move rejected: ${bad[0]}.`);
        else if (!samePoint(to, outline[v.i])) void edit?.execute({ type: "SetBoardOutline", points: next });
        return;
      }
      const d = drag;
      setDrag(null);
      if (!d) return;
      const to = fromView(x, y);
      if (d.mode === "marquee") {
        const found = P.marquee(snapshot, P.rectOf(d.from, to)).filter((id) => !fpLocked(id));
        setSel(d.additive ? [...sel, ...found.filter((f) => !sel.includes(f))] : found);
        return;
      }
      setPicked(null);
      lockedNote();
      submit(P.moveCommands(snapshot, sel, snapDelta(d.from, to)), `Move ${fps(sel.length)}`);
    },
    cancel: () => (setDrag(null), setVdrag(null)),
  };
  const shownOutline = vdrag && outline[vdrag.i] ? O.moveVertex(outline, vdrag.i, vdrag.to) : outline;
  // The board shape as drawn: arc edges (schema v4) flattened; a vertex drag keeps the edge count.
  const boardShape = outlinePath(shownOutline, pcb.board.outline_arcs);
  const cutouts = Object.values(pcb.cutouts ?? {});
  const movingIds = drag?.mode === "move" ? new Set(P.movable(snapshot, sel).ids) : null;
  const dragDelta = drag?.mode === "move" ? snapDelta(drag.from, drag.to) : null;

  /** Pick tolerance in nm: a few screen pixels at the last canvas scale (0.3 mm in SVG). */
  const tolNm = () => mmToNm(PICK_PX / (scaleRef.current > 0 ? scaleRef.current : 10));
  const shownDrawing = (l: string) => !lv.hidden.has(drawKey(l));
  const onPoint = (x: number, y: number, e: React.PointerEvent) => {
    if (!edit || edit.busy) return;
    if (tool === "select") return selectClick(e, x, y);
    const snap = snapPoint(
      fromView(x, y),
      GRID,
      targets.map((t) => t.at),
      CAPTURE,
    );
    const snapped = snap.hit === null ? null : targets[snap.hit];
    const fpId = hitAt(e, "data-footprint-id", x, y);
    switch (tool) {
      case "outline":
      case "zone":
      case "region":
      case "cutout":
        return setCorners((c) => [...c, snap.point]);
      case "dimension":
        if (!corners.length) return setCorners([snap.point]);
        return draw(D.dimensionCommand(corners[0], snap.point, dimLayer, dimOffset));
      case "text": {
        // Click a text to pick it up, then click where it goes; a click elsewhere places new text.
        if (movingText) {
          const t = pcb.texts?.[movingText];
          setMovingText(null);
          if (t && !samePoint(t.position, snap.point)) void edit.execute({ type: "MovePcbText", text_id: t.id, position: snap.point, rotation: null });
          return;
        }
        const h = D.drawingAt(pcb, fromView(x, y), tolNm(), shownDrawing);
        if (h?.kind === "text") return setMovingText(h.id);
        return draw(D.textCommand(snap.point, textValue, textLayer, textSize));
      }
      case "move": {
        if (!picked) {
          if (fpId) {
            setPicked({ id: fpId, from: fromView(x, y) });
            const f = snapshot.geometry.footprints.find((g) => g.id === fpId);
            if (f) onSelect(f.symbol_id);
          }
          return;
        }
        const { id, from } = picked;
        setPicked(null);
        if (P.movable(snapshot, [id]).locked) return edit.notify(`${fps(1)} locked; unlock to move.`);
        submit(P.moveCommands(snapshot, [id], snapDelta(from, fromView(x, y))), `Move ${fps(1)}`);
        return;
      }
      case "route": {
        // A click anywhere on a pad's copper means that pad (its centre), not only near the centre.
        const pad = padAt(snapshot.geometry.footprints, fromView(x, y), 0);
        if (!draft) {
          if (pad) {
            const why = startProblem(pad, outline);
            return why ? edit.notify(why) : startRoute(pad.net as string, pad.at);
          }
          const n = snapped?.net || net;
          if (!n) return edit.notify("Start on a pad or track end of a net, or pick the net first.");
          return startRoute(n, snap.point);
        }
        // Fix the engine's legal path to the clicked point; ending on copper of the net finishes,
        // a pad of another net is refused (it would short two nets).
        const d = draft;
        const verdict = finishOn(d, pad);
        if (verdict.kind === "refuse") return edit.notify(verdict.why);
        const to = verdict.kind === "finish" && pad ? pad.at : snap.point;
        const ends = verdict.kind === "finish" || (!pad && !!snapped && snapped.net === d.net && !samePoint(snap.point, d.anchor));
        void api
          .routeStep(stepRequest(d, to))
          .then((s) => {
            dispatch({ type: "fix", step: s });
            if (!s.reached) edit.notify(feedbackText(s));
            else if (ends) {
              const fixed = routeReducer(d, { type: "fix", step: s });
              const cmd = fixed ? routeCommand(fixed) : null;
              dispatch({ type: "cancel" });
              setProposal(null);
              if (cmd) void edit.execute(cmd);
            }
          })
          .catch((err) => edit.notify(`route step failed: ${String(err)}`));
        return;
      }
      case "via": {
        const n = snapped?.net ?? net;
        if (!n) return edit.notify("Pick the net for the via first.");
        void edit.execute({ type: "AddVia", position: snap.point, net: n, drill: null, diameter: null, from: viaFrom, to: viaTo });
        return;
      }
      case "delete": {
        const t = hitAt(e, "data-track-id", x, y);
        const v = hitAt(e, "data-via-id", x, y);
        const z = hitAt(e, "data-zone-id", x, y);
        const f = hitAt(e, "data-footprint-id", x, y);
        // Drawing objects (C4) are hit-tested by geometry, the same in both renderers.
        const dr = D.drawingAt(pcb, fromView(x, y), tolNm(), shownDrawing);
        const small = dr && (dr.kind === "text" || dr.kind === "dimension");
        if (v) void edit.execute({ type: "DeleteVia", via_id: v });
        else if (t) void edit.execute({ type: "DeleteTrack", track_id: t });
        else if (dr && small) void edit.execute(D.deleteDrawing(dr));
        // A footprint sits above the zone it is on; the engine refuses a locked one with the reason.
        else if (f) void edit.execute({ type: "DeleteFootprint", footprint_id: f });
        // A keep-out or cut-out sits above the copper zone it is drawn over.
        else if (dr) void edit.execute(D.deleteDrawing(dr));
        else if (z) void edit.execute({ type: "DeleteZone", zone_id: z });
        return;
      }
    }
  };

  // While routing, hovering a pad aims the live preview at the pad centre.
  const hoverPad = hover && tool === "route" && draft ? padAt(snapshot.geometry.footprints, hover, 0) : null;
  const hoverSnap = hover ? (hoverPad?.at ?? snapPoint(hover, GRID, targets.map((t) => t.at), CAPTURE).point) : null;
  // Live route proposal from the engine, debounced at pointer rate; stale replies are dropped.
  const hx = hoverSnap?.x;
  const hy = hoverSnap?.y;
  useEffect(() => {
    if (tool !== "route" || !draft || hx === undefined || hy === undefined) return;
    const seq = ++stepSeq.current;
    const t = setTimeout(() => {
      api
        .routeStep(stepRequest(draft, { x: hx, y: hy }))
        .then((s) => {
          if (seq === stepSeq.current) setProposal(s);
        })
        .catch(() => {
          if (seq === stepSeq.current) setProposal(null);
        });
    }, 30);
    return () => clearTimeout(t);
  }, [tool, draft, hx, hy, snapshot.project.revision]);
  const live = draft && proposal && samePoint(proposal.segments[0]?.a ?? draft.anchor, draft.anchor) ? proposal : null;
  const fixedSegs = (draft?.steps ?? []).flatMap((s) => s.segments.map((g) => ({ ...g, layer: s.layer, width: s.width })));
  const liveSegs = live ? live.segments.map((g) => ({ ...g, layer: live.layer, width: live.width })) : [];
  const routeVias = [...(draft?.steps ?? []).flatMap((s) => (s.via ? [s.via] : [])), ...(live?.via ? [live.via] : [])];
  const shovedNow = new Map((draft ? draftShoved(draft) : []).map((s) => [s.id, s]));
  for (const s of live?.shoved ?? []) shovedNow.set(s.id, s);
  const shovedSegs = [...shovedNow.values()].flatMap((s) => s.points.slice(1).map((b, k) => ({ a: s.points[k], b, layer: s.layer, width: s.width })));

  // Canvas renderer: selection, finding and net highlights and the drag preview are an uncached
  // overlay drawn from the pick table's outlines (`selPoly` / `pathPts`), so they never force the
  // cached layers to redraw.
  const overlayKey = [
    sel.join(","),
    [...highlighted].join(","),
    hiNet ?? "",
    trackSel ?? "",
    copperSel ? `${copperSel.kind}:${copperSel.id}` : "",
    aiPreview.join(","),
    routeNet ?? "",
    movingIds ? [...movingIds].join(",") : "",
    dragDelta ? `${dragDelta.x},${dragDelta.y}` : "",
  ].join("|");
  const overlay = useMemo<OverlayScene>(() => {
    if (!scene) return { items: [] };
    const items: OverlayItem[] = [];
    const add = (key: string, colour: string, px: number, extra: Partial<OverlayItem> = {}) => {
      const o = byKey.get(key);
      const pts = o?.selPoly ?? o?.pathPts;
      if (o && pts) items.push({ pts, closed: !!o.selPoly || pts.length > 2, colour, px, ...extra });
    };
    const fpOfSymbol = new Map(snapshot.geometry.footprints.map((f) => [f.symbol_id, f.id]));
    for (const z of Object.values(pcb.zones)) if (hiNet !== null && z.net === hiNet) add(`zone:${z.id}`, "--accent", 2);
    for (const t of Object.values(pcb.tracks)) if (hiNet !== null && t.net === hiNet) add(`track:${t.id}`, "--accent", 3);
    for (const v of Object.values(pcb.vias)) if (hiNet !== null && v.net === hiNet) add(`via:${v.id}`, "--accent", 2);
    if (trackSel) add(`track:${trackSel}`, "--accent", 4);
    if (copperSel) add(`${copperSel.kind}:${copperSel.id}`, "--accent", 3);
    if (routeNet)
      for (const f of snapshot.geometry.footprints) for (const p of f.pads) if (p.net === routeNet) add(`pad:${f.id}:${p.number}`, "--accent", 1.5);
    for (const f of snapshot.geometry.footprints) if (aiPreview.includes(f.symbol_id)) add(`footprint:${f.id}`, "--c-preview", 1.5, { dash: [4, 3] });
    for (const id of sel) add(`footprint:${id}`, "--accent", 2, { fill: "--accent-soft" });
    for (const id of highlighted) {
      const fp = fpOfSymbol.get(id) ?? id;
      for (const key of [`footprint:${fp}`, `track:${id}`, `via:${id}`, `zone:${id}`]) add(key, "--err", 2);
    }
    const moving = movingIds && dragDelta ? { owners: movingIds, delta: [mm(dragDelta.x), -mm(dragDelta.y)] as [number, number] } : null;
    return { items, moving };
    // Keyed on the overlay state; `scene` changes with the snapshot.
  }, [scene, byKey, overlayKey]);
  // A5: the active tool's options, shown by the right dock's inspector (inline here when no
  // inspector is shown), and the selection (footprints, the picked track) the inspector edits.
  const layerOpts = order.map((o) => ({ value: layerKey(o), label: `${layerName(o)}${stack.find((x) => sameLayer(x.layer, o))?.kind === "plane" ? " (plane)" : ""}` }));
  const layerNet: OptionField[] = [
    { kind: "select", name: "Layer", label: "Layer", value: layerKey(layer), options: layerOpts, onChange: (v) => changeLayer(order.find((o) => layerKey(o) === v) ?? "top") },
    { kind: "select", name: "Net", label: "Net", value: net, options: [{ value: "", label: "net…" }, ...nets.map((n) => ({ value: n, label: n }))], onChange: setNet },
  ];
  // C3: presets and the live net length follow the net being routed (else the picked net).
  const lengthNet = draft?.net ?? net;
  const widthChoices = widthPresets(snapshot.project.constraints, lengthNet, widthFromMm(widthText));
  const plainLayers = order.map((o) => ({ value: layerKey(o), label: layerName(o) }));
  const options: OptionField[] = !edit
    ? []
    : tool === "route"
      ? [
          ...layerNet,
          {
            kind: "select",
            name: "Width preset",
            label: "Width",
            value: widthPresetValue(widthChoices, widthFromMm(widthText)),
            options: widthChoices,
            onChange: (v) => {
              if (v !== "custom") changeWidth(v === "" ? "" : mmText(Number(v)));
            },
            title: "Track width: the net class, another class's width or a common width (or type one below)",
          },
          { kind: "text", name: "Track width", label: "Width (mm)", value: widthText, onChange: changeWidth, placeholder: "net class", title: "Track width for this route; empty uses the net-class width" },
          {
            kind: "select",
            name: "Via preset",
            label: "Via size",
            value: viaChoice,
            options: viaChoicesWith(snapshot.project.constraints, lengthNet, viaChoice),
            onChange: changeVia,
            title: "Drill / diameter of the via placed when the route changes layer",
          },
          {
            kind: "select",
            name: "Bend mode",
            label: "Corners",
            value: mode,
            options: [
              { value: "diagonal45", label: "45°" },
              { value: "orthogonal90", label: "90°" },
            ],
            onChange: (v) => v !== mode && toggleMode(),
            title: "45° or 90° corners (/)",
          },
          { kind: "unavailable", name: "Arc corners", label: "Arc corners", reason: "Unavailable: the route tool draws straight 45° / 90° segments only. The EAK-IR stores arc tracks (schema v4, AddTrack arc_deg) and the canvas draws them, but the route tool does not make arc corners yet" },
          {
            kind: "select",
            name: "Obstacle mode",
            label: "Obstacles",
            value: conflict,
            options: [
              { value: "walkaround", label: "Walk around" },
              { value: "push", label: "Push (shove tracks)" },
              { value: "block", label: "Block (stop before)" },
            ],
            onChange: (v) => changeConflict(v === "block" || v === "push" ? v : "walkaround"),
            title: "Walk around other-net copper, push other-net tracks aside (pads and vias never move), or stop before it",
          },
          {
            kind: "select",
            name: "Remove loops",
            label: "Loops",
            value: removeLoops ? "on" : "off",
            options: [
              { value: "on", label: "Remove loops" },
              { value: "off", label: "Keep loops" },
            ],
            onChange: (v) => setRemoveLoops(v === "on"),
            title: "When a finished route joins two points its net already joins by an unbranched track chain, delete that chain in the same undo step",
          },
          {
            kind: "select",
            name: "Ratsnest",
            label: "Ratsnest",
            value: lv.airwires ? "shown" : "hidden",
            options: [
              { value: "shown", label: "Shown" },
              { value: "hidden", label: "Hidden" },
            ],
            onChange: (v) => lv.setAirwires(v === "shown"),
            title: "Show or hide the airwires of unrouted connections",
          },
          { kind: "info", name: "Net length", label: "Net length", value: lengthNet ? netLengthText(netTrackLength(Object.values(pcb.tracks), lengthNet), draftLength(draft, live)) : "pick a net or start a route" },
          { kind: "info", name: "Route via", label: "Via", value: "L or Via + switch layer; the via size above" },
        ]
      : tool === "via"
        ? [
            // F2 (S15): no Layer field here: a via spans From..To, the active layer plays no part.
            layerNet[1],
            { kind: "select", name: "Via from", label: "From", value: layerKey(viaFrom), options: plainLayers, onChange: (v) => setViaFrom(order.find((o) => layerKey(o) === v) ?? "top") },
            { kind: "select", name: "Via to", label: "To", value: layerKey(viaTo), options: plainLayers, onChange: (v) => setViaTo(order.find((o) => layerKey(o) === v) ?? "bottom") },
            { kind: "info", name: "Via kind", label: "Kind", value: `${viaKind(viaFrom, viaTo)} via` },
          ]
        : tool === "zone"
          ? [
              ...layerNet,
              { kind: "text", name: "Zone clearance", label: "Clearance (mm)", value: zoneClearance, onChange: setZoneClearance, placeholder: "rule default" },
              { kind: "unavailable", name: "Zone fill", label: "Fill method, islands, priority", reason: "Unavailable: the engine fills every zone solid (0.1 mm raster, thermal reliefs, dead islands removed) and the EAK-IR has no fill-method, island or priority fields" },
            ]
          : tool === "region"
            ? [
                { kind: "select", name: "Keep-out layers", label: "Layers", value: regionLayer, options: [{ value: "all", label: "All copper layers" }, ...plainLayers], onChange: setRegionLayer, title: "The copper layers the region applies to" },
                { kind: "select", name: "Keep out", label: "Keep out", value: regionForbid, options: D.FORBID_PRESETS.map(([value, label]) => ({ value, label })), onChange: setRegionForbid, title: "What may not be inside the region (DRC-021); pours are kept out of the fill" },
                { kind: "text", name: "Region name", label: "Name", value: regionName, onChange: setRegionName, placeholder: "optional" },
                { kind: "text", name: "Region min width", label: "Min track (mm)", value: regionMin, onChange: setRegionMin, placeholder: "none", title: "Narrowest track allowed inside (DRC-022)" },
                { kind: "text", name: "Region max width", label: "Max track (mm)", value: regionMax, onChange: setRegionMax, placeholder: "none", title: "Widest track allowed inside (DRC-022)" },
              ]
            : tool === "text" || tool === "dimension"
              ? [
                  { kind: "select", name: "Drawing layer", label: "Layer", value: tool === "text" ? textLayer : dimLayer, options: D.DRAW_LAYERS.map(([value, label]) => ({ value, label })), onChange: (v) => (tool === "text" ? setTextLayer : setDimLayer)(D.DRAW_LAYERS.find(([l]) => l === v)?.[0] ?? "documentation") },
                  ...(tool === "text"
                    ? ([
                        { kind: "text", name: "PCB text", label: "Text", value: textValue, onChange: setTextValue, placeholder: "type the text" },
                        { kind: "text", name: "Text height", label: "Height (mm)", value: textSize, onChange: setTextSize, placeholder: "1" },
                      ] as OptionField[])
                    : ([{ kind: "text", name: "Dimension offset", label: "Offset (mm)", value: dimOffset, onChange: setDimOffset, placeholder: "0", title: "Distance of the dimension line from the measured points (positive: to the left of A→B)" }] as OptionField[])),
                ]
              : [];
  usePublishEditorContext(docId, {
    editor: "pcb",
    tool,
    toolLabel: TOOLS.find(([t]) => t === tool)?.[1] ?? tool,
    options,
    selection: { ...NO_SELECTION, footprints: sel, tracks: selTrack ? [selTrack.id] : [], vias: selVia ? [selVia.id] : [], zones: selZone ? [selZone.id] : [] },
    // F2 (S2): Edit › Delete, the toolbar and the context menu run the PCB's own delete, never DeleteSymbol.
    ops: edit
      ? { delete: { state: edit.busy ? "An engine command is running" : deletion().cmd ? true : sel.length ? `${fps(sel.length)} locked; unlock to delete` : "Select a track, via, zone or footprint first", run: deleteSelection } }
      : undefined,
  });
  const inspector = useHasInspector();
  // A click on a footprint selects its symbol (cross-probe), as the SVG footprint's onClick does.
  const canvasPoint = (x: number, y: number, e: React.PointerEvent) => {
    const id = hitAt(e, "data-footprint-id", x, y);
    const f = id ? snapshot.geometry.footprints.find((g) => g.id === id) : undefined;
    if (f) onSelect(f.symbol_id);
    if (edit) onPoint(x, y, e);
  };

  return (
    <section className={`view pcb-view${lv.outline ? " outline-view" : ""}`} aria-label="PCB view" data-flip={lv.flip ? "true" : undefined} data-outline-view={lv.outline ? "true" : undefined} data-zone-fills={zoneFills ? zoneFills.length : undefined} data-regions={Object.keys(pcb.regions ?? {}).length} data-cutouts={cutouts.length} data-texts={Object.keys(pcb.texts ?? {}).length} data-dimensions={Object.keys(pcb.dimensions ?? {}).length}>
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
                title={i < 9 ? `${label} (${i + 1})` : `${label} (Place menu)`}
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
        <Coords at={vdrag?.to ?? hover} unit={grid.unit} />
        <label
          className="renderer-pick"
          title={
            canvasOk && !ctxFailed
              ? "Canvas2D draws large boards faster; SVG is the original renderer, kept as a fallback"
              : "Canvas2D is unavailable here (no 2D canvas context), so the SVG renderer is used"
          }
        >
          Renderer
          <select aria-label="PCB renderer" value={useCanvas ? "canvas" : "svg"} onChange={(e) => setRenderer(e.target.value as PcbRenderer)}>
            <option value="canvas" disabled={!canvasOk || ctxFailed}>
              {canvasOk && !ctxFailed ? "Canvas2D" : "Canvas2D (unavailable)"}
            </option>
            <option value="svg">SVG (fallback)</option>
          </select>
        </label>
      </header>
      {edit && (
        <div className="tool-options">
          {!inspector && <ToolOptionFields fields={options} layout="strip" />}
          {tool === "route" && (
            <>
              {draft && (
                <>
                  <span className="route-net" aria-label="Active net">Routing {draft.net} · {layerName(draft.layer)}{draft.viaPending ? " · via next" : ""}</span>
                  <span className="route-net muted" aria-label="Live net length" title="Committed tracks of the net plus the route being drawn">{`net length ${netLengthText(netTrackLength(Object.values(pcb.tracks), draft.net), draftLength(draft, live))}`}</span>
                  <button type="button" title="Place a via here and continue on the next copper layer (L)" onClick={() => changeLayer(order[(order.findIndex((o) => sameLayer(o, layer)) + 1) % order.length])}>
                    Via + switch layer
                  </button>
                  <button type="button" {...gate(draft.steps.length ? null : "No fixed step to remove yet: click to fix a step first", "Remove the last fixed step (Backspace)")} onClick={() => dispatch({ type: "back" })}>
                    Undo step
                  </button>
                  <button type="button" title="Discard this route (Esc)" onClick={reset}>
                    Cancel
                  </button>
                </>
              )}
              <button
                type="button"
                onClick={finishRoute}
                {...gate(!draft ? "No route started: click a pad or track end first" : !draft.steps.length ? "The route has no fixed step yet: click to fix one" : null, "Commit the route as one undo step (Enter)")}
              >
                Finish route ({draft?.steps.length ?? 0} steps)
              </button>
              <button
                type="button"
                onClick={() => edit && net && void edit.execute(ripUpAndReroute(net))}
                {...gate(
                  !net ? "Pick a net in the Net field first" : !Object.values(pcb.tracks).some((t) => t.net === net) ? `Net ${net} has no tracks to rip up` : null,
                  "Remove this net's tracks and vias and re-route it with the EAK router (one undo step)",
                )}
              >
                Rip up &amp; re-route net
              </button>
              <button
                type="button"
                {...gate(
                  draft ? "Finish or cancel the current route first" : !queue.length ? "No unrouted connection is left" : null,
                  "Start routing the most constrained unrouted connection (power and high-current first, then shortest)",
                )}
                onClick={() => queue[0] && startRoute(queue[0][0], queue[0][1])}
              >
                Route next ({unrouted.length} unrouted)
              </button>
              <button type="button" title="Run DRC (clearance and connectivity) in eak-validation" onClick={checkRouting}>
                Check routing
              </button>
            </>
          )}
          {tool === "zone" && (
            <>
              <button type="button" onClick={finishZone} {...gate(cornersReason, "Commit the shape (Enter)")}>
                Finish zone ({corners.length} pts)
              </button>
            </>
          )}
          {tool === "region" && (
            <button type="button" onClick={finishRegion} {...gate(cornersReason, "Commit the shape (Enter)")}>
              Finish keep-out ({corners.length} pts)
            </button>
          )}
          {tool === "cutout" && (
            <button type="button" onClick={finishCutout} {...gate(cornersReason, "Commit the shape (Enter)")}>
              Finish cut-out ({corners.length} pts)
            </button>
          )}
          {tool === "text" && movingText && (
            <span className="selection-bar" role="group" aria-label="Text selection">
              <span className="muted">{`text "${pcb.texts?.[movingText]?.text ?? ""}" picked`}</span>
              <button type="button" title="Turn the picked text 90° (MovePcbText)" onClick={rotateText}>
                Rotate 90°
              </button>
              <button type="button" title="Put the text down where it is (Esc)" onClick={() => setMovingText(null)}>
                Cancel move
              </button>
            </span>
          )}
          {tool === "outline" && edit && (
            <>
              <button type="button" onClick={finishOutline} {...gate(cornersReason, "Commit the shape (Enter)")}>
                Finish outline ({corners.length} pts)
              </button>
              <OutlinePanel edit={edit} outline={outline} arcs={pcb.board.outline_arcs ?? []} corners={corners} setCorners={setCorners} />
            </>
          )}
          {tool === "select" && selTrack && (
            <span className="selection-bar" role="group" aria-label="Track selection">
              <span className="muted">{`track ${selTrack.net} · ${layerName(selTrack.layer)}`}</span>
              {!inspector && (
                <>
                  <label>
                    Width (mm)
                    <input aria-label="Selected track width" inputMode="decimal" size={6} value={trackWidthText} onChange={(e) => setTrackWidthText(e.target.value)} />
                  </label>
                  <button type="button" {...gate(widthFromMm(trackWidthText) === null ? "Type a positive width in mm first" : null, "Set the track width (EditTrack)")} onClick={() => applyTrackEdit({ width: widthFromMm(trackWidthText) ?? undefined })}>
                    Apply width
                  </button>
                  <select aria-label="Selected track layer" value={layerKey(selTrack.layer)} onChange={(e) => { const l = order.find((o) => layerKey(o) === e.target.value); if (l) applyTrackEdit({ layer: l }); }}>
                    {order.map((o) => (
                      <option key={layerKey(o)} value={layerKey(o)}>
                        {layerName(o)}
                      </option>
                    ))}
                  </select>
                </>
              )}
              <button type="button" title="Delete this track (Delete)" onClick={deleteSelection}>
                Delete track
              </button>
            </span>
          )}
          {tool === "select" && (selVia || selZone) && (
            <span className="selection-bar" role="group" aria-label={selVia ? "Via selection" : "Zone selection"}>
              <span className="muted">{selVia ? `via ${selVia.net}` : selZone ? `zone ${selZone.net} · ${layerName(selZone.layer)}` : ""}</span>
              {!inspector && <span className="muted">· position, net and layer are edited in the Properties panel</span>}
              <button type="button" title={`Delete this ${selVia ? "via" : "zone"} (Delete)`} onClick={deleteSelection}>
                {selVia ? "Delete via" : "Delete zone"}
              </button>
            </span>
          )}
          {footprintTool && sel.length > 0 && (
            <span className="selection-bar" role="group" aria-label="Footprint selection">
              <span className="muted">
                {one ? `${one.refdes} · ${one.side}${lockedCount ? " · locked" : ""}` : `${fps(sel.length)}${lockedCount ? ` (${lockedCount} locked)` : ""}`}
              </span>
              <button type="button" title="Rotate 90° (R)" onClick={actions.rotate}>Rotate</button>
              <button type="button" title="Flip to the other board side (F)" onClick={actions.flip}>Flip</button>
              {sel.length >= 2 && (
                <>
                  <button type="button" title="Align left edges" onClick={() => actions.align("left")}>Align left</button>
                  <button type="button" title="Align right edges" onClick={() => actions.align("right")}>Align right</button>
                  <button type="button" title="Align top edges" onClick={() => actions.align("top")}>Align top</button>
                  <button type="button" title="Align bottom edges" onClick={() => actions.align("bottom")}>Align bottom</button>
                  <button type="button" title="Put centres on one horizontal line" onClick={() => actions.align("hcenter")}>Center H</button>
                  <button type="button" title="Put centres on one vertical line" onClick={() => actions.align("vcenter")}>Center V</button>
                  <button type="button" {...gate(sel.length < 3 ? "Select 3 or more footprints to distribute" : null, "Equal horizontal spacing")} onClick={() => actions.distribute("x")}>Distribute H</button>
                  <button type="button" {...gate(sel.length < 3 ? "Select 3 or more footprints to distribute" : null, "Equal vertical spacing")} onClick={() => actions.distribute("y")}>Distribute V</button>
                </>
              )}
              <button type="button" {...gate(lockedCount === sel.length ? "Every selected footprint is already locked" : null, "Lock: the footprint cannot be moved")} onClick={() => actions.lock(true)}>Lock</button>
              <button type="button" {...gate(lockedCount === 0 ? "No selected footprint is locked" : null, "Unlock")} onClick={() => actions.lock(false)}>Unlock</button>
            </span>
          )}
          <span className="muted hint">
            {outline.length === 0 && tool !== "outline" && "No board outline yet — use Board outline. "}
            {tool === "outline" && "Pick a shape and type exact sizes, or click two opposite corners (3+ points = polygon) then Enter · drag a vertex handle to edit"}
            {tool === "select" && (sel.length ? `${fps(sel.length)} selected · drag to move · R rotate · F flip · Esc clear` : "Click a footprint (Shift adds, Ctrl toggles) or drag a box · middle-drag pans")}
            {tool === "move" && (picked ? "Click the new position" : "Drag a footprint, or click one then its destination")}
            {tool === "route" &&
              (draft
                ? `${feedbackText(live) || "Move to the target"} · click fixes · Enter commits · L layer (via) · / 45°/90° · Backspace undoes a step · Esc cancels`
                : "Click a pad (or track end) to start routing its net; airwires show what is still unrouted")}
            {tool === "select" && selTrack && ` · track ${selTrack.net} selected · Delete removes it`}
            {tool === "via" && "Click a pad, track end or point"}
            {tool === "delete" && "Click a track, via, footprint, zone, keep-out, cut-out, text or dimension"}
            {tool === "zone" && "Pick layer and net, click two opposite corners (or 3+ points), then Enter"}
            {tool === "region" && "Pick layers and what to keep out, click two opposite corners (or 3+ points), then Enter"}
            {tool === "cutout" && "Click two opposite corners (or 3+ points) inside the board, then Enter"}
            {tool === "text" && (movingText ? "Click where the text goes · Esc keeps it" : "Type the text, then click to place it · click a text to move it")}
            {tool === "dimension" && (corners.length ? "Click the second point" : "Click the first point to measure from")}
          </span>
        </div>
      )}
      <div className="tool-options layer-panel" role="group" aria-label="Layers">
        {edit && (
          <select
            aria-label="Copper layers"
            value={stack.length}
            disabled={edit.busy}
            onChange={(e) => void edit.execute({ type: "SetLayerCount", count: Number(e.target.value), board_thickness: null })}
          >
            {layerCountOptions(stack.length).map((n) => (
              <option key={n} value={n}>
                {n} copper layers
              </option>
            ))}
          </select>
        )}
        <div
          className="layer-tabs"
          role="tablist"
          aria-label="Active copper layer"
          onKeyDown={(e) => {
            if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
            e.preventDefault();
            e.stopPropagation();
            changeLayer(stepLayer(order, layer, e.key === "ArrowRight" ? 1 : -1));
          }}
        >
          {stack.map((s) => {
            const k = layerKey(s.layer);
            const on = k === activeKey;
            const isLocked = locks.has(k);
            const isHidden = !shown(s.layer);
            return (
              <button
                key={k}
                type="button"
                role="tab"
                aria-selected={on}
                tabIndex={on ? 0 : -1}
                data-layer={k}
                data-locked={isLocked ? "true" : undefined}
                data-hidden={isHidden ? "true" : undefined}
                className={`layer-tab${on ? " on" : ""}${isLocked ? " locked" : ""}${isHidden ? " off" : ""}`}
                title={`${layerName(s.layer)}: make it the active layer (+ / - step, Alt+T / Alt+B top / bottom)${isLocked ? " · locked" : ""}${isHidden ? " · hidden" : ""}`}
                onClick={() => changeLayer(s.layer)}
              >
                <span className="swatch" aria-hidden="true" style={{ background: layerColour(s.layer) }} />
                {layerName(s.layer)}
                {s.kind === "plane" ? " ▭" : ""}
                {isLocked && <span className="tab-flag" aria-hidden="true">🔒</span>}
              </button>
            );
          })}
        </div>
        <button
          type="button"
          aria-pressed={lv.flip}
          className={lv.flip ? "on" : ""}
          title="Flip board: view the board from the bottom (mirrored left to right)"
          onClick={() => lv.setFlip(!lv.flip)}
        >
          Flip board
        </button>
        <button
          type="button"
          aria-pressed={lv.outline}
          className={lv.outline ? "on" : ""}
          title="Outline view: draw tracks, pads, vias and zones as outlines"
          onClick={() => lv.setOutline(!lv.outline)}
        >
          Outline view
        </button>
        {edit && !stack.every((s) => s.kind !== "plane" || hasZone(s.layer)) && (
          <button
            type="button"
            onClick={fillPlanes}
            {...gate(
              !outline.length ? "Draw the board outline first" : !net ? "Pick a net in the Net field (Route, Via or Zone tool) first" : null,
              "Add one zone over the board outline on every plane layer that has none, on the chosen net (one undo step)",
            )}
          >
            Fill planes with net
          </button>
        )}
      </div>
      <Canvas
        label="PCB canvas"
        bounds={pcbBounds(snapshot)}
        mirror={lv.flip}
        zoomTo={zoomTo && zoomTo.bounds ? { bounds: zoomTo.bounds, n: zoomTo.n } : null}
        tool={!!edit && tool !== "select"}
        onPoint={scene ? canvasPoint : edit ? onPoint : undefined}
        onHover={(x, y) => setHover(fromView(x, y))}
        onDrag={edit && (MOVE_TOOLS.includes(tool) || tool === "outline") ? dragHandler : undefined}
        underlay={(vb) => (
          <>
            {outline.length > 0 && edgeShown && <polygon className="board" points={poly(boardShape)} />}
            {cutouts.map((c) => (
              <polygon key={c.id} className="board-cutout" data-cutout-id={c.id} points={poly(c.outline)} />
            ))}
            <GridLayer vb={vb} kind="pcb" cfg={grid} />
          </>
        )}
        overlay={(vb) => <Rulers vb={vb} kind="pcb" cfg={grid} />}
        scene={
          scene
            ? (vb, size) => {
                if (size.width > 0 && vb.w > 0) scaleRef.current = size.width / vb.w;
                return <PcbSceneCanvas scene={scene} overlay={overlay} vb={vb} size={size} onUnavailable={() => setCtxFailed(true)} />;
              }
            : undefined
        }
      >
        {!scene && outline.length > 0 && edgeShown && <polygon className="board-edge" points={poly(boardShape)} />}
        {!scene && <PcbDrawings pcb={pcb} />}
        {edit && tool === "outline" &&
          shownOutline.map((p, i) => {
            const [cx, cy] = toView(p);
            return <circle key={i} className="outline-handle" data-outline-vertex={i} cx={cx} cy={cy} r={0.9}><title>{`vertex ${i + 1}: drag to move`}</title></circle>;
          })}
        {!scene &&
          Object.values(pcb.zones)
          .filter((z) => shown(z.layer))
          .map((z) => {
            // With the engine fill known, the outline is the hit target and the fill is drawn
            // from the DRC raster; until then the zone area is drawn translucent.
            const fill = zoneFills?.find((f) => f.zone_id === z.id);
            return [
              <polygon
                key={z.id}
                data-zone-id={z.id}
                className={`zone ${layerKey(z.layer)}${zoneFills ? " has-fill" : ""}${highlighted.has(z.id) ? " finding-hit" : ""}${netHit(z.net)}${offLayer(z.layer) ? " off-layer" : ""}`}
                style={{ fill: layerColour(z.layer), stroke: layerColour(z.layer) }}
                points={poly(z.outline)}
              >
                <title>{`zone ${z.net} (${layerName(z.layer)})`}</title>
              </polygon>,
              fill && !lv.outline && (
                <path key={`${z.id}-fill`} className="zone-fill" data-zone-fill={z.id} style={{ fill: layerColour(z.layer) }} d={fillPath(fill)} />
              ),
            ];
          })}
        {!scene &&
          Object.values(pcb.tracks)
          .filter((t) => shown(t.layer))
          .map((t) => {
          const [x1, y1] = toView(t.a);
          const [x2, y2] = toView(t.b);
          if (t.arc_deg) {
            // Arc track (schema v4): drawn as its chords.
            const pts = poly(trackPath(t));
            return (
              <g key={t.id} data-track-id={t.id} className={`${highlighted.has(t.id) ? "finding-hit" : ""}${t.id === trackSel ? " selected-track" : ""}${netHit(t.net)}` || undefined}>
                <polyline className={`track ${layerKey(t.layer)}`} style={{ stroke: layerColour(t.layer), fill: "none" }} points={pts} strokeWidth={mm(t.width)}>
                  <title>{`${t.net} (${layerName(t.layer)}, arc ${t.arc_deg}°)`}</title>
                </polyline>
                <polyline className="hit" points={pts} style={{ fill: "none" }} />
              </g>
            );
          }
          return (
            <g key={t.id} data-track-id={t.id} className={`${highlighted.has(t.id) ? "finding-hit" : ""}${t.id === trackSel ? " selected-track" : ""}${netHit(t.net)}${offLayer(t.layer) ? " off-layer" : ""}` || undefined}>
              {lv.outline ? (
                <polygon className={`track-outline ${layerKey(t.layer)}`} style={{ stroke: layerColour(t.layer) }} points={segOutline(x1, y1, x2, y2, mm(t.width))}>
                  <title>{`${t.net} (${layerName(t.layer)})`}</title>
                </polygon>
              ) : (
                <line className={`track ${layerKey(t.layer)}`} style={{ stroke: layerColour(t.layer) }} x1={x1} y1={y1} x2={x2} y2={y2} strokeWidth={mm(t.width)}>
                  <title>{`${t.net} (${layerName(t.layer)})`}</title>
                </line>
              )}
              <line className="hit" x1={x1} y1={y1} x2={x2} y2={y2} />
            </g>
          );
        })}
        {[...fixedSegs.map((g) => ({ ...g, cls: "fixed" })), ...liveSegs.map((g) => ({ ...g, cls: "proposal" })), ...shovedSegs.map((g) => ({ ...g, cls: "shoved" }))].map((g, i) => {
          const [x1, y1] = toView(g.a);
          const [x2, y2] = toView(g.b);
          return (
            <line
              key={i}
              className={`track ${layerKey(g.layer)} preview ${g.cls}`}
              data-route-preview={g.cls}
              style={{ stroke: layerColour(g.layer) }}
              x1={x1}
              y1={y1}
              x2={x2}
              y2={y2}
              strokeWidth={mm(g.width)}
            />
          );
        })}
        {routeVias.map((v, i) => (
          <circle key={`rv${i}`} className="via preview" cx={toView(v.position)[0]} cy={toView(v.position)[1]} r={mm(v.diameter) / 2} />
        ))}
        {live && !live.reached && hoverSnap && (
          <line className="route-blocked" x1={toView(live.end)[0]} y1={toView(live.end)[1]} x2={toView(hoverSnap)[0]} y2={toView(hoverSnap)[1]}>
            <title>{feedbackText(live)}</title>
          </line>
        )}
        {!scene &&
          lv.airwires &&
          (airwires ?? []).map(([n, a, b], i) => {
            const [x1, y1] = toView(a);
            const [x2, y2] = toView(b);
            return (
              <line key={i} className={n === routeNet ? "airwire active" : "airwire"} x1={x1} y1={y1} x2={x2} y2={y2}>
                <title>{`unrouted: ${n}`}</title>
              </line>
            );
          })}
        {focus?.location && (
          <circle className="finding-marker" cx={toView(focus.location)[0]} cy={toView(focus.location)[1]} r={1.6} />
        )}
        {corners.length > 0 && <polyline className="outline-draft" points={poly([...corners, ...(hoverSnap ? [hoverSnap] : [])])} />}
        {!scene &&
          snapshot.geometry.footprints.map((f) => {
          const [x, y] = toView(f.position);
          const hw = mm(f.half_w);
          const hh = mm(f.half_h);
          return (
            <g
              key={f.id}
              data-footprint-id={f.id}
              data-symbol-id={f.symbol_id}
              data-refdes={f.refdes}
              className={`footprint ${f.side}${highlighted.has(f.id) || highlighted.has(f.symbol_id) ? " finding-hit" : ""}${sel.includes(f.id) ? " selected" : ""}${pcb.footprints[f.id]?.locked ? " locked" : ""}${aiPreview.includes(f.symbol_id) ? " ai-preview" : ""}`}
              transform={dragDelta && movingIds?.has(f.id) ? `translate(${mm(dragDelta.x)} ${-mm(dragDelta.y)})` : undefined}
              onClick={() => onSelect(f.symbol_id)}
            >
              <rect className="courtyard" x={x - hw} y={y - hh} width={2 * hw} height={2 * hh} />
              {sel.includes(f.id) && <rect className="sel-box" x={x - hw - 0.3} y={y - hh - 0.3} width={2 * hw + 0.6} height={2 * hh + 0.6} />}
              {f.pads.map((p) => (
                <Pad key={p.number} pad={p} active={!!routeNet && p.net === routeNet} />
              ))}
              <text className="refdes" x={x} y={y - hh - 0.5}>
                {f.refdes}
              </text>
            </g>
          );
        })}
        {!scene &&
          Object.values(pcb.vias)
          .filter((v) => viaLayers(order, v.from ?? "top", v.to ?? "bottom").some(shown))
          .map((v) => {
          const [x, y] = toView(v.position);
          return (
            <g key={v.id} className={`${highlighted.has(v.id) ? "via finding-hit" : "via"}${netHit(v.net)}`} data-via-id={v.id}>
              <title>{`${viaKind(v.from ?? "top", v.to ?? "bottom")} via ${v.net} (${layerName(v.from ?? "top")}→${layerName(v.to ?? "bottom")})`}</title>
              <circle cx={x} cy={y} r={mm(v.diameter) / 2} />
              <circle className="drill" cx={x} cy={y} r={mm(v.drill) / 2} />
            </g>
          );
        })}
        {draft && <circle className="anchor" cx={toView(draft.anchor)[0]} cy={toView(draft.anchor)[1]} r={0.5} />}
        {drag?.mode === "marquee" && (
          <rect
            className="marquee"
            x={Math.min(toView(drag.from)[0], toView(drag.to)[0])}
            y={Math.min(toView(drag.from)[1], toView(drag.to)[1])}
            width={Math.abs(toView(drag.to)[0] - toView(drag.from)[0])}
            height={Math.abs(toView(drag.to)[1] - toView(drag.from)[1])}
          />
        )}
      </Canvas>
      {outline.length === 0 && tool !== "outline" && (
        <div className="pcb-empty" role="note" aria-label="No board outline">
          <strong>No board outline yet</strong>
          <p className="muted">
            The board edge defines the fabrication area, routing limits and copper zones.
            {Object.keys(pcb.footprints).length > 0 ? " Footprints are placed; the view is framed on them until an outline exists." : " Draw the outline first, then place footprints inside it."}
          </p>
          {edit ? (
            <button type="button" className="primary" onClick={() => choose("outline")}>Draw board outline</button>
          ) : (
            <span className="muted">This view is read-only: open the PCB in an editor to draw the outline.</span>
          )}
        </div>
      )}
    </section>
  );
}

/** Schema v4 drawing objects in the SVG renderer (ADR-0042): keep-out / constraint regions,
 *  cut-out edges, PCB text and dimensions. Decoration only; they are edited through commands. */
function PcbDrawings({ pcb }: { pcb: Snapshot["project"]["pcb"] }) {
  const cls = (layer: string) => `pcb-drawing ${layer}`;
  return (
    <g className="pcb-drawings">
      {Object.values(pcb.regions ?? {}).map((r) => (
        <polygon key={r.id} className={`pcb-region${(r.forbid ?? []).length ? " keepout" : ""}`} data-region-id={r.id} points={poly(r.outline)}>
          <title>{`${r.name || ((r.forbid ?? []).length ? "keep-out" : "constraint region")}${(r.forbid ?? []).length ? `: no ${(r.forbid ?? []).join(", ")}` : ""}`}</title>
        </polygon>
      ))}
      {Object.values(pcb.cutouts ?? {}).map((c) => (
        <polygon key={c.id} className="board-edge cutout-edge" points={poly(c.outline)} />
      ))}
      {Object.values(pcb.texts ?? {}).map((t) => {
        const [x, y] = toView(t.position);
        const turn = t.rotation === "R90" ? -90 : t.rotation === "R180" ? 180 : t.rotation === "R270" ? 90 : 0;
        return (
          <text key={t.id} className={cls(t.layer)} data-text-id={t.id} x={x} y={y} fontSize={mm(t.size)} transform={turn ? `rotate(${turn} ${x} ${y})` : undefined}>
            {t.text}
          </text>
        );
      })}
      {Object.values(pcb.dimensions ?? {}).map((d) => {
        const g = dimensionDrawing(d);
        const [lx, ly] = toView(g.labelAt);
        return (
          <g key={d.id} className={cls(d.layer)} data-dimension-id={d.id}>
            {g.lines.map(([a, b], i) => {
              const [x1, y1] = toView(a);
              const [x2, y2] = toView(b);
              return <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} />;
            })}
            <text x={lx} y={ly} fontSize={mm(g.textSize)} transform={g.labelRotation ? `rotate(-90 ${lx} ${ly})` : undefined}>
              {g.label}
            </text>
          </g>
        );
      })}
    </g>
  );
}
