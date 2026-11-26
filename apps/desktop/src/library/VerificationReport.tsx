// Verification report of one library part: footprint rules FP-001..FP-009
// (`eak_validation::footprint`) and pin-to-pad rules PP-001..PP-005
// (`eak_validation::pin_pad`), computed by the backend each time. A part is shown as
// "footprint unverified" unless every footprint rule passed; UNKNOWN and pending approval are
// never shown as a pass, and the checks not implemented are always listed.
import { useEffect, useState } from "react";
import { api } from "../api";
import type { CheckState, RuleResult } from "../types";

/** `eak_validation::footprint::FootprintReport`. */
export interface FootprintReport {
  check: string;
  definition_id: string;
  footprint: string;
  spec_source: unknown;
  tolerance_nm: number;
  rule_results: RuleResult[];
  overall: CheckState;
  not_implemented: string[];
  limitations: string[];
}

/** `eak_validation::pin_pad::PinPadReport`. */
export interface PinPadReport {
  check: string;
  definition_id: string;
  footprint: string;
  exposed_pads: string[];
  rule_results: RuleResult[];
  overall: CheckState;
  not_implemented: string[];
  limitations: string[];
}

/** `parts::ComponentSaved`. */
export interface ComponentSaved {
  id: string;
  scope: string;
  replaced: boolean;
  pin_pad: PinPadReport;
  footprint: FootprintReport;
}

export const FOOTPRINT_RULES = ["FP-001", "FP-002", "FP-003", "FP-004", "FP-005", "FP-006", "FP-007", "FP-008", "FP-009"];
export const PIN_PAD_RULES = ["PP-001", "PP-002", "PP-003", "PP-004", "PP-005"];

const certifies = (s: CheckState | undefined) => s === "PASS" || s === "NOT_APPLICABLE";

/** True only when every listed rule is present and passes (or does not apply). */
export function allCertify(results: RuleResult[], rules: string[]): boolean {
  return rules.every((id) => certifies(results.find((r) => r.rule_id === id)?.state));
}

const LABEL: Record<CheckState, string> = {
  PASS: "pass",
  FAIL: "fail",
  WARNING: "warning",
  UNKNOWN: "unknown",
  NOT_APPLICABLE: "not applicable",
  REQUIRES_HUMAN_APPROVAL: "needs a person",
  DISABLED: "disabled",
};

function Rules({ label, results }: { label: string; results: RuleResult[] }) {
  return (
    <table aria-label={label}>
      <thead>
        <tr>
          <th>Rule</th>
          <th>State</th>
          <th>Why</th>
        </tr>
      </thead>
      <tbody>
        {results.map((r) => (
          <tr key={r.rule_id} data-state={r.state}>
            <td>{r.rule_id}</td>
            <td>{LABEL[r.state] ?? r.state}</td>
            <td>{r.reason || "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Notes({ report }: { report: { not_implemented: string[]; limitations: string[] } }) {
  return (
    <ul className="muted">
      {report.not_implemented.map((n) => (
        <li key={n}>Not implemented: {n}</li>
      ))}
      {report.limitations.map((n) => (
        <li key={n}>Limitation: {n}</li>
      ))}
    </ul>
  );
}

export function FootprintStatus({ report }: { report: FootprintReport | null }) {
  const ok = report !== null && allCertify(report.rule_results, FOOTPRINT_RULES);
  return (
    <strong aria-label="Footprint status" data-verified={ok ? "true" : "false"}>
      {ok ? "Footprint checks FP-001..FP-009 passed" : "Footprint unverified"}
    </strong>
  );
}

export function VerificationReport({ id }: { id: string }) {
  const [fp, setFp] = useState<FootprintReport | null>(null);
  const [pp, setPp] = useState<PinPadReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setFp(null);
    setPp(null);
    setError(null);
    Promise.all([api.footprintVerify(id), api.pinPadVerify(id)])
      .then(([f, p]) => {
        if (live) {
          setFp(f);
          setPp(p);
        }
      })
      .catch((e: unknown) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [id]);

  const ppOk = pp !== null && allCertify(pp.rule_results, PIN_PAD_RULES);
  const ppText = pp === null ? "Pin-to-pad not checked" : ppOk ? "Pin-to-pad checks passed" : "Pin-to-pad unverified";
  return (
    <section aria-label={`Verification of ${id}`}>
      <h5>Verification</h5>
      <p>
        <FootprintStatus report={fp} />
        {" · "}
        <strong aria-label="Pin to pad status">{ppText}</strong>
      </p>
      {error && <p className="muted">Verification could not run: {error}</p>}
      {fp && (
        <>
          <Rules label="Footprint rules" results={fp.rule_results} />
          <Notes report={fp} />
        </>
      )}
      {pp && (
        <>
          {pp.exposed_pads.length > 0 && <p className="muted">Exposed pads: {pp.exposed_pads.join(", ")}</p>}
          <Rules label="Pin to pad rules" results={pp.rule_results} />
          <Notes report={pp} />
        </>
      )}
    </section>
  );
}
