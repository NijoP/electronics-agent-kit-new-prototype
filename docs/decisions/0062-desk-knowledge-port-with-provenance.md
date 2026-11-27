# ADR-0062: DESK knowledge port with provenance and licensing review
- **Date:** 2026-11-27
- **Status:** Accepted (execution brief §7.2, M4.5; DESK study
  `docs/research/DESK_TO_EAK_DOMAIN_WORKFLOW.md` §3.2-§3.4, §4.3, §10; task eak-eng-rules).
  Builds on ADR-0011 (DESK domain intelligence) and ADR-0061 (rule registry; numbered before this
  one, open in its own PR).
- **Context:** EAK carried DESK's taxonomy, 100 archetypes and 17 overlays
  (`desk_domains.json`), but not the 66-rule catalogue, the references the rules cite, or the 206
  component families (study §10). The brief asks for validated knowledge with source, provenance
  and licensing review, never inventing values, with unverified knowledge labelled.

## Decision
- **Data.** `scripts/gen-desk-domains.sh` (bash + jq, no Python) also writes
  `crates/eak-intelligence/data/desk_knowledge.json`: licence, DESK commit, rules (every DESK
  field: scope, severity, applicability, required inputs, evaluation method, parameters, sources,
  verification method, evidence state, enforcement, review flag, rationale), references
  (bibliographic only) and families (role, required parameters, datasheet-verification prompts,
  relevant archetypes). Generated at DESK `9e44e7ca7d64b4997b5cf1c8afcd7f355b34c79b`.
- **Types and loader.** `eak_intelligence::domain::knowledge` parses it into typed records and
  `validate` refuses:
  - a VERIFIED_AGAINST_SOURCE or LIVE_VERIFIED rule whose sources are not all verified (DESK
    policy; none are);
  - a dangling archetype, overlay, family or reference, or an archetype's `rule_ids` naming no rule;
  - a reference that stores copyrighted text, a duplicate id, a licence other than MIT or a
    missing commit.
  The check runs in the tests and in every `desk_knowledge` answer (`loader_problems`).
- **Claims.** Each rule is shown through `Maturity::ceiling`: no DESK rule may be presented as a
  requirement today. DESK parameters are carried as DESK's working values and **never applied**:
  EAK checks use the project's declared constraints.
- **Coverage.** `EAK_COVERAGE` links 18 DESK rules to the EAK rules that evaluate a related concern
  (for example ELEC-WIDTH-001 to DRC-008); scope and parameters may differ. Every other DESK
  rule is reported `NOT_IMPLEMENTED_IN_EAK`, never as passing.
- **Surface.** Harness read tool `desk_knowledge` (all records, or one `rule_id` / `family_id`).

## Licensing review
DESK is the owner's repository under MIT (`third_party/licenses/DESK/LICENSE`). The ported data
is the owner's own work. No standard text, standard table, datasheet or third-party code is
included. References are identifiers, titles and clause pointers only, and all 46 are marked
`copyrighted_text_stored: false` (the loader enforces this). DESK's third-party components
(backpass, Freerouting, Weft; DESK `NOTICE`) are not used.

## Consequences and limitations
- The coverage links are EAK's own judgement of "related concern", not equivalence.
- DESK's rule engine, composer and profile compiler are not ported (study §3.2 defects). EAK's
  composition is ADR-0061, and the profile is M4.6.
- Regenerating the data needs a DESK checkout; the test counts (66/46/206) pin this commit.
