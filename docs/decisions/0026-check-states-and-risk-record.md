# ADR-0026: Explicit check states, stale-result rule, risk record

Status: accepted (code-reviewed only, never compiled by this change)

## Context
Reports had `errors`/`warnings` and a single `NO_ERRORS_IN_IMPLEMENTED_CHECKS` outcome. A rule that
could not run (no declared net current or voltage, no height limit, no 3D data) was listed as a
rule and looked like a pass (docs/qa/EVALUATION.md C1, C2).

## Decision
- `eak_validation::CheckState`: `PASS | FAIL | WARNING | UNKNOWN | NOT_APPLICABLE | REQUIRES_HUMAN_APPROVAL`.
  Default is `UNKNOWN`; only `PASS` and `NOT_APPLICABLE` certify. Worst state wins
  (`FAIL > UNKNOWN > REQUIRES_HUMAN_APPROVAL > WARNING > PASS`).
- `Report` gains `rule_results` (one state + reason per rule) and `overall`, both `#[serde(default)]`
  so old stored reports load as `UNKNOWN`. `outcome`, `errors`, `warnings` are unchanged.
- DRC-008/DRC-014 are `UNKNOWN` while any copper net lacks a declared current; DRC-002 is `UNKNOWN`
  while any copper net lacks a declared voltage; MECH-001 is `UNKNOWN` with no height limit or any part
  without 3D body data, MECH-002/003 with missing body data. A `FAIL` is never softened.
- `Report::currency(&Project)` / `state_for(&Project)`: a report for another project id, revision or
  canonical hash is `Stale` and certifies `UNKNOWN`. Release (`eak-manufacturing::compile`) already
  rejected stale reports and still blocks above DRAFT on unknown mechanical fit.
- `RiskRecord` (severity, likelihood, detectability, consequence, subsystem, evidence, mitigation,
  approval_required) is derived by `Report::risks()` for `FAIL`/`UNKNOWN` rules. The axes are never
  combined into one score.

## Consequences
New fields on the shared report type (hence this ADR); consumers keep working. Not done: the desktop
does not yet render `rule_results`; `REQUIRES_HUMAN_APPROVAL` has no producing rule yet; ERC/DFM/SRC
rules are `PASS`/`WARNING`/`FAIL` only. Never compiled or run.
