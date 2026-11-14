# Progress

- Before 2026-11-15: engine, validation, manufacturing, editors, IDE shell (#49), select/drag/flip/grids (#48), release scripts (#45), Linux v0.1.0–v0.1.4 releases.
- 2026-11-15 (Phase 1 integration, branch `refactor/eak-product-realignment`): squash-merged providers, harness and terminal; select, shell and release-ci branches were already in `main` (net-empty merges). Terminal registered through the workspace registry; harness endpoint adapted to multi-project sessions; ADRs renumbered 0017–0019; stale docs archived in `docs/archive/`; README, STATUS, ROADMAP, docs set rewritten.
- Installer fix: `install.sh` falls back to the `EAK_<version>_amd64.*` asset names that v0.1.4 actually uses (code-reviewed only).
- Per-milestone history: `STATUS_LOG.md`.
- 2026-11-15 (Phase 2 wiring, `fm/eak-p2-wiring`, code-reviewed only): terminal sessions now get `EAK_HARNESS_SOCKET` / `EAK_HARNESS_TOKEN_FILE` (app base env) and `EAK_PROJECT_ID` (spaces on the active project folder are bound to its id; the Terminal workspace now calls `terminal_set_context`, whose `project_id` argument was silently dropped by Tauri's camelCase default). Checks/exports re-render the editor when they evaluated a newer revision than the one on screen. Removed the unused, unlisted `harness_endpoint` IPC registration.
