// Per-document action sets. Each operation is defined once here, with the documents it applies
// to, its menu, its toolbar group and (through the default key map) its keys; the registry
// generates every route from that. Handlers come from the workspace, which owns the state.
import { formatLength, stepPreset, UNITS, type GridConfig, type GridKind } from "../grid";
import type { SideView } from "../shell/ActivityBar";
import type { BottomTab } from "../shell/BottomPanel";
import type { Check, Command, LibraryEntry, OpenProject, RecentProject, Snapshot, SymbolInstance, FootprintInstance } from "../types";
import type { IconName } from "../design/icons/icons";
import type { Action, DocContext, SplitId } from "./types";

export interface ActionDeps {
  snapshot: Snapshot;
  busy: boolean;
  selected: string | null;
  selectedSymbol: SymbolInstance | undefined;
  fpOfSelected: FootprintInstance | undefined;
  theme: "dark" | "light";
  sideOpen: boolean;
  bottomOpen: boolean;
  split: boolean;
  recent: RecentProject[];
  library: LibraryEntry[];
  docs: { id: string; kind: string; title: string }[];
  openIds: string[];
  projects: OpenProject[];
  sideViews: { id: SideView; label: string }[];
  edaViews: { id: string; label: string; shortcut?: string }[];
  dialog: (d: "new" | "open" | "export" | "rules") => void;
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
  showBottom: (t: BottomTab) => void;
  onTheme: () => void;
  resetLayout: () => void;
  onSettings: () => void;
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
}

const EDITING: readonly DocContext[] = ["schematic", "pcb"];
const SCH: readonly DocContext[] = ["schematic"];
const PCB: readonly DocContext[] = ["pcb"];
const DESIGN: readonly DocContext[] = ["home", "schematic", "pcb"];
const BOARD: readonly DocContext[] = ["pcb", "3d"];
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
    { id: "close", label: "Close project", menu: "File", sep: true, toolbar: "app", icon: "close", enabled: idle, run: d.close },

    { id: "undo", label: "Undo", menu: "Edit", toolbar: "file", icon: "undo", enabled: () => (d.snapshot.can_undo ? idle() : "Nothing to undo"), run: d.undo },
    { id: "redo", label: "Redo", menu: "Edit", toolbar: "file", icon: "redo", enabled: () => (d.snapshot.can_redo ? idle() : "Nothing to redo"), run: d.redo },
    { id: "select", label: "Select tool", title: "Select tool (key 1)", docs: EDITING, menu: "Edit", sep: true, toolbar: "edit", icon: "select", enabled: () => true, run: () => d.tool("select") },
    { id: "move", label: "Move tool", title: "Move tool (schematic key 6, PCB key 3)", docs: EDITING, menu: "Edit", toolbar: "edit", icon: "move", enabled: () => true, run: () => d.tool("move") },
    { id: "rotate", label: "Rotate selection", title: "Rotate selection (key R)", docs: EDITING, menu: "Edit", toolbar: "edit", icon: "rotate", enabled: needSel, run: d.rotateSel },
    { id: "delete", label: "Delete selection", title: "Delete selection (asks first)", docs: EDITING, menu: "Edit", toolbar: "edit", icon: "delete", enabled: needSel, run: d.deleteSel },
    { id: "deselect", label: "Clear selection", keys: ["Escape"], menu: "Edit", enabled: () => (d.selected ? true : "Nothing is selected"), run: () => d.select(null) },

    { id: "zout", label: "Zoom out", docs: EDITING, menu: "View", toolbar: "view", icon: "zoom-out", enabled: () => true, run: () => d.zoomBy(1.4) },
    { id: "zin", label: "Zoom in", docs: EDITING, menu: "View", toolbar: "view", icon: "zoom-in", enabled: () => true, run: () => d.zoomBy(1 / 1.4) },
    { id: "fit", label: "Fit to view", title: "Fit to view (or double-click the canvas)", docs: EDITING, menu: "View", toolbar: "view", icon: "fit", enabled: () => true, run: d.fit },
    ...gridActions(d),
    { id: "toolbar-labels", label: "Toolbar labels", title: "Show text labels beside the toolbar icons", menu: "View", checked: d.toolbarLabels, enabled: () => true, run: d.toggleToolbarLabels },
    { id: "toggle-side", label: "Side panel", whileTyping: true, menu: "View", sep: true, checked: d.sideOpen, enabled: () => true, run: d.toggleSide },
    { id: "toggle-bottom", label: "Bottom panel", whileTyping: true, menu: "View", checked: d.bottomOpen, enabled: () => true, run: d.toggleBottom },
    { id: "toggle-split", label: "Split schematic and PCB", menu: "View", checked: d.split, enabled: () => true, run: d.toggleSplit },
    ...d.sideViews.map<Action>((v, i) => ({ id: `show-${v.id}`, label: `Show ${v.label}`, whileTyping: v.id === "project", menu: "View", sep: i === 0, checked: d.sideOpen && d.sideView === v.id, enabled: () => true, run: () => d.showSide(v.id) })),
    { id: "bom", label: "Show BOM", menu: "View", sep: true, enabled: () => true, run: () => d.showBottom("bom") },
    { id: "log", label: "Show log", menu: "View", enabled: () => true, run: () => d.showBottom("log") },
    { id: "ws-schematic", label: "Go to Schematic editor", whileTyping: true, menu: "View", sep: true, enabled: () => true, run: () => d.showWorkspace("schematic") },
    { id: "ws-pcb", label: "Go to PCB editor", whileTyping: true, menu: "View", enabled: () => true, run: () => d.showWorkspace("pcb") },
    ...d.edaViews.map<Action>((w) => ({ id: `ws-${w.id}`, label: `Go to ${w.label}`, keys: w.shortcut ? [w.shortcut] : [], whileTyping: true, menu: "View", enabled: () => true, run: () => d.showWorkspace(w.id) })),
    { id: "reset-layout", label: "Reset panel layout", menu: "View", sep: true, enabled: () => true, run: d.resetLayout },

    ...d.docs.map<Action>((doc) => ({ id: `open-${doc.id}`, label: `Open ${doc.kind === "pcb" ? "PCB" : doc.kind === "file" ? `file ${doc.title}` : `sheet ${doc.title}`}`, menu: "Project", paletteGroup: "Documents", enabled: () => true, run: () => d.open(doc.id) })),
    { id: "close-all", label: "Close all documents", menu: "Project", paletteGroup: "Documents", enabled: () => (d.openIds.length === 0 ? "No documents are open" : true), run: d.closeAll },
    ...d.projects.filter((p) => !p.active).map<Action>((p, i) => ({ id: `proj-${p.id}`, label: `Switch to project ${p.name}`, menu: "Project", sep: i === 0, enabled: idle, run: () => d.switchTo(p.id) })),

    { id: "palette", label: "Command palette", short: "Commands", whileTyping: true, menu: "Tools", toolbar: "app", icon: "palette", palette: false, enabled: () => true, run: d.palette },
    { id: "quick-open", label: "Go to part, net or sheet…", whileTyping: true, menu: "Tools", enabled: () => true, run: d.quickOpen },

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
  return [
    tool("place", "Place part", "Place a part from the library (editor Place tool)", "part"),
    ...d.library.map<Action>((c, i) => ({ id: `part-${c.id}`, label: `Part: ${c.name}`, docs: SCH, menu: "Place", sep: i === 0, palette: false, enabled: () => true, run: () => d.placePart(c) })),
    tool("wire", "Wire", "Draw a wire; ends snap to pins (AddWire)", "wire", { sep: true }),
    tool("label", "Net / power label", "Net label or power port (AddNetLabel; pick Net or Power in the tool options)", "net-label", { split: "sch-marks" }),
    tool("noconnect", "No connect", "Mark a pin as intentionally unconnected (SetNoConnect)", "no-connect", { split: "sch-marks" }),
    tool("pinlabel", "Label pin", "Name the net on a pin (LabelPin)", "pin-label", { split: "sch-marks" }),
    {
      id: "t-mirror",
      label: "Mirror selection",
      title: "Mirror the selected symbol horizontally (FlipSymbol)",
      docs: SCH,
      menu: "Edit",
      toolbar: "edit",
      icon: "mirror",
      enabled: () => (d.selectedSymbol ? idle() : "Select a part first"),
      run: () => d.selectedSymbol && d.execute({ type: "FlipSymbol", symbol_id: d.selectedSymbol.id, axis: "horizontal" }),
    },
    { id: "erc", label: "Run ERC", docs: SCH, menu: "Design", toolbar: "checks", icon: "erc", enabled: idle, run: () => d.check("erc") },
    { id: "annotate", label: "Annotate schematic", docs: SCH, menu: "Design", enabled: idle, run: () => d.execute({ type: "Annotate" }) },
    { id: "sim", label: "Run simulation…", short: "Simulate", title: "Open the Simulation panel (ngspice)", docs: SCH, menu: "Simulation", toolbar: "checks", icon: "simulate", enabled: () => true, run: () => d.showBottom("sim") },
  ];
}

