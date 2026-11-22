// Per-document action sets. Each operation is defined once here, with the documents it applies
// to, its menu, its toolbar group and (through the default key map) its keys; the registry
// generates every route from that. Handlers come from the workspace, which owns the state.
import type { SideView } from "../shell/ActivityBar";
import type { BottomTab } from "../shell/BottomPanel";
import type { Check, Command, LibraryEntry, OpenProject, RecentProject, Snapshot, SymbolInstance, FootprintInstance } from "../types";
import type { Action, DocContext } from "./types";

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
  notify: (m: string) => void;
  open: (docId: string) => void;
  closeAll: () => void;
  showWorkspace: (id: string) => void;
  switchTo: (projectId: string) => void;
  placePart: (c: LibraryEntry) => void;
}

const EDITING: readonly DocContext[] = ["schematic", "pcb"];
const SCH: readonly DocContext[] = ["schematic"];
const PCB: readonly DocContext[] = ["pcb"];
const DESIGN: readonly DocContext[] = ["home", "schematic", "pcb"];
const BOARD: readonly DocContext[] = ["pcb", "3d"];
const never = () => {};

/** File, Edit, View, Project, Tools, Settings and Help: the chrome every document shares. */
export function commonActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const needSel = () => (d.selectedSymbol ? idle() : "Select a part first");
  const recent = d.recent.filter((r) => r.project_id !== d.snapshot.project.id).slice(0, 5);
  return [
    { id: "new", label: "New project…", short: "New project", menu: "File", toolbar: "file", enabled: idle, run: () => d.dialog("new") },
    { id: "open", label: "Open project…", short: "Open project", menu: "File", toolbar: "file", enabled: idle, run: () => d.dialog("open") },
    ...recent.map<Action>((r, i) => ({ id: `recent-${r.project_id}`, label: `Recent: ${r.name}`, menu: "File", sep: i === 0, palette: false, enabled: idle, run: () => d.openRecent(r.path) })),
    { id: "save", label: "Save", menu: "File", sep: true, toolbar: "file", whileTyping: true, enabled: idle, run: d.save },
    { id: "saveas", label: "Save as…", short: "Save as", menu: "File", toolbar: "file", enabled: () => "Not implemented: the engine saves the project in place", run: never },
    { id: "close", label: "Close project", menu: "File", sep: true, toolbar: "app", enabled: idle, run: d.close },

    { id: "undo", label: "Undo", menu: "Edit", toolbar: "file", enabled: () => (d.snapshot.can_undo ? idle() : "Nothing to undo"), run: d.undo },
    { id: "redo", label: "Redo", menu: "Edit", toolbar: "file", enabled: () => (d.snapshot.can_redo ? idle() : "Nothing to redo"), run: d.redo },
    { id: "select", label: "Select tool", title: "Select tool (key 1)", docs: EDITING, menu: "Edit", sep: true, toolbar: "edit", enabled: () => true, run: () => d.tool("select") },
    { id: "move", label: "Move tool", title: "Move tool (schematic key 6, PCB key 3)", docs: EDITING, menu: "Edit", toolbar: "edit", enabled: () => true, run: () => d.tool("move") },
    { id: "rotate", label: "Rotate selection", title: "Rotate selection (key R)", docs: EDITING, menu: "Edit", toolbar: "edit", enabled: needSel, run: d.rotateSel },
    { id: "delete", label: "Delete selection", title: "Delete selection (asks first)", docs: EDITING, menu: "Edit", toolbar: "edit", enabled: needSel, run: d.deleteSel },
    { id: "deselect", label: "Clear selection", keys: ["Escape"], menu: "Edit", enabled: () => (d.selected ? true : "Nothing is selected"), run: () => d.select(null) },

    { id: "zout", label: "Zoom out", docs: EDITING, menu: "View", toolbar: "view", enabled: () => true, run: () => d.zoomBy(1.4) },
    { id: "zin", label: "Zoom in", docs: EDITING, menu: "View", toolbar: "view", enabled: () => true, run: () => d.zoomBy(1 / 1.4) },
    { id: "fit", label: "Fit to view", title: "Fit to view (or double-click the canvas)", docs: EDITING, menu: "View", toolbar: "view", enabled: () => true, run: d.fit },
    { id: "grid", label: "Grid", docs: EDITING, menu: "View", toolbar: "view", enabled: () => "Not implemented: the grid is fixed (schematic 1.27 mm, PCB 0.5 mm)", run: never },
    { id: "toggle-side", label: "Side panel", whileTyping: true, menu: "View", sep: true, checked: d.sideOpen, enabled: () => true, run: d.toggleSide },
    { id: "toggle-bottom", label: "Bottom panel", whileTyping: true, menu: "View", checked: d.bottomOpen, enabled: () => true, run: d.toggleBottom },
    { id: "toggle-split", label: "Split schematic and PCB", menu: "View", checked: d.split, enabled: () => true, run: d.toggleSplit },
    ...d.sideViews.map<Action>((v, i) => ({ id: `show-${v.id}`, label: `Show ${v.label}`, whileTyping: v.id === "project", menu: "View", sep: i === 0, enabled: () => true, run: () => d.showSide(v.id) })),
    { id: "bom", label: "Show BOM", menu: "View", sep: true, enabled: () => true, run: () => d.showBottom("bom") },
    { id: "log", label: "Show log", menu: "View", enabled: () => true, run: () => d.showBottom("log") },
    { id: "ws-schematic", label: "Go to Schematic editor", whileTyping: true, menu: "View", sep: true, enabled: () => true, run: () => d.showWorkspace("schematic") },
    { id: "ws-pcb", label: "Go to PCB editor", whileTyping: true, menu: "View", enabled: () => true, run: () => d.showWorkspace("pcb") },
    ...d.edaViews.map<Action>((w) => ({ id: `ws-${w.id}`, label: `Go to ${w.label}`, keys: w.shortcut ? [w.shortcut] : [], whileTyping: true, menu: "View", enabled: () => true, run: () => d.showWorkspace(w.id) })),
    { id: "reset-layout", label: "Reset panel layout", menu: "View", sep: true, enabled: () => true, run: d.resetLayout },

    ...d.docs.map<Action>((doc) => ({ id: `open-${doc.id}`, label: `Open ${doc.kind === "pcb" ? "PCB" : `sheet ${doc.title}`}`, menu: "Project", paletteGroup: "Documents", enabled: () => true, run: () => d.open(doc.id) })),
    { id: "close-all", label: "Close all documents", menu: "Project", paletteGroup: "Documents", enabled: () => (d.openIds.length === 0 ? "No documents are open" : true), run: d.closeAll },
    ...d.projects.filter((p) => !p.active).map<Action>((p, i) => ({ id: `proj-${p.id}`, label: `Switch to project ${p.name}`, menu: "Project", sep: i === 0, enabled: idle, run: () => d.switchTo(p.id) })),

    { id: "palette", label: "Command palette", short: "Commands", whileTyping: true, menu: "Tools", toolbar: "app", palette: false, enabled: () => true, run: d.palette },
    { id: "quick-open", label: "Go to part, net or sheet…", whileTyping: true, menu: "Tools", enabled: () => true, run: d.quickOpen },

    { id: "settings", label: "Model settings", menu: "Settings", enabled: () => true, run: d.onSettings },
    { id: "theme", label: "Light theme", menu: "Settings", checked: d.theme === "light", enabled: () => true, run: d.onTheme },

    { id: "help", label: "Keyboard shortcuts", menu: "Help", enabled: () => true, run: d.help },
    {
      id: "about",
      label: "About EAK",
      menu: "Help",
      enabled: () => true,
      run: () => d.notify("EAK: Rust engineering core, typed commands, independent validation. AI output never certifies engineering numbers."),
    },
  ];
}

