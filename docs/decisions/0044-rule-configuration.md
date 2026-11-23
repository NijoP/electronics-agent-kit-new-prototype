# ADR-0044: ERC severities, the pin-conflict matrix and per-rule configuration (AD-8)
- **Date:** 2026-11-23
- **Status:** Accepted in parts. Part A (severity ladder mapping, pin-conflict matrix) and the
  findings export are in effect. Part B (per-rule enable and severity override in the IR
  constraints) lands with its IR change once slice C4 (IR v4, PR #73) is on main, because C4
  owns the IR until then; this ADR is updated with the schema version it uses.
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

## Part B: per-rule configuration (lands after C4)
- **IR:** `Constraints.rules: BTreeMap<rule id, RuleConfig>` with `RuleConfig { enabled: bool,
  severity: Option<RuleSeverity> }` and `RuleSeverity` = `error | warning | info` (eak-core
  cannot use the validation crate's type). Empty by default and skipped when empty, so a
  project without overrides saves byte-identically. Schema version bump with a migration that
  adds nothing, and a save/open round-trip test.
- **Command:** one typed command sets or clears one rule's configuration; the engine refuses
  unknown rule ids (the ids every check declares) so a typo cannot silently disable nothing.
- **Reports:** a disabled rule runs no evaluation; its findings are dropped and its
  `RuleResult.state` is `DISABLED` (new `CheckState`), which never certifies and makes the
  report's `overall` at best `DISABLED`, never `PASS`. An overridden rule's findings carry the
  configured severity, and `RuleResult` records the override (`severity_override`) next to
  the rule's default, so the report states it explicitly.
- **Gates (risk: disabling real failures):** gates are computed from the checks run with the
  default severities, so a severity override can never turn a failing gate into a passing one;
  a gate any of whose rules is disabled is `NOT_RUN` (not `PASS`), with the disabled rule ids
  as its reason. The manufacturing release gate treats a report whose error-class rules are
  disabled or lowered as not passing.
- **Consequences:** old reports deserialise (new fields are `#[serde(default)]`). Desktop
  `types.ts` adds `DISABLED`. A per-cell override of the pin-conflict matrix (EasyEDA allows
  it) is not offered: an override applies to every finding of the rule.
- **Source:** Phase 1 plan §6.4 AD-8, slices B5 and C5; EasyEDA behaviour study §2.5, §3.3;
  Execution Instructions §9.1 (engineering honesty).
