// Per-document action sets. Each operation is defined once here, with the documents it applies
// to, its menu, its toolbar group and (through the default key map) its keys; the registry
// generates every route from that. Handlers come from the workspace, which owns the state.
import { formatLength, stepPreset, UNITS, type GridConfig, type GridKind } from "../grid";
import type { SideView } from "../shell/ActivityBar";
import type { BottomTab } from "../shell/BottomPanel";
import type { RightTab } from "../shell/RightDock";
import type { Check, Command, LibraryEntry, OpenProject, RecentProject, Snapshot, SymbolInstance, FootprintInstance } from "../types";
import type { IconName } from "../design/icons/icons";
import type { View3dControls } from "../viewer3d/controls";
import { PRESETS } from "../viewer3d/view";
import type { Action, DocContext, SplitId } from "./types";
import { powerPortProblem, powerPorts } from "../settings/powerPorts";
import { stitchBlocked } from "../shell/planes";
import { fanOutCommand, type PowerPreset } from "../tools/schematic/placeTools";

export interface ActionDeps {
  snapshot: Snapshot;
  busy: boolean;
  /** Power-port split entries (Settings › Power ports); the stored list when absent. */
  powerPorts?: readonly PowerPreset[];
  selected: string | null;
  selectedSymbol: SymbolInstance | undefined;
  fpOfSelected: FootprintInstance | undefined;
  theme: "dark" | "light";
  sideOpen: boolean;
  bottomOpen: boolean;
  rightOpen: boolean;
  split: boolean;
  recent: RecentProject[];
  library: LibraryEntry[];
  docs: { id: string; kind: string; title: string }[];
  openIds: string[];
  projects: OpenProject[];
  sideViews: { id: SideView; label: string }[];
  edaViews: { id: string; label: string; shortcut?: string }[];
  dialog: (d: "new" | "open" | "export" | "rules" | "bom" | "outputs" | "annotate" | "release") => void;
  save: () => void;
  undo: () => void;
  redo: () => void;
  close: () => void;
  openRecent: (path: string) => void;
  execute: (c: Command) => void;
  check: (c: Check) => void;
  syncPcb: () => void;
  refillZones: () => void;
  rotateSel: () => void;
  deleteSel: () => void;
  /** Arms a tool of the active editor (the same call the editor's own tool buttons make). */
  tool: (id: string) => void;
  zoomBy: (f: number) => void;
  fit: () => void;
  select: (id: string | null) => void;
  toggleSplit: () => void;
  /** Shows that side view (opens the panel if needed; never collapses it). */
  showSide: (v: SideView) => void;
  toggleSide: () => void;
  toggleBottom: () => void;
  toggleRight: () => void;
  showBottom: (t: BottomTab) => void;
  showRight: (t: RightTab) => void;
  onTheme: () => void;
  resetLayout: () => void;
  onSettings: () => void;
  /** Opens Settings (hotkeys, canvas colours, grid defaults, theme). */
  preferences: () => void;
  palette: () => void;
  quickOpen: () => void;
  help: () => void;
  /** Opens the About dialog (version, what EAK does and does not claim). */
  about: () => void;
  /** Side view shown in the side panel ("Show <view>" is checked when it is open). */
  sideView: SideView;
  notify: (m: string) => void;
  open: (docId: string) => void;
  closeAll: () => void;
  showWorkspace: (id: string) => void;
  switchTo: (projectId: string) => void;
  placePart: (c: LibraryEntry) => void;
  /** Grid of the active editor (null when no schematic or PCB editor is active). */
  grid: { kind: GridKind; cfg: GridConfig; set: (patch: Partial<GridConfig>) => void } | null;
  /** Toolbar shows text labels beside its icons (accessibility option). */
  toolbarLabels: boolean;
  toggleToolbarLabels: () => void;
  /** The open 3D document's published controls (slice C6); absent or null while none is drawing. */
  view3d?: View3dControls | null;
  /** Cross-probe the selected part into the PCB or the schematic editor (3D document). */
  probeTo?: (kind: "pcb" | "schematic") => void;
  /**
   * An editing operation of the active editor (slice B3: clipboard, arrange, find, net highlight),
   * or null when the active editor has none by that id. Read at call time, so never stale.
   */
  editorOp?: (op: string) => { enabled: () => true | string; run: () => void } | null;
}

