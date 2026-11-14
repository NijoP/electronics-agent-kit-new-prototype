# Implementation log

Chronological log of work on the owner's 2026-11-14 EDA desktop directive, newest first. Each entry
names the phase (directive Part 14), what changed, the decisions taken, and where the evidence is.
Test outcomes go in `TEST_RESULTS.md`; feature status in `FEATURE_MATRIX.md`; the per-milestone record
stays in `docs/development/STATUS_LOG.md`.

## 2026-11-14 - Phase 0: repository audit and research

- **Scope:** documentation only; no code, IR or command changes.
- **EasyEDA research (directive Part 2):** listed all 104 public repositories of the EasyEDA
  organisation; shallow-cloned and inspected the 37 the directive names (all exist and are public).
  Deliverables in `docs/research/`: `EASYEDA_ECOSYSTEM_AUDIT.md` (per-repository findings with
  commits, paths, licences, classifications), `EASYEDA_FEATURE_MATRIX.md` (195 classified features),
  `EASYEDA_API_AND_FORMAT_ANALYSIS.md`, `EAK_COMPETITIVE_ARCHITECTURE.md`,
  `LICENCE_AND_ATTRIBUTION_AUDIT.md`. Nothing from EasyEDA was copied into EAK.
- **Repository audit (directive Part 3):** `docs/archive/eda-desktop-plan/EDA_DESKTOP_GAP_ANALYSIS.md` (state of
  `origin/main` at `afd9351`, in-flight work named as pending),
  `EDA_DESKTOP_TARGET_ARCHITECTURE.md`, `EDA_DESKTOP_IMPLEMENTATION_PLAN.md` (Phases 0-8 with files,
  contract changes, ownership and effort).
- **Tracking files started:** `FEATURE_MATRIX.md`, `TEST_RESULTS.md` (baseline copied from PR bodies,
  not re-run), this log.
- **Decisions proposed (each needs an ADR before code):**
  - documents are typed views onto the single EAK-IR; tab and layout state live outside EAK-IR;
  - Save = named checkpoint and "dirty" = revision since the last checkpoint;
  - EAK-IR v2 replaces `CopperLayer` with stable `LayerId`s, an ordered stack-up and explicit via spans,
    and rejects unrepresentable v1 files;
  - a `Batch` command makes each user gesture one revision and one undo step;
  - long work runs as cancellable jobs that apply results against their base revision;
  - simulation runs a user-installed simulator out of process; nothing copyleft is bundled.
- **After the snapshot:** PR #21 (workspace shell) and PR #22 (multilayer boards) merged at `6a3b358`;
  the gap analysis, plan and feature matrix carry a note that they are not yet reflected.
- **Findings to act on:** no Level 3 desktop tests exist; one intermittent runtime test; CI disabled.
