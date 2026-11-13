// Wire types for the Tauri IPC boundary. They describe the JSON produced by the Rust
// shell (apps/desktop/src-tauri) from eak-core / eak-runtime / eak-validation types.
// The UI only renders these values; it never edits them or keeps its own project model.

export type Id = string;
/** Signed nanometres (EAK-IR integer geometry). */
export type Nm = number;
export interface Point {
  x: Nm;
  y: Nm;
}
export type Rotation = "R0" | "R90" | "R180" | "R270";
export type Side = "top" | "bottom";
export type CopperLayer = "top" | "bottom";

export interface Sheet {
  id: Id;
  name: string;
  width: Nm;
  height: Nm;
}
export interface SymbolInstance {
  id: Id;
  sheet_id: Id;
  component_id: string;
  refdes: string;
  value: string;
  position: Point;
  rotation: Rotation;
}
export interface Wire {
  id: Id;
  sheet_id: Id;
  a: Point;
  b: Point;
}
export interface NetLabel {
  id: Id;
  sheet_id: Id;
  name: string;
  position: Point;
  kind: "net" | "power";
}
export interface NoConnect {
  id: Id;
  symbol_id: Id;
  pin: string;
}
export interface FootprintInstance {
  id: Id;
  symbol_id: Id;
  component_id: string;
  refdes: string;
  position: Point;
  rotation: Rotation;
  side: Side;
  locked: boolean;
}
export interface Track {
  id: Id;
  layer: CopperLayer;
  width: Nm;
  a: Point;
  b: Point;
  net: string;
}
export interface Via {
  id: Id;
  position: Point;
  drill: Nm;
  diameter: Nm;
  net: string;
}
export interface Zone {
  id: Id;
  layer: CopperLayer;
  net: string;
  outline: Point[];
  clearance: Nm;
}

/** Canonical EAK-IR project (fields the shell renders). */
export interface Project {
  schema_version: number;
  id: Id;
  name: string;
  revision: number;
  schematic: {
    sheets: Record<Id, Sheet>;
    symbols: Record<Id, SymbolInstance>;
    wires: Record<Id, Wire>;
    labels: Record<Id, NetLabel>;
    no_connects: Record<Id, NoConnect>;
  };
  pcb: {
    board: { outline: Point[] };
    footprints: Record<Id, FootprintInstance>;
    tracks: Record<Id, Track>;
    vias: Record<Id, Via>;
    zones: Record<Id, Zone>;
  };
}

export interface PinGeom {
  number: string;
  name: string;
  position: Point;
}
export interface SymbolGeom {
  id: Id;
  sheet_id: Id;
  refdes: string;
  value: string;
  position: Point;
  half_w: Nm;
  half_h: Nm;
  pins: PinGeom[];
}
export interface PadGeom {
  number: string;
  kind: "smd" | "tht";
  shape: "rect" | "circle" | "oval";
  position: Point;
  size_x: Nm;
  size_y: Nm;
  drill: Nm | null;
  net: string | null;
}
export interface FootprintGeom {
  id: Id;
  symbol_id: Id;
  refdes: string;
  side: Side;
  position: Point;
  half_w: Nm;
  half_h: Nm;
  pads: PadGeom[];
}

export interface Snapshot {
  project: Project;
  dir: string | null;
  can_undo: boolean;
  can_redo: boolean;
  blocked: string | null;
  recovery: { reconciled: [string, string][]; torn_temp_file_removed: boolean };
  geometry: { symbols: SymbolGeom[]; footprints: FootprintGeom[] };
}

/** Argument-free engine commands offered by the command palette (eak-runtime `Command`). */
export type Command =
  | { type: "Annotate" }
  | { type: "SyncSchematicToPcb" }
  | { type: "AutoPlace"; grid: Nm | null }
  | { type: "AutoRoute"; nets: string[] | null }
  | { type: "ClearRouting"; net: string | null };

export interface CommandError {
  code: string;
  message: unknown;
}
export interface CommandResponse {
  op_id: string;
  command: string;
  class: string;
  accepted: boolean;
  revision_before: number;
  revision_after: number;
  created: Id[];
  affected: string[];
  warnings: string[];
  error: CommandError | null;
  validation_note: string;
}

export interface EngineEvent {
  seq: number;
  op_id: string;
  kind: string;
  actor: string;
  summary: string;
  revision: number;
  timestamp_ms: number;
}

export type Severity = "info" | "warning" | "error";
export interface Finding {
  rule_id: string;
  severity: Severity;
  message: string;
  objects: Id[];
  location: Point | null;
}
export interface Report {
  check: string;
  project_id: Id;
  revision: number;
  input_sha256: string;
  tool: string;
  findings: Finding[];
  outcome: "NO_ERRORS_IN_IMPLEMENTED_CHECKS" | "ERRORS";
  errors: number;
  warnings: number;
  not_implemented: string[];
  limitations: string[];
}
export type Check = "erc" | "drc";
export interface CheckResult {
  report: Report;
  evidence_path: string | null;
}

export interface RecentProject {
  path: string;
  project_id: Id;
  name: string;
  revision: number;
  last_opened_ms: number;
}
