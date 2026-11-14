// Schematic editor: draws the active sheet from the engine snapshot and turns tool
// clicks into typed engine commands (place, wire, label, move, rotate, delete, edit).
import { useEffect, useRef, useState } from "react";
import { Canvas } from "./Canvas";
import { isTyping, type EditCtx } from "./EditContext";
import { mmToNm, nextRotation, segments, snapPoint } from "./editing";
import type { Id, Point, Snapshot, SymbolInstance } from "./types";
import { boundsOf, fromView, mm, toView } from "./viewport";

interface Props {
  snapshot: Snapshot;
  selected: Id | null;
  onSelect: (symbolId: Id) => void;
  edit?: EditCtx;
  /** Symbols an AI proposal would touch; drawn dashed until the proposal is accepted or rejected. */
  aiPreview?: Id[];
  /** Arms the Place tool with a library part (Parts panel, quick open). */
  armPart?: { id: string; n: number } | null;
}

type Tool = "select" | "place" | "wire" | "connect" | "label" | "move" | "delete";
const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["place", "Place"],
  ["wire", "Wire"],
  ["connect", "Connect pins"],
  ["label", "Label"],
  ["move", "Move"],
  ["delete", "Delete"],
];
const GRID = mmToNm(1.27);
const CAPTURE = mmToNm(1.5);

const hit = (e: { target: EventTarget }, attr: string): string | null =>
  e.target instanceof Element ? (e.target.closest(`[${attr}]`)?.getAttribute(attr) ?? null) : null;

export function SchematicView({ snapshot, selected, onSelect, edit, aiPreview = [], armPart }: Props) {
  const { schematic } = snapshot.project;
  const sheets = Object.values(schematic.sheets);
  const [sheetId, setSheetId] = useState<Id | undefined>(sheets[0]?.id);
  const [tool, setTool] = useState<Tool>("select");
  const [partId, setPartId] = useState("");
  const [labelName, setLabelName] = useState("NET1");
  const [labelKind, setLabelKind] = useState<"net" | "power">("net");
  const [start, setStart] = useState<Point | null>(null);
  const [pin, setPin] = useState<{ symbol: Id; pin: string } | null>(null);
  const [picked, setPicked] = useState<Id | null>(null);
  const [hover, setHover] = useState<Point | null>(null);
  const sheet = sheets.find((s) => s.id === sheetId) ?? sheets[0];
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
  useEffect(() => {
    if (!armPart) return;
    setPartId(armPart.id);
    setStart(null);
    setPin(null);
    setPicked(null);
    setTool("place");
  }, [armPart]);

  // Keyboard shortcuts act on the current selection through the same commands.
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (!edit || isTyping(e.target) || e.ctrlKey || e.metaKey) return;
    const digit = Number(e.key);
    if (digit >= 1 && digit <= TOOLS.length) choose(TOOLS[digit - 1][0]);
    else if (e.key === "Escape") reset();
    else if (e.key.toLowerCase() === "r" && sym) {
      void edit.execute({ type: "RotateSymbol", symbol_id: sym.id, rotation: nextRotation(sym.rotation) });
    } else if (e.key === "Delete" && sym) void edit.execute({ type: "DeleteSymbol", symbol_id: sym.id });
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

  const onPoint = (x: number, y: number, e: React.PointerEvent) => {
    if (!edit || edit.busy || tool === "select") return;
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
          {tool === "label" && (
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
          <span className="muted hint">
            {tool === "select" && "Click a symbol to select · R rotates · Delete removes"}
            {tool === "place" && "Click the sheet to place the part"}
            {tool === "wire" && (start ? "Click to end the wire segment · Esc stops" : "Click a start point (pins snap)")}
            {tool === "connect" && (pin ? "Click the second pin" : "Click the first pin")}
            {tool === "label" && "Click to drop the label"}
            {tool === "move" && (picked ? "Click the new position" : "Click a symbol, then its destination")}
            {tool === "delete" && "Click a symbol, wire or label"}
          </span>
        </div>
      )}
      <Canvas
        label="Schematic canvas"
        bounds={bounds}
        tool={!!edit && tool !== "select"}
        onPoint={edit ? onPoint : undefined}
        onHover={edit ? (x, y) => setHover(fromView(x, y)) : undefined}
      >
        <rect className="sheet" x={0} y={-mm(sheet.height)} width={mm(sheet.width)} height={mm(sheet.height)} />
        {Object.values(schematic.wires)
          .filter((w) => w.sheet_id === sheet.id)
          .map((w) => {
            const [x1, y1] = toView(w.a);
            const [x2, y2] = toView(w.b);
            return (
              <g key={w.id} data-wire-id={w.id}>
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
        {symbols.map((s) => {
          const [x, y] = toView(s.position);
          const hw = mm(s.half_w);
          const hh = mm(s.half_h);
          return (
            <g
              key={s.id}
              data-symbol-id={s.id}
              className={`symbol${s.id === selected ? " selected" : ""}${aiPreview.includes(s.id) ? " ai-preview" : ""}`}
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
              <g key={l.id} data-label-id={l.id} className={`label ${l.kind}`}>
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
