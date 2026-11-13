// Model settings flow against a mocked Tauri IPC: command names, argument shapes, and that the
// key is write-only (it is sent once and never rendered back).
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import type { ProviderView } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let providers: ProviderView[];
let modelListFails: boolean;
const SECRET = "sk-ui-secret-123";

beforeEach(() => {
  calls = [];
  providers = [];
  modelListFails = false;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, any>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [];
      case "model_providers":
        return providers;
      case "model_save_provider": {
        const p = args.profile;
        const view: ProviderView = {
          id: p.id ?? "prov-1",
          label: p.label,
          kind: p.kind,
          base_url: p.base_url,
          model: p.model,
          has_key: Boolean(args.api_key) || (providers.find((x) => x.id === p.id)?.has_key ?? false),
          needs_key: p.kind !== "ollama",
          key_error: null,
        };
        providers = [...providers.filter((x) => x.id !== view.id), view];
        return view;
      }
      case "model_list_models":
        if (modelListFails) throw "HTTP 404: no model listing";
        return ["claude-a", "claude-b"];
      case "model_test_connection":
        return { ok: true, latency_ms: 42, message: "Connected to anthropic:claude-a; the model replied: OK" };
      case "model_remove_provider":
        providers = providers.filter((x) => x.id !== args.id);
        return null;
      case "model_clear_key":
        providers = providers.map((x) => (x.id === args.id ? { ...x, has_key: false } : x));
        return providers.find((x) => x.id === args.id);
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
  fireEvent.click(screen.getByRole("button", { name: "Model settings" }));
  await screen.findByRole("dialog", { name: "Model settings" });
}

describe("model settings panel", () => {
  it("adds a provider: key is sent once, never rendered back, and the test result is shown", async () => {
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Add provider" }));
    fireEvent.change(screen.getByLabelText("API key"), { target: { value: SECRET } });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "claude-a" } });
    fireEvent.click(screen.getByRole("button", { name: "Test connection" }));
    await screen.findByText(/Connection OK \(42 ms\)/);

    const save = calls.find((c) => c.cmd === "model_save_provider")!;
    expect(save.args).toEqual({
      profile: { id: null, label: "Anthropic", kind: "anthropic", base_url: "https://api.anthropic.com", model: "claude-a" },
      api_key: SECRET,
    });
    expect(calls.find((c) => c.cmd === "model_test_connection")!.args).toEqual({ id: "prov-1" });
    // The key field is cleared after saving and the key is nowhere in the DOM.
    expect((screen.getByLabelText("API key") as HTMLInputElement).value).toBe("");
    expect(document.body.innerHTML).not.toContain(SECRET);
    expect(screen.getByText(/key stored/)).toBeTruthy();
  });

  it("fetches models into the picker and falls back to free text when listing fails", async () => {
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Add provider" }));
    fireEvent.click(screen.getByRole("button", { name: "Fetch models" }));
    await screen.findByText(/2 model\(s\) available/);
    expect(document.querySelectorAll("#model-options option").length).toBe(2);

    modelListFails = true;
    fireEvent.click(screen.getByRole("button", { name: "Fetch models" }));
    await screen.findByText(/did not return a model list/);
    expect(document.querySelectorAll("#model-options option").length).toBe(0);
  });

  it("presets fill the base URL and kind; custom OpenAI-compatible accepts any URL", async () => {
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Add provider" }));
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "openrouter" } });
    expect((screen.getByLabelText("Base URL") as HTMLInputElement).value).toBe("https://openrouter.ai/api/v1");
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "custom" } });
    fireEvent.change(screen.getByLabelText("Provider name"), { target: { value: "My vLLM" } });
    fireEvent.change(screen.getByLabelText("Base URL"), { target: { value: "http://10.0.0.5:8000/v1" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText("Saved.");
    expect(calls.find((c) => c.cmd === "model_save_provider")!.args.profile).toMatchObject({
      kind: "open_ai_compatible",
      base_url: "http://10.0.0.5:8000/v1",
      label: "My vLLM",
    });
    expect(calls.find((c) => c.cmd === "model_save_provider")!.args.api_key).toBeNull();
  });

  it("removes a provider after confirmation", async () => {
    providers = [{ id: "p9", label: "Groq", kind: "open_ai_compatible", base_url: "https://api.groq.com/openai/v1", model: "m", has_key: true, needs_key: true, key_error: null }];
    window.confirm = () => true;
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Remove Groq" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "model_remove_provider")).toBe(true));
    expect(calls.find((c) => c.cmd === "model_remove_provider")!.args).toEqual({ id: "p9" });
    await screen.findByText("No model providers yet.");
  });

  it("shows provider errors from a failed save", async () => {
    await openPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Add provider" }));
    mockIPC((cmd) => {
      if (cmd === "model_save_provider") throw "base URL must start with http:// or https://";
      return [];
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect((await screen.findByRole("alert")).textContent).toContain("base URL must start with");
  });
});
