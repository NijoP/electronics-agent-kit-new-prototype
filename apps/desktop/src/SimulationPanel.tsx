// Optional ngspice simulation (.op / .tran). Reads the current revision through IPC; it never
// edits the design. Results come from ngspice and are shown as such, never as verification.
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { SimResult, SimSource, SimulationStatus } from "./types";

/** Parse `10k`, `4.7u`, `1e-3`, `5m` (SPICE-style: `meg` = 1e6, `m` = 1e-3). */
export function parseEng(text: string): number | null {
  const m = /^\s*([+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?)\s*(meg|[fpnumkgt])?\s*(?:s|v|a|hz)?\s*$/i.exec(text);
  if (!m) return null;
  const exp: Record<string, number> = { f: -15, p: -12, n: -9, u: -6, m: -3, k: 3, meg: 6, g: 9, t: 12 };
  const v = Number(m[2] ? `${m[1]}e${exp[m[2].toLowerCase()]}` : m[1]);
  return Number.isFinite(v) ? v : null;
}

export function fmtEng(v: number, unit: string): string {
  if (v === 0) return `0 ${unit}`;
  const a = Math.abs(v);
  const steps: [number, string][] = [[1e9, "G"], [1e6, "M"], [1e3, "k"], [1, ""], [1e-3, "m"], [1e-6, "µ"], [1e-9, "n"], [1e-12, "p"]];
  const [f, p] = steps.find(([f]) => a >= f) ?? steps[steps.length - 1];
  return `${Number((v / f).toPrecision(4))} ${p}${unit}`;
}

// Trace colours are design tokens (--c-plot-1..6), so both themes keep contrast.
const COLORS = [1, 2, 3, 4, 5, 6].map((n) => `var(--c-plot-${n})`);

export function TransientPlot({ result }: { result: SimResult }) {
  const t = result.tran;
  if (!t || t.time.length < 2) return null;
  const [W, H, L, B] = [520, 200, 48, 22];
  const t0 = t.time[0], t1 = t.time[t.time.length - 1];
  const all = t.traces.flatMap((s) => s.volts);
  let lo = Math.min(...all), hi = Math.max(...all);
  if (hi - lo < 1e-9) { lo -= 0.5; hi += 0.5; }
  const x = (v: number) => L + ((v - t0) / (t1 - t0)) * (W - L - 8);
  const y = (v: number) => H - B - ((v - lo) / (hi - lo)) * (H - B - 8);
  return (
    <figure className="sim-plot">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Transient plot" width="100%">
        <rect x={L} y={8} width={W - L - 8} height={H - B - 8} fill="none" stroke="currentColor" opacity={0.3} />
        <text x={L - 4} y={14} fontSize="10" textAnchor="end" fill="currentColor">{fmtEng(hi, "V")}</text>
        <text x={L - 4} y={H - B} fontSize="10" textAnchor="end" fill="currentColor">{fmtEng(lo, "V")}</text>
        <text x={L} y={H - 6} fontSize="10" fill="currentColor">{fmtEng(t0, "s")}</text>
        <text x={W - 8} y={H - 6} fontSize="10" textAnchor="end" fill="currentColor">{fmtEng(t1, "s")}</text>
        {t.traces.map((s, i) => (
          <polyline key={s.net} fill="none" style={{ stroke: COLORS[i % COLORS.length] }} strokeWidth="1.5"
            points={s.volts.map((v, k) => `${x(t.time[k]).toFixed(1)},${y(v).toFixed(1)}`).join(" ")} />
        ))}
      </svg>
      <figcaption className="muted">
        {t.traces.map((s, i) => (<span key={s.net} style={{ color: COLORS[i % COLORS.length], marginRight: 12 }}>■ {s.net}</span>))}
        {t.decimated ? `${t.time.length} of ${t.points_total} points shown` : `${t.points_total} points`}
      </figcaption>
    </figure>
  );
}

export function SimulationPanel({ revision }: { revision: number }) {
  const [status, setStatus] = useState<SimulationStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [kind, setKind] = useState<"op" | "tran">("op");
  const [stop, setStop] = useState("5m");
  const [step, setStep] = useState("10u");
  const [sources, setSources] = useState<SimSource[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<SimResult | null>(null);

  const refresh = useCallback(() => {
    api.simulationStatus().then((s) => { setStatus(s); setStatusError(null); }).catch((e) => setStatusError(String(e)));
  }, []);
  useEffect(refresh, [refresh, revision]);

  const nets = status?.nets ?? [];
  const addSource = () =>
    setSources((s) => [...s, { name: `V${s.length + 1}`, kind: "voltage", positive_net: nets.find((n) => !/^gnd$/i.test(n)) ?? nets[0] ?? "", negative_net: "GND", spec: "5" }]);
  const patch = (i: number, p: Partial<SimSource>) => setSources((s) => s.map((x, k) => (k === i ? { ...x, ...p } : x)));

  const run = async () => {
    setError(null);
    const stopV = parseEng(stop), stepV = parseEng(step);
    if (kind === "tran" && (stopV === null || stepV === null || stopV <= 0 || stepV <= 0 || stepV > stopV)) {
      setError("Transient needs numeric stop and step with 0 < step ≤ stop (e.g. 5m and 10u).");
      return;
    }
    setBusy(true);
    try {
      const analysis = kind === "op" ? ({ kind: "op" } as const) : ({ kind: "tran", stop: stopV!, step: stepV! } as const);
      setResult(await api.runSimulation({ analysis, sources }));
    } catch (e) {
      setResult(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (statusError) return <p className="error" role="alert">{statusError}</p>;
  if (!status) return <p className="empty-state">Loading simulation status…</p>;
  const missing = !status.ngspice.available;
  return (
    <section className="sim" aria-label="Simulation">
      {missing ? (
        <p className="error" role="alert">ngspice is not installed. {status.ngspice.message}</p>
      ) : (
        <p className="muted">ngspice {status.ngspice.version} · {status.elements.length} element(s) from revision {status.revision}</p>
      )}
      <div className="sim-controls">
        <label>Analysis{" "}
          <select aria-label="Analysis" value={kind} onChange={(e) => setKind(e.target.value as "op" | "tran")}>
            <option value="op">.op (operating point)</option>
            <option value="tran">.tran (transient)</option>
          </select>
        </label>
        {kind === "tran" && (
          <>
            <label>Stop <input aria-label="Stop time" value={stop} onChange={(e) => setStop(e.target.value)} size={6} /></label>
            <label>Step <input aria-label="Step time" value={step} onChange={(e) => setStep(e.target.value)} size={6} /></label>
          </>
        )}
        <button type="button" className="primary" onClick={run} disabled={busy || missing} title={busy ? "A simulation is running" : missing ? "ngspice is not installed, so no simulation can run" : "Run the analysis with ngspice"}>{busy ? "Running…" : "Run simulation"}</button>
      </div>
      <h3>Sources <button type="button" onClick={addSource} disabled={nets.length === 0} title={nets.length === 0 ? "The schematic has no net to put a source on yet: wire a part first" : "Add a voltage or current source on a net"}>+ Add</button></h3>
      {sources.length === 0 && <p className="muted">Add a voltage/current source on a net (e.g. 5 V on VIN), or use a V/I part in the schematic.</p>}
      {sources.map((s, i) => (
        <div key={i} className="sim-source">
          <select aria-label={`Source ${i + 1} type`} value={s.kind} onChange={(e) => patch(i, { kind: e.target.value as SimSource["kind"], name: `${e.target.value === "voltage" ? "V" : "I"}${i + 1}` })}>
            <option value="voltage">V</option><option value="current">I</option>
          </select>
          <select aria-label={`Source ${i + 1} positive net`} value={s.positive_net} onChange={(e) => patch(i, { positive_net: e.target.value })}>
            {nets.map((n) => <option key={n}>{n}</option>)}
          </select>
          <span>→</span>
          <select aria-label={`Source ${i + 1} negative net`} value={s.negative_net} onChange={(e) => patch(i, { negative_net: e.target.value })}>
            <option value="GND">GND</option>
            {nets.filter((n) => n !== "GND").map((n) => <option key={n}>{n}</option>)}
          </select>
          <input aria-label={`Source ${i + 1} value`} value={s.spec} onChange={(e) => patch(i, { spec: e.target.value })} size={22} title="5, 3.3V, PULSE(0 5 0 1n 1n 1 2), SIN(0 1 1k)" />
          <button type="button" className="icon" aria-label={`Remove source ${i + 1}`} onClick={() => setSources((a) => a.filter((_, k) => k !== i))}>×</button>
        </div>
      ))}
      {status.skipped.length > 0 && (
        <p className="muted">Not simulated: {status.skipped.map((s) => `${s.refdes} (${s.reason})`).join("; ")}</p>
      )}
      {error && <p className="error" role="alert">{error}</p>}
      {result && (
        <div className="sim-result">
          <h3>{result.analysis.kind === "op" ? "Operating point" : "Transient — final values"} <span className="muted">ngspice {result.ngspice_version} · {result.elapsed_ms} ms</span></h3>
          <table className="bom" aria-label="Node voltages">
            <thead><tr><th>Net</th><th>Voltage</th></tr></thead>
            <tbody>{result.nodes.map((n) => <tr key={n.net}><td>{n.net}</td><td>{fmtEng(n.volts, "V")}</td></tr>)}</tbody>
          </table>
          {result.currents.length > 0 && (
            <table className="bom" aria-label="Branch currents">
              <thead><tr><th>Source</th><th>Current</th></tr></thead>
              <tbody>{result.currents.map((c) => <tr key={c.element}><td>{c.element}</td><td>{fmtEng(c.amps, "A")}</td></tr>)}</tbody>
            </table>
          )}
          <TransientPlot result={result} />
          {result.warnings.map((w) => <p key={w} className="muted">⚠ {w}</p>)}
          <p className="muted">Not verified: {result.limitations.join(" ")}</p>
          <details><summary>SPICE netlist</summary><pre>{result.netlist}</pre></details>
        </div>
      )}
    </section>
  );
}
