// Schematic editor: draws the active sheet from the engine snapshot and turns tool
// clicks into typed engine commands (place, wire, label, move, rotate, delete, edit).
import { useEffect, useRef, useState } from "react";
import { Canvas, type DragHandler } from "./Canvas";
import { isTyping, type EditCtx } from "./EditContext";
import { mmToNm, segments, snapPoint } from "./editing";
import { Coords, GridControls, GridLayer, useGridConfig } from "./grid";
import type { Command, FlipAxis, Id, Point, Snapshot, SymbolGeom, SymbolInstance } from "./types";
import { boundsOf, fromView, mm, toView } from "./viewport";
import { isActiveEditor, useDocId, useRegisterTools } from "./shell/viewState";
import * as S from "./tools/schematic/selection";
import "./tools/schematic/schematic.css";

interface Props {
  snapshot: Snapshot;
  selected: Id | null;
  onSelect: (symbolId: Id) => void;
  edit?: EditCtx;
  /** Symbols an AI proposal would touch; drawn dashed until the proposal is accepted or rejected. */
  aiPreview?: Id[];
  /** Arms the Place tool with a library part (Parts panel, quick open). */
  armPart?: { id: string; n: number } | null;
  /** Engine sheet id this document shows (from the document tab). */
  sheetId?: Id;
}

type Tool = "select" | "place" | "wire" | "connect" | "label" | "move" | "delete" | "noconnect" | "pinlabel";
const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["place", "Place"],
  ["wire", "Wire"],
  ["connect", "Connect pins"],
  ["label", "Label"],
  ["move", "Move"],
  ["delete", "Delete"],
  ["noconnect", "No connect"],
  ["pinlabel", "Pin label"],
];
/** Tools under which pressing on an object and dragging moves it (instead of panning the view). */
const MOVE_TOOLS: Tool[] = ["select", "place", "move"];
const CAPTURE = mmToNm(1.5);

const hit = (e: { target: EventTarget }, attr: string): string | null =>
  e.target instanceof Element ? (e.target.closest(`[${attr}]`)?.getAttribute(attr) ?? null) : null;

/** The schematic object under the pointer, if any. */
const objectAt = (e: { target: EventTarget }): [S.Kind, Id] | null => {
  const s = hit(e, "data-symbol-id");
  if (s) return ["symbol", s];
  const l = hit(e, "data-label-id");
  if (l) return ["label", l];
  const w = hit(e, "data-wire-id");
  return w ? ["wire", w] : null;
};

type Drag = { mode: "move" | "marquee"; from: Point; to: Point; additive: boolean };