const EDITING: readonly DocContext[] = ["schematic", "pcb"];
const SCH: readonly DocContext[] = ["schematic"];
const PCB: readonly DocContext[] = ["pcb"];
const DESIGN: readonly DocContext[] = ["home", "schematic", "pcb"];
const BOARD: readonly DocContext[] = ["pcb", "3d"];
const V3D: readonly DocContext[] = ["3d"];
/** Why an editor operation is unavailable when no schematic editor has registered it. */
export const NO_EDITOR = "No schematic editor is active";
/** Enabled state of an operation the active editor offers, else the given fallback. */
const viaEditor = (d: ActionDeps, op: string, fallback: () => true | string) => () => d.editorOp?.(op)?.enabled() ?? fallback();
/** Runs the active editor's operation when it offers one, else the fallback. */
const runVia = (d: ActionDeps, op: string, fallback: () => void) => {
  const o = d.editorOp?.(op);
  if (o) o.run();
  else fallback();
};
/** The handler of an action that has no implementation; such an action must be unavailable with a reason (`qa/uiInventory.ts`). */
export const NO_HANDLER = () => {};

/** File, Edit, View, Project, Tools, Settings and Help: the chrome every document shares. */
export function commonActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const needSel = () => (d.selectedSymbol ? idle() : "Select a part first");
  const recent = d.recent.filter((r) => r.project_id !== d.snapshot.project.id).slice(0, 5);
  return [
    { id: "new", label: "New project…", short: "New project", menu: "File", toolbar: "file", icon: "new", enabled: idle, run: () => d.dialog("new") },
    { id: "open", label: "Open project…", short: "Open project", menu: "File", toolbar: "file", icon: "open", enabled: idle, run: () => d.dialog("open") },
    ...recent.map<Action>((r, i) => ({ id: `recent-${r.project_id}`, label: `Recent: ${r.name}`, menu: "File", sep: i === 0, palette: false, enabled: idle, run: () => d.openRecent(r.path) })),
    // A-SAVE: the engine writes every accepted command before it returns, so there is nothing
    // unsaved; Save asks the engine to confirm that (and reports blocked writes). No Save as.
    { id: "save", label: "Save (confirm written)", short: "Save", title: "Save: every accepted command is already written; this asks the engine to confirm no write is blocked", menu: "File", sep: true, toolbar: "file", icon: "save", whileTyping: true, enabled: idle, run: d.save },
    // D3 outputs: each runs ERC/DRC/MECH/DFM first and writes a DRAFT-unless-released manifest.
    { id: "export-bom", label: "Export BOM…", short: "Export BOM", title: "Configurable BOM (columns, sort, grouping, filter rules) as CSV or XLSX; runs ERC/DRC/MECH/DFM", docs: DESIGN, menu: "File", sep: true, enabled: idle, run: () => d.dialog("bom") },
    { id: "export-docs", label: "Export schematic and netlist…", short: "Export schematic and netlist", title: "Schematic PDF/SVG with title block, IPC-D-356 and JSON netlists; runs ERC/DRC/MECH/DFM", docs: DESIGN, menu: "File", enabled: idle, run: () => d.dialog("outputs") },
    { id: "close", label: "Close project", menu: "File", sep: true, toolbar: "app", icon: "close", enabled: idle, run: d.close },

    { id: "undo", label: "Undo", menu: "Edit", toolbar: "file", icon: "undo", enabled: () => (d.snapshot.can_undo ? idle() : "Nothing to undo"), run: d.undo },
    { id: "redo", label: "Redo", menu: "Edit", toolbar: "file", icon: "redo", enabled: () => (d.snapshot.can_redo ? idle() : "Nothing to redo"), run: d.redo },
    { id: "select", label: "Select tool", title: "Select tool (key 1)", docs: EDITING, menu: "Edit", sep: true, toolbar: "edit", icon: "select", enabled: () => true, run: () => d.tool("select") },
    { id: "move", label: "Move tool", title: "Move tool (schematic key 6, PCB key 3)", docs: EDITING, menu: "Edit", toolbar: "edit", icon: "move", enabled: () => true, run: () => d.tool("move") },
    { id: "rotate", label: "Rotate selection", title: "Rotate selection (key R)", docs: EDITING, menu: "Edit", toolbar: "edit", icon: "rotate", hint: "R", context: ["symbol"], contextSep: true, enabled: viaEditor(d, "rotate", needSel), run: () => runVia(d, "rotate", d.rotateSel) },
    { id: "delete", label: "Delete selection", title: "Delete selection (one undo step: Ctrl+Z restores it)", docs: EDITING, menu: "Edit", toolbar: "edit", icon: "delete", context: ["symbol", "wire", "label"], enabled: viaEditor(d, "delete", needSel), run: () => runVia(d, "delete", d.deleteSel) },
    { id: "deselect", label: "Clear selection", keys: ["Escape"], menu: "Edit", enabled: () => (d.selected ? true : "Nothing is selected"), run: () => d.select(null) },

    { id: "zout", label: "Zoom out", docs: EDITING, menu: "View", toolbar: "view", icon: "zoom-out", enabled: () => true, run: () => d.zoomBy(1.4) },
    { id: "zin", label: "Zoom in", docs: EDITING, menu: "View", toolbar: "view", icon: "zoom-in", enabled: () => true, run: () => d.zoomBy(1 / 1.4) },
    { id: "fit", label: "Fit to view", title: "Fit to view (or double-click the canvas)", docs: EDITING, menu: "View", toolbar: "view", icon: "fit", enabled: () => true, run: d.fit },
    ...gridActions(d),
    { id: "toolbar-labels", label: "Toolbar labels", title: "Show text labels beside the toolbar icons", menu: "View", checked: d.toolbarLabels, enabled: () => true, run: d.toggleToolbarLabels },
    { id: "toggle-side", label: "Side panel", whileTyping: true, menu: "View", sep: true, checked: d.sideOpen, enabled: () => true, run: d.toggleSide },
    { id: "toggle-bottom", label: "Bottom panel", whileTyping: true, menu: "View", checked: d.bottomOpen, enabled: () => true, run: d.toggleBottom },
    { id: "toggle-right", label: "Right panel", whileTyping: true, menu: "View", checked: d.rightOpen, enabled: () => true, run: d.toggleRight },
    { id: "toggle-split", label: "Split schematic and PCB", menu: "View", checked: d.split, enabled: () => true, run: d.toggleSplit },
    ...d.sideViews.map<Action>((v, i) => ({ id: `show-${v.id}`, label: `Show ${v.label}`, whileTyping: v.id === "project", menu: "View", sep: i === 0, checked: d.sideOpen && d.sideView === v.id, enabled: () => true, run: () => d.showSide(v.id) })),
    { id: "show-properties", label: "Show Properties", menu: "View", sep: true, enabled: () => true, run: () => d.showRight("properties") },
    { id: "show-layers", label: "Show Layers", docs: PCB, menu: "View", enabled: () => true, run: () => d.showRight("layers") },
    { id: "library", label: "Show library", menu: "View", sep: true, enabled: () => true, run: () => d.showBottom("library") },
    { id: "bom", label: "Show BOM", menu: "View", enabled: () => true, run: () => d.showBottom("bom") },
    { id: "log", label: "Show log", menu: "View", enabled: () => true, run: () => d.showBottom("log") },
    { id: "ws-schematic", label: "Go to Schematic editor", whileTyping: true, menu: "View", sep: true, enabled: () => true, run: () => d.showWorkspace("schematic") },
    { id: "ws-pcb", label: "Go to PCB editor", whileTyping: true, menu: "View", enabled: () => true, run: () => d.showWorkspace("pcb") },
    ...d.edaViews.map<Action>((w) => ({ id: `ws-${w.id}`, label: `Go to ${w.label}`, keys: w.shortcut ? [w.shortcut] : [], whileTyping: true, menu: "View", enabled: () => true, run: () => d.showWorkspace(w.id) })),
    { id: "reset-layout", label: "Reset panel layout", menu: "View", sep: true, enabled: () => true, run: d.resetLayout },

    // A registered view (3D) is opened by its own "Go to" action, not listed again here.
    ...d.docs.filter((doc) => doc.kind !== "view").map<Action>((doc) => ({ id: `open-${doc.id}`, label: `Open ${doc.kind === "pcb" ? "PCB" : doc.kind === "file" ? `file ${doc.title}` : doc.kind === "symbol" || doc.kind === "footprint" ? `${doc.kind} editor ${doc.title.replace(/^\w+: /, "")}` : `sheet ${doc.title}`}`, menu: "Project", paletteGroup: "Documents", enabled: () => true, run: () => d.open(doc.id) })),
    { id: "close-all", label: "Close all documents", menu: "Project", paletteGroup: "Documents", enabled: () => (d.openIds.length === 0 ? "No documents are open" : true), run: d.closeAll },
    ...d.projects.filter((p) => !p.active).map<Action>((p, i) => ({ id: `proj-${p.id}`, label: `Switch to project ${p.name}`, menu: "Project", sep: i === 0, enabled: idle, run: () => d.switchTo(p.id) })),

    { id: "palette", label: "Command palette", short: "Commands", whileTyping: true, menu: "Tools", toolbar: "app", icon: "palette", palette: false, enabled: () => true, run: d.palette },
    { id: "quick-open", label: "Go to part, net or sheet…", whileTyping: true, menu: "Tools", enabled: () => true, run: d.quickOpen },

    { id: "preferences", label: "Settings…", title: "Hotkeys, canvas colours, grid defaults and theme", whileTyping: true, menu: "Settings", enabled: () => true, run: d.preferences },
    { id: "settings", label: "Model settings", menu: "Settings", enabled: () => true, run: d.onSettings },
    { id: "theme", label: "Light theme", menu: "Settings", checked: d.theme === "light", enabled: () => true, run: d.onTheme },

    { id: "help", label: "Keyboard shortcuts", menu: "Help", enabled: () => true, run: d.help },
    {
      id: "about",
      label: "About EAK",
      menu: "Help",
      enabled: () => true,
      run: d.about,
    },
  ];
}

