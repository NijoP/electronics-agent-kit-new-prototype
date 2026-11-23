// Diagnostics (Phase 1 slice E4): tasks, the performance profile, model connectivity and
// recovery, as rows with a state. Pure. The rule of the slice: a value the app does not know is
// shown as Unknown, never as OK. A provider that was never tested, a profile that did not load,
// a recovery outcome the journal could not settle and a provider list that could not be read
// are all Unknown; OK is only shown for a fact the app has (a finished task, a connection test
// that passed, a clean open).
import { FALLBACK_PROFILE, type ResourceProfile } from "../resourceProfile";
import type { ProviderView, Snapshot } from "../types";
import type { Task } from "./tasks";

export type DiagState = "ok" | "warning" | "error" | "unknown" | "info";
export type DiagSection = "tasks" | "performance" | "model" | "recovery";
export const DIAG_SECTIONS: readonly { section: DiagSection; label: string }[] = [
  { section: "tasks", label: "Tasks" },
  { section: "performance", label: "Performance profile" },
  { section: "model", label: "Model connectivity" },
  { section: "recovery", label: "Recovery" },
];
export const STATE_LABEL: Record<DiagState, string> = { ok: "OK", warning: "Warning", error: "Error", unknown: "Unknown", info: "Info" };

export interface DiagRow {
  section: DiagSection;
  id: string;
  label: string;
  state: DiagState;
  detail: string;
  /** Model rows: the provider, so the row can offer a connection test. */
  providerId?: string;
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
const clock = (ms: number) => new Date(ms).toLocaleTimeString();

export function taskRows(tasks: Task[]): DiagRow[] {
  const row = (id: string, label: string, state: DiagState, detail: string): DiagRow => ({ section: "tasks", id, label, state, detail });
  if (tasks.length === 0) return [row("tasks-none", "Tasks this session", "info", "None yet: checks, Update PCB and exports you run are listed with their outcome.")];
  const running = tasks.filter((t) => t.status === "running");
  const failed = tasks.filter((t) => t.status === "failed");
  const done = tasks.filter((t) => t.status === "done");
  const out = [row("tasks-done", "Finished", done.length ? "ok" : "info", plural(done.length, "task"))];
  if (running.length) out.push(row("tasks-running", "Running", "info", running.map((t) => t.label).join(", ")));
  const last = failed[failed.length - 1];
  out.push(
    last
      ? row("tasks-failed", "Failed", "error", `${plural(failed.length, "task")}; last: ${last.label}${last.detail ? `: ${last.detail}` : ""}`)
      : row("tasks-failed", "Failed", done.length ? "ok" : "info", "none"),
  );
  return out;
}

/** `profile` is null while it is being read; the fallback profile means it could not be read. */
export function performanceRows(profile: ResourceProfile | null): DiagRow[] {
  const row = (id: string, label: string, state: DiagState, detail: string): DiagRow => ({ section: "performance", id, label, state, detail });
  if (!profile) return [row("perf-loading", "Resource profile", "unknown", "Reading the resource profile…")];
  if (profile === FALLBACK_PROFILE)
    return [row("perf-fallback", "Resource profile", "unknown", "The app could not read the resource profile; conservative defaults are in use until it does.")];
  return [
    row("perf-mode", "Mode", "info", `${profile.mode} (tier ${profile.tier})`),
    row("perf-cpus", "Logical CPUs", "info", String(profile.logical_cpus)),
    row("perf-jobs", "Job limits", "info", `${profile.analysis_jobs} analysis · ${profile.background_jobs} background`),
    row("perf-3d", "3D view", "info", `pixel ratio cap ${profile.pixel_ratio_cap}, antialiasing ${profile.antialias ? "on" : "off"}`),
    row("perf-term", "Terminal scrollback", "info", `${profile.scrollback_lines} lines`),
    ...profile.limitations.map((l, i) => row(`perf-limit-${i}`, "Limitation", "warning", l)),
  ];
}

/** `providers` is null while it is being read; `error` set when it could not be read. */
export function modelRows(providers: ProviderView[] | null, error: string | null): DiagRow[] {
  const row = (id: string, label: string, state: DiagState, detail: string, providerId?: string): DiagRow => ({ section: "model", id, label, state, detail, ...(providerId ? { providerId } : {}) });
  if (error) return [row("model-error", "Model providers", "unknown", `Could not read the model providers: ${error}`)];
  if (!providers) return [row("model-loading", "Model providers", "unknown", "Reading the model providers…")];
  if (providers.length === 0) return [row("model-none", "Model providers", "info", "No model provider is configured; AI features are off. Add one in Settings › Model settings.")];
  return providers.map((p) => {
    const name = `${p.label}${p.is_default ? " (default)" : ""}`;
    const id = `model-${p.id}`;
    if (p.key_error) return row(id, name, "error", `API key unreadable: ${p.key_error}`, p.id);
    if (p.needs_key && !p.has_key) return row(id, name, "warning", "No API key stored; connectivity unknown until a key is set and tested.", p.id);
    const s = p.status;
    const when = s.checked_at_ms ? ` (last test ${clock(s.checked_at_ms)})` : "";
    switch (s.state) {
      case "connected":
        return row(id, name, "ok", `Connected${s.model ? ` with ${s.model}` : ""}${when}`, p.id);
      case "authenticated":
        return row(id, name, "warning", `Authenticated, no model chosen${when}`, p.id);
      case "failed":
        return row(id, name, "error", `Failed${when}${s.message ? `: ${s.message}` : ""}`, p.id);
      default:
        return row(id, name, "unknown", "Not tested: connectivity unknown.", p.id);
    }
  });
}

export function recoveryRows(s: Snapshot): DiagRow[] {
  const row = (id: string, label: string, state: DiagState, detail: string): DiagRow => ({ section: "recovery", id, label, state, detail });
  const out: DiagRow[] = [];
  if (s.blocked) out.push(row("rec-blocked", "Writes", "error", `Blocked: ${s.blocked}`));
  for (const [op, state] of s.recovery.reconciled) {
    if (state === "committed") out.push(row(`rec-${op}`, `Interrupted operation ${op}`, "info", "Committed: the change was already on disk."));
    else if (state === "aborted") out.push(row(`rec-${op}`, `Interrupted operation ${op}`, "info", "Rolled back: the project is at the revision before it."));
    else out.push(row(`rec-${op}`, `Interrupted operation ${op}`, "unknown", `Outcome unknown (${state}): inspect the project before editing.`));
  }
  if (s.recovery.torn_temp_file_removed) out.push(row("rec-torn", "Partial save", "warning", "A partly written project file was found and removed when the project was opened."));
  if (out.length === 0) out.push(row("rec-clean", "Last open", "ok", "No interrupted operation or partial save was found when this project was opened."));
  return out;
}
