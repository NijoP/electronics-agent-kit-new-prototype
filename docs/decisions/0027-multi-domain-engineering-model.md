# ADR-0027: Multi-domain engineering model, computed per revision

Status: accepted (2026-11-15), code-reviewed only (never compiled or run)

## Decision
Add `eak-intelligence::systems`: engineering domain registry, product → subsystem → board →
block → component → region profiles, typed requirements with a knowledge state, an explainable
constraint graph (hard/soft/derived/conditional, inheritance, conflicts, precedence,
provenance, impact sets), calculation records, cross-domain interaction rules, a risk model,
discipline mental models (data, tagged by EAK layer), typed per-domain engineering models
with six-outcome checks, and a 13-stage lifecycle state machine. The model is **computed** from
EAK-IR + the stored design-flow spec at the current revision; it is **not stored** and adds no
IR field, `Command` or report type. Exposed read-only through harness tools `intel_*`
(`eak-harness::intel`, which also owns the spec → requirement adapter) and the desktop IPC
command `intelligence_summary`.

## Context
Directive (multi-domain) §2–§12 and captain additions (phase state machine; EAK layer split;
Track B typed domain models). The design flow already owns the requirement spec (ADR-0020)
and intel-core owns units and fail-closed gates (ADR-0021); a second spec or unit model is
forbidden.

## Options considered
1. Store requirements, constraints and risks in EAK-IR (needs IR schema + commands + migration).
2. A separate persisted engineering database (a second authority; rejected by AGENTS.md).
3. Recompute a read-only model from IR + spec per call (chosen).

## Chosen option and consequences
Option 3: no second source of truth and no drift; nothing to migrate. Consequences: human
confirmations, waivers and interaction reviews cannot be recorded yet, so `Requirement::promote`
is enforced in code but only reachable once option 1 is done for those records (next step,
needs its own ADR); every lifecycle exit that depends on them stays Unknown. Unknown never
passes anywhere; release review never exits automatically; high-risk domains always need
human approval.

Reuse: units come from `eak_core::units`; check outcomes and risks reuse ADR-0026's
`CheckState` and `RiskRecord`. No second requirement, unit, outcome or risk model is added.

## Source requirement
Multi-domain directive §3–§12, §15 A–F; captain additions 2026-11-14 (inbox 001–003).

## Date
2026-11-15