/**
 * Grid of the active editor: visibility, snap, finer/coarser preset and the unit. The toolbar shows
 * the same settings as combos (`GridControls`); both write the editor kind's grid store.
 */
function gridActions(d: ActionDeps): Action[] {
  const g = d.grid;
  const has = () => (g ? true : "No schematic or PCB editor is active");
  const step = (dir: -1 | 1): Action => {
    const to = g ? stepPreset(g.kind, g.cfg, dir) : null;
    return {
      id: dir < 0 ? "grid-finer" : "grid-coarser",
      label: dir < 0 ? "Finer grid" : "Coarser grid",
      docs: EDITING,
      menu: "View",
      enabled: () => (!g ? has() : to === null ? `Already the ${dir < 0 ? "finest" : "coarsest"} ${g.cfg.unit} preset (${formatLength(g.cfg.spacingMm, g.cfg.unit)})` : true),
      run: () => g && to !== null && g.set({ spacingMm: to }),
    };
  };
  return [
    { id: "grid-show", label: "Show grid", docs: EDITING, menu: "View", sep: true, checked: g?.cfg.visible, enabled: has, run: () => g?.set({ visible: !g.cfg.visible }) },
    { id: "grid-snap", label: "Snap to grid", docs: EDITING, menu: "View", checked: g?.cfg.snap, enabled: has, run: () => g?.set({ snap: !g.cfg.snap }) },
    step(-1),
    step(1),
    ...UNITS.map<Action>((u) => ({ id: `unit-${u}`, label: `Unit: ${u}`, docs: EDITING, menu: "View", checked: g?.cfg.unit === u, enabled: has, run: () => g?.set({ unit: u }) })),
  ];
}

