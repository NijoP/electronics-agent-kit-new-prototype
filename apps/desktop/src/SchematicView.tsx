// Schematic editor: draws the active sheet from the engine snapshot and turns tool
// clicks into typed engine commands (place, wire, label, move, rotate, delete, edit).
// Slice B3 adds the editing set: clipboard across sheets, arrange, array, find/replace, find
// similar, net highlight, the selection filter and a canvas context menu generated from the
// action registry. Every multi-object edit is one Batch: one revision, one undo step.
import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { api } from "./api";
import type { ContextItem } from "./actions/registry";
import type { CanvasTarget } from "./actions/types";
import { CanvasContextMenu } from "./shell/CanvasContextMenu";
import { ArrayPanel } from "./tools/schematic/ArrayPanel";
import { clipboard, setClipboard } from "./tools/schematic/clipboard";
import { useSelectFilter } from "./tools/schematic/filter";
import { FindBar } from "./tools/schematic/FindBar";
import { takeFindPick, useFindPickSeq } from "./shell/findResults";
import { netAt, netMembers, sheetGroups, type NetHighlight } from "./tools/schematic/nets";
import { Canvas, type DragHandler, type DropHandler } from "./Canvas";
import { getDraggedComponent, setDraggedComponent, EAK_COMPONENT_MIME } from "./dragState";
import { isTyping, type EditCtx } from "./EditContext";
import { mmToNm, snapPoint } from "./editing";
import { Coords, GridLayer, snapStepNm, useGridConfig } from "./grid";
import type { Command, FlipAxis, Id, Point, Rotation, Snapshot, SymbolGeom, SymbolInstance } from "./types";
import { powerPortShape, powerStyleOf, symbolDrawing, type DrawableSymbol } from "./render/symbolDraw";
import { SymbolSvg } from "./tools/schematic/SymbolSvg";
import { boundsOf, fromView, mm, toView } from "./viewport";
import { isActiveEditor, useDocId, useRegisterTools } from "./shell/viewState";
import { ToolOptionFields } from "./shell/Inspector";
import { NO_SELECTION, ROTATION_CHOICES, useHasInspector, usePublishEditorContext, type EditorOp, type OptionField } from "./shell/inspectorModel";
import { sheetNoConnects, toggleNoConnect } from "./tools/schematic/noConnect";
import * as S from "./tools/schematic/selection";
import { junctionPoints } from "./tools/schematic/junctions";
import {
  ENTRY_DIR_LABELS,
  GRAPHIC_TOOLS,
  LABEL_SCOPE,
  WIRE_MODES,
  busAt,
  busEntryWirePoint,
  constrainAxis,
  graphicFrom,
  nextWireMode,
  powerPortCommand,
  wireLegs,
  type GraphicTool,
  type WireMode,
} from "./tools/schematic/placeTools";
import { GraphicShape, SheetGraphics } from "./tools/schematic/SheetGraphics";
import { powerPortProblem, usePowerPorts } from "./settings/powerPorts";
import { reportCanvas } from "./shell/viewState";
import "./tools/schematic/schematic.css";
import { sheetsInOrder } from "./shell/sheets";

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
  /** Canvas context menu for a right-click target: the action registry's entries for it (slice B3). */
  contextMenu?: (target: CanvasTarget) => ContextItem[];
}

type Tool =
  | "select"
  | "place"
  | "wire"
  | "connect"
  | "label"
  | "move"
  | "delete"
  | "noconnect"
  | "pinlabel"
  | "bus"
  | "busentry"
  | "power"
  | "netport"
  | "offpage"
  | GraphicTool;
