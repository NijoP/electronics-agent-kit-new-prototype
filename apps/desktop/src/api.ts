// Typed client for the Tauri IPC commands in src-tauri/src/lib.rs. This is the only
// module that talks to Rust; every project change is an engine call made here.
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  AiProposal,
  RouteStepReport,
  RouteStepRequest,
  Check,
  CheckResult,
  Command,
  CommandError,
  CommandResponse,
  ConnectivityReport,
  ConnectionReport,
  EngineEvent,
  ModelInfo,
  ProfileInput,
  ProviderSpec,
  ProviderView,
  ExportResult,
  ImportResult,
  LibraryEntry,
  LibraryPart,
  FootprintCandidate,
  ManufacturingPreview,
  PartsSearch,
  PrepareResult,
  RecentProject,
  DomainReport,
  SimRequest,
  SimResult,
  SimulationStatus,
  Snapshot,
  OpenProject,
} from "./types";
import type { FileContent, FileEntry, GitStatus } from "./shell/files";
import type { EngineChange } from "./shell/syncReport";

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
] as const;

// Must match `IPC_COMMANDS` in src-tauri/src/lib.rs (checked by tests/ipc.rs).
export const IPC_COMMANDS = [
  "create_project",
  "open_project",
  "close_project",
  "current_project",
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
  "domain_classification",
  "export_manufacturing",
  "connectivity",
  "ai_propose",
  "ai_accept",
  "ai_reject",
  "ai_rollback",
  "simulation_status",
  "run_simulation",
  "route_step",
  "parts_search",
  "parts_import",
  "manufacturing_preview",
  "jlcpcb_prepare",
  "open_projects",
  "switch_project",
  "close_project_by_id",
  "operation_changes",
  "project_files",
  "project_file_read",
  "project_file_create",
  "project_file_rename",
  "project_file_delete",
  "project_git_status",
  ...TERMINAL_IPC,
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

export const api = {
  createProject: (path: string, name: string) => call<Snapshot>("create_project", { path, name }),
  openProject: (path: string, takeOverLock: boolean) =>
    call<Snapshot>("open_project", { path, take_over_lock: takeOverLock }),
  closeProject: () => call<void>("close_project"),
  currentProject: () => call<Snapshot>("current_project"),
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
  domainClassification: () => call<DomainReport>("domain_classification"),
  exportManufacturing: () => call<ExportResult>("export_manufacturing"),
  connectivity: () => call<ConnectivityReport>("connectivity"),
  aiPropose: (providerId: string, prompt: string) => call<AiProposal>("ai_propose", { provider_id: providerId, prompt }),
  aiAccept: (stepId: number) => call<CommandResponse>("ai_accept", { step_id: stepId }),
  aiReject: (stepId: number) => call<void>("ai_reject", { step_id: stepId }),
  aiRollback: (opId: string) => call<CommandResponse>("ai_rollback", { op_id: opId }),
  simulationStatus: () => call<SimulationStatus>("simulation_status"),
  runSimulation: (request: SimRequest) => call<SimResult>("run_simulation", { request }),
  routeStep: (request: RouteStepRequest) => call<RouteStepReport>("route_step", { request }),
  partsSearch: (query: string) => call<PartsSearch>("parts_search", { query }),
  partsImport: (expectedRevision: number, lcsc: string) =>
    call<ImportResult>("parts_import", { expected_revision: expectedRevision, lcsc }),
  manufacturingPreview: () => call<ManufacturingPreview>("manufacturing_preview"),
  jlcpcbPrepare: (outDir: string) => call<PrepareResult>("jlcpcb_prepare", { out_dir: outDir }),
  // IDE shell: several open projects (one active) and the sandboxed project file explorer. Every
  // file call names its project, so a late call from one project can never act on another.
  openProjects: () => call<OpenProject[]>("open_projects"),
  switchProject: (projectId: string) => call<Snapshot>("switch_project", { project_id: projectId }),
  closeProjectById: (projectId: string) => call<Snapshot | null>("close_project_by_id", { project_id: projectId }),
  operationChanges: (projectId: string, opId: string) => call<EngineChange[]>("operation_changes", { project_id: projectId, op_id: opId }),
  projectFiles: (projectId: string, rel: string) => call<FileEntry[]>("project_files", { project_id: projectId, rel }),
  projectFileRead: (projectId: string, rel: string) => call<FileContent>("project_file_read", { project_id: projectId, rel }),
  projectFileCreate: (projectId: string, rel: string, isDir: boolean) =>
    call<FileEntry>("project_file_create", { project_id: projectId, rel, is_dir: isDir }),
  projectFileRename: (projectId: string, from: string, to: string) => call<void>("project_file_rename", { project_id: projectId, from, to }),
  projectGitStatus: (projectId: string) => call<GitStatus>("project_git_status", { project_id: projectId }),
  projectFileDelete: (projectId: string, rel: string, recursive: boolean) =>
    call<void>("project_file_delete", { project_id: projectId, rel, recursive }),
};

export function describeError(e: CommandError): string {
  const m = e.message;
  if (typeof m === "string") return `${e.code}: ${m}`;
  if (m && typeof m === "object") return `${e.code}: ${JSON.stringify(m)}`;
  return e.code;
}
