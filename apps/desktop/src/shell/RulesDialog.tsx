// Design rules / net class dialog. Every field maps to one typed engine command
// (SetDesignRules, UpsertNetClass, SetNetCurrent, SetNetVoltage); the engine validates and may
// reject, and DRC/ERC then use the stored values. Lengths are entered in mm.
import { useState } from "react";
import { netNames } from "./panels";
import { mmToNm } from "../editing";
import type { Command, NetClass, Snapshot } from "../types";

interface Props {
  snapshot: Snapshot;
  busy: boolean;
  apply: (c: Command | Command[]) => Promise<void>;
  onClose: () => void;
}

const toMm = (nm: number | null | undefined) => (nm === null || nm === undefined ? "" : String(+(nm / 1e6).toFixed(4)));
/** Parse a positive number from a form field; null = empty, NaN-safe `undefined` = invalid. */
function num(s: string): number | null | undefined {
  if (s.trim() === "") return null;
  const v = Number(s);
  return Number.isFinite(v) && v > 0 ? v : undefined;
}

export function RulesDialog({ snapshot, busy, apply, onClose }: Props) {
  const c = snapshot.project.constraints;
  const [rules, setRules] = useState({
    min_track_width: toMm(c?.min_track_width),
    min_clearance: toMm(c?.min_clearance),
    min_drill: toMm(c?.min_drill),
    min_annular_ring: toMm(c?.min_annular_ring),
    board_edge_clearance: toMm(c?.board_edge_clearance),
    default_track_width: toMm(c?.default_class.track_width),
    default_clearance: toMm(c?.default_class.clearance),
    temp_rise_c: c ? String(c.temp_rise_c) : "",
    max_component_height: toMm(c?.max_component_height),
  });
  const [error, setError] = useState<string | null>(null);
  const classes = c?.net_classes ?? [];
  const blank = { name: "", track_width: "", clearance: "", via_drill: "", via_diameter: "", nets: "" };
  const [cls, setCls] = useState(blank);
  const nets = netNames(snapshot);
  const [netSel, setNetSel] = useState(nets[0] ?? "");
  const [amps, setAmps] = useState("");
  const [volts, setVolts] = useState("");

  const loadClass = (n: NetClass) =>
    setCls({ name: n.name, track_width: toMm(n.track_width), clearance: toMm(n.clearance), via_drill: toMm(n.via_drill), via_diameter: toMm(n.via_diameter), nets: n.nets.join(", ") });

  const mm = (s: string): number | null | undefined => {
    const v = num(s);
    return v === undefined ? undefined : v === null ? null : mmToNm(v);
  };
  const run = (cmds: Command[]) => {
    setError(null);
    void apply(cmds);
  };

  const applyRules = () => {
    const f = Object.fromEntries(Object.entries(rules).map(([k, v]) => [k, k === "temp_rise_c" ? num(v) : mm(v)]));
    const bad = Object.entries(f).find(([, v]) => v === undefined);
    if (bad) return setError(`"${bad[0].replace(/_/g, " ")}" must be a positive number (mm, or °C for temperature rise).`);
    run([{ type: "SetDesignRules", ...(f as Record<string, number | null>) } as Command]);
  };
  const applyClass = () => {
    const name = cls.name.trim();
    const tw = mm(cls.track_width), cl = mm(cls.clearance), vd = mm(cls.via_drill), vdi = mm(cls.via_diameter);
    if (!name) return setError("A net class needs a name.");
    if (!tw || !cl || !vd || !vdi) return setError("Track width, clearance, via drill and via diameter must all be positive numbers (mm).");
    run([{ type: "UpsertNetClass", class: { name, track_width: tw, clearance: cl, via_drill: vd, via_diameter: vdi, nets: cls.nets.split(",").map((n) => n.trim()).filter(Boolean) } }]);
  };
  const applyNet = () => {
    if (!netSel) return setError("Pick a net first.");
    const a = num(amps), v = num(volts);
    if (a === undefined || v === undefined) return setError("Current and voltage must be positive numbers, or empty to clear the declaration.");
    run([
      { type: "SetNetCurrent", net: netSel, amps: a },
      { type: "SetNetVoltage", net: netSel, volts: v },
    ]);
  };
  const pickNet = (n: string) => {
    setNetSel(n);
    setAmps(c?.net_currents_a?.[n] !== undefined ? String(c.net_currents_a[n]) : "");
    setVolts(c?.net_voltages_v?.[n] !== undefined ? String(c.net_voltages_v[n]) : "");
  };

  const field = (k: keyof typeof rules, label: string) => (
    <label key={k}>
      {label}
      <input aria-label={label} inputMode="decimal" value={rules[k]} onChange={(e) => setRules({ ...rules, [k]: e.target.value })} />
    </label>
  );

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal rules" role="dialog" aria-label="Design rules" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>Design rules</h2>
          <button type="button" onClick={onClose}>Close</button>
        </div>
        <p className="muted hint">Each Apply sends one typed command to the engine, which validates it. DRC and ERC use these values; re-run them after changing rules.</p>
        {error && <p role="alert" className="error">{error}</p>}
        <section aria-label="Board rules">
          <h3>Board rules</h3>
          <div className="grid2">
            {field("min_track_width", "Min track width (mm)")}
            {field("min_clearance", "Min clearance (mm)")}
            {field("min_drill", "Min drill (mm)")}
            {field("min_annular_ring", "Min annular ring (mm)")}
            {field("board_edge_clearance", "Board edge clearance (mm)")}
            {field("default_track_width", "Default track width (mm)")}
            {field("default_clearance", "Default clearance (mm)")}
            {field("temp_rise_c", "Allowed temperature rise (°C)")}
            {field("max_component_height", "Max component height (mm, optional)")}
          </div>
          <button type="button" className="primary" disabled={busy} onClick={applyRules}>Apply board rules</button>
        </section>
        <section aria-label="Net classes">
          <h3>Net classes</h3>
          {classes.length > 0 && (
            <ul className="tree">
              {classes.map((n) => (
                <li key={n.name}><button type="button" onClick={() => loadClass(n)}>{n.name} <span className="muted">{n.nets.length} net(s)</span></button></li>
              ))}
            </ul>
          )}
          <div className="grid2">
            {(["name", "track_width", "clearance", "via_drill", "via_diameter", "nets"] as const).map((k) => {
              const label = { name: "Class name", track_width: "Class track width (mm)", clearance: "Class clearance (mm)", via_drill: "Class via drill (mm)", via_diameter: "Class via diameter (mm)", nets: "Nets (comma separated)" }[k];
              return (
                <label key={k}>
                  {label}
                  <input aria-label={label} value={cls[k]} onChange={(e) => setCls({ ...cls, [k]: e.target.value })} />
                </label>
              );
            })}
          </div>
          <button type="button" className="primary" disabled={busy} onClick={applyClass}>Save net class</button>
        </section>
        <section aria-label="Net declarations">
          <h3>Net current and voltage</h3>
          {nets.length === 0 ? (
            <p className="empty-state">No nets yet: connect pins or add net labels first.</p>
          ) : (
            <>
              <div className="grid2">
                <label>
                  Net
                  <select aria-label="Declared net" value={netSel} onChange={(e) => pickNet(e.target.value)}>
                    {nets.map((n) => <option key={n}>{n}</option>)}
                  </select>
                </label>
                <label>
                  Continuous current (A)
                  <input aria-label="Net current (A)" inputMode="decimal" value={amps} onChange={(e) => setAmps(e.target.value)} />
                </label>
                <label>
                  Peak voltage (V)
                  <input aria-label="Net voltage (V)" inputMode="decimal" value={volts} onChange={(e) => setVolts(e.target.value)} />
                </label>
              </div>
              <button type="button" className="primary" disabled={busy} onClick={applyNet}>Apply net declaration</button>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
