// Rule tables of the Design rules dialog (Phase 1 B5 and C5, ADR-0045): per-rule enable and
// severity override for ERC and DRC, the object x object clearance matrix and net-to-net
// clearances. Every Apply is one typed engine command (a Batch when several change): the engine
// validates and stores them in the IR constraints, and the checks read them from there.
import { useState } from "react";
import { mmToNm } from "../editing";
import type { ClearanceCell, Command, Constraints, ObjectClass, Report, RuleConfig, RuleSeverity } from "../types";

export const MATRIX_CLASSES: { id: ObjectClass; label: string }[] = [
  { id: "track", label: "Track" },
  { id: "smd_pad", label: "SMD pad" },
  { id: "th_pad", label: "TH pad" },
  { id: "via", label: "Via" },
  { id: "zone", label: "Zone" },
  { id: "hole", label: "Hole" },
  { id: "outline", label: "Board outline" },
  { id: "silk", label: "Silkscreen" },
];
const ORDER = MATRIX_CLASSES.map((c) => c.id);
const classLabel = (k: ObjectClass) => MATRIX_CLASSES.find((c) => c.id === k)?.label ?? k;
/** Cells are stored with `a <= b` in class order (as the engine stores them). */
export const ordered = (a: ObjectClass, b: ObjectClass): [ObjectClass, ObjectClass] => (ORDER.indexOf(a) <= ORDER.indexOf(b) ? [a, b] : [b, a]);
const COPPER: ObjectClass[] = ["track", "smd_pad", "th_pad", "via"];

/** The DRC rule that enforces a cell, or null when this build checks none (mirrors `drc::matrix_cell_rule`). */
export function matrixCellRule(x: ObjectClass, y: ObjectClass): string | null {
  const [a, b] = ordered(x, y);
  if (COPPER.includes(a) && COPPER.includes(b)) return "DRC-002";
  if (COPPER.includes(a) && b === "hole") return "DRC-018";
  if (a === "hole" && b === "hole") return "DRC-019";
  if (COPPER.includes(a) && b === "outline") return "DRC-005";
  if (a === "outline" && b === "silk") return "DRC-020";
  return null;
}

const toMm = (nm: number | undefined) => (nm === undefined ? "" : String(+(nm / 1e6).toFixed(4)));
const cellKey = (a: ObjectClass, b: ObjectClass) => ordered(a, b).join("|");

/** Commands for the matrix as edited (mm strings by `a|b`): only cells that changed. */
export function matrixCommands(cells: ClearanceCell[], edited: Record<string, string>): { commands: Command[] } | { error: string } {
  const stored = new Map(cells.map((c) => [cellKey(c.a, c.b), c.clearance]));
  const commands: Command[] = [];
  for (const [k, text] of Object.entries(edited)) {
    const [a, b] = k.split("|") as [ObjectClass, ObjectClass];
    const v = text.trim() === "" ? null : Number(text);
    if (v !== null && !(Number.isFinite(v) && v > 0)) return { error: `${classLabel(a)} / ${classLabel(b)}: a clearance must be a positive number (mm), or empty to clear it.` };
    const nm = v === null ? null : mmToNm(v);
    if ((stored.get(k) ?? null) !== nm) commands.push({ type: "SetClearanceCell", a, b, clearance: nm });
  }
  return { commands };
}

export type Choice = { enabled: boolean; severity: RuleSeverity | "" };
/** Commands for a rule table as edited: one `SetRuleConfig` per rule whose choice changed. */
export function ruleCommands(stored: Record<string, RuleConfig> | undefined, edited: Record<string, Choice>): Command[] {
  const out: Command[] = [];
  for (const [rule_id, ch] of Object.entries(edited)) {
    const was = stored?.[rule_id];
    const before: Choice = { enabled: was?.enabled ?? true, severity: was?.severity ?? "" };
    if (before.enabled === ch.enabled && before.severity === ch.severity) continue;
    const config: RuleConfig | null = ch.enabled && !ch.severity ? null : { enabled: ch.enabled, severity: ch.severity || null };
    out.push({ type: "SetRuleConfig", rule_id, config });
  }
  return out;
}

