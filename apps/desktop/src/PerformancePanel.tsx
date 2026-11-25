// Performance mode control (directive §4.2): one preference, resolved in Rust into job limits,
// 3D quality and terminal scrollback. The profile also carries a cache budget, but nothing in the
// app consumes it yet, so it is not offered as a setting (slice E1: no control without an effect). These are preferences, not promises of a
// frame rate, and no mode changes an engineering value or skips a check.
import { useEffect, useState } from "react";
import { api } from "./api";
import { loadProfile, publish, type CustomLimits, type PerformanceMode, type PerformanceSettings, type ResourceProfile } from "./resourceProfile";

const MODES: { mode: PerformanceMode; label: string }[] = [
  { mode: "auto", label: "Auto (from this computer)" },
  { mode: "low_resource", label: "Low resource" },
  { mode: "balanced", label: "Balanced" },
  { mode: "high_performance", label: "High performance" },
  { mode: "custom", label: "Custom" },
];

export function PerformancePanel() {
  const [settings, setSettings] = useState<PerformanceSettings | null>(null);
  const [profile, setProfile] = useState<ResourceProfile | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Why the settings could not be read (S21: the section used to vanish silently); Retry re-reads.
  const [loadError, setLoadError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setLoadError(null);
    Promise.resolve()
      .then(() => api.performanceSettings())
      .then((s) => {
        if (!live) return;
        if (s && typeof s.mode === "string") setSettings(s);
        else setLoadError("the app returned no performance settings");
      })
      .catch((e) => live && setLoadError(String(e).replace(/^Error: /, "")));
    void loadProfile().then((p) => {
      if (live) setProfile(p);
    });
    return () => {
      live = false;
    };
  }, [attempt]);

  const save = (next: PerformanceSettings) => {
    setSettings(next);
    api
      .setPerformanceSettings(next)
      .then((p) => {
        publish(p);
        setProfile(p);
        setError(null);
      })
      .catch((e) => setError(`Could not save the performance mode: ${String(e)}`));
  };

  if (!settings)
    return loadError ? (
      <section aria-label="Performance" className="performance-panel">
        <h3>Performance</h3>
        <p role="alert" className="error">
          Could not read the performance settings: {loadError}. The performance mode cannot be shown or changed until they are read.
        </p>
        <button type="button" onClick={() => setAttempt((n) => n + 1)}>
          Retry
        </button>
      </section>
    ) : (
      <p className="muted">Reading the performance settings…</p>
    );
  const custom = settings.custom;
  const num = (key: Exclude<keyof typeof custom, "antialias">, label: string, step = 1) => (
    <label>
      {label}{" "}
      <input
        type="number"
        step={step}
        value={custom[key]}
        onChange={(e) => {
          const v = Number(e.target.value);
          if (e.target.value === "" || !Number.isFinite(v)) return;
          const next: CustomLimits = { ...custom };
          next[key] = v;
          save({ ...settings, custom: next });
        }}
      />
    </label>
  );

  return (
    <section aria-label="Performance" className="performance-panel">
      <h3>Performance</h3>
      <label>
        Performance mode{" "}
        <select value={settings.mode} onChange={(e) => save({ ...settings, mode: e.target.value as PerformanceMode })}>
          {MODES.map((m) => (
            <option key={m.mode} value={m.mode}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
      {settings.mode === "custom" && (
        <div className="performance-custom">
          {num("analysis_jobs", "Parallel checks")}
          {num("background_jobs", "Parallel background jobs")}
          {num("pixel_ratio_cap", "3D pixel-ratio cap", 0.25)}
          {num("scrollback_lines", "Terminal scrollback lines", 500)}
          <label>
            <input
              type="checkbox"
              checked={custom.antialias}
              onChange={(e) => save({ ...settings, custom: { ...custom, antialias: e.target.checked } })}
            />{" "}
            3D antialiasing
          </label>
        </div>
      )}
      {profile && (
        <p className="muted">
          {profile.logical_cpus} logical CPU{profile.logical_cpus === 1 ? "" : "s"} · {profile.analysis_jobs} parallel check
          {profile.analysis_jobs === 1 ? "" : "s"} · {profile.background_jobs} background job{profile.background_jobs === 1 ? "" : "s"} · 3D
          pixel ratio ≤ {profile.pixel_ratio_cap}, antialiasing {profile.antialias ? "on" : "off"} · {profile.scrollback_lines} terminal
          lines. Pixel ratio applies at once; antialiasing and scrollback apply to newly opened 3D views and terminals. Queued checks
          still run; nothing is skipped. {profile.limitations.join("; ")}.
        </p>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </section>
  );
}
