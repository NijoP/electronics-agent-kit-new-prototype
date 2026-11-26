# ADR-0052: Footprint verification report (FP-001..FP-010) and the `FootprintVerified` gate
- **Date:** 2026-11-26
- **Context:** Wave-2 slice W2-D (`eak-v2-remap/report.md` §3.5). `LibraryMaturity::FootprintVerified`
  existed, but nothing checked a footprint against its datasheet, and `libmeta::maturity_problems`
  accepted the level on any evidence entry that named a source and a reviewer. Verification belongs
  in `eak-validation` (AGENTS.md). The existing `Report` is bound to a project revision and input
  hash; a footprint check is about a library definition, which has no revision.
- **Options considered:**
  1. Reuse `report::Report` with a dummy project id and revision. Rejected: a report that claims a
     revision it never ran against breaks the currency rules (`Report::currency`).
  2. Put `LandPatternSpec` in `eak-library` and the rules in `eak-validation`. Not possible:
     `eak-library` depends on `eak-manufacturing`, which depends on `eak-validation`, so
     `eak-validation` cannot see a type defined in `eak-library` without a cycle.
  3. **Chosen:** a new report type `footprint::FootprintReport` in `eak-validation`, with the input
     types `LandPatternSpec` / `PadSpec` / `FabLimits` defined next to the rules and re-exported by
     `eak-library` (`eak_library::LandPatternSpec`), which is where they are stored (W2-A).
- **Decision:**
  - `footprint::verify(def, spec, limits, tolerance)` runs FP-001..FP-009 and returns one
    `RuleResult` per rule with the shared `CheckState`s; FP-010 (3D model against the body) is
    listed in `not_implemented` until W2-K; `limitations` lists what the geometry model ignores.
  - Without a land-pattern table, FP-002 (pad geometry), FP-003 (pitch) and FP-008 (pin 1) are
    `UNKNOWN` and never pass; FP-007 (body part) and FP-009 (height) are `UNKNOWN` without their
    datasheet values; FP-004 and FP-006 are `UNKNOWN` without fabrication limits. A spec value that
    is not `Extraction::HumanVerified` gives `REQUIRES_HUMAN_APPROVAL`, so an automatic draft never
    verifies by itself. Values in a unit other than `mm` are not used (`UNKNOWN`).
  - `footprint::footprint_maturity_problems(claimed, evidence, report)` refuses `FootprintVerified`
    and higher unless a report exists in which FP-001..FP-009 all certify (no fail, no unknown, no
    pending approval) **and** `libmeta::maturity_problems` accepts the evidence (a source and a
    named reviewer). `libmeta::maturity_problems` itself is unchanged.
- **Consequences:**
  - A new report type in the shared report contract; no EAK-IR, schema, `Command` or IPC change.
    `eak-library` gains a dependency on `eak-validation`.
  - Not yet: the Tauri IPC (`footprint_verify`, `land_pattern_save`, `footprint_confirm_verified`)
    and `VerificationReport.tsx` (follow-up slice). Callers that record maturity must use
    `footprint_maturity_problems`; nothing records `FootprintVerified` today.
  - Gap: the release gate (`gates.rs`) does not look at library maturity at all, and the IR has no
    maturity field on `ComponentDef`, so a design whose footprints were never verified can still
    pass the release gate. Wiring it in needs maturity (or a stored report) in the project, which is
    an IR change and out of scope here.
- **Source requirement:** product spec library maturity ("a level is only ever raised with
  evidence"); `report.md` §3.5 and the W2-D row.
