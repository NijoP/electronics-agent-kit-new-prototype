# ADR-0014: DFM check against a fabrication profile, and DFM as a release gate

**Date:** 2026-11-14 · **Status:** accepted · **Source requirement:** product spec §9 (verification
evidence, "unknown is never pass"); plan `docs/archive/eda-desktop-plan/EDA_DESKTOP_IMPLEMENTATION_PLAN.md`
Phase 4 (DFM report) and Phase 7 (manufacturing outputs).

## Decision
1. `eak-validation::dfm::run(project, profile)` produces a revision-bound `Report` with
   `check = "DFM"`, rules DFM-001..010, and the same `not_implemented` / `limitations` lists as
   ERC/DRC. The profile is data (`crates/eak-validation/data/fab-jlcpcb-standard.json`) with its
   public source URL, retrieval date and `verified: false`; every rule's `source` names it.
2. The manufacturing release gate (`eak_manufacturing::compile`) now requires ERC, DRC, MECH
   **and DFM** reports bound to the revision. A missing DFM report makes the package DRAFT.
3. The fabrication pack (`<stem>-fab-pack.zip`: Gerber + drill + `README.txt` of layers) and the
   README are listed in the manifest (`kind` `fab-pack` / `readme`); `verify_package` re-reads the
   ZIP and checks every entry's CRC.
4. The desktop IPC `Check` gains `dfm` (`run_check`), and `export_manufacturing` runs and returns
   all four reports. No `Command` variant and no EAK-IR field changed (no schema bump).

## Context
DRC enforces the project's own rules; nothing checked whether those rules and the board fit what
a fabricator can build. Fabricator limits differ per vendor and per order option, so they are data
rather than constants, and they are unverified transcriptions of a public page.

## Options considered
- Put fabricator values into `Constraints` (IR change, schema bump, ADR): rejected for now; a
  profile is a property of the order, not of the design.
- Keep DFM advisory (not a gate): rejected; a check that exists but does not gate the release
  invites "passed" claims it cannot back.

## Consequences
- Boards outside the built-in 1-2 layer profile (for example 4-layer boards) are DRAFT on export
  until a matching profile is supplied (DFM-008 reports this rather than passing).
- Keepouts are reported as `not_implemented` in DRC: the IR has no keepout objects; adding them
  needs a schema change and its own ADR.

## Addendum: unknown is not pass in the gate (evaluator findings C1, C2)
- The gate also blocks (DRAFT) when `max_component_height` is not declared, because MECH-001 then
  evaluates nothing, and when any MECH-003 (no 3D body data) finding exists.
- DRC lists in `limitations`, per net, the nets without a declared current (DRC-008/014 not
  evaluated) or voltage (IPC voltage clearance in DRC-002 not evaluated).
