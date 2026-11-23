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
/** Mirrors `eak_core::ManufacturingConfig`. */
export interface ManufacturingConfig {
  fabricator_profile: string;
  surface_finish: string;
  solder_mask_color: string;
  mask_expansion: Nm;
}

// ---- Schema v3 schematic primitives (ADR-0040). Optional: the UI does not draw them yet.
export type LineStyle = "solid" | "dashed" | "dotted" | "dash_dot";
export interface Stroke {
  width: Nm;
  style: LineStyle;
}
export type Graphic =
  | { kind: "line"; a: Point; b: Point; stroke?: Stroke }
  | { kind: "polyline"; points: Point[]; stroke?: Stroke }
  | { kind: "arc"; center: Point; radius: Nm; start_deg: number; end_deg: number; stroke?: Stroke }
  | { kind: "circle"; center: Point; radius: Nm; fill?: boolean; stroke?: Stroke }
  | { kind: "rectangle"; min: Point; max: Point; fill?: boolean; stroke?: Stroke }
  | { kind: "polygon"; points: Point[]; fill?: boolean; stroke?: Stroke }
  | { kind: "text"; position: Point; text: string; size: Nm; rotation?: Rotation };
export type FieldSource =
  | { source: "project_name" | "project_revision" | "sheet_name" | "sheet_number" | "sheet_count" }
  | { source: "property"; key: string }
  | { source: "text"; text: string };
export interface DrawingFormat {
  border: boolean;
  zone_columns: number;
  zone_rows: number;
  title_block: { label: string; value: FieldSource }[];
}
export interface SheetGraphic {
  id: Id;
  sheet_id: Id;
  graphic: Graphic;
}
export interface Bus {
  id: Id;
  sheet_id: Id;
  name: string;
  points: Point[];
}
export interface BusEntry {
  id: Id;
  sheet_id: Id;
  bus_id: Id;
  bus_point: Point;
  wire_point: Point;
}
export type NetScope = "local" | "global" | "off_page";
export type PowerStyle = "ground" | "rail" | "chassis";
export type SymbolField = "refdes" | "value" | "name" | "footprint" | "manufacturer" | "mpn";

export interface Sheet {
  id: Id;
  name: string;
  width: Nm;
  height: Nm;
  /** Border, zones and title block; absent = none. */
  format?: DrawingFormat;
  /** Place in the sheet order (ADR-0043); absent = 0. List sheets with `sheetsInOrder`. */
  order?: number;
}
export interface SymbolInstance {
  id: Id;
  sheet_id: Id;
  component_id: string;
  refdes: string;
  value: string;
  position: Point;
  rotation: Rotation;
  /** Mirrored about the local Y axis before rotation (ADR-0016); absent when not mirrored. */
  mirror?: boolean;
  /** Field visibility that differs from the default (refdes and value shown, others hidden). */
  field_visibility?: Partial<Record<SymbolField, boolean>>;
}
/** Sheet axis of a symbol flip (ADR-0016). */
export type FlipAxis = "horizontal" | "vertical";
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
  /** Absent = global. */
  scope?: NetScope;
  power_style?: PowerStyle;
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
  /** Schema v4 (ADR-0042): a circular arc from `a` to `b` sweeping this many degrees (positive = counter-clockwise, Y up); absent = straight. */
  arc_deg?: number;
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

// Schema v4 PCB objects (ADR-0042). All optional on the project so older snapshots still type-check.
export type DrawLayer = "top_silk" | "bottom_silk" | "documentation";
export type RegionObject = "tracks" | "vias" | "components" | "zones";
export type DimensionUnit = "mm" | "mil" | "inch";
export interface OutlineArc {
  edge: number;
  sweep_deg: number;
}
/** Keep-out (forbids objects) and/or constraint region (track width limits). */
export interface Region {
  id: Id;
  name?: string;
  outline: Point[];
  /** Empty or absent = every copper layer. */
  layers?: CopperLayer[];
  forbid?: RegionObject[];
  min_track_width?: Nm;
  max_track_width?: Nm;
}
export interface PcbText {
  id: Id;
  layer: DrawLayer;
  position: Point;
  text: string;
  size: Nm;
  stroke_width?: Nm;
  rotation?: Rotation;
}
export interface PcbDimension {
  id: Id;
  layer: DrawLayer;
  a: Point;
  b: Point;
  offset: Nm;
  unit: DimensionUnit;
  precision: number;
  text_size: Nm;
}
export interface Cutout {
  id: Id;
  outline: Point[];
}
/** Symbols, wires and net labels of one sheet that select and move together (ADR-0045). */
export interface SchematicGroup {
  id: Id;
  sheet_id: Id;
  name: string;
  members: Id[];
}
/** One change of the schematic-to-PCB update, named by its symbol (SyncSchematicToPcbSelected). */
export type SyncChange =
  | { kind: "add_footprint" | "remove_footprint" | "refdes" | "part"; symbol_id: Id }
  | { kind: "pad_net"; symbol_id: Id; pad: string };

