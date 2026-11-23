// Diagnostics tab of the bottom dock (Phase 1 slice E4): this session's tasks, the resolved
// performance profile, each model provider's last connection test (with a button to test it
// now) and how the project was recovered when it was opened. Unknown values say Unknown.
import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { loadProfile, onProfile, type ResourceProfile } from "../resourceProfile";
import type { ProviderView, Snapshot } from "../types";
import { DIAG_SECTIONS, modelRows, performanceRows, recoveryRows, STATE_LABEL, taskRows, type DiagRow } from "./diagnostics";
import type { Task } from "./tasks";

interface Props {
  snapshot: Snapshot;
  tasks: Task[];
}

export function DiagnosticsTab({ snapshot, tasks }: Props) {
  const [profile, setProfile] = useState<ResourceProfile | null>(null);
  const [providers, setProviders] = useState<ProviderView[] | null>(null);
  const [providerError, setProviderError] = useState<string | null>(null);
  const [testing, setTesting] = useState<string | null>(null);

  const readProviders = useCallback(() => {
    setProviderError(null);
    return Promise.resolve()
      .then(() => api.modelProviders())
      .then((p) => {
        if (!Array.isArray(p)) throw new Error("the app returned no provider list");
        setProviders(p);
      })
      .catch((e) => {
        setProviders(null);
        setProviderError(String(e).replace(/^Error: /, ""));
      });
  }, []);
  useEffect(() => {
    let live = true;
    void loadProfile().then((p) => {
      if (live) setProfile(p);
    });
    const off = onProfile((p) => {
      if (live) setProfile(p);
    });
    void readProviders();
    return () => {
      live = false;
      off();
    };
  }, [readProviders]);

  const test = (id: string) => {
    setTesting(id);
    Promise.resolve()
      .then(() => api.testConnection(id))
      .catch(() => undefined) // the outcome is in the provider's stored status, read below
      .then(() => readProviders())
      .finally(() => setTesting(null));
  };

  const rows: DiagRow[] = [...taskRows(tasks), ...performanceRows(profile), ...modelRows(providers, providerError), ...recoveryRows(snapshot)];
  return (
    <section className="diagnostics" aria-label="Diagnostics">
      <div className="diag-bar">
        <span className="muted">Unknown means the app has no answer yet; it is never shown as OK.</span>
        <button type="button" title="Read the model providers' stored connection status again" onClick={() => void readProviders()}>
          Refresh
        </button>
      </div>
      {DIAG_SECTIONS.map(({ section, label }) => (
        <table key={section} className="bom diag-table" aria-label={label} data-section={section}>
          <thead>
            <tr>
              <th colSpan={3}>{label}</th>
            </tr>
          </thead>
          <tbody>
            {rows
              .filter((r) => r.section === section)
              .map((r) => (
                <tr key={r.id} data-row={r.id}>
                  <td className="diag-label">{r.label}</td>
                  <td>
                    <span className={`diag-state ${r.state}`} data-state={r.state}>
                      {STATE_LABEL[r.state]}
                    </span>
                  </td>
                  <td>
                    {r.detail}
                    {r.providerId && (
                      <>
                        {" "}
                        <button type="button" disabled={testing !== null} title={testing ? "A connection test is running" : "Send a test request to this provider now"} onClick={() => test(r.providerId!)}>
                          {testing === r.providerId ? "Testing…" : "Test connection"}
                        </button>
                      </>
                    )}
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      ))}
    </section>
  );
}
