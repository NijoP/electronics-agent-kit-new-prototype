// Typed client for the Tauri IPC commands in src-tauri/src/lib.rs. This is the only
// module that talks to Rust; every project change is an engine call made here.
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { PerformanceSettings, ResourceProfile } from "./resourceProfile";
import type {
  AiProposal,
  RouteStepReport,
  RouteStepRequest,
  Check,
  FindingsExport,
  CheckResult,
  Command,
  CommandError,
  CommandResponse,
  LockState,
  BoardGeometry,
  ConnectivityReport,
  ConnectionReport,
  EngineEvent,
  ModelInfo,
  ProfileInput,
  ProviderSpec,
  ProviderView,
  ExportResult,
  BomConfig,
  BomFormat,
  BomTable,
  OutputRequest,
  ReleaseCheck,
  ImportResult,
  LibraryEntry,
  LibraryPart,
  FootprintCandidate,
  ManufacturingPreview,
  Report,
  PartsSearch,
  AssemblyClass,
  CatalogueQuery,
  CatalogueReply,
  CatalogueResults,
  CategoryNode,
  DistributorView,
  LibraryBrowse,
  LibraryCatalogView,
  LibraryBrowseQuery,
  LibraryImportSummary,
  LibraryMeta,
  SavedRecord,
  Manufacturer,
  PrepareResult,
  RecentProject,
  DomainReport,
  IntelligenceSummary,
  SimRequest,
  SimResult,
  SimulationStatus,
  Snapshot,
  OpenProject,
} from "./types";
import type { FileContent, FileEntry, GitStatus } from "./shell/files";
import type { SyncPreview } from "./shell/ecoPreview";
import type { ApplyOutcome, ProposalReview, ProposalSummary } from "./shell/proposalReview";
import type { EngineChange } from "./shell/syncReport";
import type { HomeFacts } from "./home/homeFacts";
import type { AgentSession, Surface as AgentSurface, ToolStatus } from "./settings/aiTools";
import type { SessionInfo as TerminalSessionInfo } from "./terminal/api";

// Settings › AI tools commands: must match `AGENT_COMMANDS` in src-tauri/src/agents.rs.
export const AGENT_IPC = [
  "agent_tools",
  "agent_tool_connect",
  "agent_tool_disconnect",
  "agent_tool_login",
  "agent_tool_launch",
  "agent_sessions",
  "agent_session_log",
  "agent_tool_stop",
] as const;

// Terminal workspace commands: must match `TERMINAL_COMMANDS` in src-tauri/src/terminal.rs.
export const TERMINAL_IPC = [
  "terminal_overview",
  "terminal_shells",
  "terminal_launchers",
  "terminal_api",
  "terminal_set_context",
  "terminal_create",
  "terminal_write",
  "terminal_run",
  "terminal_resize",
  "terminal_scrollback",
  "terminal_rename",
  "terminal_focus",
  "terminal_restart",
  "terminal_close",
  "terminal_space_create",
  "terminal_space_rename",
  "terminal_space_close",
  "terminal_move_tab",
  "terminal_tab_rename",
] as const;