/** Checks and rules shared by the schematic, the PCB and the project home. */
export function designActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  return [
    { id: "sync", label: "Update PCB from schematic", short: "Update PCB", docs: DESIGN, menu: "Design", toolbar: "checks", enabled: idle, run: d.syncPcb },
    { id: "rules", label: "Design rules and net classes…", short: "Design rules", title: "Design rules and net classes", docs: DESIGN, menu: "Design", sep: true, toolbar: "checks", enabled: () => true, run: () => d.dialog("rules") },
  ];
}

/** Schematic editor: Place, Design (ERC, annotate) and Simulation. */
export function schematicActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const tool = (id: string, label: string, title: string, sep = false): Action => ({ id: `t-${id}`, label, title, docs: SCH, menu: "Place", sep, toolbar: "edit", enabled: () => true, run: () => d.tool(id) });
  return [
    tool("place", "Place part", "Place a part from the library (editor Place tool)"),
    ...d.library.map<Action>((c, i) => ({ id: `part-${c.id}`, label: `Part: ${c.name}`, docs: SCH, menu: "Place", sep: i === 0, palette: false, enabled: () => true, run: () => d.placePart(c) })),
    tool("wire", "Wire", "Draw a wire; ends snap to pins (AddWire)", true),
    tool("label", "Net / power label", "Net label or power port (AddNetLabel; pick Net or Power in the tool options)"),
    tool("noconnect", "No connect", "Mark a pin as intentionally unconnected (SetNoConnect)"),
    tool("pinlabel", "Label pin", "Name the net on a pin (LabelPin)"),
    { id: "t-junction", label: "Junction", docs: SCH, menu: "Place", toolbar: "edit", enabled: () => "Junctions are added automatically where wires meet", run: never },
    {
      id: "t-mirror",
      label: "Mirror selection",
      title: "Mirror the selected symbol horizontally (FlipSymbol)",
      docs: SCH,
      menu: "Edit",
      toolbar: "edit",
      enabled: () => (d.selectedSymbol ? idle() : "Select a part first"),
      run: () => d.selectedSymbol && d.execute({ type: "FlipSymbol", symbol_id: d.selectedSymbol.id, axis: "horizontal" }),
    },
    { id: "erc", label: "Run ERC", docs: SCH, menu: "Design", toolbar: "checks", enabled: idle, run: () => d.check("erc") },
    { id: "annotate", label: "Annotate schematic", docs: SCH, menu: "Design", enabled: idle, run: () => d.execute({ type: "Annotate" }) },
    { id: "sim", label: "Run simulation…", short: "Simulate", title: "Open the Simulation panel (ngspice)", docs: SCH, menu: "Simulation", toolbar: "checks", enabled: () => true, run: () => d.showBottom("sim") },
  ];
}

