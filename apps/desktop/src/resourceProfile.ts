// The app-wide resource profile, resolved in Rust (src-tauri/src/resources.rs). The UI only reads
// it: 3D back-buffer pixel-ratio cap and antialiasing, terminal view scrollback. Presentation
// only; no engineering value, geometry or check depends on it.
import { api } from "./api";

export type PerformanceMode = "auto" | "low_resource" | "balanced" | "high_performance" | "custom";

export interface CustomLimits {
  analysis_jobs: number;
  background_jobs: number;
  cache_budget_mb: number;
  pixel_ratio_cap: number;
  antialias: boolean;
  scrollback_lines: number;
}

export interface PerformanceSettings {
  mode: PerformanceMode;
  custom: CustomLimits;
}

export interface ResourceProfile {
  mode: PerformanceMode;
  tier: "low_resource" | "balanced" | "high_performance" | "custom";
  logical_cpus: number;
  analysis_jobs: number;
  background_jobs: number;
  cache_budget_mb: number;
  pixel_ratio_cap: number;
  antialias: boolean;
  scrollback_lines: number;
  limitations: string[];
}

/** Used until (or if) the backend answers: conservative, like the Balanced tier. */
export const FALLBACK_PROFILE: ResourceProfile = {
  mode: "auto",
  tier: "balanced",
  logical_cpus: 1,
  analysis_jobs: 1,
  background_jobs: 1,
  cache_budget_mb: 256,
  pixel_ratio_cap: 1.5,
  antialias: true,
  scrollback_lines: 5000,
  limitations: ["resource profile not loaded; using conservative defaults"],
};

let current: ResourceProfile = FALLBACK_PROFILE;
let loading: Promise<ResourceProfile> | null = null;
const listeners = new Set<(p: ResourceProfile) => void>();

/** The last known profile (synchronous; the fallback before the first load finishes). */
export function profileNow(): ResourceProfile {
  return current;
}

/** Load once per session; later calls share the same request. Never rejects. */
export function loadProfile(): Promise<ResourceProfile> {
  loading ??= Promise.resolve()
    .then(() => api.resourceProfile())
    .then((p) => {
      if (p && typeof p.pixel_ratio_cap === "number") publish(p);
      return current;
    })
    .catch(() => current);
  return loading;
}

/** Record a new profile (after the user changed the mode) and tell every subscriber. */
export function publish(p: ResourceProfile) {
  current = p;
  listeners.forEach((f) => f(p));
}

/** Subscribe to profile changes; returns the unsubscribe function. */
export function onProfile(f: (p: ResourceProfile) => void): () => void {
  listeners.add(f);
  return () => {
    listeners.delete(f);
  };
}