/** Checks and rules shared by the schematic, the PCB and the project home. */
export function designActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  return [
    { id: "sync", label: "Update PCB from schematic", short: "Update PCB", docs: DESIGN, menu: "Design", toolbar: "checks", icon: "sync", enabled: idle, run: d.syncPcb },
    { id: "rules", label: "Design rules and net classes…", short: "Design rules", title: "Design rules and net classes", docs: DESIGN, menu: "Design", sep: true, toolbar: "checks", icon: "rules", enabled: () => true, run: () => d.dialog("rules") },
  ];
}

/** Schematic editor: Place, Design (ERC, annotate) and Simulation. */
export function schematicActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const tool = (id: string, label: string, title: string, icon: IconName, more: Partial<Action> = {}): Action => ({ id: `t-${id}`, label, title, docs: SCH, menu: "Place", toolbar: "edit", icon, ...more, enabled: () => true, run: () => d.tool(id) });
  // Power-port split button: one entry per item of the user's list (Settings › Power ports).
  const ports = d.powerPorts ?? powerPorts();
  const power: Action[] = ports.length
    ? ports.map((p, i) => {
        const bad = powerPortProblem(p);
        return {
          id: `t-power-${i}`,
          label: `Power port ${p.name.trim() || "(unnamed)"}`,
          title: `Place power port ${p.name.trim()} (${p.style}; AddNetLabel kind power). Edit the list in Settings › Power ports`,
          docs: SCH,
          menu: "Place",
          sep: i === 0,
          toolbar: "edit",
          icon: p.style === "rail" ? "power" : "ground",
          split: "sch-power",
          enabled: () => (bad ? `${bad} (Settings › Power ports)` : true),
          run: () => d.tool(`power:${i}`),
        } satisfies Action;
      })
    : [{ id: "t-power", label: "Power port", docs: SCH, menu: "Place", sep: true, toolbar: "edit", icon: "power", split: "sch-power", enabled: () => "The power-port list is empty: add entries in Settings › Power ports", run: () => {} }];
  const fanOut = (kind: "labels" | "noconnects"): (() => void) => () => {
    if (!d.selectedSymbol) return;
    const c = fanOutCommand(d.snapshot, d.selectedSymbol.id, kind);
    if (typeof c === "string") d.notify(c);
    else d.execute(c);
  };
  const needPart = () => (d.selectedSymbol ? idle() : "Select a part first");
  return [
    tool("place", "Place part", "Place a part from the library (editor Place tool)", "part"),
    tool("wire", "Wire", "Draw a wire; ends snap to pins (AddWire). / changes the corner mode, Space flips the bend, Shift locks the axis", "wire", { sep: true }),
    tool("bus", "Bus", "Draw a named bus (AddBus): click the vertices, click the last one again or press Enter to finish", "bus", { split: "sch-bus" }),
    tool("busentry", "Bus entry", "Join a member wire to a bus (AddBusEntry): click on a bus; R turns the entry", "bus-entry", { split: "sch-bus" }),
    ...power,
    tool("label", "Net / power label", "Net label or power port (AddNetLabel; pick Net or Power in the tool options). Joins same-name labels on every sheet", "net-label", { split: "sch-labels", sep: true }),
    tool("netport", "Net port (this sheet)", "Sheet-local net port (AddNetLabel scope local): joins same-name labels on this sheet only", "net-port", { split: "sch-labels" }),
    tool("offpage", "Off-page connector", "Off-page connector (AddNetLabel scope off-page): joins the same name on every sheet", "off-page", { split: "sch-labels" }),
    tool("noconnect", "No connect", "Mark a pin as intentionally unconnected (SetNoConnect); click a marked pin to remove it (DeleteNoConnect)", "no-connect", { split: "sch-marks" }),
    tool("pinlabel", "Label pin", "Name the net on a pin (LabelPin)", "pin-label", { split: "sch-marks" }),
    tool("line", "Line", "Draw a line on the sheet (AddSheetGraphic; drawing only, never a connection)", "line", { split: "sch-graphics", sep: true }),
    tool("rect", "Rectangle", "Draw a rectangle on the sheet: click two corners (AddSheetGraphic)", "rect", { split: "sch-graphics" }),
    tool("circle", "Circle", "Draw a circle on the sheet: click the centre, then a point on it (AddSheetGraphic)", "circle", { split: "sch-graphics" }),
    tool("text", "Text", "Put text on the sheet (AddSheetGraphic text; never a net name)", "text", { split: "sch-graphics" }),
    { id: "fanout-labels", label: "Fan out net labels", title: "Give every unconnected pin of the selected part a wire stub and a net label named after the pin (one Batch)", docs: SCH, menu: "Place", sep: true, enabled: needPart, run: fanOut("labels") },
    { id: "fanout-noconnects", label: "Fan out no-connects", title: "Mark every unconnected pin of the selected part as intentionally unconnected (one Batch of SetNoConnect)", docs: SCH, menu: "Place", enabled: needPart, run: fanOut("noconnects") },
    // The tools come before the parts: the library can hold dozens of parts, and a tool listed
    // after them would be pushed past the bottom of the window.
    ...d.library.map<Action>((c, i) => ({ id: `part-${c.id}`, label: `Part: ${c.name}`, docs: SCH, menu: "Place", sep: i === 0, palette: false, enabled: () => true, run: () => d.placePart(c) })),
    {
      id: "t-mirror",
      label: "Mirror selection",
      title: "Mirror the selected symbol horizontally (FlipSymbol)",
      docs: SCH,
      menu: "Edit",
      toolbar: "edit",
      icon: "mirror",
      hint: "X",
      context: ["symbol"],
      enabled: viaEditor(d, "flip-h", () => (d.selectedSymbol ? idle() : "Select a part first")),
      run: () => runVia(d, "flip-h", () => d.selectedSymbol && d.execute({ type: "FlipSymbol", symbol_id: d.selectedSymbol.id, axis: "horizontal" })),
    },
    ...schematicEditActions(d),
    { id: "erc", label: "Run ERC", docs: SCH, menu: "Design", toolbar: "checks", icon: "erc", enabled: idle, run: () => d.check("erc") },
    { id: "annotate", label: "Annotate schematic", docs: SCH, menu: "Design", enabled: idle, run: () => d.execute({ type: "Annotate" }) },
    { id: "annotate-options", label: "Annotate options…", docs: SCH, menu: "Design", enabled: idle, run: () => d.dialog("annotate") },
    { id: "sim", label: "Run simulation…", short: "Simulate", title: "Open the Simulation panel (ngspice)", docs: SCH, menu: "Simulation", toolbar: "checks", icon: "simulate", enabled: () => true, run: () => d.showBottom("sim") },
  ];
}

