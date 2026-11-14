# ADR-0020: Autonomous design flow driver in the harness

Status: accepted (2026-11-15), code-reviewed only (never compiled or run)

## Context
External agents (Pi, OpenCode, Claude Code) in the EAK terminal need a repeatable spec → plan →
apply → validate → iterate → report loop with evidence, not ad-hoc command sequences. AI must not
self-certify (AGENTS.md), and nothing may be called manufacturing-ready.

## Decision
* New harness tool category `flow` (shared contract: the tool envelope's `category` gains a value)
  served by `crates/eak-harness/src/design.rs`: `design_spec_set/get`, `design_plan`,
  `design_apply`, `design_validate`, `design_claim`, `design_iteration_log`, `design_report`, plus
  the discovery tool `design_spec_schema`. Same JSON envelope, CLI (`eak design ...`) and MCP.
* The spec is a typed, `deny_unknown_fields` structure (rails with V/I, conditions, interfaces,
  protection, acceptance criteria, assumptions). Criteria name a metric; only metrics in
  `design::METRICS` have deterministic evaluators, anything else is always `unverified`.
* `design_plan` decodes every step against the `Command` schemas and stores it; `design_apply`
  executes the whole plan as **one** `Engine::execute` (`Command::Batch`, or the single command)
  with `expected_revision` = the plan's base revision. No other write path to the design exists.
* `design_validate` reuses `run_check all` (stored evidence) and recomputes spec numbers with
  `eak-engineering` (IPC-2221 trace capacity; new `calc::buck_duty_cycle`,
  `calc::buck_inductor_ripple_a`). Findings are structured (`id`, `source`, `severity`,
  `measured`, `required`, `suggested_fix`).
* Labels are assigned by EAK only: `verified`, `passed_deterministic_checks`,
  `simulation_supported` (successful `run_simulation` logged at this revision), `datasheet_supported`
  (library rating marked verified), `assumed` (agent-supplied inputs), `unverified`.
* Human approval gate: `design_report` fingerprints (SHA-256) the evidence body. Approval is NOT a
  tool; `eak design approve <REV> <FP>` requires an interactive TTY and a typed phrase, refuses
  reports with errors, and appends `approvals.jsonl`. Only an approved, error-free report gets the
  label "human-approved release candidate"; the report always states it is not a
  manufacturing-ready claim.
* Flow records live in `<project>/design-flow/` (spec, plans, iterations, claims, reports,
  approvals). They are evidence about the design (like `harness-calls.jsonl` and `evidence/`),
  never design data.

## Consequences
* The TTY gate stops agents that run commands through pipes; it cannot stop an agent that drives a
  real PTY and types the phrase. A desktop approval UI bound to the user's session is future work.
* No model calls inside EAK: the external agent supplies the reasoning.
* Labels describe the evidence EAK holds; they are not engineering sign-off.