export function SchematicView({ snapshot, selected, onSelect, edit, aiPreview = [], armPart, sheetId: docSheet }: Props) {
  const { schematic } = snapshot.project;
  const sheets = Object.values(schematic.sheets);
  const [sheetId, setSheetId] = useState<Id | undefined>(docSheet ?? sheets[0]?.id);
  useEffect(() => {
    if (docSheet) setSheetId(docSheet);
  }, [docSheet]);
  const [tool, setTool] = useState<Tool>("select");
  const [partId, setPartId] = useState("");
  const [labelName, setLabelName] = useState("NET1");
  const [labelKind, setLabelKind] = useState<"net" | "power">("net");
  const [start, setStart] = useState<Point | null>(null);
  const [pin, setPin] = useState<{ symbol: Id; pin: string } | null>(null);
  const [picked, setPicked] = useState<Id | null>(null);
  const [hover, setHover] = useState<Point | null>(null);
  const [selRaw, setSel] = useState<S.Selection>(S.EMPTY);
  const [drag, setDrag] = useState<Drag | null>(null);
  const clip = useRef<S.Clip | null>(null);
  const [grid, setGrid] = useGridConfig("schematic");
  // Grid step in nm; with snap off, points and moves land on whole nanometres.
  const step = mmToNm(grid.spacingMm);
  const GRID = grid.snap ? step : 1;
  const snapDelta = (from: Point, to: Point): Point => ({
    x: Math.round((to.x - from.x) / GRID) * GRID,
    y: Math.round((to.y - from.y) / GRID) * GRID,
  });
  // Object selected by the current press, so the click that ends it does not toggle it back.
  const pressed = useRef<Id | null>(null);
  const sheet = sheets.find((s) => s.id === sheetId) ?? sheets[0];
  // The canvas selection, or the symbol selected elsewhere (explorer, findings).
  const pruned = S.prune(selRaw, snapshot);
  const sel = S.isEmpty(pruned) && selected && schematic.symbols[selected] ? { ...S.EMPTY, symbols: [selected] } : pruned;
  const symbols = snapshot.geometry.symbols.filter((s) => s.sheet_id === sheet?.id);
  const pins = symbols.flatMap((s) => s.pins.map((p) => ({ symbol: s.id, number: p.number, at: p.position })));
  const sym = selected ? schematic.symbols[selected] : undefined;
  const part = edit?.library.find((c) => c.id === partId) ?? edit?.library[0];

  const reset = () => {
    setStart(null);
    setPin(null);
    setPicked(null);
  };
  const choose = (t: Tool) => {
    reset();
    setTool(t);
  };
  const docId = useDocId();
  useRegisterTools(docId, (t) => {
    const hit = TOOLS.find(([id]) => id === t);
    if (hit) choose(hit[0]);
    return !!hit;
  });
  useEffect(() => {
    if (!armPart) return;
    setPartId(armPart.id);
    setStart(null);
    setPin(null);
    setPicked(null);
    setTool("place");
  }, [armPart]);

  /** Submit a multi-object edit as one engine command (a `Batch` when there are several). */
  const submit = (commands: Command[], label: string) => {
    const c = S.asBatch(commands, label);
    if (c && edit) void edit.execute(c);
  };
  const objects = (n: number) => `${n} object${n === 1 ? "" : "s"}`;

  // Selection actions, shared by the keyboard shortcuts and the selection bar buttons. Each is
  // one engine command (a Batch for several objects).
  const actions = sheet && {
    rotate: () => submit(S.rotateCommands(snapshot, sheet.id, sel), `Rotate ${objects(sel.symbols.length)}`),
    flip: (axis: FlipAxis) => submit(S.flipCommands(snapshot, sheet.id, sel, axis), `Flip ${objects(sel.symbols.length)}`),
    remove: () => {
      submit(S.deleteCommands(sel), `Delete ${objects(S.count(sel))}`);
      setSel(S.EMPTY);
    },
    copy: () => {
      const c = S.copy(snapshot, sel);
      if (c) {
        clip.current = c;
        edit?.notify(`Copied ${objects(S.count(sel))}.`);
      }
      return c;
    },
    paste: () => clip.current && submit(S.pasteCommands(clip.current, sheet.id, S.pasteOffset(clip.current, hover, step)), "Paste"),
    /** Copy-and-paste in one step, four grid steps up and right of the original. */
    duplicate: () => {
      const c = S.copy(snapshot, sel);
      if (c) submit(S.pasteCommands(c, sheet.id, S.pasteOffset(c, null, step)), `Duplicate ${objects(S.count(sel))}`);
    },
  };

  // Keyboard shortcuts act on the current selection through the same commands. Undo/redo
  // (Ctrl+Z, Ctrl+Shift+Z) are workspace-wide and handled by the workspace.
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (!edit || !sheet || !actions || isTyping(e.target) || e.altKey || !isActiveEditor(docId)) return;
    const k = e.key.toLowerCase();
    if (e.ctrlKey || e.metaKey) {
      if (e.shiftKey) return;
      if (k === "c") actions.copy();
      else if (k === "x") {
        if (actions.copy()) actions.remove();
      } else if (k === "v" && clip.current) {
        e.preventDefault();
        actions.paste();
      } else if (k === "d") {
        e.preventDefault();
        actions.duplicate();
      } else if (k === "a") {
        e.preventDefault();
        setSel(S.marquee(snapshot, sheet.id, S.rectOf({ x: -1e15, y: -1e15 }, { x: 1e15, y: 1e15 })));
      }
      return;
    }
    const digit = Number(e.key);
    if (digit >= 1 && digit <= TOOLS.length) choose(TOOLS[digit - 1][0]);
    else if (e.key === "Escape") {
      reset();
      setDrag(null);
      setSel(S.EMPTY);
      setTool("select");
    } else if (k === "w") choose("wire");
    else if (k === "r") actions.rotate();
    else if (k === "x") actions.flip("horizontal");
    else if (k === "y") actions.flip("vertical");
    else if (e.key === "Delete" || e.key === "Backspace") actions.remove();
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  if (!sheet) return <p className="empty">This project has no schematic sheet.</p>;

  const pinAt = new Map(
    snapshot.geometry.symbols.flatMap((s) => s.pins.map((p) => [`${s.id}/${p.number}`, p.position] as const)),
  );
  const bounds = boundsOf([
    [0, 0],
    [mm(sheet.width), -mm(sheet.height)],
  ])!;

  // Select tool: a click selects exactly the object under the pointer (Shift adds, Ctrl/Cmd
  // toggles). Pressing on an object and dragging moves the selection (one Batch, wires
  // rubber-banded) under the Select, Place and Move tools; pressing an unselected object first
  // makes it the selection, so only that object moves. A drag on empty sheet draws a marquee
  // (Select tool; Shift/Ctrl add) or pans (other tools). Middle-button drag always pans.
  const selectClick = (e: React.PointerEvent) => {
    const o = objectAt(e);
    const justPicked = pressed.current;
    pressed.current = null;
    const mode = S.pickMode(e);
    if (!o) return setSel(mode === "replace" ? S.EMPTY : sel);
    if (justPicked === o[1]) return;
    setSel(S.pick(sel, o[0], o[1], mode));
    if (o[0] === "symbol") onSelect(o[1]);
  };
  const dragHandler: DragHandler = {
    start: (x, y, e) => {
      if (!edit || edit.busy) return false;
      const at = fromView(x, y);
      const o = objectAt(e);
      pressed.current = null;
      if (!o && tool !== "select") return false;
      if (o && !S.has(sel, o[0], o[1])) {
        pressed.current = o[1];
        setSel(S.pick(sel, o[0], o[1], S.pickMode(e) === "replace" ? "replace" : "add"));
        if (o[0] === "symbol") onSelect(o[1]);
      }
      setDrag({ mode: o ? "move" : "marquee", from: at, to: at, additive: S.pickMode(e) !== "replace" });
      return true;
    },
    move: (x, y) => setDrag((d) => d && { ...d, to: fromView(x, y) }),
    end: (x, y) => {
      const d = drag;
      setDrag(null);
      if (!d) return;
      const to = fromView(x, y);
      if (d.mode === "marquee") {
        const found = S.marquee(snapshot, sheet.id, S.rectOf(d.from, to));
        setSel(d.additive ? S.union(sel, found) : found);
        return;
      }
      submit(S.moveCommands(snapshot, sheet.id, sel, snapDelta(d.from, to)), `Move ${objects(S.count(sel))}`);
    },
    cancel: () => setDrag(null),
  };
  // Live preview of a move: the commands the drop would submit, drawn over the sheet.
  const movePreview = drag?.mode === "move" ? S.moveCommands(snapshot, sheet.id, sel, snapDelta(drag.from, drag.to)) : [];
  const dragDelta = drag?.mode === "move" ? snapDelta(drag.from, drag.to) : null;
  const labelShift = (id: Id): string | undefined => {
    const c = movePreview.find((m) => m.type === "MoveNetLabel" && m.label_id === id);
    if (!c || c.type !== "MoveNetLabel") return undefined;
    const l = schematic.labels[id];
    return `translate(${mm(c.position.x - l.position.x)} ${-mm(c.position.y - l.position.y)})`;
  };

  const onPoint = (x: number, y: number, e: React.PointerEvent) => {
    if (!edit || edit.busy) return;
    if (tool === "select") return selectClick(e);
    const snap = snapPoint(
      fromView(x, y),
      GRID,
      pins.map((p) => p.at),
      CAPTURE,
    );
    const pinHit = snap.hit === null ? null : pins[snap.hit];
    const symId = hit(e, "data-symbol-id");
    const wireId = hit(e, "data-wire-id");
    const labelId = hit(e, "data-label-id");
    switch (tool) {
      case "place": {
        if (!part) return edit.notify("No library part available.");
        void edit.execute({
          type: "PlaceSymbol",
          sheet_id: sheet.id,
          component_id: part.id,
          position: snap.point,
          rotation: "R0",
          refdes: null,
          value: null,
        });
        return;
      }
      case "wire": {
        if (!start) return setStart(snap.point);
        const legs = segments(start, snap.point, false);
        setStart(snap.point);
        if (legs.length)
          void edit.execute(legs.map(([a, b]) => ({ type: "AddWire" as const, sheet_id: sheet.id, a, b })));
        return;
      }
      case "connect": {
        if (!pinHit) return edit.notify("Click a pin.");
        if (!pin) return setPin({ symbol: pinHit.symbol, pin: pinHit.number });
        const a = pin;
        setPin(null);
        void edit.execute({
          type: "ConnectPins",
          a_symbol: a.symbol,
          a_pin: a.pin,
          b_symbol: pinHit.symbol,
          b_pin: pinHit.number,
        });
        return;
      }
      case "label": {
        const name = labelName.trim();
        if (!name) return edit.notify("Enter a net name first.");
        void edit.execute({
          type: "AddNetLabel",
          sheet_id: sheet.id,
          name,
          position: snap.point,
          kind: labelKind,
        });
        return;
      }
      case "move": {
        if (!picked) {
          if (symId) {
            setPicked(symId);
            onSelect(symId);
          }
          return;
        }
        const id = picked;
        setPicked(null);
        void edit.execute({ type: "MoveSymbol", symbol_id: id, position: snap.point });
        return;
      }
      case "noconnect": {
        if (!pinHit) return edit.notify("Click a pin to mark it as intentionally unconnected.");
        void edit.execute({ type: "SetNoConnect", symbol_id: pinHit.symbol, pin: pinHit.number });
        return;
      }
      case "pinlabel": {
        if (!pinHit) return edit.notify("Click a pin to label its net.");
        const name = labelName.trim();
        if (!name) return edit.notify("Enter a net name first.");
        void edit.execute({ type: "LabelPin", symbol_id: pinHit.symbol, pin: pinHit.number, name, kind: labelKind });
        return;
      }
      case "delete": {
        if (symId) void edit.execute({ type: "DeleteSymbol", symbol_id: symId });
        else if (labelId) void edit.execute({ type: "DeleteNetLabel", label_id: labelId });
        else if (wireId) void edit.execute({ type: "DeleteWire", wire_id: wireId });
        return;
      }
    }
  };

  const preview =
    hover && tool === "wire" && start
      ? segments(start, snapPoint(hover, GRID, pins.map((p) => p.at), CAPTURE).point, false)
      : [];

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
        {edit ? (
          <div className="tools" role="toolbar" aria-label="Schematic tools">
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
          <span className="muted">read-only · wheel zoom · drag pan · double-click fit</span>
        )}
        <GridControls kind="schematic" cfg={grid} onChange={setGrid} />
        <Coords at={hover} />
      </header>
      {edit && (
        <div className="tool-options">
          {tool === "place" && (
            <select aria-label="Part" value={part?.id ?? ""} onChange={(e) => setPartId(e.target.value)}>
              {edit.library.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name} ({c.id})
                </option>
              ))}
            </select>
          )}
          {(tool === "label" || tool === "pinlabel") && (
            <>
              <input aria-label="Net name" value={labelName} onChange={(e) => setLabelName(e.target.value)} />
              <select aria-label="Label kind" value={labelKind} onChange={(e) => setLabelKind(e.target.value as "net" | "power")}>
                <option value="net">net label</option>
                <option value="power">power port</option>
              </select>
            </>
          )}
          {sym && (
            <SymbolFields key={`${sym.id}|${sym.refdes}|${sym.value}`} sym={sym} edit={edit} />
          )}
          {actions && !S.isEmpty(sel) && (
            <span className="selection-bar" role="group" aria-label="Selection">
              <button type="button" title="Rotate 90° (R)" disabled={!sel.symbols.length} onClick={actions.rotate}>
                Rotate
              </button>
              <button type="button" title="Flip left/right (X)" disabled={!sel.symbols.length} onClick={() => actions.flip("horizontal")}>
                Flip H
              </button>
              <button type="button" title="Flip top/bottom (Y)" disabled={!sel.symbols.length} onClick={() => actions.flip("vertical")}>
                Flip V
              </button>
              <button type="button" title="Duplicate (Ctrl+D)" onClick={actions.duplicate}>
                Duplicate
              </button>
              <button type="button" title="Delete (Del)" onClick={actions.remove}>
                Delete
              </button>
            </span>
          )}
          <span className="muted hint">
            {tool === "select" &&
              (S.isEmpty(sel)
                ? "Click or drag a box to select (Shift adds, Ctrl toggles) · W wire · Ctrl+V paste · middle-drag pans"
                : `${objects(S.count(sel))} selected · drag to move · R rotate · X/Y flip · Del delete · Ctrl+C/X/V/D copy/cut/paste/duplicate · Esc clear`)}
            {tool === "place" && "Click the sheet to place the part · drag a part to move it"}
            {tool === "wire" && (start ? "Click to end the wire segment · Esc stops" : "Click a start point (pins snap)")}
            {tool === "connect" && (pin ? "Click the second pin" : "Click the first pin")}
            {tool === "label" && "Click to drop the label"}
            {tool === "move" && (picked ? "Click the new position" : "Drag a part, or click a symbol then its destination")}
            {tool === "delete" && "Click a symbol, wire or label"}
            {tool === "noconnect" && "Click a pin to mark it intentionally unconnected (the ERC-001 remedy)"}
            {tool === "pinlabel" && "Click a pin to put the net label on it"}
          </span>
        </div>
      )}
      <Canvas
        label="Schematic canvas"
        bounds={bounds}
        tool={!!edit && tool !== "select"}
        onPoint={edit ? onPoint : undefined}
        onHover={(x, y) => setHover(fromView(x, y))}
        onDrag={edit && MOVE_TOOLS.includes(tool) ? dragHandler : undefined}
        underlay={(vb) => (
          <>
            <rect className="sheet" x={0} y={-mm(sheet.height)} width={mm(sheet.width)} height={mm(sheet.height)} />
            <GridLayer vb={vb} kind="schematic" cfg={grid} />
          </>
        )}
      >
        {Object.values(schematic.wires)
          .filter((w) => w.sheet_id === sheet.id)
          .map((w) => {
            const [x1, y1] = toView(w.a);
            const [x2, y2] = toView(w.b);
            return (
              <g key={w.id} data-wire-id={w.id} className={S.has(sel, "wire", w.id) ? "selected" : undefined}>
                <line className="wire" x1={x1} y1={y1} x2={x2} y2={y2} />
                <line className="hit" x1={x1} y1={y1} x2={x2} y2={y2} />
              </g>
            );
          })}
        {preview.map(([a, b], i) => {
          const [x1, y1] = toView(a);
          const [x2, y2] = toView(b);
          return <line key={i} className="wire preview" x1={x1} y1={y1} x2={x2} y2={y2} />;
        })}
        {movePreview.flatMap((c) => {
          if (c.type !== "MoveWire") return [];
          const [x1, y1] = toView(c.a);
          const [x2, y2] = toView(c.b);
          return [<line key={`mv-${c.wire_id}`} className="wire preview" x1={x1} y1={y1} x2={x2} y2={y2} />];
        })}
        {symbols.map((s) => {
          const [x, y] = toView(s.position);
          const hw = mm(s.half_w);
          const hh = mm(s.half_h);
          return (
            <g
              key={s.id}
              data-symbol-id={s.id}
              transform={dragDelta && S.has(sel, "symbol", s.id) ? `translate(${mm(dragDelta.x)} ${-mm(dragDelta.y)})` : undefined}
              className={`symbol${S.has(sel, "symbol", s.id) ? " selected" : ""}${aiPreview.includes(s.id) ? " ai-preview" : ""}`}
              onClick={() => onSelect(s.id)}
            >
              <title>{`${s.refdes} ${s.value}`}</title>
              {s.pins.map((p) => {
                const [px, py] = toView(p.position);
                // Pin stub from the body edge to the connection point.
                const ex = Math.min(Math.max(px, x - hw), x + hw);
                const ey = Math.min(Math.max(py, y - hh), y + hh);
                const active = pin?.symbol === s.id && pin.pin === p.number;
                return (
                  <g key={p.number}>
                    <line className="pin" x1={ex} y1={ey} x2={px} y2={py} />
                    <circle className={active ? "pin-end active" : "pin-end"} cx={px} cy={py} r={active ? 0.8 : 0.4} />
                  </g>
                );
              })}
              <rect className="body" x={x - hw} y={y - hh} width={2 * hw} height={2 * hh} />
              {S.has(sel, "symbol", s.id) && <SelectionBox s={s} />}
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
              <g
                key={l.id}
                data-label-id={l.id}
                className={`label ${l.kind}${S.has(sel, "label", l.id) ? " selected" : ""}`}
                transform={labelShift(l.id)}
              >
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
        {tool === "place" && hover && part && <PlaceGhost snapshot={snapshot} componentId={part.id} name={part.name} at={snapPoint(hover, GRID, [], 0).point} />}
        {drag?.mode === "marquee" && <Marquee from={drag.from} to={drag.to} />}
        {start && <circle className="anchor" cx={toView(start)[0]} cy={toView(start)[1]} r={0.6} />}
      </Canvas>
    </section>
  );
}

/** Value / reference editors for the selected symbol; each commit is one engine command. */
function SymbolFields({ sym, edit }: { sym: SymbolInstance; edit: EditCtx }) {
  const [value, setValue] = useState(sym.value);
  const [refdes, setRefdes] = useState(sym.refdes);
  return (
    <>
      <label>
        Ref
        <input
          aria-label="Reference"
          size={5}
          value={refdes}
          onChange={(e) => setRefdes(e.target.value)}
          onBlur={() => refdes !== sym.refdes && refdes.trim() && void edit.execute({ type: "SetRefdes", symbol_id: sym.id, refdes: refdes.trim() })}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
      </label>
      <label>
        Value
        <input
          aria-label="Value"
          size={8}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onBlur={() => value !== sym.value && void edit.execute({ type: "SetSymbolValue", symbol_id: sym.id, value })}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
      </label>
    </>
  );
}

/** Dashed boundary around a selected symbol's body and pins. */
function SelectionBox({ s }: { s: SymbolGeom }) {
  const pts: [number, number][] = [
    [mm(s.position.x - s.half_w), -mm(s.position.y - s.half_h)],
    [mm(s.position.x + s.half_w), -mm(s.position.y + s.half_h)],
    ...s.pins.map((p) => toView(p.position)),
  ];
  const b = boundsOf(pts)!;
  const pad = 0.8;
  return <rect className="sel-box" x={b.minX - pad} y={b.minY - pad} width={b.maxX - b.minX + 2 * pad} height={b.maxY - b.minY + 2 * pad} />;
}

/**
 * Where the Place tool will drop the part: its body outline at the snapped cursor (when the
 * definition or an existing instance gives the size) and a crosshair on the origin.
 */
function PlaceGhost({ snapshot, componentId, name, at }: { snapshot: Snapshot; componentId: string; name: string; at: Point }) {
  const def = snapshot.project.library?.[componentId]?.symbol as { body_half_w?: number; body_half_h?: number } | undefined;
  const twin = Object.values(snapshot.project.schematic.symbols).find((s) => s.component_id === componentId && (s.rotation === "R0" || s.rotation === "R180"));
  const geom = twin && snapshot.geometry.symbols.find((g) => g.id === twin.id);
  const hw = def?.body_half_w ?? geom?.half_w;
  const hh = def?.body_half_h ?? geom?.half_h;
  const [x, y] = toView(at);
  return (
    <g className="place-ghost" data-place-ghost={componentId}>
      {hw !== undefined && hh !== undefined && <rect x={x - mm(hw)} y={y - mm(hh)} width={2 * mm(hw)} height={2 * mm(hh)} />}
      <path d={`M${x - 1.27} ${y}H${x + 1.27}M${x} ${y - 1.27}V${y + 1.27}`} />
      <text x={x} y={y - (hh !== undefined ? mm(hh) : 1.27) - 1}>{name}</text>
    </g>
  );
}

/** Marquee rectangle while dragging a selection box. */
function Marquee({ from, to }: { from: Point; to: Point }) {
  const [x1, y1] = toView(from);
  const [x2, y2] = toView(to);
  return <rect className="marquee" x={Math.min(x1, x2)} y={Math.min(y1, y2)} width={Math.abs(x2 - x1)} height={Math.abs(y2 - y1)} />;
}
