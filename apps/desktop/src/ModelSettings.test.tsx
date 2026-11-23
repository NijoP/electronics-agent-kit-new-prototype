// Model provider settings against a mocked Tauri IPC: catalog-driven provider types (fixed
// endpoints are not typed), write-only keys, staged connection results with error classes,
// model selector metadata, default provider, export/import without secrets.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import type { ConnectionReport, ModelInfo, ProviderSpec, ProviderView } from "./types";

type Call = { cmd: string; args: Record<string, any> };
let calls: Call[];
let providers: ProviderView[];
let modelListFails: boolean;
// When true, `performance_settings` fails (S21: the Performance section must say so, with Retry).
let performanceFails: boolean;
let nextReport: ConnectionReport;
const SECRET = "sk-ui-secret-123";

const model = (id: string, extra: Partial<ModelInfo> = {}): ModelInfo => ({
  id,
  name: null,
  context_window: null,
  max_output_tokens: null,
  tools: null,
  vision: null,
  source: "discovered",
  ...extra,
});

const spec = (s: Partial<ProviderSpec> & Pick<ProviderSpec, "id" | "name">): ProviderSpec => ({
  group: "cloud",
  kind: "open_ai_compatible",
  base_url: null,
  base_url_editable: false,
  auth: "api_key",
  org_project: false,
  discovery: true,
  implemented: true,
  note: null,
  models: [],
  ...s,
});

const CATALOG: ProviderSpec[] = [
  spec({ id: "openai", name: "OpenAI", base_url: "https://api.openai.com/v1", org_project: true }),
  spec({
    id: "anthropic",
    name: "Anthropic",
    kind: "anthropic",
    base_url: "https://api.anthropic.com",
    models: [model("claude-opus-5-5", { name: "Claude Opus 5.5", tools: true, source: "catalog" })],
  }),
  spec({ id: "ollama", name: "Ollama (local)", group: "local", kind: "ollama", base_url: "http://localhost:11434", base_url_editable: true, auth: "optional_api_key" }),
  spec({ id: "openai-compatible", name: "Custom OpenAI-compatible endpoint", group: "custom", base_url_editable: true, auth: "optional_api_key" }),
  spec({ id: "amazon-bedrock", name: "Amazon Bedrock", implemented: false, discovery: false, note: "Needs AWS SigV4 request signing." }),
];

const connected: ConnectionReport = {
  state: "connected",
  ok: true,
  latency_ms: 42,
  message: "Connected to anthropic:claude-opus-5-5; streamed 1 chunk(s); the model replied: OK",
  error_class: null,
  stages: [
    { stage: "configuration", ok: true, message: "endpoint https://api.anthropic.com" },
    { stage: "authentication", ok: true, message: "credentials accepted; 1 model(s) listed" },
    { stage: "model", ok: true, message: "claude-opus-5-5 is offered" },
    { stage: "invocation", ok: true, message: "streamed" },
  ],
  models: [],
};

function view(p: Record<string, any>, hasKey: boolean): ProviderView {
  const s = CATALOG.find((x) => x.id === p.provider)!;
  return {
    id: p.id ?? "prov-1",
    label: p.label,
    provider: p.provider,
    kind: p.kind,
    base_url: p.base_url ?? null,
    model: p.model ?? null,
    org_id: p.org_id ?? null,
    project_id: p.project_id ?? null,
    models: [],
    status: { state: "untested", error_class: null, message: null, checked_at_ms: null, model: null },
    provider_name: s.name,
    endpoint: p.base_url ?? s.base_url ?? "",
    has_key: hasKey,
    needs_key: s.auth === "api_key",
    key_error: null,
    is_default: true,
  };
}

