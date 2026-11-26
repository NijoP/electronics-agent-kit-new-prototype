// Model providers: optional infrastructure for EAK-native agent integrations, configured only
// here. Nothing in the EDA tools or the terminal needs it; terminal agents (pi, opencode,
// claude…) keep their own sign-in and EAK never reads their credentials.
//
// Provider types come from the backend catalog: known cloud endpoints are fixed (shown, not
// typed), local servers and custom endpoints take a URL, and providers EAK has not implemented
// are listed but disabled with the reason. Keys go straight to the OS credential store and are
// never shown again: the key field is write-only and this component only learns `has_key`.
// A provider shows "Connected" only after the backend's staged test streamed a real reply.
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { PerformancePanel } from "./PerformancePanel";
import { confirmed } from "./confirm";
import type { ConnectionReport, ErrorClass, ModelInfo, ProfileInput, ProviderSpec, ProviderView, StatusState } from "./types";

interface Draft {
  id: string | null;
  provider: string;
  label: string;
  base_url: string;
  model: string;
  org_id: string;
  project_id: string;
  key: string;
  hasKey: boolean;
}

export const ERROR_TITLES: Record<ErrorClass, string> = {
  missing_api_key: "API key required",
  invalid_api_key: "Invalid API key",
  authentication_failed: "Authentication failed",
  unsupported_model: "Unsupported model",
  invalid_base_url: "Invalid base URL",
  network_error: "Network error",
  timeout: "Request timed out",
  rate_limited: "Rate limit exceeded",
  provider_unavailable: "Provider unavailable",
  request_rejected: "Request rejected",
  invalid_response: "Invalid response",
  invalid_configuration: "Invalid configuration",
  not_implemented: "Not implemented",
  cancelled: "Cancelled",
};

const STATUS_NAMES: Record<StatusState, string> = {
  untested: "Not tested",
  connected: "Connected",
  authenticated: "Authenticated (choose a model)",
  failed: "Failed",
};

export function statusLabel(p: ProviderView): string {
  const s = p.status;
  if (s.state === "failed" && s.error_class) return `Failed: ${ERROR_TITLES[s.error_class]}`;
  return STATUS_NAMES[s.state];
}

const GROUPS: [ProviderSpec["group"], string][] = [
  ["cloud", "Cloud providers"],
  ["local", "Local servers"],
  ["custom", "Custom endpoints"],
];

const fmtTokens = (n: number | null) => (n == null ? "unknown" : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n));
const yesNo = (b: boolean | null) => (b == null ? "unknown" : b ? "yes" : "no");

/** Catalog suggestions plus discovered models, discovered entries winning. */
export function mergeModels(spec: ProviderSpec | undefined, discovered: ModelInfo[]): ModelInfo[] {
  const byId = new Map<string, ModelInfo>();
  for (const m of spec?.models ?? []) byId.set(m.id, m);
  for (const m of discovered) byId.set(m.id, m);
  return [...byId.values()];
}

function draftFor(p: ProviderView): Draft {
  return {
    id: p.id,
    provider: p.provider,
    label: p.label,
    base_url: p.base_url ?? "",
    model: p.model ?? "",
    org_id: p.org_id ?? "",
    project_id: p.project_id ?? "",
    key: "",
    hasKey: p.has_key,
  };
}