/** The editor's own tool buttons; digit keys 1-9 select them in this order. */
export const TOOLS: [Tool, string][] = [
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
/**
 * Place tools of slice B4, armed from the main toolbar's split buttons and the Place menu (no
 * digit key): bus and bus entry, power ports from the Settings list, the sheet-local net port and
 * the off-page connector, and the sheet graphics and text.
 */
export const PLACE_TOOLS: [Tool, string][] = [
  ["bus", "Bus"],
  ["busentry", "Bus entry"],
  ["power", "Power port"],
  ["netport", "Net port"],
  ["offpage", "Off-page connector"],
  ["line", "Line"],
  ["rect", "Rectangle"],
  ["circle", "Circle"],
  ["text", "Text"],
];
const ALL_TOOLS = [...TOOLS, ...PLACE_TOOLS];
/** Tools that draw a leg from the last clicked point (corner modes, Shift lock). */
const LEG_TOOLS: Tool[] = ["wire", "bus"];
const isGraphicTool = (t: Tool): t is GraphicTool => (GRAPHIC_TOOLS as readonly string[]).includes(t);
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

/** Bus, bus entry or sheet drawing under the pointer (Delete tool). */
const drawnAt = (e: { target: EventTarget }): Command | null => {
  const be = hit(e, "data-bus-entry-id");
  if (be) return { type: "DeleteBusEntry", entry_id: be };
  const b = hit(e, "data-bus-id");
  if (b) return { type: "DeleteBus", bus_id: b };
  const g = hit(e, "data-graphic-id");
  return g ? { type: "DeleteSheetGraphic", graphic_id: g } : null;
};

type Drag = { mode: "move" | "marquee"; from: Point; to: Point; additive: boolean };

/** The one selected object, if exactly one is selected. */
const single = (s: S.Selection): [S.Kind, Id] | null =>
  S.count(s) !== 1 ? null : s.symbols.length ? ["symbol", s.symbols[0]] : s.wires.length ? ["wire", s.wires[0]] : ["label", s.labels[0]];

export function SchematicView({ snapshot, selected, onSelect, edit, aiPreview = [], armPart, sheetId: docSheet, contextMenu }: Props) {
  const { schematic } = snapshot.project;
  const sheets = sheetsInOrder(schematic);
  const [sheetId, setSheetId] = useState<Id | undefined>(docSheet ?? sheets[0]?.id);
  useEffect(() => {
    if (docSheet) setSheetId(docSheet);
  }, [docSheet]);
  const [tool, setTool] = useState<Tool>("select");
  const [partId, setPartId] = useState("");
  const [placeRotation, setPlaceRotation] = useState<Rotation>("R0");
  const [externalDrag, setExternalDrag] = useState<{ componentId: string; name: string; at: Point } | null>(null);
  const [hi, setHi] = useState<NetHighlight | null>(null);
  const [finding, setFinding] = useState(false);
  const findPick = useFindPickSeq();
  const [arraying, setArraying] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; target: CanvasTarget; label: string } | null>(null);
  const filter = useSelectFilter();
  const [labelName, setLabelName] = useState("NET1");
  const [labelKind, setLabelKind] = useState<"net" | "power">("net");
  const [start, setStart] = useState<Point | null>(null);
  const [pin, setPin] = useState<{ symbol: Id; pin: string } | null>(null);
  const [picked, setPicked] = useState<Id | null>(null);
  const [hover, setHover] = useState<Point | null>(null);
  const [selRaw, setSel] = useState<S.Selection>(S.EMPTY);
  const [drag, setDrag] = useState<Drag | null>(null);
  // B4 place tools: leg mode and bend of wires and buses, the Shift axis lock, the bus being drawn,
  // the bus entry direction, the text tool's text and the power-port list entry armed.
  const [wireMode, setWireMode] = useState<WireMode>("90");
  const [bendFlip, setBendFlip] = useState(false);
  const [shiftDown, setShiftDown] = useState(false);
  const [busName, setBusName] = useState("D[0..7]");
  const [busPts, setBusPts] = useState<Point[]>([]);
  const [entryDir, setEntryDir] = useState(0);
  const [gfxText, setGfxText] = useState("Note");
  const [powerIdx, setPowerIdx] = useState(0);
  const ports = usePowerPorts();
  const port = ports[powerIdx];
  const [grid] = useGridConfig("schematic");
  // Grid step in nm; with snap off, points and moves land on whole nanometres.
  const step = mmToNm(grid.spacingMm);
  const GRID = snapStepNm(grid);
  const snapDelta = (from: Point, to: Point): Point => ({
    x: Math.round((to.x - from.x) / GRID) * GRID,
    y: Math.round((to.y - from.y) / GRID) * GRID,
  });

  const ROTATIONS: Rotation[] = ["R0", "R90", "R180", "R270"];
  const nextRotation = (r: Rotation): Rotation => {
    const idx = ROTATIONS.indexOf(r);
    return ROTATIONS[(idx + 1) % ROTATIONS.length];
  };

  // Object selected by the current press, so the click that ends it does not toggle it back.
  const pressed = useRef<Id | null>(null);
  // The last press began on an object (Place tool: a click there never places).
  const downOnObject = useRef(false);
  const sheet = sheets.find((s) => s.id === sheetId) ?? sheets[0];
  // Find Results (slice E4): a hit picked in the bottom dock on this sheet becomes the selection.
  useEffect(() => {
    if (!sheet) return;
    const h = takeFindPick(sheet.id);
    if (!h) return;
    setTool("select");
    setSel({ ...S.EMPTY, symbols: h.kind === "symbol" ? [h.id] : [], labels: h.kind === "label" ? [h.id] : [] });
  }, [findPick, sheet?.id]);
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
    setBusPts([]);
  };
  const choose = (t: Tool) => {
    reset();
    setExternalDrag(null);
    setTool(t);
  };
  const docId = useDocId();
  useRegisterTools(docId, (t) => {
    // "power:<n>" arms the Power port tool with entry n of the Settings list.
    const m = /^power:(\d+)$/.exec(t);
    if (m) {
      setPowerIdx(Number(m[1]));
      choose("power");
      return true;
    }
    const hit = ALL_TOOLS.find(([id]) => id === t);
    if (hit) choose(hit[0]);
    return !!hit;
  });
  // Shift held: the next wire or bus point keeps to the dominant axis (EasyEDA behaviour).
  useEffect(() => {
    const down = (e: KeyboardEvent) => e.key === "Shift" && setShiftDown(true);
    const up = (e: KeyboardEvent) => e.key === "Shift" && setShiftDown(false);
    const blur = () => setShiftDown(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", blur);
    };
  }, []);
  // The canvas strip's dX / dY count from the last committed wire or bus point, so they show the
  // leg being drawn (the strip reads view coordinates: millimetres, Y down).
  const legFrom = tool === "bus" ? (busPts[busPts.length - 1] ?? null) : tool === "wire" || isGraphicTool(tool) ? start : null;
  useEffect(() => {
    if (legFrom) reportCanvas(docId, { anchor: { x: mm(legFrom.x), y: -mm(legFrom.y) } });
  }, [docId, legFrom?.x, legFrom?.y]);
  useEffect(() => {
    if (!armPart) return;
    setPartId(armPart.id);
    setStart(null);
    setPin(null);
    setPicked(null);
    setPlaceRotation("R0");
    setTool("place");
  }, [armPart]);

  /** Submit a multi-object edit as one engine command (a `Batch` when there are several). */
  const submit = (commands: Command[], label: string) => {
    const c = S.asBatch(commands, label);
    if (c && edit) void edit.execute(c);
  };
  const objects = (n: number) => `${n} object${n === 1 ? "" : "s"}`;
  /** Bus tool: send the vertices clicked so far as one AddBus. */
  const finishBus = () => {
    const pts = busPts;
    setBusPts([]);
    if (!sheet || !edit || pts.length < 2) return;
    const name = busName.trim();
    if (!name) return edit.notify("Enter a bus name first.");
    void edit.execute({ type: "AddBus", sheet_id: sheet.id, name, points: pts });
  };

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
        setClipboard(c);
        edit?.notify(`Copied ${objects(S.count(sel))}.`);
      }
      return c;
    },
    /** Copy, then delete the originals (one command, a Batch for several). */
    cut: () => {
      const c = S.copy(snapshot, sel);
      if (!c) return;
      setClipboard(c);
      edit?.notify(`Cut ${objects(S.count(sel))}.`);
      submit(S.deleteCommands(sel), `Cut ${objects(S.count(sel))}`);
      setSel(S.EMPTY);
    },
    /** Pastes the window's clipboard (copied on any sheet) at the pointer: new objects, nets by label name only. */
    paste: () => {
      const c = clipboard();
      if (c) submit(S.pasteCommands(c, sheet.id, S.pasteOffset(c, hover, step)), "Paste");
    },
    /** Copy-and-paste in one step, four grid steps up and right of the original. */
    duplicate: () => {
      const c = S.copy(snapshot, sel);
      if (c) submit(S.pasteCommands(c, sheet.id, S.pasteOffset(c, null, step)), `Duplicate ${objects(S.count(sel))}`);
    },
    selectAll: () => setSel(S.filtered(S.marquee(snapshot, sheet.id, S.rectOf({ x: -1e15, y: -1e15 }, { x: 1e15, y: 1e15 })), filter)),
    similar: () => {
      const o = single(sel);
      if (!o) return;
      const found = S.filtered(S.similar(snapshot, sheet.id, o[0], o[1]), filter);
      setSel(found);
      edit?.notify(`Find similar: ${objects(S.count(found))} selected on this sheet.`);
    },
    align: (how: S.Align) => {
      const c = S.alignCommands(snapshot, sheet.id, sel, how);
      if (c.length) submit(c, `Align ${objects(sel.symbols.length)}`);
      else edit?.notify("The selected parts are already aligned.");
    },
    distribute: (axis: "x" | "y") => {
      const c = S.distributeCommands(snapshot, sheet.id, sel, axis);
      if (c.length) submit(c, `Distribute ${objects(sel.symbols.length)}`);
      else edit?.notify("The selected parts are already evenly spaced.");
    },
    array: (cols: number, rows: number, stepXmm: number, stepYmm: number) => {
      const c = S.copy(snapshot, sel);
      if (!c) return;
      setArraying(false);
      submit(S.arrayCommands(c, sheet.id, cols, rows, { x: mmToNm(stepXmm), y: mmToNm(stepYmm) }), `Array ${cols}×${rows}: ${cols * rows - 1} copies`);
    },
    /** Highlights the net of a wire or label: the engine netlist names it, the sheet's drawing shows it. */
    highlight: (id: Id) => {
      const groups = sheetGroups(snapshot, sheet.id);
      const show = (rows: { name: string; pins: string[]; label_names: string[] }[] | null, note: string) => {
        const h = netAt(groups, rows, id);
        if (!h) return edit?.notify("Select a wire or net label to highlight its net.");
        setHi(h);
        const m = netMembers(groups, h);
        edit?.notify(`${h.name ? `Net ${h.name}` : "An unnamed net"} highlighted: ${m.wires.size} wire(s), ${m.pinKeys.size} pin(s), ${m.labels.size} label(s) on this sheet${note}.`);
      };
      api.netlist().then(
        (rows) => show(rows, ""),
        () => show(null, " (engine netlist unavailable: only what is drawn connected here)"),
      );
    },
  };

  // B3: the operations the action registry runs on this editor (menus, palette, canvas context
  // menu). Each state is true or the reason it cannot run; published with the editor context.
  const idle = (): true | string => (edit?.busy ? "An engine command is running" : true);
  const needSel = (): true | string => (S.isEmpty(sel) ? "Select something first" : idle());
  const needParts = (n: 1 | 2 | 3): true | string => (sel.symbols.length >= n ? idle() : n === 1 ? "Select a part first" : `Select ${n === 2 ? "two" : "three"} or more parts first`);
  const netSeed = sel.symbols.length === 0 && S.count(sel) === 1 ? (sel.wires[0] ?? sel.labels[0]) : null;
  // ADR-0045: Group / Ungroup are engine commands, one revision and one undo step each.
  const groupCmd = S.groupCommand(snapshot, sel);
  const ungroupCmds = S.ungroupCommands(snapshot, sel);
  const ops: Record<string, EditorOp> | undefined =
    edit && actions
      ? {
          rotate: { state: needParts(1), run: actions.rotate },
          "flip-h": { state: needParts(1), run: () => actions.flip("horizontal") },
          "flip-v": { state: needParts(1), run: () => actions.flip("vertical") },
          delete: { state: needSel(), run: () => window.confirm(`Delete ${objects(S.count(sel))}? This is a destructive change.`) && actions.remove() },
          cut: { state: needSel(), run: actions.cut },
          copy: { state: S.isEmpty(sel) ? "Select something first" : true, run: () => void actions.copy() },
          paste: { state: clipboard() ? idle() : "Nothing to paste: copy or cut first", run: actions.paste },
          duplicate: { state: needSel(), run: actions.duplicate },
          "select-all": { state: true, run: actions.selectAll },
          "find-similar": { state: single(sel) ? true : "Select exactly one object first", run: actions.similar },
          find: { state: true, run: () => setFinding(true) },
          "highlight-net": { state: netSeed ? true : "Select one wire or net label first", run: () => netSeed && actions.highlight(netSeed) },
          "clear-highlight": { state: hi ? true : "No net is highlighted", run: () => setHi(null) },
          "align-left": { state: needParts(2), run: () => actions.align("left") },
          "align-right": { state: needParts(2), run: () => actions.align("right") },
          "align-top": { state: needParts(2), run: () => actions.align("top") },
          "align-bottom": { state: needParts(2), run: () => actions.align("bottom") },
          "align-center-x": { state: needParts(2), run: () => actions.align("center-x") },
          "align-center-y": { state: needParts(2), run: () => actions.align("center-y") },
          "distribute-x": { state: needParts(3), run: () => actions.distribute("x") },
          "distribute-y": { state: needParts(3), run: () => actions.distribute("y") },
          array: { state: needSel(), run: () => setArraying(true) },
          group: {
            state: typeof groupCmd === "string" ? groupCmd : idle(),
            run: () => {
              if (typeof groupCmd !== "string") submit([groupCmd], `Group ${objects(S.count(sel))}`);
            },
          },
          ungroup: {
            state: ungroupCmds.length ? idle() : "The selection is not part of a group",
            run: () => submit(ungroupCmds, `Ungroup ${ungroupCmds.length} groups`),
          },
        }
      : undefined;

  // Keyboard shortcuts act on the current selection through the same commands. Undo/redo
  // (Ctrl+Z, Ctrl+Shift+Z) are workspace-wide and handled by the workspace.
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e) => {
    if (!edit || !sheet || !actions || isTyping(e.target) || e.altKey || !isActiveEditor(docId)) return;
    const k = e.key.toLowerCase();
    if (e.ctrlKey || e.metaKey) {
      if (k === "g") {
        // Group / Ungroup (ADR-0045) run through the published ops, so they share the menu's state.
        e.preventDefault();
        const o = ops?.[e.shiftKey ? "ungroup" : "group"];
        if (o && o.state === true) o.run();
        else if (o && o.state !== true) edit.notify(o.state);
        return;
      }
      if (e.shiftKey) return;
      if (k === "c") actions.copy();
      else if (k === "x") actions.cut();
      else if (k === "v" && clipboard()) {
        e.preventDefault();
        actions.paste();
      } else if (k === "d") {
        e.preventDefault();
        actions.duplicate();
      } else if (k === "a") {
        e.preventDefault();
        actions.selectAll();
      } else if (k === "f") {
        e.preventDefault();
        setFinding(true);
      }
      return;
    }
    // Wire and bus legs: "/" cycles 90° / 45° / any angle, Space flips the bend; Enter finishes a bus.
    if (LEG_TOOLS.includes(tool) && (e.key === "/" || e.key === " ")) {
      e.preventDefault();
      if (e.key === "/") setWireMode((m) => nextWireMode(m));
      else setBendFlip((f) => !f);
      return;
    }
    if (tool === "bus" && e.key === "Enter") {
      e.preventDefault();
      finishBus();
      return;
    }
    const digit = Number(e.key);
    if (digit >= 1 && digit <= TOOLS.length) choose(TOOLS[digit - 1][0]);
    else if (e.key === "Escape") {
      reset();
      setDrag(null);
      setExternalDrag(null);
      setSel(S.EMPTY);
      setHi(null);
      setMenu(null);
      setArraying(false);
      setTool("select");
    } else if (k === "w") choose("wire");
    else if (k === "r") {
      if (tool === "busentry") setEntryDir((d) => (d + 1) % 4);
      else if (tool === "place" || externalDrag) {
        setPlaceRotation((r) => nextRotation(r));
      } else {
        actions.rotate();
      }
    } else if (k === "x") actions.flip("horizontal");
    else if (k === "y") actions.flip("vertical");
    else if (e.key === "Delete" || e.key === "Backspace") actions.remove();
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // A5: the active tool's options, shown by the right dock's inspector (inline here when no
  // inspector is shown), and the selection the inspector edits.
  const options: OptionField[] = !edit
    ? []
    : tool === "place"
      ? [
          { kind: "select", name: "Part", label: "Part", value: part?.id ?? "", options: edit.library.map((c) => ({ value: c.id, label: `${c.name} (${c.id})` })), onChange: setPartId },
          { kind: "select", name: "Place rotation", label: "Rotation", value: placeRotation, options: ROTATION_CHOICES, onChange: (v) => setPlaceRotation(v as Rotation), title: "Rotation of the next placed part (R)" },
        ]
      : tool === "label" || tool === "pinlabel"
        ? [
            { kind: "text", name: "Net name", label: "Net name", value: labelName, onChange: setLabelName },
            {
              kind: "select",
              name: "Label kind",
              label: "Kind",
              value: labelKind,
              options: [
                { value: "net", label: "net label" },
                { value: "power", label: "power port" },
              ],
              onChange: (v) => setLabelKind(v as "net" | "power"),
            },
          ]
        : LEG_TOOLS.includes(tool)
          ? [
              ...(tool === "bus" ? [{ kind: "text" as const, name: "Bus name", label: "Bus name", value: busName, onChange: setBusName, placeholder: "D[0..7] or {SDA,SCL}", title: "Members: D[0..7] gives D0..D7; {SDA,SCL} lists them" }] : []),
              { kind: "select", name: "Wire corners", label: "Corners", value: wireMode, options: WIRE_MODES.map((m) => ({ value: m.id, label: m.label })), onChange: (v) => setWireMode(v as WireMode), title: "Corner mode of the next leg (/ cycles it)" },
              {
                kind: "select",
                name: "Wire bend",
                label: "Bend",
                value: bendFlip ? "flip" : "first",
                options: wireMode === "45" ? [{ value: "first", label: "diagonal first" }, { value: "flip", label: "straight first" }] : [{ value: "first", label: "horizontal first" }, { value: "flip", label: "vertical first" }],
                onChange: (v) => setBendFlip(v === "flip"),
                title: "Which leg comes first (Space flips it); Shift keeps the point on one axis",
              },
              ...(tool === "wire" ? [{ kind: "unavailable" as const, name: "Wire style", label: "Width, colour, line style", reason: "Unavailable: the EAK-IR has no wire style fields yet (IR v3, slice B1)" }] : []),
            ]
          : tool === "busentry"
            ? [{ kind: "select", name: "Entry direction", label: "Direction", value: String(entryDir), options: ENTRY_DIR_LABELS.map((l, i) => ({ value: String(i), label: l })), onChange: (v) => setEntryDir(Number(v)), title: "Where the member wire leaves the bus (R turns it)" }]
            : tool === "power"
              ? [
                  {
                    kind: "select",
                    name: "Power port",
                    label: "Port",
                    value: String(powerIdx),
                    options: ports.map((p, i) => ({ value: String(i), label: `${p.name || "(unnamed)"} (${p.style})` })),
                    onChange: (v) => setPowerIdx(Number(v)),
                    title: "Entries of Settings › Power ports",
                  },
                ]
              : tool === "netport" || tool === "offpage"
                ? [{ kind: "text", name: "Net name", label: "Net name", value: labelName, onChange: setLabelName }]
                : tool === "text"
                  ? [{ kind: "text", name: "Text", label: "Text", value: gfxText, onChange: setGfxText }]
                  : [];
  usePublishEditorContext(docId, {
    editor: "schematic",
    tool,
    toolLabel: ALL_TOOLS.find(([t]) => t === tool)?.[1] ?? tool,
    options,
    selection: { ...NO_SELECTION, symbols: sel.symbols, wires: sel.wires, labels: sel.labels },
    sheetId: sheet?.id ?? null,
    ops,
  });
  const inspector = useHasInspector();
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
  // The selection filter (right dock > Filter): a kind switched off is not picked by a click, a
  // box, Select all or Find similar; the press then acts as on the empty sheet.
  const pickAt = (e: { target: EventTarget }): [S.Kind, Id] | null => {
    const o = objectAt(e);
    return o && filter[o[0]] ? o : null;
  };
  const selectClick = (e: React.PointerEvent) => {
    const o = pickAt(e);
    const justPicked = pressed.current;
    pressed.current = null;
    const mode = S.pickMode(e);
    if (!o) return setSel(mode === "replace" ? S.EMPTY : sel);
    if (justPicked === o[1]) return;
    setSel(S.pickGrouped(snapshot, sel, o[0], o[1], mode));
    if (o[0] === "symbol") onSelect(o[1]);
  };
  const dragHandler: DragHandler = {
    start: (x, y, e) => {
      if (!edit || edit.busy) return false;
      const at = fromView(x, y);
      const o = pickAt(e);
      pressed.current = null;
      downOnObject.current = !!o;
      if (!o && tool !== "select") return false;
      if (o && !S.has(sel, o[0], o[1])) {
        pressed.current = o[1];
        setSel(S.pickGrouped(snapshot, sel, o[0], o[1], S.pickMode(e) === "replace" ? "replace" : "add"));
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
        const found = S.withGroups(snapshot, S.filtered(S.marquee(snapshot, sheet.id, S.rectOf(d.from, to)), filter));
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

  // B3: right-click opens the canvas context menu the action registry generates for what was hit.
  // An unselected object becomes the selection first (a selected one keeps the whole selection).
  const onContext = (e: ReactMouseEvent<SVGSVGElement>) => {
    e.preventDefault();
    if (!edit || !contextMenu) return;
    const o = pickAt(e);
    if (o && !S.has(sel, o[0], o[1])) {
      setSel(S.pickGrouped(snapshot, sel, o[0], o[1], "replace"));
      if (o[0] === "symbol") onSelect(o[1]);
    }
    const label = !o ? `Sheet ${sheet.name}` : o[0] === "symbol" ? `Part ${schematic.symbols[o[1]]?.refdes ?? ""}`.trim() : o[0] === "label" ? `Net label ${schematic.labels[o[1]]?.name ?? ""}`.trim() : "Wire";
    setMenu({ x: e.clientX, y: e.clientY, target: o ? o[0] : "sheet", label });
  };
  const hiM = hi ? netMembers(sheetGroups(snapshot, sheet.id), hi) : null;

  const dropTarget: DropHandler = {
    onDragEnter: (x, y, e) => {
      e.preventDefault();
      const dragged = getDraggedComponent();
      let compId = dragged?.id;
      let compName = dragged?.name ?? "";
      if (!compId) {
        try {
          const json = e.dataTransfer.getData(EAK_COMPONENT_MIME);
          if (json) {
            const parsed = JSON.parse(json);
            compId = parsed.id;
            compName = parsed.name ?? parsed.id;
          }
        } catch {}
      }
      if (!compId) {
        compId = e.dataTransfer.getData("text/plain");
        if (compId) compName = compId;
      }
      if (!compId) return;
      const pt = fromView(x, y);
      const snap = snapPoint(pt, GRID, pins.map((p) => p.at), 0);
      setExternalDrag({ componentId: compId, name: compName, at: snap.point });
    },
    onDragOver: (x, y, e) => {
      e.preventDefault();
      const dragged = getDraggedComponent();
      let compId = dragged?.id;
      let compName = dragged?.name ?? "";
      if (!compId) {
        try {
          const json = e.dataTransfer.getData(EAK_COMPONENT_MIME);
          if (json) {
            const parsed = JSON.parse(json);
            compId = parsed.id;
            compName = parsed.name ?? parsed.id;
          }
        } catch {}
      }
      if (!compId) {
        compId = e.dataTransfer.getData("text/plain");
        if (compId) compName = compId;
      }
      if (!compId) return;
      const pt = fromView(x, y);
      const snap = snapPoint(pt, GRID, pins.map((p) => p.at), 0);
      setExternalDrag({ componentId: compId, name: compName, at: snap.point });
    },
    onDragLeave: () => {
      setExternalDrag(null);
    },
    onDrop: (x, y, e) => {
      e.preventDefault();
      const dragged = getDraggedComponent();
      let compId = dragged?.id;
      if (!compId) {
        try {
          const json = e.dataTransfer.getData(EAK_COMPONENT_MIME);
          if (json) {
            const parsed = JSON.parse(json);
            compId = parsed.id;
          }
        } catch {}
      }
      if (!compId) {
        compId = e.dataTransfer.getData("text/plain");
      }
      setExternalDrag(null);
      setDraggedComponent(null);
      if (!compId || !sheet || !edit || edit.busy) return;
      const pt = fromView(x, y);
      const snap = snapPoint(pt, GRID, pins.map((p) => p.at), 0);
      void edit.execute({
        type: "PlaceSymbol",
        sheet_id: sheet.id,
        component_id: compId,
        position: snap.point,
        rotation: placeRotation,
        refdes: null,
        value: null,
      });
      // The engine's result (accepted with its revision, or the rejection) is the notice.
    },
  };

  /** Shift lock: the next leg keeps to the dominant axis from its start. */
  const legEnd = (from: Point, p: Point): Point => (shiftDown ? constrainAxis(from, p) : p);
  /** The grid point under view (x, y); buses and bus entries do not capture pins. */
  const gridAt = (x: number, y: number): Point => snapPoint(fromView(x, y), GRID, [], 0).point;
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
        // A click on an existing symbol selects it (drag start); it never places on top. Decided
        // by what the press hit (selecting mounts the selection bar, which shifts the canvas
        // before the release) and by geometry (the target can be the ghost, not the body).
        const pressedObject = downOnObject.current;
        downOnObject.current = false;
        const at = fromView(x, y);
        if (pressedObject || symId || symbols.some((s) => Math.abs(at.x - s.position.x) <= s.half_w && Math.abs(at.y - s.position.y) <= s.half_h)) return;
        if (!part) return edit.notify("No library part available.");
        void edit.execute({
          type: "PlaceSymbol",
          sheet_id: sheet.id,
          component_id: part.id,
          position: snap.point,
          rotation: placeRotation,
          refdes: null,
          value: null,
        });
        return;
      }
      case "wire": {
        if (!start) return setStart(snap.point);
        const end = legEnd(start, snap.point);
        const legs = wireLegs(start, end, wireMode, bendFlip);
        setStart(end);
        // An L-shaped wire is two legs sent as ONE Batch: one revision, one undo, all-or-nothing.
        submit(legs.map(([a, b]) => ({ type: "AddWire" as const, sheet_id: sheet.id, a, b })), "Wire");
        return;
      }
      case "bus": {
        const p = gridAt(x, y);
        const last = busPts[busPts.length - 1];
        if (!last) return setBusPts([p]);
        const end = legEnd(last, p);
        // Clicking the last vertex again finishes the bus.
        if (end.x === last.x && end.y === last.y) return finishBus();
        setBusPts([...busPts, ...wireLegs(last, end, wireMode, bendFlip).map(([, b]) => b)]);
        return;
      }
      case "busentry": {
        const p = gridAt(x, y);
        const bus = busAt(snapshot, sheet.id, p);
        if (!bus) return edit.notify("Click on a bus: a bus entry starts on a bus (draw one with the Bus tool).");
        void edit.execute({ type: "AddBusEntry", bus_id: bus, bus_point: p, wire_point: busEntryWirePoint(p, entryDir) });
        return;
      }
      case "power": {
        if (!port) return edit.notify("The power-port list is empty: add entries in Settings › Power ports.");
        const bad = powerPortProblem(port);
        if (bad) return edit.notify(`${bad} for this power port (Settings › Power ports).`);
        void edit.execute(powerPortCommand(sheet.id, port, snap.point));
        return;
      }
      case "netport":
      case "offpage": {
        const name = labelName.trim();
        if (!name) return edit.notify("Enter a net name first.");
        void edit.execute({ type: "AddNetLabel", sheet_id: sheet.id, name, position: snap.point, kind: "net", scope: LABEL_SCOPE[tool] });
        return;
      }
      case "line":
      case "rect":
      case "circle":
      case "text": {
        if (tool !== "text" && !start) return setStart(snap.point);
        const g = graphicFrom(tool, start ?? snap.point, snap.point, gfxText);
        setStart(null);
        if (!g) return edit.notify(tool === "text" ? "Enter the text first." : "Click a second, different point.");
        void edit.execute({ type: "AddSheetGraphic", sheet_id: sheet.id, graphic: g });
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
        void edit.execute(toggleNoConnect(snapshot, pinHit.symbol, pinHit.number));
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
        else {
          const c = drawnAt(e);
          if (c) void edit.execute(c);
        }
        return;
      }
    }
  };

  // Live previews of the B4 tools at the snapped pointer: the wire or bus legs (in the corner mode,
  // with the Shift lock), the shape a graphic tool will draw, and where a bus entry would go.
  const gridOnly = tool === "bus" || tool === "busentry";
  const hoverSnap = hover ? snapPoint(hover, GRID, gridOnly ? [] : pins.map((p) => p.at), gridOnly ? 0 : CAPTURE).point : null;
  const legStart = tool === "wire" ? start : tool === "bus" ? (busPts[busPts.length - 1] ?? null) : null;
  const legTarget = hoverSnap && legStart ? legEnd(legStart, hoverSnap) : hoverSnap;
  const preview = legStart && legTarget ? wireLegs(legStart, legTarget, wireMode, bendFlip) : [];
  const gfxPreview = hoverSnap && isGraphicTool(tool) && tool !== "text" && start ? graphicFrom(tool, start, hoverSnap) : null;
  const entryPreview = hoverSnap && tool === "busentry" ? { a: hoverSnap, b: busEntryWirePoint(hoverSnap, entryDir), onBus: !!busAt(snapshot, sheet.id, hoverSnap) } : null;
  // Crosshair guides through the snapped pointer while a place tool is armed.
  const guide = edit && tool !== "select" && tool !== "move" ? legTarget : null;


  return (
    <section className="view" aria-label="Schematic view" data-tool={tool} data-wire-mode={wireMode}>
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
        <Coords at={hover} unit={grid.unit} />
      </header>
      {edit && finding && (
        <FindBar
          snapshot={snapshot}
          sheetId={sheet.id}
          busy={edit.busy}
          onSelect={(hits) => {
            setTool("select");
            setSel(S.filtered({ symbols: hits.filter((h) => h.kind === "symbol").map((h) => h.id), wires: [], labels: hits.filter((h) => h.kind === "label").map((h) => h.id) }, filter));
          }}
          onReplace={submit}
          onClose={() => setFinding(false)}
        />
      )}
      {edit && arraying && actions && (
        <ArrayPanel stepMm={Math.round(8 * grid.spacingMm * 1000) / 1000} busy={edit.busy} onCreate={actions.array} onClose={() => setArraying(false)} />
      )}
      {edit && (
        <div className="tool-options">
          {!inspector && <ToolOptionFields fields={options} layout="strip" />}
          {sym && !inspector && <SymbolFields key={`${sym.id}|${sym.refdes}|${sym.value}`} sym={sym} edit={edit} />}
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
                ? "Click or drag a box to select (Shift adds, Ctrl toggles) · right-click for actions · W wire · Ctrl+V paste · Ctrl+F find · middle-drag pans"
                : `${objects(S.count(sel))} selected · drag to move · R rotate · X/Y flip · Del delete · Ctrl+C/X/V/D copy/cut/paste/duplicate · Esc clear`)}
            {tool === "place" && `Click sheet to place · R rotate (${placeRotation}) · Esc cancel`}
            {externalDrag && `Drop component to place · R rotate (${placeRotation})`}
            {tool === "wire" && (start ? "Click to end the wire segment · Esc stops" : "Click a start point (pins snap)")}
            {tool === "connect" && (pin ? "Click the second pin" : "Click the first pin")}
            {tool === "label" && "Click to drop the label"}
            {tool === "move" && (picked ? "Click the new position" : "Drag a part, or click a symbol then its destination")}
            {tool === "delete" && "Click a symbol, wire or label"}
            {tool === "noconnect" && "Click a pin to mark it intentionally unconnected (the ERC-001 remedy); click a marked pin to remove its marker"}
            {tool === "pinlabel" && "Click a pin to put the net label on it"}
            {LEG_TOOLS.includes(tool) &&
              `${tool === "bus" ? (busPts.length ? "Click the next vertex · click the last one again or Enter finishes" : "Click the bus start") : ""} · / corners (${WIRE_MODES.find((m) => m.id === wireMode)?.label}) · Space flips the bend · Shift locks the axis · Esc stops`}
            {tool === "busentry" && `Click on a bus to add an entry (${ENTRY_DIR_LABELS[entryDir]}; R turns it); draw the member wire from its free end and name it`}
            {tool === "power" && (port ? `Click to place power port ${port.name} · the list is in Settings › Power ports` : "The power-port list is empty: add entries in Settings › Power ports")}
            {tool === "netport" && "Click to drop a sheet-local net port (joins this sheet only)"}
            {tool === "offpage" && "Click to drop an off-page connector (joins every sheet)"}
            {(tool === "line" || tool === "rect" || tool === "circle") && (start ? "Click the second point" : `Click the first point of the ${tool === "rect" ? "rectangle" : tool === "circle" ? "circle (its centre)" : "line"}`)}
            {tool === "text" && "Click to put the text on the sheet"}
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
        onDropTarget={dropTarget}
        onContextMenu={edit ? onContext : undefined}
        underlay={(vb) => (
          <>
            <rect className="sheet" x={0} y={-mm(sheet.height)} width={mm(sheet.width)} height={mm(sheet.height)} />
            <GridLayer vb={vb} kind="schematic" cfg={grid} />
          </>
        )}
      >
        <SheetGraphics snapshot={snapshot} sheetId={sheet.id} />
        {guide && (
          <g className="guides" data-guide="crosshair" pointerEvents="none">
            <line x1={0} y1={toView(guide)[1]} x2={mm(sheet.width)} y2={toView(guide)[1]} />
            <line x1={toView(guide)[0]} y1={-mm(sheet.height)} x2={toView(guide)[0]} y2={0} />
          </g>
        )}
        {Object.values(schematic.wires)
          .filter((w) => w.sheet_id === sheet.id)
          .map((w) => {
            const [x1, y1] = toView(w.a);
            const [x2, y2] = toView(w.b);
            return (
              <g key={w.id} data-wire-id={w.id} className={[S.has(sel, "wire", w.id) ? "selected" : "", hiM?.wires.has(w.id) ? "net-hi" : ""].filter(Boolean).join(" ") || undefined}>
                <line className="wire" x1={x1} y1={y1} x2={x2} y2={y2} />
                <line className="hit" x1={x1} y1={y1} x2={x2} y2={y2} />
              </g>
            );
          })}
        {junctionPoints(
          Object.values(schematic.wires).filter((w) => w.sheet_id === sheet?.id),
          pins.map((p) => p.at),
        ).map((p) => {
          const [cx, cy] = toView(p);
          return <circle key={`j${p.x},${p.y}`} className="junction" cx={cx} cy={cy} r={0.45} />;
        })}
        {preview.map(([a, b], i) => {
          const [x1, y1] = toView(a);
          const [x2, y2] = toView(b);
          return <line key={i} className={`${tool === "bus" ? "bus" : "wire"} preview`} x1={x1} y1={y1} x2={x2} y2={y2} />;
        })}
        {busPts.length > 1 && <polyline className="bus preview" points={busPts.map((p) => toView(p).join(",")).join(" ")} />}
        {gfxPreview && (
          <g className="sheet-graphic preview" pointerEvents="none">
            <GraphicShape g={gfxPreview} />
          </g>
        )}
        {entryPreview && (
          <line
            className={`bus-entry preview${entryPreview.onBus ? "" : " off"}`}
            x1={toView(entryPreview.a)[0]}
            y1={toView(entryPreview.a)[1]}
            x2={toView(entryPreview.b)[0]}
            y2={toView(entryPreview.b)[1]}
          />
        )}
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
          const inst = schematic.symbols[s.id];
          const drawing = symbolDrawing(
            inst ? libSymbol(snapshot, inst.component_id) : undefined,
            { position: s.position, rotation: inst?.rotation ?? "R0", mirror: inst?.mirror },
            s,
          );
          return (
            <g
              key={s.id}
              data-symbol-id={s.id}
              transform={dragDelta && S.has(sel, "symbol", s.id) ? `translate(${mm(dragDelta.x)} ${-mm(dragDelta.y)})` : undefined}
              className={`symbol${S.has(sel, "symbol", s.id) ? " selected" : ""}${aiPreview.includes(s.id) ? " ai-preview" : ""}`}
              onClick={() => onSelect(s.id)}
            >
              <title>{`${s.refdes} ${s.value}`}</title>
              {/* Whole-body hit area: open drawings (a capacitor's plates) are thin to click. */}
              <rect className="sym-hit" x={x - hw} y={y - hh} width={2 * hw} height={2 * hh} />
              <SymbolSvg drawing={drawing} activePin={pin?.symbol === s.id ? pin.pin : null} hiPins={hiM ? new Set(drawing.pins.map((p) => p.number).filter((n) => hiM.pinKeys.has(`${s.id}/${n}`))) : undefined} />
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
            const style = l.kind === "power" ? powerStyleOf(l.name, l.power_style) : null;
            const port = style ? powerPortShape(style, x, y) : null;
            return (
              <g
                key={l.id}
                data-label-id={l.id}
                data-power-style={style ?? undefined}
                data-scope={l.kind === "net" ? (l.scope ?? "global") : undefined}
                className={`label ${l.kind}${l.kind === "net" && l.scope === "local" ? " local" : ""}${S.has(sel, "label", l.id) ? " selected" : ""}${hiM?.labels.has(l.id) ? " net-hi" : ""}`}
                transform={labelShift(l.id)}
              >
                <circle cx={x} cy={y} r={0.5} />
                {port ? (
                  <>
                    <path className="power-port" d={port.d} />
                    <text x={port.text.x} y={port.text.y} style={{ textAnchor: "middle" }}>
                      {l.name}
                    </text>
                  </>
                ) : l.scope === "off_page" ? (
                  <>
                    <path className="port-shape" d={offPageShape(x, y, l.name)} />
                    <text x={x + 1.6} y={y + 0.55}>
                      {l.name}
                    </text>
                  </>
                ) : (
                  <text x={x + 1} y={y - 0.8}>
                    {l.name}
                  </text>
                )}
              </g>
            );
          })}
        {sheetNoConnects(snapshot, sheet.id).map((n) => {
          const at = pinAt.get(`${n.symbol_id}/${n.pin}`);
          if (!at) return null;
          const [x, y] = toView(at);
          return (
            <path key={n.id} className="no-connect" d={`M${x - 1} ${y - 1}L${x + 1} ${y + 1}M${x - 1} ${y + 1}L${x + 1} ${y - 1}`} />
          );
        })}
        {tool === "place" && hover && part && (
          <PlaceGhost
            snapshot={snapshot}
            componentId={part.id}
            name={part.name}
            at={snapPoint(hover, GRID, [], 0).point}
            rotation={placeRotation}
          />
        )}
        {externalDrag && (
          <PlaceGhost
            snapshot={snapshot}
            componentId={externalDrag.componentId}
            name={externalDrag.name}
            at={externalDrag.at}
            rotation={placeRotation}
          />
        )}
        {drag?.mode === "marquee" && <Marquee from={drag.from} to={drag.to} />}
        {start && <circle className="anchor" cx={toView(start)[0]} cy={toView(start)[1]} r={0.6} />}
      </Canvas>
      {menu && contextMenu && <CanvasContextMenu key={`${menu.x},${menu.y},${menu.target}`} x={menu.x} y={menu.y} label={menu.label} items={contextMenu(menu.target)} onClose={() => setMenu(null)} />}
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
 * Where the Place tool will drop the part: its symbol drawing at the snapped cursor, turned by
 * the place rotation exactly as the engine will place it, inside a dashed outline of its body
 * (when the definition or an existing instance gives the size), and a crosshair on the origin.
 */