const asOne = (cmds: Command[], label: string): Command => (cmds.length === 1 ? cmds[0] : { type: "Batch", commands: cmds, label });

interface Common {
  constraints: Constraints | undefined;
  busy: boolean;
  run: (c: Command) => void;
  setError: (e: string) => void;
}

/** Object x object clearance matrix (lower triangle; empty = no cell). */
export function ClearanceMatrix({ constraints, busy, run, setError }: Common) {
  const cells = constraints?.clearance_matrix ?? [];
  const initial: Record<string, string> = Object.fromEntries(cells.map((c) => [cellKey(c.a, c.b), toMm(c.clearance)]));
  const [edited, setEdited] = useState<Record<string, string>>({});
  const value = (a: ObjectClass, b: ObjectClass) => edited[cellKey(a, b)] ?? initial[cellKey(a, b)] ?? "";
  const apply = () => {
    const r = matrixCommands(cells, edited);
    if ("error" in r) return setError(r.error);
    if (!r.commands.length) return setError("Nothing to apply: the clearance matrix is unchanged.");
    run(asOne(r.commands, "Clearance matrix"));
    setEdited({});
  };
  return (
    <section aria-label="Clearance matrix" id="rules-matrix">
      <h3>Clearance matrix</h3>
      <p className="muted hint">
        Minimum distance (mm) between two kinds of object; empty means no extra rule. DRC applies the largest rule that covers a pair. A value in a cell marked as not checked is
        reported by DRC (DRC-002 becomes UNKNOWN), never assumed met.
      </p>
      <table className="matrix">
        <thead>
          <tr>
            <th />
            {MATRIX_CLASSES.map((c) => (
              <th key={c.id} scope="col">
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {MATRIX_CLASSES.map((row, i) => (
            <tr key={row.id}>
              <th scope="row">{row.label}</th>
              {MATRIX_CLASSES.map((col, j) => {
                if (j > i) return <td key={col.id} aria-hidden="true" />;
                const rule = matrixCellRule(row.id, col.id);
                return (
                  <td key={col.id} className={rule ? undefined : "unchecked"}>
                    <input
                      aria-label={`${col.label} to ${row.label} clearance (mm)`}
                      inputMode="decimal"
                      title={rule ? `Checked by ${rule}` : "Not checked by this build: a value here makes DRC-002 UNKNOWN"}
                      value={value(col.id, row.id)}
                      onChange={(e) => setEdited({ ...edited, [cellKey(col.id, row.id)]: e.target.value })}
                    />
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <button type="button" className="primary" disabled={busy} onClick={apply}>
        Apply clearance matrix
      </button>
    </section>
  );
}

/** Net-to-net clearance rules. */
export function NetPairs({ constraints, busy, run, setError, nets }: Common & { nets: string[] }) {
  const pairs = constraints?.net_pair_rules ?? [];
  const [a, setA] = useState(nets[0] ?? "");
  const [b, setB] = useState(nets[1] ?? "");
  const [mm, setMm] = useState("");
  const apply = () => {
    const v = Number(mm);
    if (!a || !b || a === b) return setError("A net pair rule needs two different nets.");
    if (mm.trim() === "" || !(Number.isFinite(v) && v > 0)) return setError("The net pair clearance must be a positive number (mm).");
    run({ type: "SetNetPairClearance", net_a: a, net_b: b, clearance: mmToNm(v) });
  };
  return (
    <section aria-label="Net pair rules" id="rules-net-pairs">
      <h3>Net-to-net clearance</h3>
      {pairs.length > 0 && (
        <ul className="tree" aria-label="Net pair rules in force">
          {pairs.map((p) => (
            <li key={`${p.net_a}|${p.net_b}`}>
              {p.net_a} / {p.net_b}: {toMm(p.clearance)} mm{" "}
              <button type="button" disabled={busy} onClick={() => run({ type: "SetNetPairClearance", net_a: p.net_a, net_b: p.net_b, clearance: null })}>
                Remove {p.net_a} / {p.net_b}
              </button>
            </li>
          ))}
        </ul>
      )}
      {nets.length < 2 ? (
        <p className="empty-state">Net pair rules need at least two nets.</p>
      ) : (
        <>
          <div className="grid2">
            <label>
              First net
              <select aria-label="First net" value={a} onChange={(e) => setA(e.target.value)}>
                {nets.map((n) => (
                  <option key={n}>{n}</option>
                ))}
              </select>
            </label>
            <label>
              Second net
              <select aria-label="Second net" value={b} onChange={(e) => setB(e.target.value)}>
                {nets.map((n) => (
                  <option key={n}>{n}</option>
                ))}
              </select>
            </label>
            <label>
              Clearance (mm)
              <input aria-label="Net pair clearance (mm)" inputMode="decimal" value={mm} onChange={(e) => setMm(e.target.value)} />
            </label>
          </div>
          <button type="button" className="primary" disabled={busy} onClick={apply}>
            Apply net pair rule
          </button>
        </>
      )}
    </section>
  );
}

/**
 * ERC or DRC rule table: enable and severity override per rule. The rules listed are the ones
 * the last report of that check declares; without one the check has to run first.
 */
export function RuleTable({
  constraints,
  busy,
  run,
  setError,
  check,
  report,
  onRun,
}: Common & { check: "ERC" | "DRC"; report: Report | undefined; onRun?: () => void }) {
  const stored = constraints?.rules;
  const [edited, setEdited] = useState<Record<string, Choice>>({});
  const choice = (id: string): Choice => edited[id] ?? { enabled: stored?.[id]?.enabled ?? true, severity: stored?.[id]?.severity ?? "" };
  const rules = report ? (report.rules ?? (report.rule_results ?? []).map((r) => ({ id: r.rule_id, description: "", parameters: "", source: "" }))) : [];
  const apply = () => {
    const cmds = ruleCommands(stored, edited);
    if (!cmds.length) return setError(`Nothing to apply: the ${check} rule configuration is unchanged.`);
    run(asOne(cmds, `${check} rule configuration`));
    setEdited({});
  };
  return (
    <section aria-label={`${check} rules`} id={`rules-${check.toLowerCase()}`}>
      <h3>{check} rules</h3>
      <p className="muted hint">
        A disabled rule is reported as DISABLED, never as passing, and the gates that use it are NOT_RUN. A severity override changes how its findings are shown; the gates and the
        release gate still use the default severity.
      </p>
      {!report ? (
        <p className="empty-state">
          Run {check} to list its rules.{" "}
          {onRun && (
            <button type="button" disabled={busy} onClick={onRun}>
              Run {check}
            </button>
          )}
        </p>
      ) : (
        <>
          <table className="rule-table" aria-label={`${check} rule table`}>
            <thead>
              <tr>
                <th scope="col">Rule</th>
                <th scope="col">Description</th>
                <th scope="col">Last result</th>
                <th scope="col">Enabled</th>
                <th scope="col">Severity</th>
              </tr>
            </thead>
            <tbody>
              {rules.map((r) => {
                const ch = choice(r.id);
                const res = report.rule_results?.find((x) => x.rule_id === r.id);
                return (
                  <tr key={r.id}>
                    <td>
                      <code>{r.id}</code>
                    </td>
                    <td>{r.description}</td>
                    <td>{res?.state ?? "UNKNOWN"}</td>
                    <td>
                      <input type="checkbox" aria-label={`Enable ${r.id}`} checked={ch.enabled} onChange={(e) => setEdited({ ...edited, [r.id]: { ...ch, enabled: e.target.checked } })} />
                    </td>
                    <td>
                      <select
                        aria-label={`Severity of ${r.id}`}
                        disabled={!ch.enabled}
                        title={ch.enabled ? `Report severity of ${r.id}'s findings` : "A disabled rule has no findings"}
                        value={ch.severity}
                        onChange={(e) => setEdited({ ...edited, [r.id]: { ...ch, severity: e.target.value as Choice["severity"] } })}
                      >
                        <option value="">Default</option>
                        <option value="error">Error</option>
                        <option value="warning">Warning</option>
                        <option value="info">Info</option>
                      </select>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <button type="button" className="primary" disabled={busy} onClick={apply}>
            Apply {check} rule configuration
          </button>
        </>
      )}
    </section>
  );
}