/** PCB editor: Place, Route, Design (DRC, DFM, placement, zones). */
export function pcbActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const tool = (id: string, label: string, title: string, menu: "Place" | "Route", icon: IconName, split?: SplitId, more: Partial<Action> = {}): Action => ({ id: `t-${id}`, label, title, docs: PCB, menu, toolbar: "edit", icon, split, ...more, enabled: () => true, run: () => d.tool(id) });
  const fp = d.fpOfSelected;
  return [
    tool("via", "Via", "Place a via (AddVia)", "Place", "via"),
    tool("outline", "Board outline", "Draw the board outline (SetBoardOutline)", "Place", "outline", "pcb-regions"),
    tool("zone", "Copper zone", "Draw a copper zone (AddZone). Its fill is derived from the outline; Design › Refill zones recomputes connectivity and DRC", "Place", "zone", "pcb-regions"),
    tool("region", "Keep-out region", "Draw a keep-out or track-width constraint region (AddRegion); DRC-021 / DRC-022 check it", "Place", "keepout", "pcb-regions"),
    tool("cutout", "Board cut-out", "Draw a hole milled through the board, inside the outline (AddBoardCutout)", "Place", "outline", undefined, { toolbar: undefined }),
    tool("text", "PCB text", "Place text on a silkscreen or documentation layer; click a text to move it (AddPcbText, MovePcbText)", "Place", "text", undefined, { id: "t-pcb-text", sep: true }),
    tool("dimension", "Dimension", "Measure between two clicked points (AddDimension)", "Place", "dimension"),
    {
      id: "t-flip",
      label: "Flip footprint",
      title: "Move the selected footprint to the other board side (MoveFootprint)",
      docs: PCB,
      menu: "Edit",
      toolbar: "edit",
      icon: "flip-side",
      enabled: () => (fp ? (fp.locked ? "The footprint is locked" : idle()) : "Select a part that has a footprint first"),
      run: () => fp && d.execute({ type: "MoveFootprint", footprint_id: fp.id, position: fp.position, rotation: null, side: fp.side === "top" ? "bottom" : "top" }),
    },
    tool("route", "Route track", "Route a track with the chosen width and layer (AddTrack)", "Route", "route"),
    { id: "route", label: "Auto-route all nets", short: "Auto-route", title: "Auto-route all nets", docs: PCB, menu: "Route", sep: true, toolbar: "checks", icon: "autoroute", enabled: idle, run: () => d.execute({ type: "AutoRoute", nets: null }) },
    { id: "clear", label: "Clear all routing", docs: PCB, menu: "Route", destructive: true, enabled: idle, run: () => d.execute({ type: "ClearRouting", net: null }) },
    { id: "drc", label: "Run DRC", docs: PCB, menu: "Design", toolbar: "checks", icon: "drc", split: "pcb-checks", enabled: idle, run: () => d.check("drc") },
    { id: "dfm", label: "Run DFM (Fabricator standard)", short: "Run DFM", title: "Design for manufacture against the Fabricator standard profile", docs: PCB, menu: "Design", toolbar: "checks", icon: "dfm", split: "pcb-checks", enabled: idle, run: () => d.check("dfm") },
    { id: "autoplace", label: "Auto-place footprints", docs: PCB, menu: "Design", sep: true, enabled: idle, run: () => d.execute({ type: "AutoPlace", grid: null }) },
    {
      id: "t-refill",
      label: "Refill zones",
      title: "Recompute zone connectivity (islands, airwires) and re-run DRC for this revision; the fill is derived, nothing is changed",
      docs: PCB,
      menu: "Design",
      toolbar: "edit",
      icon: "refill",
      enabled: () => (Object.keys(d.snapshot.project.pcb.zones).length === 0 ? "No copper zones to refill" : idle()),
      run: d.refillZones,
    },
    {
      id: "stitch-zones",
      label: "Stitch zones",
      title: "Join every net's pours across copper layers with stitching vias, and reach SMD pads the fill misses (StitchZones, all nets)",
      docs: PCB,
      menu: "Design",
      enabled: () => stitchBlocked(d.snapshot, null) ?? idle(),
      run: () => d.execute({ type: "StitchZones", net: null }),
    },
  ];
}

