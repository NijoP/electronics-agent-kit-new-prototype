// Model settings: add/remove providers, enter a key, pick a model, test the connection.
// Keys go straight to the OS credential store through IPC and are never shown again: the
// key field is write-only and this component only ever learns `has_key`.
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { ConnectionReport, ProfileInput, ProviderKind, ProviderView } from "./types";

interface Preset {
  id: string;
  name: string;
  kind: ProviderKind;
  base_url: string;
}

// Hosted services that speak the OpenAI API are just presets over "OpenAI-compatible".
export const PRESETS: Preset[] = [
  { id: "anthropic", name: "Anthropic", kind: "anthropic", base_url: "https://api.anthropic.com" },
  { id: "openai", name: "OpenAI", kind: "open_ai_compatible", base_url: "https://api.openai.com/v1" },
  { id: "openrouter", name: "OpenRouter", kind: "open_ai_compatible", base_url: "https://openrouter.ai/api/v1" },
  { id: "groq", name: "Groq", kind: "open_ai_compatible", base_url: "https://api.groq.com/openai/v1" },
  { id: "together", name: "Together AI", kind: "open_ai_compatible", base_url: "https://api.together.xyz/v1" },
  { id: "ollama", name: "Ollama (local)", kind: "ollama", base_url: "http://localhost:11434" },
  { id: "custom", name: "Other OpenAI-compatible (custom URL)", kind: "open_ai_compatible", base_url: "" },
];

interface Draft {
  id: string | null;
  preset: string;
  label: string;
  base_url: string;
  model: string;
  key: string;
  hasKey: boolean;
}

const blank = (): Draft => ({ id: null, preset: "anthropic", label: "Anthropic", base_url: PRESETS[0].base_url, model: "", key: "", hasKey: false });

function presetFor(p: ProviderView): string {
  return PRESETS.find((x) => x.kind === p.kind && x.base_url === (p.base_url ?? x.base_url))?.id ?? (p.kind === "anthropic" ? "anthropic" : p.kind === "ollama" ? "ollama" : "custom");
}