/** PCB editor: Place, Route, Design (DRC, DFM, placement, zones). */
export function pcbActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const tool = (id: string, label: string, title: string, menu: "Place" | "Route", icon: IconName, split?: SplitId): Action => ({ id: `t-${id}`, label, title, docs: PCB, menu, toolbar: "edit", icon, split, enabled: () => true, run: () => d.tool(id) });
  const fp = d.fpOfSelected;
  return [
    tool("via", "Via", "Place a via (AddVia)", "Place", "via"),
    tool("outline", "Board outline", "Draw the board outline (SetBoardOutline)", "Place", "outline", "pcb-regions"),
    tool("zone", "Copper zone", "Draw a copper zone (AddZone). The engine has no separate refill command", "Place", "zone", "pcb-regions"),
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
  ];
}

/** The board as a whole (PCB editor and 3D view): manufacturing outputs. */
export function boardActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  return [
    { id: "export", label: "Export manufacturing files…", short: "Export manufacturing files", title: "Gerber, Excellon, BOM, CPL, fab-pack ZIP; runs ERC/DRC/MECH/DFM", docs: BOARD, menu: "Manufacturing", toolbar: "mfg", icon: "export", enabled: idle, run: () => d.dialog("export") },
    { id: "show-mfg-panel", label: "Show manufacturing panel", docs: BOARD, menu: "Manufacturing", enabled: () => true, run: () => d.showSide("manufacturing") },
  ];
}

/** Every action of the workspace; the registry picks the ones of the active document. */
export const workspaceActions = (d: ActionDeps): Action[] => [...commonActions(d), ...designActions(d), ...schematicActions(d), ...pcbActions(d), ...boardActions(d)];