function PlaceGhost({
  snapshot,
  componentId,
  name,
  at,
  rotation = "R0",
}: {
  snapshot: Snapshot;
  componentId: string;
  name: string;
  at: Point;
  rotation?: Rotation;
}) {
  const def = libSymbol(snapshot, componentId);
  const twin = Object.values(snapshot.project.schematic.symbols).find(
    (s) => s.component_id === componentId && (s.rotation === "R0" || s.rotation === "R180"),
  );
  const geom = twin && snapshot.geometry.symbols.find((g) => g.id === twin.id);
  const w0 = def?.body_half_w ?? geom?.half_w ?? 2_000_000;
  const h0 = def?.body_half_h ?? geom?.half_h ?? 1_500_000;
  const turned = rotation === "R90" || rotation === "R270";
  const [hw, hh] = turned ? [h0, w0] : [w0, h0];
  const [x, y] = toView(at);
  const full = def ? symbolDrawing(def, { position: at, rotation }, { half_w: hw, half_h: hh, pins: [] }) : null;
  // Without v3 graphics the dashed outline already is the body.
  const drawing = full && !def?.graphics?.length ? { ...full, body: [] } : full;

  return (
    <g className="place-ghost" data-place-ghost={componentId} style={{ pointerEvents: "none" }}>
      <rect
        x={x - mm(hw)}
        y={y - mm(hh)}
        width={2 * mm(hw)}
        height={2 * mm(hh)}
        fill="var(--accent)"
        fillOpacity={0.15}
        stroke="var(--accent)"
        strokeDasharray="2 2"
        strokeWidth={0.3}
      />
      <path d={`M${x - 1.27} ${y}H${x + 1.27}M${x} ${y - 1.27}V${y + 1.27}`} stroke="var(--accent)" strokeWidth={0.4} />
      {drawing && (
        <g className="ghost-symbol">
          <SymbolSvg drawing={drawing} />
        </g>
      )}
      <text x={x} y={y - mm(hh) - 1} textAnchor="middle" fontSize={1.8} fill="currentColor" fontWeight="bold">
        {name} {rotation !== "R0" ? `(${rotation})` : ""}
      </text>
    </g>
  );
}

/** A part's symbol definition in the project library (built-ins are copied in when placed). */
function libSymbol(snapshot: Snapshot, componentId: string): DrawableSymbol | undefined {
  return snapshot.project.library?.[componentId]?.symbol;
}

/** Marquee rectangle while dragging a selection box. */
function Marquee({ from, to }: { from: Point; to: Point }) {
  const [x1, y1] = toView(from);
  const [x2, y2] = toView(to);
  return <rect className="marquee" x={Math.min(x1, x2)} y={Math.min(y1, y2)} width={Math.abs(x2 - x1)} height={Math.abs(y2 - y1)} />;
}

/** Off-page connector outline at view point (x, y): a pentagon arrow around the net name. */
function offPageShape(x: number, y: number, name: string): string {
  const w = Math.max(2, name.length + 0.8);
  return `M${x} ${y}L${x + 1} ${y - 1.2}H${x + 1 + w}L${x + 2 + w} ${y}L${x + 1 + w} ${y + 1.2}H${x + 1}Z`;
}