/** PCB editor: Place, Route, Design (DRC, DFM, placement, zones). */
export function pcbActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  const tool = (id: string, label: string, title: string, menu: "Place" | "Route"): Action => ({ id: `t-${id}`, label, title, docs: PCB, menu, toolbar: "edit", enabled: () => true, run: () => d.tool(id) });
  const fp = d.fpOfSelected;
  return [
    tool("via", "Via", "Place a via (AddVia)", "Place"),
    tool("outline", "Board outline", "Draw the board outline (SetBoardOutline)", "Place"),
    tool("zone", "Copper zone", "Draw a copper zone (AddZone). The engine has no separate refill command", "Place"),
    {
      id: "t-flip",
      label: "Flip footprint",
      title: "Move the selected footprint to the other board side (MoveFootprint)",
      docs: PCB,
      menu: "Edit",
      toolbar: "edit",
      enabled: () => (fp ? (fp.locked ? "The footprint is locked" : idle()) : "Select a part that has a footprint first"),
      run: () => fp && d.execute({ type: "MoveFootprint", footprint_id: fp.id, position: fp.position, rotation: null, side: fp.side === "top" ? "bottom" : "top" }),
    },
    tool("route", "Route track", "Route a track with the chosen width and layer (AddTrack)", "Route"),
    { id: "route", label: "Auto-route all nets", short: "Auto-route", title: "Auto-route all nets", docs: PCB, menu: "Route", sep: true, toolbar: "checks", enabled: idle, run: () => d.execute({ type: "AutoRoute", nets: null }) },
    { id: "clear", label: "Clear all routing", docs: PCB, menu: "Route", destructive: true, enabled: idle, run: () => d.execute({ type: "ClearRouting", net: null }) },
    { id: "drc", label: "Run DRC", docs: PCB, menu: "Design", toolbar: "checks", enabled: idle, run: () => d.check("drc") },
    { id: "dfm", label: "Run DFM (Fabricator standard)", short: "Run DFM", title: "Design for manufacture against the Fabricator standard profile", docs: PCB, menu: "Design", toolbar: "checks", enabled: idle, run: () => d.check("dfm") },
    { id: "autoplace", label: "Auto-place footprints", docs: PCB, menu: "Design", sep: true, enabled: idle, run: () => d.execute({ type: "AutoPlace", grid: null }) },
    {
      id: "t-refill",
      label: "Refill zones",
      title: "Recompute zone connectivity (islands, airwires) and re-run DRC for this revision; the fill is derived, nothing is changed",
      docs: PCB,
      menu: "Design",
      toolbar: "edit",
      enabled: () => (Object.keys(d.snapshot.project.pcb.zones).length === 0 ? "No copper zones to refill" : idle()),
      run: d.refillZones,
    },
  ];
}

/** The board as a whole (PCB editor and 3D view): manufacturing outputs. */
export function boardActions(d: ActionDeps): Action[] {
  const idle = () => (d.busy ? "An engine command is running" : true);
  return [
    { id: "export", label: "Export manufacturing files…", short: "Export manufacturing files", title: "Gerber, Excellon, BOM, CPL, fab-pack ZIP; runs ERC/DRC/MECH/DFM", docs: BOARD, menu: "Manufacturing", toolbar: "mfg", enabled: idle, run: () => d.dialog("export") },
    { id: "show-mfg-panel", label: "Show manufacturing panel", docs: BOARD, menu: "Manufacturing", enabled: () => true, run: () => d.showSide("manufacturing") },
    { id: "release", label: "Release check…", docs: BOARD, menu: "Manufacturing", enabled: () => "Not implemented: exports are always DRAFT", run: never },
  ];
}

/** Every action of the workspace; the registry picks the ones of the active document. */
export const workspaceActions = (d: ActionDeps): Action[] => [...commonActions(d), ...designActions(d), ...schematicActions(d), ...pcbActions(d), ...boardActions(d)];
