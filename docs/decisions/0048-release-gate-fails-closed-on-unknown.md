# ADR-0048: The release gate fails closed on UNKNOWN rule results
- **Date:** 2026-11-23
- **Context:** Phase 1 definition-of-done audit, defect S1 (fix slice F1). A DRC report can have
  0 errors while some of its rules are `UNKNOWN`. DRC-002 (voltage-based clearance) is degraded to
  UNKNOWN for nets with no declared voltage, or for a configured clearance-matrix cell no rule
  enforces. DRC-008 and DRC-014 (current capacity) are degraded for nets with no declared current,
  and MECH-001/002/003 for missing height data. `eak_manufacturing::release_gates_with_report`
  blocked only on disabled rules and default-severity failures. The `clearances` engineering gate
  appended the "undeclared nets" lines to its reasons but stayed PASS. The desktop Release check
  showed a saved report as PASS when it had no errors and no blocking line started with
  `"{check} "`, so the per-rule lines (`"DRC: …"`) never reached their row. For a signal net (power
  nets are caught by `electrical_constraints`) nothing blocked: the release check called UNKNOWN a
  PASS, which breaks the AGENTS.md rule "unknown checks are never reported as passing".
- **Options considered:**
  1. Block only on DRC-002. Rejected: the same fail-open applies to every degraded rule (DRC-008,
     DRC-014 and the MECH rules), and the next degraded rule would reopen it.
  2. Use the report's `overall` state. Rejected: `overall` also folds in `Warning` and
     `RequiresHumanApproval`, and the release would then block on warnings. The blocking reason must
     name the rule.
  3. **Chosen:** every `rule_results` entry whose state (or default-severity state) is `Unknown`
     is a blocking reason, and the gate and the release view show it as UNKNOWN.
- **Decision:**
  - `release_gates_with_report` adds `"{check}: {rule} is UNKNOWN: {reason}"` for each `Unknown`
    rule of each supplied ERC/DRC/MECH/DFM report. The reason is the rule's own, or "not evaluated".
    The package is therefore DRAFT. MECH unknowns already blocked through the height-limit and
    MECH-003 lines; they now also block per rule.
  - `gates::evaluate`: the `clearances` gate is UNKNOWN (unless already FAIL) when DRC-002 is
    UNKNOWN. Its reason is `"DRC-002 is UNKNOWN: …"`.
  - Desktop `release_check_of`: a saved report with any `Unknown` rule is shown UNKNOWN, never PASS.
    A row's reasons include the `"{check}: "` per-rule lines, so disabled rules and
    default-severity failures now also show on their row instead of under a PASS row.
  - A board can still reach `INTERNALLY_VERIFIED_RELEASE_CANDIDATE`, but only when every net on it
    has a declared voltage and current. `eak_runtime::fixtures::declare_undeclared_nets` declares
    fixture ratings (GND 0 V / 0.3 A, other nets 3.3 V / 0.02 A) through `SetNetVoltage` and
    `SetNetCurrent`, so the reference-design release tests still release. The sample project and the
    `build_pcb` fixture are unchanged: they stay DRAFT, which is the honest state.
- **Consequences:** no IR, `Command` or schema change, and no migration. The report and gate
  semantics are stricter: a project that released before with undeclared signal-net voltages or
  currents is now DRAFT until the user declares them (`SetNetVoltage` / `SetNetCurrent`). Tests: `src-tauri/tests/ipc.rs`
  `release_check_shows_a_report_with_an_unknown_rule_as_unknown_and_blocks` (DRC 0 errors, DRC-002
  UNKNOWN → row UNKNOWN, DRAFT, blocking reason naming DRC-002); `eak-validation` `gates.rs`
  `undeclared_voltage_on_a_signal_net_leaves_clearances_unknown`; ui-evidence `e7-release.json`
  steps `f1-*`.
- **Source requirement:** Product spec §10 (explicit check states; unknown is never a pass) and the
  AGENTS.md non-negotiable on unknown checks; audit defect S1.