/** Canonical EAK-IR project (fields the shell renders). */
export interface Project {
  /** Design rules and net declarations; absent only in fixtures that predate them. */
  constraints?: Constraints;
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
    graphics?: Record<Id, SheetGraphic>;
    buses?: Record<Id, Bus>;
    bus_entries?: Record<Id, BusEntry>;
    /** Object groups (IR v6, ADR-0045); absent only in fixtures that predate them. */
    groups?: Record<Id, SchematicGroup>;
  };
  /** Free project properties for title blocks; absent when empty. */
  properties?: Record<string, string>;
  /** Fabrication settings (`SetManufacturing`); absent only in fixtures that predate them. */
  manufacturing?: ManufacturingConfig;
  pcb: {
    board: { outline: Point[]; stackup?: Stackup; outline_arcs?: OutlineArc[] };
    footprints: Record<Id, FootprintInstance>;
    tracks: Record<Id, Track>;
    vias: Record<Id, Via>;
    zones: Record<Id, Zone>;
    regions?: Record<Id, Region>;
    texts?: Record<Id, PcbText>;
    dimensions?: Record<Id, PcbDimension>;
    cutouts?: Record<Id, Cutout>;
    /** Saved route tool options (schema v5, `SetRoutingOptions`); absent in older fixtures. */
    routing?: RoutingOptions;
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
      distributor?: string | null;
      /** Full v3 symbol in engine snapshots; test fixtures may give only pin numbers and names. */
      symbol: {
        pins: (Pick<SymbolPin, "number" | "name"> & Partial<SymbolPin>)[];
        body_half_w?: Nm;
        body_half_h?: Nm;
        graphics?: Graphic[];
      };
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
  | { type: "ImportLibraryComponent"; component_id: string }
  | { type: "UpsertComponentDefinition"; definition: ComponentDef }
  | { type: "SetComponentProperties"; component_id: string; manufacturer: string | null; mpn: string | null; distributor: string | null; datasheet: string | null }
  | { type: "AssignComponentFootprint"; component_id: string; footprint: FootprintDef; pin_map: Record<string, string[]> }
  | { type: "SetPadExpansion"; component_id: string; pad: string; mask_expansion: Nm | null; paste_expansion: Nm | null }
  | { type: "PlaceSymbol"; sheet_id: Id | null; component_id: string; position: Point; rotation: Rotation; refdes: string | null; value: string | null }
  | { type: "MoveSymbol"; symbol_id: Id; position: Point }
  | { type: "RotateSymbol"; symbol_id: Id; rotation: Rotation }
  | { type: "FlipSymbol"; symbol_id: Id; axis: FlipAxis }
  | { type: "SetSymbolValue"; symbol_id: Id; value: string }
  | { type: "SetRefdes"; symbol_id: Id; refdes: string }
  | { type: "DeleteSymbol"; symbol_id: Id }
  | { type: "AddWire"; sheet_id: Id | null; a: Point; b: Point }
  | { type: "DeleteWire"; wire_id: Id }
  | { type: "MoveWire"; wire_id: Id; a: Point; b: Point }
  | { type: "ConnectPins"; a_symbol: Id; a_pin: string; b_symbol: Id; b_pin: string }
  | { type: "AddNetLabel"; sheet_id: Id | null; name: string; position: Point; kind: "net" | "power"; scope?: NetScope; power_style?: PowerStyle | null }
  | { type: "AddBus"; sheet_id: Id | null; name: string; points: Point[] }
  | { type: "AddBusEntry"; bus_id: Id; bus_point: Point; wire_point: Point }
  | { type: "AddSheetGraphic"; sheet_id: Id | null; graphic: Graphic }
  | { type: "DeleteNetLabel"; label_id: Id }
  | { type: "LabelPin"; symbol_id: Id; pin: string; name: string; kind: "net" | "power"; scope?: NetScope; power_style?: PowerStyle | null }
  | { type: "SetNoConnect"; symbol_id: Id; pin: string }
  | { type: "DeleteNoConnect"; no_connect_id: Id }
  | { type: "MoveNetLabel"; label_id: Id; position: Point }
  | { type: "RenameNetLabel"; label_id: Id; name: string }
  | { type: "Group"; members: Id[]; name?: string | null }
  | { type: "Ungroup"; group_id: Id }
  | { type: "SetBoardOutline"; points: Point[] }
  | { type: "MoveFootprint"; footprint_id: Id; position: Point; rotation: Rotation | null; side: Side | null }
  | { type: "SetFootprintLocked"; footprint_id: Id; locked: boolean }
  | { type: "AddTrack"; layer: CopperLayer; a: Point; b: Point; width: Nm | null; net: string; arc_deg?: number | null }
  | { type: "DeleteTrack"; track_id: Id }
  | { type: "AddVia"; position: Point; net: string; drill: Nm | null; diameter: Nm | null; from?: CopperLayer | null; to?: CopperLayer | null }
  | { type: "AddZone"; layer: CopperLayer; net: string; outline: Point[]; clearance: Nm | null }
  | { type: "DeleteZone"; zone_id: Id }
  | { type: "SetDesignRules"; min_track_width: Nm | null; min_clearance: Nm | null; min_drill: Nm | null; min_annular_ring: Nm | null; board_edge_clearance: Nm | null; default_track_width: Nm | null; default_clearance: Nm | null; temp_rise_c: number | null; max_component_height: Nm | null }
  | { type: "UpsertNetClass"; class: NetClass }
  | { type: "SetNetCurrent"; net: string; amps: number | null }
  | { type: "SetNetVoltage"; net: string; volts: number | null }
  | { type: "SetRuleConfig"; rule_id: string; config: RuleConfig | null }
  | { type: "SetClearanceCell"; a: ObjectClass; b: ObjectClass; clearance: Nm | null }
  | { type: "SetNetPairClearance"; net_a: string; net_b: string; clearance: Nm | null }
  | { type: "SetLayerCount"; count: number; board_thickness?: Nm | null }
  | { type: "SetStackup"; stackup: Stackup }
  | { type: "SetManufacturing"; config: ManufacturingConfig }
  | { type: "SetRoutingOptions"; options: RoutingOptions }
  | { type: "DeleteVia"; via_id: Id }
  | { type: "Annotate"; order?: AnnotateOrder; scope?: AnnotateScope; symbols?: Id[]; reset?: boolean }
  | { type: "SyncSchematicToPcb" }
  | { type: "SyncSchematicToPcbSelected"; changes: SyncChange[] }
  | { type: "MoveTrack"; track_id: Id; by: Point }
  | { type: "EditTrack"; track_id: Id; width?: Nm | null; layer?: CopperLayer | null; a?: Point | null; b?: Point | null; arc_deg?: number | null }
  | { type: "MoveVia"; via_id: Id; position: Point }
  | { type: "EditZone"; zone_id: Id; outline?: Point[] | null; net?: string | null; layer?: CopperLayer | null; clearance?: Nm | null }
  | { type: "DeleteFootprint"; footprint_id: Id }
  | { type: "AddRegion"; name?: string | null; outline: Point[]; layers?: CopperLayer[]; forbid?: RegionObject[]; min_track_width?: Nm | null; max_track_width?: Nm | null }
  | { type: "DeleteRegion"; region_id: Id }
  | { type: "AddPcbText"; layer: DrawLayer; position: Point; text: string; size?: Nm | null; stroke_width?: Nm | null; rotation?: Rotation | null }
  | { type: "MovePcbText"; text_id: Id; position: Point; rotation?: Rotation | null }
  | { type: "DeletePcbText"; text_id: Id }
  | { type: "AddDimension"; layer?: DrawLayer | null; a: Point; b: Point; offset?: Nm | null; unit?: DimensionUnit | null; precision?: number | null; text_size?: Nm | null }
  | { type: "DeleteDimension"; dimension_id: Id }
  | { type: "AddBoardCutout"; outline: Point[] }
  | { type: "DeleteBoardCutout"; cutout_id: Id }
  | { type: "SetOutlineArc"; edge: number; sweep_deg: number | null }
  | { type: "AutoPlace"; grid: Nm | null }
  | { type: "AutoRoute"; nets: string[] | null }
  | { type: "ClearRouting"; net: string | null }
  | { type: "StitchZones"; net: string | null }
  | { type: "AddSheet"; name: string }
  | { type: "RenameSheet"; sheet_id: Id; name: string }
  | { type: "DeleteSheet"; sheet_id: Id }
  | { type: "ReorderSheets"; sheet_ids: Id[] }
  | { type: "DeleteSheetGraphic"; graphic_id: Id }
  | { type: "DeleteBus"; bus_id: Id }
  | { type: "DeleteBusEntry"; entry_id: Id }
  | { type: "Batch"; commands: Command[]; label?: string | null };

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
export interface Quantity {
  value: number;
  unit: string;
}
export interface Limit extends Quantity {
  bound: "min" | "max";
}
export interface Finding {
  id?: string;
  rule_id: string;
  severity: Severity;
  message: string;
  objects: Id[];
  location: Point | null;
  /** Quantitative rules only. */
  measured?: Quantity | null;
  required?: Limit | null;
  suggested_fix?: string | null;
}
export type CheckState = "PASS" | "FAIL" | "WARNING" | "UNKNOWN" | "NOT_APPLICABLE" | "REQUIRES_HUMAN_APPROVAL" | "DISABLED";
export interface RuleResult {
  rule_id: string;
  state: CheckState;
  reason: string;
  /** Severity the project's rule configuration gives the rule's findings (ADR-0046). */
  severity_override?: Severity;
  /** The rule's state at its default severity, when the override changed it. */
  default_state?: CheckState;
}

export interface RuleInfo {
  id: string;
  description: string;
  parameters: string;
  source: string;
}
export interface Report {
  check: string;
  project_id: Id;
  revision: number;
  input_sha256: string;
  tool: string;
  /** The rules the check declares (absent in older test fixtures). */
  rules?: RuleInfo[];
  findings: Finding[];
  outcome: "NO_ERRORS_IN_IMPLEMENTED_CHECKS" | "ERRORS";
  errors: number;
  warnings: number;
  not_implemented: string[];
  limitations: string[];
  /** ADR-0026: one explicit state per rule; absent in reports saved before it (treat as UNKNOWN). */
  rule_results?: RuleResult[];
  overall?: CheckState;
}
export type Check = "erc" | "drc" | "dfm";
export interface CheckResult {
  report: Report;
  evidence_path: string | null;
}
/** B5 `export_findings`: the file written and the check run it wrote. */
export interface FindingsExport {
  path: string;
  result: CheckResult;
}

/** Read-only status read from the project folder (src-tauri `recent_projects`); absent when unreadable. */
export interface ProjectStatus {
  last_modified_ms: number | null;
  revision: number;
  /** Stage derived from project content, not a stored workflow phase. */
  phase: string;
  /** Checks with no evidence for the current revision. */
  pending_validation: string[];
  evidence: { check: string; errors: number; warnings: number; state: string }[];
  /** Errors in current evidence; null when no check has current evidence. */
  unresolved_errors: number | null;
}

/** Meaning of a project's `.lock` (eak_runtime::store::LockState, ADR-0029). */
export type LockState =
  | { state: "unlocked" }
  | { state: "open_elsewhere"; this_app: boolean }
  | { state: "interrupted"; pending_operations: number }
  | { state: "unverifiable"; reason: string }
  | { state: "unavailable"; reason: string };

export interface RecentProject {
  path: string;
  project_id: Id;
  name: string;
  revision: number;
  last_opened_ms: number;
  status?: ProjectStatus | null;
}

// Model providers (src-tauri/src/models.rs, eak-intelligence provider catalog). Optional
// infrastructure for EAK-native agent integrations. The UI never receives an API key, only
// whether one is stored in the OS credential store.
export type ProviderKind = "anthropic" | "open_ai_compatible" | "ollama" | "gemini";
export type ErrorClass =
  | "missing_api_key"
  | "invalid_api_key"
  | "authentication_failed"
  | "unsupported_model"
  | "invalid_base_url"
  | "network_error"
  | "timeout"
  | "rate_limited"
  | "provider_unavailable"
  | "request_rejected"
  | "invalid_response"
  | "invalid_configuration"
  | "not_implemented"
  | "cancelled";
export interface ModelInfo {
  id: string;
  name: string | null;
  context_window: number | null;
  max_output_tokens: number | null;
  /** null = unknown, never "no". */
  tools: boolean | null;
  vision: boolean | null;
  source: "catalog" | "discovered";
}
export interface ProviderSpec {
  id: string;
  name: string;
  group: "cloud" | "local" | "custom";
  kind: ProviderKind;
  base_url: string | null;
  base_url_editable: boolean;
  auth: "api_key" | "optional_api_key";
  org_project: boolean;
  discovery: boolean;
  implemented: boolean;
  note: string | null;
  models: ModelInfo[];
}
export type StatusState = "untested" | "connected" | "authenticated" | "failed";
export interface ConnectionStatus {
  state: StatusState;
  error_class: ErrorClass | null;
  message: string | null;
  checked_at_ms: number | null;
  model: string | null;
}
export interface ProviderProfile {
  id: string;
  label: string;
  provider: string;
  kind: ProviderKind;
  base_url: string | null;
  model: string | null;
  org_id: string | null;
  project_id: string | null;
  models: ModelInfo[];
  status: ConnectionStatus;
}
export interface ProviderView extends ProviderProfile {
  provider_name: string;
  endpoint: string;
  has_key: boolean;
  needs_key: boolean;
  key_error: string | null;
  is_default: boolean;
}
export interface ProfileInput {
  id: string | null;
  label: string;
  provider: string;
  kind: ProviderKind;
  base_url: string | null;
  model: string | null;
  org_id: string | null;
  project_id: string | null;
}
export interface StageReport {
  stage: "configuration" | "authentication" | "model" | "invocation";
  /** null = skipped (reason in message). */
  ok: boolean | null;
  message: string;
}
export interface ConnectionReport {
  state: "connected" | "authenticated" | "failed";
  ok: boolean;
  latency_ms: number;
  message: string;
  error_class: ErrorClass | null;
  stages: StageReport[];
  models: ModelInfo[];
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
// ---- D3 output documents (crates/eak-manufacturing/src/{bom_config,outputs}.rs) ----
export type BomField =
  | "comment"
  | "designator"
  | "quantity"
  | "footprint"
  | "component_id"
  | "manufacturer"
  | "mpn"
  | "distributor_part"
  | "smd"
  | "placed"
  | "assembled"
  | "stock"
  | "unit_price_usd"
  | "price_as_of"
  | "issues";
export type SortDir = "none" | "asc" | "desc";
export interface BomColumn {
  field: BomField;
  header?: string;
  sort: SortDir;
}
export type FilterOp = "equals" | "not_equals" | "contains" | "is_empty" | "not_empty";
export interface FilterRule {
  field: BomField;
  op: FilterOp;
  value: string;
}
export interface BomConfig {
  version: number;
  columns: BomColumn[];
  grouping: "part" | "designator";
  exclude: FilterRule[];
}
export interface BomTable {
  fields: BomField[];
  headers: string[];
  rows: string[][];
  removed: number;
  parts: number;
}
export type BomFormat = "csv" | "xlsx";
export interface OutputRequest {
  schematic_pdf: boolean;
  schematic_svg: boolean;
  ipc_d_356: boolean;
  netlist_json: boolean;
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

/** One row of the release check (`ReleaseGate` in src-tauri/src/lib.rs, slice E7). */
export interface ReleaseGate {
  id: string;
  title: string;
  /** DISABLED: shown, but not counted by the release. */
  state: "PASS" | "FAIL" | "UNKNOWN" | "NOT_RUN" | "DISABLED";
  /** Required rows keep every output DRAFT unless PASS; advisory rows are reported only. */
  required: boolean;
  reasons: string[];
  evidence: string[];
}
/** Read-only release check of one revision (`ReleaseCheck` in src-tauri/src/lib.rs). */
export interface ReleaseCheck {
  revision: number;
  input_sha256: string;
  gates: ReleaseGate[];
  blocking: string[];
  status: "DRAFT" | "INTERNALLY_VERIFIED_RELEASE_CANDIDATE";
  not_verified: string[];
}

// ---- parts search, BOM and Fabricator preparation (src-tauri/src/parts.rs) ----

export interface SourcingRecord {
  provider: string;
  source_url: string;
  retrieved_at_unix_s: number;
  raw_sha256: string;
  distributor_part: string | null;
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
  distributor: string | null;
  mpn: string | null;
}

export type LiveStatus =
  | "ok"
  | "stale"
  | "needs_credentials"
  | "unauthorized"
  | "rate_limited"
  | "blocked"
  | "not_found"
  | "unreachable"
  | "bad_query"
  | "error";

export interface PartsSearch {
  query: string;
  library: LibraryHit[];
  stored: SourcingRecord[];
  live: {
    status: LiveStatus;
    message: string;
    records: SourcingRecord[];
    /** How the records were obtained; a `stale_cache` answer is an older snapshot shown because Distributor is down. */
    served?: { source: "live" | "cache" | "stale_cache"; age_s?: number; because?: string } | null;
  };
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
  distributor: string | null;
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

export type AssemblyClass = "unknown" | "basic" | "extended" | "preferred_extended" | "not_assemblable";

export interface AssemblyWarning {
  kind: string;
  component_id: string;
  designators: string[];
  message: string;
}

export interface AssemblyReport {
  rows: { distributor: string; class: AssemblyClass | null; source: string | null; verified_at_unix_s: number | null }[];
  warnings: AssemblyWarning[];
}

export interface ManufacturingPreview {
  revision: number;
  bom: BomLine[];
  dfm: DfmReport;
  /** Fabricator assembly classification and fail-closed BOM warnings (ADR-0034). */
  assembly: AssemblyReport;
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

/** Physical connectivity from eak-engineering (same analysis DRC uses). Points are board nm. */
export interface ConnectivityReport {
  revision: number;
  net_groups: Record<string, number>;
  shorts: [string, string, string, string][];
  /** Unrouted connections: [net, from, to]. */
  ratsnest: [string, Point, Point][];
}

// AI panel (src-tauri/src/ai.rs): agent-loop proposals awaiting Accept/Reject.
export type AgentOutcome =
  | { outcome: "completed" }
  | { outcome: "turn_limit_reached" }
  | { outcome: "cancelled" }
  | { outcome: "refused" }
  | { outcome: "provider_failed"; detail: string };

export interface AiStep {
  id: number;
  proposal_id: number;
  command: Command;
  class: "read_only" | "reversible_local" | "engineering_impacting" | "destructive";
  checks_to_run: string[];
  preview: CommandResponse;
}

export interface AiProposal {
  id: number;
  provider: string;
  outcome: AgentOutcome;
  text: string;
  revision: number;
  steps: AiStep[];
}
// ---- Phase 5: component library (mirrors eak-core `library.rs`)
export type PinType = "input" | "output" | "bidirectional" | "passive" | "power_in" | "power_out" | "open_collector" | "no_connect";
export interface SymbolPin {
  number: string;
  name: string;
  electrical_type: PinType;
  position: Point;
  /** Schema v3 pin geometry; optional in fixtures. */
  length?: Nm;
  orientation?: "right" | "left" | "up" | "down";
  name_visible?: boolean;
  number_visible?: boolean;
}
export interface PadDef {
  number: string;
  kind: "smd" | "tht";
  shape: "rect" | "circle" | "oval";
  position: Point;
  size_x: number;
  size_y: number;
  drill: number | null;
  /** Own solder-mask expansion per side (schema v5); absent or null = the board default. */
  mask_expansion?: number | null;
  /** Own paste expansion per side, SMD pads only (schema v5); absent or null = none. */
  paste_expansion?: number | null;
}
export interface FootprintDef {
  name: string;
  pads: PadDef[];
  courtyard_half_w: number;
  courtyard_half_h: number;
  body_height: number | null;
}
export interface Rating { name: string; value: number; unit: string; source: string; verified: boolean }
export interface SymbolDef {
  pins: SymbolPin[];
  body_half_w: number;
  body_half_h: number;
  /** Body drawing (schema v3); optional in fixtures. */
  graphics?: Graphic[];
}
export interface ComponentDef {
  id: string;
  name: string;
  category: string;
  refdes_prefix: string;
  default_value: string;
  manufacturer: string | null;
  mpn: string | null;
  distributor: string | null;
  datasheet: string | null;
  symbol: SymbolDef;
  footprint: FootprintDef;
  pin_map: Record<string, string[]>;
  ratings: Rating[];
  /** Box-body style and optional real 3D model reference (optional only in test fixtures). */
  model3d?: Model3d;
  provenance: string;
  /** Supplier offers (ADR-0028): commercial data, not identity. Omitted by the engine when empty. */
  supplier_offers?: SupplierOffer[];
}
/** One quantity break; the price is decimal text exactly as the supplier stated it (eak_core::OfferPrice). */
export interface OfferPrice {
  min_quantity: number;
  unit_price: string;
  currency: string;
}
/** Mirrors `eak_core::SupplierOffer`: stock, price and lifecycle go stale and never prove suitability. */
export interface SupplierOffer {
  supplier: string;
  supplier_part: string;
  url?: string | null;
  stock?: number | null;
  price_breaks?: OfferPrice[];
  lifecycle?: string | null;
  fetched_at_unix_s?: number | null;
  source_sha256?: string | null;
}
export type ModelFormat = "step" | "wrl" | "glb";
/** Offset (mm, footprint coordinates: x right, y down, z up), rotation (deg about model x/y/z, Rz*Ry*Rx), scale (> 0). */
export interface ModelTransform {
  offset_mm: [number, number, number];
  rotation_deg: [number, number, number];
  scale: [number, number, number];
}
/** Mirrors `eak_core::ModelRef`. */
export type ModelQuality = "VENDOR_EXACT" | "LIBRARY_VERIFIED" | "DERIVED_VALIDATED";
export interface ModelRef {
  source: { kind: "file"; path: string };
  format: ModelFormat;
  /** Declared trust level; absent = unclassified (never shown as exact). */
  quality?: ModelQuality | null;
  transform: ModelTransform;
  licence: string | null;
  provenance: string;
}
export interface Model3d {
  kind: string;
  color: string;
  external_ref: string | null;
  model_ref?: ModelRef | null;
}
/** A part as listed by `library_search` (built-in catalogue merged with the project library). */
export interface LibraryPart {
  id: string;
  /** Library the shown definition comes from (disabled libraries are never listed). */
  source: "builtin" | "project" | "personal" | `imported:${string}`;
  in_project: boolean;
  def: ComponentDef;
  unverified_ratings: number;
}
/** A footprint that passes library validation when assigned (`library_footprint_candidates`). */
export interface FootprintCandidate {
  name: string;
  donor: string;
  footprint: FootprintDef;
  pin_map: Record<string, string[]>;
}


/** Optional ngspice simulation (eak-simulation). Results are computed by ngspice, never verified by EAK. */
export interface NgspiceStatus {
  available: boolean;
  path: string | null;
  version: string | null;
  message: string | null;
}
export interface SimElement { refdes: string; kind: string; value: string; line: string }
export interface SimSkipped { refdes: string; reason: string }
export interface SimulationStatus {
  ngspice: NgspiceStatus;
  revision: number;
  nets: string[];
  elements: SimElement[];
  skipped: SimSkipped[];
}
export type SimAnalysis = { kind: "op" } | { kind: "tran"; stop: number; step: number };
export interface SimSource {
  name: string;
  kind: "voltage" | "current";
  positive_net: string;
  negative_net: string;
  spec: string;
}
export interface SimRequest { analysis: SimAnalysis; sources: SimSource[]; timeout_secs?: number | null }
export interface SimResult {
  analysis: SimAnalysis;
  ngspice_version: string;
  nodes: { net: string; volts: number }[];
  currents: { element: string; amps: number }[];
  tran: { time: number[]; traces: { net: string; volts: number[] }[]; points_total: number; decimated: boolean } | null;
  warnings: string[];
  limitations: string[];
  netlist: string;
  elapsed_ms: number;
}

/** Interactive routing (eak_engineering::interactive). Points are board nm. */
export type BendMode = "diagonal45" | "orthogonal90";
/** What the route step does with other-net copper in the way (eak_engineering::interactive). */
export type ConflictMode = "walkaround" | "block" | "push";
/** A committed track moved by push mode: the polyline that replaces track `id` (ends kept). */
export interface ShovedTrack {
  id: Id;
  layer: CopperLayer;
  width: Nm;
  net: string;
  points: Point[];
}
export interface ViaSize {
  drill: Nm;
  diameter: Nm;
}
/** The route tool's options as saved with the board (eak_core::RoutingOptions, ADR-0044). */
export interface RoutingOptions {
  bend: BendMode;
  conflict: ConflictMode;
  /** Track width for new routes; null = net class. */
  width: Nm | null;
  /** Via for layer switches; null = net class. */
  via: ViaSize | null;
  remove_loops: boolean;
}
export interface RouteStepRequest {
  net: string;
  layer: CopperLayer;
  from: Point;
  to: Point;
  mode: BendMode;
  width: Nm | null;
  via_at_start: boolean;
  conflict: ConflictMode;
  /** Via size for `via_at_start`; null uses the net-class via. */
  via: ViaSize | null;
  /** The route so far, uncommitted: its segments, vias and the tracks earlier steps pushed. */
  pending: {
    tracks: { layer: CopperLayer; width: Nm; a: Point; b: Point }[];
    vias: { position: Point; drill: Nm; diameter: Nm }[];
    shoved: ShovedTrack[];
  };
}
export interface RouteSegment {
  a: Point;
  b: Point;
}
export interface ClearanceFeedback {
  object: string;
  /** Copper-to-copper gap in nm (negative = overlap). */
  measured: number;
  required: Nm;
}
export interface RouteStepReport {
  revision: number;
  net: string;
  layer: CopperLayer;
  width: Nm;
  clearance: Nm;
  mode: BendMode;
  segments: RouteSegment[];
  via: { position: Point; drill: Nm; diameter: Nm } | null;
  end: Point;
  reached: boolean;
  walkaround: boolean;
  blocking: ClearanceFeedback[];
  min_gap: ClearanceFeedback | null;
  message: string | null;
  /** Push mode: tracks moved out of the path (absent from older engines). */
  shoved?: ShovedTrack[];
}
/** Net class (EAK-IR); lengths in nm. */
export interface NetClass {
  name: string;
  track_width: Nm;
  clearance: Nm;
  via_drill: Nm;
  via_diameter: Nm;
  nets: string[];
}
/** Design rules and electrical declarations (EAK-IR `Constraints`). */
export interface Constraints {
  min_track_width: Nm;
  min_clearance: Nm;
  min_drill: Nm;
  min_annular_ring: Nm;
  board_edge_clearance: Nm;
  default_class: NetClass;
  net_classes?: NetClass[];
  net_currents_a?: Record<string, number>;
  net_voltages_v?: Record<string, number>;
  temp_rise_c: number;
  max_component_height?: Nm | null;
  /** Per-rule enable and severity override (schema v7, ADR-0046). */
  rules?: Record<string, RuleConfig>;
  /** Clearance matrix cells (schema v7, Phase 1 C5). */
  clearance_matrix?: ClearanceCell[];
  /** Net-to-net clearances (schema v7, Phase 1 C5). */
  net_pair_rules?: NetPairRule[];
}

export type RuleSeverity = "error" | "warning" | "info";
export interface RuleConfig {
  enabled: boolean;
  severity?: RuleSeverity | null;
}
export type ObjectClass = "track" | "smd_pad" | "th_pad" | "via" | "zone" | "hole" | "outline" | "silk";
export interface ClearanceCell {
  a: ObjectClass;
  b: ObjectClass;
  clearance: Nm;
}
export interface NetPairRule {
  net_a: string;
  net_b: string;
  clearance: Nm;
}

/** A project open in this window (the engine keeps several; one is active). */
export interface OpenProject {
  id: string;
  name: string;
  dir: string | null;
  active: boolean;
}

// ---- Engineering intelligence summary (`intelligence_summary` IPC, ADR-0027;
// crates/eak-intelligence/src/systems). Read-only, recomputed per call; only computed values.

/** Shared check states (ADR-0026, `eak_validation::CheckState`). */
export type IntelOutcome =
  | "PASS"
  | "FAIL"
  | "WARNING"
  | "UNKNOWN"
  | "NOT_APPLICABLE"
  | "REQUIRES_HUMAN_APPROVAL";

export interface IntelDomain {
  id: string;
  name: string;
  high_risk: boolean;
  coverage: "mental_model" | "registry_only";
  nodes: string[];
  reasons: string[];
}

export interface IntelNode {
  id: string;
  level: "product" | "subsystem" | "board" | "functional_block" | "component" | "pcb_region";
  name: string;
  parent: string | null;
  domains: string[];
}

export interface IntelMissing {
  id: string;
  domain: string | null;
  what: string;
  why: string;
}

export interface IntelConflict {
  id: string;
  constraints: [string, string];
  quantity: string;
  explanation: string;
  tradeoffs: string[];
}

/** Shared risk record (ADR-0026) plus the multi-domain class, title and domains. */
export interface IntelRisk {
  id: string;
  subsystem: string;
  /** Separate axes; never combined into one score. */
  severity: "low" | "medium" | "high" | "critical";
  likelihood: "rare" | "possible" | "likely";
  detectability: "detected" | "not_checked";
  consequence: string;
  evidence: string[];
  mitigation: string | null;
  approval_required: boolean;
  class: string;
  title: string;
  detail: string;
  domains: string[];
  basis: string;
}

export interface IntelligenceSummary {
  revision: number;
  classification: {
    status: "CLASSIFIED" | "REVIEW_REQUIRED" | "UNCLASSIFIED";
    confidence: number;
    primary: string | null;
    secondary: string[];
    overlays: string[];
  };
  domains: IntelDomain[];
  architecture: IntelNode[];
  requirement_coverage: {
    total: number;
    by_knowledge: Record<string, number>;
    with_validation_method: number;
    unknown_values: number;
  };
  missing_information: IntelMissing[];
  assumptions: string[];
  constraints: { hard: number; soft: number; derived: number; conditional: number; safety: number };
  conflicts: IntelConflict[];
  /** Per domain: [check id, outcome]. */
  domain_checks: Record<string, [string, IntelOutcome][]>;
  risks: IntelRisk[];
  human_approval_required: boolean;
  /** [dashboard item, the tool that computes it]. */
  elsewhere: [string, string][];
  not_implemented: string[];
  limitations: string[];
}

// Distributor OpenAPI catalogue (ADR-0032). The API secret never reaches the frontend: only the last
// four characters of the key are returned.
export interface DistributorStatusFile {
  state: "untested" | "ok" | "failed";
  message: string;
  tested_at_unix_s: number | null;
  last_success_unix_s: number | null;
}

export interface DistributorView {
  has_credentials: boolean;
  key_hint: string | null;
  store_error: string | null;
  status: DistributorStatusFile;
  requests_this_session: number;
  apply_url: string;
  host: string;
  needs: string | null;
}

export type Served = { source: "live" | "cache" | "stale_cache"; age_s?: number; because?: string };

export interface CatalogueReply<T> {
  status: LiveStatus;
  message: string;
  served: Served | null;
  fetched_at_unix_s: number | null;
  data: T | null;
}

export interface CategoryNode {
  id: number;
  name: string;
  product_count: number | null;
  children: CategoryNode[];
}

export interface Manufacturer {
  id: number | null;
  name: string;
  product_count: number | null;
}

export type MatchKind = "exact_distributor" | "exact_mpn_manufacturer" | "exact_mpn" | "fuzzy" | "browse";

export interface Candidate {
  record: SourcingRecord;
  kind: MatchKind;
  exact: boolean;
  reasons: string[];
  manufacturer_conflict: boolean;
  package_match: boolean | null;
  category_match: boolean | null;
  engineering_status: "supplier_listing_only";
}

export interface CatalogueResults {
  candidates: Candidate[];
  /** What each listing is relative to the libraries, by Distributor number. */
  links: Record<string, LibraryLink>;
  total: number | null;
  page: number;
  limit: number;
  has_more: boolean;
  via: string;
  filtered_out: number;
}

export interface CatalogueQuery {
  keyword?: string;
  category_id?: number;
  category_name?: string;
  page?: number;
  limit?: number;
  in_stock_only?: boolean;
  currency?: string;
  manufacturer?: string;
  package?: string;
  category?: string;
  refresh?: boolean;
}

// Library scopes, browse and supplier-record linking (ADR-0033).
export type LibraryScope = { kind: "builtin" } | { kind: "project" } | { kind: "personal" } | { kind: "imported"; name: string };

export type Standing = "existing_eak" | "imported_definition" | "supplier_only" | "needs_authoring";

export interface LibraryLinkMatch {
  component_id: string;
  scope: LibraryScope;
  kind: "exact_supplier_part" | "exact_mpn_manufacturer" | "exact_mpn";
  reasons: string[];
  manufacturer_conflict: boolean;
}

export interface LibraryLink {
  standing: Standing;
  matches: LibraryLinkMatch[];
  /** Not approved for engineering use: unverified ratings, no datasheet, or no definition yet. */
  unverified: boolean;
  import_template: string | null;
  explanation: string;
}

export interface SavedRecord {
  record: SourcingRecord;
  path: string;
  link: LibraryLink;
}

export interface LibraryEntry2 {
  scope: LibraryScope;
  scope_label: string;
  id: string;
  def: ComponentDef;
  favourite: boolean;
  tags: string[];
  recent_rank: number | null;
  unverified_ratings: number;
  has_datasheet: boolean;
  in_project: boolean;
  /** From a stored sourcing record; `price_breaks` is absent from older builds. */
  supplier: { distributor: string; stock: number | null; as_of_unix_s: number; price_breaks?: OfferPrice[] } | null;
}

export interface Facet {
  value: string;
  count: number;
}

export interface ImportedLibrary {
  manifest: { name: string; licence: string; source_url: string };
  components: number;
}

export interface LibraryBrowse {
  entries: LibraryEntry2[];
  facets: { sources: Facet[]; categories: Facet[]; manufacturers: Facet[]; packages: Facet[]; tags: Facet[] };
  total: number;
  problems: string[];
  imported: ImportedLibrary[];
}

export interface LibraryBrowseQuery {
  query?: string;
  source?: string;
  category?: string;
  manufacturer?: string;
  package?: string;
  tag?: string;
  favourites_only?: boolean;
  recent_only?: boolean;
}

export interface LibraryMeta {
  favourites: string[];
  tags: Record<string, string[]>;
  recent: string[];
}

export interface LibraryImportSummary {
  library: ImportedLibrary;
  accepted: string[];
  rejected: string[];
}

export type LibraryScopeKind = "system" | "user" | "project" | "imported" | "organisation";

export interface LibraryCatalogRow {
  id: string;
  name: string;
  namespace: string;
  version: string;
  source: string;
  license: string;
  description: string;
  content_hash: string;
  scope: LibraryScopeKind;
  enabled: boolean;
  components: number;
}

export interface LibraryCatalogView {
  libraries: LibraryCatalogRow[];
  problems: string[];
}

/** Annotate numbering order within a sheet (ADR-0046): across = by Y then X, down = by X then Y. */
export type AnnotateOrder = "across" | "down";
/** Which symbols Annotate numbers or resets (ADR-0046). */
export type AnnotateScope = "unannotated" | "all" | "selection";
