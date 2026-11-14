# ADR-0022: Existing-project rework flow in the harness

Status: accepted (2026-11-15), code-reviewed only (never compiled or run)

## Context
EAK must be able to continue an unfinished or imperfect project with surgical corrections instead
of rebuilding it (directive 3H; DESK `desk/rework`). AI must not self-certify, mutations go only
through `Engine::execute`, and nothing is called manufacturing-ready (AGENTS.md).

## Decision
* New harness tool category `rework` (shared contract: the envelope `category` gains a value),
  served by `crates/eak-harness/src/rework.rs`: `rework_inspect` (read-only), `rework_baseline`,
  `rework_audit`, `rework_protect`, `rework_plan`, `rework_apply`, `rework_report`. Same envelope,
  CLI (`eak rework ...`) and MCP.
* The baseline is immutable: a project copy plus per-object content hashes and domain
  fingerprints (schematic, placement, copper, board, rules, composite), bound to a confirmed
  project id; its fingerprint and the copy's SHA-256 are re-verified by every later tool.
* Objects are PROTECTED (explicitly protected, or locked footprints; add-only), in scope of a repair
  (the audit issue's objects and the copper of its nets), or ACCEPTED (everything else: must be
  unchanged). A plan whose step mutates a PROTECTED or out-of-scope object, or is not surgical
  (`AutoPlace`, unscoped `ClearRouting`/`AutoRoute`), is refused and nothing is stored.
* Approval is not a tool: `eak rework approve <PLAN_ID> <FINGERPRINT>` works only from an
  interactive terminal with a typed phrase and is bound to the plan fingerprint and base revision.
* `rework_apply` executes each repair as one `Engine::execute`, reads the revision back, re-hashes
  all PROTECTED/ACCEPTED objects and, on any violation, undoes that repair and stops; then runs
  ERC/DRC/MECH/DFM. `rework_report` labels readiness (`integrity_violation`,
  `changes_outside_rework`, `repairs_incomplete`, `passed_deterministic_checks`) and always lists
  `not_implemented` and `limitations`.
* Records are evidence in `<project>/rework/<session>/`, never design data.

## Options considered
Rebuilding from a spec (loses accepted work); letting agents mark objects modifiable (would let
an agent un-protect what a human locked). Rejected.

## Consequences
Readback is hash-based, so a reference to an object is allowed but a change is caught. Issue
classification is only as complete as eak-validation: unimplemented checks are not issues.

## Source
Master directive 3H; spec sections on verification and approvals. DESK MIT licence in
`third_party/licenses/DESK/`.