beforeEach(() => {
  calls = [];
  providers = [];
  modelListFails = false;
  performanceFails = false;
  nextReport = connected;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, any>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [];
      case "performance_settings":
        if (performanceFails) throw "settings file unreadable";
        return { mode: "balanced", custom: { analysis_jobs: 2, background_jobs: 1, cache_budget_mb: 256, pixel_ratio_cap: 2, antialias: true, scrollback_lines: 5000 } };
      case "model_catalog":
        return CATALOG;
      case "model_providers":
        return providers;
      case "model_save_provider": {
        const prev = providers.find((x) => x.id === args.profile.id);
        const v = view(args.profile, Boolean(args.api_key) || (prev?.has_key ?? false));
        providers = [...providers.filter((x) => x.id !== v.id), v];
        return v;
      }
      case "model_list_models":
        if (modelListFails) throw "Invalid base URL: HTTP 404";
        return [model("gpt-big", { name: "GPT Big", context_window: 200000, max_output_tokens: 8192, tools: true, vision: false })];
      case "model_test_connection": {
        const state = nextReport.state === "connected" ? "connected" : "failed";
        providers = providers.map((x) => (x.id === args.id ? { ...x, status: { ...x.status, state, error_class: nextReport.error_class } } : x));
        return nextReport;
      }
      case "model_set_default":
        providers = providers.map((x) => ({ ...x, is_default: x.id === args.id }));
        return providers;
      case "model_remove_provider":
        providers = providers.filter((x) => x.id !== args.id);
        return null;
      case "model_clear_key":
        providers = providers.map((x) => (x.id === args.id ? { ...x, has_key: false } : x));
        return providers.find((x) => x.id === args.id);
      case "model_export_config":
        return JSON.stringify({ format: "eak-model-providers", version: 1, providers: providers.map((p) => ({ id: p.id, label: p.label, provider: p.provider })) });
      case "model_import_config":
        return providers;
      default:
        throw new Error(`unexpected ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

async function openPanel() {
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Settings" }));
  fireEvent.click(screen.getByRole("menuitem", { name: /Model and provider settings/ }));
  await screen.findByRole("dialog", { name: "Model settings" });
}

async function addProvider(type: string) {
  fireEvent.click(await screen.findByRole("button", { name: "Add provider" }));
  fireEvent.change(screen.getByLabelText("Provider type"), { target: { value: type } });
}

describe("model provider settings", () => {
  it("is labelled optional and says terminal agents keep their own sign-in", async () => {
    await openPanel();
    expect(screen.getByRole("heading", { name: "Model providers (optional)" })).toBeTruthy();
    expect(screen.getByText(/use their own sign-in; EAK never reads their credentials/)).toBeTruthy();
  });

  // Regression (unrun): Escape must close the dialog even when focus is outside it.
  it("Escape closes the dialog", async () => {
    await openPanel();
    fireEvent.keyDown(document.body, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Model settings" })).toBeNull());
  });

  it("known providers have a fixed endpoint: no base URL to type", async () => {
    await openPanel();
    await addProvider("anthropic");
    expect(screen.queryByLabelText("Base URL")).toBeNull();
    expect(screen.getByLabelText("Endpoint").textContent).toContain("https://api.anthropic.com (fixed)");
    // OpenAI offers organization / project ids; Anthropic does not.
    expect(screen.queryByLabelText("Organization ID (optional)")).toBeNull();
    fireEvent.change(screen.getByLabelText("Provider type"), { target: { value: "openai" } });
    expect(screen.getByLabelText("Organization ID (optional)")).toBeTruthy();
  });

  it("save & test: key sent once, never rendered back, staged result shown", async () => {
    await openPanel();
    await addProvider("anthropic");
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: SECRET } });
    // The catalog suggestion is selectable in the model table.
    const table = screen.getByRole("table", { name: "Models" });
    expect(within(table).getByText("built-in suggestion (not verified)")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Use claude-opus-5-5"));
    fireEvent.click(screen.getByRole("button", { name: "Save & test" }));
    const status = await screen.findByRole("status");
    expect(status.textContent).toContain("Connected");
    expect(status.textContent).toContain("✓ invocation");

    const save = calls.find((c) => c.cmd === "model_save_provider")!;
    expect(save.args).toEqual({
      profile: { id: null, label: "Anthropic", provider: "anthropic", kind: "anthropic", base_url: null, model: "claude-opus-5-5", org_id: null, project_id: null },
      api_key: SECRET,
    });
    expect(calls.find((c) => c.cmd === "model_test_connection")!.args).toEqual({ id: "prov-1" });
    expect((screen.getByLabelText("API key") as HTMLInputElement).value).toBe("");
    expect(document.body.innerHTML).not.toContain(SECRET);
    await waitFor(() => expect(screen.getByLabelText("Status of Anthropic").textContent).toBe("Connected"));
  });

  it("a saved but untested provider is never shown as connected", async () => {
    await openPanel();
    await addProvider("anthropic");
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: SECRET } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText(/Saved. Not tested yet/);
    expect(screen.getByLabelText("Status of Anthropic").textContent).toBe("Not tested");
    expect(calls.some((c) => c.cmd === "model_test_connection")).toBe(false);
  });

  it("shows the failure class of a failed test", async () => {
    nextReport = {
      ...connected,
      state: "failed",
      ok: false,
      error_class: "invalid_api_key",
      message: "Invalid API key: provider rejected the credentials (HTTP 401)",
      stages: [{ stage: "authentication", ok: false, message: "HTTP 401" }],
    };
    await openPanel();
    await addProvider("anthropic");
    fireEvent.click(screen.getByRole("button", { name: "Save & test" }));
    const status = await screen.findByRole("status");
    expect(status.textContent).toContain("Invalid API key");
    expect(status.textContent).toContain("✗ authentication");
    await waitFor(() => expect(screen.getByLabelText("Status of Anthropic").textContent).toBe("Failed: Invalid API key"));
  });

  it("discovers models with capabilities and context limits, and falls back to typing an id", async () => {
    await openPanel();
    await addProvider("openai");
    fireEvent.click(screen.getByRole("button", { name: "Discover models" }));
    await screen.findByText(/1 model\(s\) discovered/);
    const row = screen.getByText("GPT Big").closest("tr")!;
    expect(row.textContent).toContain("gpt-big");
    expect(row.textContent).toContain("200k");
    expect(row.textContent).toContain("8k");
    expect(row.textContent).toContain("listed by provider");

    modelListFails = true;
    fireEvent.click(screen.getByRole("button", { name: "Discover models" }));
    await screen.findByText(/did not return a model list/);
    expect(screen.queryByRole("table", { name: "Models" })).toBeNull();
    fireEvent.change(screen.getByLabelText("Model identifier"), { target: { value: "typed-model" } });
    expect((screen.getByLabelText("Model identifier") as HTMLInputElement).value).toBe("typed-model");
  });

  it("custom endpoints take a base URL and an optional key", async () => {
    await openPanel();
    await addProvider("openai-compatible");
    expect(screen.getByLabelText("API key (optional)")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Provider name"), { target: { value: "My vLLM" } });
    fireEvent.change(screen.getByLabelText("Base URL"), { target: { value: "http://10.0.0.5:8000/v1" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText(/Saved/);
    expect(calls.find((c) => c.cmd === "model_save_provider")!.args).toMatchObject({
      profile: { provider: "openai-compatible", kind: "open_ai_compatible", base_url: "http://10.0.0.5:8000/v1", label: "My vLLM" },
      api_key: null,
    });
  });

  it("lists unimplemented providers as disabled with the reason", async () => {
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Add provider" }));
    const opt = screen.getByRole("option", { name: "Amazon Bedrock (not implemented)" }) as HTMLOptionElement;
    expect(opt.disabled).toBe(true);
    expect(opt.title).toContain("SigV4");
  });

  it("sets the default provider and removes a provider after confirmation", async () => {
    providers = [
      view({ id: "p1", label: "Groq", provider: "openai", kind: "open_ai_compatible", model: "m" }, true),
      { ...view({ id: "p2", label: "Local", provider: "ollama", kind: "ollama", model: "llama3" }, false), is_default: false },
    ];
    window.confirm = () => true;
    await openPanel();
    expect(screen.queryByRole("button", { name: "Set Groq as default" })).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: "Set Local as default" }));
    await screen.findByRole("button", { name: "Set Groq as default" });
    expect(calls.some((c) => c.cmd === "model_set_default" && c.args.id === "p2")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Remove Groq" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "model_remove_provider")?.args).toEqual({ id: "p1" }));
  });

  it("exports without keys and imports through the backend", async () => {
    providers = [view({ id: "p1", label: "Claude", provider: "anthropic", kind: "anthropic", model: "m" }, true)];
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Export (no keys)" }));
    const out = (await screen.findByLabelText("Exported provider configuration")) as HTMLTextAreaElement;
    expect(out.value).toContain("eak-model-providers");
    fireEvent.click(screen.getByRole("button", { name: "Import" }));
    fireEvent.change(screen.getByLabelText("Provider configuration to import"), { target: { value: '{"format":"eak-model-providers"}' } });
    fireEvent.click(screen.getByRole("button", { name: "Import providers" }));
    await screen.findByText(/Enter the API keys again/);
    expect(calls.find((c) => c.cmd === "model_import_config")!.args.json).toContain("eak-model-providers");
  });

  it("shows backend errors from a failed save", async () => {
    await openPanel();
    await addProvider("openai-compatible");
    mockIPC((cmd) => {
      if (cmd === "model_save_provider") throw "Custom OpenAI-compatible endpoint needs a base URL";
      return [];
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect((await screen.findByRole("alert")).textContent).toContain("needs a base URL");
  });
});

// S21: the Performance section vanished silently when its settings could not be read.
describe("Performance settings read error", () => {
  it("shows why the settings could not be read and reads them again on Retry", async () => {
    performanceFails = true;
    await openPanel();
    const perf = await screen.findByRole("region", { name: "Performance" });
    expect((await within(perf).findByRole("alert")).textContent).toContain("settings file unreadable");
    performanceFails = false;
    fireEvent.click(within(perf).getByRole("button", { name: "Retry" }));
    expect(await screen.findByLabelText(/Performance mode/)).toBeTruthy();
    expect(calls.filter((c) => c.cmd === "performance_settings").length).toBe(2);
  });
});
