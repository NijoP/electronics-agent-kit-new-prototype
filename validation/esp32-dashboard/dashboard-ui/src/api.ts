// Types and fetchers for the JSON API served by firmware/dashboard-host/src/server.rs.
// Field names mirror `state_json` / `history_json` there. A missing value is `null`,
// never a substituted number.

export interface Stats {
  count: number;
  min: number;
  max: number;
  mean: number;
}

export interface Latest {
  age_ms: number;
  seq: number;
  device_uptime_ms: number;
  temperature_c: number | null;
  humidity_pct: number | null;
  vbus_v: number | null;
  button_pressed: boolean;
  sensor_faults: number;
}

export interface State {
  source: string;
  simulated: boolean;
  latest: Latest | null;
  stats: {
    temperature_c: Stats | null;
    humidity_pct: Stats | null;
    vbus_v: Stats | null;
  };
  alarms: string[];
  link: {
    frames_ok: number;
    frames_rejected: number;
    sequence_gaps: number;
    last_error: string | null;
  };
  thresholds: {
    temp_high_c: number;
    temp_low_c: number;
    rh_high_pct: number;
    vbus_low_v: number;
  };
}

/** One history row: [host_ms, seq, temperature_c, humidity_pct, vbus_v]. */
export type HistoryRow = [number, number, number | null, number | null, number | null];

export interface History {
  columns: string[];
  rows: HistoryRow[];
}

async function get(path: string, signal: AbortSignal): Promise<Response> {
  const r = await fetch(path, { signal, cache: "no-store" });
  if (!r.ok) throw new Error(`${path}: HTTP ${r.status}`);
  return r;
}

export const fetchState = async (signal: AbortSignal): Promise<State> =>
  (await (await get("/api/state", signal)).json()) as State;
export const fetchHistory = async (signal: AbortSignal): Promise<History> =>
  (await (await get("/api/history", signal)).json()) as History;
export const fetchDisplay = async (signal: AbortSignal): Promise<string> =>
  (await get("/display.txt", signal)).text();

export function value(v: number | null | undefined, digits: number, unit: string): string {
  return v === null || v === undefined ? "no data" : `${v.toFixed(digits)} ${unit}`;
}

export function statsLine(s: Stats | null, digits: number, unit: string): string {
  if (!s) return "no samples";
  return `min ${s.min.toFixed(digits)} · mean ${s.mean.toFixed(digits)} · max ${s.max.toFixed(digits)} ${unit} (${s.count} samples)`;
}
