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
  /** Definitions in the project's own library (built-ins copied in, and imported parts). */
  library?: Record<
    string,
    {
      id: string;
      name: string;
      category: string;
      refdes_prefix: string;
      default_value: string;
      lcsc?: string | null;
      symbol: { pins: { number: string; name: string }[] };
    }
  >;
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
  | { type: "PlaceSymbol"; sheet_id: Id | null; component_id: string; position: Point; rotation: Rotation; refdes: string | null; value: string | null }
  | { type: "MoveSymbol"; symbol_id: Id; position: Point }
  | { type: "RotateSymbol"; symbol_id: Id; rotation: Rotation }
  | { type: "SetSymbolValue"; symbol_id: Id; value: string }
  | { type: "SetRefdes"; symbol_id: Id; refdes: string }
  | { type: "DeleteSymbol"; symbol_id: Id }
  | { type: "AddWire"; sheet_id: Id | null; a: Point; b: Point }
  | { type: "DeleteWire"; wire_id: Id }
  | { type: "ConnectPins"; a_symbol: Id; a_pin: string; b_symbol: Id; b_pin: string }
  | { type: "AddNetLabel"; sheet_id: Id | null; name: string; position: Point; kind: "net" | "power" }
  | { type: "DeleteNetLabel"; label_id: Id }
  | { type: "SetBoardOutline"; points: Point[] }
  | { type: "MoveFootprint"; footprint_id: Id; position: Point; rotation: Rotation | null; side: Side | null }
  | { type: "AddTrack"; layer: CopperLayer; a: Point; b: Point; width: Nm | null; net: string }
  | { type: "DeleteTrack"; track_id: Id }
  | { type: "AddVia"; position: Point; net: string; drill: Nm | null; diameter: Nm | null }
  | { type: "DeleteVia"; via_id: Id }
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

/** A built-in library part (`library_components` IPC). */
export interface LibraryEntry {
  id: string;
  name: string;
  category: string;
  refdes_prefix: string;
  default_value: string;
  pins: [string, string][];
}

export interface ExportFile {
  name: string;
  bytes: number;
  sha256: string;
  kind: string;
}
export interface ExportResult {
  out_dir: string;
  manifest: {
    revision: number;
    status: "DRAFT" | "INTERNALLY_VERIFIED_RELEASE_CANDIDATE";
    statement: string;
    blocking_reasons: string[];
    files: ExportFile[];
    not_verified: string[];
  };
  reports: Report[];
}

// ---- parts search, BOM and JLCPCB preparation (src-tauri/src/parts.rs) ----

export interface SourcingRecord {
  provider: string;
  source_url: string;
  retrieved_at_unix_s: number;
  raw_sha256: string;
  lcsc_part: string | null;
  mpn: string | null;
  manufacturer: string | null;
  package: string | null;
  category: string | null;
  description: string | null;
  datasheet_url: string | null;
  specs: { name: string; value: string }[];
  stock: number | null;
  price_breaks: { min_quantity: number; unit_price: string; currency: string }[];
  missing: string[];
  rejected: string[];
  state: string;
}

export interface LibraryHit {
  id: string;
  name: string;
  category: string;
  lcsc: string | null;
  mpn: string | null;
}

export type LiveStatus = "ok" | "blocked" | "not_found" | "unreachable" | "bad_query" | "error";

export interface PartsSearch {
  query: string;
  library: LibraryHit[];
  stored: SourcingRecord[];
  live: { status: LiveStatus; message: string; records: SourcingRecord[] };
}

export interface ImportResult {
  component_id: string;
  response: CommandResponse;
  notes: string[];
  record: SourcingRecord;
}

export interface BomLine {
  component_id: string;
  value: string;
  designators: string[];
  quantity: number;
  footprint: string;
  lcsc: string | null;
  manufacturer: string | null;
  mpn: string | null;
  smd: boolean;
  placed: number;
  stock: number | null;
  unit_price_usd: string | null;
  price_as_of_unix_s: number | null;
  issues: string[];
}

export type DfmStatus = "pass" | "warn" | "fail" | "not_checked";

export interface DfmCheck {
  id: string;
  title: string;
  status: DfmStatus;
  required: string;
  measured: string;
  detail: string[];
}

export interface DfmReport {
  profile: { id: string; version: string; source_url: string; verified_on: string };
  checks: DfmCheck[];
  failures: number;
  warnings: number;
  not_checked: number;
  statement: string;
  limitations: string[];
}

export interface ManufacturingPreview {
  revision: number;
  bom: BomLine[];
  dfm: DfmReport;
  default_out_dir: string | null;
}

export interface PrepareResult {
  prep: {
    out_dir: string;
    manifest: { statement: string; files: ExportFile[]; not_verified: string[] };
    dfm: DfmReport;
  };
  reports: Report[];
}