// Must match `IPC_COMMANDS` in src-tauri/src/lib.rs (checked by tests/ipc.rs).
export const IPC_COMMANDS = [
  "create_project",
  "open_project",
  "project_lock_state",
  "close_project",
  "current_project",
  "agent_tasks",
  "netlist",
  "kicad_import_report",
  "kicad_import_commit",
  "execute_command",
  "undo",
  "redo",
  "events_since",
  "acknowledge_recovery",
  "run_check",
  "recent_projects",
  "model_providers",
  "model_save_provider",
  "model_remove_provider",
  "model_set_key",
  "model_clear_key",
  "model_list_models",
  "model_test_connection",
  "model_catalog",
  "model_set_default",
  "model_export_config",
  "model_import_config",
  "library_components",
  "library_search",
  "library_footprint_candidates",
  "library_model_read",
  "domain_classification",
  "intelligence_summary",
  "export_manufacturing",
  "bom_table",
  "export_bom",
  "export_documents",
  "export_findings",
  "release_check",
  "run_release_checks",
  "connectivity",
  "board_geometry",
  "ai_propose",
  "ai_accept",
  "ai_reject",
  "ai_rollback",
  "simulation_status",
  "run_simulation",
  "route_step",
  "parts_search",
  "parts_import",
  "distributor_settings_get",
  "distributor_settings_save",
  "distributor_settings_clear",
  "distributor_test_connection",
  "catalogue_categories",
  "catalogue_manufacturers",
  "catalogue_search",
  "parts_save_record",
  "library_browse",
  "library_catalog",
  "library_set_enabled",
  "library_meta",
  "library_personal_save",
  "library_personal_remove",
  "library_import_open_source",
  "library_imported_remove",
  "assembly_attest",
  "assembly_import_parts_file",
  "manufacturing_preview",
  "part_standardisation",
  "fabricator_prepare",
  "open_projects",
  "switch_project",
  "close_project_by_id",
  "operation_changes",
  "sync_preview",
  "sync_preview_export",
  "proposal_list",
  "proposal_review",
  "proposal_apply",
  "proposal_discard",
  "proposal_export",
  "activity_timeline",
  "project_files",
  "project_file_read",
  "project_file_create",
  "project_file_rename",
  "project_file_delete",
  "project_git_status",
  "resource_profile",
  "performance_settings",
  "set_performance_settings",
  "home_project_facts",
  "home_default_dir",
  "home_suggest_folder",
  "create_project_with_brief",
  "create_sample_project",
  ...TERMINAL_IPC,
  ...AGENT_IPC,
] as const;
type IpcCommand = (typeof IPC_COMMANDS)[number];

