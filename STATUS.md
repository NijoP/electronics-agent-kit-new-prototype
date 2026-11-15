| Manual PCB routing workflow (pad start/finish, width, 45/90, via + layer switch, track edit/delete, active net, next-connection priority, DRC check) | code-reviewed only | 2026-11-15 `fm/eak-pcb-routing`; not compiled, tests written but unrun; not exercised in a real window |
# EAK status

Current, short record. Per-milestone history with exact test counts: `docs/development/STATUS_LOG.md`.
Evidence levels and the testing freeze: `docs/development/TEST_EVIDENCE.md`.

**Verification labels** — *runtime-verified* (run on a real target, evidence kept) ·
*build/test-verified* (built and automated tests passed when last run) · *code-reviewed only*
(written and read, never compiled or run). Nothing is professionally validated. Nothing is
manufacturing-ready.

## Product direction (2026-11-15)
EAK is an engineering workstation: an EDA workspace, a Terminal workspace for external coding
agents, and an engineering harness, over one EAK-IR and independent validation. The chat-style
assistant panel was removed in the IDE shell (#49); model providers remain as optional infrastructure.

## Capability labels
| Capability | Label | Note |
|---|---|---|
| EAK-IR, command engine, journal, undo, recovery | build/test-verified | engine contract tests (before the freeze) |
| ERC / DRC / MECH, sourcing checks | build/test-verified | reports list `not_implemented` and `limitations` |
| Placement intelligence (roles, snaps, `analyze_placement`) | code-reviewed only, unbuilt | DESK-derived heuristics; no thermal, RF, switching-node or escape analysis |
| Router intelligence (passes, rip-up, congestion, pairs) and existing-project rework | code-reviewed only | never compiled or run; see ADR-0022/0023 |
| Engineering calculators, router, placer | build/test-verified | IPC tables not checked against the standard text |
| Gerber / drill / BOM / CPL, JLCPCB preparation | build/test-verified | DRAFT output; no external viewer or fabricator check |
| Schematic and PCB editors (select, drag, flip, grids, align) | build/test-verified (jsdom + engine) | not exercised in a real window |
| IDE shell: workspaces, explorer, multi-project, Project Home | build/test-verified at #49 | merged before the freeze; real-window run not done |
| UI redesign: design system (tokens, primitives, `docs/design-system.md`), Project Home launchpad (rebuilt 2026-11-15), project Overview panel, editor/terminal chrome on tokens | code-reviewed only | never built, run or looked at in a window; `tokens.test.ts`, `projectHome.test.ts`, `projectOverview.test.ts` written, unrun |
| Install scripts and v0.1.4 Linux release | code-reviewed only (scripts); package published | no install of the published package has been verified on real hardware (`docs/PLATFORM_SUPPORT.md`); Windows/macOS: no published binaries |
| Terminal engine (`eak-terminal`) | runtime-verified (real-PTY tests passed before the override, Linux) | later edits and the derived agent table test: not run |
| Terminal workspace UI, registry wiring | code-reviewed only | |
| Engineering harness (`eak` CLI, MCP, app endpoint) | code-reviewed only | never compiled; e2e tests written, never run |
| Provider catalog, streaming, Gemini, staged connection test | code-reviewed only | |
| Integration of all of the above (this commit) | code-reviewed only | conflicts resolved by reading; `Cargo.lock` hand-merged |
| Phase 2 wiring: harness env in terminal sessions, space-project binding, editor resync after checks/exports | code-reviewed only | `git diff --check` and grep only; never compiled or run |
| Engineering gates (PASS/FAIL/UNKNOWN/NOT_RUN), copper/power engineering, physical quantities, harness `run_gates` | code-reviewed only | ADR-0021; never compiled or run |
| Autonomous design flow (`eak design`, `design_*` tools, evidence labels, TTY-only human approval; ADR-0020) | code-reviewed only | DESK mental models / 12-step procedure / derating (`eak-intelligence::mental`), domain gates blocking apply, intent read-back, component evidence; never compiled or run; unit tests in `design.rs` and `calc.rs` written, unrun; worked example in `docs/agents/AUTONOMOUS_DESIGN.md` not run |
| Performance: resource profile (AUTO/LOW/BALANCED/HIGH/CUSTOM), bounded blocking jobs (P0–P5), lock-free analysis on a per-revision snapshot with stale-evidence rejection, per-frame pointer coalescing, 3D context-loss handling and quality tier, profile-sized terminal scrollback | code-reviewed only | never compiled or run; nothing measured; `resources.rs` and `canvasFrame.test.tsx` tests written, unrun; ledger `docs/development/PERFORMANCE_LEDGER.md` |

## Known gaps
- No cargo, npm, vitest, tsc or tauri run has happened on the integrated tree.
- Harness env is exported to terminal sessions (code-reviewed only, never run).
- Autonomous design loop exists as harness tools only (code-reviewed only, never run); no per-call approval UI for agent edits; no Windows named pipes.
- No agent (Claude Code, OpenCode, Pi) has been tested end to end through the harness.
- 3D board viewer (three.js, registered EDA view, read-only: extruded outline, copper, pads, tracks, vias, courtyard outlines, bodies from library body height, orbit/presets/layer toggles): written, **code-reviewed only** (never built or run). Differential pairs, impedance control, requirements capture: not implemented.
- Component 3D models (ADR-0025): typed `ModelRef` on `Model3d`, STEP/WRL/GLB lazy loading with a file-keyed cache, one ECAD-to-viewer transform, quality states (VENDOR_EXACT / LIBRARY_VERIFIED / DERIVED_VALIDATED, declared not verified) with visible MISSING, library attach/inspect UI, click selection: written, **code-reviewed only** (never built or run). Not done: model-vs-footprint verification, collision checks, LCSC/EasyEDA model fetch, silkscreen/holes in 3D.
- `FEATURE_MATRIX.md` rows for older features need a re-audit.
- Fail-closed engineering gates, unit-aware copper/power calculations, typed physical quantities (ADR-0021): **code-reviewed only**. IPC tables remain unverified against the standard text; reference nets are recognised by name.
- Knowledge retrieval (`eak-intelligence::knowledge`, harness `knowledge_search/cite/import`, ADR-0024): **code-reviewed only**, never compiled; BM25 over text only (no PDF/table extraction, no MPN/LCSC exact lookup, no rerank, no embeddings); results are advisory and never a validation result.
- Multi-domain engineering model (ADR-0028, `eak-intelligence::systems`, harness `intel_*`, desktop `intelligence_summary`): domain registry, product/board/block profiles, typed requirements with knowledge state, constraint graph (conflicts, precedence, impact), calculation records, interactions, risks, discipline models, typed domain models with six-outcome checks, 13-stage lifecycle: **code-reviewed only** (never compiled or run). Computed per revision, not stored; human confirmations/waivers cannot be recorded yet; multi-board and PCB regions not implemented.
- Component identity vs supplier offers (`eak-core::library::SupplierOffer`, ADR-0028) and resilient LCSC search/import in the desktop (`parts.rs`, `Resilient` cache/retry/stale label): **code-reviewed only**, never compiled; a supplier listing is never suitability evidence; lifecycle not parsed; cache per process.
