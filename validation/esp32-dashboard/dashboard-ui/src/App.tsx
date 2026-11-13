import { useEffect, useState } from "react";
import { Chart } from "./Chart";
import { Oled } from "./Oled";
import {
  fetchDisplay,
  fetchHistory,
  fetchState,
  statsLine,
  value,
  type History,
  type State,
} from "./api";

const POLL_MS = 1000;

interface Snapshot {
  state: State;
  history: History;
  display: string;
}

function usePoll(): { data: Snapshot | null; error: string | null } {
  const [data, setData] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    let ctrl = new AbortController();
    const tick = async () => {
      ctrl.abort();
      ctrl = new AbortController();
      try {
        const [state, history, display] = await Promise.all([
          fetchState(ctrl.signal),
          fetchHistory(ctrl.signal),
          fetchDisplay(ctrl.signal),
        ]);
        if (live) {
          setData({ state, history, display });
          setError(null);
        }
      } catch (e) {
        if (live && !(e instanceof DOMException && e.name === "AbortError")) {
          setError(String(e));
        }
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), POLL_MS);
    return () => {
      live = false;
      ctrl.abort();
      window.clearInterval(id);
    };
  }, []);
  return { data, error };
}

function Card(props: { id: string; title: string; value: string; detail: string }) {
  return (
    <section className="card" aria-labelledby={`${props.id}-h`}>
      <h2 id={`${props.id}-h`}>{props.title}</h2>
      <div className="value" id={props.id}>
        {props.value}
      </div>
      <div className="detail">{props.detail}</div>
    </section>
  );
}

export function App() {
  const { data, error } = usePoll();
  const s = data?.state;
  const l = s?.latest ?? null;
  const rows = data?.history.rows ?? [];

  return (
    <>
      <header>
        <h1>ESP32-C3 Environment Dashboard</h1>
        <div className="source">
          {error ? `dashboard server unreachable: ${error}` : (s?.source ?? "connecting…")}
        </div>
      </header>
      {s?.simulated && (
        <div className="banner" role="status">
          SIMULATED DATA — not a physical hardware measurement
        </div>
      )}
      <main>
        <Card
          id="temperature"
          title="Temperature"
          value={value(l?.temperature_c, 2, "°C")}
          detail={statsLine(s?.stats.temperature_c ?? null, 2, "°C")}
        />
        <Card
          id="humidity"
          title="Relative humidity"
          value={value(l?.humidity_pct, 1, "%")}
          detail={statsLine(s?.stats.humidity_pct ?? null, 1, "%")}
        />
        <Card
          id="vbus"
          title="USB VBUS"
          value={value(l?.vbus_v, 2, "V")}
          detail={statsLine(s?.stats.vbus_v ?? null, 2, "V")}
        />
        <Card
          id="link"
          title="Link"
          value={l ? `#${l.seq}` : "–"}
          detail={
            s
              ? `${s.link.frames_ok} ok · ${s.link.frames_rejected} rejected · ${s.link.sequence_gaps} gaps · sensor faults ${l?.sensor_faults ?? "–"} · button ${l?.button_pressed ? "PRESSED" : "released"}`
              : ""
          }
        />
        <section className="card wide" aria-labelledby="alarms-h">
          <h2 id="alarms-h">Alarms</h2>
          {s && s.alarms.length === 0 && <span className="ok">No active alarms</span>}
          {s?.alarms.map((a) => (
            <span key={a} className="alarm">
              {a}
            </span>
          ))}
          {s?.link.last_error && (
            <div className="detail">last link error: {s.link.last_error}</div>
          )}
        </section>
        <section className="card wide" aria-labelledby="history-h">
          <h2 id="history-h">History</h2>
          <Chart
            title="Temperature"
            unit="°C"
            digits={1}
            className="temp"
            values={rows.map((r) => r[2])}
            limit={s && { value: s.thresholds.temp_high_c, label: "high-temperature alarm" }}
          />
          <Chart
            title="Relative humidity"
            unit="%"
            digits={0}
            className="rh"
            values={rows.map((r) => r[3])}
            limit={s && { value: s.thresholds.rh_high_pct, label: "high-humidity alarm" }}
          />
        </section>
        <section className="card" aria-labelledby="oled-h">
          <h2 id="oled-h">On-board OLED (firmware renderer)</h2>
          <Oled text={data?.display ?? null} />
        </section>
      </main>
    </>
  );
}
