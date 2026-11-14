// Typed client for the Tauri IPC commands in src-tauri/src/lib.rs. This is the only
// module that talks to Rust; every project change is an engine call made here.
import { invoke } from "@tauri-apps/api/core";
import type {
  Check,
  CheckResult,
  Command,
  CommandError,
  CommandResponse,
  ConnectivityReport,
  ConnectionReport,
  EngineEvent,
  ProfileInput,
  ProviderView,
  ExportResult,
  ImportResult,
  LibraryEntry,
  ManufacturingPreview,
  PartsSearch,
  PrepareResult,
  RecentProject,
  DomainReport,
  Snapshot,
} from "./types";

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
  "library_components",
  "domain_classification",
  "export_manufacturing",
  "connectivity",
  "parts_search",
  "parts_import",
  "manufacturing_preview",
  "jlcpcb_prepare",
] as const;
type IpcCommand = (typeof IPC_COMMANDS)[number];

function call<T>(cmd: IpcCommand, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
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
  listModels: (id: string) => call<string[]>("model_list_models", { id }),
  testConnection: (id: string) => call<ConnectionReport>("model_test_connection", { id }),
  libraryComponents: () => call<LibraryEntry[]>("library_components"),
  domainClassification: () => call<DomainReport>("domain_classification"),
  exportManufacturing: () => call<ExportResult>("export_manufacturing"),
  connectivity: () => call<ConnectivityReport>("connectivity"),
  partsSearch: (query: string) => call<PartsSearch>("parts_search", { query }),
  partsImport: (expectedRevision: number, lcsc: string) =>
    call<ImportResult>("parts_import", { expected_revision: expectedRevision, lcsc }),
  manufacturingPreview: () => call<ManufacturingPreview>("manufacturing_preview"),
  jlcpcbPrepare: (outDir: string) => call<PrepareResult>("jlcpcb_prepare", { out_dir: outDir }),
};

export function describeError(e: CommandError): string {
  const m = e.message;
  if (typeof m === "string") return `${e.code}: ${m}`;
  if (m && typeof m === "object") return `${e.code}: ${JSON.stringify(m)}`;
  return e.code;
}
