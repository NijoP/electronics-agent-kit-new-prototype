# ADR-0045: ERC severities, the pin-conflict matrix, per-rule configuration (AD-8) and the clearance matrix
- **Date:** 2026-11-23
- **Status:** Accepted. Part A (severity ladder mapping, pin-conflict matrix) and the findings
  export, Part B (per-rule enable and severity override, schema v6) and Part C (clearance matrix
  and net-pair rules, schema v6) are in effect. Schema v5 is ADR-0044 (pad expansion, routing
  options); this record originally took number 0043 and then 0044, both claimed by slices that
  landed first.
- **Context:** Phase 1 slice B5 (plan gaps #19-#21) and C5 (#20, #33, #34). The plan's
  architecture decision AD-8 says rule severity overrides and enable flags live in the IR
  constraints and appear in reports, and a disabled rule is reported as `disabled`, never as
  passing (AGENTS.md: unknown or unimplemented checks are never passing). The brief asks to use
  the EasyEDA Pro behaviour study (`eak-p1-rea-easyeda/report.md` §2.5, observed in
  `sch.js@2049095`) for ERC severities and the pin-conflict matrix. `Report`, `RuleResult` and
  `Constraints` are shared contracts, so their change is recorded here.

## Part A: severity ladder and pin-conflict matrix (in effect)
- **EasyEDA's ladder** is Fatal, Error, Warning, Note, Ignore. EAK keeps its three report
  severities and maps the ladder onto them: Fatal and Error → `error`, Warning → `warning`,
  Note → `info`, Ignore → the rule is disabled (Part B). Fatal is not added as a fourth report
  severity: every consumer (gates, release gate, desktop, harness) already treats `error` as
  blocking, and a new variant would change all of them for no difference in what blocks.
- **Default rule severities are not lowered to EasyEDA's.** Where EasyEDA is milder than EAK
  (for example a floating component pin is a Warning there and ERC-001 is an error here; a
  duplicate designator is a Note there and ERC-006 is an error here) EAK keeps the stricter
  default, because the PCB-sync ERC gate (ADR-0036) and the release gate rely on them. A
  project can still lower a rule through Part B; such a change is shown in every report.
- **Pin-conflict matrix (ERC-014).** EasyEDA's 11 pin types map onto EAK's: IN → `Input`,
  OUT → `Output`, BI → `Bidirectional`, Passive → `Passive`, Open Collector → `OpenCollector`,
  Power → both `PowerIn` and `PowerOut`. EAK's `NoConnect` has no EasyEDA counterpart (ERC-002
  covers it). The cells that map, with EasyEDA's severities:

  | Pair | Severity | Where |
  |---|---|---|
  | OUT-OUT | error | ERC-004 (several drivers) |
  | Power-OUT | error | ERC-014 for `PowerIn`-`Output`; ERC-004 for `PowerOut`-`Output` |
  | OpenCollector-OUT | error | ERC-014 |
  | Power-OpenCollector | error | ERC-014 (`PowerIn` and `PowerOut`) |
  | BI-OUT | warning | ERC-014 |
  | Power-BI | warning | ERC-014 (`PowerIn` and `PowerOut`) |
  | every other pairing of these types | ignore | none |

  Changed from before: `Output`-`PowerIn` was a warning and is now an error; `Output`-
  `Bidirectional`, `PowerIn`-`Bidirectional` and `PowerIn`-`OpenCollector` are new. Cells for
  EasyEDA types EAK does not have (Open Emitter, Ground, HiZ, Terminator, Undefined) are listed
  in the ERC report's `not_implemented`, never assumed compatible.