/** The board as a whole (PCB editor and 3D view): manufacturing outputs. */
export function boardActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  return [
    { id: "export", label: "Export manufacturing files…", short: "Export manufacturing files", title: "Gerber, Excellon, BOM, CPL, fab-pack ZIP; runs ERC/DRC/MECH/DFM", docs: BOARD, menu: "Manufacturing", toolbar: "mfg", icon: "export", enabled: idle, run: () => d.dialog("export") },
    // E7: read-only view of the release gate (saved ERC/DRC/MECH/DFM reports and gates.rs); opening it runs nothing.
    { id: "release-check", label: "Release check…", short: "Release check", title: "Every release gate of this revision (PASS, FAIL, UNKNOWN, NOT_RUN or disabled); outputs stay DRAFT unless all required gates pass", docs: BOARD, menu: "Manufacturing", enabled: () => true, run: () => d.dialog("release") },
    { id: "show-mfg-panel", label: "Show manufacturing panel", docs: BOARD, menu: "Manufacturing", enabled: () => true, run: () => d.showSide("manufacturing") },
  ];
}

/**
 * The 3D document (slices C6, E6): camera presets, projection, zoom and reset, the measure tool,
 * placeholder labels, model reload and cross-probing into the editors. Every action drives the
 * mounted viewer's published controls (`viewer3d/controls.ts`), the same functions as its toolbar.
 */
