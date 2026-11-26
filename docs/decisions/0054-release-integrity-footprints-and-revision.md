# ADR-0054: Release integrity: unverified footprints block release, packaging is revision-bound
- **Date:** 2026-11-26
- **Context:** Captain requirement for the release: nothing may be labelled release-ready unless the
  gates pass, and a part whose footprint verification (ADR-0052) is not "verified" must block. The
  release gate (`eak_manufacturing::release_gates_with_report`) already blocked on missing, stale,
  failing, UNKNOWN or disabled ERC/DRC/MECH/DFM rules and on the engineering gates, but never looked
  at footprints. The fabricator bundle (`fabricator::prepare_with`) zipped the files that `compile`
  had just written, without checking them again against the project it was packaging.
- **Options considered:**
  1. Store a footprint verification result in the IR (new field, migration). Rejected for now: a
     stored result can go stale when the definition changes, and the land-pattern specs it needs live
     in `eak-library`, which `eak-manufacturing` cannot depend on (cycle, see ADR-0052).
  2. **Chosen:** recompute `eak_validation::footprint::verify` for every definition used on the
     schematic at each gate evaluation. No IR change, no migration, nothing to go stale.
- **Decision:**
  - `footprint_blockers(p)` adds one blocking reason per definition that does not verify (or is
    missing from the project library), naming its footprint, its reference designators and the first
    rule that does not certify. It is part of `release_gates`, so export, the read-only release
    check, the Problems panel and the fabricator bundle all see it.
  - Until a land-pattern table can reach the gate, FP-002/003/008 are UNKNOWN for every part, so
    **every package is DRAFT**. That is the fail-closed reading of "not verified": UNKNOWN never
    passes. The reference end-to-end tests now assert that the footprint blockers are the only
    remaining reasons.
  - `verify_package_for(dir, p)` checks a package on disk against the project: same project id,
    revision and EAK-IR hash, plus every per-file hash and syntax check of `verify_package`.
    `fabricator::prepare_with` refuses (returns an error, writes no bundle) when it finds a problem.
  - Every Gerber and drill file carries `EAK-IR sha256 <hash> revision <n>` (a `G04` comment, an
    Excellon `;` comment), and `verify_package_for` refuses a file whose stamp is not this design
    revision. BOM and CPL CSVs stay plain tables for fabricator import; the manifest binds them by
    their SHA-256 next to the same revision and EAK-IR hash.
  - `FootprintInputs` (land-pattern tables by definition id, fabrication limits) reach the gate
    through `release_gates_with_footprints` and `compile_with_footprints`; a part whose footprint
    matches its reviewed table no longer blocks, and one that no longer matches blocks again (tested).
- **Consequences:** No design reaches `INTERNALLY_VERIFIED_RELEASE_CANDIDATE` until its callers pass
  the land-pattern tables and fabrication limits on record (follow-up: the desktop and harness
  callers read them from `eak-library` once it stores them). The status is never worded "manufacturing-ready" (tested).