export function ModelSettings({ onClose }: { onClose: () => void }) {
  const [catalog, setCatalog] = useState<ProviderSpec[]>([]);
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [discovered, setDiscovered] = useState<ModelInfo[]>([]);
  const [report, setReport] = useState<ConnectionReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [exported, setExported] = useState<string | null>(null);
  const [importText, setImportText] = useState<string | null>(null);

  const reload = useCallback(async () => setProviders(await api.modelProviders()), []);
  // Escape closes the dialog wherever focus is (a handler on the dialog element misses
  // presses while focus is on the page behind or on the backdrop).
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  useEffect(() => {
    Promise.all([api.modelCatalog().then(setCatalog), reload()]).catch((e) => setError(String(e)));
  }, [reload]);

  const spec = useMemo(() => catalog.find((s) => s.id === draft?.provider), [catalog, draft?.provider]);
  const models = useMemo(() => mergeModels(spec, discovered), [spec, discovered]);

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

  const reset = () => {
    setReport(null);
    setInfo(null);
    setError(null);
  };

  const startNew = () => {
    reset();
    setDiscovered([]);
    const first = catalog.find((s) => s.implemented);
    if (!first) return;
    setDraft({ id: null, provider: first.id, label: first.name, base_url: "", model: "", org_id: "", project_id: "", key: "", hasKey: false });
  };

  const edit = (p: ProviderView) => {
    reset();
    setDiscovered(p.models);
    setDraft(draftFor(p));
  };

  const chooseProvider = (id: string) => {
    const s = catalog.find((x) => x.id === id);
    if (!s) return;
    setDiscovered([]);
    setReport(null);
    setDraft((d) => d && { ...d, provider: id, base_url: "", model: "", org_id: "", project_id: "", label: d.id ? d.label : s.name });
  };

  const save = async (d: Draft): Promise<ProviderView> => {
    const s = catalog.find((x) => x.id === d.provider)!;
    const input: ProfileInput = {
      id: d.id,
      label: d.label,
      provider: s.id,
      kind: s.kind,
      base_url: s.base_url_editable ? d.base_url.trim() || null : null,
      model: d.model.trim() || null,
      org_id: s.org_project ? d.org_id.trim() || null : null,
      project_id: s.org_project ? d.project_id.trim() || null : null,
    };
    const saved = await api.saveProvider(input, d.key.trim() || null);
    setDraft(draftFor(saved));
    await reload();
    return saved;
  };

  const doSave = () =>
    run(async () => {
      await save(draft!);
      setInfo("Saved. Not tested yet: use Save & test to check the connection.");
    });

  const discover = () =>
    run(async () => {
      const saved = await save(draft!);
      setInfo(null);
      try {
        const list = await api.listModels(saved.id);
        setDiscovered(list);
        setInfo(`${list.length} model(s) discovered. Pick one or type a model identifier.`);
      } catch (e) {
        setDiscovered([]);
        setInfo(`This provider did not return a model list (${String(e)}). Type a model identifier instead.`);
      }
      await reload();
    });

  const saveAndTest = () =>
    run(async () => {
      const saved = await save(draft!);
      setReport(null);
      setInfo(null);
      const r = await api.testConnection(saved.id);
      setReport(r);
      if (r.models.length) setDiscovered(r.models);
      await reload();
    });

  const testRow = (p: ProviderView) =>
    run(async () => {
      const r = await api.testConnection(p.id);
      await reload();
      setInfo(`${p.label}: ${r.message}`);
    });

  const makeDefault = (p: ProviderView) =>
    run(async () => {
      setProviders(await api.setDefaultProvider(p.id));
    });

  const remove = (p: ProviderView) =>
    run(async () => {
      if (!(await confirmed(`Remove ${p.label} and delete its stored key?`))) return;
      await api.removeProvider(p.id);
      if (draft?.id === p.id) setDraft(null);
      await reload();
    });

  const clearKey = () =>
    run(async () => {
      const v = await api.clearKey(draft!.id!);
      setDraft({ ...draft!, hasKey: v.has_key });
      await reload();
      setInfo("Stored key deleted from the OS credential store.");
    });

  const doExport = () =>
    run(async () => {
      setImportText(null);
      setExported(await api.exportProviders());
    });

  const copyExport = () =>
    run(async () => {
      await navigator.clipboard.writeText(exported ?? "");
      setInfo("Export copied to the clipboard.");
    });

  const doImport = () =>
    run(async () => {
      setProviders(await api.importProviders(importText ?? ""));
      setImportText(null);
      setInfo("Imported. Enter the API keys again on this machine, then test each provider.");
    });

  const needsKey = spec?.auth === "api_key";

  return (
    <div className="modal-backdrop">
      <div className="modal" role="dialog" aria-modal="true" aria-label="Model settings">
        <header className="modal-head">
          <h2>Model providers (optional)</h2>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </header>
        <p className="muted">
          Optional: only EAK-native agent integrations use these providers. The schematic, PCB and terminal work without them, and agents
          you run in the terminal (pi, opencode, claude…) use their own sign-in; EAK never reads their credentials. API keys are stored only
          in your operating system credential store, never in project files or logs, and are not shown again.
        </p>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {info && <p className="muted">{info}</p>}
        <section aria-label="Providers">
          {providers.length === 0 && <p className="muted">No model providers yet.</p>}
          <ul className="providers">
            {providers.map((p) => (
              <li key={p.id}>
                <span>
                  <strong>{p.label}</strong> {p.is_default && <span className="badge">default</span>}{" "}
                  <span className="muted">
                    {p.provider_name} · {p.model ?? "no model chosen"}
                  </span>
                  <br />
                  <span className={p.status.state === "connected" ? "ok" : p.status.state === "failed" ? "error" : "muted"} aria-label={`Status of ${p.label}`}>
                    {statusLabel(p)}
                  </span>
                  <span className="muted">
                    {" "}
                    · {p.key_error ? `key store error: ${p.key_error}` : p.has_key ? "key stored" : p.needs_key ? "no key" : "no key needed"} · {p.endpoint}
                  </span>
                </span>
                <span className="row">
                  <button type="button" disabled={busy} onClick={() => edit(p)} aria-label={`Edit ${p.label}`}>
                    Edit
                  </button>
                  <button type="button" disabled={busy} onClick={() => testRow(p)} aria-label={`Test ${p.label}`}>
                    Test
                  </button>
                  {!p.is_default && (
                    <button type="button" disabled={busy} onClick={() => makeDefault(p)} aria-label={`Set ${p.label} as default`}>
                      Set default
                    </button>
                  )}
                  <button type="button" disabled={busy} onClick={() => remove(p)} aria-label={`Remove ${p.label}`}>
                    Remove
                  </button>
                </span>
              </li>
            ))}
          </ul>
          <div className="row">
            {!draft && (
              <button type="button" disabled={busy || catalog.length === 0} onClick={startNew}>
                Add provider
              </button>
            )}
            <button type="button" disabled={busy || providers.length === 0} onClick={doExport}>
              Export (no keys)
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                setExported(null);
                setImportText("");
              }}
            >
              Import
            </button>
          </div>
          {exported !== null && (
            <div className="row">
              <textarea aria-label="Exported provider configuration" readOnly rows={6} value={exported} />
              <button type="button" disabled={busy} onClick={copyExport}>
                Copy
              </button>
            </div>
          )}
          {importText !== null && (
            <div className="row">
              <textarea aria-label="Provider configuration to import" rows={6} value={importText} onChange={(e) => setImportText(e.target.value)} />
              <button type="button" disabled={busy || !importText.trim()} onClick={doImport}>
                Import providers
              </button>
            </div>
          )}
        </section>
        {draft && spec && (
          <form
            aria-label="Provider details"
            onSubmit={(e) => {
              e.preventDefault();
              saveAndTest();
            }}
          >
            <h3>{draft.id ? "Edit provider" : "Add provider"}</h3>
            <label>
              Provider type
              <select value={draft.provider} onChange={(e) => chooseProvider(e.target.value)}>
                {GROUPS.map(([g, title]) => (
                  <optgroup key={g} label={title}>
                    {catalog
                      .filter((s) => s.group === g && s.implemented)
                      .map((s) => (
                        <option key={s.id} value={s.id}>
                          {s.name}
                        </option>
                      ))}
                  </optgroup>
                ))}
                <optgroup label="Not implemented yet">
                  {catalog
                    .filter((s) => !s.implemented)
                    .map((s) => (
                      <option key={s.id} value={s.id} disabled title={s.note ?? undefined}>
                        {s.name} (not implemented)
                      </option>
                    ))}
                </optgroup>
              </select>
            </label>
            {spec.note && <p className="muted">{spec.note}</p>}
            <label>
              Provider name
              <input value={draft.label} onChange={(e) => setDraft({ ...draft, label: e.target.value })} />
            </label>
            {spec.base_url_editable ? (
              <label>
                Base URL
                <input
                  value={draft.base_url}
                  placeholder={spec.base_url ?? "https://host/v1 (required)"}
                  onChange={(e) => setDraft({ ...draft, base_url: e.target.value })}
                />
              </label>
            ) : (
              <p className="muted" aria-label="Endpoint">
                Endpoint: {spec.base_url} (fixed)
              </p>
            )}
            <label>
              {needsKey ? "API key" : "API key (optional)"}
              <input
                type="password"
                autoComplete="off"
                value={draft.key}
                placeholder={draft.hasKey ? "stored in OS credential storage (leave blank to keep)" : needsKey ? "paste key" : "only if your server requires one"}
                onChange={(e) => setDraft({ ...draft, key: e.target.value })}
              />
            </label>
            {draft.hasKey && draft.id && (
              <button type="button" disabled={busy} onClick={clearKey}>
                Delete stored key
              </button>
            )}
            {spec.org_project && (
              <div className="row">
                <label>
                  Organization ID (optional)
                  <input value={draft.org_id} onChange={(e) => setDraft({ ...draft, org_id: e.target.value })} />
                </label>
                <label>
                  Project ID (optional)
                  <input value={draft.project_id} onChange={(e) => setDraft({ ...draft, project_id: e.target.value })} />
                </label>
              </div>
            )}
            <label>
              Model identifier
              <input value={draft.model} onChange={(e) => setDraft({ ...draft, model: e.target.value })} />
            </label>
            {models.length > 0 && (
              <table className="model-table" aria-label="Models">
                <thead>
                  <tr>
                    <th>Model</th>
                    <th>Identifier</th>
                    <th>Tools</th>
                    <th>Vision</th>
                    <th>Context</th>
                    <th>Max output</th>
                    <th>Availability</th>
                  </tr>
                </thead>
                <tbody>
                  {models.map((m) => (
                    <tr key={m.id} className={m.id === draft.model ? "selected" : undefined}>
                      <td>
                        <label className="inline">
                          <input
                            type="radio"
                            name="model"
                            aria-label={`Use ${m.id}`}
                            checked={m.id === draft.model}
                            onChange={() => setDraft({ ...draft, model: m.id })}
                          />{" "}
                          {m.name ?? m.id}
                        </label>
                      </td>
                      <td>
                        <code>{m.id}</code>
                      </td>
                      <td>{yesNo(m.tools)}</td>
                      <td>{yesNo(m.vision)}</td>
                      <td>{fmtTokens(m.context_window)}</td>
                      <td>{fmtTokens(m.max_output_tokens)}</td>
                      <td>{m.source === "discovered" ? "listed by provider" : "built-in suggestion (not verified)"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <div className="row">
              <button type="submit" className="primary" disabled={busy}>
                Save &amp; test
              </button>
              <button type="button" disabled={busy} onClick={doSave}>
                Save
              </button>
              {spec.discovery && (
                <button type="button" disabled={busy} onClick={discover}>
                  Discover models
                </button>
              )}
              <button type="button" disabled={busy} onClick={() => setDraft(null)}>
                Done
              </button>
            </div>
            {report && (
              <div role="status" className={report.ok ? "ok" : "error"}>
                <strong>
                  {report.ok
                    ? "Connected"
                    : report.state === "authenticated"
                      ? "Authenticated"
                      : report.error_class
                        ? ERROR_TITLES[report.error_class]
                        : "Connection failed"}
                </strong>{" "}
                ({report.latency_ms} ms): {report.message}
                <ul className="stages">
                  {report.stages.map((s) => (
                    <li key={s.stage}>
                      {s.ok === true ? "✓" : s.ok === false ? "✗" : "–"} {s.stage}: {s.message}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </form>
        )}
        <PerformancePanel />
      </div>
    </div>
  );
}