export function view3dActions(d: ActionDeps): Action[] {
  const v = d.view3d ?? null;
  const has = (): true | string => (v ? true : "The 3D view is not drawing (no WebGL in this window)");
  const probe = (kind: "pcb" | "schematic") => (): true | string =>
    !v ? has() : !d.selectedSymbol ? "Select a part first (click it in the 3D view)" : kind === "pcb" && !d.fpOfSelected ? "The selected part has no PCB footprint (run Update PCB first)" : d.probeTo ? true : "Cross-probing is not available here";
  return [
    ...PRESETS.map<Action>((p, i) => ({ id: `v3d-${p.id}`, label: p.id === "iso" ? "Isometric view (fit)" : `${p.label} view`, title: p.title, docs: V3D, menu: "View", sep: i === 0, enabled: has, run: () => v?.preset(p.id) })),
    { id: "v3d-reset", label: "Reset view", title: "Isometric, perspective and fit to the board; clears any measurement", docs: V3D, menu: "View", toolbar: "view", icon: "fit", enabled: has, run: () => v?.reset() },
    { id: "v3d-zout", label: "Zoom out", docs: V3D, menu: "View", toolbar: "view", icon: "zoom-out", enabled: has, run: () => v?.zoomBy(1.25) },
    { id: "v3d-zin", label: "Zoom in", docs: V3D, menu: "View", toolbar: "view", icon: "zoom-in", enabled: has, run: () => v?.zoomBy(1 / 1.25) },
    { id: "v3d-ortho", label: "Orthographic projection", title: "Switch between perspective and orthographic projection", docs: V3D, menu: "View", sep: true, checked: v?.projection === "orthographic", enabled: has, run: () => { if (v) v.setProjection(v.projection === "orthographic" ? "perspective" : "orthographic"); } },
    { id: "v3d-labels", label: "Placeholder labels", title: "Label every part that is drawn without a loaded 3D model", docs: V3D, menu: "View", checked: v?.labels, enabled: has, run: () => v?.toggleLabels() },
    { id: "v3d-probe-pcb", label: "Show selection in PCB", docs: V3D, menu: "View", sep: true, enabled: probe("pcb"), run: () => d.probeTo?.("pcb") },
    { id: "v3d-probe-sch", label: "Show selection in schematic", docs: V3D, menu: "View", enabled: probe("schematic"), run: () => d.probeTo?.("schematic") },
    { id: "v3d-measure", label: "Measure distance", title: "Click two points on the board or a part; the distance is shown in mm", docs: V3D, menu: "Tools", sep: true, toolbar: "edit", icon: "dimension", checked: v?.measuring, enabled: has, run: () => v?.toggleMeasure() },
    { id: "v3d-measure-clear", label: "Clear measurement", docs: V3D, menu: "Tools", enabled: () => (!v ? has() : v.measured ? true : "No measurement on screen"), run: () => v?.clearMeasure() },
    { id: "v3d-reload", label: "Reload 3D models", title: "Re-read every attached model file (after you replaced one on disk)", docs: V3D, menu: "Tools", enabled: () => (!v ? has() : v.modelCount > 0 ? true : "No part references a 3D model file"), run: () => v?.reloadModels() },
  ];
}