## Part B: per-rule configuration (in effect, schema v6)
- **IR:** `Constraints.rules: BTreeMap<rule id, RuleConfig>` with `RuleConfig { enabled: bool
  (default true), severity: Option<RuleSeverity> }` and `RuleSeverity` = `error | warning |
  info` (eak-core cannot use the validation crate's type). Empty by default and not written
  when empty, so a project without configuration saves as before. Schema v6: the v5 -> v6
  migration only changes the version number, so a v5 build refuses a v6 file instead of
  silently running a disabled rule. Round-trip and migration tests in `migrate.rs`.
- **Command:** `SetRuleConfig { rule_id, config }` sets or clears one rule (enabled at the
  default severity is stored as no entry). The engine refuses a rule id no check declares:
  `eak_core::RULE_FAMILIES` (ERC-001..021, DRC-001..023, DFM-001..010, MECH-001..003,
  SRC-001..012), kept equal to the checks' rule tables by `tests/rule_config.rs`.
- **Reports (`Report::new`):** a disabled rule's findings are dropped and its
  `RuleResult.state` is `DISABLED` (new `CheckState`, ranked above `WARNING` and below
  `REQUIRES_HUMAN_APPROVAL`), reason "disabled in the project's rule configuration: not
  evaluated, not a pass"; it never certifies, a report with one is at best `DISABLED`, and it is
  a risk record like an unknown rule. An overridden rule's findings carry the configured
  severity; `RuleResult.severity_override` states it and `RuleResult.default_state` keeps the
  state at the default severity when it differs. `overall` uses the worse of the two, so an
  override changes what the findings say, never the report's verdict.
- **Gates (risk: disabling real failures):** `gates::evaluate` runs the checks with the rule
  configuration removed, so an override can never turn a failing gate into a passing one, and
  says so in the gate's reasons; a gate any of whose rules is disabled is `NOT_RUN` with the
  disabled ids as its first reason. The manufacturing release gate blocks on any `DISABLED`
  rule and on any rule whose `default_state` is `FAIL`.
- **Consequences:** old reports deserialise (new fields default). A per-cell override of the
  pin-conflict matrix (EasyEDA allows it) is not offered: an override applies to every finding
  of the rule.

## Part C: clearance matrix and net-pair rules (Phase 1 C5, in effect, schema v6)
- **IR:** `Constraints.clearance_matrix: Vec<ClearanceCell { a, b, clearance }>` over the
  object classes track, SMD pad, TH pad, via, zone, hole, board outline and silkscreen (one entry
  per unordered pair, `a <= b`), and `Constraints.net_pair_rules: Vec<NetPairRule { net_a,
  net_b, clearance }>` (`net_a < net_b`). Both empty by default and not written when empty;
  integrity refuses non-positive values, unordered or duplicate cells and a pair naming one net
  twice. Net renames rewrite the pairs (a pair a merge collapses is dropped; duplicates keep the
  larger value).
- **Commands:** `SetClearanceCell { a, b, clearance }` and `SetNetPairClearance { net_a, net_b,
  clearance }`; `null` clears.
- **DRC reads the matrix:** the largest applicable rule wins. Copper pairs (track, SMD pad, TH
  pad, via; different nets) raise DRC-002; copper to holes raise DRC-018; hole to hole raises
  DRC-019; copper to the outline raises DRC-005; silkscreen to the outline raises DRC-020. That
  is 20 of the 36 cells (`drc::matrix_cell_rule`), each enforced by a test in
  `tests/clearance_matrix.rs`. The other 16 (every zone cell: pours keep their own zone
  clearance; silkscreen to copper, holes and silk; hole to outline) are listed in every DRC
  report's `not_implemented`, and a configured one makes DRC-002 `UNKNOWN` with the cell named:
  unknown cells are reported, never assumed met. Net-pair rules raise DRC-002 and the
  interactive router's `clearance::required`; matrix cells are DRC-only (a stated limitation).
- **Region rules** (C4's constraint regions) keep their track-width limits (DRC-022); region
  clearance rules stay in `not_implemented`.
- **Source:** Phase 1 plan §6.4 AD-8, slices B5 and C5; EasyEDA behaviour study §2.5, §3.3;
  Execution Instructions §9.1 (engineering honesty).