export function ModelSettings({ onClose }: { onClose: () => void }) {
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [report, setReport] = useState<ConnectionReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => setProviders(await api.modelProviders()), []);
  useEffect(() => {
    reload().catch((e) => setError(String(e)));
  }, [reload]);

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const edit = (p: ProviderView) => {
    setModels([]);
    setReport(null);
    setInfo(null);
    setDraft({ id: p.id, preset: presetFor(p), label: p.label, base_url: p.base_url ?? "", model: p.model ?? "", key: "", hasKey: p.has_key });
  };

  const choosePreset = (id: string) => {
    const p = PRESETS.find((x) => x.id === id)!;
    setDraft((d) => d && { ...d, preset: id, base_url: p.base_url, label: d.id ? d.label : p.id === "custom" ? "" : p.name });
  };

  const save = async (d: Draft): Promise<ProviderView> => {
    const preset = PRESETS.find((x) => x.id === d.preset)!;
    const input: ProfileInput = {
      id: d.id,
      label: d.label,
      kind: preset.kind,
      base_url: d.base_url.trim() || null,
      model: d.model.trim() || null,
    };
    const saved = await api.saveProvider(input, d.key.trim() || null);
    setDraft({ ...d, id: saved.id, key: "", hasKey: saved.has_key });
    await reload();
    return saved;
  };

  const doSave = () =>
    run(async () => {
      await save(draft!);
      setInfo("Saved.");
    });

  const fetchModels = () =>
    run(async () => {
      const saved = await save(draft!);
      setInfo(null);
      try {
        const list = await api.listModels(saved.id);
        setModels(list);
        setInfo(`${list.length} model(s) available. Pick one or type an id.`);
      } catch (e) {
        setModels([]);
        setInfo(`This provider did not return a model list (${String(e)}). Type a model id instead.`);
      }
    });

  const test = () =>
    run(async () => {
      const saved = await save(draft!);
      setReport(null);
      setReport(await api.testConnection(saved.id));
    });

  const remove = (p: ProviderView) =>
    run(async () => {
      if (!window.confirm(`Remove ${p.label} and delete its stored key?`)) return;
      await api.removeProvider(p.id);
      if (draft?.id === p.id) setDraft(null);
      await reload();
    });

  const clearKey = () =>
    run(async () => {
      const v = await api.clearKey(draft!.id!);
      setDraft({ ...draft!, hasKey: v.has_key });
      await reload();
      setInfo("Stored key deleted.");
    });

  return (
    <div className="modal-backdrop">
      <div className="modal" role="dialog" aria-modal="true" aria-label="Model settings">
        <header className="modal-head">
          <h2>Model settings</h2>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </header>
        <p className="muted">
          API keys are stored only in your operating system credential store. They are never written to project files or logs and are
          not shown again after you save them.
        </p>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <section aria-label="Providers">
          {providers.length === 0 && <p className="muted">No model providers yet.</p>}
          <ul className="providers">
            {providers.map((p) => (
              <li key={p.id}>
                <span>
                  <strong>{p.label}</strong> <span className="muted">{p.model ?? "no model chosen"}</span>
                  <br />
                  <span className="muted">
                    {p.base_url ?? "default endpoint"} ·{" "}
                    {p.key_error ? `key store error: ${p.key_error}` : p.has_key ? "key stored" : p.needs_key ? "no key" : "no key needed"}
                  </span>
                </span>
                <span className="row">
                  <button type="button" disabled={busy} onClick={() => edit(p)} aria-label={`Edit ${p.label}`}>
                    Edit
                  </button>
                  <button type="button" disabled={busy} onClick={() => remove(p)} aria-label={`Remove ${p.label}`}>
                    Remove
                  </button>
                </span>
              </li>
            ))}
          </ul>
          {!draft && (
            <button type="button" onClick={() => { setModels([]); setReport(null); setInfo(null); setDraft(blank()); }}>
              Add provider
            </button>
          )}
        </section>
        {draft && (
          <form
            aria-label="Provider details"
            onSubmit={(e) => {
              e.preventDefault();
              doSave();
            }}
          >
            <h3>{draft.id ? "Edit provider" : "Add provider"}</h3>
            <label>
              Provider
              <select value={draft.preset} onChange={(e) => choosePreset(e.target.value)}>
                {PRESETS.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Provider name
              <input value={draft.label} onChange={(e) => setDraft({ ...draft, label: e.target.value })} />
            </label>
            <label>
              Base URL
              <input value={draft.base_url} onChange={(e) => setDraft({ ...draft, base_url: e.target.value })} />
            </label>
            <label>
              API key
              <input
                type="password"
                autoComplete="off"
                value={draft.key}
                placeholder={draft.hasKey ? "stored in OS credential storage (leave blank to keep)" : "paste key"}
                onChange={(e) => setDraft({ ...draft, key: e.target.value })}
              />
            </label>
            {draft.hasKey && draft.id && (
              <button type="button" disabled={busy} onClick={clearKey}>
                Delete stored key
              </button>
            )}
            <label>
              Model
              <input list="model-options" value={draft.model} onChange={(e) => setDraft({ ...draft, model: e.target.value })} />
              <datalist id="model-options">
                {models.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </label>
            <div className="row">
              <button type="submit" disabled={busy}>
                Save
              </button>
              <button type="button" disabled={busy} onClick={fetchModels}>
                Fetch models
              </button>
              <button type="button" disabled={busy} onClick={test}>
                Test connection
              </button>
              <button type="button" disabled={busy} onClick={() => setDraft(null)}>
                Done
              </button>
            </div>
            {info && <p className="muted">{info}</p>}
            {report && (
              <p role="status" className={report.ok ? "ok" : "error"}>
                {report.ok ? "Connection OK" : "Connection failed"} ({report.latency_ms} ms): {report.message}
              </p>
            )}
          </form>
        )}
      </div>
    </div>
  );
}
