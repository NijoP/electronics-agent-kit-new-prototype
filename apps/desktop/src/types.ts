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
/** "top", "bottom", or the n-th inner layer from the top (1-based). */
export type CopperLayer = "top" | "bottom" | { inner: number };
export interface StackupLayer {
  layer: CopperLayer;
  kind?: "signal" | "plane";
  copper_oz: number;
  dielectric_after_um: number;
  er: number;
}
export interface Stackup {
  layers: StackupLayer[];
  board_thickness: Nm;
  dielectric: string;
}

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
  /** Barrel span; absent in older projects = through (top to bottom). */
  from?: CopperLayer;
  to?: CopperLayer;
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
    board: { outline: Point[]; stackup?: Stackup };
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
  | { type: "AddVia"; position: Point; net: string; drill: Nm | null; diameter: Nm | null; from?: CopperLayer | null; to?: CopperLayer | null }
  | { type: "AddZone"; layer: CopperLayer; net: string; outline: Point[]; clearance: Nm | null }
  | { type: "DeleteZone"; zone_id: Id }
  | { type: "SetLayerCount"; count: number; board_thickness?: Nm | null }
  | { type: "SetStackup"; stackup: Stackup }
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

// Model settings (src-tauri/src/models.rs). The UI never receives an API key, only whether one
// is stored in the OS credential store.
export type ProviderKind = "anthropic" | "open_ai_compatible" | "ollama";
export interface ProviderProfile {
  id: string;
  label: string;
  kind: ProviderKind;
  base_url: string | null;
  model: string | null;
}
export interface ProviderView extends ProviderProfile {
  has_key: boolean;
  needs_key: boolean;
  key_error: string | null;
}
export interface ProfileInput {
  id: string | null;
  label: string;
  kind: ProviderKind;
  base_url: string | null;
  model: string | null;
}
export interface ConnectionReport {
  ok: boolean;
  latency_ms: number;
  message: string;
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

// ---- Domain intelligence (`domain_classification` IPC; crates/eak-intelligence/src/domain)

/** DESK evidence state; EAK never claims more for a class than this label allows. */
export type Maturity =
  | "DRAFT"
  | "ADVISORY"
  | "IMPLEMENTED"
  | "OFFLINE_TESTED"
  | "VERIFIED_AGAINST_SOURCE"
  | "LIVE_VERIFIED"
  | "NEEDS_HARDWARE_VALIDATION"
  | "BLOCKED";

export interface MaturityCeiling {
  label: Maturity;
  claim: string;
  may_present_as_requirement: boolean;
  may_show_guidance: boolean;
}

export interface DomainCandidate {
  archetype_id: string;
  name: string;
  domain: string;
  domain_name: string;
  description: string;
  score: number;
  reasons: string[];
  penalties: string[];
  maturity: Maturity;
  ceiling: MaturityCeiling;
}

export interface DomainGuidance {
  archetype_id: string;
  ceiling: MaturityCeiling;
  heading: string;
  critical_electrical_constraints: string[];
  verification_requirements: string[];
  required_component_roles: string[];
  applicable_standards: string[];
  unsupported_conditions: string[];
  desk_rule_ids: string[];
  maturity_evidence: string;
}

export interface DomainClassification {
  status: "CLASSIFIED" | "REVIEW_REQUIRED" | "UNCLASSIFIED";
  confidence: number;
  primary: string | null;
  secondary: string[];
  candidates: DomainCandidate[];
  overlays: string[];
  required_component_families: string[];
  missing_information: string[];
  conflicts: string[];
  evidence: string[];
  guidance: DomainGuidance | null;
  registry_classes: number;
  not_implemented: string[];
  limitations: string[];
}

export interface DomainReport {
  revision: number;
  classification: DomainClassification;
}