/** Every action of the workspace; the registry picks the ones of the active document. */
export const workspaceActions = (d: ActionDeps): Action[] => [...commonActions(d), ...designActions(d), ...schematicActions(d), ...pcbActions(d), ...boardActions(d), ...view3dActions(d)];

/**
 * Schematic editing set (slice B3): clipboard, arrange, find and net highlight. Each entry runs the
 * active schematic editor's own operation on its multi-selection (one engine command, a `Batch`
 * for several objects, so one revision and one undo step); without a schematic editor it is
 * disabled and says so. The keys are the editor's own (it owns the canvas focus): shown, never
 * bound twice.
 */
export function schematicEditActions(d: ActionDeps): Action[] {
  const OBJ = ["symbol", "wire", "label"] as const;
  const op = (id: string, label: string, more: Partial<Action> = {}): Action => ({
    id,
    label,
    docs: SCH,
    menu: "Edit",
    ...more,
    enabled: () => d.editorOp?.(id)?.enabled() ?? NO_EDITOR,
    run: () => d.editorOp?.(id)?.run(),
  });
  return [
    op("flip-v", "Mirror selection vertically", { title: "Mirror the selected symbols top to bottom (FlipSymbol)", hint: "Y", context: ["symbol"] }),
    op("cut", "Cut", { sep: true, hint: "Mod+X", context: OBJ, contextSep: true }),
    op("copy", "Copy", { hint: "Mod+C", context: OBJ }),
    op("paste", "Paste", { title: "Paste at the pointer (another sheet keeps nets by label name only)", hint: "Mod+V", context: ["sheet"] }),
    op("duplicate", "Duplicate", { hint: "Mod+D", context: OBJ }),
    op("select-all", "Select all", { title: "Select every object the selection filter allows on this sheet", hint: "Mod+A", context: ["sheet"] }),
    op("find-similar", "Find similar objects", { title: "Select parts of the same library component, labels of the same name, or every wire", context: OBJ, contextSep: true }),
    op("find", "Find and replace…", { title: "Find designators, values and net labels; replace as one Batch", hint: "Mod+F", context: ["sheet"] }),
    op("highlight-net", "Highlight net", { sep: true, title: "Highlight the net of the selected wire or net label (engine netlist)", context: ["wire", "label"], contextSep: true }),
    op("clear-highlight", "Clear net highlight", { context: ["sheet", "wire", "label"] }),
    op("align-left", "Align left edges", { sep: true, context: ["symbol"], contextSep: true }),
    op("align-right", "Align right edges", { context: ["symbol"] }),
    op("align-top", "Align top edges", { context: ["symbol"] }),
    op("align-bottom", "Align bottom edges", { context: ["symbol"] }),
    op("align-center-x", "Align centres on a vertical line", { context: ["symbol"] }),
    op("align-center-y", "Align centres on a horizontal line", { context: ["symbol"] }),
    op("distribute-x", "Distribute horizontally", { title: "Space three or more parts evenly from left to right", context: ["symbol"] }),
    op("distribute-y", "Distribute vertically", { title: "Space three or more parts evenly from bottom to top", context: ["symbol"] }),
    op("array", "Array…", { title: "Paste copies of the selection on a grid of columns and rows (one Batch)", context: OBJ }),
    op("group", "Group", { sep: true, title: "Group the selected objects so they select and move together (Group, one undo step)", hint: "Mod+G", context: OBJ, contextSep: true }),
    op("ungroup", "Ungroup", { title: "Dissolve the groups of the selection; the objects stay where they are (Ungroup)", hint: "Mod+Shift+G", context: OBJ }),
  ];
}
