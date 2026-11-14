# ADR-0021: Fail-closed engineering gates and typed physical quantities

Status: accepted (2026-11-15), code-reviewed only (never compiled or run)

## Context
The integration directive (§3F, §3G) asks for independent checks whose unknown outcome never
counts as a pass, gate results with evidence and provenance that block release labels, and
unit-aware copper/power calculations that never substitute silent defaults. EAK's DRC/DFM
reports already list findings, `not_implemented` and `limitations`, but nothing combined them
into pass/fail/unknown gates, and `export_manufacturing` only looked at report error counts.
DESK (`desk/quality.py`) has a five-state hard-gate lifecycle; the earlier EAK prototype has a
typed physical-quantity crate.

## Decision
* `eak-core::units` (new public module, not an IR field): `PhysicalQuantity`/`Unit`/`Tolerance`
  reused from the prototype (`79f4e9d`) with attribution. A temperature *rise* is read with
  `temperature_difference_k` (no Celsius offset).
* `eak-engineering::power`: IPC-2221 trace width (source-range applicability flags,
  `verified_against_standard = false`), IEC 60028 copper resistance/voltage drop, track-only rail
  drop upper bound, return-path continuity over adjacent ground fill. Every conclusion-changing
  input is a required argument.
* `eak-validation::gates` (new shared report type `GateReport`): gates `no_shorts`,
  `required_pads_connected`, `net_connectivity`, `clearances`, `annular_ring`, `board_outline`,
  `stackup`, `copper_continuity`, `fabricator_capability`, `electrical_constraints` (required) and
  `return_path_continuity` (advisory). States `PASS`/`FAIL`/`UNKNOWN`/`NOT_RUN`; a stored report is
  stale unless `is_current_for` the project's id, revision and input hash. Each gate lists
  evidence: report, rule ids, finding ids, revision, input hash.
* `eak_manufacturing::compile` keeps a package `DRAFT` when any required gate is not `PASS`
  (fabricator capability stays covered by the supplied, revision-bound DFM report). This makes
  release stricter: zone islands (DRC-013), relief spokes (DRC-014), dangling tracks (DRC-012) and
  power nets without a declared current or voltage now block the release-candidate label.
* Harness: `run_gates` (validation, `GATES` evidence), `calc_copper_trace_width`,
  `calc_copper_voltage_drop`, `power_analysis` (read). The envelope and categories are unchanged.
* DESK's weighted quality score is not adopted: a score must never override a gate.

## Consequences
Boards with power nets lacking declared current/voltage can no longer reach
`INTERNALLY_VERIFIED_RELEASE_CANDIDATE`; the reference fixture declares both rails, so
`end_to_end.rs`/`multilayer.rs` are expected to keep passing (not run). Reference (ground) nets are
recognised by name until the IR has a net-role field (eak-multidomain). The fabricator gate is
`UNKNOWN` while the built-in profile is an unverified transcription.
