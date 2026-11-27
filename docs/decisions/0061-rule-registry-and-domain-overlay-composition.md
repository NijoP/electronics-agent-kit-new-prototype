# ADR-0061: Versioned engineering rule registry and domain-overlay composition
- **Date:** 2026-11-27
- **Status:** Accepted (execution brief §7.2 C, §8.2, roadmap M4.4; task eak-eng-rules). Builds on
  ADR-0046 (rule configuration), ADR-0021 (fail-closed gates) and ADR-0060 (domain classes as
  `domain.*` requirement fields).
- **Context:** Rules were known only by id (`RULE_FAMILIES`) and by the `RuleInfo` each check
  writes into its report. §8.2 asks every rule to carry a stable id and version, its domain or
  overlay scope, blocking behaviour, evaluator identity and source with its evidence state, and
  asks rules to compose across overlays "without silently discarding stricter safety
  constraints". The gate report is a shared contract, so the new gates are recorded here. No
  EAK-IR field changes: the composition reads the existing requirement record and rule
  configuration.

## Decision
- **Registry (`eak_validation::rules::registry`).** Built once from the checks' own rule tables
  (ERC, DRC, DFM, MECH, SRC run on an empty project), so it cannot drift from them. Each entry:
  `id`, `version` (1 unless listed in `RULE_VERSIONS`), `check`, `evaluator`
  (`eak_validation::<check>::run`), `description`, `source`, `source_state`, `gates` (read from
  the gates' own evidence), `required_by` (domain classes) and a 16-hex SHA-256 `fingerprint` of
  id, version, check, description and source. `REGISTRY_VERSION` = 1.
- **Source state** is `EAK_DEFINITION` (EAK's structural definition or a project-declared limit)
  or `CITED_NOT_VERIFIED` (a standard, formula, fabricator or practice is cited). There is no
  verified state: no table in this build has been compared by a person with the cited text.
- **Overlay table (`OVERLAY_RULES`).** One entry per `DOMAIN_CLASSES` id, in catalogue order
  (a test enforces it): the existing rules the class requires, and the concerns of the class that
  no evaluator covers. It names rules and concerns only; it carries no engineering values.
- **Composition (`compose`).** A class is active when its recorded value is `true`, or when any
  unresolved conflicting candidate is `true` (the stricter reading is kept until a person
  resolves it). A class recorded `false` with no `true` candidate adds nothing. The required rules
  of all active classes are unioned; composition only adds. A required rule that the project
  configuration disables, or overrides to anything but `error`, is a conflict: reported, never
  applied silently. Overriding to `error` is stricter and accepted.
- **Gates.** `gates::evaluate` adds two gates after the ADR-0046 configuration pass:
  - `domain_rules` (required): `FAIL` on a conflict; `UNKNOWN` when no archetype is recorded as
    applying, or a recorded class is not firm (provisional or in conflict); `PASS` otherwise.
  - `domain_concerns` (advisory): `UNKNOWN` while an active class has concerns no evaluator
    covers (or no class applies); they are listed one per reason.
  Because `domain_rules` is required, an unclassified board stays DRAFT in the release flow.
- **Surfaces.** Harness read tool `engineering_rules` (CLI `eak rules`) returns the registry and
  the composition of the current revision. No desktop change in this increment.

## Consequences and limitations
- Rule versions are kept by hand; the fingerprint makes an unbumped change visible, but no test
  pins fingerprints yet (they cannot be generated without a local build).
- The overlay table is EAK's own mapping, not a standard; review it with the DESK knowledge port
  (M4.5).
- EAK records no review of an uncovered concern, so `domain_concerns` cannot pass for any active
  class today; recording such reviews needs an IR decision (M4.6/M4.7).
- `SetRuleConfig` still accepts disabling a required rule; the gate reports it. Refusing it in
  the engine would make the rule configuration depend on the classification order.