function call<T>(cmd: IpcCommand, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

/** Native folder picker (OS dialog). Resolves to the chosen absolute path, or null when cancelled. */
export async function pickFolder(title: string): Promise<string | null> {
  const r = await openDialog({ directory: true, multiple: false, title });
  return typeof r === "string" ? r : null;
}

/** Native file picker for a 3D model (STEP/WRL/GLB). Resolves to the absolute path, or null when cancelled. */
export async function pickModelFile(title: string): Promise<string | null> {
  const r = await openDialog({ directory: false, multiple: false, title, filters: [{ name: "3D model", extensions: ["step", "stp", "wrl", "glb"] }] });
  return typeof r === "string" ? r : null;
}

/** Must match `KicadImportView` in src-tauri/src/kicad.rs (eak-core `ImportReport`). */
export interface KicadImportView {
  file: string;
  imported: number;
  rejected: number;
  report: {
    format: string;
    items: { name: string; status: "imported" | "rejected"; notes: string[] }[];
    unsupported: Record<string, number>;
  };
}

export interface KicadCommitResult {
  library_name: string;
  /** Licence and source the user named; a commit without them is refused (F2). */
  licence: string;
  source_url: string;
  /** Symbol imports: every footprint is a one-pad-per-pin placeholder, not the real package. */
  placeholder_footprints: boolean;
  components_created: number;
  component_ids: string[];
  rejected: string[];
  report: {
    format: string;
    items: { name: string; status: "imported" | "rejected"; notes: string[] }[];
    unsupported: Record<string, number>;
  };
}

/** Native file picker for an optional KiCad interchange file. Resolves to the path, or null when cancelled. */
export async function pickKicadFile(title: string): Promise<string | null> {
  const r = await openDialog({ directory: false, multiple: false, title, filters: [{ name: "KiCad symbol or footprint", extensions: ["kicad_sym", "kicad_mod"] }] });
  return typeof r === "string" ? r : null;
}

export const api = {
  createProject: (path: string, name: string) => call<Snapshot>("create_project", { path, name }),
  // Project Home (slice S1, src-tauri/src/home.rs): read-only facts of a project folder, the default
  // projects folder, a free folder name, and the two Home starts that go through the engine.
  homeProjectFacts: (path: string) => call<HomeFacts>("home_project_facts", { path }),
  homeDefaultDir: () => call<string | null>("home_default_dir"),
  homeSuggestFolder: (parent: string, name: string) => call<string>("home_suggest_folder", { parent, name }),
  createProjectWithBrief: (path: string, name: string, brief: string[]) => call<Snapshot>("create_project_with_brief", { path, name, brief }),
  createSampleProject: (path: string) => call<Snapshot>("create_sample_project", { path }),
  openProject: (path: string, takeOverLock: boolean) =>
    call<Snapshot>("open_project", { path, take_over_lock: takeOverLock }),
  projectLockState: (path: string) => call<LockState>("project_lock_state", { path }),
  closeProject: () => call<void>("close_project"),
  currentProject: () => call<Snapshot>("current_project"),
  kicadImportReport: (path: string) => call<KicadImportView>("kicad_import_report", { path }),
  kicadImportCommit: (path: string, licence: string, source: string) =>
    call<KicadCommitResult>("kicad_import_commit", { path, licence, source }),
  netlist: () => call<{ name: string; pins: string[]; label_names: string[]; has_power_port: boolean }[]>("netlist"),
  agentTasks: () => call<import("./agentTasks").AgentTask[]>("agent_tasks"),
  execute: (expectedRevision: number, command: Command) =>
    call<CommandResponse>("execute_command", { expected_revision: expectedRevision, command }),
  undo: (expectedRevision: number) =>
    call<CommandResponse>("undo", { expected_revision: expectedRevision }),
  redo: (expectedRevision: number) =>
    call<CommandResponse>("redo", { expected_revision: expectedRevision }),
  eventsSince: (seq: number) => call<EngineEvent[]>("events_since", { seq }),
  acknowledgeRecovery: () => call<Snapshot>("acknowledge_recovery"),
  runCheck: (check: Check) => call<CheckResult>("run_check", { check }),
  recentProjects: (limit: number) => call<RecentProject[]>("recent_projects", { limit }),
  modelProviders: () => call<ProviderView[]>("model_providers"),
  saveProvider: (profile: ProfileInput, apiKey: string | null) =>
    call<ProviderView>("model_save_provider", { profile, api_key: apiKey }),
  removeProvider: (id: string) => call<void>("model_remove_provider", { id }),
  setKey: (id: string, apiKey: string) => call<ProviderView>("model_set_key", { id, api_key: apiKey }),
  clearKey: (id: string) => call<ProviderView>("model_clear_key", { id }),
  listModels: (id: string) => call<ModelInfo[]>("model_list_models", { id }),
  testConnection: (id: string) => call<ConnectionReport>("model_test_connection", { id }),
  modelCatalog: () => call<ProviderSpec[]>("model_catalog"),
  setDefaultProvider: (id: string) => call<ProviderView[]>("model_set_default", { id }),
  exportProviders: () => call<string>("model_export_config"),
  importProviders: (json: string) => call<ProviderView[]>("model_import_config", { json }),
  libraryComponents: () => call<LibraryEntry[]>("library_components"),
  librarySearch: (query: string, category: string, source: string) => call<LibraryPart[]>("library_search", { query, category, source }),
  libraryFootprintCandidates: (componentId: string) => call<FootprintCandidate[]>("library_footprint_candidates", { component_id: componentId }),
  /** Raw bytes of a component 3D model file (absolute path; .step/.stp/.wrl/.glb only). */
  libraryModelRead: (path: string) => call<ArrayBuffer>("library_model_read", { path }),
  domainClassification: () => call<DomainReport>("domain_classification"),
  intelligenceSummary: () => call<IntelligenceSummary>("intelligence_summary"),
  exportManufacturing: () => call<ExportResult>("export_manufacturing"),
  // D3 outputs: the BOM dialog preview and export, and the schematic/netlist documents.
  bomTable: (config: BomConfig) => call<BomTable>("bom_table", { config }),
  exportBom: (config: BomConfig, format: BomFormat) => call<ExportResult>("export_bom", { bom: { config, format } }),
  exportDocuments: (request: OutputRequest) => call<ExportResult>("export_documents", { request }),
  /** B5: re-run `check` on the current revision and write its report as CSV or JSON into `output/rev-<N>/findings`. */
  exportFindings: (check: Check, format: "csv" | "json") => call<FindingsExport>("export_findings", { check, format }),
  // E7 release check: read-only over the saved reports and the engineering gates; the run saves ERC/DRC/MECH/DFM evidence.
  releaseCheck: () => call<ReleaseCheck>("release_check"),
  runReleaseChecks: () => call<ReleaseCheck>("run_release_checks"),
  connectivity: () => call<ConnectivityReport>("connectivity"),
  /** E1 (ADR-0052): the board as fabricated, the geometry DRC checks; the 2D and 3D views draw it. */
  boardGeometry: () => call<BoardGeometry>("board_geometry"),
  aiPropose: (providerId: string, prompt: string) => call<AiProposal>("ai_propose", { provider_id: providerId, prompt }),
  aiAccept: (stepId: number) => call<CommandResponse>("ai_accept", { step_id: stepId }),
  aiReject: (stepId: number) => call<void>("ai_reject", { step_id: stepId }),
  aiRollback: (opId: string) => call<CommandResponse>("ai_rollback", { op_id: opId }),
  simulationStatus: () => call<SimulationStatus>("simulation_status"),
  runSimulation: (request: SimRequest) => call<SimResult>("run_simulation", { request }),
  routeStep: (request: RouteStepRequest) => call<RouteStepReport>("route_step", { request }),
  partsSearch: (query: string) => call<PartsSearch>("parts_search", { query }),
  partsImport: (expectedRevision: number, distributor: string) =>
    call<ImportResult>("parts_import", { expected_revision: expectedRevision, distributor }),
  distributorSettings: () => call<DistributorView>("distributor_settings_get"),
  distributorSaveCredentials: (key: string, secret: string) => call<DistributorView>("distributor_settings_save", { key, secret }),
  distributorClearCredentials: () => call<DistributorView>("distributor_settings_clear"),
  distributorTestConnection: () => call<DistributorView>("distributor_test_connection"),
  catalogueCategories: (refresh: boolean) => call<CatalogueReply<CategoryNode[]>>("catalogue_categories", { refresh }),
  catalogueManufacturers: (refresh: boolean) => call<CatalogueReply<Manufacturer[]>>("catalogue_manufacturers", { refresh }),
  catalogueSearch: (query: CatalogueQuery) => call<CatalogueReply<CatalogueResults>>("catalogue_search", { query }),
  partsSaveRecord: (distributor: string) => call<SavedRecord>("parts_save_record", { distributor }),
  libraryBrowse: (query: LibraryBrowseQuery) => call<LibraryBrowse>("library_browse", { query }),
  libraryCatalog: () => call<LibraryCatalogView>("library_catalog"),
  librarySetEnabled: (id: string, enabled: boolean) => call<LibraryCatalogView>("library_set_enabled", { id, enabled }),
  libraryMeta: (id: string, change: { favourite?: boolean; tags?: string[]; touch?: boolean }) =>
    call<LibraryMeta>("library_meta", { id, ...change }),
  libraryPersonalSave: (id: string) => call<void>("library_personal_save", { id }),
  libraryPersonalRemove: (id: string) => call<void>("library_personal_remove", { id }),
  libraryImportOpenSource: (dir: string) => call<LibraryImportSummary>("library_import_open_source", { dir }),
  libraryImportedRemove: (name: string) => call<void>("library_imported_remove", { name }),
  assemblyAttest: (distributor: string, cls: AssemblyClass) => call<unknown>("assembly_attest", { distributor, class: cls }),
  assemblyImportPartsFile: (path: string) =>
    call<{ file: string; recorded: number; unknown_class: number }>("assembly_import_parts_file", { path }),
  manufacturingPreview: () => call<ManufacturingPreview>("manufacturing_preview"),
  /** Part standardisation (D1): the SOURCING check over the placed parts and the stored supplier records (read-only, offline). */
  partStandardisation: () => call<Report>("part_standardisation"),
  fabricatorPrepare: (outDir: string) => call<PrepareResult>("fabricator_prepare", { out_dir: outDir }),
  // IDE shell: several open projects (one active) and the sandboxed project file explorer. Every
  // file call names its project, so a late call from one project can never act on another.
  openProjects: () => call<OpenProject[]>("open_projects"),
  switchProject: (projectId: string) => call<Snapshot>("switch_project", { project_id: projectId }),
  closeProjectById: (projectId: string) => call<Snapshot | null>("close_project_by_id", { project_id: projectId }),
  operationChanges: (projectId: string, opId: string) => call<EngineChange[]>("operation_changes", { project_id: projectId, op_id: opId }),
  // ECO preview (slice C2): what Update PCB would change at `revision`, computed by the engine on a
  // copy (read-only); the export saves that preview as project evidence.
  syncPreview: (revision: number) => call<SyncPreview>("sync_preview", { expected_revision: revision }),
  syncPreviewExport: (revision: number, excluded: string[]) => call<string>("sync_preview_export", { expected_revision: revision, excluded }),
  // Agent proposals (slice S9, ADR-0051): an agent session in propose mode stores its edits as a
  // proposal; EAK reviews it on a copy (read-only) and only the person applies it, as one Batch at
  // the expected revision. `steps` null means every step.
  proposalList: () => call<ProposalSummary[]>("proposal_list"),
  proposalReview: (id: string, steps: number[] | null) => call<ProposalReview>("proposal_review", { id, steps }),
  proposalApply: (id: string, revision: number, steps: number[] | null) => call<ApplyOutcome>("proposal_apply", { id, expected_revision: revision, steps }),
  proposalDiscard: (id: string) => call<ProposalSummary>("proposal_discard", { id }),
  proposalExport: (id: string, steps: number[] | null) => call<string>("proposal_export", { id, steps }),
  // Agent activity (ADR-0053): revisions, proposal events, WEFT messages and saved checks, read from the project journals.
  activityTimeline: () => call<import("./shell/activity").ActivityItem[]>("activity_timeline"),
  projectFiles: (projectId: string, rel: string) => call<FileEntry[]>("project_files", { project_id: projectId, rel }),
  projectFileRead: (projectId: string, rel: string) => call<FileContent>("project_file_read", { project_id: projectId, rel }),
  projectFileCreate: (projectId: string, rel: string, isDir: boolean) =>
    call<FileEntry>("project_file_create", { project_id: projectId, rel, is_dir: isDir }),
  projectFileRename: (projectId: string, from: string, to: string) => call<void>("project_file_rename", { project_id: projectId, from, to }),
  projectGitStatus: (projectId: string) => call<GitStatus>("project_git_status", { project_id: projectId }),
  // Central resource profile (src-tauri/src/resources.rs): job limits, cache budget, 3D tier, scrollback.
  resourceProfile: () => call<ResourceProfile>("resource_profile"),
  performanceSettings: () => call<PerformanceSettings>("performance_settings"),
  setPerformanceSettings: (settings: PerformanceSettings) => call<ResourceProfile>("set_performance_settings", { settings }),
  projectFileDelete: (projectId: string, rel: string, recursive: boolean) =>
    call<void>("project_file_delete", { project_id: projectId, rel, recursive }),
  // Settings › AI tools (src-tauri/src/agents.rs): locally installed coding-agent CLIs, probed with
  // their own commands; Connect registers eak-mcp through the tool's own CLI; Sign in and terminal
  // sessions open as tabs of the existing Terminal workspace.
  agentTools: () => call<ToolStatus[]>("agent_tools"),
  agentToolConnect: (tool: string) => call<ToolStatus>("agent_tool_connect", { tool }),
  agentToolDisconnect: (tool: string) => call<ToolStatus>("agent_tool_disconnect", { tool }),
  agentToolLogin: (tool: string) => call<TerminalSessionInfo>("agent_tool_login", { tool }),
  agentToolLaunch: (tool: string, surface: AgentSurface, prompt: string | null) =>
    call<AgentSession>("agent_tool_launch", { tool, surface, prompt }),
  agentSessions: () => call<AgentSession[]>("agent_sessions"),
  agentSessionLog: (id: string, lines: number) => call<string>("agent_session_log", { id, lines }),
  agentToolStop: (id: string) => call<AgentSession>("agent_tool_stop", { id }),
};

export function describeError(e: CommandError): string {
  const m = e.message;
  if (typeof m === "string") return `${e.code}: ${m}`;
  if (m && typeof m === "object") return `${e.code}: ${JSON.stringify(m)}`;
  return e.code;
}
